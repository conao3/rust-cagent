use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum InternalCommand {
    ClaudeWrapper(InternalClaudeWrapperArgs),
    CodexWrapper(InternalCodexWrapperArgs),
}

#[derive(Parser)]
pub struct InternalClaudeWrapperArgs {
    #[arg(long, default_value = "claude")]
    pub claude_command: String,
    #[arg(long)]
    pub claude_config_dir: Option<String>,
    #[arg(long)]
    pub initial_prompt: Option<String>,
    pub session_id: String,
}

#[derive(Parser)]
pub struct InternalCodexWrapperArgs {
    #[arg(long, default_value = "codex")]
    pub codex_command: String,
    #[arg(long)]
    pub initial_prompt: Option<String>,
    pub session_id: String,
}

pub async fn run(command: InternalCommand) -> anyhow::Result<()> {
    match command {
        InternalCommand::ClaudeWrapper(args) => {
            crate::agent::claude::run::run_server(
                &args.session_id,
                &args.claude_command,
                args.claude_config_dir.as_deref(),
                args.initial_prompt.as_deref(),
            )
            .await?
        }
        InternalCommand::CodexWrapper(args) => {
            crate::agent::codex::run::run_server(
                &args.session_id,
                &args.codex_command,
                args.initial_prompt.as_deref(),
            )
            .await?
        }
    }
    Ok(())
}
