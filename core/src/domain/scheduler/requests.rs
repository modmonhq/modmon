//! What a client asks the host that runs an environment about its jobs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::JobTrigger;

/// Most runs one answer lists.
pub const MAX_LISTED_RUNS: u32 = 1_000;

/// Runs one job the environment's configuration declares, in a fresh
/// container of its service's image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRunRequest {
    pub job: String,
    /// Answer when the run has ended; otherwise as soon as it started.
    pub wait: bool,
    /// Why the job runs: by hand, or again after a run that failed.
    pub trigger: JobTrigger,
    /// The deployment plan that runs the job, for its record. Never on the
    /// wire: only a host that deploys sets it.
    #[serde(skip)]
    #[schemars(skip)]
    pub deployment: Option<String>,
    /// The run's ID, when the host that starts the run chooses it: the ID
    /// of the request that asked for it, so the operation in flight and the
    /// run share one ID and `mm deploy cancel` stops both. Never on the
    /// wire.
    #[serde(skip)]
    #[schemars(skip)]
    pub run: Option<Uuid>,
}

impl JobRunRequest {
    /// A run of `job` asked for by hand.
    #[must_use]
    pub fn manual(job: &str, wait: bool) -> Self {
        Self {
            job: job.to_owned(),
            wait,
            trigger: JobTrigger::Manual,
            deployment: None,
            run: None,
        }
    }
}

/// The job runs an operation on a host started, which `mm deploy cancel`
/// stops with it: the run a request asked for by hand (its ID is the
/// request's), or the runs a deployment started before or after it changed
/// the host. A `once` job a deployment started runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartedRuns {
    Run(Uuid),
    Deployment(String),
    /// A run named by its ID or its first six or more characters.
    Named(String),
}

impl StartedRuns {
    /// Whether `run` is one of them.
    #[must_use]
    pub fn contain(&self, run: &super::JobRun) -> bool {
        match self {
            Self::Run(id) => run.id == *id,
            Self::Named(wanted) => {
                wanted.len() >= 6 && run.id.to_string().starts_with(wanted.as_str())
            }
            Self::Deployment(plan_hash) => {
                matches!(
                    run.trigger,
                    JobTrigger::BeforeDeploy | JobTrigger::AfterDeploy
                ) && run.source
                    == format!("deployment {}", plan_hash.get(..12).unwrap_or(plan_hash))
            }
        }
    }
}

/// A running job run as an operation `mm deploy status` lists.
#[must_use]
pub fn run_in_flight(
    run: &super::JobRun,
    timeout: std::time::Duration,
    now_unix_ms: i64,
) -> crate::domain::rollout::OperationInFlight {
    let started = run.started_at_unix_ms.unwrap_or(run.requested_at_unix_ms);
    crate::domain::rollout::OperationInFlight {
        id: run.id.to_string(),
        what: format!("run of job '{}'", run.job),
        principal: run.principal.clone(),
        started_at_unix_ms: started,
        alive_at_unix_ms: now_unix_ms,
        deadline_unix_ms: started
            .saturating_add(i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX)),
        stopping: (run.cancellation_requested || run.state == super::JobRunState::Cancelling)
            .then(|| "being cancelled".to_owned()),
        doing: None,
    }
}

/// Whether `run` is one `mm deploy status` lists: started and not ended.
#[must_use]
pub const fn runs_now(run: &super::JobRun) -> bool {
    matches!(
        run.state,
        super::JobRunState::Launching
            | super::JobRunState::Running
            | super::JobRunState::Cancelling
    )
}

impl super::JobRun {
    /// Keeps the last `bytes` of the output at most, cut at a character,
    /// and says so in `output_truncated` when anything was cut.
    pub fn keep_output_end(&mut self, bytes: usize) {
        let Some(mut start) = self
            .output
            .len()
            .checked_sub(bytes)
            .filter(|start| *start > 0)
        else {
            return;
        };
        while !self.output.is_char_boundary(start) {
            start += 1;
        }
        self.output.drain(..start);
        self.output_truncated = true;
    }
}

/// The plan a deployment files its job runs under: on a cluster the
/// cluster plan that every node rolls out, so `mm deploy logs` finds the
/// runs of one deployment on all nodes together; on one host its own plan.
/// Each node of a cluster has a plan of its own (its section), whose hash
/// differs from node to node.
#[must_use]
pub fn deployment_label<'a>(cluster_plan: Option<&'a str>, node_plan: &'a str) -> &'a str {
    cluster_plan.unwrap_or(node_plan)
}

/// Which recorded runs to list, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRunsRequest {
    /// Only the runs of this job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
    /// Only this run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Uuid>,
    /// At most this many; capped at [`MAX_LISTED_RUNS`].
    pub limit: u32,
}

/// Why `job` cannot run: the configuration, whose jobs are `declared`,
/// does not declare it.
#[must_use]
pub fn undeclared_job(declared: &[&str], job: &str) -> String {
    if declared.is_empty() {
        return format!(
            "job '{job}' is not declared: the configuration has no jobs:. Declare it under jobs: in mm.yaml"
        );
    }
    format!(
        "job '{job}' is not declared; the declared jobs are {}",
        declared.join(", ")
    )
}

/// Why a schedule of a remote environment is not reconciled, toggled, or
/// started from this machine: its deployment reconciles the schedules on
/// the host, which runs them on the job's node.
#[must_use]
pub fn remote_schedules(environment: &str) -> crate::domain::error::MmError {
    crate::domain::error::MmError::Remote {
        code: "MM_REMOTE_SCHEDULES",
        message: format!(
            "the schedules of '{environment}' are reconciled by its deployment and run by mmd on each job's node; change them in mm.yaml and deploy (`mm deploy plan --environment {environment}`). `mm job history` shows their runs"
        ),
    }
    .in_own_words()
}

impl OnceRecord {
    /// The record of a `once` job a node starts in a deployment, filed
    /// like the deployment's runs: under the cluster plan on a cluster
    /// node, so every node's record names the same deployment
    /// ([`deployment_label`]), and under its own plan on one host.
    #[must_use]
    pub fn started(
        node: Option<String>,
        run: Option<Uuid>,
        (cluster_plan, node_plan): (Option<&str>, &str),
        at_unix_ms: i64,
    ) -> Self {
        Self {
            node,
            run,
            plan_hash: deployment_label(cluster_plan, node_plan).to_owned(),
            at_unix_ms,
        }
    }
}

/// The record every host of an environment keeps of a `once` job: which
/// node started it, as part of which deployment, and its run there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OnceRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Uuid>,
    pub plan_hash: String,
    pub at_unix_ms: i64,
}

/// A job a deployment ran or started once it was in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobOutcome {
    pub job: String,
    pub run: crate::domain::config::JobWhen,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// `None` when the job could not start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<super::JobRunState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Why it could not start, or its last lines when it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

impl JobOutcome {
    #[must_use]
    pub fn of(
        run: crate::domain::config::JobWhen,
        record: &super::JobRun,
        node: Option<String>,
    ) -> Self {
        let ended_badly =
            record.state.is_terminal() && record.state != super::JobRunState::Succeeded;
        Self {
            job: record.job.clone(),
            run,
            node,
            run_id: Some(record.id),
            state: Some(record.state),
            exit_code: record.exit_code,
            problem: ended_badly.then(|| last_lines(&record.output)),
        }
    }

    #[must_use]
    pub fn not_started(
        job: &str,
        run: crate::domain::config::JobWhen,
        node: Option<String>,
        problem: String,
    ) -> Self {
        Self {
            job: job.to_owned(),
            run,
            node,
            run_id: None,
            state: None,
            exit_code: None,
            problem: Some(problem),
        }
    }

    /// Whether the job did what it should: succeeded, or (a `once` job)
    /// started.
    #[must_use]
    pub fn went_well(&self) -> bool {
        match self.state {
            Some(super::JobRunState::Succeeded) => true,
            Some(state) => self.run == crate::domain::config::JobWhen::Once && !state.is_terminal(),
            None => false,
        }
    }
}

fn last_lines(output: &str) -> String {
    let lines = output.lines().collect::<Vec<_>>();
    lines[lines.len().saturating_sub(5)..].join(" | ")
}
