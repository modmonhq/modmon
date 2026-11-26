//! One collection of what a node serves at `/metrics`, shared by every
//! scrape for a few seconds (`domain::node_metrics`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::app::ports::{DeployedEnvironments, HostStats, MeshPeers, NodeContainers};
use crate::domain::node_metrics::procfs::HostSample;
use crate::domain::node_metrics::{
    CACHE_FOR, EnvironmentSample, NodeSnapshot, PeerSample, environment_samples,
};

/// How long the host's files may take to read.
const HOST_PATIENCE: Duration = Duration::from_secs(5);
/// How long the engine may take to list and inspect the containers.
const ENGINE_PATIENCE: Duration = Duration::from_secs(10);
/// How long the mesh interface may take to answer.
const MESH_PATIENCE: Duration = Duration::from_secs(5);
/// How long the node's records may take to read.
const RECORDS_PATIENCE: Duration = Duration::from_secs(5);

/// Who the node is, for every series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeIdentity {
    pub node: String,
    pub version: String,
    pub engine: String,
}

/// Where a collection reads from.
pub struct NodeMetricsSources {
    pub host: Arc<dyn HostStats>,
    pub containers: Arc<dyn NodeContainers>,
    pub mesh: Arc<dyn MeshPeers>,
    pub deployed: Arc<dyn DeployedEnvironments>,
}

pub struct NodeMetricsCollector {
    identity: NodeIdentity,
    sources: NodeMetricsSources,
    cached: tokio::sync::Mutex<Option<(Instant, Arc<NodeSnapshot>)>>,
    reading_host: Arc<AtomicBool>,
    reading_records: Arc<AtomicBool>,
}

impl std::fmt::Debug for NodeMetricsCollector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeMetricsCollector")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl NodeMetricsCollector {
    #[must_use]
    pub fn new(identity: NodeIdentity, sources: NodeMetricsSources) -> Arc<Self> {
        Arc::new(Self {
            identity,
            sources,
            cached: tokio::sync::Mutex::new(None),
            reading_host: Arc::new(AtomicBool::new(false)),
            reading_records: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The newest collection, collected now when the last one is older
    /// than [`CACHE_FOR`]. Scrapes that arrive meanwhile wait for it, so
    /// only one collection runs at a time.
    pub async fn snapshot(&self) -> Arc<NodeSnapshot> {
        let mut cached = self.cached.lock().await;
        if let Some((at, snapshot)) = cached.as_ref()
            && at.elapsed() < CACHE_FOR
        {
            return Arc::clone(snapshot);
        }
        let snapshot = Arc::new(self.collect().await);
        *cached = Some((Instant::now(), Arc::clone(&snapshot)));
        snapshot
    }

    /// What each environment's last deployment runs here, read the way a
    /// collection reads it.
    pub async fn deployed(&self) -> Option<Vec<crate::domain::node_metrics::DeployedEnvironment>> {
        let deployed = Arc::clone(&self.sources.deployed);
        let read = blocking(&self.reading_records, RECORDS_PATIENCE, move || {
            deployed.deployed()
        })
        .await?;
        read.inspect_err(|error| tracing::debug!(%error, "cannot read what the node's deployments run, for its metrics"))
            .ok()
    }

    async fn collect(&self) -> NodeSnapshot {
        let started = Instant::now();
        let (host, peers, environments) =
            tokio::join!(self.host(), self.peers(), self.environments());
        NodeSnapshot {
            node: self.identity.node.clone(),
            version: self.identity.version.clone(),
            engine: self.identity.engine.clone(),
            collect_seconds: started.elapsed().as_secs_f64(),
            host,
            peers,
            environments,
        }
    }

    async fn host(&self) -> Option<HostSample> {
        let host = Arc::clone(&self.sources.host);
        blocking(&self.reading_host, HOST_PATIENCE, move || host.read()).await
    }

    async fn peers(&self) -> Option<Vec<PeerSample>> {
        match tokio::time::timeout(MESH_PATIENCE, self.sources.mesh.peers()).await {
            Ok(Ok(peers)) => peers,
            Ok(Err(error)) => {
                tracing::debug!(%error, "cannot read the mesh peers for the node's metrics");
                None
            }
            Err(_) => None,
        }
    }

    async fn environments(&self) -> Option<Vec<EnvironmentSample>> {
        let (deployed, containers) = tokio::join!(
            self.deployed(),
            tokio::time::timeout(ENGINE_PATIENCE, self.sources.containers.containers())
        );
        let containers = match containers {
            Ok(Ok(containers)) => containers,
            Ok(Err(error)) => {
                tracing::debug!(%error, "cannot list the node's containers for its metrics");
                return None;
            }
            Err(_) => return None,
        };
        Some(environment_samples(&deployed?, &containers))
    }
}

/// Runs `read` on a thread of the blocking pool and waits at most
/// `patience`. A read that is still running when the next one is due is
/// not started again: `busy` stays set until it returns.
async fn blocking<T: Send + 'static>(
    busy: &Arc<AtomicBool>,
    patience: Duration,
    read: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    if busy.swap(true, Ordering::AcqRel) {
        return None;
    }
    let done = Free(Arc::clone(busy));
    let task = tokio::task::spawn_blocking(move || {
        let _done = done;
        read()
    });
    tokio::time::timeout(patience, task).await.ok()?.ok()
}

struct Free(Arc<AtomicBool>);

impl Drop for Free {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
