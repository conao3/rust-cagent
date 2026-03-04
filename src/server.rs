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
    if is_pid_alive(pid) { Some(pid) } else { None }
}

pub fn spawn_via_server(argv: Vec<String>) -> anyhow::Result<u32> {
    if running_server_pid().is_none() {
        anyhow::bail!("server is not running. start it with `cagent server`");
    }
    tracing::info!("spawning via server: {:?}", argv);
    std::thread::spawn(move || {
        let req = SpawnRequest { argv };
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?;
        let resp: SpawnResponse = client
            .post(format!("http://{}/spawn", server_addr()))
            .json(&req)
            .send()?
            .json()?;
        if resp.ok {
            tracing::info!("spawned pid={:?}", resp.pid);
            resp.pid
                .ok_or_else(|| anyhow::anyhow!("server returned no pid"))
        } else {
            tracing::error!("spawn failed: {:?}", resp.error);
            anyhow::bail!(
                resp.error
                    .unwrap_or_else(|| "server request failed".to_string())
            )
        }
    })
    .join()
    .map_err(|_| anyhow::anyhow!("spawn_via_server thread panicked"))?
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

    tracing::info!("spawn_handler: argv={:?}", req.argv);
    let mut cmd = std::process::Command::new(&*state.exe_path);
    cmd.args(&req.argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    match cmd.spawn() {
        Ok(child) => {
            tracing::info!("spawn_handler: spawned pid={}", child.id());
            (
                StatusCode::OK,
                Json(SpawnResponse {
                    ok: true,
                    pid: Some(child.id()),
                    error: None,
                }),
            )
        }
        Err(e) => {
            tracing::error!("spawn_handler: failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SpawnResponse {
                    ok: false,
                    pid: None,
                    error: Some(e.to_string()),
                }),
            )
        }
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    let state_dir = server_state_dir();
    fs::create_dir_all(&state_dir)?;

    if let Some(pid) = running_server_pid()
        && pid != std::process::id()
    {
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

    let mut telegram_task = tokio::spawn(async { crate::telegram::bot::start().await });
    let mut api_task = tokio::spawn(async move { axum::serve(listener, app).await });

    tokio::select! {
        res = &mut api_task => {
            telegram_task.abort();
            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => anyhow::bail!("api server failed: {e}"),
                Err(e) => anyhow::bail!("api task join error: {e}"),
            }
        }
        res = &mut telegram_task => {
            api_task.abort();
            match res {
                Ok(Ok(())) => anyhow::bail!("telegram bot stopped unexpectedly"),
                Ok(Err(e)) => anyhow::bail!("telegram bot failed: {e}"),
                Err(e) => anyhow::bail!("telegram task join error: {e}"),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_returns_ok() {
        assert_eq!(health().await, "ok");
    }

    #[tokio::test]
    async fn spawn_handler_rejects_empty_argv() {
        let state = AppState {
            exe_path: Arc::new(std::env::current_exe().expect("current exe")),
        };
        let (status, Json(resp)) =
            spawn_handler(State(state), Json(SpawnRequest { argv: vec![] })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!resp.ok);
        assert!(resp.pid.is_none());
    }

    #[tokio::test]
    async fn spawn_handler_spawns_process() {
        let state = AppState {
            exe_path: Arc::new(std::path::PathBuf::from("/bin/sh")),
        };
        let (status, Json(resp)) = spawn_handler(
            State(state),
            Json(SpawnRequest {
                argv: vec!["-c".to_string(), "true".to_string()],
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(resp.ok);
        assert!(resp.pid.is_some());
    }

    #[tokio::test]
    async fn spawn_via_server_no_panic_in_async_context() {
        fs::create_dir_all(server_state_dir()).unwrap();
        let backup = fs::read_to_string(server_pid_path()).ok();
        fs::write(server_pid_path(), format!("{}\n", std::process::id())).unwrap();

        let _result = spawn_via_server(vec!["--help".to_string()]);

        match backup {
            Some(content) => fs::write(server_pid_path(), content).unwrap(),
            None => {
                let _ = fs::remove_file(server_pid_path());
            }
        }
    }
}
