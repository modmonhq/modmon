use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::workspace::ProjectId;

macro_rules! provider_identifier {
    ($name:ident, $kind:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                let valid = !value.is_empty()
                    && value.len() <= 63
                    && value
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_lowercase())
                    && value.chars().all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '-'
                    });
                if !valid {
                    return Err(format!(
                        "{} must be 1-63 lowercase ASCII letters, numbers or '-' and start with a letter",
                        $kind
                    ));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

provider_identifier!(ProviderInstanceName, "provider instance names");
provider_identifier!(ProviderDriverId, "provider driver IDs");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    Managed,
    External,
}

impl ProviderMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderScope {
    Installation,
    Project,
    /// One store per environment: on a cluster target, the store mm runs
    /// on the target's nodes.
    Environment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOperation {
    Add,
    Edit,
    Test,
    Status,
    Install,
    Restart,
    Upgrade,
    Seal,
    Unseal,
    Backup,
    Restore,
    Remove,
}

impl ProviderOperation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Edit => "edit",
            Self::Test => "test",
            Self::Status => "status",
            Self::Install => "install",
            Self::Restart => "restart",
            Self::Upgrade => "upgrade",
            Self::Seal => "seal",
            Self::Unseal => "unseal",
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFieldKind {
    Text,
    HttpsUrl,
    Path,
    Sha256,
    U16,
    Boolean,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderConfigurationField {
    pub key: String,
    pub label: String,
    pub kind: ProviderFieldKind,
    pub required: bool,
    pub secret: bool,
    pub default: Option<String>,
    pub allowed_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderModeDescriptor {
    pub mode: ProviderMode,
    pub scopes: Vec<ProviderScope>,
    pub operations: Vec<ProviderOperation>,
    pub configuration: Vec<ProviderConfigurationField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderDescriptor {
    pub driver: ProviderDriverId,
    pub display_name: String,
    pub modes: Vec<ProviderModeDescriptor>,
}

impl ProviderDescriptor {
    #[must_use]
    pub fn mode(&self, mode: ProviderMode) -> Option<&ProviderModeDescriptor> {
        self.modes.iter().find(|descriptor| descriptor.mode == mode)
    }

    pub fn require(
        &self,
        mode: ProviderMode,
        operation: ProviderOperation,
    ) -> Result<(), crate::domain::error::MmError> {
        let supported = self
            .mode(mode)
            .map_or(&[][..], |descriptor| descriptor.operations.as_slice());
        if supported.contains(&operation) {
            return Ok(());
        }
        Err(crate::domain::error::MmError::Secret {
            message: format!(
                "provider driver '{}' in {} mode does not support {}; supported operations are {:?}",
                self.driver,
                mode.as_str(),
                operation.as_str(),
                supported
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "mode",
    rename_all = "snake_case",
    rename_all_fields = "kebab-case",
    deny_unknown_fields
)]
pub enum ProviderConfiguration {
    Managed {
        /// Local development only. Default: the provider's name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        installation: Option<String>,
        release: String,
        /// Local development only. Default: 18200.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        management_port: Option<u16>,
        /// Cluster targets only: the store's Raft members. Default: every
        /// node of the target, when it has 3 or 5.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nodes: Option<crate::domain::secret_store::StoreNodes>,
    },
    External {
        address: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        storage_mount: String,
        automation_auth_mount: String,
        trust_bundle: PathBuf,
        expected_trust_sha256: String,
    },
}

/// A managed provider as local development runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalManagedProvider {
    pub installation: String,
    pub release: String,
    pub management_port: u16,
}

impl ProviderConfiguration {
    /// The installation, release and loopback port of a managed provider
    /// named `instance`, with their defaults.
    #[must_use]
    pub fn local_managed(&self, instance: &str) -> Option<LocalManagedProvider> {
        match self {
            Self::Managed {
                installation,
                release,
                management_port,
                ..
            } => Some(LocalManagedProvider {
                installation: installation.clone().unwrap_or_else(|| instance.to_owned()),
                release: release.clone(),
                management_port: management_port
                    .unwrap_or(crate::domain::secret_store::DEFAULT_MANAGEMENT_PORT),
            }),
            Self::External { .. } => None,
        }
    }

    #[must_use]
    pub const fn mode(&self) -> ProviderMode {
        match self {
            Self::Managed { .. } => ProviderMode::Managed,
            Self::External { .. } => ProviderMode::External,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ProviderInstanceConfig {
    pub driver: ProviderDriverId,
    pub scope: ProviderScope,
    #[serde(flatten)]
    pub configuration: ProviderConfiguration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderInstanceDescriptor {
    pub name: ProviderInstanceName,
    pub driver: ProviderDriverId,
    pub driver_display_name: String,
    pub mode: ProviderMode,
    pub scope: ProviderScope,
    pub operations: Vec<ProviderOperation>,
    pub configuration: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteSecretBackendSetupRequest {
    pub instance: ProviderInstanceName,
    pub operation: ProviderOperation,
    pub address: String,
    pub namespace: Option<String>,
    pub storage_mount: String,
    pub automation_auth_mount: String,
    pub trust_bundle: PathBuf,
    pub expected_trust_sha256: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub locations: Vec<String>,
    pub allow_bootstrap: bool,
    pub target: String,
    pub config_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteSecretBackendSetupPlan {
    pub request: RemoteSecretBackendSetupRequest,
    pub consequences: Vec<String>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteSecretBackendStatusRequest {
    pub instance: ProviderInstanceName,
    pub project_id: ProjectId,
    pub environment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretBackendHealth {
    pub initialized: bool,
    pub sealed: bool,
    pub standby: bool,
    pub healthy: bool,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteSecretBackendStatus {
    pub instance: ProviderInstanceName,
    pub driver: ProviderDriverId,
    pub driver_display_name: String,
    pub mode: ProviderMode,
    pub scope: ProviderScope,
    pub capabilities: Vec<ProviderOperation>,
    pub environment: String,
    pub health: SecretBackendHealth,
    pub authenticated: bool,
    pub isolated_scope: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSecretBackendAction {
    Install,
    Restart,
    Upgrade,
    Seal,
    Unseal,
    Remove,
}

impl ManagedSecretBackendAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Restart => "restart",
            Self::Upgrade => "upgrade",
            Self::Seal => "seal",
            Self::Unseal => "unseal",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedSecretBackendPlanRequest {
    pub instance: ProviderInstanceName,
    pub action: ManagedSecretBackendAction,
    pub installation: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub locations: Vec<String>,
    pub release: String,
    pub management_port: u16,
    pub purge_data: bool,
    pub target: String,
    pub config_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedSecretBackendPlan {
    pub instance: ProviderInstanceName,
    pub action: ManagedSecretBackendAction,
    pub installation: String,
    pub immutable_artifact: String,
    pub previous_artifact: Option<String>,
    pub ingress_url: String,
    pub management_access: String,
    pub data_resource: String,
    pub purge_data: bool,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedSecretBackendStatus {
    pub instance: ProviderInstanceName,
    pub driver: ProviderDriverId,
    pub driver_display_name: String,
    pub mode: ProviderMode,
    pub scope: ProviderScope,
    pub capabilities: Vec<ProviderOperation>,
    pub installed: bool,
    pub installation: Option<String>,
    pub connection: Option<String>,
    pub immutable_artifact: Option<String>,
    pub data_resource: Option<String>,
    pub ingress_url: String,
    pub management_url: Option<String>,
    pub management_access: String,
    pub health: Option<SecretBackendHealth>,
    pub recovery_available_off_instance: bool,
    pub journal: Option<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedSecretBackendResult {
    pub instance: ProviderInstanceName,
    pub action: ManagedSecretBackendAction,
    pub plan_hash: String,
    pub rolled_back: bool,
    pub data_purged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSecretDataAction {
    Backup,
    Restore,
}

impl ManagedSecretDataAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Backup => "backup",
            Self::Restore => "restore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedSecretDataPlan {
    pub instance: ProviderInstanceName,
    pub action: ManagedSecretDataAction,
    pub path: PathBuf,
    pub state_digest: String,
    pub source_digest: Option<String>,
    pub target: String,
    pub config_hash: String,
    pub consequences: Vec<String>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedSecretDataResult {
    pub instance: ProviderInstanceName,
    pub action: ManagedSecretDataAction,
    pub path: PathBuf,
    pub snapshot_digest: String,
    pub encrypted: bool,
    pub integrity_checked: bool,
}

#[cfg(test)]
mod tests;
