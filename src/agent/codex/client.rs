use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout};

use super::protocol::*;

pub struct CodexClient {
    child: Child,
    writer: tokio::io::BufWriter<tokio::process::ChildStdin>,
    reader: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl CodexClient {
    pub async fn spawn(command: &str, cwd: &Path) -> anyhow::Result<Self> {
        let mut child = tokio::process::Command::new(command)
            .arg("app-server")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no stdout"))?;

        Ok(Self {
            child,
            writer: tokio::io::BufWriter::new(stdin),
            reader: BufReader::new(stdout).lines(),
            next_id: 1,
        })
    }

    async fn send_request(&mut self, method: &'static str, params: Option<serde_json::Value>) -> anyhow::Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = JsonRpcRequest { id, method, params };
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        tracing::debug!("codex send: {}", line.trim());
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;

        loop {
            let raw = self.reader.next_line().await?
                .ok_or_else(|| anyhow::anyhow!("codex stdout closed while waiting for response to {method}"))?;

            tracing::debug!("codex recv (waiting for id={id}): {raw}");

            if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&raw)
                && resp.id == id {
                    return Ok(resp.result);
            }
            if let Ok(err) = serde_json::from_str::<JsonRpcError>(&raw)
                && err.id == id {
                    anyhow::bail!("codex error: {} (code={})", err.error.message, err.error.code);
            }
        }
    }

    async fn send_notification(&mut self, method: &'static str, params: Option<serde_json::Value>) -> anyhow::Result<()> {
        let mut obj = serde_json::Map::new();
        obj.insert("method".to_string(), serde_json::Value::String(method.to_string()));
        if let Some(p) = params {
            obj.insert("params".to_string(), p);
        }
        let mut line = serde_json::to_string(&obj)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let params = InitializeParams {
            client_info: ClientInfo {
                name: "cagent".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };
        let _resp: InitializeResponse = serde_json::from_value(
            self.send_request("initialize", Some(serde_json::to_value(&params)?)).await?,
        )?;
        self.send_notification("initialized", None).await?;
        Ok(())
    }

    pub async fn thread_start(&mut self) -> anyhow::Result<String> {
        let params = ThreadStartParams {
            approval_policy: Some("never".to_string()),
        };
        let resp: ThreadStartResponse = serde_json::from_value(
            self.send_request("thread/start", Some(serde_json::to_value(&params)?)).await?,
        )?;
        Ok(resp.thread.id)
    }

    pub async fn turn_start(&mut self, thread_id: &str, text: &str) -> anyhow::Result<()> {
        let params = TurnStartParams {
            thread_id: thread_id.to_string(),
            input: vec![UserInput::Text { text: text.to_string() }],
        };
        let _resp: TurnStartResponse = serde_json::from_value(
            self.send_request("turn/start", Some(serde_json::to_value(&params)?)).await?,
        )?;
        Ok(())
    }

    pub async fn read_notification(&mut self) -> anyhow::Result<Option<Notification>> {
        let raw = match self.reader.next_line().await? {
            Some(l) => l,
            None => return Ok(None),
        };
        tracing::debug!("codex notification: {raw}");
        Ok(Notification::parse(&raw))
    }

    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }
}
