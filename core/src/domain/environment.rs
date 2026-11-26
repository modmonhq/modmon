use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::domain::diagnostic::Diagnostic;
use crate::domain::environment_delivery::ResolvedEnvironmentDelivery;
use crate::domain::operation::OperationId;
use crate::domain::secret::SecretStatus;

pub const ENVIRONMENT_GRAMMAR_VERSION: u32 = 1;

#[must_use]
pub fn is_safe_environment_value(value: &str) -> bool {
    !value.chars().any(char::is_control)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VariableDeclaration {
    pub name: String,
    pub global_key: Option<String>,
    pub required: bool,
    pub default_value: Option<String>,
    pub literal_value: Option<String>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModuleEnvironmentIndex {
    pub module: String,
    pub template_path: PathBuf,
    pub services: Vec<StackService>,
    pub variables: Vec<VariableDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StackService {
    pub stack: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnvironmentIndex {
    pub environment: String,
    pub input_hash: String,
    pub modules: Vec<ModuleEnvironmentIndex>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueSource {
    ServiceEnv,
    SqliteOverride,
    ConfigValue,
    /// A fact of the release being deployed: `MM.` and `RELEASE.` keys.
    Release,
    Secret,
    TemplateDefault,
    TemplateLiteral,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Resolved,
    Defaulted,
    Unset,
    NotChecked,
    ProviderUnavailable,
    AccessDenied,
    Invalid,
}

impl From<SecretStatus> for ResolutionStatus {
    fn from(status: SecretStatus) -> Self {
        match status {
            SecretStatus::Set => Self::Resolved,
            SecretStatus::Unset => Self::Unset,
            SecretStatus::NotChecked => Self::NotChecked,
            SecretStatus::ProviderUnavailable => Self::ProviderUnavailable,
            SecretStatus::AccessDenied => Self::AccessDenied,
            SecretStatus::Invalid => Self::Invalid,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VariableStatus {
    pub name: String,
    pub global_key: Option<String>,
    pub required: bool,
    pub has_default: bool,
    pub sensitive: bool,
    pub source: ValueSource,
    pub status: ResolutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_by: Option<String>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceEnvironmentStatus {
    pub stack: String,
    pub service: String,
    pub variables: Vec<VariableStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModuleEnvironmentStatus {
    pub module: String,
    pub template_path: PathBuf,
    pub services: Vec<ServiceEnvironmentStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnvironmentStatusSnapshot {
    pub project_name: String,
    pub environment: String,
    #[serde(default)]
    pub delivery: ResolvedEnvironmentDelivery,
    pub config_hash: String,
    pub input_hash: String,
    pub modules: Vec<ModuleEnvironmentStatus>,
    pub diagnostics: Vec<Diagnostic>,
}

impl EnvironmentStatusSnapshot {
    /// Only the variables named `key`, by name or global key, as `mm env
    /// explain KEY` shows them.
    pub fn only(mut self, key: &str) -> Result<Self, crate::domain::error::MmError> {
        let mut found = false;
        for module in &mut self.modules {
            for service in &mut module.services {
                service.variables.retain(|variable| {
                    let matches =
                        variable.name == key || variable.global_key.as_deref() == Some(key);
                    found |= matches;
                    matches
                });
            }
            module
                .services
                .retain(|service| !service.variables.is_empty());
        }
        self.modules.retain(|module| !module.services.is_empty());
        if !found {
            return Err(crate::domain::error::MmError::Environment {
                message: format!("no environment variable or global key matches '{key}'"),
            });
        }
        Ok(self)
    }

    /// The snapshot with no value of a sensitive variable.
    #[must_use]
    pub fn without_sensitive_values(mut self) -> Self {
        self.modules
            .iter_mut()
            .flat_map(|module| &mut module.services)
            .flat_map(|service| &mut service.variables)
            .filter(|variable| variable.sensitive)
            .for_each(|variable| variable.value = None);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnvironmentValueOverride {
    pub key: String,
    pub value: String,
    pub revision: i64,
    pub updated_at_unix_ms: i64,
}

pub struct CompiledServiceEnvironment {
    pub module: String,
    pub stack: String,
    pub service: String,
    pub direct_values: crate::domain::runtime::RuntimeEnvironment,
    pub runtime_secrets: Vec<CompiledRuntimeSecret>,
    bytes: Zeroizing<Vec<u8>>,
}

impl CompiledServiceEnvironment {
    #[must_use]
    pub fn new(module: String, stack: String, service: String, bytes: Vec<u8>) -> Self {
        Self {
            module,
            stack,
            service,
            direct_values: crate::domain::runtime::RuntimeEnvironment::default(),
            runtime_secrets: Vec::new(),
            bytes: Zeroizing::new(bytes),
        }
    }

    #[must_use]
    pub fn with_intent(
        mut self,
        direct_values: crate::domain::runtime::RuntimeEnvironment,
    ) -> Self {
        self.direct_values = direct_values;
        self
    }

    #[must_use]
    pub fn with_runtime_secrets(mut self, runtime_secrets: Vec<CompiledRuntimeSecret>) -> Self {
        self.runtime_secrets = runtime_secrets;
        self
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl std::fmt::Debug for CompiledServiceEnvironment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledServiceEnvironment")
            .field("module", &self.module)
            .field("stack", &self.stack)
            .field("service", &self.service)
            .field("direct_values", &self.direct_values)
            .field("runtime_secrets", &self.runtime_secrets)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone)]
pub struct CompiledRuntimeSecret {
    pub logical_name: String,
    pub material: std::sync::Arc<crate::domain::secret::SecretMaterial>,
}

impl std::fmt::Debug for CompiledRuntimeSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledRuntimeSecret")
            .field("logical_name", &self.logical_name)
            .field("material", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompiledEnvironmentManifest {
    pub project_name: String,
    pub environment: String,
    pub input_hash: String,
    pub services: Vec<CompiledServiceManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompiledServiceManifest {
    pub module: String,
    pub stack: String,
    pub service: String,
    pub variable_count: usize,
    pub secret_count: usize,
}

#[derive(Debug)]
pub struct CompiledEnvironmentBundle {
    pub manifest: CompiledEnvironmentManifest,
    pub services: Vec<CompiledServiceEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MaterializedEnvironment {
    pub module: String,
    pub stack: String,
    pub service: String,
    pub path: PathBuf,
    pub variable_count: usize,
    pub secret_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnvironmentDiff {
    pub module: String,
    pub stack: String,
    pub service: String,
    pub path: PathBuf,
    pub exists: bool,
    pub changed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnvironmentReview {
    pub input_hash: String,
    pub services: Vec<CompiledServiceManifest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secret_revisions: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub delivery: ResolvedEnvironmentDelivery,
}

impl EnvironmentReview {
    #[must_use]
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.input_hash.as_bytes());
        hasher.update(&(self.services.len() as u64).to_le_bytes());
        for service in &self.services {
            hasher.update(service.module.as_bytes());
            hasher.update(service.stack.as_bytes());
            hasher.update(service.service.as_bytes());
            hasher.update(&(service.variable_count as u64).to_le_bytes());
            hasher.update(&(service.secret_count as u64).to_le_bytes());
        }
        for (key, revision) in &self.secret_revisions {
            hasher.update(key.as_bytes());
            match revision {
                Some(revision) => {
                    hasher.update(&[1]);
                    hasher.update(revision.as_bytes());
                }
                None => {
                    hasher.update(&[0]);
                }
            }
        }
        hasher.update(self.delivery.mode.as_str().as_bytes());
        hasher.update(self.delivery.provenance.as_str().as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedEnvironmentBundle {
    pub operation_id: OperationId,
    pub root: PathBuf,
    pub manifest: CompiledEnvironmentManifest,
    pub services: Vec<PreparedServiceEnvironment>,
    pub interpolation_environment: crate::domain::runtime::RuntimeEnvironment,
    pub runtime_secret_overlays: Vec<(String, PathBuf)>,
    pub runtime_secret_rollback_overlays: Vec<(String, PathBuf)>,
    /// Registry logins for this operation; held in memory only.
    pub registry_credentials: Vec<crate::domain::runtime::RegistryCredential>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedServiceEnvironment {
    pub module: String,
    pub stack: String,
    pub service: String,
    pub path: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
