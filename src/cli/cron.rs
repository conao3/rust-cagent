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

pub async fn run(command: CronCommand) -> anyhow::Result<()> {
    match command {
        CronCommand::Add { cron, prompt } => {
            let id = crate::cron::storage::add(&cron, &prompt)?;
            println!("added: {}", id);
        }
        CronCommand::List => {
            for job in crate::cron::storage::list()? {
                println!("{}\t{}\t{}", job.id, job.cron, job.prompt);
            }
        }
        CronCommand::Rm { job_id } => {
            crate::cron::storage::remove(&job_id)?;
            println!("removed: {}", job_id);
        }
    }
    Ok(())
}
