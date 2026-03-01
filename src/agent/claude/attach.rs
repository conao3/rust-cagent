use std::io::{Read, Write};

use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;

use super::pty::RawModeGuard;
use super::server;

pub async fn run(session_id: &str) -> anyhow::Result<()> {
    let dir = server::session_dir(session_id);
    if !dir.exists() {
        anyhow::bail!("session directory not found: {}", dir.display());
    }

    let fifo_path = dir.join("input");
    let pty_sock_path = dir.join("pty.sock");

    let _raw = RawModeGuard::enter()?;

    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut fifo = match std::fs::OpenOptions::new().write(true).open(&fifo_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("failed to open FIFO for writing: {e}");
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

    let stream = UnixStream::connect(&pty_sock_path).await?;
    let (mut reader, _) = stream.into_split();
    let mut stdout = tokio::io::stdout();
    let mut buf = [0u8; 4096];

    loop {
        match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tokio::io::AsyncWriteExt::write_all(&mut stdout, &buf[..n])
                    .await
                    .is_err()
                {
                    break;
                }
                let _ = tokio::io::AsyncWriteExt::flush(&mut stdout).await;
            }
        }
    }

    Ok(())
}
