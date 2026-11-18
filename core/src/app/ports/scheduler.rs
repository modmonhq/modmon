use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::error::MmError;
use crate::domain::scheduler::{
    ClaimedJobRun, ClaimedSchedule, DesiredDefinitions, JobAuditRecord, JobLaunch, JobRun,
    JobRunCompletion, NewJobRun, ReconciliationCommit, RuntimeJobState, ScheduleRecord,
    ScheduleTickCommit, StoredDefinitionHashes,
};
use crate::domain::workspace::ProjectId;

pub trait SchedulerRepository: Send + Sync {
    fn definition_hashes(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<StoredDefinitionHashes, MmError>;

    fn reconcile(&self, commit: &ReconciliationCommit) -> Result<(), MmError>;

    fn list_jobs(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<DesiredDefinitions, MmError>;

    fn list_schedules(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Vec<ScheduleRecord>, MmError>;

    fn set_schedule_enabled(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
        enabled: bool,
        next_due_at_unix_ms: Option<i64>,
        audit: &JobAuditRecord,
    ) -> Result<ScheduleRecord, MmError>;

    fn insert_run(&self, run: &NewJobRun, audit: &JobAuditRecord) -> Result<JobRun, MmError>;

    fn run(&self, run_id: Uuid) -> Result<Option<JobRun>, MmError>;

    fn history(
        &self,
        project_id: ProjectId,
        environment: &str,
        job: Option<&str>,
        schedule: Option<&str>,
        limit: usize,
    ) -> Result<Vec<JobRun>, MmError>;

    fn request_cancel(&self, run_id: Uuid, audit: &JobAuditRecord) -> Result<JobRun, MmError>;

    fn claim_due_schedules(
        &self,
        project_id: ProjectId,
        environment: &str,
        owner: Uuid,
        now_unix_ms: i64,
        lease: Duration,
        limit: usize,
    ) -> Result<Vec<ClaimedSchedule>, MmError>;

    fn commit_schedule_tick(&self, commit: &ScheduleTickCommit) -> Result<Vec<JobRun>, MmError>;

    fn claim_runnable_run(
        &self,
        project_id: ProjectId,
        environment: &str,
        owner: Uuid,
        now_unix_ms: i64,
        lease: Duration,
    ) -> Result<Option<ClaimedJobRun>, MmError>;

    /// Claims run `run_id` when it can be worked on now: queued and due,
    /// or active with no live lease. `None` when another worker holds it
    /// or there is nothing to do.
    fn claim_run(
        &self,
        run_id: Uuid,
        owner: Uuid,
        now_unix_ms: i64,
        lease: Duration,
    ) -> Result<Option<ClaimedJobRun>, MmError>;

    fn mark_run_running(
        &self,
        claim: &ClaimedJobRun,
        runtime_id: &str,
        started_at_unix_ms: i64,
    ) -> Result<JobRun, MmError>;

    fn complete_run(&self, completion: &JobRunCompletion) -> Result<JobRun, MmError>;

    fn schedule_retry(
        &self,
        claim: &ClaimedJobRun,
        next_attempt_at_unix_ms: i64,
        error_code: &str,
    ) -> Result<JobRun, MmError>;

    fn mark_run_unknown(
        &self,
        claim: &ClaimedJobRun,
        diagnostic_code: &str,
    ) -> Result<JobRun, MmError>;

    fn append_job_audit(&self, audit: &JobAuditRecord) -> Result<(), MmError>;

    fn prune_history(
        &self,
        project_id: ProjectId,
        environment: &str,
        job: &str,
        retain: usize,
    ) -> Result<usize, MmError>;
}

#[async_trait]
pub trait RuntimeJobDriver: Send + Sync {
    async fn launch(&self, launch: JobLaunch) -> Result<String, MmError>;
    async fn inspect(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        runtime_name: &str,
        runtime_id: Option<&str>,
        output_limit_bytes: usize,
        redactions: Vec<zeroize::Zeroizing<String>>,
    ) -> Result<RuntimeJobState, MmError>;
    async fn cancel(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        runtime_name: &str,
        runtime_id: Option<&str>,
    ) -> Result<(), MmError>;
    async fn cleanup(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        runtime_name: &str,
        runtime_id: Option<&str>,
    ) -> Result<(), MmError>;
    /// The names of the job containers in the target's Compose project,
    /// running or not.
    async fn job_containers(
        &self,
        _target: &crate::domain::runtime::RuntimeTarget,
    ) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }
}

/// Where a job's container is created and with what: the Compose files,
/// environment, and secrets the environment's last deployment gave the
/// job's service, so a job gets everything its service gets.
pub struct PreparedJobTarget {
    pub target: crate::domain::runtime::RuntimeTarget,
    /// Removes what was staged for the container once it exists.
    pub release: Box<dyn FnOnce() -> Result<(), MmError> + Send>,
}

#[async_trait]
pub trait JobLaunchTargets: Send + Sync {
    /// The target for run `run` of a job of stack `stack`; `None` when the
    /// environment has no deployment on this host to take it from.
    async fn prepare(
        &self,
        selection: &crate::app::WorkspaceSelection,
        stack: &str,
        run: Uuid,
    ) -> Result<Option<PreparedJobTarget>, MmError>;
}

/// The jobs a deployment runs, for the deployment service.
#[async_trait]
pub trait DeploymentJobs: Send + Sync {
    /// Runs declared job `job` to its end in `target`, as part of the
    /// deployment of plan `plan_hash`.
    async fn run(
        &self,
        selection: &crate::app::WorkspaceSelection,
        job: &str,
        target: crate::domain::runtime::RuntimeTarget,
        plan_hash: &str,
    ) -> Result<crate::domain::scheduler::JobRun, MmError>;

    /// Removes the job containers in the Compose projects of `projects`
    /// that no recorded run still waits for, after moving on the runs
    /// that were active. Returns the names it removed.
    async fn sweep(
        &self,
        selection: &crate::app::WorkspaceSelection,
        projects: &[crate::domain::runtime::RuntimeTarget],
    ) -> Result<Vec<String>, MmError>;
}
