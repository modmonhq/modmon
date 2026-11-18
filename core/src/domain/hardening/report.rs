//! Whether each host really has the OCI runtimes its policy offers, as
//! `mm doctor` asks.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::Diagnostic;

/// One runtime of a host's `runtimes:`, tried on that host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RuntimeCheck {
    pub name: String,
    /// Whether the engine ran a container under it.
    pub works: bool,
    /// Why not, and what to do on the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// What a host's `mmd` says about the OCI runtimes of its policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HostRuntimeStatus {
    /// Each runtime the host offers, tried.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtimes: Vec<RuntimeCheck>,
    /// The runtime the host puts services that publish a port or take
    /// ingress traffic under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposed_runtime: Option<String>,
}

/// One host in a runtime report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeRuntimeReport {
    /// The node's name, or the target's for a target that names one host.
    pub name: String,
    /// Whether the host's `mmd` was reached.
    pub reachable: bool,
    /// Why there is no status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<HostRuntimeStatus>,
}

/// One finding per runtime a host offers and cannot run, and one per host
/// that was reached and could not check. A host that was not reached is
/// not a finding here: the node list reports it.
#[must_use]
pub fn runtime_diagnostics(nodes: &[NodeRuntimeReport]) -> Vec<Diagnostic> {
    let unchecked = nodes
        .iter()
        .filter(|node| node.reachable && node.status.is_none())
        .map(|node| {
            Diagnostic::warning(
                "MM_RUNTIME_NOT_CHECKED",
                format!(
                    "host '{}' did not say whether the runtimes its policy offers work: {}",
                    node.name,
                    node.problem.as_deref().unwrap_or("no reason given")
                ),
            )
            .with_help(
                "`mmd access validate` on the host checks its policy; `journalctl -u mmd` says what failed",
            )
        });
    nodes
        .iter()
        .flat_map(|node| {
            node.status
                .iter()
                .flat_map(|status| &status.runtimes)
                .filter(|check| !check.works)
                .map(|check| {
                    Diagnostic::error(
                        "MM_RUNTIME_NOT_WORKING",
                        format!(
                            "host '{}' offers the runtime '{}' in its policy and cannot run a container under it",
                            node.name, check.name
                        ),
                    )
                    .with_help(check.problem.clone().unwrap_or_else(|| {
                        "install the runtime on the host or take it out of `runtimes:` in /etc/mm/host.yaml".to_owned()
                    }))
                })
        })
        .chain(unchecked)
        .collect()
}
