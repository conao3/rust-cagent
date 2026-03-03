use std::env;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

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

#[derive(Clone)]
struct AppState {
    exe_path: Arc<std::path::PathBuf>,
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

fn server_addr() -> &'static str {
    "127.0.0.1:45931"
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
    if running_server_pid().is_none() {
        anyhow::bail!("server is not running. start it with `cagent server`");
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;
    let url = format!("http://{}/spawn", server_addr());
    let resp = client.post(url).json(&req).send()?;
    let resp: SpawnResponse = resp.json()?;
    if resp.ok {
        resp.pid.ok_or_else(|| anyhow::anyhow!("server returned no pid"))
    } else {
        anyhow::bail!(resp.error.unwrap_or_else(|| "server request failed".to_string()))
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn spawn_handler(
    State(state): State<AppState>,
    Json(req): Json<SpawnRequest>,
) -> (StatusCode, Json<SpawnResponse>) {
    if req.argv.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(SpawnResponse {
                ok: false,
                pid: None,
                error: Some("argv must not be empty".to_string()),
            }),
        );
    }

    let mut cmd = std::process::Command::new(&*state.exe_path);
    cmd.args(&req.argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    match cmd.spawn() {
        Ok(child) => (
            StatusCode::OK,
            Json(SpawnResponse {
                ok: true,
                pid: Some(child.id()),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SpawnResponse {
                ok: false,
                pid: None,
                error: Some(e.to_string()),
            }),
        ),
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    let state_dir = server_state_dir();
    fs::create_dir_all(&state_dir)?;

    if let Some(pid) = running_server_pid() && pid != std::process::id() {
        anyhow::bail!("server already running: pid={pid}");
    }

    let pid_path = server_pid_path();
    fs::write(&pid_path, format!("{}\n", std::process::id()))?;

    let exe_path = env::current_exe()?;
    let app = Router::new()
        .route("/health", get(health))
        .route("/spawn", post(spawn_handler))
        .with_state(AppState {
            exe_path: Arc::new(exe_path),
        });

    let listener = TcpListener::bind(server_addr()).await?;

    tracing::info!("server started pid={}", std::process::id());
    tracing::info!("pid file: {}", pid_path.display());
    tracing::info!("listen: http://{}", server_addr());

    axum::serve(listener, app).await?;
    Ok(())
}
