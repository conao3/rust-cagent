use std::fs;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;

#[derive(Serialize, Deserialize)]
struct SessionMeta {
    pid: u32,
    cwd: String,
}

pub fn session_dir(session_id: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/cagent/session/{session_id}"))
}

pub fn message_send_fifo_path(session_id: &str) -> PathBuf {
    session_dir(session_id).join("message_send.fifo")
}

pub fn message_receive_fifo_path(session_id: &str) -> PathBuf {
    session_dir(session_id).join("message_receive.fifo")
}

pub fn create_session_dir(session_id: &str) -> anyhow::Result<PathBuf> {
    let dir = session_dir(session_id);
    fs::create_dir_all(&dir)?;

    let send_fifo_path = message_send_fifo_path(session_id);
    if !send_fifo_path.exists() {
        nix_mkfifo(&send_fifo_path)?;
    }

    let receive_fifo_path = message_receive_fifo_path(session_id);
    if !receive_fifo_path.exists() {
        nix_mkfifo(&receive_fifo_path)?;
    }

    Ok(dir)
}

fn nix_mkfifo(path: &Path) -> anyhow::Result<()> {
    let c_path = std::ffi::CString::new(
        path.to_str()
            .ok_or_else(|| anyhow::anyhow!("invalid path"))?,
    )?;
    let ret = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    if ret != 0 {
        anyhow::bail!("mkfifo failed: {}", std::io::Error::last_os_error());
    }
    Ok(())
}

pub fn start_fifo_reader(fifo_path: &Path, input_tx: std_mpsc::Sender<Vec<u8>>) {
    let fifo_path = fifo_path.to_path_buf();
    std::thread::spawn(move || {
        let mut file = match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&fifo_path)
        {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("failed to open FIFO: {e}");
                return;
            }
        };

        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }

        let mut buf = [0u8; 4096];
        loop {
            match file.read(&mut buf) {
                Ok(0) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Ok(n) => {
                    let mut data = buf[..n].to_vec();
                    let send_enter = data.last() == Some(&b'\n');
                    if send_enter {
                        data.pop();
                    }
                    if !data.is_empty() && input_tx.send(data).is_err() {
                        break;
                    }
                    if send_enter {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        if input_tx.send(vec![b'\r']).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
}

pub fn start_fifo_broadcast(
    fifo_path: PathBuf,
    mut session_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> broadcast::Sender<String> {
    let (broadcast_tx, _) = broadcast::channel::<String>(256);
    let tx = broadcast_tx.clone();

    tokio::spawn(async move {
        if !fifo_path.exists() && let Err(e) = nix_mkfifo(&fifo_path) {
            tracing::error!("failed to create message receive fifo: {e}");
            return;
        }

        let mut fifo = match tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo_path)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("failed to open message receive fifo: {e}");
                return;
            }
        };

        while let Some(msg) = session_rx.recv().await {
            let _ = tx.send(msg.clone());
            let mut line = msg;
            line.push('\n');
            if let Err(e) = fifo.write_all(line.as_bytes()).await {
                tracing::warn!("failed to write message receive fifo: {e}");
            }
        }
    });

    broadcast_tx
}

pub fn cleanup_session_dir(session_id: &str) {
    let dir = session_dir(session_id);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
}

pub fn write_meta(session_id: &str, cwd: &Path) -> anyhow::Result<()> {
    let meta = SessionMeta {
        pid: std::process::id(),
        cwd: cwd.to_string_lossy().into_owned(),
    };
    fs::write(
        session_dir(session_id).join("meta.json"),
        serde_json::to_string(&meta)?,
    )?;
    Ok(())
}

pub fn list_sessions() -> anyhow::Result<()> {
    let base = PathBuf::from("/tmp/cagent/session");
    if !base.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        let meta_path = entry.path().join("meta.json");
        let content = match fs::read_to_string(&meta_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let meta: SessionMeta = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let alive = unsafe { libc::kill(meta.pid as i32, 0) } == 0;
        if !alive {
            cleanup_session_dir(&entry.file_name().to_string_lossy());
            continue;
        }

        println!(
            "{}\tpid={}\t{}",
            entry.file_name().to_string_lossy(),
            meta.pid,
            meta.cwd,
        );
    }

    Ok(())
}

pub fn prune_sessions() -> anyhow::Result<()> {
    let base = PathBuf::from("/tmp/cagent/session");
    if !base.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        let sid = entry.file_name().to_string_lossy().to_string();
        let meta_path = entry.path().join("meta.json");
        let content = match fs::read_to_string(&meta_path) {
            Ok(c) => c,
            Err(_) => {
                cleanup_session_dir(&sid);
                println!("removed {sid} (no meta)");
                continue;
            }
        };
        let meta: SessionMeta = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => {
                cleanup_session_dir(&sid);
                println!("removed {sid} (invalid meta)");
                continue;
            }
        };

        unsafe { libc::kill(-(meta.pid as i32), libc::SIGKILL) };
        cleanup_session_dir(&sid);
        println!("killed {sid} (pid={})", meta.pid);
    }

    Ok(())
}

pub fn kill_session(session_id: &str) -> anyhow::Result<()> {
    let meta_path = session_dir(session_id).join("meta.json");
    if !meta_path.exists() {
        anyhow::bail!("session not found: {session_id}");
    }

    let meta: SessionMeta = serde_json::from_str(&fs::read_to_string(&meta_path)?)?;

    unsafe {
        libc::kill(-(meta.pid as i32), libc::SIGKILL);
    }

    println!("killed session {session_id} (pid={})", meta.pid);
    Ok(())
}

pub fn force_kill_session(session_id: &str) {
    let meta_path = session_dir(session_id).join("meta.json");
    if let Ok(content) = fs::read_to_string(&meta_path)
        && let Ok(meta) = serde_json::from_str::<SessionMeta>(&content)
    {
        unsafe { libc::kill(-(meta.pid as i32), libc::SIGKILL) };
    }
    cleanup_session_dir(session_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::FileTypeExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_session_id(prefix: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        format!("{prefix}-{now}-{}", std::process::id())
    }

    #[test]
    fn create_and_cleanup_session_dir_with_fifo() {
        let sid = unique_session_id("test-session");
        let dir = create_session_dir(&sid).expect("create session dir");
        let send_fifo = dir.join("message_send.fifo");
        let receive_fifo = dir.join("message_receive.fifo");

        assert!(dir.exists());
        assert!(send_fifo.exists());
        assert!(receive_fifo.exists());
        let meta = std::fs::metadata(&send_fifo).expect("fifo metadata");
        let receive_meta = std::fs::metadata(&receive_fifo).expect("fifo metadata");
        assert!(meta.file_type().is_fifo());
        assert!(receive_meta.file_type().is_fifo());

        cleanup_session_dir(&sid);
        assert!(!dir.exists());
    }

    #[test]
    fn write_meta_creates_meta_json() {
        let sid = unique_session_id("test-meta");
        let dir = create_session_dir(&sid).expect("create session dir");
        let cwd = std::env::current_dir().expect("cwd");

        write_meta(&sid, &cwd).expect("write meta");
        let meta_path = dir.join("meta.json");
        let content = std::fs::read_to_string(meta_path).expect("read meta");
        let parsed: SessionMeta = serde_json::from_str(&content).expect("parse meta");
        assert_eq!(parsed.pid, std::process::id());
        assert_eq!(parsed.cwd, cwd.to_string_lossy());

        cleanup_session_dir(&sid);
    }
}
