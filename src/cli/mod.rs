mod error;
mod grammar;
mod output;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::Value;

use crate::agent::claude::server as session_store;
use error::{Error, Result, other, parameter, target};
use output::{OutputFormat, Response};

#[derive(Parser)]
#[command(
    name = "cagent",
    version,
    about = "Local session manager for Claude/Codex with CLI and Telegram interfaces"
)]
struct Cli {
    /// Output format
    #[arg(long, global = true, help_heading = "Global Options", value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
    /// Print debug logs to stderr
    #[arg(long, global = true, help_heading = "Global Options")]
    debug: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the supervisor that spawns session workers and answers Telegram
    #[command(subcommand)]
    Server(ServerCommand),
    /// Create, list, attach to, prompt, and kill Claude/Codex sessions
    #[command(subcommand)]
    Session(SessionCommand),
    /// Store, list, and delete cron job definitions
    #[command(subcommand, name = "cron-job")]
    CronJob(CronJobCommand),
}

#[derive(Subcommand)]
enum ServerCommand {
    /// Run the server in the foreground (REST endpoint and Telegram bot) until interrupted
    #[command(name = "start-server")]
    Start,
}

#[derive(Subcommand)]
enum SessionCommand {
    /// Ask the running server to spawn a session worker and print its session ID
    #[command(name = "create-session")]
    Create(CreateSessionArgs),
    /// Run a session worker in the foreground (the server spawns this; it rarely needs to be run by hand)
    #[command(name = "run-session")]
    Run(RunSessionArgs),
    /// List the live sessions, removing the directories of dead ones
    #[command(name = "list-sessions")]
    List,
    /// Kill every session and remove all session directories
    #[command(name = "prune-sessions")]
    Prune,
    /// Kill one session
    #[command(name = "kill-session")]
    Kill(SessionIdArgs),
    /// Stream the session's messages as JSON lines and forward standard input to the session
    #[command(name = "attach-session")]
    Attach(SessionIdArgs),
    /// Send one prompt to the session
    #[command(name = "send-prompt")]
    SendPrompt(SendPromptArgs),
}

#[derive(Subcommand)]
enum CronJobCommand {
    /// Store a cron job definition
    #[command(name = "create-cron-job")]
    Create(CreateCronJobArgs),
    /// List the stored cron job definitions
    #[command(name = "list-cron-jobs")]
    List,
    /// Delete a stored cron job definition
    #[command(name = "delete-cron-job")]
    Delete(JobIdArgs),
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
enum Agent {
    Claude,
    Codex,
}

impl Agent {
    fn default_command(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
}

#[derive(Args)]
struct AgentArgs {
    /// Agent that runs in the session
    #[arg(long, value_enum)]
    agent: Agent,
    /// Command that starts the agent (defaults to `claude` or `codex`)
    #[arg(long)]
    agent_command: Option<String>,
    /// CLAUDE_CONFIG_DIR of the Claude session (claude only)
    #[arg(long)]
    claude_config_dir: Option<String>,
    /// Prompt sent when the session starts
    #[arg(long)]
    initial_prompt: Option<String>,
}

impl AgentArgs {
    fn agent_command(&self) -> &str {
        self.agent_command
            .as_deref()
            .unwrap_or_else(|| self.agent.default_command())
    }

    fn validate(&self) -> Result<()> {
        if self.agent == Agent::Codex && self.claude_config_dir.is_some() {
            return Err(parameter(
                "--claude-config-dir applies only to --agent claude",
            ));
        }
        Ok(())
    }
}

#[derive(Args)]
struct CreateSessionArgs {
    #[command(flatten)]
    agent: AgentArgs,
    /// Session ID to use (a random 8-digit hex ID when omitted)
    #[arg(long, allow_hyphen_values = true)]
    session_id: Option<String>,
}

#[derive(Args)]
struct RunSessionArgs {
    #[command(flatten)]
    agent: AgentArgs,
    /// Session ID whose directory and FIFOs the worker uses
    #[arg(long, allow_hyphen_values = true)]
    session_id: String,
}

#[derive(Args)]
struct SessionIdArgs {
    /// Session ID
    #[arg(long, allow_hyphen_values = true)]
    session_id: String,
}

#[derive(Args)]
struct SendPromptArgs {
    /// Session ID
    #[arg(long, allow_hyphen_values = true)]
    session_id: String,
    /// Prompt text (file:///dev/stdin reads standard input)
    #[arg(long, allow_hyphen_values = true)]
    prompt: String,
}

#[derive(Args)]
struct CreateCronJobArgs {
    /// Cron expression of the schedule
    #[arg(long)]
    cron_expression: String,
    /// Prompt to send on schedule
    #[arg(long, allow_hyphen_values = true)]
    prompt: String,
}

#[derive(Args)]
struct JobIdArgs {
    /// Cron job ID
    #[arg(long)]
    job_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CreateSessionOutput {
    session_id: String,
    agent: Agent,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SessionsOutput<T> {
    sessions: Vec<T>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct KillSessionOutput {
    session_id: String,
    pid: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SendPromptOutput {
    session_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CronJob {
    job_id: String,
    cron_expression: String,
    prompt: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CronJobOutput {
    cron_job: CronJob,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CronJobsOutput {
    cron_jobs: Vec<CronJob>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct DeleteCronJobOutput {
    job_id: String,
}

fn to_value<T: Serialize>(value: &T) -> Result<Value> {
    serde_json::to_value(value).map_err(|error| other(error.to_string()))
}

fn failed(error: anyhow::Error) -> Error {
    other(format!("{error:#}"))
}

fn require_session(session_id: &str) -> Result<()> {
    if session_store::session_exists(session_id) {
        Ok(())
    } else {
        Err(target(format!("session not found: {session_id}")))
    }
}

fn require_server() -> Result<()> {
    match crate::server::running_server_pid() {
        Some(_) => Ok(()),
        None => Err(target(
            "server is not running. start it with `cagent server start-server`",
        )),
    }
}

pub fn debug_requested() -> bool {
    std::env::args().any(|arg| arg == "--debug")
}

pub async fn main() -> i32 {
    match run().await {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: {error}");
            error.exit_code()
        }
    }
}

async fn run() -> Result<()> {
    let mut root = Cli::command();
    root.build();
    let argv = grammar::prepare(&root, std::env::args_os().collect())?;
    if argv.iter().any(|token| token == "--debug") {
        eprintln!("debug: argv {:?}", &argv[1..]);
    }

    let cli = match Cli::try_parse_from(&argv) {
        Ok(cli) => cli,
        Err(error) => {
            let code = if error.use_stderr() { 252 } else { 0 };
            let _ = error.print();
            std::process::exit(code);
        }
    };

    let (name, value) = match cli.command {
        Command::Server(ServerCommand::Start) => {
            crate::server::run_server().await.map_err(failed)?;
            return Ok(());
        }
        Command::Session(command) => match command {
            SessionCommand::Create(args) => {
                args.agent.validate()?;
                require_server()?;
                let agent = &args.agent;
                let session_id = match (agent.agent, args.session_id.as_deref()) {
                    (Agent::Claude, Some(session_id)) => {
                        crate::agent::claude::run::launch_session_with_id(
                            session_id,
                            agent.agent_command(),
                            agent.claude_config_dir.as_deref(),
                            agent.initial_prompt.as_deref(),
                        )
                    }
                    (Agent::Claude, None) => crate::agent::claude::run::launch_session(
                        agent.agent_command(),
                        agent.claude_config_dir.as_deref(),
                        agent.initial_prompt.as_deref(),
                    ),
                    (Agent::Codex, Some(session_id)) => {
                        crate::agent::codex::run::launch_session_with_id(
                            session_id,
                            agent.agent_command(),
                            agent.initial_prompt.as_deref(),
                        )
                    }
                    (Agent::Codex, None) => crate::agent::codex::run::launch_session(
                        agent.agent_command(),
                        agent.initial_prompt.as_deref(),
                    ),
                }
                .map_err(failed)?;
                (
                    "create-session",
                    to_value(&CreateSessionOutput {
                        session_id,
                        agent: agent.agent,
                    })?,
                )
            }
            SessionCommand::Run(args) => {
                args.agent.validate()?;
                let agent = &args.agent;
                match agent.agent {
                    Agent::Claude => {
                        crate::agent::claude::run::run_server(
                            &args.session_id,
                            agent.agent_command(),
                            agent.claude_config_dir.as_deref(),
                            agent.initial_prompt.as_deref(),
                        )
                        .await
                    }
                    Agent::Codex => {
                        crate::agent::codex::run::run_server(
                            &args.session_id,
                            agent.agent_command(),
                            agent.initial_prompt.as_deref(),
                        )
                        .await
                    }
                }
                .map_err(failed)?;
                return Ok(());
            }
            SessionCommand::List => (
                "list-sessions",
                to_value(&SessionsOutput {
                    sessions: session_store::list_sessions().map_err(failed)?,
                })?,
            ),
            SessionCommand::Prune => (
                "prune-sessions",
                to_value(&SessionsOutput {
                    sessions: session_store::prune_sessions().map_err(failed)?,
                })?,
            ),
            SessionCommand::Kill(args) => {
                if !session_store::session_dir(&args.session_id)
                    .join("meta.json")
                    .exists()
                {
                    return Err(target(format!("session not found: {}", args.session_id)));
                }
                let pid = session_store::kill_session(&args.session_id).map_err(failed)?;
                (
                    "kill-session",
                    to_value(&KillSessionOutput {
                        session_id: args.session_id,
                        pid,
                    })?,
                )
            }
            SessionCommand::Attach(args) => {
                require_session(&args.session_id)?;
                crate::agent::claude::attach::run(&args.session_id)
                    .await
                    .map_err(failed)?;
                return Ok(());
            }
            SessionCommand::SendPrompt(args) => {
                require_session(&args.session_id)?;
                crate::agent::claude::send::run(&args.session_id, &args.prompt).map_err(failed)?;
                (
                    "send-prompt",
                    to_value(&SendPromptOutput {
                        session_id: args.session_id,
                    })?,
                )
            }
        },
        Command::CronJob(command) => match command {
            CronJobCommand::Create(args) => {
                let job_id = crate::cron::storage::add(&args.cron_expression, &args.prompt)
                    .map_err(failed)?;
                (
                    "create-cron-job",
                    to_value(&CronJobOutput {
                        cron_job: CronJob {
                            job_id,
                            cron_expression: args.cron_expression,
                            prompt: args.prompt,
                        },
                    })?,
                )
            }
            CronJobCommand::List => {
                let cron_jobs = crate::cron::storage::list()
                    .map_err(failed)?
                    .into_iter()
                    .map(|job| CronJob {
                        job_id: job.id,
                        cron_expression: job.cron,
                        prompt: job.prompt,
                    })
                    .collect();
                ("list-cron-jobs", to_value(&CronJobsOutput { cron_jobs })?)
            }
            CronJobCommand::Delete(args) => {
                let exists = crate::cron::storage::list()
                    .map_err(failed)?
                    .iter()
                    .any(|job| job.id == args.job_id);
                if !exists {
                    return Err(target(format!("cron job not found: {}", args.job_id)));
                }
                crate::cron::storage::remove(&args.job_id).map_err(failed)?;
                (
                    "delete-cron-job",
                    to_value(&DeleteCronJobOutput {
                        job_id: args.job_id,
                    })?,
                )
            }
        },
    };
    print!(
        "{}",
        output::render(
            &grammar::pascal_case(name),
            Response { value, text: None },
            cli.output
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("parse")
    }

    #[test]
    fn parser_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_start_server() {
        let cli = parse(&["cagent", "server", "start-server"]);
        assert!(matches!(cli.command, Command::Server(ServerCommand::Start)));
    }

    #[test]
    fn parses_send_prompt() {
        let cli = parse(&[
            "cagent",
            "session",
            "send-prompt",
            "--session-id",
            "-100123:0",
            "--prompt",
            "hello",
        ]);
        match cli.command {
            Command::Session(SessionCommand::SendPrompt(args)) => {
                assert_eq!(args.session_id, "-100123:0");
                assert_eq!(args.prompt, "hello");
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_run_session_as_spawned_by_the_server() {
        let cli = parse(&[
            "cagent",
            "session",
            "run-session",
            "--agent",
            "codex",
            "--session-id",
            "1234abcd",
            "--agent-command",
            "codex",
            "--initial-prompt",
            "hi",
        ]);
        match cli.command {
            Command::Session(SessionCommand::Run(args)) => {
                assert!(args.agent.agent == Agent::Codex);
                assert_eq!(args.session_id, "1234abcd");
                assert_eq!(args.agent.agent_command(), "codex");
                assert_eq!(args.agent.initial_prompt.as_deref(), Some("hi"));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn agent_command_defaults_to_the_agent_name() {
        let cli = parse(&["cagent", "session", "create-session", "--agent", "claude"]);
        match cli.command {
            Command::Session(SessionCommand::Create(args)) => {
                assert_eq!(args.agent.agent_command(), "claude");
                assert!(args.session_id.is_none());
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn codex_rejects_claude_config_dir() {
        let cli = parse(&[
            "cagent",
            "session",
            "create-session",
            "--agent",
            "codex",
            "--claude-config-dir",
            "/tmp/x",
        ]);
        match cli.command {
            Command::Session(SessionCommand::Create(args)) => {
                assert_eq!(args.agent.validate().unwrap_err().exit_code(), 252);
            }
            _ => panic!("unexpected command"),
        }
    }
}
