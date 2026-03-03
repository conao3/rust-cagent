pub mod agent;
pub mod claude_server;
pub mod codex_server;
pub mod cron;
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
    ClaudeServer(claude_server::ClaudeServerArgs),
    #[command(hide = true)]
    CodexServer(codex_server::CodexServerArgs),
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Agent { command } => agent::run(command).await?,
        Commands::Server => server::run().await?,
        Commands::Telegram { command } => telegram::run(command).await?,
        Commands::Cron { command } => cron::run(command).await?,
        Commands::ClaudeServer(args) => claude_server::run(args).await?,
        Commands::CodexServer(args) => codex_server::run(args).await?,
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
    fn parses_hidden_claude_server_command() {
        let cli = Cli::try_parse_from([
            "cagent",
            "claude-server",
            "--claude-command",
            "claude",
            "--initial-prompt",
            "hi",
            "1234abcd",
        ])
        .expect("parse hidden claude-server");
        match cli.command {
            Commands::ClaudeServer(args) => {
                assert_eq!(args.session_id, "1234abcd");
                assert_eq!(args.claude_command, "claude");
                assert_eq!(args.initial_prompt.as_deref(), Some("hi"));
            }
            _ => panic!("unexpected command"),
        }
    }
}
