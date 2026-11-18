use std::time::Duration;

use crate::app::{JobSchedulerService, RemoteEnvironment, WorkspaceSelection};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::observability::ApprovalContext;
use crate::domain::scheduler::{JobRun, JobRunRequest, JobRunsRequest, JobTrigger};

/// How the record of a run a deployment started begins its `source`.
const DEPLOYMENT_SOURCE: &str = "deployment ";

/// How long a host may take to start a job's container and answer.
const LAUNCH_PATIENCE: Duration = Duration::from_secs(5 * 60);

/// The declared jobs of one selected environment, whether it runs on this
/// machine or on remote hosts. The command line, the terminal UI, and the
/// MCP server all run and read jobs through it.
pub enum JobGateway {
    Local {
        scheduler: Box<JobSchedulerService>,
        selection: Box<WorkspaceSelection>,
    },
    Remote {
        environment: Box<RemoteEnvironment>,
        /// This checkout's configuration: which jobs exist, and which
        /// service each runs as.
        document: Box<ConfigDocument>,
    },
}

impl JobGateway {
    /// Runs a declared job in a fresh container of its service's image.
    /// With `wait`, answers when the run ended (after any retries);
    /// otherwise once it started.
    pub async fn run(
        &self,
        request: JobRunRequest,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        match self {
            Self::Local {
                scheduler,
                selection,
            } => scheduler.run_job(selection, &request, approval).await,
            Self::Remote {
                environment,
                document,
            } => {
                let job = document
                    .config
                    .jobs
                    .get(&request.job)
                    .ok_or_else(|| undeclared(document, &request.job))?;
                let patience = if request.wait {
                    let timeout = job
                        .timeout
                        .parse()
                        .map_err(|message| MmError::Environment { message })?;
                    timeout
                        .saturating_mul(u32::from(job.retry.attempts.max(1)))
                        .saturating_add(LAUNCH_PATIENCE)
                } else {
                    LAUNCH_PATIENCE
                };
                let node = crate::domain::placement::job_node_on(
                    &document.config,
                    environment.target(),
                    &request.job,
                )
                .map_err(|message| MmError::Placement { message })?;
                environment
                    .run_job(node.as_deref(), request, patience)
                    .await
            }
        }
    }

    /// The recorded runs, newest first. A project that declares no jobs
    /// has none, and no host is asked.
    pub async fn runs(
        &self,
        request: JobRunsRequest,
        approval: &ApprovalContext,
    ) -> Result<Vec<JobRun>, MmError> {
        match self {
            Self::Local {
                scheduler,
                selection,
            } => scheduler.runs(selection, &request, approval).await,
            Self::Remote {
                environment,
                document,
            } => {
                if document.config.jobs.is_empty() {
                    return Ok(Vec::new());
                }
                environment.job_runs(request).await
            }
        }
    }

    /// The job runs of the newest deployment that ran any, in the order
    /// they ran. Empty when no deployment ran a job.
    pub async fn deployment_runs(
        &self,
        approval: &ApprovalContext,
    ) -> Result<Vec<JobRun>, MmError> {
        let request = JobRunsRequest {
            job: None,
            run: None,
            limit: crate::domain::scheduler::MAX_LISTED_RUNS,
        };
        let runs = self.runs(request, approval).await?;
        let Some(newest) = runs
            .iter()
            .find(|run| run.source.starts_with(DEPLOYMENT_SOURCE))
            .map(|run| run.source.clone())
        else {
            return Ok(Vec::new());
        };
        let mut runs = runs
            .into_iter()
            .filter(|run| run.source == newest)
            .collect::<Vec<_>>();
        runs.reverse();
        Ok(runs)
    }

    /// After a deployment on this machine: its `after-deploy` jobs, and
    /// its `once` jobs that never ran in the environment (started, not
    /// waited for). A remote host does this itself, as its deployment ends.
    pub async fn after_local_deployment(
        &self,
        (plan_hash, jobs): (&str, &[crate::domain::rollout::DeployJob]),
        approval: &ApprovalContext,
    ) -> Result<Vec<crate::domain::scheduler::JobOutcome>, MmError> {
        use crate::domain::config::JobWhen;
        let ran = self
            .runs(
                JobRunsRequest {
                    job: None,
                    run: None,
                    limit: crate::domain::scheduler::MAX_LISTED_RUNS,
                },
                approval,
            )
            .await?
            .into_iter()
            .filter(|run| run.trigger == JobTrigger::Once)
            .map(|run| run.job)
            .collect::<std::collections::BTreeSet<_>>();
        let mut outcomes = Vec::new();
        for job in jobs.iter().filter(|job| job.run != JobWhen::BeforeDeploy) {
            let once = job.run == JobWhen::Once;
            if once && ran.contains(&job.job) {
                continue;
            }
            let request = JobRunRequest {
                job: job.job.clone(),
                wait: !once,
                trigger: if once {
                    JobTrigger::Once
                } else {
                    JobTrigger::AfterDeploy
                },
                deployment: Some(plan_hash.to_owned()),
                run: None,
            };
            outcomes.push(match self.run(request, approval).await {
                Ok(run) => crate::domain::scheduler::JobOutcome::of(job.run, &run, None),
                Err(error) => crate::domain::scheduler::JobOutcome::not_started(
                    &job.job,
                    job.run,
                    None,
                    error.to_string(),
                ),
            });
        }
        Ok(outcomes)
    }

    /// One recorded run.
    pub async fn run_status(
        &self,
        run: uuid::Uuid,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let request = JobRunsRequest {
            job: None,
            run: Some(run),
            limit: 1,
        };
        self.runs(request, approval)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| MmError::Environment {
                message: format!(
                    "no host of this environment recorded job run '{run}'; `mm job history` lists the runs"
                ),
            })
    }

    /// What stopping to wait for `run` means: `None` for a run on this
    /// machine, which is cancelled; for a run on a remote host, the note
    /// that it goes on there. Not an error: nothing failed.
    #[must_use]
    pub fn left_running(&self, run: uuid::Uuid) -> Option<String> {
        match self {
            Self::Local { .. } => None,
            Self::Remote { .. } => Some(format!(
                "job run '{run}' goes on on the remote host: mm does not stop a run there, and it ends at its job's timeout (jobs.<name>.timeout). `mm job status {run}` follows it"
            )),
        }
    }

    /// Asks a run on this machine to stop. A run on a remote host ends at
    /// its job's timeout.
    pub fn cancel(&self, run: uuid::Uuid, approval: &ApprovalContext) -> Result<JobRun, MmError> {
        match self {
            Self::Local {
                scheduler,
                selection,
            } => scheduler.cancel_run(selection, run, approval),
            Self::Remote { .. } => Err(MmError::Environment {
                message: format!(
                    "job run '{run}' runs on a remote host, where mm does not stop a run; it ends at its job's timeout (jobs.<name>.timeout). `mm job status {run}` follows it"
                ),
            }),
        }
    }

    /// Runs the job of an earlier run again, from its current definition.
    pub async fn retry(
        &self,
        run: uuid::Uuid,
        wait: bool,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        if let Self::Local {
            scheduler,
            selection,
        } = self
        {
            let queued = scheduler.retry_run(selection, run, approval)?;
            return scheduler
                .follow_run(selection, queued.id, wait, approval)
                .await;
        }
        let previous = self.run_status(run, approval).await?;
        if !previous.state.is_terminal() {
            return Err(MmError::Environment {
                message: format!(
                    "job run '{run}' has not ended; only a run that ended can be run again"
                ),
            });
        }
        let request = JobRunRequest {
            job: previous.job,
            wait,
            trigger: JobTrigger::Retry,
            deployment: None,
            run: None,
        };
        self.run(request, approval).await
    }
}

fn undeclared(document: &ConfigDocument, job: &str) -> MmError {
    let declared = document
        .config
        .jobs
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    MmError::Environment {
        message: crate::domain::scheduler::undeclared_job(&declared, job),
    }
}
