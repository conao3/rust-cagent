use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "cagent - a CLI agent tool")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {}

fn main() {
    let _cli = Cli::parse();
}
