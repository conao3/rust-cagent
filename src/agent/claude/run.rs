use std::env;
use std::io::Read;

use rand::Rng;

use super::{pty, server, session};

fn generate_session_id() -> String {
    let mut rng = rand::rng();
    format!("{:08x}", rng.random::<u32>())
}

pub async fn run() -> anyhow::Result<()> {
    let cwd = env::current_dir()?;
    let session_id = generate_session_id();

    let session_dir = server::create_session_dir(&session_id)?;
    let fifo_path = session_dir.join("input");
    let sock_path = session_dir.join("output.sock");

    let _guard = pty::RawModeGuard::enter()?;

    let handle = pty::spawn_claude(&cwd)?;

    let stdin_tx = handle.input_tx.clone();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if stdin_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    server::start_fifo_reader(&fifo_path, handle.input_tx.clone());

    let (session_tx, session_rx) = tokio::sync::mpsc::unbounded_channel();
    let session_cwd = cwd.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = session::watch_session(&session_cwd, session_tx) {
            log::error!("session watcher error: {e}");
        }
    });

    let _broadcast_tx = server::start_broadcast_server(sock_path, session_rx);

    eprintln!("\r\n[cagent] session-id: {session_id}");
    eprintln!("[cagent] dir: {}\r", session_dir.display());

    let mut child_exited = handle.child_exited;
    match (&mut child_exited).await {
        Ok(Ok(code)) => log::info!("claude exited with code {code}"),
        Ok(Err(e)) => log::error!("claude wait error: {e}"),
        Err(_) => log::error!("child_exited channel dropped"),
    }

    server::cleanup_session_dir(&session_id);

    Ok(())
}
