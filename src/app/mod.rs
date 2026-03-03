mod agent;
mod cron;
mod internal;

pub async fn run(command: crate::cli::Command) -> anyhow::Result<()> {
    match command {
        crate::cli::Command::Agent { command } => agent::run(command).await?,
        crate::cli::Command::Server => crate::server::run_server().await?,
        crate::cli::Command::Cron { command } => cron::run(command).await?,
        crate::cli::Command::Internal { command } => internal::run(command).await?,
    }
    Ok(())
}
