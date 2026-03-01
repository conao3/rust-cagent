use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    #[default]
    Claude,
    Codex,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub agent: AgentType,
    pub claude_command: Option<String>,
    pub claude_config_dir: Option<PathBuf>,
    pub codex_command: Option<String>,
    pub telegram: TelegramConfig,
}

#[derive(Debug, Deserialize)]
pub struct TelegramConfig {
    pub token: String,
    pub working_dir: Option<PathBuf>,
}

pub fn load() -> anyhow::Result<Config> {
    let path = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("config dir not found"))?
        .join("cagent")
        .join("config.toml");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    Ok(toml::from_str(&content)?)
}
