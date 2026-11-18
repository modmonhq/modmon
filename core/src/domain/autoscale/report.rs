//! What the nodes keep, exchange and show about a scaled service.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ScalePolicy, share};
use crate::domain::metrics::{MetricReading, Tally};

/// The name a scaled service goes by between the nodes, on disk, and in the
/// metrics store: the same on every node. (A node's Compose project name is
/// its own, so it cannot be the name.)
#[must_use]
pub fn service_key(
    project: crate::domain::workspace::ProjectId,
    environment: &str,
    stack: &str,
    service: &str,
) -> String {
    format!("{}/{environment}/{stack}/{service}", project.0)
}

/// One change of the count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ScaleChange {
    pub from: u32,
    pub to: u32,
    /// Such as "cpu 82% above 75% for 30s".
    pub reason: String,
    pub at_unix_ms: i64,
}

/// How many containers the decider wants a service to run over all its
/// nodes. Every node keeps the newest one it heard of, on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Want {
    pub count: u32,
    /// One more with each change: the highest wins.
    pub generation: u64,
    /// The node that decided it.
    pub by: String,
    /// The change that set it; absent for the count a deployment started
    /// the service with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ScaleChange>,
}

/// What a node tells the other nodes about one scaled service, with every
/// exchange of its records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ScaleReport {
    /// [`service_key`].
    pub service: String,
    /// The sum and count of the newest samples of its containers here
    /// that are not leaving.
    pub tally: Tally,
    /// Its containers here that run and are not leaving.
    pub running: u32,
    /// The node runs its share, healthy, with nothing leaving.
    pub settled: bool,
    /// The newest want the node knows of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub want: Option<Want>,
}

/// A service whose number of containers is not one per node: it sets
/// `replicas` or `autoscale`. Part of each node's section of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ScaledService {
    /// [`service_key`].
    pub key: String,
    pub stack: String,
    pub service: String,
    /// The node's Compose project of the service's stack: its containers
    /// carry it as a label.
    pub compose_project: String,
    /// The service's placement, in the target's order.
    pub nodes: Vec<String>,
    /// Its containers over every node when nothing scales it.
    pub replicas: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoscale: Option<ScalePolicy>,
    /// How long one of its containers drains when it leaves.
    pub drain_ms: u64,
    /// The count the nodes went by when the plan was made. Not covered by
    /// the plan's hash: it changes with load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<u32>,
}

impl ScaledService {
    #[must_use]
    pub fn key(&self) -> String {
        self.key.clone()
    }

    #[must_use]
    pub const fn policy(&self) -> Option<&ScalePolicy> {
        self.autoscale.as_ref()
    }

    /// The count over every node, given the count the nodes last agreed
    /// on (`None` before anything scaled the service).
    #[must_use]
    pub fn count(&self, wanted: Option<u32>) -> u32 {
        match self.policy() {
            Some(policy) => policy.bounded(wanted.unwrap_or(policy.initial)),
            None => self.replicas,
        }
    }

    /// This node's containers of that count.
    #[must_use]
    pub fn share(&self, node: &str, wanted: Option<u32>) -> u32 {
        share(self.count(wanted), &self.nodes, node)
    }
}

/// How many containers a scaled service runs, and why: part of the
/// service's status, for `mm service status`, the terminal UI and MCP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceScale {
    /// The containers the nodes go by now, over every node.
    pub wanted: u32,
    /// The containers that run and are not leaving, on the nodes heard
    /// from.
    pub running: u32,
    pub min: u32,
    pub max: u32,
    /// Whether `autoscale` changes the count; without it `replicas` sets it.
    pub autoscale: bool,
    pub generation: u64,
    /// The node that decides now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_change: Option<ScaleChange>,
    /// Why the count does not change now, as the decider sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holding: Option<String>,
    /// The metric over the up rule's window, from the decider's store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<MetricReading>,
}

impl ServiceScale {
    /// The more informed of two nodes' answers: the newer count, and the
    /// decider's reading.
    #[must_use]
    pub fn merged(self, other: Self) -> Self {
        let rank = |scale: &Self| (scale.generation, scale.reading.is_some());
        let running = self.running.max(other.running);
        let mut best = if rank(&other) > rank(&self) {
            other
        } else {
            self
        };
        best.running = running;
        best
    }
}
