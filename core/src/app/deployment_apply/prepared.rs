//! The environment a rollout's steps share on a node: reviewed, compiled
//! and logged in once, at the first step that needs it, and forgotten when
//! the rollout finishes or rolls back. Each step still publishes and
//! disposes of its own files.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::domain::environment::CompiledEnvironmentBundle;
use crate::domain::runtime::RegistryCredential;

/// How long a rollout's environment is kept after its last step.
const IDLE: Duration = Duration::from_secs(10 * 60);
/// How many rollouts' environments a node keeps at once.
const KEPT: usize = 4;

/// What every step of one operation deploys with.
pub struct EnvironmentMaterial {
    pub compiled: Option<CompiledEnvironmentBundle>,
    pub registry_credentials: Vec<RegistryCredential>,
}

type Entry = (OperationId, Instant, Arc<EnvironmentMaterial>);

/// The environments of the rollouts in progress on a node.
#[derive(Default)]
pub struct PreparedEnvironments {
    entries: Mutex<Vec<Entry>>,
}

impl std::fmt::Debug for PreparedEnvironments {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedEnvironments")
            .field("kept", &self.len())
            .finish()
    }
}

impl PreparedEnvironments {
    /// How many operations have an environment kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether `operation` has an environment kept.
    #[must_use]
    pub fn holds(&self, operation: OperationId) -> bool {
        self.entries.lock().is_ok_and(|entries| {
            entries
                .iter()
                .any(|(kept, used, _)| *kept == operation && used.elapsed() < IDLE)
        })
    }

    pub fn get(&self, operation: OperationId) -> Option<Arc<EnvironmentMaterial>> {
        let Ok(mut entries) = self.entries.lock() else {
            return None;
        };
        entries.retain(|(_, used, _)| used.elapsed() < IDLE);
        let index = entries.iter().position(|(kept, _, _)| *kept == operation)?;
        let (_, _, material) = entries.remove(index);
        entries.push((operation, Instant::now(), Arc::clone(&material)));
        Some(material)
    }

    pub fn keep(&self, operation: OperationId, material: Arc<EnvironmentMaterial>) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.retain(|(kept, used, _)| *kept != operation && used.elapsed() < IDLE);
        if entries.len() >= KEPT {
            entries.remove(0);
        }
        entries.push((operation, Instant::now(), material));
    }

    /// Drops the environment of `operation`, when one is kept.
    pub fn forget(&self, operation: OperationId) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(kept, _, _)| *kept != operation);
        }
    }
}

impl DeploymentService {
    /// The rollout's steps on a cluster node share one prepared
    /// environment (see [`PreparedEnvironments`]).
    #[must_use]
    pub fn with_prepared_environments(mut self, prepared: Arc<PreparedEnvironments>) -> Self {
        self.prepared = Some(prepared);
        self
    }

    /// The reviewed, compiled environment of `plan` and its registry
    /// logins: the one this operation kept before, or a new one, kept
    /// with `keep`.
    pub(super) async fn environment_material(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
        operation_id: OperationId,
        keep: bool,
    ) -> Result<Arc<EnvironmentMaterial>, MmError> {
        if let Some(material) = self
            .prepared
            .as_ref()
            .and_then(|prepared| prepared.get(operation_id))
        {
            return Ok(material);
        }
        let selection = &selection.with_release(plan.release.clone());
        let review = self.ports.environments.review(selection).await?;
        if review.digest() != plan.environment_review.digest() {
            return Err(MmError::State {
                message: "environment metadata changed after the reviewed deployment plan"
                    .to_owned(),
            });
        }
        let compiled = if review.services.is_empty() {
            None
        } else {
            Some(self.ports.environments.compile(selection).await?)
        };
        let registry_credentials = self
            .ports
            .environments
            .registry_credentials(selection)
            .await?;
        if !registry_credentials.is_empty() && compiled.is_none() {
            return Err(MmError::Environment {
                message: "registries: needs at least one service with a compiled environment to carry its logins".to_owned(),
            });
        }
        let material = Arc::new(EnvironmentMaterial {
            compiled,
            registry_credentials,
        });
        if let (Some(prepared), true) = (&self.prepared, keep) {
            prepared.keep(operation_id, Arc::clone(&material));
        }
        Ok(material)
    }

    /// Forgets the environment of an operation that ended.
    pub(super) fn forget_environment(&self, operation_id: OperationId) {
        if let Some(prepared) = &self.prepared {
            prepared.forget(operation_id);
        }
    }
}
