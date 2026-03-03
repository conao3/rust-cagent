use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum AgentCommand {
    Claude(ClaudeArgs),
    Codex(CodexArgs),
    List,
    Prune,
    Kill { session_id: String },
    Subscribe { session_id: String },
    Attach { session_id: String },
    Send { session_id: String, prompt: String },
}

#[derive(Args)]
pub struct ClaudeArgs {
    #[arg(long)]
    pub run: bool,
    #[arg(long)]
    pub session_id: Option<String>,
    #[arg(long, default_value = "claude")]
    pub claude_command: String,
    #[arg(long)]
    pub claude_config_dir: Option<String>,
    #[arg(long)]
    pub initial_prompt: Option<String>,
}

#[derive(Args)]
pub struct CodexArgs {
    #[arg(long)]
    pub run: bool,
    #[arg(long)]
    pub session_id: Option<String>,
    #[arg(long, default_value = "codex")]
    pub codex_command: String,
    #[arg(long)]
    pub initial_prompt: Option<String>,
}

pub async fn run(command: AgentCommand) -> anyhow::Result<()> {
    match command {
        AgentCommand::Claude(args) => {
            if args.run {
                let session_id = args
                    .session_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("--session-id is required with --run"))?;
                crate::agent::claude::run::run_server(
                    session_id,
                    &args.claude_command,
                    args.claude_config_dir.as_deref(),
                    args.initial_prompt.as_deref(),
                )
                .await?;
            } else {
                let session_id = match args.session_id.as_deref() {
                    Some(session_id) => crate::agent::claude::run::launch_session_with_id(
                        session_id,
                        &args.claude_command,
                        args.claude_config_dir.as_deref(),
                        args.initial_prompt.as_deref(),
                    )?,
                    None => crate::agent::claude::run::launch_session(
                        &args.claude_command,
                        args.claude_config_dir.as_deref(),
                        args.initial_prompt.as_deref(),
                    )?,
                };
                println!("{session_id}");
            }
        }
        AgentCommand::Codex(args) => {
            if args.run {
                let session_id = args
                    .session_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("--session-id is required with --run"))?;
                crate::agent::codex::run::run_server(
                    session_id,
                    &args.codex_command,
                    args.initial_prompt.as_deref(),
                )
                .await?;
            } else {
                let session_id = match args.session_id.as_deref() {
                    Some(session_id) => crate::agent::codex::run::launch_session_with_id(
                        session_id,
                        &args.codex_command,
                        args.initial_prompt.as_deref(),
                    )?,
                    None => crate::agent::codex::run::launch_session(
                        &args.codex_command,
                        args.initial_prompt.as_deref(),
                    )?,
                };
                println!("{session_id}");
            }
        }
        AgentCommand::List => crate::agent::claude::server::list_sessions()?,
        AgentCommand::Prune => crate::agent::claude::server::prune_sessions()?,
        AgentCommand::Kill { session_id } => crate::agent::claude::server::kill_session(&session_id)?,
        AgentCommand::Subscribe { session_id } => crate::agent::claude::subscribe::run(&session_id).await?,
        AgentCommand::Attach { session_id } => crate::agent::claude::attach::run(&session_id).await?,
        AgentCommand::Send { session_id, prompt } => crate::agent::claude::send::run(&session_id, &prompt)?,
    }
    Ok(())
}
