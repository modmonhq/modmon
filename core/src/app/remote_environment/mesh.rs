//! `mm target mesh status`: every node's WireGuard interface, read through
//! each node's `mmd`.

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode};
use crate::domain::error::MmError;
use crate::domain::mesh::{MeshNodeReport, MeshReport};

impl RemoteEnvironment {
    /// The mesh as every node sees it, with what is wrong and what to do.
    /// Never fails: a node that does not answer is a row that says why.
    pub async fn mesh_report(&self) -> MeshReport {
        if let Some(interface) = &self.existing_interface {
            return MeshReport::on_existing_interface(self.target.clone(), interface.clone());
        }
        let nodes = join_all(self.nodes.iter().map(|node| self.mesh_node(node))).await;
        MeshReport::new(self.target.clone(), nodes)
    }

    async fn mesh_node(&self, node: &RemoteNode) -> MeshNodeReport {
        let mut report = MeshNodeReport {
            name: self.name_of(node).to_owned(),
            host: node
                .repo_address
                .as_ref()
                .map_or_else(|| node.host.clone(), |(host, _)| host.clone()),
            reachable: true,
            problem: None,
            status: None,
        };
        let answer = match &node.transport {
            Ok(transport) => transport.mesh_status(node.scope.clone()).await,
            Err(unavailable) => Err(unavailable.error()),
        };
        match answer {
            Ok(status) => report.status = Some(status),
            Err(MmError::Unreachable { problem, help, .. }) => {
                report.reachable = false;
                report.problem = Some(format!("{problem}. {help}"));
            }
            Err(error) => {
                report.reachable = false;
                report.problem = Some(error.to_string());
            }
        }
        report
    }
}
