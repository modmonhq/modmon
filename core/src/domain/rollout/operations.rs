//! What a host is doing right now: the operations in flight on it, how its
//! daemon fares, and what `mm deploy cancel` stopped there. `mm deploy
//! status`, `mm doctor`, the MCP read `deployments`, and the heartbeat of a
//! client that deploys all read it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How many characters of an operation's ID are shown, and are enough to
/// name it to `mm deploy cancel`.
pub const SHORT_ID: usize = 12;

/// One operation a host's `mmd` is running: a deployment or one step of
/// it, an access sync, a restart, a job run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OperationInFlight {
    /// The operation's ID: the reviewed plan's hash for a deployment or an
    /// access sync, which every step and node of it shares, and the
    /// request's ID for anything else.
    pub id: String,
    /// What it does, in words, such as `deployment (stack 'app')`.
    pub what: String,
    /// Who started it.
    pub principal: String,
    pub started_at_unix_ms: i64,
    /// When the host last saw it at work: the loop that waits on the engine
    /// for it looks every two seconds.
    pub alive_at_unix_ms: i64,
    /// When the host stops it, if it has not ended by then.
    pub deadline_unix_ms: i64,
    /// Why it is being stopped, once it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopping: Option<String>,
    /// What it does right now, when the host says, such as `pulling image
    /// 2 of 3 for stack 'app': registry.example.com/api:1.4`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doing: Option<String>,
}

impl OperationInFlight {
    /// The ID as it is shown.
    #[must_use]
    pub fn short_id(&self) -> &str {
        short_id(&self.id)
    }

    /// Whether `wanted` names this operation: its whole ID or the start of
    /// it, at least six characters long.
    #[must_use]
    pub fn is_named(&self, wanted: &str) -> bool {
        wanted.len() >= 6 && self.id.starts_with(wanted)
    }

    /// One line: what, which, who, since when.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "{} {} by {} since {}",
            self.what,
            self.short_id(),
            self.principal,
            super::utc_time(self.started_at_unix_ms)
        )
    }
}

/// The ID as it is shown: its first twelve characters.
#[must_use]
pub fn short_id(id: &str) -> &str {
    id.get(..SHORT_ID).unwrap_or(id)
}

/// A child process of `mmd` that is still there after its deadline: its
/// supervisor ended its process group, and something kept it alive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LateChild {
    pub pid: u32,
    /// The program, without its arguments.
    pub program: String,
    pub started_at_unix_ms: i64,
    pub deadline_unix_ms: i64,
}

/// How a host's daemon fares: what `mm doctor` reports for each node.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DaemonHealth {
    /// When this `mmd` process started: a recent time after a long uptime
    /// means it was restarted, by its watchdog or by hand.
    pub started_at_unix_ms: i64,
    /// The service manager's watchdog interval, when one watches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watchdog_seconds: Option<u64>,
    /// Parts of the daemon that did not answer when it last looked: the
    /// name resolver, the watch over child processes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_answering: Vec<String>,
    /// Every operation running on the host, for any environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<OperationInFlight>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub late_children: Vec<LateChild>,
}

/// What `mm deploy cancel` did on one host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CancelOutcome {
    /// The operations it stopped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stopped: Vec<OperationInFlight>,
    /// The deployment (by plan hash) it returned to what the host ran
    /// before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolled_back: Option<String>,
    /// A deployment (by plan hash) that had changed nothing here yet and
    /// gave up the environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    /// What the host could not do, and what to run: undo what was stopped,
    /// or stop the job runs it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The job runs it stopped: what it stopped started them, by hand or
    /// as part of a deployment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<CancelledRun>,
}

/// A job run that `mm deploy cancel` stopped on a host: its container was
/// stopped and the run recorded as cancelled, or, while the host's job
/// keeper looks at it, is being cancelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CancelledRun {
    pub job: String,
    pub run: uuid::Uuid,
    pub state: crate::domain::scheduler::JobRunState,
}

impl CancelOutcome {
    /// Whether nothing was running and nothing held the environment.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stopped.is_empty()
            && self.runs.is_empty()
            && self.rolled_back.is_none()
            && self.released.is_none()
            && self.problem.is_none()
    }
}

/// What `mm deploy cancel` did on one host of the environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeCancel {
    pub node: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<CancelOutcome>,
    /// Why the host did not answer, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// What `mm deploy cancel` did on every host of the environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelReport {
    pub target: String,
    pub environment: String,
    /// The operation asked for, when one was named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    pub nodes: Vec<NodeCancel>,
}

impl CancelReport {
    /// Whether no host had anything to stop or undo.
    #[must_use]
    pub fn found_nothing(&self) -> bool {
        self.nodes
            .iter()
            .all(|node| node.outcome.as_ref().is_none_or(CancelOutcome::is_empty))
    }
}

/// An operation the host has not seen at work for this long is reported as
/// stuck: the loop that waits on the engine for it looks every two seconds.
pub const STUCK_AFTER_MS: i64 = 120_000;
/// A daemon that started this recently is reported, with when: it was
/// restarted, by its watchdog or by hand.
pub const RECENT_START_MS: i64 = 15 * 60 * 1_000;

/// What `mm doctor` says about one node's daemon: parts that do not
/// answer, a recent restart, operations in flight and stuck ones, and
/// child processes still there after their deadline.
#[must_use]
pub fn daemon_diagnostics(
    node: &str,
    health: &DaemonHealth,
    now_unix_ms: i64,
) -> Vec<crate::domain::diagnostic::Diagnostic> {
    use crate::domain::diagnostic::Diagnostic;

    let mut found = Vec::new();
    for part in &health.not_answering {
        found.push(
            Diagnostic::error(
                "MM_DAEMON_NOT_ANSWERING",
                format!("node '{node}': part of mmd does not answer: {part}"),
            )
            .with_help(
                "`journalctl -u mmd` on the node says why; its watchdog restarts mmd when its request loop stops answering, and `systemctl restart mmd` does it by hand",
            ),
        );
    }
    let up = now_unix_ms.saturating_sub(health.started_at_unix_ms);
    if (0..RECENT_START_MS).contains(&up) {
        found.push(
            Diagnostic::info(
                "MM_DAEMON_RESTARTED",
                format!(
                    "node '{node}': mmd started {} min ago, at {}",
                    up / 60_000,
                    super::utc_time(health.started_at_unix_ms)
                ),
            )
            .with_help("`journalctl -u mmd` on the node shows whether its watchdog restarted it"),
        );
    }
    for operation in &health.operations {
        let quiet = now_unix_ms.saturating_sub(operation.alive_at_unix_ms);
        let late = now_unix_ms > operation.deadline_unix_ms;
        let line = format!("node '{node}' runs {}", operation.describe());
        let help = format!(
            "`mm deploy status` shows it; `mm deploy cancel {}` stops it",
            operation.short_id()
        );
        found.push(if late || quiet > STUCK_AFTER_MS {
            Diagnostic::warning(
                "MM_OPERATION_STUCK",
                format!(
                    "{line}, and {}",
                    if late {
                        "it is past its deadline".to_owned()
                    } else {
                        format!("the host has not seen it at work for {} s", quiet / 1_000)
                    }
                ),
            )
            .with_help(help)
        } else {
            Diagnostic::info("MM_OPERATION_IN_FLIGHT", line).with_help(help)
        });
    }
    for child in &health.late_children {
        found.push(
            Diagnostic::error(
                "MM_CHILD_PAST_DEADLINE",
                format!(
                    "node '{node}': {} (process {}) started by mmd at {} is still there after its deadline of {}",
                    child.program,
                    child.pid,
                    super::utc_time(child.started_at_unix_ms),
                    super::utc_time(child.deadline_unix_ms)
                ),
            )
            .with_help(
                "mmd ended its process group and it did not go; `systemctl restart mmd` on the node ends what mmd started",
            ),
        );
    }
    found
}
