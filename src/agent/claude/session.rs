use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;

use notify::{EventKind, RecursiveMode, Watcher};
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionMessage {
    #[serde(rename = "queue_operation")]
    QueueOperation(serde_json::Value),
    #[serde(rename = "progress")]
    Progress(serde_json::Value),
    #[serde(rename = "user")]
    User { message: MessageBody },
    #[serde(rename = "assistant")]
    Assistant { message: MessageBody },
    #[serde(rename = "system")]
    System(serde_json::Value),
    #[serde(rename = "file_history_snapshot")]
    FileHistorySnapshot(serde_json::Value),
    #[serde(other)]
    Unknown,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MessageBody {
    #[serde(default)]
    pub content: Vec<ContentBlock>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
    #[serde(other)]
    Other,
}

fn session_dir(cwd: &Path) -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("HOME not found"))?;
    let cwd_str = cwd.to_string_lossy();
    let hash = cwd_str.replace('/', "-");
    Ok(home.join(".claude").join("projects").join(hash))
}

pub fn watch_session(
    cwd: &Path,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
) -> anyhow::Result<()> {
    let dir = session_dir(cwd)?;
    log::info!("watching session dir: {}", dir.display());

    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }

    let existing: std::collections::HashSet<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();

    let mut offsets: HashMap<PathBuf, u64> = HashMap::new();
    for path in &existing {
        offsets.insert(path.clone(), fs::metadata(path)?.len());
    }

    let (notify_tx, notify_rx) = std_mpsc::channel();
    let mut watcher = notify::recommended_watcher(notify_tx)?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;

    for event in notify_rx {
        let event = match event {
            Ok(e) => e,
            Err(e) => {
                log::warn!("notify error: {e}");
                continue;
            }
        };

        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {}
            _ => continue,
        }

        for path in event.paths {
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }

            let offset = offsets.get(&path).copied().unwrap_or(0);
            let mut file = match fs::File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    log::warn!("failed to open {}: {e}", path.display());
                    continue;
                }
            };

            if file.seek(SeekFrom::Start(offset)).is_err() {
                continue;
            }

            let reader = BufReader::new(&mut file);
            let mut new_offset = offset;

            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                new_offset += line.len() as u64 + 1;

                if line.trim().is_empty() {
                    continue;
                }

                if tx.send(line).is_err() {
                    return Ok(());
                }
            }

            offsets.insert(path, new_offset);
        }
    }

    drop(watcher);
    Ok(())
}
