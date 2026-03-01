mod agent;
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
    Telegram {
        #[command(subcommand)]
        command: TelegramCommands,
    },
    #[command(hide = true)]
    Daemon {
        #[arg(long, default_value = "claude")]
        claude_command: String,
        #[arg(long)]
        claude_config_dir: Option<String>,
        session_id: String,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    Claude,
    List,
    Kill { session_id: String },
    Subscribe { session_id: String },
    Attach { session_id: String },
    Send { session_id: String, prompt: String },
}

#[derive(Subcommand)]
enum TelegramCommands {
    Start,
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
        Commands::Telegram { command } => match command {
            TelegramCommands::Start => telegram::bot::start().await?,
        },
        Commands::Daemon { session_id, claude_command, claude_config_dir } => {
            agent::claude::run::run_daemon(&session_id, &claude_command, claude_config_dir.as_deref()).await?
        }
    }
    Ok(())
}
