use clap::Parser;

#[derive(Parser)]
pub struct InternalCodexWrapperArgs {
    #[arg(long, default_value = "codex")]
    pub codex_command: String,
    #[arg(long)]
    pub initial_prompt: Option<String>,
    pub session_id: String,
}

pub async fn run(args: InternalCodexWrapperArgs) -> anyhow::Result<()> {
    crate::agent::codex::run::run_server(
        &args.session_id,
        &args.codex_command,
        args.initial_prompt.as_deref(),
    )
    .await
}
