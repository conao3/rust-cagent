pub async fn run() -> anyhow::Result<()> {
    crate::server::run_server().await
}
