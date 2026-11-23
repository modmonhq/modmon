//! Holding a claimed run's lease while its container starts, and starting
//! it apart from the look that claimed it.

use std::future::Future;

use super::*;

const RENEW_EVERY: Duration = Duration::from_secs(SCHEDULER_LEASE.as_secs() / 3);

impl JobSchedulerService {
    /// Looks started by this service start a run's container in a task of
    /// its own, which holds the run's claim until the container runs; the
    /// look goes on to the next run at once.
    #[must_use]
    pub fn with_background_launches(mut self) -> Self {
        self.background_launches = true;
        self
    }

    /// Runs `work` while the claim's lease is renewed. `None` once the
    /// claim lost the run, to another worker or to a cancel: `work` is then
    /// dropped, which stops what it was waiting for.
    pub(super) async fn holding<T>(
        &self,
        claim: &ClaimedJobRun,
        work: impl Future<Output = T>,
    ) -> Result<Option<T>, MmError> {
        tokio::pin!(work);
        loop {
            tokio::select! {
                biased;
                done = &mut work => return Ok(Some(done)),
                () = tokio::time::sleep(RENEW_EVERY) => {
                    if !self.repository.renew_lease(claim, self.clock.now_unix_ms(), SCHEDULER_LEASE)? {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Starts the claimed run's container: in a task of its own with
    /// background launches, before returning otherwise.
    pub(super) async fn launch_claimed(
        &self,
        document: &ConfigDocument,
        selection: &WorkspaceSelection,
        target: &crate::domain::runtime::RuntimeTarget,
        claim: ClaimedJobRun,
        output_limit: usize,
    ) -> Result<(), MmError> {
        if !self.background_launches {
            return self
                .start(document, selection, target, &claim, output_limit)
                .await;
        }
        let service = self.clone();
        let (document, selection, target) = (document.clone(), selection.clone(), target.clone());
        tokio::spawn(async move {
            if let Err(error) = service
                .start(&document, &selection, &target, &claim, output_limit)
                .await
            {
                tracing::warn!(run_id = %claim.run.id, code = error.code(), %error, "a job run could not start");
            }
        });
        Ok(())
    }
}
