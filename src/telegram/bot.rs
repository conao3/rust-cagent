use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;

use teloxide::prelude::*;
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixStream;
use tokio::sync::RwLock;

use crate::agent::claude::{run as claude_run, server, session};

use super::config;
use super::format;
use super::mapping::{ConversationKey, SessionMap};

pub async fn start() -> anyhow::Result<()> {
    let cfg = config::load()?;

    if let Some(ref wd) = cfg.telegram.working_dir {
        std::env::set_current_dir(wd)?;
    }

    let claude_command: Arc<String> = Arc::new(
        cfg.claude_command.unwrap_or_else(|| "claude".to_string()),
    );
    let claude_config_dir: Arc<Option<String>> = Arc::new(
        cfg.claude_config_dir.map(|p| p.to_string_lossy().into_owned()),
    );

    let bot = Bot::new(&cfg.telegram.token);
    let session_map = SessionMap::load();
    let active_subscribers: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));

    let handler = Update::filter_message().endpoint(handle_message);

    Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![
            claude_command,
            claude_config_dir,
            session_map,
            active_subscribers,
            bot.clone()
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    claude_command: Arc<String>,
    claude_config_dir: Arc<Option<String>>,
    session_map: SessionMap,
    active_subscribers: Arc<RwLock<HashSet<String>>>,
) -> anyhow::Result<()> {
    let text = match msg.text() {
        Some(t) => t.to_string(),
        None => return Ok(()),
    };

    let key = ConversationKey::from_message(&msg);

    let is_new_session;
    let session_id = match session_map.get(&key).await {
        Some(id) if server::session_dir(&id).exists() => {
            is_new_session = false;
            id
        }
        existing => {
            if existing.is_some() {
                session_map.remove(&key).await;
            }
            let id = claude_run::launch_session(&claude_command, (*claude_config_dir).as_deref(), Some(&text))?;
            session_map.insert(key.clone(), id.clone()).await;
            is_new_session = true;
            id
        }
    };

    let already_subscribed = active_subscribers.read().await.contains(&session_id);
    if !already_subscribed {
        active_subscribers
            .write()
            .await
            .insert(session_id.clone());

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        spawn_output_subscriber(
            bot,
            session_id.clone(),
            key,
            active_subscribers,
            ready_tx,
        );

        if is_new_session {
            let _ = ready_rx.await;
        }
    }

    if !is_new_session {
        let dir = server::session_dir(&session_id);
        let fifo_path = dir.join("input");

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut fifo = std::fs::OpenOptions::new()
                .write(true)
                .open(&fifo_path)?;
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
    active_subscribers: Arc<RwLock<HashSet<String>>>,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) {
    tokio::spawn(async move {
        let dir = server::session_dir(&session_id);
        let sock_path = dir.join("output.sock");

        for _ in 0..60 {
            if sock_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let stream = match UnixStream::connect(&sock_path).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("failed to connect output.sock for {session_id}: {e}");
                active_subscribers.write().await.remove(&session_id);
                let _ = ready_tx.send(());
                return;
            }
        };

        let _ = ready_tx.send(());

        let (reader, _) = stream.into_split();
        let mut lines = tokio::io::BufReader::new(reader).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let parsed: Result<session::SessionMessage, _> = serde_json::from_str(&line);
            let text = match parsed {
                Ok(session::SessionMessage::Assistant { message }) => {
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

            let chat_id = ChatId(key.chat_id);
            for chunk in format::split_for_telegram(&text) {
                let mut req = bot.send_message(chat_id, &chunk);
                if let Some(thread_id) = key.thread_id {
                    req = req.message_thread_id(thread_id);
                }
                if let Err(e) = req.await {
                    log::error!("failed to send telegram message: {e}");
                }
            }
        }

        active_subscribers.write().await.remove(&session_id);
    });
}
