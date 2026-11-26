//! What `mmd` serves at `/metrics` on a cluster node, in the Prometheus
//! text format: the host, the node and its mesh, and the services of the
//! environments the node runs.
//!
//! A scrape sees what its [`MetricsScope`] allows: the host and the node
//! always, and the services of one environment, or of every environment
//! for the host's own admin.

use std::time::Duration;

use crate::domain::discovery::Health;
use crate::domain::workspace::ProjectId;

#[path = "node_metrics/exposition.rs"]
mod exposition;
#[path = "node_metrics/listeners.rs"]
mod listeners;
#[path = "node_metrics/procfs.rs"]
pub mod procfs;
#[path = "node_metrics/render.rs"]
mod render;
#[path = "node_metrics/request.rs"]
mod request;

pub use exposition::{Exposition, Kind};
pub use listeners::{MetricsListener, listeners};
pub use render::render;
pub use request::{MetricsRequest, RequestProblem, parse_request};

/// The TCP port `mmd` serves metrics on, on each address it serves them.
pub const METRICS_PORT: u16 = 9753;
/// How long one collection answers every scrape.
pub const CACHE_FOR: Duration = Duration::from_secs(5);
/// The most one answer holds.
pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// The most a request's line and headers may hold.
pub const MAX_REQUEST_BYTES: usize = 8 * 1024;
/// The most containers one collection reads.
pub const MAX_CONTAINERS: usize = 4_096;
/// The most filesystems, disks, or interfaces one collection reports.
pub const MAX_DEVICES: usize = 64;
/// A peer whose last handshake is older than this counts as down:
/// WireGuard renews a session every two minutes while traffic flows, and
/// the mesh keeps it flowing.
pub const PEER_DOWN_AFTER_SECONDS: u64 = 180;

/// Whose services a scrape sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetricsScope {
    /// One environment's, for a scrape on one of its cluster networks.
    Environment {
        project_id: ProjectId,
        environment: String,
    },
    /// Every environment's, for the host's admin on the mesh address.
    Host,
}

impl MetricsScope {
    #[must_use]
    pub fn sees(&self, project_id: ProjectId, environment: &str) -> bool {
        match self {
            Self::Environment {
                project_id: scoped,
                environment: scoped_environment,
            } => *scoped == project_id && scoped_environment == environment,
            Self::Host => true,
        }
    }
}

/// One container of an environment, as the engine shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelledContainer {
    pub project_id: ProjectId,
    pub environment: String,
    pub stack: String,
    pub service: String,
    pub health: Health,
    pub restarts: u64,
}

impl LabelledContainer {
    /// From one document of the engine's container inspect. `None` for a
    /// container mm did not start as a service: no project, environment,
    /// stack, or Compose service label, or a one-off `run`.
    #[must_use]
    pub fn from_inspected(inspected: &serde_json::Value) -> Option<Self> {
        let labels = &inspected["Config"]["Labels"];
        let label = |key: &str| labels[key].as_str().map(str::to_owned);
        if label("com.docker.compose.oneoff")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            return None;
        }
        let project_id = label(crate::domain::ownership::LABEL_PROJECT_ID)?
            .parse::<uuid::Uuid>()
            .ok()?;
        let state = &inspected["State"];
        Some(Self {
            project_id: ProjectId(project_id),
            environment: label(crate::domain::ownership::LABEL_ENVIRONMENT)?,
            stack: label(crate::domain::ownership::LABEL_STACK)?,
            service: label("com.docker.compose.service")?,
            health: Health::from_engine(
                state["Status"].as_str().unwrap_or_default(),
                state["Health"]["Status"].as_str(),
            ),
            restarts: inspected["RestartCount"].as_u64().unwrap_or_default(),
        })
    }
}

/// A service of an environment on this node.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceSample {
    pub stack: String,
    pub service: String,
    /// The containers the last deployment runs of it here; `None` for a
    /// container of a service that deployment does not place here.
    pub desired: Option<u32>,
    /// Its containers here by health, in the order of [`Health`].
    pub containers: [u32; 4],
    /// How often the engine restarted its containers here.
    pub restarts: u64,
}

impl ServiceSample {
    #[must_use]
    pub fn running(&self) -> u32 {
        self.containers[..3].iter().sum()
    }
}

/// How an environment's last rollout on this node went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolloutOutcome {
    Running,
    Finished,
    Failed,
    RolledBack,
}

impl RolloutOutcome {
    pub const ALL: [Self; 4] = [
        Self::Running,
        Self::Finished,
        Self::Failed,
        Self::RolledBack,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::RolledBack => "rolled-back",
        }
    }
}

/// An environment the node runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentSample {
    pub project_id: ProjectId,
    /// The project's name in the host policy.
    pub project: String,
    pub environment: String,
    pub deployed_at_unix_ms: Option<i64>,
    pub rollout: Option<RolloutOutcome>,
    pub services: Vec<ServiceSample>,
}

/// A mesh peer as this node sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerSample {
    pub peer: String,
    pub handshake_age_seconds: Option<u64>,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    pub on_interface: bool,
}

impl PeerSample {
    #[must_use]
    pub fn up(&self) -> bool {
        self.on_interface
            && self
                .handshake_age_seconds
                .is_some_and(|age| age <= PEER_DOWN_AFTER_SECONDS)
    }
}

/// Which part of a collection a scrape got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Host,
    Filesystems,
    Mesh,
    Services,
}

impl Part {
    pub const ALL: [Self; 4] = [Self::Host, Self::Filesystems, Self::Mesh, Self::Services];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Filesystems => "filesystems",
            Self::Mesh => "mesh",
            Self::Services => "services",
        }
    }
}

/// One collection of everything a node serves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeSnapshot {
    pub node: String,
    pub version: String,
    pub engine: String,
    pub collect_seconds: f64,
    pub host: Option<procfs::HostSample>,
    /// Read apart from the rest of the host: a filesystem that hangs
    /// leaves only these out.
    pub filesystems: Option<Vec<procfs::FilesystemSample>>,
    pub peers: Option<Vec<PeerSample>>,
    pub environments: Option<Vec<EnvironmentSample>>,
}

impl NodeSnapshot {
    #[must_use]
    pub fn collected(&self, part: Part) -> bool {
        match part {
            Part::Host => self.host.is_some(),
            Part::Filesystems => self.filesystems.is_some(),
            Part::Mesh => self.peers.is_some(),
            Part::Services => self.environments.is_some(),
        }
    }
}

/// What the last deployment of an environment runs on this node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployedEnvironment {
    pub project_id: ProjectId,
    pub project: String,
    pub environment: String,
    /// Each service it places here, with the containers it runs of it.
    pub services: Vec<(String, String, u32)>,
    pub deployed_at_unix_ms: Option<i64>,
    pub rollout: Option<RolloutOutcome>,
    /// The cluster networks, by their name across the cluster, whose
    /// `metrics: true` asks for the node's metrics.
    pub metrics_networks: Vec<String>,
}

/// The node's services: what each environment's last deployment runs
/// here next to what the engine runs, by environment and service.
#[must_use]
pub fn environment_samples(
    deployed: &[DeployedEnvironment],
    containers: &[LabelledContainer],
) -> Vec<EnvironmentSample> {
    let mut samples = deployed
        .iter()
        .map(|environment| EnvironmentSample {
            project_id: environment.project_id,
            project: environment.project.clone(),
            environment: environment.environment.clone(),
            deployed_at_unix_ms: environment.deployed_at_unix_ms,
            rollout: environment.rollout,
            services: environment
                .services
                .iter()
                .map(|(stack, service, desired)| ServiceSample {
                    stack: stack.clone(),
                    service: service.clone(),
                    desired: Some(*desired),
                    ..ServiceSample::default()
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    for container in containers {
        let Some(environment) = samples.iter_mut().find(|sample| {
            sample.project_id == container.project_id && sample.environment == container.environment
        }) else {
            continue;
        };
        let index = match environment.services.iter().position(|service| {
            service.stack == container.stack && service.service == container.service
        }) {
            Some(index) => index,
            None => {
                environment.services.push(ServiceSample {
                    stack: container.stack.clone(),
                    service: container.service.clone(),
                    ..ServiceSample::default()
                });
                environment.services.len() - 1
            }
        };
        if let Some(service) = environment.services.get_mut(index) {
            let slot = match container.health {
                Health::Healthy => 0,
                Health::Starting => 1,
                Health::Unhealthy => 2,
                Health::Stopped => 3,
            };
            if let Some(count) = service.containers.get_mut(slot) {
                *count = count.saturating_add(1);
            }
            service.restarts = service.restarts.saturating_add(container.restarts);
        }
    }
    for sample in &mut samples {
        sample.services.sort_by(|left, right| {
            (&left.stack, &left.service).cmp(&(&right.stack, &right.service))
        });
    }
    samples
}
