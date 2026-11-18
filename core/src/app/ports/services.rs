use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::managed_service::{
    ManagedServicePlan, ManagedServicePlanRequest, ManagedServiceStatus,
};
use crate::domain::service_stack::{
    PreparedServiceStackArtifact, ServiceComponentStatus, ServiceStackPlan, ServiceStackState,
};

pub struct ServiceStackArtifactInput<'a> {
    pub plan_identity: &'a str,
    pub manifest: &'a [u8],
    pub compose: &'a [u8],
    pub route: &'a [u8],
    pub dashboard_unit: &'a [u8],
}

pub trait ServiceStackStore: Send + Sync {
    fn load_state(&self) -> Result<Option<ServiceStackState>, MmError>;
    fn save_state(&self, state: &ServiceStackState) -> Result<(), MmError>;
    fn remove_state(&self) -> Result<(), MmError>;
    fn ensure_cap_admin_key_file(&self) -> Result<std::path::PathBuf, MmError>;
    fn publish(
        &self,
        artifact: ServiceStackArtifactInput<'_>,
    ) -> Result<PreparedServiceStackArtifact, MmError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceExecutorStatus {
    pub components: Vec<ServiceComponentStatus>,
    pub diagnostics: Vec<crate::domain::diagnostic::Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashboardServiceStatus {
    pub installed: bool,
    pub running: bool,
}

#[async_trait]
pub trait DashboardServiceInstaller: Send + Sync {
    async fn preflight(&self, plan: &ServiceStackPlan) -> Result<(), MmError>;
    async fn install(&self, plan: &ServiceStackPlan) -> Result<(), MmError>;
    async fn status(&self, plan: &ServiceStackPlan) -> Result<DashboardServiceStatus, MmError>;
    async fn uninstall(&self, plan: &ServiceStackPlan) -> Result<(), MmError>;
}

#[async_trait]
pub trait ServiceStackExecutor: Send + Sync {
    async fn preflight(&self, plan: &ServiceStackPlan) -> Result<(), MmError>;
    async fn apply(&self, plan: &ServiceStackPlan) -> Result<(), MmError>;
    async fn status(&self, plan: &ServiceStackPlan) -> Result<ServiceExecutorStatus, MmError>;
    async fn remove(&self, plan: &ServiceStackPlan) -> Result<(), MmError>;
}

#[async_trait]
pub trait ManagedServiceRuntime: Send + Sync {
    fn kind(&self) -> crate::domain::runtime::RuntimeKind;
    fn plan(&self, request: ManagedServicePlanRequest) -> Result<ManagedServicePlan, MmError>;
    async fn apply(&self, plan: &ManagedServicePlan) -> Result<(), MmError>;
    async fn observe(&self, plan: &ManagedServicePlan) -> Result<ManagedServiceStatus, MmError>;
    async fn remove(&self, plan: &ManagedServicePlan, purge_data: bool) -> Result<(), MmError>;
}
pub trait InstallationIngressOwnership: Send + Sync {
    fn owns_ingress(
        &self,
        target_id: crate::domain::target::TargetId,
    ) -> Result<bool, crate::domain::error::MmError>;
}
