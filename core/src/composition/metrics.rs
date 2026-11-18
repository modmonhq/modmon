//! Wiring for the container metrics `mmd` keeps about its own node.

use std::sync::Arc;

use crate::adapters::metrics::{CgroupCounters, EngineStats};
use crate::app::MetricsSampler;
use crate::app::ports::{EngineUsage, MeteredContainers};

/// The sampler that keeps the last four hours of CPU and memory of
/// `containers` in memory, once [`MetricsSampler::start`] is called; its
/// store is [`MetricsSampler::metrics`]. It reads the host's cgroup v2
/// files, and asks the engine only about a container those do not show.
/// It asks `engine`, the engine setup installed on the host. Outside a
/// runtime there is nothing to run engine commands on, and such a
/// container has no samples.
#[must_use]
pub fn container_metrics(
    containers: Arc<dyn MeteredContainers>,
    engine: crate::domain::runtime::RuntimeKind,
) -> Arc<MetricsSampler> {
    let engine = match tokio::runtime::Handle::try_current() {
        Ok(commands) => Some(Arc::new(EngineStats::new(
            super::RuntimeComposition::new().driver(engine),
            commands,
        )) as Arc<dyn EngineUsage>),
        Err(_) => None,
    };
    MetricsSampler::new(containers, Arc::new(CgroupCounters::host()), engine)
}
