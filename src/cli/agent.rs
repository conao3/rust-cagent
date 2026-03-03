use clap::Subcommand;

#[derive(Subcommand)]
pub enum AgentCommand {
    Claude,
    Codex,
    List,
    Prune,
    Kill { session_id: String },
    Subscribe { session_id: String },
    Attach { session_id: String },
    Send { session_id: String, prompt: String },
}

pub async fn run(command: AgentCommand) -> anyhow::Result<()> {
    match command {
        AgentCommand::Claude => crate::agent::claude::run::launch().await?,
        AgentCommand::Codex => crate::agent::codex::run::launch().await?,
        AgentCommand::List => crate::agent::claude::server::list_sessions()?,
        AgentCommand::Prune => crate::agent::claude::server::prune_sessions()?,
        AgentCommand::Kill { session_id } => crate::agent::claude::server::kill_session(&session_id)?,
        AgentCommand::Subscribe { session_id } => crate::agent::claude::subscribe::run(&session_id).await?,
        AgentCommand::Attach { session_id } => crate::agent::claude::attach::run(&session_id).await?,
        AgentCommand::Send { session_id, prompt } => crate::agent::claude::send::run(&session_id, &prompt)?,
    }
    Ok(())
}
