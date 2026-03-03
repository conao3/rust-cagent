use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub id: u64,
    pub method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub id: u64,
    pub result: serde_json::Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub id: u64,
    pub error: JsonRpcErrorDetail,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct JsonRpcErrorDetail {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub client_info: ClientInfo,
}

#[derive(Debug, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub user_agent: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadStartParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadStartResponse {
    pub thread: Thread,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartParams {
    pub thread_id: String,
    pub input: Vec<UserInput>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UserInput {
    Text { text: String },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartResponse {
    pub turn: Turn,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub status: String,
}

#[derive(Debug)]
pub enum Notification {
    TurnStarted,
    TurnCompleted,
    ItemCompleted { item: ThreadItem },
    AgentMessageDelta { delta: String },
    Other(()),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ThreadItem {
    AgentMessage {
        #[serde(default)]
        text: String,
    },
    Reasoning {
        #[serde(default)]
        summary: Vec<String>,
    },
    CommandExecution {
        #[serde(default)]
        command: String,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNotification {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemNotificationParams {
    item: ThreadItem,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentMessageDeltaParams {
    delta: String,
}

impl Notification {
    pub fn parse(line: &str) -> Option<Self> {
        let raw: RawNotification = serde_json::from_str(line).ok()?;
        match raw.method.as_str() {
            "turn/started" => Some(Notification::TurnStarted),
            "turn/completed" => Some(Notification::TurnCompleted),
            "item/completed" => {
                let params: ItemNotificationParams = serde_json::from_value(raw.params).ok()?;
                Some(Notification::ItemCompleted { item: params.item })
            }
            "item/agentMessage/delta" => {
                let params: AgentMessageDeltaParams = serde_json::from_value(raw.params).ok()?;
                Some(Notification::AgentMessageDelta {
                    delta: params.delta,
                })
            }
            _ => Some(Notification::Other(())),
        }
    }
}
