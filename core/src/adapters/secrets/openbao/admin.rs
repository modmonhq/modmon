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
    AppRoleCredential, OpenBaoConnection, OpenBaoConnectionStore, OpenBaoMode, ca_sha256,
    validate_path_segment,
};
use super::http::OpenBaoHttpTransport;
use super::provider::{OpenBaoHealth, OpenBaoSecretStore};

#[derive(Clone)]
pub struct OpenBaoConnectionAdministrator {
    connections: Arc<OpenBaoConnectionStore>,
    transport: Arc<dyn OpenBaoHttpTransport>,
}

impl std::fmt::Debug for OpenBaoConnectionAdministrator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenBaoConnectionAdministrator")
            .finish_non_exhaustive()
    }
}

pub struct OpenBaoSetupRequest {
    pub name: String,
    pub mode: OpenBaoMode,
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

impl std::fmt::Debug for OpenBaoSetupRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenBaoSetupRequest")
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct OpenBaoConnectionStatus {
    pub environment: String,
    pub health: OpenBaoHealth,
    pub authenticated: bool,
    pub isolated_path: String,
}

impl OpenBaoConnectionAdministrator {
    #[must_use]
    pub fn new(
        connections: Arc<OpenBaoConnectionStore>,
        transport: Arc<dyn OpenBaoHttpTransport>,
    ) -> Self {
        Self {
            connections,
            transport,
        }
    }

    pub async fn setup(
        &self,
        request: OpenBaoSetupRequest,
    ) -> Result<OpenBaoConnectionStatus, MmError> {
        validate_path_segment(&request.name, "connection name")?;
        validate_path_segment(&request.environment, "environment")?;
        validate_path_segment(&request.mount, "secret mount")?;
        validate_path_segment(&request.auth_mount, "AppRole auth mount")?;
        let locations = normalized_locations(request.locations)?;
        if request.developer_token.is_empty() {
            return Err(MmError::Security {
                message: "developer bootstrap token may not be empty".to_owned(),
            });
        }
        let calculated_pin = ca_sha256(&request.ca_pem);
        if calculated_pin != request.expected_ca_sha256 {
            return Err(MmError::Security {
                message: "provider TLS trust-bundle pin mismatch".to_owned(),
            });
        }
        let mut connection = OpenBaoConnection {
            mode: request.mode,
            address: request.address,
            namespace: request.namespace,
            mount: request.mount,
            auth_mount: request.auth_mount,
            ca_pem: request.ca_pem,
            ca_sha256: calculated_pin,
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
                message: "provider must be initialized and unsealed before connection setup"
                    .to_owned(),
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
            .save_connection(&request.name, connection.clone())?;
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
        connection: &OpenBaoConnection,
        project_id: ProjectId,
        environment: &str,
        developer_token: &str,
        locations: &[String],
    ) -> Result<AppRoleCredential, MmError> {
        let role = scoped_name("mm", project_id, environment);
        let policy = scoped_name("mm-policy", project_id, environment);
        let policy_body = serde_json::json!({
            "policy": environment_policy(connection, project_id, environment, locations)
        });
        self.successful_json(
            connection,
            Method::PUT,
            &format!("sys/policies/acl/{policy}"),
            developer_token,
            Some(&policy_body),
            "scoped policy configuration",
        )
        .await?;
        let role_body = serde_json::json!({
            "token_policies": [policy], "token_ttl": "15m", "token_max_ttl": "30m",
            "token_type": "service", "secret_id_num_uses": 0,
        });
        self.successful_json(
            connection,
            Method::POST,
            &format!("auth/{}/role/{role}", connection.auth_mount),
            developer_token,
            Some(&role_body),
            "environment AppRole configuration",
        )
        .await?;
        let role_id = self
            .successful_json(
                connection,
                Method::GET,
                &format!("auth/{}/role/{role}/role-id", connection.auth_mount),
                developer_token,
                None,
                "AppRole identifier retrieval",
            )
            .await?;
        let secret_id = self
            .successful_json(
                connection,
                Method::POST,
                &format!("auth/{}/role/{role}/secret-id", connection.auth_mount),
                developer_token,
                Some(&serde_json::json!({})),
                "AppRole secret identifier generation",
            )
            .await?;
        Ok(AppRoleCredential {
            role_id: parse::<RoleIdResponse>(&role_id, "AppRole identifier response")?
                .data
                .role_id,
            secret_id: parse::<SecretIdResponse>(&secret_id, "AppRole secret identifier response")?
                .data
                .secret_id,
        })
    }

    pub async fn status(
        &self,
        name: &str,
        mode: OpenBaoMode,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<OpenBaoConnectionStatus, MmError> {
        let connection = self.connections.connection(name)?;
        if connection.mode != mode || connection.project_id != project_id {
            return Err(MmError::Security {
                message: "private provider connection mode or project ownership mismatch"
                    .to_owned(),
            });
        }
        let health = self.health(&connection).await?;
        let instance = crate::domain::secret_backend::ProviderInstanceName::new(name)
            .map_err(|message| MmError::Security { message })?;
        let provider = OpenBaoSecretStore::new(
            instance.clone(),
            mode,
            project_id,
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        );
        let location = connection
            .allowed_locations
            .get(environment)
            .and_then(|locations| locations.first())
            .ok_or_else(|| MmError::Security {
                message:
                    "private provider connection has no declared location for this environment"
                        .to_owned(),
            })?;
        let reference = SecretReference::new(
            StorageProviderId::new(instance.as_str())
                .map_err(|message| MmError::Security { message })?,
            location,
        )
        .map_err(|message| MmError::Security { message })?;
        let metadata = provider
            .inspect(&SecretScope::new(environment), &reference)
            .await?;
        Ok(OpenBaoConnectionStatus {
            environment: environment.to_owned(),
            health,
            authenticated: matches!(metadata.status, SecretStatus::Unset | SecretStatus::Set),
            isolated_path: format!("{}/mm/{}/{}", connection.mount, project_id.0, environment),
        })
    }

    pub async fn health(&self, connection: &OpenBaoConnection) -> Result<OpenBaoHealth, MmError> {
        OpenBaoSecretStore::new(
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
        connection: &OpenBaoConnection,
        token: &str,
        bootstrap: bool,
    ) -> Result<(), MmError> {
        let mounts = self
            .transport
            .request(connection, Method::GET, "sys/mounts", Some(token), None)
            .await?;
        require_success(&mounts, "mount inspection")?;
        let mounts_json = parse::<serde_json::Value>(&mounts, "mount inspection response")?;
        let mount_key = format!("{}/", connection.mount);
        if !mounts_json
            .get("data")
            .and_then(|data| data.get(&mount_key))
            .is_some_and(|mount| {
                mount.get("type").and_then(serde_json::Value::as_str) == Some("kv")
                    && mount
                        .pointer("/options/version")
                        .and_then(serde_json::Value::as_str)
                        == Some("2")
            })
        {
            if !bootstrap {
                return Err(MmError::Security {
                    message: format!(
                        "KV v2 mount '{}' is unavailable; rerun the explicit developer bootstrap flow if server mutation is intended",
                        connection.mount
                    ),
                });
            }
            self.successful_json(
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
        let auth_json = parse::<serde_json::Value>(&auth, "auth mount response")?;
        let auth_key = format!("{}/", connection.auth_mount);
        if auth_json
            .get("data")
            .and_then(|data| data.get(&auth_key))
            .and_then(|auth| auth.get("type"))
            .and_then(serde_json::Value::as_str)
            != Some("approle")
        {
            if !bootstrap {
                return Err(MmError::Security {
                    message: format!(
                        "AppRole auth mount '{}' is unavailable; rerun the explicit developer bootstrap flow if server mutation is intended",
                        connection.auth_mount
                    ),
                });
            }
            self.successful_json(
                connection,
                Method::POST,
                &format!("sys/auth/{}", connection.auth_mount),
                token,
                Some(&serde_json::json!({"type": "approle"})),
                "AppRole auth bootstrap",
            )
            .await?;
        }
        Ok(())
    }

    async fn successful_json(
        &self,
        connection: &OpenBaoConnection,
        method: Method,
        path: &str,
        token: &str,
        body: Option<&serde_json::Value>,
        operation: &str,
    ) -> Result<super::http::OpenBaoHttpResponse, MmError> {
        let response = self
            .transport
            .request(connection, method, path, Some(token), body)
            .await?;
        require_success(&response, operation)?;
        Ok(response)
    }
}

pub fn environment_policy(
    connection: &OpenBaoConnection,
    project_id: ProjectId,
    environment: &str,
    locations: &[String],
) -> String {
    let prefix = format!("mm/{}/{}", project_id.0, environment);
    locations
        .iter()
        .map(|location| {
            let location = encoded_location(location);
            format!(
                "path \"{}/data/{prefix}/{location}\" {{ capabilities = [\"create\", \"read\", \"update\"] }}\npath \"{}/metadata/{prefix}/{location}\" {{ capabilities = [\"read\"] }}\npath \"{}/delete/{prefix}/{location}\" {{ capabilities = [\"update\"] }}\n",
                connection.mount, connection.mount, connection.mount
            )
        })
        .collect()
}

pub(super) fn normalized_locations(mut locations: Vec<String>) -> Result<Vec<String>, MmError> {
    locations.sort();
    locations.dedup();
    if locations.is_empty() {
        return Err(MmError::Security {
            message: "provider connection requires at least one declared secret location"
                .to_owned(),
        });
    }
    let validation_provider = StorageProviderId::new("location-validation")
        .map_err(|message| MmError::Security { message })?;
    for location in &locations {
        SecretReference::new(validation_provider.clone(), location)
            .map_err(|message| MmError::Security { message })?;
    }
    Ok(locations)
}

fn encoded_location(location: &str) -> String {
    location
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(super::provider::encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn scoped_name(prefix: &str, project_id: ProjectId, environment: &str) -> String {
    let input = format!("{}:{environment}", project_id.0);
    let digest = blake3::hash(input.as_bytes()).to_hex();
    format!("{prefix}-{}", &digest[..24])
}

fn ensure_same_connection(
    existing: &OpenBaoConnection,
    replacement: &OpenBaoConnection,
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
        Err(MmError::Security {
            message: "an existing private connection may not be rebound to another server, mode, project, namespace, mount, or TLS identity"
                .to_owned(),
        })
    }
}

fn require_success(
    response: &super::http::OpenBaoHttpResponse,
    operation: &str,
) -> Result<(), MmError> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else if response.status == 403 {
        Err(MmError::Security {
            message: format!("developer credential was denied during {operation}"),
        })
    } else {
        Err(MmError::Secret {
            message: format!("provider {operation} failed with HTTP {}", response.status),
        })
    }
}

fn parse<T: for<'de> Deserialize<'de>>(
    response: &super::http::OpenBaoHttpResponse,
    context: &str,
) -> Result<T, MmError> {
    serde_json::from_slice(&response.body).map_err(|_| MmError::Secret {
        message: format!("{context} was malformed"),
    })
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
