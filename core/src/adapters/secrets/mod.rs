mod age_store;
mod bundle;
mod environment;
mod file;
mod host_connection;
mod kv_author;
mod openbao;
mod registry;
mod runtime_delivery;
mod vault;

pub use crate::domain::secret::InitializedAgeStore;
pub use age_store::{
    AgeSecretStore, HostVariable, PreparedStage, SlotReport, StagedSlot, StoreChange,
    StoreStagedChange, age_storage_descriptor,
};
pub use bundle::{
    MAX_SEALED_SECRET_BUNDLE_BYTES, open_sealed, open_secret_bundle, open_staged_values,
    seal_secret_bundle, seal_staged_values, seal_to_recipient,
};
pub use environment::{
    EnvironmentSecretStore, environment_storage_descriptor, load_or_create_environment_revision_key,
};
pub use file::{FileSecretStore, HostFileSecrets, file_storage_descriptor};
pub use host_connection::{
    HostProviderConnect, HostProviderConnection, HostProviderDriver, approle_from_lines,
    connect_host_openbao, connect_host_vault, disconnect_host_openbao, disconnect_host_vault,
    host_connection_name,
};
pub use openbao::{
    AppRoleCredential, FileStoreKeeper, ManagedOpenBaoAction, ManagedOpenBaoManager,
    ManagedOpenBaoPlan, ManagedOpenBaoPlanRequest, ManagedOpenBaoResult, ManagedOpenBaoStatus,
    ManagedRecoveryMaterial, MemberRecord, NodeStore, OPENBAO_2_3_IMAGE, OPENBAO_2_4_IMAGE,
    OPENBAO_MANAGED_HOST, OpenBaoBackendUseCases, OpenBaoConnection,
    OpenBaoConnectionAdministrator, OpenBaoConnectionStatus, OpenBaoConnectionStore, OpenBaoHealth,
    OpenBaoHttpResponse, OpenBaoHttpTransport, OpenBaoMode, OpenBaoSecretStore,
    OpenBaoSetupRequest, ReqwestOpenBaoTransport, StoreJoinAnswer, StoreLayout, StoreUnsealer,
    ca_sha256, environment_policy, openbao_provider_descriptor, openbao_storage_descriptor,
    random_installation_name, render_config, render_unit,
};
pub use registry::{SecretProviderRegistry, StorageProviderRegistration};
pub use runtime_delivery::{
    DockerComposeFileSecretDelivery, PodmanSecretDelivery, compose_file_secret_descriptor,
    podman_native_secret_descriptor,
};
pub use vault::{
    ManagedVaultManager, ReqwestVaultTransport, VAULT_1_18_IMAGE, VAULT_1_19_IMAGE,
    VAULT_MANAGED_HOST, VaultAppRoleCredential, VaultBackendUseCases, VaultConnection,
    VaultConnectionAdministrator, VaultConnectionStore, VaultHealth, VaultHttpResponse,
    VaultHttpTransport, VaultMode, VaultSecretStore, vault_ca_sha256, vault_environment_policy,
    vault_provider_descriptor, vault_storage_descriptor,
};
