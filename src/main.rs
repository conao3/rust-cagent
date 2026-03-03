mod agent;
mod cron;
mod telegram;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "cagent - a CLI agent tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    Server,
    Telegram {
        #[command(subcommand)]
        command: TelegramCommands,
    },
    Cron {
        #[command(subcommand)]
        command: CronCommands,
    },
    #[command(hide = true)]
    ClaudeServer {
        #[arg(long, default_value = "claude")]
        claude_command: String,
        #[arg(long)]
        claude_config_dir: Option<String>,
        #[arg(long)]
        initial_prompt: Option<String>,
        session_id: String,
    },
    #[command(hide = true)]
    CodexServer {
        #[arg(long, default_value = "codex")]
        codex_command: String,
        #[arg(long)]
        initial_prompt: Option<String>,
        session_id: String,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    Claude,
    Codex,
    List,
    Prune,
    Kill { session_id: String },
    Subscribe { session_id: String },
    Attach { session_id: String },
    Send { session_id: String, prompt: String },
}

#[derive(Subcommand)]
enum TelegramCommands {
    Start,
}

#[derive(Subcommand)]
enum CronCommands {
    Add {
        #[arg(long)]
        cron: String,
        #[arg(long)]
        prompt: String,
    },
    List,
    Rm {
        job_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Agent { command } => match command {
            AgentCommands::Claude => agent::claude::run::launch().await?,
            AgentCommands::Codex => agent::codex::run::launch().await?,
            AgentCommands::List => agent::claude::server::list_sessions()?,
            AgentCommands::Prune => agent::claude::server::prune_sessions()?,
            AgentCommands::Kill { session_id } => {
                agent::claude::server::kill_session(&session_id)?
            }
            AgentCommands::Subscribe { session_id } => {
                agent::claude::subscribe::run(&session_id).await?
            }
            AgentCommands::Attach { session_id } => {
                agent::claude::attach::run(&session_id).await?
            }
            AgentCommands::Send { session_id, prompt } => {
                agent::claude::send::run(&session_id, &prompt)?
            }
        },
        Commands::Server => agent::server::run_server().await?,
        Commands::Telegram { command } => match command {
            TelegramCommands::Start => telegram::bot::start().await?,
        },
        Commands::Cron { command } => match command {
            CronCommands::Add { cron, prompt } => {
                let id = cron::storage::add(&cron, &prompt)?;
                println!("added: {}", id);
            }
            CronCommands::List => {
                for job in cron::storage::list()? {
                    println!("{}\t{}\t{}", job.id, job.cron, job.prompt);
                }
            }
            CronCommands::Rm { job_id } => {
                cron::storage::remove(&job_id)?;
                println!("removed: {}", job_id);
            }
        },
        Commands::ClaudeServer { session_id, claude_command, claude_config_dir, initial_prompt } => {
            agent::claude::run::run_server(&session_id, &claude_command, claude_config_dir.as_deref(), initial_prompt.as_deref()).await?
        }
        Commands::CodexServer { session_id, codex_command, initial_prompt } => {
            agent::codex::run::run_server(&session_id, &codex_command, initial_prompt.as_deref()).await?
        }
    }
    Ok(())
}
