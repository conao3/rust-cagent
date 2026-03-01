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
    #[command(hide = true)]
    Daemon {
        session_id: String,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    Claude,
    List,
    Kill { session_id: String },
    Attach { session_id: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Agent { command } => match command {
            AgentCommands::Claude => agent::claude::run::launch().await?,
            AgentCommands::List => agent::claude::server::list_sessions()?,
            AgentCommands::Kill { session_id } => {
                agent::claude::server::kill_session(&session_id)?
            }
            AgentCommands::Attach { session_id } => {
                agent::claude::attach::run(&session_id).await?
            }
        },
        Commands::Daemon { session_id } => agent::claude::run::run_daemon(&session_id).await?,
    }
    Ok(())
}
