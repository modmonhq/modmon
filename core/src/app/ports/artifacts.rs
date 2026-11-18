use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use crate::domain::environment::{CompiledEnvironmentBundle, PreparedEnvironmentBundle};
use crate::domain::error::MmError;
use crate::domain::operation::OperationId;
use crate::domain::secret_contract::{DeliveryCapabilityDescriptor, DeliveryDriverId};
use crate::domain::stack::{CompiledStackArtifact, PreparedStackArtifact};
use crate::domain::target::TargetId;
use crate::domain::target_ingress::{CompiledTargetIngressArtifact, PreparedTargetIngressArtifact};
use crate::domain::traefik_dynamic::{PreparedTraefikDynamicConfig, RenderedTraefikDynamicConfig};

pub struct EnvironmentArtifactRequest<'a> {
    pub operation_id: OperationId,
    pub delivery_driver: DeliveryDriverId,
    pub bundle: &'a CompiledEnvironmentBundle,
}

pub trait SecretDeliveryDriver: Send + Sync {
    fn descriptor(&self) -> &DeliveryCapabilityDescriptor;
    fn publish(
        &self,
        operation_id: OperationId,
        bundle: &CompiledEnvironmentBundle,
    ) -> Result<PreparedEnvironmentBundle, MmError>;
}

pub trait SecretDeliveryCapabilityCatalog: Send + Sync {
    fn delivery_capabilities(&self) -> Vec<DeliveryCapabilityDescriptor>;
    fn delivery_capability(
        &self,
        driver: &DeliveryDriverId,
    ) -> Result<DeliveryCapabilityDescriptor, MmError>;
}

pub trait EnvironmentArtifactStore: Send + Sync {
    fn bundle_root(&self, operation_id: OperationId) -> PathBuf;
    fn publish(
        &self,
        request: EnvironmentArtifactRequest<'_>,
    ) -> Result<PreparedEnvironmentBundle, MmError>;
    fn dispose(&self, bundle: &PreparedEnvironmentBundle) -> Result<(), MmError>;
    fn reconcile(&self, active_operations: &HashSet<OperationId>) -> Result<(), MmError>;

    /// The `overlays` of `bundle` limited to `services`, for putting back
    /// the stack files of an earlier deployment with the environment of a
    /// plan whose stack has more services: Compose refuses a stack whose
    /// overlay names a service its files do not define. An overlay that
    /// names no other service is returned as it is.
    fn limit_overlays(
        &self,
        _bundle: &PreparedEnvironmentBundle,
        overlays: &[PathBuf],
        _services: &BTreeSet<String>,
    ) -> Result<Vec<PathBuf>, MmError> {
        Ok(overlays.to_vec())
    }
}

pub trait StackArtifactStore: Send + Sync {
    fn publish(
        &self,
        artifacts: &[CompiledStackArtifact],
    ) -> Result<Vec<PreparedStackArtifact>, MmError>;

    /// The directory that holds the versions of an environment's config
    /// files on this host. `publish` writes a stack's versions there, and
    /// a stack's overlay mounts them from there.
    fn config_file_root(
        &self,
        target_id: TargetId,
        project_id: crate::domain::workspace::ProjectId,
        environment: &str,
    ) -> PathBuf;

    /// Removes every version in that directory that is not in `keep`, and
    /// returns the removed ones.
    fn prune_config_files(
        &self,
        target_id: TargetId,
        project_id: crate::domain::workspace::ProjectId,
        environment: &str,
        keep: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<String>, MmError>;
}

pub trait TargetIngressArtifactStore: Send + Sync {
    fn publish(
        &self,
        artifact: &CompiledTargetIngressArtifact,
    ) -> Result<PreparedTargetIngressArtifact, MmError>;
}

pub trait TraefikDynamicConfigStore: Send + Sync {
    fn publish(
        &self,
        expected_target_id: TargetId,
        config: &RenderedTraefikDynamicConfig,
    ) -> Result<PreparedTraefikDynamicConfig, MmError>;
}
