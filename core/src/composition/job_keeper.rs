//! What `mmd` does with jobs by itself, on a thread of its own: it moves on
//! the runs nobody waits for (a `once` job, a run whose client went away),
//! retrying them by their policy, and starts the runs of the schedules a
//! deployment reconciled on this host. On a cluster node only the jobs
//! placed on it exist here, so a schedule runs on its job's node alone.

use std::sync::Arc;
use std::time::Duration;

use crate::adapters::remote::{DaemonConfig, HostAccessStore};
use crate::domain::error::MmError;

/// How often the keeper looks.
const KEEP_EVERY: Duration = Duration::from_secs(15);

pub struct JobKeeper {
    config: DaemonConfig,
    use_cases: Arc<super::DaemonUseCaseAdapter>,
}

impl JobKeeper {
    #[must_use]
    pub fn new(config: DaemonConfig, use_cases: Arc<super::DaemonUseCaseAdapter>) -> Arc<Self> {
        Arc::new(Self { config, use_cases })
    }

    /// Looks every few seconds for as long as the process lives.
    pub async fn run(self: Arc<Self>) {
        loop {
            tokio::time::sleep(KEEP_EVERY).await;
            if let Err(error) = self.keep().await {
                tracing::warn!(code = error.code(), %error, "the job keeper could not look at the jobs");
            }
        }
    }

    async fn keep(&self) -> Result<(), MmError> {
        let access = HostAccessStore::new(&self.config);
        let snapshot = access.load()?;
        let node = self.config.node_env()?.map(|node| node.name);
        for (project, enrolled) in &snapshot.host.projects {
            for environment in &enrolled.environments {
                let Some(deployed) = access.deployed(*project, environment)? else {
                    continue;
                };
                if let Err(error) = self
                    .use_cases
                    .keep_jobs(*project, environment, &deployed.root, node.as_deref())
                    .await
                {
                    tracing::warn!(environment = %environment, code = error.code(), %error, "the job keeper could not move the environment's jobs on");
                }
            }
        }
        Ok(())
    }
}
