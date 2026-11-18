//! A deployment host's own connection to an external OpenBao or Vault.
//!
//! `mmd secrets connect` stores, for one project and environment, where the
//! host reaches the store, which certificate authority it trusts there, and
//! an AppRole credential. It lands in the same encrypted connection store the
//! provider adapters read (owner-only files under the daemon's mm home), under
//! a name that carries the project, so several projects on one host can each
//! use a provider called the same. Nothing here returns a credential.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use zeroize::Zeroizing;

use crate::app::ports::SecretMetadataInspector;
use crate::domain::error::MmError;
use crate::domain::repo_target::DeclaredProviderAddress;
use crate::domain::secret::{SecretReference, SecretScope, SecretStatus};
use crate::domain::secret_backend::ProviderInstanceName;
use crate::domain::secret_contract::StorageProviderId;
use crate::domain::workspace::ProjectId;

use super::openbao::{
    AppRoleCredential, OpenBaoConnection, OpenBaoConnectionStore, OpenBaoHttpTransport,
    OpenBaoMode, OpenBaoSecretStore, ca_sha256,
};
use super::vault::{
    VaultAppRoleCredential, VaultConnection, VaultConnectionStore, VaultHttpTransport, VaultMode,
    VaultSecretStore, vault_ca_sha256,
};

/// The store's own policy decides what the host's role reads; mm records
/// that it did not narrow it.
const ROLE_SCOPED_LOCATIONS: &str = "*";
/// A location nothing is stored at: reading its metadata proves the login.
const PROBE_LOCATION: &str = "mm-connection-check";

/// The name a host keeps its connection to `provider` under for `project`.
pub fn host_connection_name(provider: &str, project: ProjectId) -> Result<String, MmError> {
    // The end of the id: random in every UUID version mm creates.
    let project = project.0.simple().to_string();
    let name = format!("{provider}.{}", &project[project.len() - 12..]);
    if name.len() > 63 {
        return Err(MmError::Secret {
            message: format!(
                "provider name '{provider}' is too long for a host connection; use at most 50 characters"
            ),
        });
    }
    Ok(name)
}

/// Not `MmError::Secret`: a host hides those from its clients, and this one
/// has to reach the person who deploys. It names the project, environment
/// and provider, nothing a store holds.
fn no_connection(provider: &str, project: ProjectId, environment: &str) -> MmError {
    MmError::Environment {
        message: format!(
            "no connection to secret provider '{provider}' for environment '{environment}' here. On a deployment host, run as root: `mmd secrets connect --project {} --environment {environment} --provider {provider} --address URL --ca-file FILE` with the AppRole role ID and secret ID on stdin. On your own machine: `mm secret provider add {provider}`",
            project.0
        ),
    }
}

/// How a provider adapter looks at one stored connection.
pub(super) struct ConnectionFacts {
    pub project: ProjectId,
    pub address: String,
    pub has_environment: bool,
}

/// The connection a provider adapter uses for `name`: the one set up on this
/// machine for the project, else the host's own. A host's connection is used
/// only at the address the configuration declares for the environment.
pub(super) fn find_connection<C>(
    name: &str,
    project: ProjectId,
    environment: &str,
    declared: Option<&DeclaredProviderAddress>,
    has: impl Fn(&str) -> Result<bool, MmError>,
    get: impl Fn(&str) -> Result<C, MmError>,
    facts: impl Fn(&C, &str) -> ConnectionFacts,
) -> Result<C, MmError> {
    let own = if has(name)? { Some(get(name)?) } else { None };
    if let Some(own) = &own
        && facts(own, environment).project == project
    {
        return get(name);
    }
    if let Ok(host_name) = host_connection_name(name, project)
        && has(&host_name)?
    {
        let connection = get(&host_name)?;
        let facts = facts(&connection, environment);
        if facts.project == project && facts.has_environment {
            if let Some(declared) = declared
                && !declared.matches(environment, &facts.address)
            {
                return Err(MmError::Environment {
                    message: format!(
                        "this host reads secret provider '{name}' at another address than the configuration declares for environment '{environment}' ({}). Set the address the host uses under `targets.<target>.providers.{name}.address`, or on the host run `mmd secrets disconnect --project {} --provider {name}` and `mmd secrets connect` again with that address",
                        declared.for_environment(environment),
                        project.0
                    ),
                });
            }
            return Ok(connection);
        }
    }
    // A connection another project owns keeps the adapter's own refusal.
    own.map_or_else(|| Err(no_connection(name, project, environment)), Ok)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HostProviderDriver {
    OpenBao,
    Vault,
}

/// What `mmd secrets connect` was given.
pub struct HostProviderConnect {
    pub project: ProjectId,
    pub environment: String,
    pub provider: ProviderInstanceName,
    pub address: String,
    pub namespace: Option<String>,
    pub storage_mount: String,
    pub auth_mount: String,
    pub ca_pem: String,
    pub role_id: Zeroizing<String>,
    pub secret_id: Zeroizing<String>,
}

/// The AppRole credential as `mmd secrets connect` reads it from stdin: the
/// role ID on the first line and the secret ID on the second.
pub fn approle_from_lines(
    input: &Zeroizing<String>,
) -> Result<(Zeroizing<String>, Zeroizing<String>), MmError> {
    let mut lines = input.lines().map(str::trim).filter(|line| !line.is_empty());
    match (lines.next(), lines.next(), lines.next()) {
        (Some(role_id), Some(secret_id), None) => Ok((
            Zeroizing::new(role_id.to_owned()),
            Zeroizing::new(secret_id.to_owned()),
        )),
        _ => Err(MmError::Secret {
            message: "stdin must hold two lines: the AppRole role ID, then its secret ID"
                .to_owned(),
        }),
    }
}

impl std::fmt::Debug for HostProviderConnect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostProviderConnect")
            .field("project", &self.project)
            .field("environment", &self.environment)
            .field("provider", &self.provider)
            .field("role_id", &"[REDACTED]")
            .field("secret_id", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// What a host says about a connection: never the credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct HostProviderConnection {
    pub project: ProjectId,
    pub provider: ProviderInstanceName,
    pub driver: HostProviderDriver,
    pub address: String,
    pub trust_sha256: String,
    pub environments: Vec<String>,
    /// The store accepted the environment's AppRole login from this host.
    pub authenticated: bool,
}

fn other_project(provider: &ProviderInstanceName) -> MmError {
    MmError::Security {
        message: format!(
            "this host keeps a connection to provider '{provider}' for another project under the same short name; give the provider another name in this project"
        ),
    }
}

fn other_settings(provider: &ProviderInstanceName, project: ProjectId) -> MmError {
    MmError::Secret {
        message: format!(
            "this host already reads provider '{provider}' for this project at another address, mount or certificate authority; run `mmd secrets disconnect --project {} --provider {provider}` first",
            project.0
        ),
    }
}

fn not_proven(provider: &ProviderInstanceName, address: &str) -> MmError {
    MmError::Secret {
        message: format!(
            "provider '{provider}' at {address} did not answer this host as an initialized, unsealed store with a certificate from the CA file; nothing was stored. Check the address and `--ca-file`, then run the command again"
        ),
    }
}

fn reference(provider: &ProviderInstanceName) -> Result<SecretReference, MmError> {
    SecretReference::new(
        StorageProviderId::new(provider.as_str())
            .map_err(|message| MmError::Security { message })?,
        PROBE_LOCATION,
    )
    .map_err(|message| MmError::Security { message })
}

/// A login that worked answers `set`, `unset` or `access denied` for the
/// probe location; a store that did not answer is `unavailable`.
fn proven(
    inspected: Result<crate::domain::secret::SecretMetadata, MmError>,
    provider: &ProviderInstanceName,
    address: &str,
) -> Result<(), MmError> {
    match inspected?.status {
        SecretStatus::ProviderUnavailable => Err(not_proven(provider, address)),
        _ => Ok(()),
    }
}

/// Stores the environment's AppRole credential for an OpenBao the host
/// reaches at `request.address`, after the store accepted a login with it.
pub async fn connect_host_openbao(
    store: Arc<OpenBaoConnectionStore>,
    transport: Arc<dyn OpenBaoHttpTransport>,
    request: HostProviderConnect,
) -> Result<HostProviderConnection, MmError> {
    let name = host_connection_name(request.provider.as_str(), request.project)?;
    let previous = if store.has_connection(&name)? {
        Some(store.connection(&name)?)
    } else {
        None
    };
    let mut connection = OpenBaoConnection {
        mode: OpenBaoMode::External,
        address: request.address,
        namespace: request.namespace,
        mount: request.storage_mount,
        auth_mount: request.auth_mount,
        ca_sha256: ca_sha256(&request.ca_pem),
        ca_pem: request.ca_pem,
        project_id: request.project,
        credentials: BTreeMap::new(),
        allowed_locations: BTreeMap::new(),
        managed_installation: None,
    };
    if let Some(previous) = &previous {
        if previous.project_id != request.project {
            return Err(other_project(&request.provider));
        }
        if previous.address != connection.address
            || previous.namespace != connection.namespace
            || previous.mount != connection.mount
            || previous.auth_mount != connection.auth_mount
            || previous.ca_sha256 != connection.ca_sha256
        {
            return Err(other_settings(&request.provider, request.project));
        }
        connection.credentials.clone_from(&previous.credentials);
        connection
            .allowed_locations
            .clone_from(&previous.allowed_locations);
    }
    connection.credentials.insert(
        request.environment.clone(),
        AppRoleCredential {
            role_id: request.role_id,
            secret_id: request.secret_id,
        },
    );
    connection.allowed_locations.insert(
        request.environment.clone(),
        vec![ROLE_SCOPED_LOCATIONS.to_owned()],
    );
    let answer = HostProviderConnection {
        project: request.project,
        provider: request.provider.clone(),
        driver: HostProviderDriver::OpenBao,
        address: connection.address.clone(),
        trust_sha256: connection.ca_sha256.clone(),
        environments: connection.credentials.keys().cloned().collect(),
        authenticated: true,
    };
    store.save_connection(&name, connection)?;
    let probe = OpenBaoSecretStore::new(
        request.provider.clone(),
        OpenBaoMode::External,
        request.project,
        Arc::clone(&store),
        transport,
    );
    let inspected = probe
        .inspect(
            &SecretScope::new(request.environment.as_str()),
            &reference(&request.provider)?,
        )
        .await;
    if let Err(error) = proven(inspected, &request.provider, &answer.address) {
        match previous {
            Some(previous) => store.save_connection(&name, previous)?,
            None => {
                store.remove_connection(&name)?;
            }
        }
        return Err(error);
    }
    Ok(answer)
}

/// The same for Vault.
pub async fn connect_host_vault(
    store: Arc<VaultConnectionStore>,
    transport: Arc<dyn VaultHttpTransport>,
    request: HostProviderConnect,
) -> Result<HostProviderConnection, MmError> {
    let name = host_connection_name(request.provider.as_str(), request.project)?;
    let previous = if store.has_connection(&name)? {
        Some(store.connection(&name)?)
    } else {
        None
    };
    let mut connection = VaultConnection {
        mode: VaultMode::External,
        address: request.address,
        namespace: request.namespace,
        mount: request.storage_mount,
        auth_mount: request.auth_mount,
        ca_sha256: vault_ca_sha256(&request.ca_pem),
        ca_pem: request.ca_pem,
        project_id: request.project,
        credentials: BTreeMap::new(),
        allowed_locations: BTreeMap::new(),
        managed_installation: None,
    };
    if let Some(previous) = &previous {
        if previous.project_id != request.project {
            return Err(other_project(&request.provider));
        }
        if previous.address != connection.address
            || previous.namespace != connection.namespace
            || previous.mount != connection.mount
            || previous.auth_mount != connection.auth_mount
            || previous.ca_sha256 != connection.ca_sha256
        {
            return Err(other_settings(&request.provider, request.project));
        }
        connection.credentials.clone_from(&previous.credentials);
        connection
            .allowed_locations
            .clone_from(&previous.allowed_locations);
    }
    connection.credentials.insert(
        request.environment.clone(),
        VaultAppRoleCredential {
            role_id: request.role_id,
            secret_id: request.secret_id,
        },
    );
    connection.allowed_locations.insert(
        request.environment.clone(),
        vec![ROLE_SCOPED_LOCATIONS.to_owned()],
    );
    let answer = HostProviderConnection {
        project: request.project,
        provider: request.provider.clone(),
        driver: HostProviderDriver::Vault,
        address: connection.address.clone(),
        trust_sha256: connection.ca_sha256.clone(),
        environments: connection.credentials.keys().cloned().collect(),
        authenticated: true,
    };
    store.save_connection(&name, connection)?;
    let probe = VaultSecretStore::new(
        request.provider.clone(),
        VaultMode::External,
        request.project,
        Arc::clone(&store),
        transport,
    );
    let inspected = probe
        .inspect(
            &SecretScope::new(request.environment.as_str()),
            &reference(&request.provider)?,
        )
        .await;
    if let Err(error) = proven(inspected, &request.provider, &answer.address) {
        match previous {
            Some(previous) => store.save_connection(&name, previous)?,
            None => {
                store.remove_connection(&name)?;
            }
        }
        return Err(error);
    }
    Ok(answer)
}

/// Forgets the host's OpenBao connection for a project. Whether one existed.
pub fn disconnect_host_openbao(
    store: &OpenBaoConnectionStore,
    provider: &ProviderInstanceName,
    project: ProjectId,
) -> Result<bool, MmError> {
    let name = host_connection_name(provider.as_str(), project)?;
    if !store.has_connection(&name)? {
        return Ok(false);
    }
    store.remove_connection(&name)
}

/// Forgets the host's Vault connection for a project. Whether one existed.
pub fn disconnect_host_vault(
    store: &VaultConnectionStore,
    provider: &ProviderInstanceName,
    project: ProjectId,
) -> Result<bool, MmError> {
    let name = host_connection_name(provider.as_str(), project)?;
    if !store.has_connection(&name)? {
        return Ok(false);
    }
    store.remove_connection(&name)
}
