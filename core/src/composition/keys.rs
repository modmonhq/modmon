//! This device's SSH keys, and the connections made with them.

use super::*;
use crate::adapters::keys::{
    FileKeyVault, ManagedKeys, NoKeychain, OpenSshKeyFiles, system_keychain,
};
use crate::app::{KeyService, KeyServiceDependencies, VaultPreference};
use crate::domain::key::{KeptConnection, ManagedKeyKind};

/// Set to `keychain` or `file` to say where the passphrase of a new or
/// newly protected key goes; unset, mm uses the system keychain when one
/// answers and a file beside the key otherwise. With `file`, no keychain
/// is ever asked.
pub const KEY_VAULT_ENVIRONMENT: &str = "MM_KEY_VAULT";

/// Closes every connection mm keeps open to remote hosts. Called when a
/// key or the access behind one changes, so nothing authenticated before
/// the change is used after it.
pub(crate) fn close_kept_connections(home: &MmHome) {
    ConnectionReuse::close_all_under(&home.run_directory);
}

/// The connections this device keeps open to remote hosts.
pub(crate) fn kept_connections(
    explicit_home: Option<&Path>,
) -> Result<Vec<KeptConnection>, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    Ok(ConnectionReuse::list_under(&home.run_directory))
}

/// Closes them, and says which ones they were.
pub(crate) fn close_all_kept_connections(
    explicit_home: Option<&Path>,
) -> Result<Vec<KeptConnection>, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let kept = ConnectionReuse::list_under(&home.run_directory);
    close_kept_connections(&home);
    Ok(kept)
}

struct HomeConnections {
    run_directory: PathBuf,
}

impl crate::app::ports::KeptConnections for HomeConnections {
    fn close_all(&self) {
        ConnectionReuse::close_all_under(&self.run_directory);
    }
}

fn keys_directory(home: &MmHome) -> PathBuf {
    home.identities_directory.join("ssh")
}

/// This device's mm-managed SSH key: `MM_HOME/identities/ssh/id_ed25519`.
pub(crate) fn device_key_path(home: &MmHome) -> PathBuf {
    keys_directory(home).join(ManagedKeyKind::Device.file_name())
}

/// This device's mm-managed SSH key.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceKey {
    pub path: PathBuf,
    /// `<type> <base64> <comment>`, ready for the users file.
    pub public_key: String,
    pub fingerprint: String,
}

/// The key `mm mcp serve` reaches remote environments with:
/// `MM_HOME/identities/ssh/agent_ed25519`. The users file lists it with
/// `agent: true`.
pub(crate) fn agent_key_path(home: &MmHome) -> PathBuf {
    keys_directory(home).join(ManagedKeyKind::Agent.file_name())
}

/// The public half of this device's key, read without asking a vault.
pub(crate) fn device_key(explicit_home: Option<&Path>) -> Result<Option<DeviceKey>, MmError> {
    read_key(device_key_path(&MmHome::resolve(explicit_home)?))
}

pub(crate) fn agent_key(explicit_home: Option<&Path>) -> Result<Option<DeviceKey>, MmError> {
    read_key(agent_key_path(&MmHome::resolve(explicit_home)?))
}

fn read_key(path: PathBuf) -> Result<Option<DeviceKey>, MmError> {
    let public = path.with_extension("pub");
    let line = match std::fs::read_to_string(&public) {
        Ok(line) => line,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("reading {}", public.display()),
                source,
            });
        }
    };
    let parsed = crate::domain::host_policy::parse_public_key(line.trim())
        .map_err(|message| MmError::Security { message })?;
    Ok(Some(DeviceKey {
        path,
        public_key: line.trim().to_owned(),
        fingerprint: parsed.fingerprint,
    }))
}

/// Creating, protecting, replacing and removing this device's keys, for
/// the CLI and the terminal UI alike.
pub(crate) fn key_service(explicit_home: Option<&Path>) -> Result<Arc<KeyService>, MmError> {
    key_service_at(&MmHome::resolve(explicit_home)?)
}

pub(crate) fn key_service_at(home: &MmHome) -> Result<Arc<KeyService>, MmError> {
    let preference = VaultPreference::parse(std::env::var(KEY_VAULT_ENVIRONMENT).ok().as_deref())
        .map_err(|message| MmError::Environment { message })?;
    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_owned());
    let host = std::fs::read_to_string("/etc/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .map(|host| host.trim().to_owned())
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "device".to_owned());
    Ok(Arc::new(KeyService::new(KeyServiceDependencies {
        files: Arc::new(OpenSshKeyFiles::new(home.run_directory.clone())),
        keychain: if preference == VaultPreference::File {
            Arc::new(NoKeychain)
        } else {
            system_keychain()
        },
        file_vault: Arc::new(FileKeyVault),
        connections: Arc::new(HomeConnections {
            run_directory: home.run_directory.clone(),
        }),
        directory: keys_directory(home),
        preference,
        owner: format!("{user}@{host}"),
    })))
}

/// What a connection needs to authenticate with one of this device's keys:
/// the device key for the user's commands, the agent key for the MCP
/// server. It opens that key and no other.
pub(crate) fn managed_keys(home: &MmHome, kind: ManagedKeyKind) -> Result<ManagedKeys, MmError> {
    Ok(ManagedKeys::new(
        key_service_at(home)?.unlock(kind),
        home.run_directory.clone(),
    ))
}

/// Creates this device's key, as the terminal UI does when a user joins a
/// team from it.
pub(crate) fn create_device_key(
    explicit_home: Option<&Path>,
    security_key: bool,
) -> Result<crate::app::KeyOutcome, MmError> {
    key_service(explicit_home)?.create(ManagedKeyKind::Device, security_key)
}
