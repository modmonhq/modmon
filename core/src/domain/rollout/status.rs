//! What a host remembers of its deployments: the last one it finished, the
//! one before, and a rollout that is running or was left unfinished.
//! `mm deploy status` shows it and `mm deploy rollback` plans from it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::StepFailure;
use crate::domain::build::BuildReceipt;
use crate::domain::release_facts::{ReleaseFacts, ReleaseRequest};

/// A deployment a host finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentRecord {
    pub commit: String,
    pub deployed_at_unix_ms: i64,
    pub deployed_by: String,
    /// Whether the deployment placed services on the host.
    pub runs_services: bool,
    /// The plan that was applied: for a cluster, the hash every node of
    /// the deployment shares.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_hash: Option<String>,
    /// The build whose images the deployment runs, when it was planned
    /// with one. Planning the commit again needs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<Box<BuildReceipt>>,
    /// The release the deployment runs: its name and the inputs it was
    /// given. Planning it again gives it the same ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseFacts>,
}

impl DeploymentRecord {
    /// Whether two records run the same thing: the same commit with the
    /// same images, name and inputs.
    #[must_use]
    pub fn same_release(&self, other: &Self) -> bool {
        fn build(record: &DeploymentRecord) -> Option<&str> {
            record
                .receipt
                .as_ref()
                .map(|receipt| receipt.receipt_hash.as_str())
        }
        self.commit == other.commit
            && build(self) == build(other)
            && ReleaseFacts::same_request(self.release.as_ref(), other.release.as_ref())
    }

    /// What planning this deployment again asks for: a rollback returns to
    /// it, and a redeploy keeps it.
    #[must_use]
    pub fn release_request(&self) -> ReleaseRequest {
        self.release
            .as_ref()
            .map(ReleaseFacts::request)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RolloutState {
    /// A client holds the lock; the host has changed nothing yet.
    Held,
    /// The host began its part and has neither finished nor rolled back.
    Running,
    Finished,
    RolledBack,
}

/// The rollout a host last took part in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeRollout {
    pub plan_hash: String,
    pub commit: String,
    pub started_by: String,
    pub started_at_unix_ms: i64,
    /// When the host last heard from the client that deploys.
    pub heard_at_unix_ms: i64,
    /// Whether that client still holds the environment's deployment lock.
    pub active: bool,
    pub state: RolloutState,
    /// The stacks the host applied, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stacks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<StepFailure>,
}

impl NodeRollout {
    /// Begun, and neither finished nor rolled back.
    #[must_use]
    pub fn unfinished(&self) -> bool {
        self.state == RolloutState::Running
    }
}

/// What one host says about its deployments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NodeDeploymentStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<DeploymentRecord>,
    /// The last deployment before `current` that ran something else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<DeploymentRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout: Option<NodeRollout>,
    /// What runs on the host for the environment right now: a deployment
    /// step, an access sync. Nothing waits for an operation to answer this.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<super::OperationInFlight>,
}

/// One host's row of `mm deploy status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeStatusRow {
    pub node: String,
    /// Why the host did not answer, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<NodeDeploymentStatus>,
}

impl NodeStatusRow {
    fn rollout(&self) -> Option<&NodeRollout> {
        self.status
            .as_ref()
            .and_then(|status| status.rollout.as_ref())
    }

    fn current(&self) -> Option<&DeploymentRecord> {
        self.status
            .as_ref()
            .and_then(|status| status.current.as_ref())
    }
}

/// `mm deploy status`: every host of the environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeploymentStatus {
    pub target: String,
    pub environment: String,
    pub nodes: Vec<NodeStatusRow>,
    /// Services the nodes answer and route to while none of their
    /// containers is healthy (fail open), as `<service> on <network>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answered_unhealthy: Vec<String>,
}

impl DeploymentStatus {
    /// A rollout a client is still working on, if any host knows of one.
    #[must_use]
    pub fn running(&self) -> Option<(&str, &NodeRollout)> {
        self.nodes.iter().find_map(|row| {
            row.rollout()
                .filter(|rollout| {
                    rollout.active
                        && matches!(rollout.state, RolloutState::Held | RolloutState::Running)
                })
                .map(|rollout| (row.node.as_str(), rollout))
        })
    }

    /// Every operation the hosts are running for the environment, with the
    /// host that runs it.
    #[must_use]
    pub fn in_flight(&self) -> Vec<(&str, &super::OperationInFlight)> {
        self.nodes
            .iter()
            .flat_map(|row| {
                row.status
                    .iter()
                    .flat_map(|status| &status.operations)
                    .map(|operation| (row.node.as_str(), operation))
            })
            .collect()
    }

    /// The hosts that did not answer: what runs there is unknown.
    #[must_use]
    pub fn unanswered(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|row| row.status.is_none())
            .map(|row| row.node.as_str())
            .collect()
    }

    /// Whether every host answered and none has finished a deployment of
    /// the environment: nothing serves it yet.
    #[must_use]
    pub fn never_deployed(&self) -> bool {
        !self.nodes.is_empty()
            && self.nodes.iter().all(|row| {
                row.status
                    .as_ref()
                    .is_some_and(|status| status.current.is_none() && status.previous.is_none())
            })
    }

    /// Whether there are hosts and none of them answered.
    #[must_use]
    pub fn none_answered(&self) -> bool {
        !self.nodes.is_empty() && self.nodes.iter().all(|row| row.status.is_none())
    }

    /// The sentence `mm deploy status` ends with.
    #[must_use]
    pub fn summary(&self) -> String {
        let unknown = self.unanswered();
        let also = match unknown.as_slice() {
            [] => String::new(),
            nodes => format!(
                " {} did not answer: what runs there is unknown.",
                nodes.join(", ")
            ),
        };
        if self.none_answered() {
            return "No node answered, so whether a rollout is in progress is unknown. Each node's line above says why; `mm doctor` checks the nodes.".to_owned();
        }
        if let Some((node, rollout)) = self.running() {
            return format!(
                "{} is deploying plan {} right now (node {node}). Run `mm deploy status` again to follow it; `mm deploy cancel {}` stops it.{also}",
                rollout.started_by,
                short(&rollout.plan_hash),
                short(&rollout.plan_hash)
            );
        }
        if let Some((node, operation)) = self.in_flight().first() {
            return format!(
                "{} runs on {node} right now; `mm deploy cancel {}` stops it.{also}",
                operation.describe(),
                operation.short_id()
            );
        }
        let unfinished = self.unfinished();
        if unfinished.is_empty() {
            return if unknown.is_empty() {
                "No rollout is in progress.".to_owned()
            } else {
                format!("No rollout is in progress on the nodes that answered.{also}")
            };
        }
        format!(
            "A rollout was left unfinished on {}. `mm deploy rollback` returns the environment to its last finished deployment; `mm deploy plan` plans a new one, and applying it undoes the unfinished rollout first.{also}",
            unfinished
                .iter()
                .map(|(node, _)| *node)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    /// The hosts that began a rollout nobody finished or rolled back, and
    /// that no client is working on.
    #[must_use]
    pub fn unfinished(&self) -> Vec<(&str, &NodeRollout)> {
        self.nodes
            .iter()
            .filter_map(|row| {
                row.rollout()
                    .filter(|rollout| rollout.unfinished() && !rollout.active)
                    .map(|rollout| (row.node.as_str(), rollout))
            })
            .collect()
    }
}

/// What `mm deploy rollback` plans: a commit, the build it ran with, and
/// the nodes that return to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackTarget {
    pub commit: String,
    pub receipt: Option<BuildReceipt>,
    /// The release name and inputs that deployment ran with.
    pub release: ReleaseRequest,
    /// The nodes to plan; every node when the rollback covers them all.
    pub nodes: Vec<String>,
    /// Whether every host of the environment is in `nodes`.
    pub every_node: bool,
    /// One sentence on what the rollback undoes.
    pub undoes: String,
    /// When the release it returns to was deployed: the jobs that ran
    /// since are not undone.
    pub since_unix_ms: i64,
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

/// Decides what a rollback returns to, from what the hosts remember.
///
/// A rollout that was left unfinished is undone first: the hosts return to
/// the deployment they last finished. Otherwise the last finished
/// deployment is undone: the hosts that took part return to what each ran
/// before it. `only` limits the rollback to some nodes.
pub fn rollback_target(
    status: &DeploymentStatus,
    only: Option<&[String]>,
) -> Result<RollbackTarget, String> {
    let selected = status
        .nodes
        .iter()
        .filter(|row| only.is_none_or(|only| only.iter().any(|name| name == &row.node)))
        .collect::<Vec<_>>();
    if let Some(silent) = selected.iter().find(|row| row.status.is_none()) {
        let others = selected
            .iter()
            .filter(|row| row.status.is_some())
            .map(|row| row.node.as_str())
            .collect::<Vec<_>>();
        return Err(format!(
            "node '{}' did not answer, so mm cannot tell what it ran before: {}. {}",
            silent.node,
            silent.problem.as_deref().unwrap_or("no answer"),
            if others.is_empty() {
                "Bring the node back and run `mm deploy rollback` again".to_owned()
            } else {
                format!(
                    "To roll back the others, run `mm deploy rollback --nodes {}`",
                    others.join(",")
                )
            }
        ));
    }
    if let Some((node, rollout)) = status.running() {
        return Err(format!(
            "{} is deploying plan {} to '{}' right now (node '{node}' last heard from it at {}); wait for it to finish, then run `mm deploy rollback` again. `mm deploy status` shows where it is",
            rollout.started_by,
            short(&rollout.plan_hash),
            status.environment,
            utc_time(rollout.heard_at_unix_ms),
        ));
    }
    let unfinished = selected
        .iter()
        .filter(|row| row.rollout().is_some_and(NodeRollout::unfinished))
        .copied()
        .collect::<Vec<_>>();
    if unfinished.is_empty() {
        undo_last(status, &selected)
    } else {
        undo_unfinished(status, &selected, &unfinished)
    }
}

/// The hosts return to the deployment they last finished.
fn undo_unfinished(
    status: &DeploymentStatus,
    selected: &[&NodeStatusRow],
    unfinished: &[&NodeStatusRow],
) -> Result<RollbackTarget, String> {
    let rollout = unfinished
        .first()
        .and_then(|row| row.rollout())
        .ok_or("the unfinished rollout disappeared")?;
    let finished = selected
        .iter()
        .filter_map(|row| row.current().map(|current| (row.node.as_str(), current)))
        .collect::<Vec<_>>();
    let (_, target) = finished.first().ok_or_else(|| {
        format!(
            "the rollout of plan {} was left unfinished on {}, and no node finished a deployment before it, so there is nothing to return to. Run `mm deploy plan` and apply it: it starts from what each node runs now",
            short(&rollout.plan_hash),
            names(unfinished.iter().map(|row| row.node.as_str())),
        )
    })?;
    agree(&finished, "last finished")?;
    Ok(RollbackTarget {
        commit: target.commit.clone(),
        receipt: target.receipt.as_deref().cloned(),
        release: target.release_request(),
        since_unix_ms: target.deployed_at_unix_ms,
        nodes: selected.iter().map(|row| row.node.clone()).collect(),
        every_node: selected.len() == status.nodes.len(),
        undoes: format!(
            "the unfinished rollout of plan {} (commit {}, started by {}) on {}",
            short(&rollout.plan_hash),
            short(&rollout.commit),
            rollout.started_by,
            names(unfinished.iter().map(|row| row.node.as_str())),
        ),
    })
}

/// The hosts that took part in the last finished deployment return to what
/// each ran before it.
fn undo_last(
    status: &DeploymentStatus,
    selected: &[&NodeStatusRow],
) -> Result<RollbackTarget, String> {
    let latest = selected
        .iter()
        .filter_map(|row| row.current())
        .max_by_key(|current| current.deployed_at_unix_ms)
        .ok_or_else(|| {
            format!(
                "nothing is deployed to '{}' yet, so there is nothing to roll back",
                status.environment
            )
        })?;
    let before = selected
        .iter()
        .filter(|row| {
            row.current()
                .is_some_and(|current| match &latest.plan_hash {
                    Some(hash) => current.plan_hash.as_ref() == Some(hash),
                    None => current.same_release(latest),
                })
        })
        .filter_map(|row| {
            row.status
                .as_ref()
                .and_then(|status| status.previous.as_ref())
                .map(|previous| (row.node.as_str(), previous))
        })
        .collect::<Vec<_>>();
    let (_, target) = before.first().ok_or_else(|| {
        format!(
            "commit {} is the first deployment to '{}', so there is no earlier one to return to",
            short(&latest.commit),
            status.environment
        )
    })?;
    agree(&before, "previous")?;
    let nodes = before
        .iter()
        .map(|(node, _)| (*node).to_owned())
        .collect::<Vec<_>>();
    Ok(RollbackTarget {
        commit: target.commit.clone(),
        receipt: target.receipt.as_deref().cloned(),
        release: target.release_request(),
        since_unix_ms: target.deployed_at_unix_ms,
        every_node: nodes.len() == status.nodes.len(),
        nodes,
        undoes: format!(
            "the deployment of commit {} by {}",
            short(&latest.commit),
            latest.deployed_by
        ),
    })
}

/// What `mm deploy redeploy` plans: the commit and build the selected nodes
/// run now, so that what was staged on them since goes live.
pub fn redeploy_target(
    status: &DeploymentStatus,
    only: Option<&[String]>,
) -> Result<RollbackTarget, String> {
    let selected = status
        .nodes
        .iter()
        .filter(|row| only.is_none_or(|only| only.iter().any(|name| name == &row.node)))
        .collect::<Vec<_>>();
    if let Some(silent) = selected.iter().find(|row| row.status.is_none()) {
        return Err(format!(
            "node '{}' did not answer, so mm cannot tell what it runs: {}. Bring it back, or leave it out with --nodes",
            silent.node,
            silent.problem.as_deref().unwrap_or("no answer"),
        ));
    }
    if let Some((node, rollout)) = status.running() {
        return Err(format!(
            "{} is deploying plan {} to '{}' right now (node '{node}'); wait for it to finish, then run `mm deploy redeploy` again",
            rollout.started_by,
            short(&rollout.plan_hash),
            status.environment,
        ));
    }
    let running = selected
        .iter()
        .filter_map(|row| row.current().map(|current| (row.node.as_str(), current)))
        .collect::<Vec<_>>();
    let (_, target) = running.first().ok_or_else(|| {
        format!(
            "nothing is deployed to '{}' yet, so there is nothing to deploy again; run `mm deploy plan`",
            status.environment
        )
    })?;
    agree(&running, "current")?;
    Ok(RollbackTarget {
        commit: target.commit.clone(),
        receipt: target.receipt.as_deref().cloned(),
        release: target.release_request(),
        since_unix_ms: target.deployed_at_unix_ms,
        every_node: running.len() == status.nodes.len(),
        nodes: running.iter().map(|(node, _)| (*node).to_owned()).collect(),
        undoes: format!(
            "commit {} as {} deployed it",
            short(&target.commit),
            target.deployed_by
        ),
    })
}

/// A plan has one commit, so the nodes of a rollback must return to the
/// same one. When they do not, the error names a command per commit.
fn agree(records: &[(&str, &DeploymentRecord)], which: &str) -> Result<(), String> {
    let Some((_, first)) = records.first() else {
        return Ok(());
    };
    if records.iter().all(|(_, record)| record.same_release(first)) {
        return Ok(());
    }
    let mut groups = Vec::<(&DeploymentRecord, Vec<&str>)>::new();
    for (node, record) in records {
        match groups
            .iter_mut()
            .find(|(known, _)| known.same_release(record))
        {
            Some((_, nodes)) => nodes.push(node),
            None => groups.push((record, vec![node])),
        }
    }
    let commands = groups
        .iter()
        .map(|(record, nodes)| {
            format!(
                "`mm deploy plan --commit {}{} --nodes {}`",
                record.commit,
                record.release_request().arguments(),
                nodes.join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(", then ");
    Err(format!(
        "the nodes' {which} deployments differ ({}), and one plan deploys one commit. Plan them one group at a time: {commands}",
        groups
            .iter()
            .map(|(record, nodes)| format!("{} on {}", short(&record.commit), nodes.join(", ")))
            .collect::<Vec<_>>()
            .join("; "),
    ))
}

fn names<'a>(nodes: impl Iterator<Item = &'a str>) -> String {
    nodes.collect::<Vec<_>>().join(", ")
}

/// A moment as `2026-01-31T14:02:11Z`.
#[must_use]
pub fn utc_time(unix_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(unix_ms).map_or_else(
        || unix_ms.to_string(),
        |moment| moment.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
}
