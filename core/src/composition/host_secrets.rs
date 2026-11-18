//! `mmd secrets`: the host side of pushing an environment's secrets and git
//! deploy key from the operator's machine. Values land in the same age store
//! the daemon resolves deploy secrets from: `<mm_home>/secrets/<project>.age`,
//! opened with the mm home's age identity. A host can also be connected to
//! an external OpenBao or Vault, which it then reads itself.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use super::providers::identity_path;
use crate::adapters::remote::{DaemonConfig, HostAccessStore};
use crate::adapters::secrets::{
    AgeSecretStore, HostProviderConnect, HostProviderConnection, HostProviderDriver,
    OpenBaoConnectionStore, ReqwestOpenBaoTransport, ReqwestVaultTransport, VaultConnectionStore,
};
use crate::adapters::state::MmHome;
use crate::domain::error::MmError;
use crate::domain::secret::{
    DeployKeyImport, SecretBundle, SecretImportEntry, SecretImportSummary, SecretScope,
};
use crate::domain::secret_backend::ProviderInstanceName;
use crate::domain::workspace::ProjectId;

pub(super) fn host_store(mm_home: &Path, project: ProjectId) -> Result<AgeSecretStore, MmError> {
    let home = MmHome::resolve(Some(mm_home))?;
    Ok(AgeSecretStore::new(
        identity_path(&home),
        home.secrets_directory.join(format!("{}.age", project.0)),
    ))
}

/// Creates the project's host age store when missing and returns the
/// recipient a bundle for `environment` is sealed to.
pub fn host_secret_recipient(
    config: &DaemonConfig,
    project: ProjectId,
    environment: &str,
) -> Result<String, MmError> {
    crate::domain::host_policy::validate_environment_name(environment)
        .map_err(|message| MmError::Environment { message })?;
    host_store(&config.mm_home, project)?.ensure_recipient()
}

/// Opens a sealed bundle for an environment this host runs, mirrors its
/// secrets with their revisions, and installs its deploy key.
pub fn import_host_secrets(
    config: &DaemonConfig,
    project: ProjectId,
    environment: &str,
    sealed: &[u8],
    now_unix_ms: i64,
) -> Result<SecretImportSummary, MmError> {
    let access = HostAccessStore::new(config);
    ensure_enrolled(config, project, environment)?;
    let store = host_store(&config.mm_home, project)?;
    let bundle = store.open_secret_bundle(sealed)?;
    bundle
        .ensure_scope(project, environment)
        .map_err(|message| MmError::Security { message })?;
    let SecretBundle {
        secrets,
        deploy_key,
        ..
    } = bundle;

    let references = secrets
        .iter()
        .map(|secret| format!("age://{}", secret.reference.locator()))
        .collect::<Vec<_>>();
    let results = store.mirror(&SecretScope::new(environment), secrets, now_unix_ms)?;
    let secrets = references
        .into_iter()
        .zip(results)
        .map(|(reference, result)| {
            (
                reference,
                SecretImportEntry {
                    revision: result.revision,
                    changed: result.changed,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let deploy_key = deploy_key
        .map(|key| {
            let changed = access.install_deploy_key(project, &key.private_key, &key.public_key)?;
            Ok::<_, MmError>(DeployKeyImport {
                public_key: key.public_key.trim().to_owned(),
                changed,
            })
        })
        .transpose()?;
    Ok(SecretImportSummary {
        project,
        environment: environment.to_owned(),
        secrets,
        deploy_key,
    })
}

fn ensure_enrolled(
    config: &DaemonConfig,
    project: ProjectId,
    environment: &str,
) -> Result<(), MmError> {
    let (host, _) = HostAccessStore::new(config).load_host()?;
    if host
        .projects
        .get(&project)
        .is_some_and(|enrolled| enrolled.environments.contains(environment))
    {
        return Ok(());
    }
    Err(MmError::Environment {
        message: format!(
            "this host does not run {environment} for project {}; run `mmd access enroll` first",
            project.0
        ),
    })
}

/// `mmd secrets connect`: lets this host read an external OpenBao or Vault
/// for one environment it runs. The AppRole credential is stored encrypted
/// under the daemon's mm home, readable by root only, after the store
/// accepted a login with it; the answer never carries it.
pub async fn connect_host_provider(
    config: &DaemonConfig,
    driver: HostProviderDriver,
    request: HostProviderConnect,
) -> Result<HostProviderConnection, MmError> {
    crate::domain::host_policy::validate_environment_name(&request.environment)
        .map_err(|message| MmError::Environment { message })?;
    crate::domain::repo_target::validate_provider_address(&request.address).map_err(|problem| {
        MmError::Secret {
            message: format!("--address {problem}"),
        }
    })?;
    ensure_enrolled(config, request.project, &request.environment)?;
    let home = MmHome::resolve(Some(&config.mm_home))?;
    match driver {
        HostProviderDriver::OpenBao => {
            crate::adapters::secrets::connect_host_openbao(
                Arc::new(OpenBaoConnectionStore::new(
                    home.identities_directory.join("openbao-connections.key"),
                    home.secrets_directory.join("openbao-connections.age"),
                )),
                Arc::new(ReqwestOpenBaoTransport),
                request,
            )
            .await
        }
        HostProviderDriver::Vault => {
            crate::adapters::secrets::connect_host_vault(
                Arc::new(VaultConnectionStore::new(
                    home.identities_directory.join("vault-connections.key"),
                    home.secrets_directory.join("vault-connections.age"),
                )),
                Arc::new(ReqwestVaultTransport),
                request,
            )
            .await
        }
    }
}

/// `mmd secrets disconnect`: forgets the host's connection to a provider
/// for a project, with every environment's credential. Whether one existed.
pub fn disconnect_host_provider(
    config: &DaemonConfig,
    driver: HostProviderDriver,
    provider: &ProviderInstanceName,
    project: ProjectId,
) -> Result<bool, MmError> {
    let home = MmHome::resolve(Some(&config.mm_home))?;
    match driver {
        HostProviderDriver::OpenBao => crate::adapters::secrets::disconnect_host_openbao(
            &OpenBaoConnectionStore::new(
                home.identities_directory.join("openbao-connections.key"),
                home.secrets_directory.join("openbao-connections.age"),
            ),
            provider,
            project,
        ),
        HostProviderDriver::Vault => crate::adapters::secrets::disconnect_host_vault(
            &VaultConnectionStore::new(
                home.identities_directory.join("vault-connections.key"),
                home.secrets_directory.join("vault-connections.age"),
            ),
            provider,
            project,
        ),
    }
}

#[cfg(test)]
#[path = "host_secrets_tests.rs"]
mod tests;
