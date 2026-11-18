//! The OCI runtimes the hosts of a remote target offer, tried on each.

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode};
use crate::domain::error::MmError;
use crate::domain::hardening::NodeRuntimeReport;

impl RemoteEnvironment {
    /// Every host of the target with the runtimes its policy offers and
    /// whether its engine ran a container under each. Never fails: a host
    /// that does not answer is a row that says why. A host that offers no
    /// runtime answers at once.
    pub async fn runtime_reports(&self) -> Vec<NodeRuntimeReport> {
        join_all(self.nodes.iter().map(|node| self.runtime_report(node))).await
    }

    async fn runtime_report(&self, node: &RemoteNode) -> NodeRuntimeReport {
        let answer = match &node.transport {
            Ok(transport) => transport.runtimes(node.scope.clone()).await,
            Err(unavailable) => Err(unavailable.error()),
        };
        let name = self.name_of(node).to_owned();
        match answer {
            Ok(status) => NodeRuntimeReport {
                name,
                reachable: true,
                problem: None,
                status: Some(status),
            },
            Err(MmError::Unreachable { problem, help, .. }) => NodeRuntimeReport {
                name,
                reachable: false,
                problem: Some(format!("{problem}. {help}")),
                status: None,
            },
            // Reached, and it could not say: an older `mmd`, a broken policy.
            Err(error) => NodeRuntimeReport {
                name,
                reachable: true,
                problem: Some(error.to_string()),
                status: None,
            },
        }
    }
}
