pub async fn run(command: crate::cli::internal::InternalCommand) -> anyhow::Result<()> {
    match command {
        crate::cli::internal::InternalCommand::ClaudeWrapper(args) => {
            crate::agent::claude::run::run_server(
                &args.session_id,
                &args.claude_command,
                args.claude_config_dir.as_deref(),
                args.initial_prompt.as_deref(),
            )
            .await?
        }
        crate::cli::internal::InternalCommand::CodexWrapper(args) => {
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
