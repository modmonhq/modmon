use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use atomic_write_file::AtomicWriteFile;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize as _, Zeroizing};

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_private_file, ensure_user_controlled_directory,
    read_regular_file,
};
use crate::app::ports::ManagedServiceRuntime;
use crate::domain::error::MmError;
use crate::domain::managed_service::{
    ManagedServiceFile, ManagedServicePlan, ManagedServicePlanRequest,
};
use crate::domain::secret_backend::ManagedSecretBackendAction;
use crate::domain::workspace::ProjectId;

use super::admin::{VaultConnectionAdministrator, VaultSetupRequest, normalized_locations};
use super::connection_store::{
    VaultConnection, VaultConnectionStore, VaultMode, VaultRecoveryMaterial, ca_sha256,
    validate_name, validate_segment,
};
use super::http::VaultHttpTransport;
use super::provider::VaultHealth;

pub const VAULT_1_18_IMAGE: &str = "docker.io/hashicorp/vault@sha256:750bb37c1638fa194ab37053a81618c61bb0491ddec6fccac87c07a8e6cd8166";
pub const VAULT_1_19_IMAGE: &str = "docker.io/hashicorp/vault@sha256:c4298db7f9b2ea8cab452cbff5877749087913aa035fcae62026cf16132929f5";
pub const VAULT_MANAGED_HOST: &str = "vault.mm.test";
const STATE_SCHEMA: u32 = 1;
const BACKUP_SCHEMA: u32 = 1;
const DEFAULT_MOUNT: &str = "mm-kv";
const DEFAULT_AUTH_MOUNT: &str = "mm-approle";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ManagedVaultPlan {
    schema: u32,
    pub action: ManagedSecretBackendAction,
    pub installation: String,
    pub connection: String,
    project_id: ProjectId,
    environment: String,
    locations: Vec<String>,
    pub image: String,
    pub previous_image: Option<String>,
    host_port: u16,
    pub volume: String,
    config_volume: String,
    pub ingress_host: String,
    pub management_access: String,
    pub purge_data: bool,
    previous_state_digest: String,
    pub service: ManagedServicePlan,
    pub plan_hash: String,
}

impl ManagedVaultPlan {
    fn hash(&self) -> Result<String, MmError> {
        let mut value = self.clone();
        value.plan_hash.clear();
        let bytes = serde_json::to_vec(&value).map_err(|error| MmError::State {
            message: format!("could not serialize managed Vault plan: {error}"),
        })?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    fn validate(&self) -> Result<(), MmError> {
        if self.schema != STATE_SCHEMA
            || !supported_image(&self.image)
            || self.host_port == 0
            || !self.ingress_host.ends_with(".vault.mm.test")
            || self.management_access != "local-loopback-only"
            || (self.action == ManagedSecretBackendAction::Install && self.locations.is_empty())
            || self.service.identity.provider_instance != self.connection
            || self.service.identity.component != "vault"
            || self.service.identity.data_volume != self.volume
            || self.service.identity.config_volume != self.config_volume
            || self.hash()? != self.plan_hash
        {
            return Err(MmError::Security {
                message: "managed Vault plan is invalid, unpinned, or changed".to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedVaultState {
    schema: u32,
    installation: String,
    connection: String,
    project_id: ProjectId,
    environment: String,
    locations: Vec<String>,
    image: String,
    host_port: u16,
    container: String,
    volume: String,
    config_volume: String,
    config_directory: PathBuf,
    ingress_host: String,
    service: ManagedServicePlan,
    plan_hash: String,
    journal: Journal,
    previous: Option<Box<ManagedVaultState>>,
}

impl ManagedVaultState {
    fn digest(&self) -> Result<String, MmError> {
        let bytes = serde_json::to_vec(self).map_err(|error| MmError::State {
            message: format!("could not serialize managed Vault state: {error}"),
        })?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Journal {
    Applying,
    Applied,
    Removing,
    Retained,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedBackup {
    schema: u32,
    installation: String,
    project_id: ProjectId,
    environment: String,
    connection: VaultConnection,
    recovery: VaultRecoveryMaterial,
    snapshot: Zeroizing<Vec<u8>>,
}

#[derive(Clone)]
pub(super) struct ManagedVaultRequest {
    pub action: ManagedSecretBackendAction,
    pub installation: String,
    pub connection: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub locations: Vec<String>,
    pub image: String,
    pub host_port: u16,
    pub purge_data: bool,
    pub target: String,
}

#[derive(Clone)]
pub struct ManagedVaultManager {
    root: PathBuf,
    connections: Arc<VaultConnectionStore>,
    transport: Arc<dyn VaultHttpTransport>,
    runtime: Arc<dyn ManagedServiceRuntime>,
}

impl std::fmt::Debug for ManagedVaultManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedVaultManager")
            .field("root", &self.root)
            .field("runtime", &self.runtime.kind())
            .finish_non_exhaustive()
    }
}

pub(super) struct ManagedVaultStatus {
    pub installed: bool,
    pub installation: Option<String>,
    pub connection: Option<String>,
    pub image: Option<String>,
    pub volume: Option<String>,
    pub ingress_url: String,
    pub management_url: Option<String>,
    pub health: Option<VaultHealth>,
    pub recovery: bool,
    pub journal: Option<String>,
    pub diagnostics: Vec<String>,
}

include!("managed/planning.rs");
include!("managed/install.rs");
include!("managed/lifecycle.rs");

impl ManagedVaultManager {
    managed_vault_planning!();
    managed_vault_install!();
    managed_vault_lifecycle!();
}

fn lifecycle_generation(
    request: &ManagedVaultRequest,
    previous_state_digest: &str,
) -> Result<String, MmError> {
    let canonical = serde_json::to_vec(&(
        request.action,
        &request.installation,
        &request.connection,
        request.project_id,
        &request.environment,
        &request.locations,
        &request.image,
        request.host_port,
        request.purge_data,
        &request.target,
        previous_state_digest,
    ))
    .map_err(|error| MmError::State {
        message: format!("could not canonicalize managed Vault service intent: {error}"),
    })?;
    Ok(blake3::hash(&canonical).to_hex().to_string())
}

fn managed_host(instance: &str) -> String {
    format!("{instance}.vault.mm.test")
}

fn service_request(
    request: &ManagedVaultRequest,
    config_directory: &Path,
    generation: String,
    runtime: crate::domain::runtime::RuntimeKind,
) -> ManagedServicePlanRequest {
    let prefix = format!("{}-vault", request.connection);
    ManagedServicePlanRequest {
        target: request.target.clone(),
        runtime,
        provider_instance: request.connection.clone(),
        component: "vault".to_owned(),
        image: request.image.clone(),
        host_port: request.host_port,
        container_port: 8200,
        user: "100:1000".to_owned(),
        entrypoint: "vault".to_owned(),
        command: vec![
            "server".to_owned(),
            "-config=/vault/config/config.hcl".to_owned(),
        ],
        healthcheck: vec![
            "CMD".to_owned(),
            "vault".to_owned(),
            "status".to_owned(),
            "-address=https://127.0.0.1:8200".to_owned(),
            "-ca-cert=/vault/config/ca.pem".to_owned(),
        ],
        config_mount: "/vault/config".to_owned(),
        data_mount: "/vault/file".to_owned(),
        config_files: [
            ("config.hcl", 0o600),
            ("ca.pem", 0o644),
            ("server.pem", 0o644),
            ("server-key.pem", 0o600),
        ]
        .into_iter()
        .map(|(name, mode)| ManagedServiceFile {
            source: config_directory.join(name),
            name: name.to_owned(),
            mode,
            uid: 100,
            gid: 1000,
        })
        .collect(),
        ingress_files: [
            (
                "vault.dynamic.yaml",
                format!("{prefix}.dynamic.yaml"),
                0o600,
            ),
            ("ca.pem", format!("{prefix}-ca.crt"), 0o644),
            ("server.pem", format!("{prefix}.crt"), 0o644),
            ("server-key.pem", format!("{prefix}.key"), 0o600),
        ]
        .into_iter()
        .map(|(source, name, mode)| ManagedServiceFile {
            source: config_directory.join(source),
            name,
            mode,
            uid: 0,
            gid: 0,
        })
        .collect(),
        generation,
    }
}

fn prepare_ingress(
    directory: &Path,
    service: &ManagedServicePlan,
    ingress_host: &str,
) -> Result<(), MmError> {
    let names = &service.service.ingress_files;
    if names.len() != 4 {
        return Err(MmError::Security {
            message: "managed Vault ingress file intent is incomplete".to_owned(),
        });
    }
    let route = format!(
        "http:\n  routers:\n    mm-{instance}-vault-secure:\n      rule: Host(`{ingress_host}`)\n      entryPoints: [websecure]\n      service: mm-{instance}-vault\n      middlewares: [mm-{instance}-vault-local-only]\n      tls: {{}}\n  middlewares:\n    mm-{instance}-vault-local-only:\n      ipAllowList:\n        sourceRange: [\"127.0.0.0/8\", \"::1/128\"]\n  services:\n    mm-{instance}-vault:\n      loadBalancer:\n        serversTransport: mm-{instance}-vault-transport\n        servers:\n          - url: https://{container}:8200\n  serversTransports:\n    mm-{instance}-vault-transport:\n      serverName: 127.0.0.1\n      rootCAs: [/etc/traefik/dynamic/{ca}]\ntls:\n  certificates:\n    - certFile: /etc/traefik/dynamic/{certificate}\n      keyFile: /etc/traefik/dynamic/{key}\n",
        instance = service.identity.provider_instance,
        container = service.identity.service_name,
        ca = names[1].name,
        certificate = names[2].name,
        key = names[3].name,
    );
    write_private(&directory.join("vault.dynamic.yaml"), route.as_bytes())
}

fn prepare_configuration(
    directory: &Path,
    host_port: u16,
    installation: &str,
    ingress_host: &str,
    service_name: &str,
) -> Result<String, MmError> {
    prepare_directory(directory)?;
    let ca_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(tls_error)?;
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).map_err(tls_error)?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "mm managed Vault CA");
    let ca = ca_params.self_signed(&ca_key).map_err(tls_error)?;
    let issuer = rcgen::Issuer::from_ca_cert_pem(&ca.pem(), ca_key).map_err(tls_error)?;
    let server_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(tls_error)?;
    let mut params = CertificateParams::new(vec![
        "127.0.0.1".to_owned(),
        "localhost".to_owned(),
        ingress_host.to_owned(),
        service_name.to_owned(),
    ])
    .map_err(tls_error)?;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, ingress_host);
    let server = params.signed_by(&server_key, &issuer).map_err(tls_error)?;
    let ca_pem = ca.pem();
    write_private(&directory.join("ca.pem"), ca_pem.as_bytes())?;
    write_private(&directory.join("server.pem"), server.pem().as_bytes())?;
    let mut key = Zeroizing::new(server_key.serialize_pem());
    write_private(&directory.join("server-key.pem"), key.as_bytes())?;
    key.zeroize();
    let config = format!(
        "ui = true\ndisable_mlock = true\napi_addr = \"https://127.0.0.1:{host_port}\"\ncluster_addr = \"https://127.0.0.1:8201\"\nstorage \"raft\" {{\n  path = \"/vault/file\"\n  node_id = \"{installation}\"\n}}\nlistener \"tcp\" {{\n  address = \"0.0.0.0:8200\"\n  cluster_address = \"0.0.0.0:8201\"\n  tls_cert_file = \"/vault/config/server.pem\"\n  tls_key_file = \"/vault/config/server-key.pem\"\n  tls_min_version = \"tls13\"\n}}\n"
    );
    write_private(&directory.join("config.hcl"), config.as_bytes())?;
    Ok(ca_pem)
}

fn tls_error(error: impl std::fmt::Display) -> MmError {
    MmError::Security {
        message: format!("could not generate managed Vault TLS identity: {error}"),
    }
}
fn supported_image(image: &str) -> bool {
    matches!(image, VAULT_1_18_IMAGE | VAULT_1_19_IMAGE)
}

fn with_cleanup_failures(
    primary: MmError,
    cleanup: Vec<String>,
    operation_id: &str,
    restored: bool,
) -> MmError {
    if cleanup.is_empty() {
        primary
    } else {
        MmError::operation_failure(
            operation_id,
            if restored {
                crate::domain::error::MutationStatus::NotStarted
            } else {
                crate::domain::error::MutationStatus::Unknown
            },
            primary,
            cleanup,
            "inspect the managed Vault journal, encrypted recovery material, and pinned runtime service before retrying",
        )
    }
}
fn prepare_directory(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|source| MmError::Io {
                context: format!("creating managed Vault directory {}", path.display()),
                source,
            })?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!("securing managed Vault directory {}", path.display()),
                source,
            }
        })?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating managed Vault directory {}", path.display()),
        source,
    })?;
    ensure_user_controlled_directory(path)
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    let parent = path.parent().ok_or_else(|| MmError::State {
        message: "managed Vault path has no parent".to_owned(),
    })?;
    prepare_directory(parent)?;
    ensure_no_symlink_components(path)?;
    let mut options = AtomicWriteFile::options();
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .preserve_mode(false)
            .preserve_owner(false);
    }
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("opening private managed Vault file {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing private managed Vault file {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing private managed Vault file {}", path.display()),
        source,
    })?;
    ensure_private_file(path)
}

#[derive(Deserialize)]
struct InitializeResponse {
    keys_base64: Vec<Zeroizing<String>>,
    root_token: Zeroizing<String>,
}

#[cfg(test)]
#[path = "managed/tests.rs"]
mod tests;
