use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::error::MmError;
use crate::domain::stored_state::{StoredOn, check_schema, stored_version, unreadable};

pub const DAEMON_CONFIG_SCHEMA: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub schema_version: u32,
    pub socket_path: PathBuf,
    pub state_root: PathBuf,
    pub mm_home: PathBuf,
    pub socket_uid: u32,
    pub socket_gid: u32,
    #[serde(default = "default_idle_timeout_ms")]
    pub idle_timeout_ms: u64,
    /// Root-owned host policy (`/etc/mm/host.yaml`).
    pub host_policy_path: PathBuf,
    /// The only account whose socket connections the daemon accepts besides
    /// root: the shared `mm` relay account.
    pub relay_uid: u32,
    /// Rendered from the effective policy on start and after every change.
    #[serde(default = "default_authorized_keys_path")]
    pub authorized_keys_path: PathBuf,
    /// The command each `authorized_keys` line forces.
    #[serde(default = "default_relay_command")]
    pub relay_command: String,
    /// Immutable per-commit checkouts, `<projects_root>/<id>/<env>/<sha12>`.
    #[serde(default = "default_projects_root")]
    pub projects_root: PathBuf,
    /// Written by `mm target setup` on a cluster node: its name and its
    /// address on the private network.
    #[serde(default = "default_node_env_path")]
    pub node_env_path: PathBuf,
}

/// What this host is as a cluster node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeEnv {
    pub name: String,
    pub private_address: Option<String>,
    /// The container engine setup installed (`MM_RUNTIME`); `None` on a
    /// node set up before mm recorded it, which runs Docker Engine.
    pub runtime: Option<crate::domain::runtime::RuntimeKind>,
}

impl DaemonConfig {
    pub fn load(path: &Path) -> Result<Self, MmError> {
        ensure_regular_not_symlink(path)?;
        let bytes = fs::read(path).map_err(|source| MmError::Io {
            context: format!("reading daemon configuration {}", path.display()),
            source,
        })?;
        if bytes.len() > 1024 * 1024 {
            return Err(security("daemon configuration exceeds 1 MiB"));
        }
        Self::parse(path, &bytes)
    }

    /// The configuration in `bytes`, read from `path`.
    pub fn parse(path: &Path, bytes: &[u8]) -> Result<Self, MmError> {
        if let Some(schema) = stored_version(bytes, "schema_version") {
            check_schema(path, schema, DAEMON_CONFIG_SCHEMA, StoredOn::Host)?;
        }
        let config: Self = serde_json::from_slice(bytes)
            .map_err(|error| unreadable(path, "mmd configuration", &error, StoredOn::Host))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), MmError> {
        if self.schema_version != DAEMON_CONFIG_SCHEMA {
            return Err(security(
                "unsupported mmd configuration schema; expected 2 (host_policy_path and relay_uid replace access_policy_path)",
            ));
        }
        if !self.socket_path.is_absolute()
            || !self.state_root.is_absolute()
            || !self.mm_home.is_absolute()
            || !self.host_policy_path.is_absolute()
            || !self.authorized_keys_path.is_absolute()
            || !self.projects_root.is_absolute()
            || !self.node_env_path.is_absolute()
            || self.relay_command.trim().is_empty()
            || self.relay_command.contains(['"', '\n', '\r'])
            || self.socket_path.parent().is_none()
            || self.idle_timeout_ms == 0
            || self.idle_timeout_ms > 24 * 60 * 60 * 1_000
        {
            return Err(security(
                "daemon paths, timeout, relay command, or host policy path is invalid",
            ));
        }
        Ok(())
    }

    pub fn host_policy_path(&self) -> &Path {
        &self.host_policy_path
    }

    /// This host's node name and private address, or `None` on a host that
    /// was not set up as a cluster node. Read on every request, so a setup
    /// that names the node takes effect without restarting the daemon.
    pub fn node_env(&self) -> Result<Option<NodeEnv>, MmError> {
        let text = match fs::read_to_string(&self.node_env_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading {}", self.node_env_path.display()),
                    source,
                });
            }
        };
        let value = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let Some(name) = value("MM_NODE_NAME") else {
            return Ok(None);
        };
        crate::domain::target::validate_target_name(&name).map_err(|_| {
            security("the node name in the node environment file is not a valid node name")
        })?;
        let runtime = value("MM_RUNTIME")
            .map(|runtime| {
                runtime.parse().map_err(|_| {
                    security("the runtime in the node environment file is not docker-compose or podman-compose")
                })
            })
            .transpose()?;
        Ok(Some(NodeEnv {
            name,
            private_address: value("MM_PRIVATE_ADDRESS"),
            runtime,
        }))
    }

    /// The container engine this host runs, for what `mmd` does on its own
    /// host outside a request (names, shared storage, metrics, the
    /// firewall chains): the one setup recorded, else Docker Engine, which
    /// every host set up before mm recorded it runs.
    #[must_use]
    pub fn host_runtime(&self) -> crate::domain::runtime::RuntimeKind {
        match self.node_env() {
            Ok(Some(NodeEnv {
                runtime: Some(runtime),
                ..
            })) => runtime,
            _ => crate::domain::runtime::RuntimeKind::DockerCompose,
        }
    }
}

fn default_idle_timeout_ms() -> u64 {
    30_000
}

fn default_authorized_keys_path() -> PathBuf {
    PathBuf::from("/etc/ssh/mm_authorized_keys")
}

fn default_relay_command() -> String {
    "/usr/bin/mm relay serve --stdio --socket /run/mm/mm.sock --daemon-uid 0".to_owned()
}

fn default_projects_root() -> PathBuf {
    PathBuf::from("/srv/mm/projects")
}

fn default_node_env_path() -> PathBuf {
    PathBuf::from("/etc/mm/node.env")
}

fn ensure_regular_not_symlink(path: &Path) -> Result<(), MmError> {
    let parent = path
        .parent()
        .ok_or_else(|| security("daemon configuration has no parent directory"))?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(|source| MmError::Io {
        context: format!(
            "inspecting daemon configuration directory {}",
            parent.display()
        ),
        source,
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting daemon configuration {}", path.display()),
        source,
    })?;
    if !parent_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.file_type().is_symlink()
    {
        return Err(security(
            "daemon configuration and its parent must be regular non-symlink objects",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if parent_metadata.uid() != 0
            || parent_metadata.permissions().mode() & 0o022 != 0
            || metadata.uid() != 0
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err(security(
                "daemon configuration and its parent must be root-owned and not group/other writable",
            ));
        }
    }
    Ok(())
}

fn security(message: &str) -> MmError {
    MmError::Security {
        message: message.to_owned(),
    }
}

#[cfg(all(test, unix))]
#[path = "config/tests.rs"]
mod tests;
