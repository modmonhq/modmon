//! The networks that span a cluster target: what every node holds, and the
//! addresses a deployment plan gives out.
//!
//! Each node stores the table it last applied. The client reads all of
//! them, merges them, and gives a network that a node newly needs the
//! lowest free addresses, so projects that share the hosts never collide.

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode, in_node};
use crate::domain::cluster_network::{
    AddressTable, ClusterNetworkPlan, NetworkReport, NodeNetworkReport, NodeNetworkStatus,
    PlannedNode, SealedNetworks, environment_network, plan_networks_retiring, range_conflict,
};
use crate::domain::error::MmError;
use crate::domain::remote::NodeDeploymentPlan;

impl RemoteEnvironment {
    /// Makes the target's mm networks span its nodes: `settings` names the
    /// address space, the mesh interface, and the MTU; its table is empty.
    #[must_use]
    pub fn with_cluster_networks(mut self, settings: Option<SealedNetworks>) -> Self {
        self.cluster_networks = settings;
        self
    }

    /// The name in `mm.yaml` of the network `key` names, when it is this
    /// environment's, told from the networks of an environment whose name
    /// starts like this one's by the project's environments.
    fn ours(&self, key: &str) -> Option<String> {
        let scope = &self.first().scope;
        environment_network(
            key,
            scope.project_id,
            &scope.environment,
            &self.environments,
        )
    }

    /// The project's environments: a deployment plan then takes away the
    /// networks of this environment that no service of it uses any more.
    #[must_use]
    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments = environments;
        self
    }

    async fn node_networks(&self, node: &RemoteNode) -> Result<NodeNetworkStatus, MmError> {
        match &node.transport {
            Ok(transport) => transport.networks(node.scope.clone()).await,
            Err(unavailable) => Err(unavailable.error()),
        }
    }

    /// The cluster networks as every node holds them, with what differs on
    /// a node from what it stored. Never fails: a node that does not answer
    /// is a row that says why.
    pub async fn network_report(&self) -> NetworkReport {
        let environment = self.first().scope.environment.clone();
        if self.cluster_networks.is_none() || !self.is_cluster() {
            return NetworkReport::new(self.target.clone(), &environment, Vec::new());
        }
        let answers = join_all(self.nodes.iter().map(|node| self.node_networks(node))).await;
        let nodes = self
            .nodes
            .iter()
            .zip(answers)
            .map(|(node, answer)| {
                let name = self.name_of(node).to_owned();
                match answer {
                    Ok(status) => NodeNetworkReport {
                        name,
                        reachable: true,
                        problem: None,
                        status: Some(status),
                    },
                    Err(MmError::Unreachable { problem, help, .. }) => NodeNetworkReport {
                        name,
                        reachable: false,
                        problem: Some(format!("{problem}. {help}")),
                        status: None,
                    },
                    Err(error) => NodeNetworkReport {
                        name,
                        reachable: false,
                        problem: Some(error.to_string()),
                        status: None,
                    },
                }
            })
            .collect();
        NetworkReport::new(self.target.clone(), &environment, nodes)
    }

    /// The addresses of the selected environment's networks: the block of
    /// the cluster's address space each one has, each node's slice of it,
    /// and the addresses and routes every node already uses. It asks what
    /// [`Self::network_report`] asks, one request per node and all at once,
    /// and never fails. Empty for a target whose networks stay local.
    pub async fn address_table(&self) -> AddressTable {
        let Some(settings) = self.cluster_networks.as_ref().filter(|_| self.is_cluster()) else {
            return AddressTable::default();
        };
        let report = self.network_report().await;
        AddressTable::from_reports(Some(settings.space), &|key| self.ours(key), &report.nodes)
    }

    /// What every node of the target stored for the cluster's networks, in
    /// declared order; `None` when the target's networks stay local.
    pub(super) async fn stored_networks(&self) -> Option<Vec<Result<NodeNetworkStatus, MmError>>> {
        if self.cluster_networks.is_none() || !self.is_cluster() {
            return None;
        }
        Some(join_all(self.nodes.iter().map(|node| self.node_networks(node))).await)
    }

    /// The network section of a deployment plan over `selected`, from what
    /// each of their plans says its services use and what every node of the
    /// target `stored` (`stored_networks`). A node that is left out of the
    /// plan and did not answer is planned around.
    pub(super) fn plan_cluster_networks(
        &self,
        selected: &[&RemoteNode],
        plans: &[NodeDeploymentPlan],
        stored: Option<Vec<Result<NodeNetworkStatus, MmError>>>,
    ) -> Result<Option<ClusterNetworkPlan>, MmError> {
        let (Some(settings), Some(answers)) = (
            self.cluster_networks.as_ref().filter(|_| self.is_cluster()),
            stored,
        ) else {
            return Ok(None);
        };
        let mut known = Vec::new();
        let mut planned = Vec::new();
        let mut held = Vec::new();
        for (node, answer) in self.nodes.iter().zip(answers) {
            let name = self.name_of(node).to_owned();
            let section = selected
                .iter()
                .position(|chosen| std::ptr::eq(*chosen, node))
                .and_then(|index| plans.get(index));
            let status = match (answer, section) {
                (Ok(status), _) => status,
                (Err(error), Some(_)) => return Err(in_node(node.name.as_deref(), error)),
                // Left out with --nodes and silent: what it holds is known
                // from the tables of the nodes that answer.
                (Err(_), None) => continue,
            };
            if let Some(stored) = &status.stored {
                known.push((name.clone(), stored.table.clone()));
            }
            let Some(section) = section else {
                continue;
            };
            held.push((name.clone(), status.host_ranges.clone()));
            planned.push(PlannedNode {
                node: name,
                uses: section
                    .plan
                    .plan
                    .as_ref()
                    .and_then(|plan| plan.node.as_ref())
                    .map(|node| node.networks.clone())
                    .unwrap_or_default(),
                drift: status
                    .drift
                    .iter()
                    .map(|drift| drift.detail.clone())
                    .collect(),
                stored: status.stored,
            });
        }
        // Before any address is given out: a node that already uses part
        // of the space would send its containers' traffic there.
        if let Some(message) = held.iter().find_map(|(node, _)| {
            range_conflict(&self.target, node, &held, None, Some(settings.space))
        }) {
            return Err(MmError::Target { message });
        }
        let ours = |key: &str| {
            if self.environments.is_empty() {
                return None;
            }
            self.ours(key)
        };
        plan_networks_retiring(settings, &known, &planned, &ours).map_err(|message| {
            MmError::Target {
                // The message names the key to set, as the one about a range
                // a node already uses does.
                message: format!(
                    "target '{}': {}",
                    self.target,
                    message.replace(
                        crate::domain::cluster_network::UNDER_THE_TARGET,
                        &format!("under targets.{}.cluster", self.target)
                    )
                ),
            }
        })
    }
}

impl RemoteEnvironment {
    /// What the names of the environment resolve to, from every node that
    /// answers (one that does not is left out). Empty on a target that
    /// names one host.
    pub async fn name_report(&self) -> crate::domain::discovery::NameReport {
        if !self.is_cluster() {
            return crate::domain::discovery::NameReport::default();
        }
        let answers = join_all(self.nodes.iter().map(|node| async move {
            let answer = match node.connect() {
                Ok(transport) => transport.names(node.scope.clone()).await,
                Err(error) => Err(error),
            };
            (self.name_of(node).to_owned(), answer)
        }))
        .await;
        crate::domain::discovery::NameReport::merged(
            answers
                .into_iter()
                .filter_map(|(name, answer)| match answer {
                    Ok(view) => Some((name, view)),
                    // A node that does not answer says nothing here; the
                    // status and `mm target nodes` name it.
                    Err(error) => {
                        tracing::debug!(node = %name, %error, "a node did not say what it resolves");
                        None
                    }
                })
                .collect(),
        )
    }

    /// `mm deploy status` (and MCP's read of the deployments): what every
    /// host remembers of its deployments, and the services the nodes
    /// answer while none of their containers is healthy.
    pub async fn deployment_report(&self) -> crate::domain::rollout::DeploymentStatus {
        let (mut status, names) = tokio::join!(self.deployment_status(), self.name_report());
        status.answered_unhealthy = names.answered_unhealthy();
        status
    }
}
