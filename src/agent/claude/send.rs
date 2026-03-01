use std::io::Write;

use super::server;

pub fn run(session_id: &str, prompt: &str) -> anyhow::Result<()> {
    let dir = server::session_dir(session_id);
    if !dir.exists() {
        anyhow::bail!("session directory not found: {}", dir.display());
    }

    let fifo_path = dir.join("input");
    let mut fifo = std::fs::OpenOptions::new()
        .write(true)
        .open(&fifo_path)?;
    fifo.write_all(prompt.as_bytes())?;
    fifo.write_all(b"\n")?;
    fifo.flush()?;
    Ok(())
}
