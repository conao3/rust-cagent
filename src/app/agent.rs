pub async fn run(command: crate::cli::agent::AgentCommand) -> anyhow::Result<()> {
    match command {
        crate::cli::agent::AgentCommand::Claude => crate::agent::claude::run::launch().await?,
        crate::cli::agent::AgentCommand::Codex => crate::agent::codex::run::launch().await?,
        crate::cli::agent::AgentCommand::List => crate::agent::claude::server::list_sessions()?,
        crate::cli::agent::AgentCommand::Prune => crate::agent::claude::server::prune_sessions()?,
        crate::cli::agent::AgentCommand::Kill { session_id } => {
            crate::agent::claude::server::kill_session(&session_id)?
        }
        crate::cli::agent::AgentCommand::Subscribe { session_id } => {
            crate::agent::claude::subscribe::run(&session_id).await?
        }
        crate::cli::agent::AgentCommand::Attach { session_id } => {
            crate::agent::claude::attach::run(&session_id).await?
        }
        crate::cli::agent::AgentCommand::Send { session_id, prompt } => {
            crate::agent::claude::send::run(&session_id, &prompt)?
        }
    }
    Ok(())
}
