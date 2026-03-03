use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

#[derive(Serialize, Deserialize)]
struct SpawnRequest {
    argv: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct SpawnResponse {
    ok: bool,
    pid: Option<u32>,
    error: Option<String>,
}

fn base_state_dir() -> PathBuf {
    if let Some(dir) = env::var_os("XDG_STATE_HOME") {
        PathBuf::from(dir)
    } else if let Some(dir) = dirs::state_dir() {
        dir
    } else if let Some(home) = env::var_os("HOME") {
        PathBuf::from(home).join(".local/state")
    } else {
        PathBuf::from("/tmp")
    }
}

fn server_state_dir() -> PathBuf {
    base_state_dir().join("cagent")
}

pub fn server_pid_path() -> PathBuf {
    server_state_dir().join("server-pid")
}

fn server_sock_path() -> PathBuf {
    server_state_dir().join("server.sock")
}

fn read_server_pid() -> Option<u32> {
    let content = fs::read_to_string(server_pid_path()).ok()?;
    content.trim().parse::<u32>().ok()
}

fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn running_server_pid() -> Option<u32> {
    let pid = read_server_pid()?;
    if is_pid_alive(pid) {
        Some(pid)
    } else {
        None
    }
}

pub fn spawn_via_server(argv: Vec<String>) -> anyhow::Result<u32> {
    let req = SpawnRequest { argv };
    let sock = server_sock_path();
    if !sock.exists() {
        anyhow::bail!("server is not running. start it with `cagent server`");
    }
    let mut stream = {
        let start = std::time::Instant::now();
        loop {
            match StdUnixStream::connect(&sock) {
                Ok(stream) => break stream,
                Err(e) => {
                    if start.elapsed() >= std::time::Duration::from_secs(3) {
                        anyhow::bail!("failed to connect server socket: {e}");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    };
    let body = serde_json::to_vec(&req)?;
    stream.write_all(&body)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;

    let resp: SpawnResponse = serde_json::from_slice(&buf)?;
    if resp.ok {
        resp.pid.ok_or_else(|| anyhow::anyhow!("server returned no pid"))
    } else {
        anyhow::bail!(resp.error.unwrap_or_else(|| "server request failed".to_string()))
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    let state_dir = server_state_dir();
    fs::create_dir_all(&state_dir)?;

    if let Some(pid) = running_server_pid()
        && pid != std::process::id()
        && server_sock_path().exists()
    {
        anyhow::bail!("server already running: pid={pid}");
    }

    let pid_path = server_pid_path();
    fs::write(&pid_path, format!("{}\n", std::process::id()))?;

    let sock = server_sock_path();
    if sock.exists() {
        let _ = fs::remove_file(&sock);
    }
    let listener = UnixListener::bind(&sock)?;

    tracing::info!("server started pid={}", std::process::id());
    tracing::info!("pid file: {}", pid_path.display());
    tracing::info!("socket: {}", sock.display());

    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let resp = match stream.read_to_end(&mut buf).await {
                Ok(_) => {
                    let req: anyhow::Result<SpawnRequest> = serde_json::from_slice(&buf).map_err(anyhow::Error::from);
                    match req {
                        Ok(req) => {
                            if req.argv.is_empty() {
                                SpawnResponse {
                                    ok: false,
                                    pid: None,
                                    error: Some("argv must not be empty".to_string()),
                                }
                            } else {
                                match env::current_exe() {
                                    Ok(exe) => {
                                        let mut cmd = std::process::Command::new(exe);
                                        cmd.args(&req.argv)
                                            .stdin(Stdio::null())
                                            .stdout(Stdio::null())
                                            .stderr(Stdio::null())
                                            .process_group(0);
                                        match cmd.spawn() {
                                            Ok(child) => SpawnResponse {
                                                ok: true,
                                                pid: Some(child.id()),
                                                error: None,
                                            },
                                            Err(e) => SpawnResponse {
                                                ok: false,
                                                pid: None,
                                                error: Some(e.to_string()),
                                            },
                                        }
                                    }
                                    Err(e) => SpawnResponse {
                                        ok: false,
                                        pid: None,
                                        error: Some(e.to_string()),
                                    },
                                }
                            }
                        }
                        Err(e) => SpawnResponse {
                            ok: false,
                            pid: None,
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => SpawnResponse {
                    ok: false,
                    pid: None,
                    error: Some(e.to_string()),
                },
            };

            if let Ok(out) = serde_json::to_vec(&resp) {
                let _ = stream.write_all(&out).await;
            }
        });
    }
}
