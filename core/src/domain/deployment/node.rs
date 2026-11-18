//! What a deployment plan says about one node of a cluster target.
//!
//! A node renders one Compose project per stack with only the services
//! placed on it. This section of its plan lists those services, the ones
//! that leave the node, and where the node's project differs from what the
//! Compose files say because a service lives on another node.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::placement::PlacedService;

/// A service that has containers on the node and is no longer placed there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeavingService {
    pub stack: String,
    pub service: String,
    /// The Compose project on this node that holds the service's containers.
    pub compose_project_name: String,
    /// The nodes that run the service now. Empty when the service is gone
    /// from the environment. When it names nodes, the service moved: it
    /// keeps running here until those nodes have applied the plan.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves_to: Vec<String>,
}

impl LeavingService {
    #[must_use]
    pub fn moved(&self) -> bool {
        !self.moves_to.is_empty()
    }

    #[must_use]
    pub fn placed(&self) -> PlacedService {
        PlacedService {
            stack: self.stack.clone(),
            service: self.service.clone(),
        }
    }
}

/// A `depends_on` the node's engine cannot honour: the dependency runs on
/// other nodes, so it is left out of this node's project.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DroppedDependency {
    pub stack: String,
    pub service: String,
    pub depends_on: String,
    /// The nodes that run the dependency.
    pub nodes: Vec<String>,
}

/// One node's part of a deployment: what it runs after the plan, and what
/// stops running on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeProjectPlan {
    pub node: String,
    /// The services this node runs once the plan is applied.
    pub services: Vec<PlacedService>,
    /// Those of `services` that have no container here yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub starting: Vec<PlacedService>,
    /// Services with containers here that the plan does not place here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leaving: Vec<LeavingService>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped_dependencies: Vec<DroppedDependency>,
    /// The mm networks the node's services are on, and the engine's network
    /// of each name as it is now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<crate::domain::cluster_network::NetworkUse>,
    /// Services whose Compose files set no `restart:` policy. On a cluster
    /// node they get `unless-stopped`, so they come back when the node
    /// reboots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restart_defaults: Vec<PlacedService>,
    /// The shared volumes the node's services mount: each is its directory
    /// on the storage node, used directly there and mounted over the mesh
    /// everywhere else.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_volumes: Vec<crate::domain::shared_storage::SharedVolumeUse>,
    /// Set on a node that runs the ingress proxy for the environment's
    /// routes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<crate::domain::cluster_ingress::NodeIngress>,
    /// The node's services that set `replicas` or `autoscale`: how many
    /// containers each runs over its nodes, and so here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scaled: Vec<crate::domain::autoscale::ScaledService>,
    /// The node's services that set `health:`; every other one is
    /// restarted as [`crate::domain::healing::RestartPolicy::default`] says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub health: Vec<crate::domain::healing::ServiceHealth>,
}

impl NodeProjectPlan {
    /// The services that moved to other nodes and stop here only after
    /// those nodes have applied.
    pub fn moved(&self) -> impl Iterator<Item = &LeavingService> {
        self.leaving.iter().filter(|service| service.moved())
    }

    /// Whether a stack has a service that moved away and still runs here.
    #[must_use]
    pub fn stack_has_moved(&self, stack: &str) -> bool {
        self.moved().any(|service| service.stack == stack)
    }

    /// The scaling of one of the node's services, when it sets `replicas`
    /// or `autoscale`.
    #[must_use]
    pub fn scaled(
        &self,
        stack: &str,
        service: &str,
    ) -> Option<&crate::domain::autoscale::ScaledService> {
        self.scaled
            .iter()
            .find(|scaled| scaled.stack == stack && scaled.service == service)
    }

    /// What the node does when a container of the service keeps failing
    /// its health check; `None` when it does nothing.
    #[must_use]
    pub fn restart_policy(
        &self,
        stack: &str,
        service: &str,
    ) -> Option<crate::domain::healing::RestartPolicy> {
        self.health
            .iter()
            .find(|health| health.stack == stack && health.service == service)
            .map_or_else(
                || Some(crate::domain::healing::RestartPolicy::default()),
                |health| health.restart,
            )
    }

    /// The section as the plan's hash covers it: without the counts of the
    /// moment the plan was made, which change with load.
    #[must_use]
    pub fn hashed(&self) -> Self {
        let mut node = self.clone();
        for scaled in &mut node.scaled {
            scaled.now = None;
        }
        node
    }

    /// Whether the node runs any container of the environment after the
    /// plan is applied and before its moved services are stopped.
    #[must_use]
    pub fn keeps_containers(&self) -> bool {
        !self.services.is_empty() || self.moved().next().is_some()
    }
}
