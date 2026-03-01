mod agent;

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
    Attach {
        session_id: String,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    Claude,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Agent { command } => match command {
            AgentCommands::Claude => agent::claude::run::run().await?,
        },
        Commands::Attach { session_id } => agent::claude::attach::run(&session_id).await?,
    }
    Ok(())
}
