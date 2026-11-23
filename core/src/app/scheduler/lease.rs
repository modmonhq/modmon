//! Holding a claimed run's lease while its container starts, and starting
//! it apart from the look that claimed it.

use std::future::Future;
use std::sync::LazyLock;

use tokio::sync::Semaphore;

use super::*;

const RENEW_EVERY: Duration = Duration::from_secs(SCHEDULER_LEASE.as_secs() / 3);
/// After a renewal the store did not answer, the next try comes sooner.
const RENEW_AGAIN: Duration = Duration::from_secs(2);
/// A claim whose lease ends sooner than this without a renewal is given up
/// before another worker may take the run.
const GIVE_UP_BEFORE: Duration = Duration::from_secs(4);
/// How many runs one process starts in the background at once.
const BACKGROUND_LAUNCHES: usize = 4;

static LAUNCH_SLOTS: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(BACKGROUND_LAUNCHES)));

enum Renewal {
    Held { until_unix_ms: i64 },
    Lost,
    Unanswered(MmError),
}

impl JobSchedulerService {
    /// Looks started by this service start a run's container in a task of
    /// its own, which holds the run's claim until the container runs; the
    /// look goes on to the next run at once.
    #[must_use]
    pub fn with_background_launches(mut self) -> Self {
        self.background_launches = true;
        self
    }

    /// Runs `work` while the claim's lease is renewed, from the start. `None`
    /// once the claim lost the run, to another worker or to a cancel, or
    /// once its lease is about to end with the store not answering: `work`
    /// is then dropped, which stops what it was waiting for.
    pub(super) async fn holding<T>(
        &self,
        claim: &ClaimedJobRun,
        work: impl Future<Output = T>,
    ) -> Result<Option<T>, MmError> {
        let mut held_until = match self.renew(claim) {
            Renewal::Held { until_unix_ms } => until_unix_ms,
            Renewal::Lost => return Ok(None),
            Renewal::Unanswered(error) => return Err(error),
        };
        let mut next = RENEW_EVERY;
        tokio::pin!(work);
        loop {
            tokio::select! {
                biased;
                done = &mut work => return Ok(Some(done)),
                () = tokio::time::sleep(next) => match self.renew(claim) {
                    Renewal::Held { until_unix_ms } => {
                        held_until = until_unix_ms;
                        next = RENEW_EVERY;
                    }
                    Renewal::Lost => return Ok(None),
                    Renewal::Unanswered(error) => {
                        let left = held_until.saturating_sub(self.clock.now_unix_ms());
                        if left <= duration_ms(GIVE_UP_BEFORE) {
                            tracing::warn!(run_id = %claim.run.id, code = error.code(), %error, "the job run's lease could not be renewed before it ended; another worker takes the run");
                            return Ok(None);
                        }
                        tracing::warn!(run_id = %claim.run.id, code = error.code(), %error, "the job run's lease could not be renewed; trying again");
                        next = RENEW_AGAIN;
                    }
                },
            }
        }
    }

    fn renew(&self, claim: &ClaimedJobRun) -> Renewal {
        let now = self.clock.now_unix_ms();
        match self.repository.renew_lease(claim, now, SCHEDULER_LEASE) {
            Ok(true) => Renewal::Held {
                until_unix_ms: now.saturating_add(duration_ms(SCHEDULER_LEASE)),
            },
            Ok(false) => Renewal::Lost,
            Err(error) => Renewal::Unanswered(error),
        }
    }

    /// Starts the claimed run's container: in a task of its own with
    /// background launches, before returning otherwise. With every
    /// background slot taken, the run waits for a later look.
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
        let Ok(slot) = Arc::clone(&LAUNCH_SLOTS).try_acquire_owned() else {
            tracing::debug!(run_id = %claim.run.id, "every background launch slot is taken; a later look starts the run");
            return Ok(());
        };
        let (service, document, selection, target) = (
            self.clone(),
            document.clone(),
            selection.clone(),
            target.clone(),
        );
        let starting = {
            let (service, target, claim) = (service.clone(), target.clone(), claim.clone());
            tokio::spawn(async move {
                service
                    .start(&document, &selection, &target, &claim, output_limit)
                    .await
            })
        };
        tokio::spawn(async move {
            let _slot = slot;
            let ended = match starting.await {
                Ok(started) => started,
                Err(stopped) => {
                    let error = MmError::Launch {
                        message: format!(
                            "starting the job's container stopped inside mm: {stopped}"
                        ),
                    };
                    service.not_started(&target, &claim, error).await
                }
            };
            if let Err(error) = ended {
                tracing::warn!(run_id = %claim.run.id, code = error.code(), %error, "a job run could not start");
            }
        });
        Ok(())
    }
}

fn duration_ms(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
