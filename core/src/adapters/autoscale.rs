//! The autoscaler `mmd` runs on a cluster node (`domain::autoscale`).
//!
//! Every step of the metrics store (5 s) it looks at each autoscaled
//! service the node's last deployment placed here:
//!
//! 1. what the node runs of it (one read of the engine's API), and the
//!    newest count it knows of, its own on disk or a peer's;
//! 2. one step toward its own share of that count: one container started
//!    (Compose, with the deployment's files and environment, through the
//!    daemon) or one container leaving (`domain::leaving`), never both and
//!    never a second before the first is done;
//! 3. what it tells the other nodes: the sum of its containers' newest
//!    samples, how many run, whether it is settled, and the count;
//! 4. on the decider only: the cluster's step in the metrics store, and a
//!    new count when a rule has held for its `for:`.
//!
//! Nothing here is kept but the count (one small file per service), so an
//! `mmd` that restarts carries on from the count, and its rules wait one
//! whole `for:` because the metrics store starts empty.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::adapters::discovery::NameResolver;
use crate::adapters::engine_api::EngineApi;
use crate::adapters::leaving::LeaveKeeper;
use crate::app::ports::{Metrics, ScaleState};
use crate::domain::autoscale::{ScaleChange, ScaleReport, ScaledService, ServiceScale};
use crate::domain::error::MmError;
use crate::domain::remote::RemoteScope;

mod service;
pub mod store;

pub use store::DeployedScaling;

/// Where a node keeps what its autoscaler needs, under the daemon's state
/// directory.
pub const SCALE_DIRECTORY: &str = "scale";
/// One step of the metrics store.
const STEP: Duration = Duration::from_secs(crate::domain::metrics::STEP_SECONDS);
/// How long the engine may take to list a service's containers.
const ENGINE_PATIENCE: Duration = Duration::from_secs(10);

/// What the autoscaler asks of the daemon it runs in.
#[async_trait::async_trait]
pub trait ScaleHost: Send + Sync {
    /// Whether a deployment holds the environment of `scope` now: nothing
    /// is started or stopped then.
    fn deploying(&self, scope: &RemoteScope) -> bool;

    /// Has the engine count `count` containers of `service`, starting the
    /// ones it misses as the deployment `deployed` runs the service.
    async fn start(
        &self,
        deployed: &DeployedScaling,
        service: &ScaledService,
        count: u32,
    ) -> Result<(), MmError>;

    /// Writes a change of the count to the daemon's audit journal.
    fn audit(&self, deployed: &DeployedScaling, service: &ScaledService, change: &ScaleChange);
}

/// What the autoscaler remembers of one service between steps.
#[derive(Debug, Default)]
struct Tracked {
    /// Since when this node decides for the service.
    deciding_since: Option<Instant>,
    /// Since when every answering node runs its share, settled.
    settled_since: Option<Instant>,
    /// The engine ids of the service's containers leaving this node.
    leaving: Vec<String>,
    /// What `mm service status` shows of the service.
    status: Option<ServiceScale>,
}

pub struct Autoscaler {
    directory: PathBuf,
    state_root: PathBuf,
    leaving_directory: PathBuf,
    names: Arc<NameResolver>,
    metrics: Arc<dyn Metrics>,
    leaves: Arc<LeaveKeeper>,
    engine: Option<EngineApi>,
    tracked: Mutex<BTreeMap<String, Tracked>>,
}

impl std::fmt::Debug for Autoscaler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Autoscaler")
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

impl Autoscaler {
    #[must_use]
    pub fn new(
        state_root: &Path,
        names: Arc<NameResolver>,
        metrics: Arc<dyn Metrics>,
        leaves: Arc<LeaveKeeper>,
        engine: Option<EngineApi>,
    ) -> Arc<Self> {
        Arc::new(Self {
            directory: state_root.join(SCALE_DIRECTORY),
            state_root: state_root.to_path_buf(),
            leaving_directory: state_root.join(crate::adapters::leaving::LEAVING_DIRECTORY),
            names,
            metrics,
            leaves,
            engine,
            tracked: Mutex::new(BTreeMap::new()),
        })
    }

    /// Where the daemon keeps what each finished deployment scales.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn tracked(&self) -> MutexGuard<'_, BTreeMap<String, Tracked>> {
        self.tracked.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Scales until the process ends.
    pub async fn run(self: Arc<Self>, host: Arc<dyn ScaleHost>) {
        let mut tick = tokio::time::interval(STEP);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            self.step(host.as_ref()).await;
        }
    }

    /// One look at every autoscaled service of the node.
    pub async fn step(&self, host: &dyn ScaleHost) {
        let deployed = match store::deployed(&self.directory) {
            Ok(deployed) => deployed,
            Err(error) => {
                tracing::warn!(%error, "cannot read what the node's deployments scale");
                return;
            }
        };
        let peers = self.names.peer_scaling();
        let mut reports = Vec::new();
        let mut keys = BTreeSet::new();
        for deployment in &deployed {
            for service in deployment
                .services
                .iter()
                .filter(|service| service.autoscale.is_some())
            {
                keys.insert(service.key());
                match self.service_step(host, deployment, service, &peers).await {
                    Ok(report) => reports.push(report),
                    Err(error) => tracing::warn!(
                        service = %service.key(),
                        %error,
                        "the autoscaler could not look at a service"
                    ),
                }
            }
        }
        self.names.publish_scaling(reports);
        if let Err(error) = store::sweep_wants(&self.directory, &deployed) {
            tracing::warn!(%error, "cannot remove the counts of services no longer autoscaled here");
        }
        // A service no longer deployed here frees what was kept of it.
        let mut tracked = self.tracked();
        let gone = tracked
            .keys()
            .filter(|key| !keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in gone {
            tracked.remove(&key);
            self.metrics.forget_service(&key);
        }
    }

    /// What `mm service status` shows of the service `key`.
    #[must_use]
    pub fn status(&self, key: &str) -> Option<ServiceScale> {
        self.tracked()
            .get(key)
            .and_then(|tracked| tracked.status.clone())
    }

    /// Every container of `service` on this node, as the engine shows it.
    async fn containers(
        &self,
        service: &ScaledService,
    ) -> Result<Vec<crate::domain::autoscale::OwnContainer>, MmError> {
        let engine = self.engine.as_ref().ok_or_else(|| MmError::Environment {
            message: "the node's Docker engine has no Unix socket this daemon can read: DOCKER_HOST or the Docker context of mmd's environment names another engine; remove it from mmd's environment".to_owned(),
        })?;
        let listed = tokio::time::timeout(
            ENGINE_PATIENCE,
            crate::adapters::runtime::engine_containers::project_containers(
                engine,
                &service.compose_project,
                Some(&service.service),
            ),
        )
        .await
        .map_err(|_| MmError::RuntimeResponse {
            message: format!(
                "the engine did not list the containers of '{}' within {} s",
                service.service,
                ENGINE_PATIENCE.as_secs()
            ),
        })??;
        let leaves = crate::adapters::leaving::leaving_records(&self.state_root);
        let leave = |id: &str| leaves.iter().find(|leave| leave.id == id);
        Ok(listed
            .into_iter()
            .map(|container| container.observation)
            .filter(|container| !container.is_one_off())
            .map(|container| {
                let healthy = container
                    .health
                    .as_deref()
                    .is_none_or(|health| health.eq_ignore_ascii_case("healthy"));
                crate::domain::autoscale::OwnContainer::seen(
                    (
                        container.id.clone(),
                        container.created_at.clone().unwrap_or_default(),
                    ),
                    container.is_running(),
                    healthy,
                    leave(&container.id).map(|leave| leave.restart),
                )
            })
            .collect())
    }
}

impl ScaleState for Autoscaler {
    fn wanted(&self, key: &str) -> Option<u32> {
        match store::want(&self.directory, key) {
            Ok(want) => want.map(|want| want.count),
            Err(error) => {
                tracing::warn!(%error, service = key, "cannot read the count of a scaled service");
                None
            }
        }
    }

    fn leaving(&self, key: &str) -> u32 {
        let tracked = self.tracked();
        let ids = tracked.get(key).map_or(&[][..], |tracked| &tracked.leaving);
        let pending = ids
            .iter()
            .filter(|id| self.leaving_directory.join(format!("{id}.json")).exists())
            .count();
        u32::try_from(pending).unwrap_or(u32::MAX)
    }
}

/// The reports of the other nodes about `key`, by node, from those of
/// `nodes` heard from recently.
fn heard<'a>(
    peers: &'a [(String, Vec<ScaleReport>)],
    nodes: &[String],
    key: &str,
) -> Vec<(&'a str, &'a ScaleReport)> {
    peers
        .iter()
        .filter(|(node, _)| nodes.contains(node))
        .filter_map(|(node, reports)| {
            reports
                .iter()
                .find(|report| report.service == key)
                .map(|report| (node.as_str(), report))
        })
        .collect()
}
