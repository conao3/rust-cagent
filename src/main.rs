mod agent;
mod cli;
mod cron;
mod server;
mod telegram;

#[tokio::main]
async fn main() {
    let filter = if cli::debug_requested() {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    std::process::exit(cli::main().await);
}
