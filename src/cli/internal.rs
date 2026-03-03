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
