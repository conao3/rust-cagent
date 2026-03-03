use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub cron: String,
    pub prompt: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CronStore {
    jobs: Vec<CronJob>,
}

fn storage_path() -> PathBuf {
    dirs::config_dir()
        .expect("config dir not found")
        .join("cagent")
        .join("cron.json")
}

fn load() -> CronStore {
    std::fs::read_to_string(storage_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(store: &CronStore) -> anyhow::Result<()> {
    let path = storage_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(store)?)?;
    Ok(())
}

fn generate_id(store: &CronStore) -> String {
    let mut rng = rand::rng();
    loop {
        let id = format!("{:04x}", rand::Rng::random_range(&mut rng, 0..0x10000u32));
        if !store.jobs.iter().any(|j| j.id == id) {
            return id;
        }
    }
}

pub fn add(cron_expr: &str, prompt: &str) -> anyhow::Result<String> {
    let mut store = load();
    let id = generate_id(&store);
    store.jobs.push(CronJob {
        id: id.clone(),
        cron: cron_expr.to_string(),
        prompt: prompt.to_string(),
    });
    save(&store)?;
    Ok(id)
}

pub fn list() -> anyhow::Result<Vec<CronJob>> {
    Ok(load().jobs)
}

pub fn remove(job_id: &str) -> anyhow::Result<()> {
    let mut store = load();
    let len_before = store.jobs.len();
    store.jobs.retain(|j| j.id != job_id);
    if store.jobs.len() == len_before {
        anyhow::bail!("job not found: {}", job_id);
    }
    save(&store)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn add_list_remove_roundtrip() {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock");
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: serialized by ENV_LOCK in this test module.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", tmp.path()) };

        let id = add("*/5 * * * *", "hello").expect("add");
        let jobs = list().expect("list");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);
        assert_eq!(jobs[0].cron, "*/5 * * * *");
        assert_eq!(jobs[0].prompt, "hello");

        remove(&id).expect("remove");
        let jobs = list().expect("list after remove");
        assert!(jobs.is_empty());

        // SAFETY: serialized by ENV_LOCK in this test module.
        unsafe {
            if let Some(v) = old {
                std::env::set_var("XDG_CONFIG_HOME", v);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
    }

    #[test]
    fn remove_missing_job_returns_error() {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock");
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: serialized by ENV_LOCK in this test module.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", tmp.path()) };

        let err = remove("no-such-job").expect_err("must fail");
        assert!(err.to_string().contains("job not found"));

        // SAFETY: serialized by ENV_LOCK in this test module.
        unsafe {
            if let Some(v) = old {
                std::env::set_var("XDG_CONFIG_HOME", v);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
    }
}
