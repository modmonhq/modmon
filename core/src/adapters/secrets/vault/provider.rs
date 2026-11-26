use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use zeroize::Zeroizing;

use crate::adapters::secrets::kv_author::{author_metadata, recorded_author};
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

use super::connection_store::{VaultConnection, VaultConnectionStore, VaultMode};
use super::http::{VaultHttpResponse, VaultHttpTransport};

const MAX_VAULT_SECRET_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct VaultHealth {
    pub initialized: bool,
    pub sealed: bool,
    pub standby: bool,
    pub version: String,
    pub healthy: bool,
}

#[derive(Clone)]
pub struct VaultSecretStore {
    instance: crate::domain::secret_backend::ProviderInstanceName,
    mode: VaultMode,
    project_id: ProjectId,
    connections: Arc<VaultConnectionStore>,
    transport: Arc<dyn VaultHttpTransport>,
    declared: Option<crate::domain::repo_target::DeclaredProviderAddress>,
}

impl std::fmt::Debug for VaultSecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultSecretStore")
            .field("mode", &self.mode)
            .field("project_id", &self.project_id)
            .finish_non_exhaustive()
    }
}

impl VaultSecretStore {
    #[must_use]
    pub fn new(
        instance: crate::domain::secret_backend::ProviderInstanceName,
        mode: VaultMode,
        project_id: ProjectId,
        connections: Arc<VaultConnectionStore>,
        transport: Arc<dyn VaultHttpTransport>,
    ) -> Self {
        Self {
            instance,
            mode,
            project_id,
            connections,
            transport,
            declared: None,
        }
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
    ) -> Result<VaultConnection, MmError> {
        if reference.provider().as_str() != self.instance.as_str() {
            return Err(MmError::Secret {
                message: format!(
                    "{} adapter received another provider's reference",
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
            |connection: &VaultConnection, environment| {
                crate::adapters::secrets::host_connection::ConnectionFacts {
                    project: connection.project_id,
                    address: connection.address.clone(),
                    has_environment: connection.credentials.contains_key(environment),
                }
            },
        )?;
        if connection.mode != self.mode || connection.project_id != self.project_id {
            return Err(MmError::Security {
                message: "private Vault connection mode or project ownership mismatch".to_owned(),
            });
        }
        Ok(connection)
    }

    pub async fn health_for_connection(
        &self,
        connection: &VaultConnection,
    ) -> Result<VaultHealth, MmError> {
        parse_health(
            self.transport
                .request(connection, Method::GET, "sys/health", None, None)
                .await?,
        )
    }

    async fn authenticated<T>(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        operation: impl AsyncFnOnce(&VaultConnection, &str, String) -> Result<T, MmError>,
    ) -> Result<T, MmError> {
        let connection = self.connection(reference, scope.environment())?;
        let health = self.health_for_connection(&connection).await?;
        if !health.initialized || health.sealed {
            return Err(MmError::Secret {
                message: "Vault is not initialized or is sealed".to_owned(),
            });
        }
        let token = self.login(&connection, scope.environment()).await?;
        operation(
            &connection,
            token.as_str(),
            data_path(&connection, self.project_id, scope, reference),
        )
        .await
    }

    async fn login(
        &self,
        connection: &VaultConnection,
        environment: &str,
    ) -> Result<Zeroizing<String>, MmError> {
        let credential = connection.credentials.get(environment).ok_or_else(|| {
            MmError::Security {
                message: format!(
                    "private Vault connection has no AppRole scoped to environment '{environment}'"
                ),
            }
        })?;
        let body = serde_json::json!({ "role_id": credential.role_id.as_str(), "secret_id": credential.secret_id.as_str() });
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
                message: "Vault AppRole authentication was denied".to_owned(),
            });
        }
        require_success(&response, "AppRole authentication")?;
        let login: LoginResponse = parse_json(&response, "Vault AppRole login response")?;
        if !login.auth.renewable
            || !(60..=3_600).contains(&login.auth.lease_duration)
            || login.auth.client_token.is_empty()
        {
            return Err(MmError::Security {
                message: "Vault issued a non-renewable or non-short-lived automation token"
                    .to_owned(),
            });
        }
        Ok(login.auth.client_token)
    }

    async fn metadata(
        &self,
        connection: &VaultConnection,
        token: &str,
        path: &str,
    ) -> Result<SecretMetadata, MmError> {
        self.metadata_state(connection, token, path)
            .await
            .map(|(metadata, _)| metadata)
    }

    async fn metadata_state(
        &self,
        connection: &VaultConnection,
        token: &str,
        path: &str,
    ) -> Result<(SecretMetadata, Option<u64>), MmError> {
        metadata_state_from_response(
            self.transport
                .request(
                    connection,
                    Method::GET,
                    &path.replacen("/data/", "/metadata/", 1),
                    Some(token),
                    None,
                )
                .await?,
        )
    }

    /// Records who wrote `version` in the secret's custom metadata. A store
    /// whose policy refuses the metadata write keeps the version without it.
    async fn record_author(
        &self,
        connection: &VaultConnection,
        token: &str,
        data_path: &str,
        (version, set_by): (u64, Option<&str>),
    ) {
        let Some(set_by) = set_by else {
            return;
        };
        let path = data_path.replacen("/data/", "/metadata/", 1);
        let body = author_metadata(version, set_by);
        match self
            .transport
            .request(connection, Method::POST, &path, Some(token), Some(&body))
            .await
        {
            Ok(response) if (200..300).contains(&response.status) => {}
            Ok(response) => {
                tracing::debug!(
                    status = response.status,
                    "Vault kept no writer for this secret version"
                );
            }
            Err(error) => {
                tracing::debug!(
                    code = error.code(),
                    "Vault kept no writer for this secret version"
                );
            }
        }
    }
}

#[async_trait]
impl SecretMetadataInspector for VaultSecretStore {
    async fn inspect(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        self.authenticated(scope, reference, async |connection, token, path| self.metadata(connection, token, &path).await).await.or_else(|error| {
            if matches!(&error, MmError::Secret { message } if message.contains("unavailable") || message.contains("sealed")) {
                Ok(metadata(SecretStatus::ProviderUnavailable, None, None))
            } else { Err(error) }
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for VaultSecretStore {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        self.authenticated(scope, reference, async |connection, token, path| {
            let response = self
                .transport
                .request(connection, Method::GET, &path, Some(token), None)
                .await?;
            if response.status == 404 {
                return Ok(SecretResolution {
                    metadata: metadata(SecretStatus::Unset, None, None),
                    material: None,
                });
            }
            if response.status == 403 {
                return Ok(SecretResolution {
                    metadata: metadata(SecretStatus::AccessDenied, None, None),
                    material: None,
                });
            }
            require_success(&response, "secret read")?;
            let read: ReadResponse = parse_json(&response, "Vault KV v2 read response")?;
            let revision = read.data.metadata.version.to_string();
            let created = parse_timestamp(&read.data.metadata.created_time);
            let value = read.data.data.value;
            if value.is_empty()
                || value.len() > MAX_VAULT_SECRET_BYTES
                || !is_safe_environment_value(&value)
            {
                return Ok(SecretResolution {
                    metadata: metadata(SecretStatus::Invalid, Some(revision), created),
                    material: None,
                });
            }
            Ok(SecretResolution {
                metadata: metadata(SecretStatus::Set, Some(revision), created),
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
impl SecretCreator for VaultSecretStore {
    async fn create(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        now_unix_ms: i64,
        set_by: Option<&str>,
    ) -> Result<SecretWriteResult, MmError> {
        validate_material(&material)?;
        self.authenticated(scope, reference, async |connection, token, path| {
            let body = material.expose(|value| serde_json::json!({ "options": {"cas": 0}, "data": {"value": value, "mm_created_at_unix_ms": now_unix_ms} }));
            let response = self.transport.request(connection, Method::POST, &path, Some(token), Some(&body)).await?;
            if matches!(response.status, 400 | 409) {
                let (current, current_version) = self.metadata_state(connection, token, &path).await?;
                if current.status == SecretStatus::Set && let Some(revision) = current.revision {
                    return Ok(SecretWriteResult { revision, changed: false });
                }
                if current.status == SecretStatus::Unset && let Some(current_version) = current_version {
                    let body = material.expose(|value| serde_json::json!({ "options": {"cas": current_version}, "data": {"value": value, "mm_created_at_unix_ms": now_unix_ms} }));
                    let response = self.transport.request(connection, Method::POST, &path, Some(token), Some(&body)).await?;
                    if matches!(response.status, 400 | 409) { return Err(lifecycle_conflict()); }
                    require_success(&response, "secret recreation")?;
                    let write: WriteResponse = parse_json(&response, "Vault KV v2 write response")?;
                    self.record_author(connection, token, &path, (write.data.version, set_by)).await;
                    return Ok(SecretWriteResult { revision: write.data.version.to_string(), changed: true });
                }
            }
            require_success(&response, "secret create")?;
            let write: WriteResponse = parse_json(&response, "Vault KV v2 write response")?;
            self.record_author(connection, token, &path, (write.data.version, set_by)).await;
            Ok(SecretWriteResult { revision: write.data.version.to_string(), changed: true })
        }).await
    }
}

#[async_trait]
impl SecretReplacer for VaultSecretStore {
    async fn replace(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
        set_by: Option<&str>,
    ) -> Result<String, MmError> {
        validate_material(&material)?;
        let version = expected_version(&expected)?;
        self.authenticated(scope, reference, async |connection, token, path| {
            let body = material.expose(|value| serde_json::json!({ "options": {"cas": version}, "data": {"value": value, "mm_created_at_unix_ms": now_unix_ms} }));
            let response = self.transport.request(connection, Method::POST, &path, Some(token), Some(&body)).await?;
            if matches!(response.status, 400 | 409) { return Err(lifecycle_conflict()); }
            require_success(&response, "secret replacement")?;
            let write: WriteResponse = parse_json(&response, "Vault KV v2 write response")?;
            self.record_author(connection, token, &path, (write.data.version, set_by)).await;
            Ok(write.data.version.to_string())
        }).await
    }
}

#[async_trait]
impl SecretRemover for VaultSecretStore {
    async fn unset(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        let version = expected_version(&expected)?;
        self.authenticated(scope, reference, async |connection, token, path| {
            let current = self.metadata(connection, token, &path).await?;
            match current.status {
                SecretStatus::Unset => return Ok(false),
                SecretStatus::Set | SecretStatus::Invalid
                    if current.revision.as_deref() == Some(version.to_string().as_str()) => {}
                _ => return Err(lifecycle_conflict()),
            }
            let response = self
                .transport
                .request(
                    connection,
                    Method::POST,
                    &path.replacen("/data/", "/delete/", 1),
                    Some(token),
                    Some(&serde_json::json!({"versions": [version]})),
                )
                .await?;
            require_success(&response, "secret deletion")?;
            if matches!(
                self.metadata(connection, token, &path).await?.status,
                SecretStatus::Set | SecretStatus::Invalid
            ) {
                return Err(lifecycle_conflict());
            }
            Ok(true)
        })
        .await
    }
}

pub fn vault_storage_descriptor(
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
        max_material_bytes: MAX_VAULT_SECRET_BYTES,
        max_location_bytes: 1024,
        environment_scoping: StorageEnvironmentScoping::Isolated,
        material_kinds: vec![SecretMaterialKind::Text],
        rotation: RotationCapability::Replace,
        authentication: AuthenticationKind::AppRoleEphemeralToken,
        metadata_without_material_resolution: true,
    })
}

fn data_path(
    connection: &VaultConnection,
    project_id: ProjectId,
    scope: &SecretScope,
    reference: &SecretReference,
) -> String {
    let location = reference
        .locator()
        .split('/')
        .filter(|part| !part.is_empty())
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "{}/data/mm/{}/{}/{}",
        encode_segment(&connection.mount),
        project_id.0,
        encode_segment(scope.environment()),
        location
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

fn metadata_state_from_response(
    response: VaultHttpResponse,
) -> Result<(SecretMetadata, Option<u64>), MmError> {
    if response.status == 404 {
        return Ok((metadata(SecretStatus::Unset, None, None), None));
    }
    if response.status == 403 {
        return Ok((metadata(SecretStatus::AccessDenied, None, None), None));
    }
    require_success(&response, "secret metadata inspection")?;
    let response: MetadataResponse = parse_json(&response, "Vault KV v2 metadata response")?;
    let current = response.data.current_version;
    let version = response.data.versions.get(&current.to_string());
    if version.is_some_and(|value| value.destroyed || !value.deletion_time.is_empty()) {
        return Ok((metadata(SecretStatus::Unset, None, None), Some(current)));
    }
    Ok((
        SecretMetadata {
            set_by: recorded_author(response.data.custom_metadata.as_ref(), current),
            ..metadata(
                SecretStatus::Set,
                Some(current.to_string()),
                version.and_then(|value| parse_timestamp(&value.created_time)),
            )
        },
        Some(current),
    ))
}

fn parse_health(response: VaultHttpResponse) -> Result<VaultHealth, MmError> {
    if !matches!(response.status, 200 | 429 | 472 | 473 | 501 | 503) {
        return Err(MmError::Secret {
            message: format!("Vault health endpoint returned HTTP {}", response.status),
        });
    }
    let body: HealthResponse = parse_json(&response, "Vault health response")?;
    let version = semver::Version::parse(body.version.trim_start_matches('v')).map_err(|_| {
        MmError::Security {
            message: "Vault reported an invalid server version".to_owned(),
        }
    })?;
    if version.major != 1 || !matches!(version.minor, 18 | 19) {
        return Err(MmError::Security {
            message: format!(
                "Vault {} is unsupported; tested Community server lines are 1.18.x and 1.19.x",
                body.version
            ),
        });
    }
    Ok(VaultHealth {
        initialized: body.initialized,
        sealed: body.sealed,
        standby: body.standby,
        version: body.version,
        healthy: response.status == 200 && body.initialized && !body.sealed,
    })
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    response: &VaultHttpResponse,
    context: &str,
) -> Result<T, MmError> {
    serde_json::from_slice(&response.body).map_err(|_| MmError::Secret {
        message: format!("{context} was malformed"),
    })
}

fn require_success(response: &VaultHttpResponse, operation: &str) -> Result<(), MmError> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else if response.status == 403 {
        Err(MmError::Security {
            message: format!("Vault denied {operation}"),
        })
    } else {
        Err(MmError::Secret {
            message: format!("Vault {operation} failed with HTTP {}", response.status),
        })
    }
}

fn expected_version(expected: &SecretExpectedState) -> Result<u64, MmError> {
    if expected.candidate_revision.is_some() {
        return Err(MmError::Secret {
            message: "Vault base driver does not support staged candidates".to_owned(),
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
    if material.expose(|value| {
        !value.is_empty()
            && value.len() <= MAX_VAULT_SECRET_BYTES
            && is_safe_environment_value(value)
    }) {
        Ok(())
    } else {
        Err(MmError::Secret {
            message: format!(
                "Vault secret values must be safe non-empty text of at most {MAX_VAULT_SECRET_BYTES} bytes"
            ),
        })
    }
}

fn metadata(
    status: SecretStatus,
    revision: Option<String>,
    created_at_unix_ms: Option<i64>,
) -> SecretMetadata {
    SecretMetadata {
        status,
        revision,
        created_at_unix_ms,
        candidate_revision: None,
        set_by: None,
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
            tracing::debug!(%error, "ignoring out-of-range Vault timestamp");
            None
        }
    }
}

fn lifecycle_conflict() -> MmError {
    MmError::Secret {
        message: "Vault secret lifecycle changed since inspection; retry from current metadata"
            .to_owned(),
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
    #[serde(default)]
    custom_metadata: Option<std::collections::BTreeMap<String, String>>,
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

#[cfg(test)]
#[path = "provider/tests.rs"]
mod tests;
