use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::app::WorkspaceSelection;
use crate::app::config_validation::validate;
use crate::app::ports::{
    Clock, EnvironmentSourceDiscovery, EnvironmentValueStore, MaterializedEnvironmentWriter,
    ProjectDocumentStore, SecretDeliveryCapabilityCatalog, SecretMaterialResolver,
    SecretMetadataInspector, SecretStorageCapabilityCatalog, StateStore,
};
use crate::domain::config::{
    ConfigDocument, EnvironmentBinding, SecretDeclaration, resolve_environment,
};
use crate::domain::config_resolve::{effective_binding, effective_bindings, resolve_cast_url};
use crate::domain::diagnostic::{Diagnostic, Severity, ValidationReport};
use crate::domain::environment::{
    CompiledEnvironmentBundle, CompiledEnvironmentManifest, CompiledRuntimeSecret,
    CompiledServiceEnvironment, CompiledServiceManifest, EnvironmentDiff, EnvironmentIndex,
    EnvironmentReview, EnvironmentStatusSnapshot, EnvironmentValueOverride,
    MaterializedEnvironment, ModuleEnvironmentStatus, ResolutionStatus, ServiceEnvironmentStatus,
    ValueSource, VariableDeclaration, VariableStatus, is_safe_environment_value,
};
use crate::domain::environment_delivery::{EnvironmentDeliveryMode, resolve_environment_delivery};
use crate::domain::error::MmError;
use crate::domain::path_policy::{ensure_no_symlink_components, read_regular_file};
use crate::domain::runtime_secret::RuntimeSecretStatusItem;
use crate::domain::runtime_secret::{RuntimeSecretConsumer, RuntimeSecretPlanItem};
use crate::domain::secret::{
    SecretBindingStatus, SecretMaterial, SecretMetadata, SecretReference, SecretScope,
    SecretStatus, SecretStatusSnapshot,
};
use crate::domain::secret_contract::{
    DeliveryDriverId, LogicalSecretId, MaterializationName, SecretCompatibilityRequest,
    SecretMaterialKind, SecretWorkloadKind, StorageOperation, plan_secret_compatibility,
};
use crate::domain::workspace::{WorkspaceRecord, WorkspaceRegistration};

mod status;
use status::*;
mod compilation;
mod secret_defaults;
use compilation::*;
mod store_outage;
use store_outage::StoreOutage;
mod release;
use release::{check_release_keys, refuse_release_override};
pub use store_outage::{SECRET_STORE_UNAVAILABLE, refused_at_apply};

#[derive(Clone)]
pub struct EnvironmentManager {
    documents: Arc<dyn ProjectDocumentStore>,
    sources: Arc<dyn EnvironmentSourceDiscovery>,
    writer: Arc<dyn MaterializedEnvironmentWriter>,
    state: Arc<dyn StateStore>,
    values: Arc<dyn EnvironmentValueStore>,
    inspector: Arc<dyn SecretMetadataInspector>,
    resolver: Arc<dyn SecretMaterialResolver>,
    storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog>,
    delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog>,
    clock: Arc<dyn Clock>,
}

#[derive(Clone)]
pub struct EnvironmentDependencies {
    pub documents: Arc<dyn ProjectDocumentStore>,
    pub sources: Arc<dyn EnvironmentSourceDiscovery>,
    pub writer: Arc<dyn MaterializedEnvironmentWriter>,
    pub state: Arc<dyn StateStore>,
    pub values: Arc<dyn EnvironmentValueStore>,
    pub inspector: Arc<dyn SecretMetadataInspector>,
    pub resolver: Arc<dyn SecretMaterialResolver>,
    pub storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog>,
    pub delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog>,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for EnvironmentManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EnvironmentManager")
            .finish_non_exhaustive()
    }
}

mod manager;
mod secret_driver;
use secret_driver::runtime_secret_driver;
mod redaction;
pub(crate) use redaction::environment_redactions;
mod registry;
mod tls;
mod tunnel;
pub use registry::unset_registry_secret;
