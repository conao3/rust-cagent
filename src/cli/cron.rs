use clap::Subcommand;

#[derive(Subcommand)]
pub enum CronCommand {
    Add {
        #[arg(long)]
        cron: String,
        #[arg(long)]
        prompt: String,
    },
    List,
    Rm {
        job_id: String,
    },
}
