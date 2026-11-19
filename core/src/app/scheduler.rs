use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::app::ports::{
    Clock, JobLaunchTargets, RuntimeJobDriver, RuntimeOperationAuthorizer, SchedulerRepository,
    SecretMaterialResolver,
};
use crate::app::{Application, WorkspaceSelection};
use crate::domain::config::{ConfigDocument, JobConfig, ScheduleConfig};
use crate::domain::error::MmError;
use crate::domain::observability::{ApprovalContext, AuthorizationRequest, RuntimeOperation};
use crate::domain::scheduler::{
    ClaimedJobRun, DesiredDefinitions, JobAuditRecord, JobLaunch, JobRun, JobRunCompletion,
    JobRunState, JobTrigger, NewJobRun, ReconciliationCommit, ReconciliationPlan, RuntimeJobState,
    ScheduleRecord, ScheduleTickCommit, due_occurrences, next_occurrence, reconciliation_plan,
    retry_delay,
};

const SCHEDULER_LEASE: Duration = Duration::from_secs(30);
const MAX_SCHEDULES_PER_CYCLE: usize = 100;
const MAX_RUNS_PER_CYCLE: usize = 100;

mod definitions;
pub(crate) use definitions::desired_definitions;
use definitions::*;

#[derive(Clone)]
pub struct JobSchedulerService {
    application: Application,
    repository: Arc<dyn SchedulerRepository>,
    runtime: Arc<dyn RuntimeJobDriver>,
    secrets: Arc<dyn SecretMaterialResolver>,
    authorizer: Arc<dyn RuntimeOperationAuthorizer>,
    clock: Arc<dyn Clock>,
    /// Where a job's container gets its files, environment, and secrets:
    /// from the last deployment, when there is one.
    launch_targets: Option<Arc<dyn JobLaunchTargets>>,
}

pub struct JobSchedulerDependencies {
    pub application: Application,
    pub repository: Arc<dyn SchedulerRepository>,
    pub runtime: Arc<dyn RuntimeJobDriver>,
    pub secrets: Arc<dyn SecretMaterialResolver>,
    pub authorizer: Arc<dyn RuntimeOperationAuthorizer>,
    pub clock: Arc<dyn Clock>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct SchedulerCycleReport {
    pub schedules_claimed: usize,
    pub runs_enqueued: usize,
    pub runs_processed: usize,
    pub unknown_runs: usize,
}

struct RunCompletion<'a> {
    state: JobRunState,
    exit_code: Option<i32>,
    error_code: Option<&'a str>,
    diagnostic_code: Option<String>,
    output: String,
    output_truncated: bool,
}

struct NewRunInput<'a> {
    /// The run's ID when its caller chose it; a new one otherwise.
    id: Option<Uuid>,
    document: &'a ConfigDocument,
    environment: &'a str,
    job_name: &'a str,
    job: JobConfig,
    schedule: Option<(String, ScheduleConfig)>,
    trigger: JobTrigger,
    retry_of: Option<Uuid>,
    approval: &'a ApprovalContext,
}

mod manager;
mod runs;

#[cfg(test)]
mod tests;
