use clap::Subcommand;

#[derive(Subcommand)]
pub enum TelegramCommand {
    Start,
}

pub async fn run(command: TelegramCommand) -> anyhow::Result<()> {
    match command {
        TelegramCommand::Start => crate::telegram::bot::start().await?,
    }
    Ok(())
}
