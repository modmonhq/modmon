//! The steps of a rollout on one host, as the client asks for them.

use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::deployment::DeploymentResult;

/// How long a host keeps an environment's deployment lock after it last
/// heard from the client that holds it.
pub const LOCK_LEASE_MS: i64 = 60_000;
/// How often a client that deploys says it is still there.
pub const LOCK_RENEW_MS: u64 = 15_000;

/// One step of a rollout on one host. The client decides the order across
/// hosts; a host only ever acts on its own engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case", deny_unknown_fields)]
pub enum RolloutStep {
    /// Takes the environment's deployment lock for this plan, or says the
    /// client that holds it is still there. Changes nothing else.
    Hold,
    /// Stores the addresses the plan gives the cluster's networks and
    /// brings the host's routes and firewall rules to them. Every node of
    /// a plan does this before any node is replaced, so a container that
    /// starts on a new network is reachable and resolvable from the nodes
    /// whose turn has not come. No container changes.
    Networks,
    /// Makes the plan's shared volumes ready: a node that mounts one
    /// checks that it reaches the storage node, and the storage node
    /// creates each volume's directory. The client asks the mounting
    /// nodes first, so a deployment that cannot go through creates
    /// nothing, and every node before any container changes, so no node
    /// mounts a directory that is not there. Nothing is ever removed.
    Storage,
    /// Claims the host's section of the plan and prepares it: access,
    /// network addresses, networks, and volumes. No container changes.
    Begin,
    /// Starts the new containers of the stack's `start-first` services
    /// next to the old ones and waits until they are healthy.
    Start { stack: String },
    /// Stops the old containers of `start-first` services that `Start`
    /// replaced, applies the stack, and waits until it is healthy.
    Apply { stack: String },
    /// Starts, or replaces, the services of `stack` on this host that the
    /// stack's `before-deploy` jobs' services depend on, and waits until
    /// they are healthy. The rest of the stack waits for the jobs.
    Ahead { stack: String },
    /// Runs job `job` of `stack` to its end, in a fresh container with
    /// what the deployment gives the job's service, before the stack's
    /// service is started or replaced on any node. No container of a
    /// service changes.
    Job { stack: String, job: String },
    /// Every stack is healthy on every host: removes what the plan no
    /// longer runs, records the deployment, and gives up the lock.
    Finish,
    /// Returns the host to what it ran before `Begin` and gives up the
    /// lock.
    Rollback,
    /// Gives up the lock and leaves the host as it is.
    Release,
}

impl RolloutStep {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Networks => "networks",
            Self::Storage => "storage",
            Self::Begin => "begin",
            Self::Start { .. } => "start",
            Self::Apply { .. } => "apply",
            Self::Job { .. } => "job",
            Self::Ahead { .. } => "ahead",
            Self::Finish => "finish",
            Self::Rollback => "rollback",
            Self::Release => "release",
        }
    }

    /// Whether the step can change what the host runs or enforces. The
    /// others only touch the lock.
    #[must_use]
    pub const fn changes_host(&self) -> bool {
        !matches!(self, Self::Hold | Self::Release)
    }
}

/// What a rollout's steps changed on a host so far. Undoing the rollout
/// puts back only this: a stack a `start` or `apply` step began, and the
/// `ahead` services of a stack whose jobs had them started. A stack no
/// step reached is left alone, since its configuration is newer than what
/// runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolloutChanges {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stacks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ahead: Vec<String>,
}

impl RolloutChanges {
    /// Notes, before it runs, that a step changes `stack`'s containers:
    /// only its `ahead` services, or the stack itself.
    pub fn note(&mut self, stack: &str, ahead_only: bool) {
        let list = if ahead_only {
            &mut self.ahead
        } else {
            &mut self.stacks
        };
        if !list.iter().any(|noted| noted == stack) {
            list.push(stack.to_owned());
        }
    }

    /// Whether a step began `stack` as a whole.
    #[must_use]
    pub fn whole(&self, stack: &str) -> bool {
        self.stacks.iter().any(|noted| noted == stack)
    }

    /// Whether only `stack`'s `ahead` services were started.
    #[must_use]
    pub fn ahead_only(&self, stack: &str) -> bool {
        !self.whole(stack) && self.ahead.iter().any(|noted| noted == stack)
    }
}

/// Why a stack did not become healthy on a host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepFailure {
    pub stack: String,
    /// The service that failed, when the host could tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    pub reason: String,
    /// The last lines the failed service's container logged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
    /// The error code behind it, when the step failed with an error
    /// rather than an unhealthy service: `MM_TIMEOUT` for a step that ran
    /// out of time, for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl std::fmt::Display for StepFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.service, self.stack.is_empty()) {
            (Some(service), _) => write!(
                formatter,
                "service '{service}' of stack '{}': {}",
                self.stack, self.reason
            )?,
            (None, false) => write!(formatter, "stack '{}': {}", self.stack, self.reason)?,
            (None, true) => write!(formatter, "{}", self.reason)?,
        }
        // The code says the same to a script or an agent: why a service
        // did not become healthy (`MM_RUNTIME_EXITED`), or what stopped
        // the step (`MM_TIMEOUT`).
        match &self.code {
            Some(code) if !self.reason.contains(code.as_str()) => write!(formatter, " ({code})"),
            _ => Ok(()),
        }
    }
}

/// The code a deployment that did not become healthy fails with: the
/// runtime's own reason when it gave one, else `MM_RUNTIME_RESPONSE`.
#[must_use]
pub fn unhealthy_code(failure: &StepFailure) -> &'static str {
    const RUNTIME: [&str; 5] = [
        "MM_RUNTIME_EXITED",
        "MM_RUNTIME_RESTARTING",
        "MM_RUNTIME_UNHEALTHY",
        "MM_RUNTIME_CONTAINER_FAILED",
        "MM_CONVERGENCE_TIMEOUT",
    ];
    failure
        .code
        .as_deref()
        .and_then(|code| RUNTIME.iter().find(|known| **known == code))
        .copied()
        .unwrap_or("MM_RUNTIME_RESPONSE")
}

impl StepFailure {
    /// The failure with the log lines behind it, on one line: for an error
    /// message, which has no room for a list.
    #[must_use]
    pub fn with_log(&self) -> String {
        if self.log.is_empty() {
            return self.to_string();
        }
        format!("{self}. Its last log lines: {}", self.log.join(" | "))
    }
}

/// What a host did for one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum RolloutAnswer {
    Held,
    /// The plan's network addresses are stored and in force on the host.
    Addressed,
    /// The plan's shared volumes are ready for this host: `created` are
    /// the directories the storage node made that were not there.
    StorageReady {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        created: Vec<String>,
    },
    Begun,
    Started {
        /// The services whose new containers now run next to the old ones.
        services: Vec<String>,
        /// The new containers' addresses on the networks that span the
        /// cluster. The rollout goes on when every node resolves them.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        addresses: Vec<Ipv4Addr>,
    },
    Applied {
        /// The addresses of the stack's containers on this host, on the
        /// networks that span the cluster.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        addresses: Vec<Ipv4Addr>,
    },
    /// The job of a `Job` step ran and succeeded.
    JobRan(Box<crate::domain::scheduler::JobRun>),
    /// The stack did not become healthy, or a job failed. The host is left
    /// as it is; the client decides whether to roll back.
    Failed(StepFailure),
    /// `None` on a cluster node the plan places no service on.
    Finished(Option<Box<DeploymentResult>>),
    RolledBack,
    Released,
}
