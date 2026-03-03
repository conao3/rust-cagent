use std::io::{Read, Write};

use tokio::io::AsyncBufReadExt;

use super::server;

pub async fn run(session_id: &str) -> anyhow::Result<()> {
    let dir = server::session_dir(session_id);
    if !dir.exists() {
        anyhow::bail!("session directory not found: {}", dir.display());
    }

    let fifo_path = server::message_send_fifo_path(session_id);
    let recv_fifo_path = server::message_receive_fifo_path(session_id);

    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut fifo = match std::fs::OpenOptions::new().write(true).open(&fifo_path) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("failed to open FIFO for writing: {e}");
                return;
            }
        };
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if fifo.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = fifo.flush();
                }
            }
        }
    });

    let stream = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&recv_fifo_path)
        .await?;
    let reader = tokio::io::BufReader::new(stream);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        println!("{line}");
    }

    Ok(())
}
