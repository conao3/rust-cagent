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
