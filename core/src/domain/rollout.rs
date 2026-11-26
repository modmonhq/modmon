//! How a deployment rolls out, and what a host remembers of it.
//!
//! A host's plan carries its rollout policy: how long a stack may take to
//! become healthy, the order of the stacks, and each service's `update:`
//! settings. For a cluster the client turns the nodes' policies into
//! stages, one per stack, each a list of node batches. The stages are part
//! of the reviewed cluster plan, so an apply rolls out exactly what the
//! plan showed.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::{
    DeployConfig, JobWhen, MmConfig, StackConfig, UpdateConfig, UpdateFailure, UpdateOrder,
};

#[path = "rollout/status.rs"]
mod status;
pub use status::{
    DeploymentRecord, DeploymentStatus, NodeDeploymentStatus, NodeRollout, NodeStatusRow,
    RollbackTarget, RolloutState, redeploy_target, rollback_target, utc_time,
};
#[path = "rollout/begun.rs"]
mod begun;
pub use begun::{BegunUnder, RolloutRequest};
#[path = "rollout/progress.rs"]
mod progress;
pub use progress::{ProgressStep, RolloutProgress};
#[path = "rollout/operations.rs"]
mod operations;
pub use operations::{
    CancelOutcome, CancelReport, CancelledRun, DaemonHealth, LateChild, NodeCancel,
    OperationInFlight, RECENT_START_MS, SHORT_ID, STUCK_AFTER_MS, daemon_diagnostics, short_id,
};
#[path = "rollout/auto_order.rs"]
mod auto_order;
pub use auto_order::{HostTies, starts_first_by_default};
#[path = "rollout/steps.rs"]
mod steps;
pub use steps::{
    DEFAULT_FAILURE_LOG_LINES, FAILURE_LINE_CHARS, FAILURE_LOG_BYTES, FAILURE_SUMMARY_LINES,
    LOCK_LEASE_MS, LOCK_RENEW_MS, MAX_FAILURE_LOG_LINES, RolloutAnswer, RolloutChanges,
    RolloutStep, StepFailure, failure_log_summary, kept_failure_log, unhealthy_code,
};

/// What a job that did not succeed in a rollout tells the client: the
/// stack it ran before, how it ended, and the last `lines` of its output.
#[must_use]
pub fn job_failure(
    stack: &str,
    run: &crate::domain::scheduler::JobRun,
    lines: usize,
) -> StepFailure {
    use crate::domain::scheduler::JobRunState;
    let ended = match (run.state, run.exit_code) {
        (JobRunState::TimedOut, _) => "ran past its timeout".to_owned(),
        (JobRunState::Cancelled, _) => "was cancelled".to_owned(),
        (_, Some(code)) => format!("exited with {code}"),
        (state, None) => format!("ended {}", format!("{state:?}").to_lowercase()),
    };
    StepFailure {
        stack: stack.to_owned(),
        service: None,
        reason: format!(
            "job '{}' {ended}, so the stack was not rolled out. `mm deploy logs` shows its output",
            run.job
        ),
        log: kept_failure_log(&run.output.lines().collect::<Vec<_>>(), lines),
        code: run.error_code.clone(),
    }
}

/// How long the client waits for every node to resolve a node's new
/// containers before it calls the node's step failed.
pub const DISCOVERY_PATIENCE: Duration = Duration::from_secs(30);

/// One service's `update:` settings, where they differ from the defaults.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceUpdate {
    pub stack: String,
    pub service: String,
    pub order: UpdateOrder,
    pub parallelism: u32,
    pub failure: UpdateFailure,
}

/// A job a deployment runs on this host before the job's service is
/// started or replaced on any node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeployJob {
    pub job: String,
    pub stack: String,
    pub service: String,
    /// When the deployment runs it.
    pub run: crate::domain::config::JobWhen,
    /// How long the job may run.
    pub timeout_seconds: u64,
}

/// A job in a cluster rollout: which node runs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageJob {
    pub job: String,
    pub node: String,
    pub timeout_seconds: u64,
}

/// How one host rolls out its plan. Hashed with the plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RolloutPolicy {
    /// How long a stack may take on this host to become healthy.
    pub convergence_timeout_seconds: u64,
    /// How long this host may take to pull a stack's or a job's images,
    /// when the environment sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_timeout_seconds: Option<u64>,
    /// Every stack of the configuration, in the order a deployment rolls
    /// them out: by name, except that a stack follows the stacks its
    /// `after:` lists.
    pub stack_order: Vec<String>,
    /// The services of this host whose `update:` differs from the defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceUpdate>,
    /// The jobs a deployment runs on this host: before their service is
    /// started or replaced, once it converged, or once per environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<DeployJob>,
    /// The services of this host whose `drain` differs from the default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drains: Vec<ServiceDrain>,
    /// The services of this host that start before the stack's
    /// `before-deploy` jobs, because a job's service depends on them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ahead: Vec<crate::domain::placement::PlacedService>,
    /// How many last lines of a failed container's log this host keeps,
    /// when the environment sets `deploy.failure-log-lines`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_log_lines: Option<u32>,
}

/// One service's `drain`, where it differs from the default.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceDrain {
    pub stack: String,
    pub service: String,
    pub milliseconds: u64,
}

impl RolloutPolicy {
    /// The policy of `environment` as `config` declares it. On a cluster
    /// node `config` is the node's view, so only its own services count.
    pub fn from_config(config: &MmConfig, environment: &str) -> Result<Self, String> {
        let deploy = config
            .environments
            .get(environment)
            .and_then(|environment| environment.deploy.as_ref());
        let timeout = DeployConfig::convergence_timeout(deploy).map_err(|problem| {
            format!("environments.{environment}.deploy.convergence-timeout {problem}")
        })?;
        let failure_log_lines = DeployConfig::failure_log_lines(deploy).map_err(|problem| {
            format!("environments.{environment}.deploy.failure-log-lines {problem}")
        })?;
        let pull = match deploy.and_then(|deploy| deploy.pull_timeout.as_ref()) {
            None => None,
            Some(_) => Some(
                DeployConfig::pull_timeout(deploy)
                    .map_err(|problem| {
                        format!("environments.{environment}.deploy.pull-timeout {problem}")
                    })?
                    .as_secs(),
            ),
        };
        let mut services = Vec::new();
        let mut drains = Vec::new();
        for (module_name, module) in &config.modules {
            for (name, service) in &module.services {
                if !service.is_active(environment) {
                    continue;
                }
                if let Some(spec) = &service.drain {
                    let drain =
                        crate::domain::leaving::drain_of(Some(spec)).map_err(|problem| {
                            format!("modules.{module_name}.services.{name}.drain {problem}")
                        })?;
                    drains.push(ServiceDrain {
                        stack: service.stack.clone(),
                        service: name.clone(),
                        milliseconds: u64::try_from(drain.as_millis()).unwrap_or(u64::MAX),
                    });
                }
                let Some(update) = &service.update else {
                    continue;
                };
                let update = service_update(&service.stack, name, update).map_err(|problem| {
                    format!("modules.{module_name}.services.{name}.update.{problem}")
                })?;
                if update.order != UpdateOrder::default()
                    || update.parallelism != 1
                    || update.failure != UpdateFailure::default()
                {
                    services.push(update);
                }
            }
        }
        services.sort();
        drains.sort();
        Ok(Self {
            convergence_timeout_seconds: timeout.as_secs().max(1),
            pull_timeout_seconds: pull,
            stack_order: stack_order(&config.stacks)?,
            services,
            jobs: deploy_jobs(config, environment)?,
            drains,
            ahead: Vec::new(),
            failure_log_lines: deploy
                .and_then(|deploy| deploy.failure_log_lines)
                .map(|_| failure_log_lines),
        })
    }

    /// How many last lines of a failed container's log this host keeps.
    #[must_use]
    pub fn failure_log_lines(&self) -> usize {
        usize::try_from(self.failure_log_lines.unwrap_or(DEFAULT_FAILURE_LOG_LINES))
            .unwrap_or(usize::MAX)
    }

    /// How long a container of `service` that leaves keeps serving once
    /// no node sends it new requests.
    #[must_use]
    pub fn drain(&self, stack: &str, service: &str) -> Duration {
        self.drains
            .iter()
            .find(|drain| drain.stack == stack && drain.service == service)
            .map_or(crate::domain::leaving::DEFAULT_DRAIN, |drain| {
                Duration::from_millis(drain.milliseconds)
            })
    }

    #[must_use]
    pub const fn convergence_timeout(&self) -> Duration {
        // A plan stored before rollouts existed carries no window.
        if self.convergence_timeout_seconds == 0 {
            return crate::domain::config::DEFAULT_CONVERGENCE_TIMEOUT;
        }
        Duration::from_secs(self.convergence_timeout_seconds)
    }

    #[must_use]
    pub fn pull_timeout(&self) -> Duration {
        pull_timeout(self.pull_timeout_seconds)
    }

    /// The services of `stack` whose new container starts before the old
    /// one stops.
    #[must_use]
    pub fn start_first(&self, stack: &str) -> Vec<String> {
        self.services
            .iter()
            .filter(|service| service.stack == stack && service.order == UpdateOrder::StartFirst)
            .map(|service| service.service.clone())
            .collect()
    }

    /// `stacks` in rollout order. A stack the policy does not list (a plan
    /// stored before rollouts existed) keeps its place after the listed
    /// ones, by name.
    #[must_use]
    pub fn ordered<'a, T>(&self, stacks: &'a [T], name: impl Fn(&T) -> &str) -> Vec<&'a T> {
        let position = |stack: &T| {
            self.stack_order
                .iter()
                .position(|listed| listed == name(stack))
                .unwrap_or(usize::MAX)
        };
        let mut ordered = stacks.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            position(left)
                .cmp(&position(right))
                .then_with(|| name(left).cmp(name(right)))
        });
        ordered
    }
}

/// The jobs of `environment` a deployment runs on this host, in the order
/// they run: before the stacks, after them, then the `once` ones.
fn deploy_jobs(config: &MmConfig, environment: &str) -> Result<Vec<DeployJob>, String> {
    let mut jobs = Vec::new();
    for (name, job) in &config.jobs {
        let Some(run) = job.run else {
            continue;
        };
        if job.placed_elsewhere.is_some() || !job.runs_in(environment) {
            continue;
        }
        let Some(service) = config
            .modules
            .get(&job.module)
            .and_then(|module| module.services.get(&job.service))
            .filter(|service| service.is_active(environment))
        else {
            continue;
        };
        let timeout = job
            .timeout
            .parse()
            .map_err(|problem| format!("jobs.{name}.timeout {problem}"))?;
        jobs.push(DeployJob {
            job: name.clone(),
            stack: service.stack.clone(),
            service: job.service.clone(),
            run,
            timeout_seconds: timeout.as_secs().max(1),
        });
    }
    jobs.sort_by(|left, right| {
        left.run
            .cmp(&right.run)
            .then_with(|| left.job.cmp(&right.job))
    });
    Ok(jobs)
}

fn service_update(
    stack: &str,
    service: &str,
    update: &UpdateConfig,
) -> Result<ServiceUpdate, String> {
    let parallelism = update.parallelism.unwrap_or(1);
    if parallelism == 0 {
        return Err("parallelism must be at least 1".to_owned());
    }
    Ok(ServiceUpdate {
        stack: stack.to_owned(),
        service: service.to_owned(),
        order: update.order.unwrap_or_default(),
        parallelism,
        failure: update.failure.unwrap_or_default(),
    })
}

/// The stacks in the order a deployment rolls them out: by name, except
/// that a stack comes after every stack its `after:` lists.
pub fn stack_order(stacks: &BTreeMap<String, StackConfig>) -> Result<Vec<String>, String> {
    for (name, stack) in stacks {
        for earlier in &stack.after {
            if earlier == name {
                return Err(format!(
                    "stacks.{name}.after lists the stack itself; remove '{name}' from it"
                ));
            }
            if !stacks.contains_key(earlier) {
                return Err(format!(
                    "stacks.{name}.after names '{earlier}', which is not a stack; the stacks are {}",
                    stacks.keys().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    let mut ordered = Vec::with_capacity(stacks.len());
    let mut placed = BTreeSet::new();
    while ordered.len() < stacks.len() {
        let next = stacks.iter().find(|(name, stack)| {
            !placed.contains(name.as_str())
                && stack
                    .after
                    .iter()
                    .all(|earlier| placed.contains(earlier.as_str()))
        });
        let Some((name, _)) = next else {
            let waiting = stacks
                .keys()
                .filter(|name| !placed.contains(name.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            return Err(format!(
                "stacks {} wait for each other through `after:`; remove one of those lines so one of them can go first",
                waiting.join(", ")
            ));
        };
        placed.insert(name.as_str());
        ordered.push(name.clone());
    }
    Ok(ordered)
}

/// One stack's turn in a cluster deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RolloutStage {
    pub stack: String,
    /// The nodes that run the stack, in the target's declared order, in
    /// batches: the nodes of a batch are replaced at the same time, and the
    /// next batch starts when every node of this one is healthy and the
    /// other nodes resolve its new containers.
    pub batches: Vec<Vec<String>>,
    /// Services whose new container starts before the old one stops.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start_first: Vec<String>,
    /// Services whose failure pauses the deployment instead of rolling it
    /// back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pause_on_failure: Vec<String>,
    /// The jobs that run, in this order, before any node of the stage is
    /// replaced; a job that fails stops the deployment there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<StageJob>,
    /// The nodes that start, before the jobs, the services the jobs'
    /// services depend on, in the target's order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ahead: Vec<String>,
}

impl RolloutStage {
    /// "app on app-1, then app-2 and app-3".
    #[must_use]
    pub fn describe(&self) -> String {
        let batches = self
            .batches
            .iter()
            .map(|batch| batch.join(" and "))
            .collect::<Vec<_>>();
        format!("{} on {}", self.stack, batches.join(", then "))
    }
}

/// The order a cluster deployment rolls out in. Part of the cluster plan's
/// hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClusterRollout {
    /// How long a stack may take on a node to become healthy.
    pub convergence_timeout_seconds: u64,
    /// How long a node may take to pull a stack's or a job's images, when
    /// a node's environment sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_timeout_seconds: Option<u64>,
    pub stages: Vec<RolloutStage>,
    /// The `once` jobs some node of the environment already ran or
    /// started: no node starts them again, and every node keeps the
    /// record.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ran_once: BTreeMap<String, crate::domain::scheduler::OnceRecord>,
    /// The `once` jobs this deployment starts on their nodes once it is in
    /// place: the ones no node ran before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub once: Vec<StageJob>,
}

fn pull_timeout(seconds: Option<u64>) -> Duration {
    seconds.map_or(
        crate::domain::config::DEFAULT_PULL_TIMEOUT,
        Duration::from_secs,
    )
}

/// One node's part of a cluster plan, as the rollout sees it: the stacks it
/// renders and its policy.
#[derive(Debug, Clone, Copy)]
pub struct NodeRolloutInput<'a> {
    pub node: &'a str,
    pub stacks: &'a [String],
    pub policy: &'a RolloutPolicy,
}

/// The jobs of `nodes`' policies that `keep` takes, each on its node.
fn placed_jobs<'a, 'b: 'a>(
    nodes: impl Iterator<Item = &'a NodeRolloutInput<'b>>,
    keep: impl Fn(&DeployJob) -> bool,
) -> Vec<StageJob> {
    nodes
        .flat_map(|node| {
            node.policy
                .jobs
                .iter()
                .filter(|job| keep(job))
                .map(|job| StageJob {
                    job: job.job.clone(),
                    node: node.node.to_owned(),
                    timeout_seconds: job.timeout_seconds,
                })
        })
        .collect()
}

impl ClusterRollout {
    /// The stages of a plan over `nodes`, which are in the target's
    /// declared order.
    #[must_use]
    pub fn of(nodes: &[NodeRolloutInput<'_>]) -> Self {
        let mut order = Vec::<&str>::new();
        for node in nodes {
            for stack in &node.policy.stack_order {
                if !order.contains(&stack.as_str()) {
                    order.push(stack);
                }
            }
        }
        // A stack no policy lists still rolls out, after the listed ones.
        let mut unlisted = nodes
            .iter()
            .flat_map(|node| node.stacks.iter().map(String::as_str))
            .filter(|stack| !order.contains(stack))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        order.append(&mut unlisted);
        let mut stages = Vec::new();
        for stack in order {
            let running = nodes
                .iter()
                .filter(|node| node.stacks.iter().any(|rendered| rendered == stack))
                .collect::<Vec<_>>();
            if running.is_empty() {
                continue;
            }
            let updates = running
                .iter()
                .flat_map(|node| &node.policy.services)
                .filter(|service| service.stack == stack)
                .collect::<Vec<_>>();
            let parallelism = updates
                .iter()
                .map(|service| service.parallelism)
                .min()
                .unwrap_or(1)
                .max(1) as usize;
            let names = |keep: &dyn Fn(&ServiceUpdate) -> bool| {
                updates
                    .iter()
                    .filter(|service| keep(service))
                    .map(|service| service.service.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            };
            stages.push(RolloutStage {
                stack: stack.to_owned(),
                batches: running
                    .chunks(parallelism)
                    .map(|batch| batch.iter().map(|node| node.node.to_owned()).collect())
                    .collect(),
                start_first: names(&|service| service.order == UpdateOrder::StartFirst),
                pause_on_failure: names(&|service| service.failure == UpdateFailure::Pause),
                jobs: placed_jobs(running.iter().copied(), |job| {
                    job.stack == stack && job.run == JobWhen::BeforeDeploy
                }),
                ahead: running
                    .iter()
                    .filter(|node| node.policy.ahead.iter().any(|placed| placed.stack == stack))
                    .map(|node| node.node.to_owned())
                    .collect(),
            });
        }
        Self {
            convergence_timeout_seconds: nodes
                .iter()
                .map(|node| node.policy.convergence_timeout().as_secs())
                .max()
                .unwrap_or_default(),
            pull_timeout_seconds: nodes
                .iter()
                .any(|node| node.policy.pull_timeout_seconds.is_some())
                .then(|| {
                    nodes
                        .iter()
                        .map(|node| node.policy.pull_timeout().as_secs())
                        .max()
                        .unwrap_or_default()
                }),
            stages,
            ran_once: BTreeMap::new(),
            once: placed_jobs(nodes.iter(), |job| job.run == JobWhen::Once),
        }
    }

    #[must_use]
    pub fn pull_timeout(&self) -> Duration {
        pull_timeout(self.pull_timeout_seconds)
    }

    /// Takes in what the nodes say ran once: those jobs are not started
    /// again.
    pub fn knowing_ran_once(
        mut self,
        records: impl IntoIterator<Item = (String, crate::domain::scheduler::OnceRecord)>,
    ) -> Self {
        for (job, record) in records {
            self.ran_once.entry(job).or_insert(record);
        }
        let ran = &self.ran_once;
        self.once.retain(|due| !ran.contains_key(&due.job));
        self
    }

    /// Whether a failure of `service` in `stack` pauses the deployment.
    #[must_use]
    pub fn pauses(&self, stack: &str, service: Option<&str>) -> bool {
        let Some(service) = service else {
            return false;
        };
        self.stages
            .iter()
            .filter(|stage| stage.stack == stack)
            .any(|stage| stage.pause_on_failure.iter().any(|name| name == service))
    }

    /// Checks the stages against what a node's own plan renders: every
    /// stack of the node has a stage that lists the node, no stage lists
    /// the node for a stack it does not render, and the jobs the stages
    /// give the node are the ones its own plan runs.
    pub fn check_node(
        &self,
        node: &str,
        stacks: &[String],
        jobs: &[DeployJob],
    ) -> Result<(), String> {
        let given = self
            .stages
            .iter()
            .flat_map(|stage| {
                stage
                    .jobs
                    .iter()
                    .filter(|job| job.node == node)
                    .map(|job| (stage.stack.as_str(), job.job.as_str()))
            })
            .collect::<BTreeSet<_>>();
        let own = jobs
            .iter()
            .filter(|job| job.run == JobWhen::BeforeDeploy)
            .map(|job| (job.stack.as_str(), job.job.as_str()))
            .collect::<BTreeSet<_>>();
        if given != own {
            return Err(format!(
                "the cluster plan's rollout runs other jobs on node '{node}' than its own plan does"
            ));
        }
        for stack in stacks {
            let listed = self.stages.iter().any(|stage| {
                &stage.stack == stack && stage.batches.iter().flatten().any(|name| name == node)
            });
            if !listed {
                return Err(format!(
                    "the cluster plan's rollout leaves out stack '{stack}' on node '{node}'"
                ));
            }
        }
        for stage in &self.stages {
            let lists = stage.batches.iter().flatten().any(|name| name == node);
            if lists && !stacks.contains(&stage.stack) {
                return Err(format!(
                    "the cluster plan's rollout has stack '{}' on node '{node}', which does not run it",
                    stage.stack
                ));
            }
        }
        Ok(())
    }
}
