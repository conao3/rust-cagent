use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use teloxide::types::ThreadId;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConversationKey {
    pub chat_id: i64,
    pub thread_id: Option<ThreadId>,
}

impl ConversationKey {
    pub fn from_message(msg: &teloxide::types::Message) -> Self {
        Self {
            chat_id: msg.chat.id.0,
            thread_id: msg.thread_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Inner {
    map: HashMap<ConversationKey, String>,
}

#[derive(Debug, Clone)]
pub struct SessionMap {
    inner: Arc<RwLock<Inner>>,
}

fn persistence_path() -> PathBuf {
    PathBuf::from("/tmp/cagent/telegram-mapping.json")
}

impl SessionMap {
    pub fn load() -> Self {
        let inner = std::fs::read_to_string(persistence_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub async fn insert(&self, key: ConversationKey, session_id: String) {
        self.inner.write().await.map.insert(key, session_id);
        self.save().await;
    }

    async fn save(&self) {
        let guard = self.inner.read().await;
        if let Ok(json) = serde_json::to_string_pretty(&*guard) {
            let path = persistence_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, json);
        }
    }
}
