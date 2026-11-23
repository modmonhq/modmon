//! Holding a claimed run's lease while its container starts, and starting
//! it apart from the look that claimed it.

use std::future::Future;
use std::sync::{Mutex, PoisonError};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::*;

const RENEW_EVERY: Duration = Duration::from_secs(SCHEDULER_LEASE.as_secs() / 3);
/// After a renewal the store did not answer, the next try comes sooner.
const RENEW_AGAIN: Duration = Duration::from_secs(2);
/// A claim whose lease ends sooner than this without a renewal is given up
/// before another worker may take the run; longer than the state store's
/// busy timeout, so one more try still fits.
const GIVE_UP_BEFORE: Duration = Duration::from_secs(6);
/// How many job images one environment pulls at once in the background.
const PULLS_PER_ENVIRONMENT: usize = 4;

/// The image pulls background starts may run at once, per environment.
/// Shared by every look of one host, so a later look waits for the pulls
/// an earlier one started.
#[derive(Default)]
pub struct JobPullSlots(Mutex<BTreeMap<String, Arc<Semaphore>>>);

impl JobPullSlots {
    fn of(&self, run: &JobRun) -> Arc<Semaphore> {
        let mut slots = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(
            slots
                .entry(format!("{}/{}", run.project_id.0, run.environment))
                .or_insert_with(|| Arc::new(Semaphore::new(PULLS_PER_ENVIRONMENT))),
        )
    }
}

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
        self.pull_slots.get_or_insert_with(Default::default);
        self
    }

    /// Background starts take their pull slots from `slots`, which the
    /// host keeps from one look to the next.
    #[must_use]
    pub fn with_pull_slots(mut self, slots: Arc<JobPullSlots>) -> Self {
        self.pull_slots = Some(slots);
        self
    }

    /// A slot for the claimed run's image pull: `None` when pulls are not
    /// limited. Waits, with the run's lease held, until one is free.
    pub(super) async fn pull_slot(
        &self,
        claim: &ClaimedJobRun,
    ) -> Result<Option<OwnedSemaphorePermit>, MmError> {
        let Some(slots) = &self.pull_slots else {
            return Ok(None);
        };
        slots
            .of(&claim.run)
            .acquire_owned()
            .await
            .map(Some)
            .map_err(|closed| MmError::State {
                message: format!("the job pull slots were closed: {closed}"),
            })
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
