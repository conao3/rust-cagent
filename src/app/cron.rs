pub async fn run(command: crate::cli::cron::CronCommand) -> anyhow::Result<()> {
    match command {
        crate::cli::cron::CronCommand::Add { cron, prompt } => {
            let id = crate::cron::storage::add(&cron, &prompt)?;
            println!("added: {}", id);
        }
        crate::cli::cron::CronCommand::List => {
            for job in crate::cron::storage::list()? {
                println!("{}\t{}\t{}", job.id, job.cron, job.prompt);
            }
        }
        crate::cli::cron::CronCommand::Rm { job_id } => {
            crate::cron::storage::remove(&job_id)?;
            println!("removed: {}", job_id);
        }
    }
    Ok(())
}
