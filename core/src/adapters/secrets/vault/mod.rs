mod admin;
mod connection_store;
mod http;
mod managed;
mod provider;
mod usecase;

pub use admin::VaultConnectionAdministrator;
pub use connection_store::{
    VaultAppRoleCredential, VaultConnection, VaultConnectionStore, VaultMode,
    ca_sha256 as vault_ca_sha256,
};
pub use http::{ReqwestVaultTransport, VaultHttpResponse, VaultHttpTransport};
pub use managed::{ManagedVaultManager, VAULT_1_18_IMAGE, VAULT_1_19_IMAGE, VAULT_MANAGED_HOST};
pub use provider::{VaultHealth, VaultSecretStore, vault_storage_descriptor};
pub use usecase::{VaultBackendUseCases, vault_provider_descriptor};

pub fn vault_environment_policy(
    connection: &VaultConnection,
    project_id: crate::domain::workspace::ProjectId,
    environment: &str,
    locations: &[String],
) -> String {
    admin::environment_policy(connection, project_id, environment, locations)
}
