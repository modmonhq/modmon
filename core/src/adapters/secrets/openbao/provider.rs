use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use zeroize::Zeroizing;

use crate::app::ports::{
    SecretCreator, SecretMaterialResolver, SecretMetadataInspector, SecretRemover, SecretReplacer,
};
use crate::domain::environment::is_safe_environment_value;
use crate::domain::error::MmError;
use crate::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretResolution,
    SecretScope, SecretStatus, SecretWriteResult,
};
use crate::domain::secret_contract::{
    AuthenticationKind, RevisionSemantics, RotationCapability, SecretMaterialKind,
    StorageCapabilityDescriptor, StorageEnvironmentScoping, StorageOperation, StorageProviderId,
};
use crate::domain::workspace::ProjectId;

use super::connection_store::{OpenBaoConnection, OpenBaoConnectionStore, OpenBaoMode};
use super::http::{OpenBaoHttpResponse, OpenBaoHttpTransport};

const MAX_OPENBAO_SECRET_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct OpenBaoHealth {
    pub initialized: bool,
    pub sealed: bool,
    pub standby: bool,
    pub version: String,
    pub healthy: bool,
}

#[derive(Clone)]
pub struct OpenBaoSecretStore {
    instance: crate::domain::secret_backend::ProviderInstanceName,
    mode: OpenBaoMode,
    project_id: ProjectId,
    connections: Arc<OpenBaoConnectionStore>,
    transport: Arc<dyn OpenBaoHttpTransport>,
    declared: Option<crate::domain::repo_target::DeclaredProviderAddress>,
    /// The managed store of a cluster environment: what the host has staged
    /// for it comes first, and each version carries the revision it was
    /// staged with.
    staged: Option<Arc<crate::adapters::secrets::AgeSecretStore>>,
}

impl std::fmt::Debug for OpenBaoSecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenBaoSecretStore")
            .field("mode", &self.mode)
            .field("project_id", &self.project_id)
            .finish_non_exhaustive()
    }
}

impl OpenBaoSecretStore {
    #[must_use]
    pub fn new(
        instance: crate::domain::secret_backend::ProviderInstanceName,
        mode: OpenBaoMode,
        project_id: ProjectId,
        connections: Arc<OpenBaoConnectionStore>,
        transport: Arc<dyn OpenBaoHttpTransport>,
    ) -> Self {
        Self {
            instance,
            mode,
            project_id,
            connections,
            transport,
            declared: None,
            staged: None,
        }
    }

    /// The managed store of cluster environments, with what a host stages
    /// in its own `staged` store.
    #[must_use]
    pub fn staged_in(mut self, staged: Arc<crate::adapters::secrets::AgeSecretStore>) -> Self {
        self.staged = Some(staged);
        self
    }

    /// The address the configuration declares for this provider. A
    /// deployment host's own connection is used only when it is for that
    /// address.
    #[must_use]
    pub fn declared_at(
        mut self,
        declared: Option<crate::domain::repo_target::DeclaredProviderAddress>,
    ) -> Self {
        self.declared = declared;
        self
    }

    fn connection(
        &self,
        reference: &SecretReference,
        environment: &str,
    ) -> Result<OpenBaoConnection, MmError> {
        if reference.provider().as_str() != self.instance.as_str() {
            return Err(MmError::Secret {
                message: format!(
                    "{} provider received a reference for another backend",
                    self.instance
                ),
            });
        }
        let name = reference
            .connection()
            .unwrap_or_else(|| self.instance.as_str());
        let connection = crate::adapters::secrets::host_connection::find_connection(
            name,
            self.project_id,
            environment,
            self.declared.as_ref(),
            |name| self.connections.has_connection(name),
            |name| self.connections.connection(name),
            |connection: &OpenBaoConnection, environment| {
                crate::adapters::secrets::host_connection::ConnectionFacts {
                    project: connection.project_id,
                    address: connection.address.clone(),
                    has_environment: connection.credentials.contains_key(environment),
                }
            },
        )?;
        if connection.mode != self.mode {
            return Err(MmError::Security {
                message: format!(
                    "private connection '{name}' is {:?}, not {:?}",
                    connection.mode, self.mode
                ),
            });
        }
        if connection.project_id != self.project_id {
            return Err(MmError::Security {
                message: "private provider connection belongs to another project".to_owned(),
            });
        }
        Ok(connection)
    }

    pub async fn health_for_connection(
        &self,
        connection: &OpenBaoConnection,
    ) -> Result<OpenBaoHealth, MmError> {
        let response = self
            .transport
            .request(connection, Method::GET, "sys/health", None, None)
            .await?;
        parse_health(response)
    }

    async fn authenticated<T>(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        operation: impl AsyncFnOnce(&OpenBaoConnection, &str, String) -> Result<T, MmError>,
    ) -> Result<T, MmError> {
        let connection = self.connection(reference, scope.environment())?;
        let (connection, health) = self.serving(connection).await?;
        if !health.initialized || health.sealed {
            return Err(MmError::Secret {
                message: "provider is not initialized or is sealed".to_owned(),
            });
        }
        let token = self.login(&connection, scope.environment()).await?;
        let path = data_path(&connection, self.project_id, scope, reference);
        operation(&connection, token.as_str(), path).await
    }

    async fn login(
        &self,
        connection: &OpenBaoConnection,
        environment: &str,
    ) -> Result<Zeroizing<String>, MmError> {
        let credential = connection.credentials.get(environment).ok_or_else(|| {
            MmError::Security {
                message: format!(
                    "private provider connection has no AppRole scoped to environment '{environment}'"
                ),
            }
        })?;
        let body = serde_json::json!({
            "role_id": credential.role_id.as_str(),
            "secret_id": credential.secret_id.as_str(),
        });
        let response = self
            .transport
            .request(
                connection,
                Method::POST,
                &format!("auth/{}/login", encode_segment(&connection.auth_mount)),
                None,
                Some(&body),
            )
            .await?;
        // OpenBao and Vault refuse an AppRole login with a wrong, expired or
        // destroyed secret ID as a bad request (400, "invalid role or secret
        // ID"), not as 403: both mean the store does not take this credential.
        if matches!(response.status, 400 | 403) {
            return Err(MmError::Security {
                message: "provider AppRole authentication was denied".to_owned(),
            });
        }
        require_success(&response, "AppRole authentication")?;
        let login = parse_json::<LoginResponse>(&response, "AppRole login response")?;
        if !login.auth.renewable
            || !(60..=3_600).contains(&login.auth.lease_duration)
            || login.auth.client_token.is_empty()
        {
            return Err(MmError::Security {
                message: "provider issued a non-renewable or non-short-lived automation token"
                    .to_owned(),
            });
        }
        Ok(login.auth.client_token)
    }

    async fn metadata(
        &self,
        connection: &OpenBaoConnection,
        token: &str,
        data_path: &str,
    ) -> Result<SecretMetadata, MmError> {
        let path = data_path.replacen("/data/", "/metadata/", 1);
        let response = self
            .transport
            .request(connection, Method::GET, &path, Some(token), None)
            .await?;
        metadata_from_response(response)
    }
}

#[async_trait]
impl SecretMetadataInspector for OpenBaoSecretStore {
    async fn inspect(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        if let Some(staged) = &self.staged {
            if let Some(metadata) = staged.staged_metadata(scope, reference)? {
                return Ok(metadata);
            }
            return self.live(scope, reference).await;
        }
        self.authenticated(scope, reference, async |connection, token, path| {
            self.metadata(connection, token, &path).await
        })
        .await
        .or_else(|error| {
            if unavailable(&error) {
                Ok(unavailable_metadata())
            } else {
                Err(error)
            }
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for OpenBaoSecretStore {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        if let Some(staged) = &self.staged
            && let Some(resolution) = staged.staged_resolution(scope, reference)?
        {
            return Ok(resolution);
        }
        self.authenticated(scope, reference, async |connection, token, path| {
            let response = self
                .transport
                .request(connection, Method::GET, &path, Some(token), None)
                .await?;
            if response.status == 404 {
                return Ok(SecretResolution {
                    metadata: unset_metadata(),
                    material: None,
                });
            }
            if response.status == 403 {
                return Ok(SecretResolution {
                    metadata: access_denied_metadata(),
                    material: None,
                });
            }
            require_success(&response, "secret read")?;
            let read = parse_json::<ReadResponse>(&response, "secret read response")?;
            let revision = read.revision();
            let created_at_unix_ms = parse_timestamp(&read.data.metadata.created_time);
            let value = read.data.data.value;
            if value.is_empty()
                || value.len() > MAX_OPENBAO_SECRET_BYTES
                || !is_safe_environment_value(&value)
            {
                return Ok(SecretResolution {
                    metadata: SecretMetadata {
                        status: SecretStatus::Invalid,
                        revision: Some(revision),
                        created_at_unix_ms,
                        candidate_revision: None,
                    },
                    material: None,
                });
            }
            Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::Set,
                    revision: Some(revision),
                    created_at_unix_ms,
                    candidate_revision: None,
                },
                material: Some(
                    SecretMaterial::from_zeroizing(value)
                        .map_err(|message| MmError::Secret { message })?,
                ),
            })
        })
        .await
    }
}

#[async_trait]
impl SecretCreator for OpenBaoSecretStore {
    async fn create(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        now_unix_ms: i64,
    ) -> Result<SecretWriteResult, MmError> {
        validate_material(&material)?;
        self.authenticated(scope, reference, async |connection, token, path| {
            let body = material.expose(|value| {
                serde_json::json!({
                    "options": { "cas": 0 },
                    "data": { "value": value, "mm_created_at_unix_ms": now_unix_ms }
                })
            });
            let response = self
                .transport
                .request(connection, Method::POST, &path, Some(token), Some(&body))
                .await?;
            if matches!(response.status, 400 | 409) {
                let metadata = self.metadata(connection, token, &path).await?;
                if metadata.status == SecretStatus::Set
                    && let Some(revision) = metadata.revision
                {
                    return Ok(SecretWriteResult {
                        revision,
                        changed: false,
                    });
                }
            }
            require_success(&response, "secret create")?;
            let write = parse_json::<WriteResponse>(&response, "secret write response")?;
            Ok(SecretWriteResult {
                revision: write.data.version.to_string(),
                changed: true,
            })
        })
        .await
    }
}

#[async_trait]
impl SecretReplacer for OpenBaoSecretStore {
    async fn replace(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError> {
        validate_material(&material)?;
        let version = expected_version(&expected)?;
        self.authenticated(scope, reference, async |connection, token, path| {
            let body = material.expose(|value| {
                serde_json::json!({
                    "options": { "cas": version },
                    "data": { "value": value, "mm_created_at_unix_ms": now_unix_ms }
                })
            });
            let response = self
                .transport
                .request(connection, Method::POST, &path, Some(token), Some(&body))
                .await?;
            if matches!(response.status, 400 | 409) {
                return Err(lifecycle_conflict());
            }
            require_success(&response, "secret replacement")?;
            Ok(
                parse_json::<WriteResponse>(&response, "secret write response")?
                    .data
                    .version
                    .to_string(),
            )
        })
        .await
    }
}

#[async_trait]
impl SecretRemover for OpenBaoSecretStore {
    async fn unset(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        let version = expected_version(&expected)?;
        self.authenticated(scope, reference, async |connection, token, path| {
            let metadata = self.metadata(connection, token, &path).await?;
            match metadata.status {
                SecretStatus::Unset => return Ok(false),
                SecretStatus::Set | SecretStatus::Invalid
                    if metadata.revision.as_deref() == Some(version.to_string().as_str()) => {}
                _ => return Err(lifecycle_conflict()),
            }
            let body = serde_json::json!({ "versions": [version] });
            let delete_path = path.replacen("/data/", "/delete/", 1);
            let response = self
                .transport
                .request(
                    connection,
                    Method::POST,
                    &delete_path,
                    Some(token),
                    Some(&body),
                )
                .await?;
            require_success(&response, "secret deletion")?;
            let current = self.metadata(connection, token, &path).await?;
            if matches!(current.status, SecretStatus::Set | SecretStatus::Invalid) {
                return Err(lifecycle_conflict());
            }
            Ok(true)
        })
        .await
    }
}

pub fn openbao_storage_descriptor(
    instance: &crate::domain::secret_backend::ProviderInstanceName,
) -> Result<StorageCapabilityDescriptor, MmError> {
    Ok(StorageCapabilityDescriptor {
        id: StorageProviderId::new(instance.as_str())
            .map_err(|message| MmError::Security { message })?,
        operations: vec![
            StorageOperation::InspectMetadata,
            StorageOperation::ResolveMaterial,
            StorageOperation::Create,
            StorageOperation::Replace,
            StorageOperation::Delete,
        ],
        revision_semantics: RevisionSemantics::CompareAndSwap,
        max_material_bytes: MAX_OPENBAO_SECRET_BYTES,
        max_location_bytes: 1024,
        environment_scoping: StorageEnvironmentScoping::Isolated,
        material_kinds: vec![SecretMaterialKind::Text],
        rotation: RotationCapability::Replace,
        authentication: AuthenticationKind::AppRoleEphemeralToken,
        metadata_without_material_resolution: true,
    })
}

fn data_path(
    connection: &OpenBaoConnection,
    project_id: ProjectId,
    scope: &SecretScope,
    reference: &SecretReference,
) -> String {
    let locator = reference
        .locator()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "{}/data/mm/{}/{}/{}",
        encode_segment(&connection.mount),
        project_id.0,
        encode_segment(scope.environment()),
        locator
    )
}

pub(super) fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn metadata_from_response(response: OpenBaoHttpResponse) -> Result<SecretMetadata, MmError> {
    if response.status == 404 {
        return Ok(unset_metadata());
    }
    if response.status == 403 {
        return Ok(access_denied_metadata());
    }
    require_success(&response, "secret metadata inspection")?;
    let metadata = parse_json::<MetadataResponse>(&response, "secret metadata response")?;
    let current = metadata.data.current_version;
    let version = metadata.data.versions.get(&current.to_string());
    if version.is_some_and(|version| version.destroyed || !version.deletion_time.is_empty()) {
        return Ok(unset_metadata());
    }
    Ok(SecretMetadata {
        status: SecretStatus::Set,
        revision: Some(current.to_string()),
        created_at_unix_ms: version.and_then(|version| parse_timestamp(&version.created_time)),
        candidate_revision: None,
    })
}

fn parse_health(response: OpenBaoHttpResponse) -> Result<OpenBaoHealth, MmError> {
    if !matches!(response.status, 200 | 429 | 472 | 473 | 501 | 503) {
        return Err(MmError::Secret {
            message: format!("provider health endpoint returned HTTP {}", response.status),
        });
    }
    let body = parse_json::<HealthResponse>(&response, "provider health response")?;
    let version = semver::Version::parse(body.version.trim_start_matches('v')).map_err(|_| {
        MmError::Security {
            message: "provider reported an invalid server version".to_owned(),
        }
    })?;
    if version.major != 2 || !matches!(version.minor, 3..=5) {
        return Err(MmError::Security {
            message: format!(
                "OpenBao {} is unsupported; tested server lines are 2.3.x, 2.4.x and 2.5.x",
                body.version
            ),
        });
    }
    Ok(OpenBaoHealth {
        initialized: body.initialized,
        sealed: body.sealed,
        standby: body.standby,
        version: body.version,
        healthy: response.status == 200 && body.initialized && !body.sealed,
    })
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    response: &OpenBaoHttpResponse,
    context: &str,
) -> Result<T, MmError> {
    serde_json::from_slice(&response.body).map_err(|_| MmError::Secret {
        message: format!("{context} was malformed"),
    })
}

fn require_success(response: &OpenBaoHttpResponse, operation: &str) -> Result<(), MmError> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else if response.status == 403 {
        Err(MmError::Security {
            message: format!("provider denied {operation}"),
        })
    } else {
        Err(MmError::Secret {
            message: format!("provider {operation} failed with HTTP {}", response.status),
        })
    }
}

fn expected_version(expected: &SecretExpectedState) -> Result<u64, MmError> {
    if expected.candidate_revision.is_some() {
        return Err(MmError::Secret {
            message: "provider does not support staged secret candidates".to_owned(),
        });
    }
    let revision = expected
        .active_revision
        .as_deref()
        .ok_or_else(lifecycle_conflict)?;
    match revision.parse::<u64>() {
        Ok(revision) if revision > 0 => Ok(revision),
        Ok(_) | Err(_) => Err(lifecycle_conflict()),
    }
}

fn validate_material(material: &SecretMaterial) -> Result<(), MmError> {
    let valid = material.expose(|value| {
        !value.is_empty()
            && value.len() <= MAX_OPENBAO_SECRET_BYTES
            && is_safe_environment_value(value)
    });
    if valid {
        Ok(())
    } else {
        Err(MmError::Secret {
            message: format!(
                "remote secret values must be non-empty text without control characters and at most {MAX_OPENBAO_SECRET_BYTES} bytes"
            ),
        })
    }
}

fn parse_timestamp(value: &str) -> Option<i64> {
    let time = match OffsetDateTime::parse(value, &Rfc3339) {
        Ok(time) => time,
        Err(_) => return None,
    };
    match i64::try_from(time.unix_timestamp_nanos() / 1_000_000) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::debug!(%error, "ignoring out-of-range OpenBao timestamp");
            None
        }
    }
}

fn lifecycle_conflict() -> MmError {
    MmError::Secret {
        message: "secret lifecycle changed since it was inspected; retry from current metadata"
            .to_owned(),
    }
}

fn unavailable(error: &MmError) -> bool {
    matches!(error, MmError::Secret { message } if message.contains("unavailable") || message.contains("sealed"))
}

fn unavailable_metadata() -> SecretMetadata {
    SecretMetadata {
        status: SecretStatus::ProviderUnavailable,
        revision: None,
        created_at_unix_ms: None,
        candidate_revision: None,
    }
}

fn unset_metadata() -> SecretMetadata {
    SecretMetadata {
        status: SecretStatus::Unset,
        revision: None,
        created_at_unix_ms: None,
        candidate_revision: None,
    }
}

fn access_denied_metadata() -> SecretMetadata {
    SecretMetadata {
        status: SecretStatus::AccessDenied,
        revision: None,
        created_at_unix_ms: None,
        candidate_revision: None,
    }
}

#[derive(Deserialize)]
struct LoginResponse {
    auth: LoginAuth,
}

#[derive(Deserialize)]
struct LoginAuth {
    client_token: Zeroizing<String>,
    renewable: bool,
    lease_duration: u64,
}

#[derive(Deserialize)]
struct HealthResponse {
    initialized: bool,
    sealed: bool,
    standby: bool,
    version: String,
}

#[derive(Deserialize)]
struct MetadataResponse {
    data: MetadataData,
}

#[derive(Deserialize)]
struct MetadataData {
    current_version: u64,
    versions: std::collections::BTreeMap<String, MetadataVersion>,
}

#[derive(Deserialize)]
struct MetadataVersion {
    created_time: String,
    deletion_time: String,
    destroyed: bool,
}

#[derive(Deserialize)]
struct ReadResponse {
    data: ReadData,
}

#[derive(Deserialize)]
struct ReadData {
    data: SecretData,
    metadata: ReadMetadata,
}

#[derive(Deserialize)]
struct SecretData {
    value: Zeroizing<String>,
    #[serde(default)]
    mm_revision: Option<String>,
}

impl ReadResponse {
    /// The revision a value was staged with, else its version.
    fn revision(&self) -> String {
        self.data
            .data
            .mm_revision
            .clone()
            .unwrap_or_else(|| self.data.metadata.version.to_string())
    }
}

#[derive(Deserialize)]
struct ReadMetadata {
    created_time: String,
    version: u64,
}

#[derive(Deserialize)]
struct WriteResponse {
    data: WriteData,
}

#[derive(Deserialize)]
struct WriteData {
    version: u64,
}

#[path = "provider/live.rs"]
mod live;

#[cfg(test)]
#[path = "provider/tests.rs"]
mod tests;
