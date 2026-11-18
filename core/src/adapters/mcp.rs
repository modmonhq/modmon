use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::agents::{AgentsConfig, agents_policy_hash};
use crate::domain::error::MmError;
use crate::domain::mcp::McpServerConfig;
use crate::domain::workspace::ProjectId;

const MAX_CONFIG_BYTES: usize = 1024 * 1024;

pub struct McpPolicyStore;

impl McpPolicyStore {
    /// The private limits in `path`. Without the file an agent is limited
    /// only by each project's `agents:` block; `required` is for a path the
    /// user named, which must exist.
    pub fn load(path: &Path, required: bool) -> Result<McpServerConfig, MmError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => {
                return Ok(McpServerConfig::default());
            }
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("inspecting MCP policy {}", path.display()),
                    source,
                });
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(security("MCP policy must be a regular non-symlink file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.uid() != rustix::process::getuid().as_raw()
                || metadata.permissions().mode() & 0o077 != 0
            {
                return Err(security(
                    "MCP policy must be owned by the current user and mode 0600",
                ));
            }
        }
        let bytes = fs::read(path).map_err(|source| MmError::Io {
            context: format!("reading MCP policy {}", path.display()),
            source,
        })?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(security("MCP policy exceeds 1 MiB"));
        }
        let version = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("schema_version")
                    .and_then(serde_json::Value::as_u64)
            });
        if version == Some(1) {
            return Err(MmError::Security {
                message: format!(
                    "{} uses MCP policy schema 1, which listed environments and capabilities privately. They now come from the project's `agents:` block: delete the file, or keep only schema_version 2 with `principal`, `ceiling`, and the bounds",
                    path.display()
                ),
            });
        }
        let config: McpServerConfig =
            serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        config
            .validate()
            .map_err(|message| MmError::Security { message })?;
        Ok(config)
    }

    #[must_use]
    pub fn default_path(mm_home: &Path) -> PathBuf {
        mm_home.join("mcp.json")
    }
}

/// The `agents:` block a user last accepted for one checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedAgentsPolicy {
    pub project_id: ProjectId,
    pub policy_hash: String,
    pub trusted_at_unix_ms: i64,
}

/// Which `agents:` blocks the user trusts, by project root
/// (`MM_HOME/agents-trust.json`). An agent that edits the block in its
/// checkout changes the hash, and gets `view` until the user trusts it again
/// with `mm agent trust`.
#[derive(Debug, Clone)]
pub struct AgentTrustStore {
    path: PathBuf,
}

impl AgentTrustStore {
    #[must_use]
    pub fn new(mm_home: &Path) -> Self {
        Self {
            path: mm_home.join("agents-trust.json"),
        }
    }

    fn load(&self) -> Result<BTreeMap<String, TrustedAgentsPolicy>, MmError> {
        match fs::read(&self.path) {
            Ok(bytes) if bytes.len() > MAX_CONFIG_BYTES => {
                Err(security("agents trust file exceeds 1 MiB"))
            }
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
                path: self.path.clone(),
                message: error.to_string(),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(source) => Err(MmError::Io {
                context: format!("reading {}", self.path.display()),
                source,
            }),
        }
    }

    /// Whether the user trusted exactly this block for this checkout. Any
    /// doubt, including an unreadable trust file, is "no".
    #[must_use]
    pub fn is_trusted(&self, root: &Path, project_id: ProjectId, agents: &AgentsConfig) -> bool {
        self.load().is_ok_and(|trusted| {
            trusted.get(&root_key(root)).is_some_and(|entry| {
                entry.project_id == project_id && entry.policy_hash == agents_policy_hash(agents)
            })
        })
    }

    /// Records the block as trusted for this checkout.
    pub fn trust(
        &self,
        root: &Path,
        project_id: ProjectId,
        agents: &AgentsConfig,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        let mut trusted = self.load()?;
        trusted.insert(
            root_key(root),
            TrustedAgentsPolicy {
                project_id,
                policy_hash: agents_policy_hash(agents),
                trusted_at_unix_ms: now_unix_ms,
            },
        );
        let mut text = serde_json::to_string_pretty(&trusted).map_err(|error| MmError::State {
            message: format!("serializing agents trust: {error}"),
        })?;
        text.push('\n');
        let parent = self.path.parent().ok_or_else(|| MmError::State {
            message: "agents trust file has no directory".to_owned(),
        })?;
        fs::create_dir_all(parent).map_err(|source| MmError::Io {
            context: format!("creating {}", parent.display()),
            source,
        })?;
        let temporary = parent.join(format!(".agents-trust.{}", uuid::Uuid::new_v4()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let written = options
            .open(&temporary)
            .and_then(|mut file| {
                std::io::Write::write_all(&mut file, text.as_bytes())?;
                file.sync_all()
            })
            .and_then(|()| fs::rename(&temporary, &self.path));
        written.map_err(|source| {
            if let Err(cleanup) = fs::remove_file(&temporary)
                && cleanup.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%cleanup, "could not remove a temporary agents trust file");
            }
            MmError::Io {
                context: format!("writing {}", self.path.display()),
                source,
            }
        })
    }
}

fn root_key(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

fn security(message: &str) -> MmError {
    MmError::Security {
        message: message.to_owned(),
    }
}

#[cfg(test)]
#[path = "mcp/tests.rs"]
mod tests;
