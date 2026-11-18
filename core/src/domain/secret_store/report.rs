//! The managed store of a cluster environment as its members report it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{StoreStatus, store_findings};
use crate::domain::diagnostic::{Diagnostic, Severity};
use crate::domain::remote::NodeReport;

/// What `mm secret provider status` and MCP's read `secret_store` answer
/// for the store of a cluster environment: each member's status as its node
/// reports it to `mm target nodes` and `mm doctor`, and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterStoreReport {
    pub provider: String,
    pub environment: String,
    pub target: String,
    /// The OpenBao release the configuration names.
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leader: Option<String>,
    pub members: Vec<ClusterStoreMember>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

/// One member, as its node reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ClusterStoreMember {
    pub node: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StoreStatus>,
    /// Why the node reported no member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

impl ClusterStoreReport {
    /// The store's `members`, from the reports of the target's nodes.
    #[must_use]
    pub fn of(
        (provider, environment, target): (&str, &str, &str),
        version: &str,
        members: &[String],
        reports: &[NodeReport],
    ) -> Self {
        let members = members
            .iter()
            .map(|name| {
                let report = reports.iter().find(|report| report.name == *name);
                let status = report.and_then(|report| report.secret_store.clone());
                let problem = match (report, &status) {
                    (_, Some(_)) => None,
                    (None, None) => Some("the target names no such node".to_owned()),
                    (Some(report), None) if !report.reachable => Some(format!(
                        "the node does not answer: {}",
                        report.problem.as_deref().unwrap_or("no reason given")
                    )),
                    (Some(_), None) => Some("the node runs no member of the store".to_owned()),
                };
                ClusterStoreMember {
                    node: name.clone(),
                    status,
                    problem,
                }
            })
            .collect::<Vec<_>>();
        let reported = members
            .iter()
            .filter_map(|member| Some((member.node.as_str(), member.status.as_ref()?)))
            .collect::<Vec<_>>();
        let absent = members
            .iter()
            .filter_map(|member| Some((member.node.as_str(), member.problem.as_deref()?)))
            .collect::<Vec<_>>();
        let diagnostics = store_findings(target, &reported, &absent);
        let leader = reported
            .iter()
            .find_map(|(_, status)| status.leader.clone());
        Self {
            provider: provider.to_owned(),
            environment: environment.to_owned(),
            target: target.to_owned(),
            version: version.to_owned(),
            leader,
            members,
            diagnostics,
        }
    }

    /// Every member reported, and nothing is wrong.
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.members.iter().all(|member| member.status.is_some())
            && self
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != Severity::Error)
    }
}
