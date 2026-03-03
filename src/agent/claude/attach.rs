pub async fn run(session_id: &str) -> anyhow::Result<()> {
    super::subscribe::run(session_id).await
}
