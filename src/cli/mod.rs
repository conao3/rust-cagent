pub mod agent;
pub mod cron;
pub mod internal;
pub mod server;
pub mod telegram;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "cagent - a CLI agent tool")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Agent {
        #[command(subcommand)]
        command: agent::AgentCommand,
    },
    Server,
    Telegram {
        #[command(subcommand)]
        command: telegram::TelegramCommand,
    },
    Cron {
        #[command(subcommand)]
        command: cron::CronCommand,
    },
    #[command(hide = true)]
    Internal {
        #[command(subcommand)]
        command: internal::InternalCommand,
    },
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Agent { command } => agent::run(command).await?,
        Commands::Server => server::run().await?,
        Commands::Telegram { command } => telegram::run(command).await?,
        Commands::Cron { command } => cron::run(command).await?,
        Commands::Internal { command } => internal::run(command).await?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_command() {
        let cli = Cli::try_parse_from(["cagent", "server"]).expect("parse server");
        assert!(matches!(cli.command, Commands::Server));
    }

    #[test]
    fn parses_agent_send_command() {
        let cli = Cli::try_parse_from(["cagent", "agent", "send", "deadbeef", "hello"])
            .expect("parse agent send");
        match cli.command {
            Commands::Agent { command } => match command {
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
    fn parses_hidden_internal_claude_wrapper_command() {
        let cli = Cli::try_parse_from([
            "cagent",
            "internal",
            "claude-wrapper",
            "--claude-command",
            "claude",
            "--initial-prompt",
            "hi",
            "1234abcd",
        ])
        .expect("parse hidden internal claude-wrapper");
        match cli.command {
            Commands::Internal { command } => match command {
                internal::InternalCommand::ClaudeWrapper(args) => {
                    assert_eq!(args.session_id, "1234abcd");
                    assert_eq!(args.claude_command, "claude");
                    assert_eq!(args.initial_prompt.as_deref(), Some("hi"));
                }
                _ => panic!("unexpected internal command"),
            },
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_hidden_internal_codex_wrapper_command() {
        let cli = Cli::try_parse_from([
            "cagent",
            "internal",
            "codex-wrapper",
            "--codex-command",
            "codex",
            "--initial-prompt",
            "hi",
            "1234abcd",
        ])
        .expect("parse hidden internal codex-wrapper");
        match cli.command {
            Commands::Internal { command } => match command {
                internal::InternalCommand::CodexWrapper(args) => {
                    assert_eq!(args.session_id, "1234abcd");
                    assert_eq!(args.codex_command, "codex");
                    assert_eq!(args.initial_prompt.as_deref(), Some("hi"));
                }
                _ => panic!("unexpected internal command"),
            },
            _ => panic!("unexpected command"),
        }
    }
}
