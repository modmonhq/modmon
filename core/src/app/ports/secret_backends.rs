use std::path::Path;

use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::secret::SecretMaterial;
use crate::domain::secret_backend::{
    ManagedSecretBackendPlan, ManagedSecretBackendPlanRequest, ManagedSecretBackendResult,
    ManagedSecretBackendStatus, ManagedSecretDataAction, ManagedSecretDataPlan,
    ManagedSecretDataResult, RemoteSecretBackendSetupRequest, RemoteSecretBackendStatus,
    RemoteSecretBackendStatusRequest,
};

#[async_trait]
pub trait RemoteSecretBackendProvisioner: Send + Sync {
    async fn setup(
        &self,
        request: RemoteSecretBackendSetupRequest,
        bootstrap_credential: SecretMaterial,
    ) -> Result<RemoteSecretBackendStatus, MmError>;

    async fn status(
        &self,
        request: &RemoteSecretBackendStatusRequest,
    ) -> Result<RemoteSecretBackendStatus, MmError>;
}

#[async_trait]
pub trait ManagedSecretBackendProvisioner: Send + Sync {
    fn plan(
        &self,
        request: ManagedSecretBackendPlanRequest,
    ) -> Result<ManagedSecretBackendPlan, MmError>;

    async fn apply(
        &self,
        request: ManagedSecretBackendPlanRequest,
        reviewed_plan_hash: &str,
    ) -> Result<ManagedSecretBackendResult, MmError>;

    async fn status(
        &self,
        instance: &crate::domain::secret_backend::ProviderInstanceName,
    ) -> Result<ManagedSecretBackendStatus, MmError>;

    fn plan_data(
        &self,
        instance: &crate::domain::secret_backend::ProviderInstanceName,
        action: ManagedSecretDataAction,
        path: &Path,
    ) -> Result<ManagedSecretDataPlan, MmError>;

    async fn apply_data(
        &self,
        plan: &ManagedSecretDataPlan,
    ) -> Result<ManagedSecretDataResult, MmError>;
}
