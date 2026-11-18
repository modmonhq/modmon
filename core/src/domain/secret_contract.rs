use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;

pub const MAX_SECRET_MATERIAL_BYTES: usize = 1024 * 1024;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct StorageProviderId(String);

impl StorageProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        validate_identifier(&value, "storage provider")?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for StorageProviderId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<StorageProviderId> for String {
    fn from(value: StorageProviderId) -> Self {
        value.0
    }
}

impl fmt::Display for StorageProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct DeliveryDriverId(String);

impl DeliveryDriverId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        validate_identifier(&value, "secret delivery driver")?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_static(value: &'static str) -> Self {
        Self(value.to_owned())
    }
}

impl TryFrom<String> for DeliveryDriverId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<DeliveryDriverId> for String {
    fn from(value: DeliveryDriverId) -> Self {
        value.0
    }
}

impl fmt::Display for DeliveryDriverId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogicalSecretId(String);

impl LogicalSecretId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 255
            || !value
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphanumeric())
            || !value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
            })
        {
            return Err(
                "logical secret identifiers must start with an ASCII letter or number and contain 1-255 ASCII letters, numbers, '.', '-' or '_'".to_owned(),
            );
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LogicalSecretId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterializationName(String);

impl MaterializationName {
    pub fn environment(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let valid = value.len() <= 255
            && value
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_');
        if !valid {
            return Err(
                "environment materialization names must be 1-255 ASCII letters, numbers or '_' and may not start with a number"
                    .to_owned(),
            );
        }
        Ok(Self(value))
    }

    pub fn file(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 255
            || matches!(value.as_str(), "." | "..")
            || value.contains(['/', '\\'])
            || value.chars().any(char::is_control)
        {
            return Err("secret file names must be one safe 1-255 byte path component".to_owned());
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MaterializationName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretMaterialKind {
    Text,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorageOperation {
    InspectMetadata,
    ResolveMaterial,
    Create,
    Replace,
    ReplaceBatch,
    StageCandidate,
    PromoteCandidate,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevisionSemantics {
    None,
    Observed,
    CompareAndSwap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorageEnvironmentScoping {
    Isolated,
    ProcessGlobal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RotationCapability {
    None,
    Replace,
    Staged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationKind {
    LocalIdentity,
    ProcessEnvironment,
    AppRoleEphemeralToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StorageCapabilityDescriptor {
    pub id: StorageProviderId,
    pub operations: Vec<StorageOperation>,
    pub revision_semantics: RevisionSemantics,
    pub max_material_bytes: usize,
    pub max_location_bytes: usize,
    pub environment_scoping: StorageEnvironmentScoping,
    pub material_kinds: Vec<SecretMaterialKind>,
    pub rotation: RotationCapability,
    pub authentication: AuthenticationKind,
    pub metadata_without_material_resolution: bool,
}

impl StorageCapabilityDescriptor {
    #[must_use]
    pub fn supports(&self, operation: StorageOperation) -> bool {
        self.operations.contains(&operation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryOperation {
    DirectEnvironmentValue,
    OperationScopedEnvironmentFile,
    MountedFile,
    RuntimeNativeSecret,
    ProviderProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryEnvironmentScoping {
    Operation,
    Workload,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretWorkloadKind {
    Service,
    Build,
    Job,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaintextPersistence {
    None,
    OperationScopedPrivateFile,
    /// One private file per secret revision, kept on the host until a
    /// deployment that no longer uses the revision has succeeded.
    RevisionScopedPrivateFile,
    ProviderManaged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeliveryCapabilityDescriptor {
    pub id: DeliveryDriverId,
    pub operation: DeliveryOperation,
    pub environment_scoping: DeliveryEnvironmentScoping,
    pub material_kinds: Vec<SecretMaterialKind>,
    pub max_material_bytes: usize,
    pub max_materialization_name_bytes: usize,
    pub runtimes: Vec<RuntimeKind>,
    pub workload_kinds: Vec<SecretWorkloadKind>,
    pub plaintext_persistence: PlaintextPersistence,
}

pub struct SecretCompatibilityRequest<'a> {
    pub logical_secret: &'a LogicalSecretId,
    pub backend: &'a StorageCapabilityDescriptor,
    pub delivery: &'a DeliveryCapabilityDescriptor,
    pub runtime: RuntimeKind,
    pub workload: SecretWorkloadKind,
    pub material_kind: SecretMaterialKind,
    pub materialization_name: &'a MaterializationName,
}

pub fn plan_secret_compatibility(request: SecretCompatibilityRequest<'_>) -> Result<(), MmError> {
    let supported_deliveries = |descriptor: &DeliveryCapabilityDescriptor| {
        format!(
            "driver '{}' supports runtimes {:?}, workloads {:?}, and material kinds {:?}",
            descriptor.id,
            descriptor.runtimes,
            descriptor.workload_kinds,
            descriptor.material_kinds
        )
    };
    if !request
        .backend
        .material_kinds
        .contains(&request.material_kind)
    {
        return Err(compatibility_error(
            &request,
            format!(
                "backend material kinds are {:?}; {}",
                request.backend.material_kinds,
                supported_deliveries(request.delivery)
            ),
        ));
    }
    if !request
        .delivery
        .material_kinds
        .contains(&request.material_kind)
        || !request.delivery.runtimes.contains(&request.runtime)
        || !request.delivery.workload_kinds.contains(&request.workload)
        || request.materialization_name.as_str().len()
            > request.delivery.max_materialization_name_bytes
    {
        return Err(compatibility_error(
            &request,
            supported_deliveries(request.delivery),
        ));
    }
    Ok(())
}

fn compatibility_error(request: &SecretCompatibilityRequest<'_>, supported: String) -> MmError {
    MmError::SecretCompatibility {
        logical_secret: request.logical_secret.to_string(),
        backend: request.backend.id.to_string(),
        delivery: request.delivery.id.to_string(),
        runtime: request.runtime,
        workload: request.workload,
        supported,
    }
}

fn validate_identifier(value: &str, kind: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= 63
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase())
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{kind} identifiers must be 1-63 lowercase ASCII letters, numbers or '-' and start with a letter"
        ))
    }
}

#[cfg(test)]
mod tests;
