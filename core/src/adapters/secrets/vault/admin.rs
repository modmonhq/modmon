use std::collections::BTreeMap;
use std::sync::Arc;

use reqwest::Method;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::app::ports::SecretMetadataInspector;
use crate::domain::error::MmError;
use crate::domain::secret::{SecretReference, SecretScope, SecretStatus};
use crate::domain::secret_contract::StorageProviderId;
use crate::domain::workspace::ProjectId;

use super::connection_store::{
    VaultAppRoleCredential, VaultConnection, VaultConnectionStore, VaultMode, ca_sha256,
    validate_name, validate_segment,
};
use super::http::{VaultHttpResponse, VaultHttpTransport};
use super::provider::{VaultHealth, VaultSecretStore};

pub(super) struct VaultSetupRequest {
    pub name: String,
    pub mode: VaultMode,
    pub address: String,
    pub namespace: Option<String>,
    pub mount: String,
    pub auth_mount: String,
    pub ca_pem: String,
    pub expected_ca_sha256: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub locations: Vec<String>,
    pub developer_token: Zeroizing<String>,
    pub allow_mount_bootstrap: bool,
    pub managed_installation: Option<String>,
}

impl std::fmt::Debug for VaultSetupRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultSetupRequest")
            .field("name", &self.name)
            .field("mode", &self.mode)
            .field("address", &"[PRIVATE]")
            .field("namespace", &self.namespace.as_ref().map(|_| "[PRIVATE]"))
            .field("mount", &self.mount)
            .field("auth_mount", &self.auth_mount)
            .field("ca_pem", &"[PRIVATE]")
            .field("expected_ca_sha256", &self.expected_ca_sha256)
            .field("project_id", &self.project_id)
            .field("environment", &self.environment)
            .field("locations", &self.locations)
            .field("developer_token", &"[REDACTED]")
            .field("allow_mount_bootstrap", &self.allow_mount_bootstrap)
            .field("managed_installation", &self.managed_installation)
            .finish()
    }
}

pub(super) struct VaultConnectionStatus {
    pub environment: String,
    pub health: VaultHealth,
    pub authenticated: bool,
    pub isolated_path: String,
}

#[derive(Clone)]
pub struct VaultConnectionAdministrator {
    connections: Arc<VaultConnectionStore>,
    transport: Arc<dyn VaultHttpTransport>,
}

impl std::fmt::Debug for VaultConnectionAdministrator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultConnectionAdministrator")
            .finish_non_exhaustive()
    }
}

impl VaultConnectionAdministrator {
    #[must_use]
    pub fn new(
        connections: Arc<VaultConnectionStore>,
        transport: Arc<dyn VaultHttpTransport>,
    ) -> Self {
        Self {
            connections,
            transport,
        }
    }

    pub(super) async fn setup(
        &self,
        request: VaultSetupRequest,
    ) -> Result<VaultConnectionStatus, MmError> {
        validate_name(&request.name)?;
        validate_segment(&request.environment, "Vault environment")?;
        validate_segment(&request.mount, "Vault KV mount")?;
        validate_segment(&request.auth_mount, "Vault AppRole mount")?;
        let locations = normalized_locations(request.locations)?;
        if request.developer_token.is_empty() {
            return Err(MmError::Security {
                message: "Vault developer bootstrap token may not be empty".to_owned(),
            });
        }
        let pin = ca_sha256(&request.ca_pem);
        if pin != request.expected_ca_sha256 {
            return Err(MmError::Security {
                message: "Vault TLS trust-bundle pin mismatch".to_owned(),
            });
        }
        let mut connection = VaultConnection {
            mode: request.mode,
            address: request.address,
            namespace: request.namespace,
            mount: request.mount,
            auth_mount: request.auth_mount,
            ca_pem: request.ca_pem,
            ca_sha256: pin,
            project_id: request.project_id,
            credentials: BTreeMap::new(),
            allowed_locations: BTreeMap::new(),
            managed_installation: request.managed_installation,
        };
        let existing = if self.connections.has_connection(&request.name)? {
            Some(self.connections.connection(&request.name)?)
        } else {
            None
        };
        if let Some(existing) = &existing {
            ensure_same_connection(existing, &connection)?;
        }
        let health = self.health(&connection).await?;
        if !health.initialized || health.sealed {
            return Err(MmError::Security {
                message: "Vault must be initialized and unsealed before setup".to_owned(),
            });
        }
        self.ensure_mounts(
            &connection,
            request.developer_token.as_str(),
            request.allow_mount_bootstrap,
        )
        .await?;
        let credential = self
            .provision_approle(
                &connection,
                request.project_id,
                &request.environment,
                request.developer_token.as_str(),
                &locations,
            )
            .await?;
        if let Some(existing) = existing {
            connection.credentials = existing.credentials;
            connection.allowed_locations = existing.allowed_locations;
        }
        connection
            .credentials
            .insert(request.environment.clone(), credential);
        connection
            .allowed_locations
            .insert(request.environment.clone(), locations);
        self.connections
            .save_connection(&request.name, connection)?;
        self.status(
            &request.name,
            request.mode,
            request.project_id,
            &request.environment,
        )
        .await
    }

    async fn provision_approle(
        &self,
        connection: &VaultConnection,
        project_id: ProjectId,
        environment: &str,
        developer_token: &str,
        locations: &[String],
    ) -> Result<VaultAppRoleCredential, MmError> {
        let role = scoped_name("mm", project_id, environment);
        let policy = scoped_name("mm-policy", project_id, environment);
        self.successful(
            connection,
            Method::PUT,
            &format!("sys/policies/acl/{policy}"),
            developer_token,
            Some(&serde_json::json!({
                "policy": environment_policy(connection, project_id, environment, locations)
            })),
            "scoped policy configuration",
        )
        .await?;
        self.successful(connection, Method::POST, &format!("auth/{}/role/{role}", connection.auth_mount), developer_token, Some(&serde_json::json!({
            "token_policies": [policy], "token_ttl": "15m", "token_max_ttl": "30m", "token_type": "service", "secret_id_num_uses": 0
        })), "environment AppRole configuration").await?;
        let role_response = self
            .successful(
                connection,
                Method::GET,
                &format!("auth/{}/role/{role}/role-id", connection.auth_mount),
                developer_token,
                None,
                "AppRole identifier retrieval",
            )
            .await?;
        let secret_response = self
            .successful(
                connection,
                Method::POST,
                &format!("auth/{}/role/{role}/secret-id", connection.auth_mount),
                developer_token,
                Some(&serde_json::json!({})),
                "AppRole secret generation",
            )
            .await?;
        Ok(VaultAppRoleCredential {
            role_id: parse::<RoleIdResponse>(&role_response, "Vault AppRole identifier response")?
                .data
                .role_id,
            secret_id: parse::<SecretIdResponse>(
                &secret_response,
                "Vault AppRole secret response",
            )?
            .data
            .secret_id,
        })
    }

    pub(super) async fn status(
        &self,
        name: &str,
        mode: VaultMode,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<VaultConnectionStatus, MmError> {
        let connection = self.connections.connection(name)?;
        if connection.mode != mode || connection.project_id != project_id {
            return Err(MmError::Security {
                message: "private Vault connection mode or project ownership mismatch".to_owned(),
            });
        }
        let health = self.health(&connection).await?;
        let location = connection
            .allowed_locations
            .get(environment)
            .and_then(|values| values.first())
            .ok_or_else(|| MmError::Security {
                message: "private Vault connection has no exact location for this environment"
                    .to_owned(),
            })?;
        let instance = crate::domain::secret_backend::ProviderInstanceName::new(name)
            .map_err(|message| MmError::Security { message })?;
        let provider = VaultSecretStore::new(
            instance.clone(),
            mode,
            project_id,
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        );
        let reference = SecretReference::new(
            StorageProviderId::new(instance.as_str())
                .map_err(|message| MmError::Security { message })?,
            location,
        )
        .map_err(|message| MmError::Security { message })?;
        let metadata = provider
            .inspect(&SecretScope::new(environment), &reference)
            .await?;
        Ok(VaultConnectionStatus {
            environment: environment.to_owned(),
            health,
            authenticated: matches!(metadata.status, SecretStatus::Set | SecretStatus::Unset),
            isolated_path: format!("{}/mm/{}/{}", connection.mount, project_id.0, environment),
        })
    }

    pub(super) async fn health(
        &self,
        connection: &VaultConnection,
    ) -> Result<VaultHealth, MmError> {
        VaultSecretStore::new(
            crate::domain::secret_backend::ProviderInstanceName::new("provider")
                .map_err(|message| MmError::Security { message })?,
            connection.mode,
            connection.project_id,
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        )
        .health_for_connection(connection)
        .await
    }

    async fn ensure_mounts(
        &self,
        connection: &VaultConnection,
        token: &str,
        bootstrap: bool,
    ) -> Result<(), MmError> {
        let mounts = self
            .transport
            .request(connection, Method::GET, "sys/mounts", Some(token), None)
            .await?;
        require_success(&mounts, "mount inspection")?;
        let mounts_json: serde_json::Value = parse(&mounts, "Vault mount response")?;
        let mount_key = format!("{}/", connection.mount);
        let has_kv2 = mounts_json
            .get("data")
            .and_then(|data| data.get(&mount_key))
            .is_some_and(|mount| {
                mount
                    .pointer("/options/version")
                    .and_then(serde_json::Value::as_str)
                    == Some("2")
                    && mount.get("type").and_then(serde_json::Value::as_str) == Some("kv")
            });
        if !has_kv2 {
            if !bootstrap {
                return Err(MmError::Security {
                    message: format!(
                        "Vault KV v2 mount '{}' is missing; use explicit bootstrap to create it",
                        connection.mount
                    ),
                });
            }
            self.successful(
                connection,
                Method::POST,
                &format!("sys/mounts/{}", connection.mount),
                token,
                Some(&serde_json::json!({"type": "kv", "options": {"version": "2"}})),
                "KV v2 mount bootstrap",
            )
            .await?;
        }
        let auth = self
            .transport
            .request(connection, Method::GET, "sys/auth", Some(token), None)
            .await?;
        require_success(&auth, "auth mount inspection")?;
        let auth_json: serde_json::Value = parse(&auth, "Vault auth response")?;
        let auth_key = format!("{}/", connection.auth_mount);
        let has_approle = auth_json
            .get("data")
            .and_then(|data| data.get(&auth_key))
            .and_then(|auth| auth.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("approle");
        if !has_approle {
            if !bootstrap {
                return Err(MmError::Security {
                    message: format!(
                        "Vault AppRole mount '{}' is missing; use explicit bootstrap to create it",
                        connection.auth_mount
                    ),
                });
            }
            self.successful(
                connection,
                Method::POST,
                &format!("sys/auth/{}", connection.auth_mount),
                token,
                Some(&serde_json::json!({"type": "approle"})),
                "AppRole mount bootstrap",
            )
            .await?;
        }
        Ok(())
    }

    async fn successful(
        &self,
        connection: &VaultConnection,
        method: Method,
        path: &str,
        token: &str,
        body: Option<&serde_json::Value>,
        operation: &str,
    ) -> Result<VaultHttpResponse, MmError> {
        let response = self
            .transport
            .request(connection, method, path, Some(token), body)
            .await?;
        require_success(&response, operation)?;
        Ok(response)
    }
}

pub(super) fn environment_policy(
    connection: &VaultConnection,
    project_id: ProjectId,
    environment: &str,
    locations: &[String],
) -> String {
    let prefix = format!("mm/{}/{}", project_id.0, encode_segment(environment));
    locations.iter().map(|location| {
        let location = location.split('/').filter(|part| !part.is_empty()).map(encode_segment).collect::<Vec<_>>().join("/");
        format!(
            "path \"{}/data/{prefix}/{location}\" {{ capabilities = [\"create\", \"read\", \"update\"] }}\npath \"{}/metadata/{prefix}/{location}\" {{ capabilities = [\"read\", \"update\"] }}\npath \"{}/delete/{prefix}/{location}\" {{ capabilities = [\"update\"] }}\n",
            connection.mount, connection.mount, connection.mount
        )
    }).collect()
}

pub(super) fn normalized_locations(mut locations: Vec<String>) -> Result<Vec<String>, MmError> {
    locations.sort();
    locations.dedup();
    if locations.is_empty() {
        return Err(MmError::Security {
            message: "Vault connection requires at least one exact secret location".to_owned(),
        });
    }
    let validation = StorageProviderId::new("vault-location-validation")
        .map_err(|message| MmError::Security { message })?;
    for location in &locations {
        SecretReference::new(validation.clone(), location)
            .map_err(|message| MmError::Security { message })?;
    }
    Ok(locations)
}

fn scoped_name(prefix: &str, project_id: ProjectId, environment: &str) -> String {
    let digest = blake3::hash(format!("{}:{environment}", project_id.0).as_bytes()).to_hex();
    format!("{prefix}-{}", &digest[..24])
}

fn ensure_same_connection(
    existing: &VaultConnection,
    replacement: &VaultConnection,
) -> Result<(), MmError> {
    if existing.mode == replacement.mode
        && existing.address == replacement.address
        && existing.namespace == replacement.namespace
        && existing.mount == replacement.mount
        && existing.auth_mount == replacement.auth_mount
        && existing.ca_sha256 == replacement.ca_sha256
        && existing.project_id == replacement.project_id
        && existing.managed_installation == replacement.managed_installation
    {
        Ok(())
    } else {
        Err(MmError::Security { message: "an existing private Vault connection cannot be rebound to another server, mode, project, namespace, mount, or TLS identity".to_owned() })
    }
}

fn require_success(response: &VaultHttpResponse, operation: &str) -> Result<(), MmError> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else if response.status == 403 {
        Err(MmError::Security {
            message: format!("Vault developer credential was denied during {operation}"),
        })
    } else {
        Err(MmError::Secret {
            message: format!("Vault {operation} failed with HTTP {}", response.status),
        })
    }
}

fn parse<T: for<'de> Deserialize<'de>>(
    response: &VaultHttpResponse,
    context: &str,
) -> Result<T, MmError> {
    serde_json::from_slice(&response.body).map_err(|_| MmError::Secret {
        message: format!("{context} was malformed"),
    })
}

fn encode_segment(value: &str) -> String {
    super::provider::encode_segment(value)
}

#[derive(Deserialize)]
struct RoleIdResponse {
    data: RoleIdData,
}
#[derive(Deserialize)]
struct RoleIdData {
    role_id: Zeroizing<String>,
}
#[derive(Deserialize)]
struct SecretIdResponse {
    data: SecretIdData,
}
#[derive(Deserialize)]
struct SecretIdData {
    secret_id: Zeroizing<String>,
}

#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
