use std::env;

use super::{pty, session};

pub async fn run() -> anyhow::Result<()> {
    let cwd = env::current_dir()?;
    let _guard = pty::RawModeGuard::enter()?;
    let handle = pty::spawn_claude(&cwd)?;

    let (session_tx, mut session_rx) = tokio::sync::mpsc::unbounded_channel();
    let session_cwd = cwd.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = session::watch_session(&session_cwd, session_tx) {
            log::error!("session watcher error: {e}");
        }
    });

    let mut child_exited = handle.child_exited;
    loop {
        tokio::select! {
            result = &mut child_exited => {
                match result {
                    Ok(Ok(code)) => log::info!("claude exited with code {code}"),
                    Ok(Err(e)) => log::error!("claude wait error: {e}"),
                    Err(_) => log::error!("child_exited channel dropped"),
                }
                break;
            }
            Some(msg) = session_rx.recv() => {
                log::info!("session: {msg:?}");
            }
        }
    }

    while let Ok(msg) = session_rx.try_recv() {
        log::info!("session: {msg:?}");
    }

    Ok(())
}
