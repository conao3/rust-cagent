use std::fs;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::sync::broadcast;

#[derive(Serialize, Deserialize)]
struct SessionMeta {
    pid: u32,
    cwd: String,
}

pub fn session_dir(session_id: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/cagent/{session_id}"))
}

pub fn create_session_dir(session_id: &str) -> anyhow::Result<PathBuf> {
    let dir = session_dir(session_id);
    fs::create_dir_all(&dir)?;

    let fifo_path = dir.join("input");
    if !fifo_path.exists() {
        nix_mkfifo(&fifo_path)?;
    }

    Ok(dir)
}

fn nix_mkfifo(path: &Path) -> anyhow::Result<()> {
    let c_path = std::ffi::CString::new(path.to_str().ok_or_else(|| anyhow::anyhow!("invalid path"))?)?;
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
                log::error!("failed to open FIFO: {e}");
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
                    if !data.is_empty() {
                        if input_tx.send(data).is_err() {
                            break;
                        }
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

pub fn start_broadcast_server(
    sock_path: PathBuf,
    mut session_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> broadcast::Sender<String> {
    let (broadcast_tx, _) = broadcast::channel::<String>(256);
    let tx = broadcast_tx.clone();

    tokio::spawn(async move {
        if sock_path.exists() {
            let _ = fs::remove_file(&sock_path);
        }

        let listener = match UnixListener::bind(&sock_path) {
            Ok(l) => l,
            Err(e) => {
                log::error!("failed to bind unix socket: {e}");
                return;
            }
        };

        let accept_tx = tx.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let mut rx = accept_tx.subscribe();
                        tokio::spawn(async move {
                            let (_, mut writer) = stream.into_split();
                            while let Ok(msg) = rx.recv().await {
                                let mut line = msg;
                                line.push('\n');
                                if writer.write_all(line.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::warn!("unix socket accept error: {e}");
                    }
                }
            }
        });

        while let Some(msg) = session_rx.recv().await {
            let _ = tx.send(msg);
        }
    });

    broadcast_tx
}

pub fn start_pty_server(
    sock_path: PathBuf,
    mut output_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let (broadcast_tx, _) = broadcast::channel::<Vec<u8>>(256);
    let tx = broadcast_tx.clone();

    tokio::spawn(async move {
        if sock_path.exists() {
            let _ = fs::remove_file(&sock_path);
        }

        let listener = match UnixListener::bind(&sock_path) {
            Ok(l) => l,
            Err(e) => {
                log::error!("failed to bind pty socket: {e}");
                return;
            }
        };

        let accept_tx = tx.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let mut rx = accept_tx.subscribe();
                        tokio::spawn(async move {
                            let (_, mut writer) = stream.into_split();
                            while let Ok(data) = rx.recv().await {
                                if writer.write_all(&data).await.is_err() {
                                    break;
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::warn!("pty socket accept error: {e}");
                    }
                }
            }
        });

        while let Some(data) = output_rx.recv().await {
            let _ = tx.send(data);
        }
    });
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
    let base = PathBuf::from("/tmp/cagent");
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
    let base = PathBuf::from("/tmp/cagent");
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
    if let Ok(content) = fs::read_to_string(&meta_path) {
        if let Ok(meta) = serde_json::from_str::<SessionMeta>(&content) {
            unsafe { libc::kill(-(meta.pid as i32), libc::SIGKILL) };
        }
    }
    cleanup_session_dir(session_id);
}
