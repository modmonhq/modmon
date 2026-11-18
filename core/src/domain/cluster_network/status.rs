//! What each node really has for the cluster's networks, against what it
//! stored at the last deployment: a missing rule or route, a bridge that
//! lost its settings, a network on other addresses.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::NodeNetworks;
use crate::domain::diagnostic::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkDriftKind {
    /// A rule of mm's firewall chains is missing or unexpected, or the
    /// chains are not entered.
    Firewall,
    /// A slice of another node is not routed through the mesh.
    Route,
    /// A network's bridge does not answer for the members on other nodes.
    Bridge,
    /// The engine's network is missing or has other addresses.
    Network,
}

/// One difference between a node and the state it stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NetworkDrift {
    pub kind: NetworkDriftKind,
    pub detail: String,
}

impl NetworkDrift {
    #[must_use]
    pub fn new(kind: NetworkDriftKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

/// One node's cluster networks, as its `mmd` reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeNetworkStatus {
    /// The node's own name from `/etc/mm/node.env`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// What the node stored at its last deployment; `None` on a node that
    /// never held a cluster network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored: Option<NodeNetworks>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drift: Vec<NetworkDrift>,
    /// The addresses and routes the node has outside mm, which the space
    /// of the cluster's networks must not overlap.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_ranges: Vec<super::HostRange>,
}

/// One node in a network report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeNetworkReport {
    pub name: String,
    /// Whether the node's `mmd` answered.
    pub reachable: bool,
    /// Why there is no status, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<NodeNetworkStatus>,
}

/// The cluster networks of a target as every node holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NetworkReport {
    pub target: String,
    pub nodes: Vec<NodeNetworkReport>,
    /// Each problem with the command that fixes it.
    pub problems: Vec<Diagnostic>,
}

impl NetworkReport {
    #[must_use]
    pub fn new(target: String, environment: &str, nodes: Vec<NodeNetworkReport>) -> Self {
        let problems = network_diagnostics(environment, &nodes);
        Self {
            target,
            nodes,
            problems,
        }
    }
}

/// One finding per node whose networks drifted, with what to run. A node
/// that did not answer is not a finding here: the node list reports it.
#[must_use]
pub fn network_diagnostics(environment: &str, nodes: &[NodeNetworkReport]) -> Vec<Diagnostic> {
    nodes
        .iter()
        .filter_map(|report| {
            let status = report.status.as_ref()?;
            if status.drift.is_empty() {
                return None;
            }
            let node = &report.name;
            let details = status
                .drift
                .iter()
                .map(|drift| drift.detail.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            Some(
                Diagnostic::error(
                    "MM_NETWORK_DRIFT",
                    format!(
                        "node '{node}': the cluster networks differ from what the last deployment stored: {details}"
                    ),
                )
                .with_help(format!(
                    "`mm deploy plan -e {environment}` shows the repair and applying it puts the node right; on {node}, `systemctl restart mm-network` restores rules, routes, and bridge settings from what mmd stored"
                )),
            )
        })
        .collect()
}
