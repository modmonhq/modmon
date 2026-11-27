//! Plans, apply reports, and node reports for a cluster target.
//!
//! The client coordinates: it asks every node's `mmd` to plan, computes one
//! hash over the ordered node sections, and has every node keep that seal.
//! `mm deploy apply --plan HASH` then applies exactly the sealed sections. A
//! node only ever acts on its own engine.

use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    RemoteAccessPlan, RemoteDeploymentPlan, RemotePlanKind, is_commit_sha, is_lower_hex_digest,
};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::workspace::ProjectId;

/// One node's part of a cluster plan: the plan that node stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClusterPlanSection {
    pub node: String,
    pub plan_hash: String,
}

/// What every node of a cluster plan keeps: the ordered sections and the one
/// hash over them. Changing anything on any node changes that node's plan
/// hash, and so the cluster hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClusterPlanSeal {
    pub kind: RemotePlanKind,
    pub project_id: ProjectId,
    pub environment: String,
    pub target: String,
    pub target_fingerprint: String,
    pub commit: String,
    /// The planned nodes, in the target's declared order.
    pub sections: Vec<ClusterPlanSection>,
    /// Nodes of the target the plan leaves out; they keep what they run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
    /// The addresses of the cluster's networks the plan gives out. Every
    /// node of the plan stores them when it applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub networks: Option<crate::domain::cluster_network::SealedNetworks>,
    /// The order a deployment plan rolls out in: one stage per stack, each
    /// a list of node batches. The apply follows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout: Option<crate::domain::rollout::ClusterRollout>,
    /// The plan's shared volumes: the storage node and the directories it
    /// creates before any node is replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<crate::domain::shared_storage::SealedStorage>,
    /// The routes of the plan and the nodes that serve them; `None` when
    /// the plan leaves nodes out, so every ingress keeps what it serves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<crate::domain::cluster_ingress::SealedIngress>,
    pub cluster_plan_hash: String,
}

/// What a cluster plan carries besides its node sections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SealCarried {
    pub networks: Option<crate::domain::cluster_network::SealedNetworks>,
    pub rollout: Option<crate::domain::rollout::ClusterRollout>,
    pub storage: Option<crate::domain::shared_storage::SealedStorage>,
    pub ingress: Option<crate::domain::cluster_ingress::SealedIngress>,
}

impl
    From<(
        Option<crate::domain::cluster_network::SealedNetworks>,
        Option<crate::domain::rollout::ClusterRollout>,
    )> for SealCarried
{
    fn from(
        (networks, rollout): (
            Option<crate::domain::cluster_network::SealedNetworks>,
            Option<crate::domain::rollout::ClusterRollout>,
        ),
    ) -> Self {
        Self {
            networks,
            rollout,
            storage: None,
            ingress: None,
        }
    }
}

impl ClusterPlanSeal {
    /// The seal as a node keeps and returns it: without what only the
    /// client's own plan output reads.
    #[must_use]
    pub fn as_kept(&self) -> Self {
        let mut kept = self.clone();
        if let Some(rollout) = kept.rollout.as_mut() {
            rollout.older_nodes.clear();
        }
        kept
    }

    /// Builds the seal and computes its hash.
    pub fn new(
        kind: RemotePlanKind,
        scope: &super::RemoteScope,
        commit: &str,
        sections: Vec<ClusterPlanSection>,
        skipped: Vec<String>,
        carried: impl Into<SealCarried>,
    ) -> Result<Self, String> {
        let carried = carried.into();
        let mut seal = Self {
            kind,
            project_id: scope.project_id,
            environment: scope.environment.clone(),
            target: scope.target.clone(),
            target_fingerprint: scope.target_fingerprint.clone(),
            commit: commit.to_owned(),
            sections,
            skipped,
            networks: carried.networks,
            rollout: carried.rollout,
            storage: carried.storage,
            ingress: carried.ingress,
            cluster_plan_hash: String::new(),
        };
        seal.cluster_plan_hash = seal.canonical_hash()?;
        Ok(seal)
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        if self.sections.is_empty() {
            return Err("a cluster plan needs at least one node".to_owned());
        }
        if !is_commit_sha(&self.commit) || !is_lower_hex_digest(&self.target_fingerprint) {
            return Err("cluster plan identity metadata is invalid".to_owned());
        }
        let mut names = std::collections::BTreeSet::new();
        for name in self
            .sections
            .iter()
            .map(|section| &section.node)
            .chain(&self.skipped)
        {
            crate::domain::target::validate_target_name(name)
                .map_err(|_| "cluster plan names an invalid node".to_owned())?;
            if !names.insert(name.as_str()) {
                return Err(format!("cluster plan lists node '{name}' twice"));
            }
        }
        let mut hasher = blake3::Hasher::new();
        let mut field = |value: &str| {
            hasher.update(&(value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        };
        field("mm-cluster-plan-v1");
        field(match self.kind {
            RemotePlanKind::Deployment => "deployment",
            RemotePlanKind::TeamSync => "team-sync",
        });
        field(&self.project_id.0.to_string());
        field(&self.environment);
        field(&self.target);
        field(&self.target_fingerprint);
        field(&self.commit);
        field(&self.sections.len().to_string());
        for section in &self.sections {
            if !is_lower_hex_digest(&section.plan_hash) {
                return Err("cluster plan section hash is invalid".to_owned());
            }
            field(&section.node);
            field(&section.plan_hash);
        }
        field(&self.skipped.len().to_string());
        for node in &self.skipped {
            field(node);
        }
        if let Some(networks) = &self.networks {
            networks.validate()?;
            field("networks-v1");
            field(&serde_json::to_string(networks).map_err(|error| error.to_string())?);
        }
        if let Some(rollout) = &self.rollout {
            field("rollout-v1");
            field(&serde_json::to_string(rollout).map_err(|error| error.to_string())?);
        }
        if let Some(storage) = &self.storage {
            storage.validate()?;
            field("storage-v1");
            field(&serde_json::to_string(storage).map_err(|error| error.to_string())?);
        }
        if let Some(ingress) = &self.ingress {
            ingress.validate()?;
            field("ingress-v1");
            field(&serde_json::to_string(ingress).map_err(|error| error.to_string())?);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.canonical_hash()? != self.cluster_plan_hash {
            return Err("cluster plan hash does not cover its node sections".to_owned());
        }
        Ok(())
    }

    /// The section of `node`, when the plan includes it.
    #[must_use]
    pub fn section(&self, node: &str) -> Option<&ClusterPlanSection> {
        self.sections.iter().find(|section| section.node == node)
    }
}

/// One node's section of a deployment plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeDeploymentPlan {
    /// `None` for a target that names one host.
    pub node: Option<String>,
    pub plan: RemoteDeploymentPlan,
}

/// `mm deploy plan` on a remote target: a section per node and one hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClusterDeploymentPlan {
    /// The hash `mm deploy apply --plan` takes. For a cluster it covers every
    /// node section; for one host it is that host's plan hash.
    pub plan_hash: String,
    pub target: String,
    pub environment: String,
    pub commit: String,
    /// The earliest moment a node's plan expires.
    pub expires_at_unix_ms: i64,
    pub nodes: Vec<NodeDeploymentPlan>,
    /// Nodes left out with `--nodes`; they keep what they run now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
    /// Services that change node: each starts where it is placed before it
    /// stops where it was.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<super::ServiceMove>,
    /// The addresses of the networks that span the nodes, and what the plan
    /// changes about them on each node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub networks: Option<crate::domain::cluster_network::ClusterNetworkPlan>,
    /// The order the nodes are replaced in and how long each may take to
    /// become healthy. Absent for a target that names one host, whose plan
    /// says the same in its own section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout: Option<crate::domain::rollout::ClusterRollout>,
    /// The plan's shared volumes: the node that keeps each one's files and
    /// the nodes that mount it over the mesh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<crate::domain::shared_storage::ClusterStoragePlan>,
    /// The routes of the plan and the nodes whose proxy serves them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<crate::domain::cluster_ingress::SealedIngress>,
    /// Nodes that hold a rollout nobody finished or rolled back. Applying
    /// this plan first returns each of them to what it ran before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unfinished: Vec<UnfinishedRollout>,
    /// On a target whose hosts take the project's files from the client:
    /// the bundle every node planned from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<crate::domain::delivery::PlanBundle>,
}

impl ClusterDeploymentPlan {
    /// The routes the deployment serves: on a cluster those of the ingress
    /// nodes, which no node section claims for itself; on one host the
    /// routes its section claims.
    #[must_use]
    pub fn route_count(&self) -> usize {
        self.ingress.as_ref().map_or_else(
            || {
                self.nodes
                    .iter()
                    .filter_map(|section| section.plan.plan.as_ref())
                    .map(|deployment| deployment.desired_route_claims.len())
                    .sum()
            },
            |ingress| ingress.routes.len(),
        )
    }
}

/// A rollout a node began and nobody finished, as a plan reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnfinishedRollout {
    pub node: String,
    pub plan_hash: String,
    pub commit: String,
    pub started_by: String,
    pub started_at_unix_ms: i64,
}

/// One node's section of a team sync plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeAccessPlan {
    pub node: Option<String>,
    pub plan: RemoteAccessPlan,
}

/// `mm team sync` on a remote target: a section per node and one hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClusterAccessPlan {
    pub plan_hash: String,
    pub target: String,
    pub environment: String,
    pub commit: String,
    pub expires_at_unix_ms: i64,
    pub nodes: Vec<NodeAccessPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NodeApplyState {
    /// The node applied its section.
    Applied,
    /// The deployment failed on this node; `error` says why. With
    /// `rolled_back` the node runs what it ran before.
    Failed,
    /// The node was changed and then returned to what it ran before,
    /// because the deployment failed on another node.
    RolledBack,
    /// The node was changed and is left as it is: the deployment paused,
    /// or could not be finished or rolled back here.
    Unfinished,
    /// The deployment stopped before it reached this node, or the node
    /// refused it before changing anything (`error` says why); it is
    /// unchanged.
    NotStarted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeApplyOutcome<T> {
    pub node: String,
    pub state: NodeApplyState,
    /// What the node reported. Absent on a node that runs nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// On a failed node: whether it was returned to what it ran before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rolled_back: bool,
    /// On a failed node: the last lines the failed service's container
    /// logged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
    /// Services that moved to other nodes and were stopped here after every
    /// node had applied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stopped: Vec<crate::domain::placement::PlacedService>,
    /// On a failed node: it failed while it prepared, before any of its
    /// containers changed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unchanged: bool,
    /// On a failed node: the code of the error that stopped it, when an
    /// error did (`MM_TIMEOUT` for a step that ran out of time), rather
    /// than a service that did not become healthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// On a failed node: no node had finished a deployment of the
    /// environment, so every node keeps what the deployment started.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
}

/// Codes of a step that ran out of time on a host or showed no progress.
const TIMEOUT_CODES: [&str; 2] = ["MM_TIMEOUT", "MM_PROCESS_TIMEOUT"];

/// What applying a cluster plan did, node by node in apply order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClusterApplyReport<T> {
    pub plan_hash: String,
    pub target: String,
    pub environment: String,
    pub nodes: Vec<NodeApplyOutcome<T>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
}

impl<T> ClusterApplyReport<T> {
    /// The error a command fails with when the apply stopped at a node: a
    /// timeout when the node ran out of time or showed no progress,
    /// `MM_CANCELLED` when it was cancelled, else `MM_APPLY_STOPPED`. The
    /// message is `failure`'s.
    #[must_use]
    pub fn stopped(&self, replan: &str) -> Option<crate::domain::error::MmError> {
        let message = self.failure(replan)?;
        let code = self
            .nodes
            .iter()
            .find(|node| node.state == NodeApplyState::Failed)
            .and_then(|node| node.code.as_deref());
        Some(match code {
            Some(code) if TIMEOUT_CODES.contains(&code) => {
                crate::domain::error::MmError::Timeout { message }
            }
            Some("MM_CANCELLED") => crate::domain::error::MmError::Cancelled { message },
            _ => crate::domain::error::MmError::ApplyStopped { message },
        })
    }

    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.nodes
            .iter()
            .all(|node| node.state == NodeApplyState::Applied)
    }

    /// Why the deployment failed and what state each node is in, when it
    /// did not apply everywhere. `replan` is the command that plans again.
    #[must_use]
    pub fn failure(&self, replan: &str) -> Option<String> {
        let failed = self
            .nodes
            .iter()
            .find(|node| node.state == NodeApplyState::Failed)
            .or_else(|| {
                self.nodes
                    .iter()
                    .find(|node| node.state == NodeApplyState::Unfinished)
            })
            .or_else(|| {
                // A node that refused the change before touching anything.
                self.nodes
                    .iter()
                    .find(|node| node.state == NodeApplyState::NotStarted && node.error.is_some())
            })?;
        let names = |keep: &dyn Fn(&NodeApplyOutcome<T>) -> bool| {
            let names = self
                .nodes
                .iter()
                .filter(|node| keep(node))
                .map(|node| node.node.as_str())
                .collect::<Vec<_>>();
            (!names.is_empty()).then(|| names.join(", "))
        };
        // Someone stopped it: nothing to fix, only to run again.
        let cancelled = failed.code.as_deref() == Some("MM_CANCELLED");
        let mut message = format!(
            "plan {} {} on node '{}': {}",
            self.plan_hash,
            if cancelled { "was stopped" } else { "failed" },
            failed.node,
            failed
                .error
                .as_deref()
                .unwrap_or("the node reported no reason"),
        );
        // The interfaces that print no report still get the log's end.
        if let Some(last) = failed.log.rchunks(5).next() {
            message.push_str(&format!(". Its last log lines: {}", last.join(" | ")));
        }
        message.push_str(". ");
        let mut facts = Vec::new();
        if let Some(nodes) = names(&|node| {
            node.state == NodeApplyState::RolledBack
                || (node.state == NodeApplyState::Failed && node.rolled_back)
        }) {
            facts.push(format!("Rolled back to what they ran before: {nodes}"));
        }
        let left = names(&|node| {
            node.state == NodeApplyState::Unfinished
                || (node.state == NodeApplyState::Failed && !node.rolled_back && !node.unchanged)
        });
        if let Some(nodes) = &left {
            facts.push(format!("Changed and left as they are: {nodes}"));
        }
        if let Some(nodes) = names(&|node| node.state == NodeApplyState::Applied) {
            facts.push(format!("Run the new deployment: {nodes}"));
        }
        if let Some(nodes) = names(&|node| {
            node.state == NodeApplyState::NotStarted
                || (node.state == NodeApplyState::Failed && node.unchanged)
        }) {
            facts.push(format!("Not changed: {nodes}"));
        }
        message.push_str(&facts.join(". "));
        message.push_str(&if failed.kept {
            format!(". `mm deploy status` shows each node; fix the cause, then `{replan}` deploys over what they run")
        } else if left.is_some() {
            format!(
                ". `mm deploy status` shows each node; `mm deploy rollback` returns the changed nodes to what they ran before, and `{replan}` plans again from what each one runs now"
            )
        } else if cancelled {
            format!(". Run `{replan}` again to deploy")
        } else {
            format!(". Fix the cause, then run `{replan}` again")
        });
        Some(message)
    }
}

/// The last deployment a node applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NodeDeployment {
    pub commit: String,
    pub deployed_at_unix_ms: i64,
    pub deployed_by: String,
    /// Whether that deployment placed services on the node.
    pub runs_services: bool,
}

/// What a node's `mmd` says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NodeFacts {
    /// The node's own name from `/etc/mm/node.env`; `None` on a host that
    /// was not set up as a cluster node.
    pub node: Option<String>,
    pub daemon_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed: Option<NodeDeployment>,
    /// The rollout the node last took part in, when one runs there or was
    /// left unfinished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout: Option<crate::domain::rollout::NodeRollout>,
    /// The node's container engine, as its `mmd` last probed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<super::EngineFacts>,
    /// How the node's daemon fares: when it started, parts that do not
    /// answer, operations in flight, child processes past their deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon: Option<crate::domain::rollout::DaemonHealth>,
    /// The node's member of the environment's managed secret store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_store: Option<crate::domain::secret_store::StoreStatus>,
    /// The rollout steps the node's `mmd` knows beyond the first ones
    /// (`domain::rollout::NEWER_ROLLOUT_STEPS`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rollout_steps: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NodeState {
    /// `mmd` answered as this node.
    Ready,
    /// The host or its `mmd` did not answer.
    Unreachable,
    /// Its `mmd` answered and refused.
    Refused,
    /// The repo records no keys for the node yet.
    NotSetUp,
    /// The address reaches a host that is another node, or no node.
    WrongNode,
}

/// One row of `mm target nodes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeReport {
    pub name: String,
    /// The address this device connected to.
    pub host: String,
    pub port: u16,
    /// The repo's `host:port` for the node, when this device connects at
    /// an address of its own instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_address: Option<Ipv4Addr>,
    pub state: NodeState,
    pub reachable: bool,
    /// Why the node is not ready, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mmd_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed: Option<NodeDeployment>,
    /// A rollout that runs on the node or was left unfinished there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout: Option<crate::domain::rollout::NodeRollout>,
    /// The node's container engine: its version and Compose provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<super::EngineFacts>,
    /// How the node's daemon fares, when it answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon: Option<crate::domain::rollout::DaemonHealth>,
    /// The node's member of the environment's managed secret store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_store: Option<crate::domain::secret_store::StoreStatus>,
    /// The rollout steps the node's `mmd` knows beyond the first ones; a
    /// deploy uses them only when every node lists them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rollout_steps: Vec<String>,
}

impl NodeReport {
    /// `deploying` or `rollout unfinished`, when either is the case.
    #[must_use]
    pub fn rollout_summary(&self) -> Option<&'static str> {
        let rollout = self.rollout.as_ref()?;
        match (rollout.active, rollout.unfinished()) {
            (true, _) => Some("deploying"),
            (false, true) => Some("rollout unfinished"),
            (false, false) => None,
        }
    }
}

/// How to go ahead without `unreachable`: the `--nodes` list of the others.
#[must_use]
pub fn unreachable_help(unreachable: &[&str], others: &[&str]) -> String {
    let missing = unreachable.join(", ");
    if others.is_empty() {
        return format!("No other node is left to act on without {missing}");
    }
    format!(
        "To go ahead without {missing}, add `--nodes {}`: {missing} {} what {} now",
        others.join(","),
        if unreachable.len() == 1 {
            "keeps"
        } else {
            "keep"
        },
        if unreachable.len() == 1 {
            "it runs"
        } else {
            "they run"
        },
    )
}

/// Per-node problems for `mm doctor`: nodes that do not answer, are not set
/// up, or answer as another node, and nodes that disagree on the `mmd`
/// version or on the deployed commit.
#[must_use]
pub fn node_diagnostics(target: &str, reports: &[NodeReport]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for report in reports {
        let (code, help) = match report.state {
            NodeState::Ready => continue,
            NodeState::Unreachable => (
                "MM_NODE_UNREACHABLE",
                format!(
                    "check the host and `systemctl status mmd` on it; `mm target nodes` shows every node of '{target}'"
                ),
            ),
            NodeState::Refused => (
                "MM_NODE_REFUSED",
                format!(
                    "the node's mmd answered and refused: what it said is the next step there. `mm target nodes` shows every node of '{target}'"
                ),
            ),
            NodeState::NotSetUp => (
                "MM_NODE_NOT_SET_UP",
                "run `mm target setup plan`, apply it, then `mm target setup record`".to_owned(),
            ),
            NodeState::WrongNode => (
                "MM_NODE_MISMATCH",
                format!(
                    "check `host` for each node under targets.{target}.nodes, then run `mm target setup record`"
                ),
            ),
        };
        diagnostics.push(
            Diagnostic::error(
                code,
                format!(
                    "node '{}' ({}:{}): {}",
                    report.name,
                    report.host,
                    report.port,
                    report.problem.as_deref().unwrap_or("not ready")
                ),
            )
            .with_help(help),
        );
    }
    let unfinished = reports
        .iter()
        .filter(|report| report.rollout_summary() == Some("rollout unfinished"))
        .map(|report| report.name.as_str())
        .collect::<Vec<_>>();
    if !unfinished.is_empty() {
        diagnostics.push(
            Diagnostic::warning(
                "MM_ROLLOUT_UNFINISHED",
                format!(
                    "a rollout was left unfinished on {} of '{target}': the node is changed and nobody is working on it",
                    unfinished.join(", ")
                ),
            )
            .with_help(
                "`mm deploy status` shows what was applied; `mm deploy rollback` returns the environment to its last finished deployment",
            ),
        );
    }
    diagnostics.extend(skew_diagnostics(target, reports));
    diagnostics.extend(super::engine_diagnostics(reports));
    diagnostics.extend(crate::domain::secret_store::store_diagnostics(
        target,
        &reports
            .iter()
            .filter_map(|report| {
                report
                    .secret_store
                    .as_ref()
                    .map(|store| (report.name.as_str(), store))
            })
            .collect::<Vec<_>>(),
    ));
    diagnostics
}

/// Ready nodes that disagree with each other: on the `mmd` version, or on
/// the commit they last applied.
fn skew_diagnostics(target: &str, reports: &[NodeReport]) -> Vec<Diagnostic> {
    let ready = reports
        .iter()
        .filter(|report| report.state == NodeState::Ready)
        .collect::<Vec<_>>();
    let version = |report: &NodeReport| report.mmd_version.clone();
    let commit = |report: &NodeReport| {
        report
            .deployed
            .as_ref()
            .map(|deployed| deployed.commit.chars().take(12).collect::<String>())
    };
    let differ = |value: &dyn Fn(&NodeReport) -> Option<String>| {
        ready
            .iter()
            .map(|report| value(report))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1
    };
    let each = |value: &dyn Fn(&NodeReport) -> Option<String>, missing: &str| {
        ready
            .iter()
            .map(|report| {
                format!(
                    "{} {}",
                    report.name,
                    value(report).unwrap_or_else(|| missing.to_owned())
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut diagnostics = Vec::new();
    if differ(&version) {
        diagnostics.push(
            Diagnostic::warning(
                "MM_NODE_VERSION_SKEW",
                format!(
                    "nodes of '{target}' run different mmd versions: {}",
                    each(&version, "unknown")
                ),
            )
            .with_help(
                "run `mm target setup plan` and apply it so every node runs the same version",
            ),
        );
    }
    if differ(&commit) {
        diagnostics.push(
            Diagnostic::warning(
                "MM_NODE_COMMIT_SKEW",
                format!(
                    "nodes of '{target}' last applied different commits: {}",
                    each(&commit, "nothing")
                ),
            )
            .with_help("run `mm deploy plan` and apply it to bring every node to one commit"),
        );
    }
    diagnostics
}

#[cfg(test)]
#[path = "cluster_tests.rs"]
mod tests;
