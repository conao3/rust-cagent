use clap::Parser;

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

pub async fn run(args: InternalClaudeWrapperArgs) -> anyhow::Result<()> {
    crate::agent::claude::run::run_server(
        &args.session_id,
        &args.claude_command,
        args.claude_config_dir.as_deref(),
        args.initial_prompt.as_deref(),
    )
    .await
}
