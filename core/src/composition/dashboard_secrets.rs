use super::*;
use crate::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretResolution,
    SecretScope, SecretWriteResult,
};

/// Gate references carry their project identity. Resolve that registered owner
/// on each request instead of binding the global HTTP service to one checkout.
pub(super) struct DashboardGateSecrets {
    home: MmHome,
    state: Arc<dyn StateStore>,
}

impl DashboardGateSecrets {
    pub(super) fn new(home: MmHome, state: Arc<dyn StateStore>) -> Self {
        Self { home, state }
    }

    fn registry(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<Arc<SecretProviderRegistry>, MmError> {
        let mut segments = reference.locator().split('/');
        if segments.next() != Some("gate") {
            return Err(MmError::Security {
                message:
                    "the dashboard secret resolver accepts only project-scoped gate references"
                        .to_owned(),
            });
        }
        let project_id = segments
            .next()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .map(crate::domain::workspace::ProjectId)
            .ok_or_else(|| MmError::Security {
                message: "gate secret reference has no valid project owner".to_owned(),
            })?;
        if segments.next() != Some(scope.environment()) {
            return Err(MmError::Security {
                message: "gate secret reference does not match the requested environment"
                    .to_owned(),
            });
        }
        let limit = crate::domain::workspace_registry::MAX_REGISTERED_WORKSPACES;
        let workspaces = crate::app::registered_workspaces(self.state.as_ref(), limit + 1)?;
        if workspaces.len() > limit {
            return Err(MmError::Security {
                message: "dashboard project registry exceeds its bound".to_owned(),
            });
        }
        let matching = workspaces
            .iter()
            .filter(|workspace| workspace.project_id == project_id && !workspace.stale)
            .collect::<Vec<_>>();
        let [workspace] = matching.as_slice() else {
            return Err(MmError::Security {
                message: "gate secret owner must identify exactly one registered project"
                    .to_owned(),
            });
        };
        let document = ConfigLoader.load_from(&workspace.root)?;
        if document.config.project.id != project_id || document.path != workspace.config_path {
            return Err(MmError::Security {
                message: "registered gate project ownership changed".to_owned(),
            });
        }
        let age = Arc::new(AgeSecretStore::new(
            identity_path(&self.home),
            self.home
                .secrets_directory
                .join(format!("{}.age", project_id.0)),
        ));
        let revision_key = crate::adapters::secrets::load_or_create_environment_revision_key(
            &self
                .home
                .identities_directory
                .join("environment-revisions.key"),
        )?;
        secret_provider_registry(age, revision_key, &self.home, project_id, &document.config)
    }
}

#[async_trait::async_trait]
impl SecretMetadataInspector for DashboardGateSecrets {
    async fn inspect(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        self.registry(scope, reference)?
            .inspect(scope, reference)
            .await
    }
}

#[async_trait::async_trait]
impl SecretMaterialResolver for DashboardGateSecrets {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        self.registry(scope, reference)?
            .resolve(scope, reference)
            .await
    }
}

#[async_trait::async_trait]
impl SecretCreator for DashboardGateSecrets {
    async fn create(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        now_unix_ms: i64,
        set_by: Option<&str>,
    ) -> Result<SecretWriteResult, MmError> {
        self.registry(scope, reference)?
            .create(scope, reference, material, now_unix_ms, set_by)
            .await
    }
}

#[async_trait::async_trait]
impl SecretRemover for DashboardGateSecrets {
    async fn unset(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        self.registry(scope, reference)?
            .unset(scope, reference, expected)
            .await
    }
}
