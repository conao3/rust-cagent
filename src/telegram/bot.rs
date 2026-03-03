use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::types::ChatAction;
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;
use tokio::task::AbortHandle;

use crate::agent::claude::{run as claude_run, server, session};
use crate::agent::codex::run as codex_run;

use super::config::{self, AgentType};
use super::format;
use super::mapping::{ConversationKey, SessionMap};

pub async fn start() -> anyhow::Result<()> {
    let cfg = config::load()?;

    if let Some(ref wd) = cfg.telegram.working_dir {
        std::env::set_current_dir(wd)?;
    }

    let agent_type: Arc<AgentType> = Arc::new(cfg.agent);
    let claude_command: Arc<String> =
        Arc::new(cfg.claude_command.unwrap_or_else(|| "claude".to_string()));
    let claude_config_dir: Arc<Option<String>> = Arc::new(
        cfg.claude_config_dir
            .map(|p| p.to_string_lossy().into_owned()),
    );
    let codex_command: Arc<String> =
        Arc::new(cfg.codex_command.unwrap_or_else(|| "codex".to_string()));

    let bot = Bot::new(&cfg.telegram.token);
    let session_map = SessionMap::load();
    let active_subscribers: Arc<RwLock<HashMap<String, AbortHandle>>> =
        Arc::new(RwLock::new(HashMap::new()));

    let handler = Update::filter_message().endpoint(handle_message);

    Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![
            agent_type,
            claude_command,
            claude_config_dir,
            codex_command,
            session_map,
            active_subscribers,
            bot.clone()
        ])
        .build()
        .dispatch()
        .await;

    Ok(())
}

fn dispatch_launch(
    agent_type: &AgentType,
    session_id: &str,
    claude_command: &str,
    claude_config_dir: Option<&str>,
    codex_command: &str,
    initial_prompt: Option<&str>,
) -> anyhow::Result<String> {
    match agent_type {
        AgentType::Claude => claude_run::launch_session_with_id(
            session_id,
            claude_command,
            claude_config_dir,
            initial_prompt,
        ),
        AgentType::Codex => codex_run::launch_session_with_id(session_id, codex_command, initial_prompt),
    }
}

fn dispatch_respawn(
    agent_type: &AgentType,
    session_id: &str,
    claude_command: &str,
    claude_config_dir: Option<&str>,
    codex_command: &str,
    initial_prompt: Option<&str>,
) -> anyhow::Result<String> {
    match agent_type {
        AgentType::Claude => claude_run::respawn_session(
            session_id,
            claude_command,
            claude_config_dir,
            initial_prompt,
        ),
        AgentType::Codex => codex_run::respawn_session(session_id, codex_command, initial_prompt),
    }
}

fn session_id_from_key(key: &ConversationKey) -> String {
    let thread_id = key.thread_id.map(|id| id.0.0).unwrap_or(0);
    format!("{}:{thread_id}", key.chat_id)
}

#[allow(clippy::too_many_arguments)]
async fn handle_message(
    bot: Bot,
    msg: Message,
    agent_type: Arc<AgentType>,
    claude_command: Arc<String>,
    claude_config_dir: Arc<Option<String>>,
    codex_command: Arc<String>,
    session_map: SessionMap,
    active_subscribers: Arc<RwLock<HashMap<String, AbortHandle>>>,
) -> anyhow::Result<()> {
    let text = match msg.text() {
        Some(t) => t.to_string(),
        None => return Ok(()),
    };

    let key = ConversationKey::from_message(&msg);
    let derived_session_id = session_id_from_key(&key);
    let chat_id = ChatId(key.chat_id);

    if text == "/new" {
        if let Some(handle) = active_subscribers
            .write()
            .await
            .remove(&derived_session_id)
        {
            handle.abort();
        }
        dispatch_respawn(
            &agent_type,
            &derived_session_id,
            &claude_command,
            (*claude_config_dir).as_deref(),
            &codex_command,
            Some("session renewed"),
        )?;
        session_map
            .insert(key.clone(), derived_session_id.clone())
            .await;

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let abort_handle = spawn_output_subscriber(
            bot.clone(),
            derived_session_id.clone(),
            key.clone(),
            ready_tx,
        );
        active_subscribers
            .write()
            .await
            .insert(derived_session_id, abort_handle);
        let _ = ready_rx.await;

        let mut req = bot.send_message(chat_id, "Session renewed.");
        if let Some(thread_id) = key.thread_id {
            req = req.message_thread_id(thread_id);
        }
        let _ = req.await;
        return Ok(());
    }

    let mut typing_req = bot.send_chat_action(chat_id, ChatAction::Typing);
    if let Some(thread_id) = key.thread_id {
        typing_req = typing_req.message_thread_id(thread_id);
    }
    let _ = typing_req.await;

    let session_id = derived_session_id;
    let is_new_session = !server::session_dir(&session_id).exists();
    if is_new_session {
        dispatch_launch(
            &agent_type,
            &session_id,
            &claude_command,
            (*claude_config_dir).as_deref(),
            &codex_command,
            Some(&text),
        )?;
    }
    session_map.insert(key.clone(), session_id.clone()).await;

    let already_subscribed = active_subscribers.read().await.contains_key(&session_id);
    if !already_subscribed {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let abort_handle = spawn_output_subscriber(bot, session_id.clone(), key, ready_tx);
        active_subscribers
            .write()
            .await
            .insert(session_id.clone(), abort_handle);

        if is_new_session {
            let _ = ready_rx.await;
        }
    }

    if !is_new_session {
        let fifo_path = server::message_send_fifo_path(&session_id);

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut fifo = std::fs::OpenOptions::new().write(true).open(&fifo_path)?;
            fifo.write_all(text.as_bytes())?;
            fifo.write_all(b"\n")?;
            fifo.flush()?;
            Ok(())
        })
        .await??;
    }

    Ok(())
}

fn spawn_output_subscriber(
    bot: Bot,
    session_id: String,
    key: ConversationKey,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> AbortHandle {
    let handle = tokio::spawn(async move {
        let recv_fifo_path = server::message_receive_fifo_path(&session_id);

        for _ in 0..60 {
            if recv_fifo_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let stream = match tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&recv_fifo_path)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to open message_receive.fifo for {session_id}: {e}");
                let _ = ready_tx.send(());
                return;
            }
        };

        let _ = ready_tx.send(());

        let chat_id = ChatId(key.chat_id);
        let typing_active = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let typing_flag = typing_active.clone();
        let typing_bot = bot.clone();
        let typing_thread_id = key.thread_id;
        let typing_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                if typing_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    let mut req = typing_bot.send_chat_action(chat_id, ChatAction::Typing);
                    if let Some(thread_id) = typing_thread_id {
                        req = req.message_thread_id(thread_id);
                    }
                    let _ = req.await;
                }
            }
        });

        let mut lines = tokio::io::BufReader::new(stream).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let parsed: Result<session::SessionMessage, _> = serde_json::from_str(&line);
            let text = match parsed {
                Ok(session::SessionMessage::User { .. }) => {
                    typing_active.store(true, std::sync::atomic::Ordering::Relaxed);
                    continue;
                }
                Ok(session::SessionMessage::Assistant { message }) => {
                    typing_active.store(false, std::sync::atomic::Ordering::Relaxed);
                    let texts: Vec<&str> = message
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            session::ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect();
                    if texts.is_empty() {
                        continue;
                    }
                    texts.join("\n")
                }
                _ => continue,
            };

            for chunk in format::split_for_telegram(&text) {
                let mut req = bot.send_message(chat_id, &chunk);
                if let Some(thread_id) = key.thread_id {
                    req = req.message_thread_id(thread_id);
                }
                if let Err(e) = req.await {
                    tracing::error!("failed to send telegram message: {e}");
                }
            }
        }

        typing_handle.abort();
    });
    handle.abort_handle()
}
