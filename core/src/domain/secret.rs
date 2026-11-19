use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

use crate::domain::diagnostic::Diagnostic;
use crate::domain::secret_contract::{DeliveryCapabilityDescriptor, StorageCapabilityDescriptor};
use crate::domain::secret_contract::{MAX_SECRET_MATERIAL_BYTES, StorageProviderId};

mod bundle;
mod file;
pub use bundle::{
    BundledDeployKey, BundledSecret, DeployKeyImport, MAX_SECRET_BUNDLE_BYTES,
    SECRET_BUNDLE_VERSION, SecretBundle, SecretImportEntry, SecretImportSummary,
    validate_secret_revision,
};
pub use file::{FileSecretLocation, file_secret_location};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitializedAgeStore {
    pub identity_path: std::path::PathBuf,
    pub store_path: std::path::PathBuf,
    pub recipient: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretScope {
    environment: String,
}

impl SecretScope {
    #[must_use]
    pub fn new(environment: impl Into<String>) -> Self {
        Self {
            environment: environment.into(),
        }
    }

    #[must_use]
    pub fn environment(&self) -> &str {
        &self.environment
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SecretReference {
    provider: StorageProviderId,
    connection: Option<String>,
    locator: String,
}

impl SecretReference {
    #[must_use]
    pub fn scheme(&self) -> &str {
        self.provider.as_str()
    }

    #[must_use]
    pub fn provider(&self) -> &StorageProviderId {
        &self.provider
    }

    #[must_use]
    pub fn locator(&self) -> &str {
        &self.locator
    }

    #[must_use]
    pub fn connection(&self) -> Option<&str> {
        self.connection.as_deref()
    }

    #[must_use]
    pub fn canonical(&self) -> String {
        self.connection.as_ref().map_or_else(
            || format!("{}/{}", self.provider, self.locator),
            |connection| format!("{}/{connection}/{}", self.provider, self.locator),
        )
    }

    pub fn new(provider: StorageProviderId, locator: impl Into<String>) -> Result<Self, String> {
        let locator = locator.into();
        validate_locator(&locator)?;
        Ok(Self {
            provider,
            connection: None,
            locator,
        })
    }

    pub fn with_connection(
        provider: StorageProviderId,
        connection: impl Into<String>,
        locator: impl Into<String>,
    ) -> Result<Self, String> {
        let connection = connection.into();
        validate_connection_name(&connection)?;
        let locator = locator.into();
        validate_locator(&locator)?;
        Ok(Self {
            provider,
            connection: Some(connection),
            locator,
        })
    }
}

impl FromStr for SecretReference {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (scheme, locator) = value
            .split_once("://")
            .ok_or_else(|| "secret reference must contain ://".to_owned())?;
        Self::new(
            StorageProviderId::new(scheme.to_owned())?,
            locator.to_owned(),
        )
    }
}

impl fmt::Debug for SecretReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SecretReference({}/[REDACTED])", self.provider)
    }
}

impl Serialize for SecretReference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}://{}", self.provider, self.locator))
    }
}

impl<'de> Deserialize<'de> for SecretReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for SecretReference {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SecretReference".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = String::json_schema(generator);
        schema.insert(
            "pattern".to_owned(),
            serde_json::Value::String(
                "^[a-z][a-z0-9+.-]*://[^\\u0000-\\u001f\\u007f]+$".to_owned(),
            ),
        );
        schema
    }
}

/// Owned plaintext secret material. The type is deliberately neither cloneable nor serializable.
///
/// ```compile_fail
/// let material = mm::domain::secret::SecretMaterial::new("secret".to_owned()).unwrap();
/// let _copy = material.clone();
/// ```
///
/// ```compile_fail
/// let material = mm::domain::secret::SecretMaterial::new("secret".to_owned()).unwrap();
/// let _json = serde_json::to_string(&material).unwrap();
/// ```
pub struct SecretMaterial(Zeroizing<String>);

impl SecretMaterial {
    pub fn new(value: String) -> Result<Self, String> {
        Self::from_zeroizing(Zeroizing::new(value))
    }

    pub fn from_zeroizing(value: Zeroizing<String>) -> Result<Self, String> {
        if value.is_empty() {
            return Err("secret material may not be empty".to_owned());
        }
        if value.len() > MAX_SECRET_MATERIAL_BYTES {
            return Err(format!(
                "secret material exceeds the {MAX_SECRET_MATERIAL_BYTES}-byte limit"
            ));
        }
        Ok(Self(value))
    }

    pub fn expose<T>(&self, use_value: impl FnOnce(&str) -> T) -> T {
        use_value(self.0.as_str())
    }

    #[must_use]
    pub fn into_zeroizing(self) -> Zeroizing<String> {
        self.0
    }

    /// What output must hide: the value, and each line of a value that has
    /// several, since output is often read one line at a time.
    #[must_use]
    pub fn redactions(&self) -> Vec<Zeroizing<String>> {
        let value = self.0.as_str();
        std::iter::once(value)
            .chain(
                value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && *line != value),
            )
            .map(|fragment| Zeroizing::new(fragment.to_owned()))
            .collect()
    }
}

fn validate_locator(locator: &str) -> Result<(), String> {
    if locator.is_empty() || locator.len() > 1024 || locator.chars().any(char::is_control) {
        return Err(
            "secret locator must be 1-1024 bytes and contain no control characters".to_owned(),
        );
    }
    if locator
        .split('/')
        .any(|segment| matches!(segment, "." | ".."))
    {
        return Err("secret locator contains a traversal segment".to_owned());
    }
    Ok(())
}

fn validate_connection_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err("secret connection names must be 1-63 lowercase ASCII letters, numbers or '-' and start with a letter".to_owned())
    }
}

impl fmt::Debug for SecretMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretMaterial([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretStatus {
    Set,
    Unset,
    NotChecked,
    ProviderUnavailable,
    AccessDenied,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMetadata {
    pub status: SecretStatus,
    pub revision: Option<String>,
    pub created_at_unix_ms: Option<i64>,
    pub candidate_revision: Option<String>,
}

impl SecretMetadata {
    #[must_use]
    pub fn expected_state(&self) -> SecretExpectedState {
        SecretExpectedState {
            active_revision: self.revision.clone(),
            candidate_revision: self.candidate_revision.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretExpectedState {
    pub active_revision: Option<String>,
    pub candidate_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretWriteResult {
    pub revision: String,
    pub changed: bool,
}

pub struct SecretReplacement {
    pub scope: SecretScope,
    pub reference: SecretReference,
    pub material: SecretMaterial,
    pub expected: SecretExpectedState,
}

impl fmt::Debug for SecretReplacement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretReplacement")
            .field("scope", &self.scope)
            .field("reference", &self.reference)
            .field("material", &"[REDACTED]")
            .field("expected", &self.expected)
            .finish()
    }
}

pub struct SecretResolution {
    pub metadata: SecretMetadata,
    pub material: Option<SecretMaterial>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretBindingStatus {
    pub key: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub status: SecretStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretStatusSnapshot {
    pub project_name: String,
    pub environment: String,
    pub secrets: Vec<SecretBindingStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_delivery: Vec<crate::domain::runtime_secret::RuntimeSecretStatusItem>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretCapabilitySnapshot {
    pub storage: Vec<StorageCapabilityDescriptor>,
    pub delivery: Vec<DeliveryCapabilityDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DirectSecretRotationPlan {
    pub project_name: String,
    pub environment: String,
    pub name: String,
    pub provider: String,
    pub current_revision: String,
    pub requires_hidden_input: bool,
    pub plan_hash: String,
}

/// Which configured generated secrets `mm secret generate` creates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretGenerationSelection {
    /// One secret; any failure fails the run.
    Named(String),
    /// The secrets the selected environment uses.
    Missing,
    /// Every secret with a generator, whichever environment uses it.
    All,
    /// These secrets, run like `Missing` and `All`.
    Listed(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretGenerationStatus {
    Generated,
    AlreadySet,
    /// Its provider has no connection from this machine or is read-only;
    /// `reason` says why.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretGenerationItem {
    pub name: String,
    pub provider: String,
    pub status: SecretGenerationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretGenerationReport {
    pub project_name: String,
    pub environment: String,
    pub secrets: Vec<SecretGenerationItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretRotationPlanStatus {
    NotScheduled,
    NotDue,
    Ready,
    InputRequired,
    UnsupportedStrategy,
    Unset,
    Invalid,
    ProviderUnavailable,
    CandidateStaged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretRotationPlanItem {
    pub name: String,
    pub provider: String,
    pub strategy: String,
    pub status: SecretRotationPlanStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretRotationPlan {
    pub project_name: String,
    pub generated_at_unix_ms: i64,
    pub plan_hash: String,
    pub secrets: Vec<SecretRotationPlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretRotationApplyReport {
    pub project_name: String,
    pub plan_hash: String,
    pub rotated: Vec<SecretRotationResultItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretRotationResultItem {
    pub name: String,
    pub provider: String,
    pub revision: String,
}

impl fmt::Debug for SecretResolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretResolution")
            .field("metadata", &self.metadata)
            .field("material", &self.material.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[cfg(test)]
mod tests;
