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

fn resolve_claude_config_dir() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }

    if let Some(dir) = extract_config_dir_from_wrapper() {
        return Ok(dir);
    }

    Ok(dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("HOME not found"))?
        .join(".claude"))
}

fn extract_config_dir_from_wrapper() -> Option<PathBuf> {
    let output = std::process::Command::new("which")
        .arg("claude")
        .output()
        .ok()?;
    let mut script_path = fs::canonicalize(String::from_utf8_lossy(&output.stdout).trim()).ok()?;

    for _ in 0..10 {
        let content = fs::read_to_string(&script_path).ok()?;
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("export CLAUDE_CONFIG_DIR=") {
                let val = rest.trim_matches('"').replace("$HOME", &dirs::home_dir()?.to_string_lossy());
                return Some(PathBuf::from(val));
            }
        }
        let next_cmd = content.lines().find_map(|l| {
            let l = l.trim();
            l.strip_prefix("exec ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").trim_matches('"').to_string())
        })?;
        if next_cmd.is_empty() {
            return None;
        }
        let resolved = if next_cmd.starts_with('/') {
            PathBuf::from(&next_cmd)
        } else {
            let output = std::process::Command::new("which")
                .arg(&next_cmd)
                .output()
                .ok()?;
            PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        };
        script_path = fs::canonicalize(resolved).ok()?;
    }
    None
}

fn session_dir(cwd: &Path) -> anyhow::Result<PathBuf> {
    let base = resolve_claude_config_dir()?;
    let cwd_str = cwd.to_string_lossy();
    let hash = cwd_str.replace('/', "-").replace('.', "-");
    Ok(base.join("projects").join(hash))
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
    let mut target_file: Option<PathBuf> = None;

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

            if existing.contains(&path) && target_file.is_none() {
                continue;
            }

            if target_file.is_none() {
                log::info!("tracking new session file: {}", path.display());
                target_file = Some(path.clone());
            } else if target_file.as_ref() != Some(&path) {
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
