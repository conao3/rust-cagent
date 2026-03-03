use std::env;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::OpenOptionsExt;

use rand::Rng;

use crate::agent::claude::{server, session};
use crate::server as launcher_server;

use super::client::CodexClient;
use super::protocol::{Notification, ThreadItem};

fn generate_session_id() -> String {
    let mut rng = rand::rng();
    format!("{:08x}", rng.random::<u32>())
}

pub fn launch_session(codex_command: &str, initial_prompt: Option<&str>) -> anyhow::Result<String> {
    spawn_session(&generate_session_id(), codex_command, initial_prompt)
}

pub fn respawn_session(
    session_id: &str,
    codex_command: &str,
    initial_prompt: Option<&str>,
) -> anyhow::Result<String> {
    server::force_kill_session(session_id);
    spawn_session(session_id, codex_command, initial_prompt)
}

fn spawn_session(
    session_id: &str,
    codex_command: &str,
    initial_prompt: Option<&str>,
) -> anyhow::Result<String> {
    let session_id = session_id.to_string();

    server::create_session_dir(&session_id)?;

    let mut args = vec![
        "internal".to_string(),
        "codex-wrapper".to_string(),
        "--codex-command".to_string(),
        codex_command.to_string(),
    ];
    if let Some(prompt) = initial_prompt {
        args.extend(["--initial-prompt".to_string(), prompt.to_string()]);
    }
    args.push(session_id.clone());
    launcher_server::spawn_via_server(args)?;

    Ok(session_id)
}

pub async fn launch() -> anyhow::Result<()> {
    let session_id = launch_session("codex", None)?;
    println!("{session_id}");
    Ok(())
}

fn start_fifo_line_reader(
    fifo_path: &std::path::Path,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
) {
    let fifo_path = fifo_path.to_path_buf();
    std::thread::spawn(move || {
        let file = match std::fs::OpenOptions::new()
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

        let reader = BufReader::new(file);
        for line in reader.lines() {
            match line {
                Ok(l) if !l.is_empty() => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
}

fn emit_session_message(
    broadcast_tx: &tokio::sync::mpsc::UnboundedSender<String>,
    msg: &session::SessionMessage,
) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = broadcast_tx.send(json);
    }
}

pub async fn run_server(
    session_id: &str,
    codex_command: &str,
    initial_prompt: Option<&str>,
) -> anyhow::Result<()> {
    let cwd = env::current_dir()?;

    let send_fifo_path = server::message_send_fifo_path(session_id);
    let receive_fifo_path = server::message_receive_fifo_path(session_id);

    server::write_meta(session_id, &cwd)?;

    let mut client = CodexClient::spawn(codex_command, &cwd).await?;
    client.initialize().await?;
    let thread_id = client.thread_start().await?;

    tracing::info!("codex thread started: {thread_id}");

    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    start_fifo_line_reader(&send_fifo_path, prompt_tx.clone());

    if let Some(prompt) = initial_prompt {
        let _ = prompt_tx.send(prompt.to_string());
    }

    let (session_tx, session_rx) = tokio::sync::mpsc::unbounded_channel();
    let _broadcast_tx = server::start_fifo_broadcast(receive_fifo_path, session_rx);

    loop {
        let prompt = match prompt_rx.recv().await {
            Some(p) => p,
            None => break,
        };

        emit_session_message(
            &session_tx,
            &session::SessionMessage::User {
                message: session::MessageBody {
                    content: vec![session::ContentBlock::Text {
                        text: prompt.clone(),
                    }],
                },
            },
        );

        if let Err(e) = client.turn_start(&thread_id, &prompt).await {
            tracing::error!("turn_start failed: {e}");
            break;
        }

        let mut agent_text = String::new();
        loop {
            match client.read_notification().await {
                Ok(Some(Notification::AgentMessageDelta { delta })) => {
                    agent_text.push_str(&delta);
                }
                Ok(Some(Notification::ItemCompleted { item })) => {
                    let blocks = match &item {
                        ThreadItem::AgentMessage { text } if !text.is_empty() => {
                            agent_text.clear();
                            vec![session::ContentBlock::Text { text: text.clone() }]
                        }
                        ThreadItem::Reasoning { summary } if !summary.is_empty() => {
                            vec![session::ContentBlock::Thinking {
                                thinking: summary.join("\n"),
                            }]
                        }
                        ThreadItem::CommandExecution { command, .. } if !command.is_empty() => {
                            vec![session::ContentBlock::ToolUse {
                                name: "command_execution".to_string(),
                                input: serde_json::json!({ "command": command }),
                            }]
                        }
                        _ => vec![],
                    };
                    if !blocks.is_empty() {
                        emit_session_message(
                            &session_tx,
                            &session::SessionMessage::Assistant {
                                message: session::MessageBody { content: blocks },
                            },
                        );
                    }
                }
                Ok(Some(Notification::TurnCompleted)) => {
                    if !agent_text.is_empty() {
                        emit_session_message(
                            &session_tx,
                            &session::SessionMessage::Assistant {
                                message: session::MessageBody {
                                    content: vec![session::ContentBlock::Text {
                                        text: std::mem::take(&mut agent_text),
                                    }],
                                },
                            },
                        );
                    }
                    break;
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    tracing::info!("codex stdout closed");
                    break;
                }
                Err(e) => {
                    tracing::error!("read notification error: {e}");
                    break;
                }
            }
        }
    }

    client.kill().await;
    server::cleanup_session_dir(session_id);

    Ok(())
}
