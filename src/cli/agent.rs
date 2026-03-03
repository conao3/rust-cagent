use clap::Subcommand;

#[derive(Subcommand)]
pub enum AgentCommand {
    Claude,
    Codex,
    List,
    Prune,
    Kill { session_id: String },
    Subscribe { session_id: String },
    Attach { session_id: String },
    Send { session_id: String, prompt: String },
}
