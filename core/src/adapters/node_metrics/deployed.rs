//! What each environment's last deployment runs on this node, from the
//! records the node keeps: the services it watches (`healing`), the
//! counts of its scaled services (`autoscale`), its deployment history,
//! and the deployed checkout's `mm.yaml`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::adapters::autoscale::{SCALE_DIRECTORY, store as scaling};
use crate::adapters::healing::{HEALTH_DIRECTORY, WatchedEnvironment, store as watching};
use crate::adapters::remote::HostAccessStore;
use crate::app::ports::{DeployedEnvironments, ProjectDocumentStore};
use crate::domain::autoscale::ScaledService;
use crate::domain::cluster_network::network_key;
use crate::domain::config::ServiceRole;
use crate::domain::error::MmError;
use crate::domain::node_metrics::{DeployedEnvironment, RolloutOutcome};
use crate::domain::rollout::RolloutState;
use crate::domain::workspace::ProjectId;

/// What a deployed checkout's `mm.yaml` says that matters here.
#[derive(Debug, Clone, Default)]
struct Declared {
    /// The networks with `metrics: true`, by their names in `mm.yaml`.
    metrics: Vec<String>,
    /// The `role: job` services, by stack and name: their containers come
    /// and go with their runs.
    jobs: BTreeSet<(String, String)>,
}

pub struct NodeDeployments {
    state_root: PathBuf,
    access: HostAccessStore,
    documents: Arc<dyn ProjectDocumentStore>,
    declared: Mutex<BTreeMap<PathBuf, Declared>>,
}

impl std::fmt::Debug for NodeDeployments {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeDeployments")
            .field("state_root", &self.state_root)
            .finish_non_exhaustive()
    }
}

impl NodeDeployments {
    #[must_use]
    pub fn new(
        state_root: &Path,
        access: HostAccessStore,
        documents: Arc<dyn ProjectDocumentStore>,
    ) -> Self {
        Self {
            state_root: state_root.to_path_buf(),
            access,
            documents,
            declared: Mutex::new(BTreeMap::new()),
        }
    }

    fn declared(&self, root: &Path) -> Declared {
        let document = match self.documents.load_from(root) {
            Ok(document) => document,
            Err(error) => {
                tracing::warn!(checkout = %root.display(), %error, "cannot read the deployed mm.yaml; its networks serve no metrics");
                return Declared::default();
            }
        };
        let config = document.config;
        Declared {
            metrics: config
                .networks
                .iter()
                .filter(|(_, network)| network.metrics)
                .map(|(name, _)| name.clone())
                .collect(),
            jobs: config
                .modules
                .values()
                .flat_map(|module| &module.services)
                .filter(|(_, service)| service.role == Some(ServiceRole::Job))
                .map(|(name, service)| (service.stack.clone(), name.clone()))
                .collect(),
        }
    }

    fn desired(&self, scaled: Option<&ScaledService>, node: &str) -> u32 {
        let Some(scaled) = scaled else {
            return 1;
        };
        let wanted = scaling::want(&self.state_root.join(SCALE_DIRECTORY), &scaled.key())
            .ok()
            .flatten()
            .map(|want| want.count);
        scaled.share(node, wanted)
    }

    fn environment(
        &self,
        watched: &WatchedEnvironment,
        declared: &Declared,
        scaled: &[ScaledService],
        project: String,
    ) -> DeployedEnvironment {
        let scope = &watched.scope;
        let node = scope.node.clone().unwrap_or_default();
        let services = watched
            .services
            .iter()
            .filter(|service| {
                !declared
                    .jobs
                    .contains(&(service.stack.clone(), service.service.clone()))
            })
            .map(|service| {
                let scaled = scaled.iter().find(|scaled| {
                    scaled.stack == service.stack && scaled.service == service.service
                });
                (
                    service.stack.clone(),
                    service.service.clone(),
                    self.desired(scaled, &node),
                )
            })
            .collect();
        let history = self
            .access
            .deployments(scope.project_id, &scope.environment)
            .unwrap_or_default();
        DeployedEnvironment {
            project_id: scope.project_id,
            project,
            environment: scope.environment.clone(),
            services,
            deployed_at_unix_ms: history.current.map(|current| current.deployed_at_unix_ms),
            rollout: history
                .rollout
                .map(|rollout| outcome(rollout.state, rollout.failure.is_some())),
            metrics_networks: declared
                .metrics
                .iter()
                .map(|name| network_key(scope.project_id, &scope.environment, name))
                .collect(),
        }
    }
}

const fn outcome(state: RolloutState, failed: bool) -> RolloutOutcome {
    match state {
        RolloutState::Finished => RolloutOutcome::Finished,
        RolloutState::RolledBack => RolloutOutcome::RolledBack,
        RolloutState::Held | RolloutState::Running if failed => RolloutOutcome::Failed,
        RolloutState::Held | RolloutState::Running => RolloutOutcome::Running,
    }
}

impl DeployedEnvironments for NodeDeployments {
    fn deployed(&self) -> Result<Vec<DeployedEnvironment>, MmError> {
        let watched = watching::watched(&self.state_root.join(HEALTH_DIRECTORY))?;
        let scaled = scaling::deployed(&self.state_root.join(SCALE_DIRECTORY)).unwrap_or_default();
        let names = self
            .access
            .load_host()
            .map(|(host, _)| {
                host.projects
                    .into_iter()
                    .map(|(id, project)| (id, project.name))
                    .collect::<BTreeMap<ProjectId, String>>()
            })
            .unwrap_or_default();
        let mut declared = self.declared.lock().unwrap_or_else(PoisonError::into_inner);
        declared.retain(|root, _| watched.iter().any(|record| record.root == *root));
        Ok(watched
            .iter()
            .map(|record| {
                let facts = declared
                    .entry(record.root.clone())
                    .or_insert_with(|| self.declared(&record.root))
                    .clone();
                let services = scaled
                    .iter()
                    .find(|deployed| deployed.scope == record.scope)
                    .map(|deployed| deployed.services.as_slice())
                    .unwrap_or_default();
                let project = names
                    .get(&record.scope.project_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        crate::domain::cluster_network::project_key(record.scope.project_id)
                    });
                self.environment(record, &facts, services, project)
            })
            .collect())
    }
}
