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

/// What the node serves at `/metrics` (`domain::node_metrics`), once
/// [`MetricsEndpoint::run`](crate::adapters::node_metrics::MetricsEndpoint::run)
/// is spawned: on the networks whose `metrics: true` asks for it, and on
/// the mesh address when `/etc/mm/host.yaml` says so.
#[must_use]
pub fn metrics_endpoint(
    config: &crate::adapters::remote::DaemonConfig,
) -> Arc<crate::adapters::node_metrics::MetricsEndpoint> {
    use crate::adapters::node_metrics::{
        EndpointSources, EngineServiceContainers, InterfacePeers, MetricsEndpoint, NodeDeployments,
        ProcHostStats,
    };
    let engine = config.host_runtime();
    let identity = crate::app::NodeIdentity {
        node: config
            .node_env()
            .ok()
            .flatten()
            .map(|node| node.name)
            .unwrap_or_default(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        engine: engine.name().to_owned(),
    };
    let sources = crate::app::NodeMetricsSources {
        host: Arc::new(ProcHostStats::host()),
        containers: Arc::new(EngineServiceContainers::new(super::engine_api::host_api(
            engine,
        ))),
        mesh: Arc::new(InterfacePeers::new(Arc::new(
            crate::adapters::mesh::WireguardMeshHost::new(
                &config.state_root,
                Arc::new(crate::adapters::process::TokioProcessRunner),
            ),
        ))),
        deployed: Arc::new(NodeDeployments::new(
            &config.state_root,
            crate::adapters::remote::HostAccessStore::new(config),
            Arc::new(crate::adapters::config::ConfigLoader),
        )),
    };
    MetricsEndpoint::new(
        crate::app::NodeMetricsCollector::new(identity, sources),
        EndpointSources {
            host_policy: config.host_policy_path.clone(),
            networks: super::cluster_networks(config),
            mesh: super::mesh_host(config),
        },
    )
}
