use std::env;

use rand::Rng;

use crate::agent::server as launcher_server;

use super::{pty, server, session};

fn generate_session_id() -> String {
    let mut rng = rand::rng();
    format!("{:08x}", rng.random::<u32>())
}

pub fn launch_session(claude_command: &str, claude_config_dir: Option<&str>, initial_prompt: Option<&str>) -> anyhow::Result<String> {
    spawn_session(&generate_session_id(), claude_command, claude_config_dir, initial_prompt)
}

pub fn respawn_session(session_id: &str, claude_command: &str, claude_config_dir: Option<&str>, initial_prompt: Option<&str>) -> anyhow::Result<String> {
    server::force_kill_session(session_id);
    spawn_session(session_id, claude_command, claude_config_dir, initial_prompt)
}

fn spawn_session(session_id: &str, claude_command: &str, claude_config_dir: Option<&str>, initial_prompt: Option<&str>) -> anyhow::Result<String> {
    let session_id = session_id.to_string();

    server::create_session_dir(&session_id)?;

    let mut args = vec!["claude-server".to_string(), "--claude-command".to_string(), claude_command.to_string()];
    if let Some(dir) = claude_config_dir {
        args.extend(["--claude-config-dir".to_string(), dir.to_string()]);
    }
    if let Some(prompt) = initial_prompt {
        args.extend(["--initial-prompt".to_string(), prompt.to_string()]);
    }
    args.push(session_id.clone());
    launcher_server::spawn_via_server(args)?;

    Ok(session_id)
}

pub async fn launch() -> anyhow::Result<()> {
    let session_id = launch_session("claude", None, None)?;
    println!("{session_id}");
    Ok(())
}

pub async fn run_server(session_id: &str, claude_command: &str, claude_config_dir: Option<&str>, initial_prompt: Option<&str>) -> anyhow::Result<()> {
    let cwd = env::current_dir()?;

    let session_dir = server::session_dir(session_id);
    let fifo_path = session_dir.join("input");
    let sock_path = session_dir.join("output.sock");

    server::write_meta(session_id, &cwd)?;

    let handle = pty::spawn_claude(&cwd, claude_command, initial_prompt)?;

    server::start_fifo_reader(&fifo_path, handle.input_tx.clone());

    let pty_sock_path = session_dir.join("pty.sock");
    server::start_pty_server(pty_sock_path, handle.output_rx);

    let (session_tx, session_rx) = tokio::sync::mpsc::unbounded_channel();
    let session_cwd = cwd.clone();
    let session_config_dir = claude_config_dir.map(std::path::PathBuf::from);
    tokio::task::spawn_blocking(move || {
        if let Err(e) = session::watch_session(&session_cwd, session_config_dir, session_tx) {
            log::error!("session watcher error: {e}");
        }
    });

    let _broadcast_tx = server::start_broadcast_server(sock_path, session_rx);

    let mut child_exited = handle.child_exited;
    match (&mut child_exited).await {
        Ok(Ok(code)) => log::info!("claude exited with code {code}"),
        Ok(Err(e)) => log::error!("claude wait error: {e}"),
        Err(_) => log::error!("child_exited channel dropped"),
    }

    server::cleanup_session_dir(session_id);

    Ok(())
}
