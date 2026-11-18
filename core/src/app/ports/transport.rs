use std::path::Path;

use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::target::{
    BootstrapExecution, NamedTarget, TargetBootstrapPlan, TargetDefinition, TargetId,
    TargetIngress, TargetTransport,
};
use crate::domain::workspace::ProjectId;

#[async_trait]
pub trait TargetBootstrapExecutor: Send + Sync {
    async fn execute(
        &self,
        project_root: &Path,
        plan: &TargetBootstrapPlan,
    ) -> Result<BootstrapExecution, MmError>;
}

/// The release of mm a single host's `mmd` reports, read before setup
/// replaces it.
#[async_trait]
pub trait HostReleaseReader: Send + Sync {
    /// `None` when the host has no `mmd`, or one too old to report its
    /// release.
    async fn host_release(
        &self,
        transport: &TargetTransport,
    ) -> Result<Option<crate::domain::cluster_setup::HostRelease>, MmError>;
}

pub trait TargetOperationLockGuard: Send + Sync {}

pub trait TargetOperationLock: Send + Sync {
    fn acquire(&self, target_id: TargetId) -> Result<Box<dyn TargetOperationLockGuard>, MmError>;
}

pub trait BuildOperationLock: Send + Sync {
    fn acquire(&self, project_id: ProjectId) -> Result<Box<dyn TargetOperationLockGuard>, MmError>;
}

pub trait TargetStore: Send + Sync {
    fn list_targets(&self) -> Result<Vec<NamedTarget>, MmError>;
    fn resolve_target(&self, name: &str) -> Result<NamedTarget, MmError>;
    fn insert_target(
        &self,
        name: &str,
        definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError>;
    fn ensure_target(
        &self,
        name: &str,
        definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError>;
    fn remove_target(&self, name: &str, expected_fingerprint: &str)
    -> Result<NamedTarget, MmError>;
    fn update_target_transport(
        &self,
        name: &str,
        expected_fingerprint: &str,
        transport: TargetTransport,
    ) -> Result<NamedTarget, MmError>;
    fn update_target_ingress(
        &self,
        name: &str,
        expected_fingerprint: &str,
        ingress: TargetIngress,
    ) -> Result<NamedTarget, MmError>;
}
