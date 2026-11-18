use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::diagnostic::ValidationReport;
use crate::domain::operation::OperationSummary;
use crate::domain::runtime::{RuntimeKind, RuntimeProbe, ServiceSnapshot};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct ProjectId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct WorkspaceId(pub Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceRegistration {
    pub project_id: ProjectId,
    pub root: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceRecord {
    pub id: WorkspaceId,
    pub project_id: ProjectId,
    pub root: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceSnapshot {
    pub workspace: WorkspaceRecord,
    pub project_name: String,
    pub target: String,
    pub environment: String,
    pub config_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<RuntimeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_source: Option<String>,
    pub runtime: RuntimeProbe,
    pub validation: ValidationReport,
    pub stacks: Vec<StackSnapshot>,
    #[serde(default)]
    pub ingress: Vec<crate::domain::ingress::IngressServiceStatus>,
    pub recent_operations: Vec<OperationSummary>,
    /// On a cluster target: every node that was asked, in declared order,
    /// with the reason for each one that did not answer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeStatus>,
    /// On a cluster target: what each service name resolves to on each
    /// network that spans the nodes, as the nodes' resolvers answer it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<crate::domain::discovery::NameReport>,
    /// On a remote host: the deployment it last finished, which is what its
    /// containers were started from. On a cluster target each node has its
    /// own under `nodes`, and this is set when they all agree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment: Option<AppliedDeployment>,
    /// On a remote host: what runs there that differs from that
    /// deployment. On a cluster target each node has its own under `nodes`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drift: Vec<String>,
    /// On a node of a cluster target, as the node answers: what its
    /// cluster networks have that differs from what the deployment stored.
    /// A merged status carries it per node under `nodes`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_drift: Vec<String>,
    /// In the development environment: the `development: optional` units
    /// and whether each runs on this machine.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional: Vec<crate::domain::development_units::OptionalUnit>,
    /// On an ingress node of a cluster target whose environment comes in
    /// through a Cloudflare Tunnel: what its connector says. A merged
    /// status carries it per node under `nodes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<crate::domain::cluster_ingress::TunnelStatus>,
}

/// The deployment a host last finished: the receipt of what it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppliedDeployment {
    pub commit: String,
    pub deployed_at_unix_ms: i64,
    pub deployed_by: String,
    /// The reviewed plan that was applied; every node of one deployment
    /// has the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_hash: Option<String>,
    /// The build receipt whose images the deployment runs, when it was
    /// planned with one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_receipt: Option<String>,
}

impl AppliedDeployment {
    /// Whether two hosts run the same release: the same commit with the
    /// same images, whoever deployed it and when.
    #[must_use]
    pub fn same_release(&self, other: &Self) -> bool {
        self.commit == other.commit && self.build_receipt == other.build_receipt
    }
}

/// Whether one node of a cluster target answered a status request.
// Every field is written every time, so a reader finds the same keys on
// every node: `problem` and `deployment` are `null` and the drift lists
// are empty when there is nothing to say. Reading stays tolerant of a
// missing key. (Plain comments: a doc comment would change the schema.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NodeStatus {
    pub name: String,
    pub reachable: bool,
    #[serde(default)]
    pub problem: Option<String>,
    /// What the node's cluster networks have that differs from what its
    /// last deployment stored: a missing firewall rule or route, a network
    /// on other addresses.
    #[serde(default)]
    pub network_drift: Vec<String>,
    /// The deployment the node last finished.
    #[serde(default)]
    pub deployment: Option<AppliedDeployment>,
    /// What runs on the node that differs from that deployment: a service
    /// it places there without a container, a service that runs there
    /// although it places it elsewhere.
    #[serde(default)]
    pub drift: Vec<String>,
    /// On an ingress node whose environment comes in through a Cloudflare
    /// Tunnel: what its connector says.
    #[serde(default)]
    pub tunnel: Option<crate::domain::cluster_ingress::TunnelStatus>,
}

impl NodeStatus {
    /// A node that was asked and has neither answered nor failed yet. Only
    /// a status that is still being read lists one; a finished status has a
    /// problem for every node that is not reachable.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        !self.reachable && self.problem.is_none()
    }

    /// The row of a node that has not answered yet.
    #[must_use]
    pub fn pending(name: String) -> Self {
        Self {
            name,
            reachable: false,
            problem: None,
            network_drift: Vec::new(),
            deployment: None,
            drift: Vec::new(),
            tunnel: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StackSnapshot {
    pub name: String,
    pub compose_project_name: String,
    pub artifact_hash: String,
    pub rendered_config_hash: String,
    pub services: Vec<ServiceSnapshot>,
    #[serde(default)]
    pub owned_resources: Vec<OwnedResourceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OwnedResourceSnapshot {
    pub kind: crate::domain::ownership::ResourceKind,
    pub logical_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DoctorReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<crate::domain::readiness::ReadinessReport>,
    pub project_name: Option<String>,
    pub root: Option<PathBuf>,
    pub target: Option<String>,
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<RuntimeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_delivery:
        Option<crate::domain::environment_delivery::ResolvedEnvironmentDelivery>,
    #[serde(default)]
    pub project_selected: bool,
    pub runtime: RuntimeProbe,
    pub validation: ValidationReport,
    #[serde(default)]
    pub ingress: Vec<crate::domain::ingress::IngressServiceStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<crate::domain::release::UpdateCheckReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ValidationSnapshot {
    pub project_name: String,
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub target: String,
    pub environment: String,
    pub config_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<RuntimeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_delivery:
        Option<crate::domain::environment_delivery::ResolvedEnvironmentDelivery>,
    pub validation: ValidationReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InitializedProject {
    pub project_name: String,
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub project_id: ProjectId,
    pub starter_files: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mount_proposals: Vec<crate::domain::development_mount::DevelopmentMountProposal>,
}
