pub mod agent;
pub mod cron;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "cagent - a CLI agent tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Agent {
        #[command(subcommand)]
        command: agent::AgentCommand,
    },
    Server,
    Cron {
        #[command(subcommand)]
        command: cron::CronCommand,
    },
}

pub fn parse_command() -> Command {
    Cli::parse().command
}

pub async fn run() -> anyhow::Result<()> {
    match parse_command() {
        Command::Agent { command } => agent::run(command).await?,
        Command::Server => crate::server::run_server().await?,
        Command::Cron { command } => cron::run(command).await?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_command() {
        let cli = Cli::try_parse_from(["cagent", "server"]).expect("parse server");
        assert!(matches!(cli.command, Command::Server));
    }

    #[test]
    fn parses_agent_send_command() {
        let cli = Cli::try_parse_from(["cagent", "agent", "send", "deadbeef", "hello"])
            .expect("parse agent send");
        match cli.command {
            Command::Agent { command } => match command {
                agent::AgentCommand::Send { session_id, prompt } => {
                    assert_eq!(session_id, "deadbeef");
                    assert_eq!(prompt, "hello");
                }
                _ => panic!("unexpected agent command"),
            },
            _ => panic!("unexpected root command"),
        }
    }

    #[test]
    fn parses_agent_claude_run_command() {
        let cli = Cli::try_parse_from([
            "cagent",
            "agent",
            "claude",
            "--run",
            "--session-id",
            "1234abcd",
            "--claude-command",
            "claude",
            "--initial-prompt",
            "hi",
        ])
        .expect("parse agent claude run");
        match cli.command {
            Command::Agent { command } => match command {
                agent::AgentCommand::Claude(args) => {
                    assert!(args.run);
                    assert_eq!(args.session_id.as_deref(), Some("1234abcd"));
                    assert_eq!(args.claude_command, "claude");
                    assert_eq!(args.initial_prompt.as_deref(), Some("hi"));
                }
                _ => panic!("unexpected agent command"),
            },
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_agent_codex_run_command() {
        let cli = Cli::try_parse_from([
            "cagent",
            "agent",
            "codex",
            "--run",
            "--session-id",
            "1234abcd",
            "--codex-command",
            "codex",
            "--initial-prompt",
            "hi",
        ])
        .expect("parse agent codex run");
        match cli.command {
            Command::Agent { command } => match command {
                agent::AgentCommand::Codex(args) => {
                    assert!(args.run);
                    assert_eq!(args.session_id.as_deref(), Some("1234abcd"));
                    assert_eq!(args.codex_command, "codex");
                    assert_eq!(args.initial_prompt.as_deref(), Some("hi"));
                }
                _ => panic!("unexpected agent command"),
            },
            _ => panic!("unexpected command"),
        }
    }
}
