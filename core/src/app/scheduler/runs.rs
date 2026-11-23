//! Running one declared job now, and reading what ran.
//!
//! A run is recorded first, then its container starts detached from
//! whoever asked: a client that stops waiting does not stop the job. Every
//! later look at the run (a wait, a read of the runs) moves its record on,
//! and cleans up its container once it ended.

use super::*;
use crate::domain::scheduler::{JobRunRequest, JobRunsRequest, MAX_LISTED_RUNS};

/// How often a run that is waited for is looked at.
const FOLLOW_INTERVAL: Duration = Duration::from_millis(500);

impl JobSchedulerService {
    /// Records a run of a declared job and starts it. With `wait`, follows
    /// it (and its retries) until it ends; otherwise answers once its
    /// container runs.
    pub async fn run_job(
        &self,
        selection: &WorkspaceSelection,
        request: &JobRunRequest,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let (document, environment) = self.context(selection)?;
        let deployment = request.deployment.as_deref().map(deployment_approval);
        let approval = deployment.as_ref().unwrap_or(approval);
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRun)?;
        let job = active_job(&document, &environment, &request.job)
            .map_err(|_| not_declared_here(selection, &document, &environment, &request.job))?
            .clone();
        let run = self.insert_run(NewRunInput {
            id: request.run,
            document: &document,
            environment: &environment,
            job_name: &request.job,
            job,
            schedule: None,
            trigger: request.trigger,
            retry_of: None,
            approval,
        })?;
        self.follow(selection, &document, run.id, request.wait, approval)
            .await
    }

    /// Records a new run, under the ID its caller chose or a new one.
    pub(super) fn insert_run(&self, input: NewRunInput<'_>) -> Result<JobRun, MmError> {
        let id = input.id.unwrap_or_else(Uuid::new_v4);
        let now = self.clock.now_unix_ms();
        let schedule_name = input.schedule.as_ref().map(|(name, _)| name.clone());
        let run = NewJobRun {
            id,
            project_id: input.document.config.project.id,
            environment: input.environment.to_owned(),
            job: input.job_name.to_owned(),
            schedule: schedule_name,
            trigger: input.trigger,
            idempotency_key: format!("operator:{}:{id}", input.approval.principal),
            job_definition: input.job,
            schedule_definition: input.schedule.map(|(_, definition)| definition),
            config_hash: input.document.source_hash.clone(),
            requested_at_unix_ms: now,
            scheduled_for_unix_ms: None,
            principal: input.approval.principal.clone(),
            source: input.approval.source.clone(),
            retry_of_run_id: input.retry_of,
        };
        self.repository.insert_run(
            &run,
            &audit(
                input.document,
                input.environment,
                input.approval,
                self.clock.now_unix_ms(),
                "enqueue",
                "run",
                id.to_string(),
            ),
        )
    }

    /// The recorded runs, newest first. Runs that were still going are
    /// looked at first, so what they say is current.
    pub async fn runs(
        &self,
        selection: &WorkspaceSelection,
        request: &JobRunsRequest,
        approval: &ApprovalContext,
    ) -> Result<Vec<JobRun>, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRead)?;
        let limit = request.limit.clamp(1, MAX_LISTED_RUNS) as usize;
        let listed = |service: &Self| -> Result<Vec<JobRun>, MmError> {
            Ok(match request.run {
                Some(id) => service
                    .repository
                    .run(id)?
                    .filter(|run| {
                        run.project_id == document.config.project.id
                            && run.environment == environment
                    })
                    .into_iter()
                    .collect(),
                None => service.repository.history(
                    document.config.project.id,
                    &environment,
                    request.job.as_deref(),
                    None,
                    limit,
                )?,
            })
        };
        let mut runs = listed(self)?;
        let mut moved = false;
        for run in runs.iter().filter(|run| run.state.is_active()) {
            moved |= self.advance(selection, &document, run.id, approval).await?;
        }
        if moved {
            runs = listed(self)?;
        }
        Ok(runs)
    }

    /// Follows a recorded run as `run_job` follows a new one.
    pub async fn follow_run(
        &self,
        selection: &WorkspaceSelection,
        run_id: Uuid,
        wait: bool,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let (document, _) = self.context(selection)?;
        self.follow(selection, &document, run_id, wait, approval)
            .await
    }

    /// Moves run `run_id` on as far as it goes now and returns it, until
    /// it ended (or, without `wait`, until its container runs).
    async fn follow(
        &self,
        selection: &WorkspaceSelection,
        document: &ConfigDocument,
        run_id: Uuid,
        wait: bool,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        loop {
            self.advance(selection, document, run_id, approval).await?;
            let run = self.repository.run(run_id)?.ok_or_else(|| MmError::State {
                message: format!("job run '{run_id}' disappeared while it was followed"),
            })?;
            let settled = run.state.is_terminal() || run.state == JobRunState::Unknown;
            if settled || (!wait && run.state == JobRunState::Running) {
                return Ok(run);
            }
            tokio::time::sleep(FOLLOW_INTERVAL).await;
        }
    }

    /// One look at run `run_id`, when no other worker holds it. Whether
    /// there was anything to look at.
    async fn advance(
        &self,
        selection: &WorkspaceSelection,
        document: &ConfigDocument,
        run_id: Uuid,
        approval: &ApprovalContext,
    ) -> Result<bool, MmError> {
        let claim = self.repository.claim_run(
            run_id,
            Uuid::new_v4(),
            self.clock.now_unix_ms(),
            SCHEDULER_LEASE,
        )?;
        let Some(claim) = claim else {
            return Ok(false);
        };
        self.process_claim(document, selection, claim, approval)
            .await?;
        Ok(true)
    }
}

impl JobSchedulerService {
    /// The environment's job runs that run now, as operations.
    pub fn in_flight(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
    ) -> Result<Vec<crate::domain::rollout::OperationInFlight>, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRead)?;
        let now = self.clock.now_unix_ms();
        Ok(self
            .repository
            .history(
                document.config.project.id,
                &environment,
                None,
                None,
                MAX_LISTED_RUNS as usize,
            )?
            .iter()
            .filter(|run| crate::domain::scheduler::runs_now(run))
            .map(|run| {
                let timeout = match document
                    .config
                    .jobs
                    .get(&run.job)
                    .map(|job| job.timeout.parse())
                {
                    Some(Ok(timeout)) => timeout,
                    Some(Err(_)) | None => Duration::ZERO,
                };
                crate::domain::scheduler::run_in_flight(run, timeout, now)
            })
            .collect())
    }

    /// Stops the runs that `started` names and that have not ended: each
    /// run's container is stopped and the run is recorded as cancelled
    /// (`MM_CANCELLED`). A run another worker looks at right now is stopped
    /// at its next look. Returns the runs as they are afterwards.
    pub async fn cancel_started(
        &self,
        selection: &WorkspaceSelection,
        started: &[crate::domain::scheduler::StartedRuns],
        approval: &ApprovalContext,
    ) -> Result<Vec<JobRun>, MmError> {
        let (document, environment) = self.context(selection)?;
        // Without jobs nothing ran, and the engine is not asked.
        if document.config.jobs.is_empty() {
            return Ok(Vec::new());
        }
        self.authorize(
            &document,
            &environment,
            approval,
            RuntimeOperation::JobCancel,
        )?;
        let project = document.config.project.id;
        let running = self
            .repository
            .history(project, &environment, None, None, MAX_LISTED_RUNS as usize)?
            .into_iter()
            .filter(|run| {
                !run.state.is_terminal() && started.iter().any(|which| which.contain(run))
            })
            .collect::<Vec<_>>();
        let mut cancelled = Vec::new();
        for run in running {
            let record = audit(
                &document,
                &environment,
                approval,
                self.clock.now_unix_ms(),
                "cancel",
                "run",
                run.id.to_string(),
            );
            self.repository.request_cancel(run.id, &record)?;
            self.advance(selection, &document, run.id, approval).await?;
            cancelled.push(self.repository.run(run.id)?.unwrap_or(run));
        }
        Ok(cancelled)
    }
}

/// Why a job cannot run here: not declared at all, or declared for
/// another environment or, on a cluster, for another node.
fn not_declared_here(
    selection: &WorkspaceSelection,
    document: &ConfigDocument,
    environment: &str,
    job: &str,
) -> MmError {
    let message = match (document.config.jobs.get(job), &selection.node) {
        (None, _) => {
            let declared = document
                .config
                .jobs
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            crate::domain::scheduler::undeclared_job(&declared, job)
        }
        (Some(_), Some(node)) => format!(
            "job '{job}' does not run on node '{}' in environment '{environment}'",
            node.node
        ),
        (Some(_), None) => {
            format!("job '{job}' is not configured for environment '{environment}'")
        }
    };
    MmError::Environment { message }
}

/// A job's container is created from these files, staged by whoever made
/// them: a deployment step that runs the job.
pub struct FixedJobTarget(pub crate::domain::runtime::RuntimeTarget);

#[async_trait::async_trait]
impl JobLaunchTargets for FixedJobTarget {
    async fn prepare(
        &self,
        _selection: &WorkspaceSelection,
        _stack: &str,
        _run: Uuid,
    ) -> Result<Option<crate::app::ports::PreparedJobTarget>, MmError> {
        Ok(Some(crate::app::ports::PreparedJobTarget {
            target: self.0.clone(),
            release: Box::new(|| Ok(())),
        }))
    }
}

#[async_trait::async_trait]
impl crate::app::ports::DeploymentJobs for JobSchedulerService {
    async fn run(
        &self,
        selection: &WorkspaceSelection,
        job: &str,
        target: crate::domain::runtime::RuntimeTarget,
        plan_hash: &str,
    ) -> Result<JobRun, MmError> {
        let request = JobRunRequest {
            job: job.to_owned(),
            wait: true,
            trigger: JobTrigger::BeforeDeploy,
            deployment: Some(plan_hash.to_owned()),
            run: None,
        };
        self.clone()
            .with_launch_targets(Arc::new(FixedJobTarget(target)))
            .run_job(selection, &request, &deployment_approval(plan_hash))
            .await
    }

    async fn sweep(
        &self,
        selection: &WorkspaceSelection,
        projects: &[crate::domain::runtime::RuntimeTarget],
    ) -> Result<Vec<String>, MmError> {
        let (document, environment) = self.context(selection)?;
        let approval = deployment_approval("sweep");
        let listed = |service: &Self| {
            service.repository.history(
                document.config.project.id,
                &environment,
                None,
                None,
                MAX_LISTED_RUNS as usize,
            )
        };
        for run in listed(self)?.iter().filter(|run| run.state.is_active()) {
            if matches!(
                run.trigger,
                JobTrigger::BeforeDeploy | JobTrigger::AfterDeploy | JobTrigger::Once
            ) {
                let record = audit(
                    &document,
                    &environment,
                    &approval,
                    self.clock.now_unix_ms(),
                    "cancel",
                    "run",
                    run.id.to_string(),
                );
                self.repository.request_cancel(run.id, &record)?;
            }
            self.advance(selection, &document, run.id, &approval)
                .await?;
        }
        let waited_for = listed(self)?
            .into_iter()
            .filter(|run| !run.state.is_terminal())
            .map(|run| run.runtime_name)
            .collect::<std::collections::BTreeSet<_>>();
        let mut removed = Vec::new();
        for project in projects {
            for name in self.runtime.job_containers(project).await? {
                if !waited_for.contains(&name) {
                    self.runtime.cleanup(project, &name, None).await?;
                    removed.push(name);
                }
            }
        }
        Ok(removed)
    }
}

/// Who a deployment's job runs as, in the run's record.
fn deployment_approval(plan_hash: &str) -> ApprovalContext {
    ApprovalContext {
        principal: "deployment".to_owned(),
        source: format!("deployment {}", plan_hash.get(..12).unwrap_or(plan_hash)),
        approved_high_risk: false,
    }
}

impl JobSchedulerService {
    /// One look by the host itself: it moves on each run that is not over
    /// (except one a deployment queued, which that deployment starts with
    /// its own files), and when a schedule is due, runs a controller
    /// cycle. With nothing to do, the engine is not asked.
    pub async fn keep(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
    ) -> Result<(), MmError> {
        let (document, context) = self.application.operation_context(selection)?;
        let environment = context.selection.environment;
        let project = document.config.project.id;
        let now = self.clock.now_unix_ms();
        let schedule_due = self
            .repository
            .list_schedules(project, &environment)?
            .iter()
            .any(|schedule| {
                schedule.enabled
                    && !schedule.retired
                    && schedule.next_due_at_unix_ms.is_some_and(|due| due <= now)
            });
        let keeper = self.clone().with_background_launches();
        if schedule_due {
            return keeper.cycle(selection, approval).await.map(|_| ());
        }
        let waiting = self
            .repository
            .history(project, &environment, None, None, 100)?
            .into_iter()
            .filter(|run| {
                !run.state.is_terminal()
                    && (run.state.is_active() || run.trigger != JobTrigger::BeforeDeploy)
            })
            .map(|run| run.id)
            .collect::<Vec<_>>();
        if waiting.is_empty() {
            return Ok(());
        }
        for run in waiting {
            keeper.advance(selection, &document, run, approval).await?;
        }
        Ok(())
    }
}
