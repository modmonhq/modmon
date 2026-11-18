//! What a node keeps on disk for its restarts of unhealthy containers: what
//! each environment's last finished deployment runs here, and what was done
//! to each container.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::adapters::node_files::{directory, names, read_whole, remove, write_whole};
use crate::domain::error::MmError;
use crate::domain::healing::{RestartEvent, RestartPolicy};
use crate::domain::remote::RemoteScope;

const DEPLOYED: &str = "deployed";
const HISTORY: &str = "history";

/// One environment's services on this node, as its last finished
/// deployment runs them, and where that deployment is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct WatchedEnvironment {
    pub scope: RemoteScope,
    /// The checkout of the deployed commit.
    pub root: PathBuf,
    pub services: Vec<WatchedService>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct WatchedService {
    pub stack: String,
    pub service: String,
    /// `None` for `on-failure: none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<RestartPolicy>,
}

/// What was done to one container, by the engine's whole id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct History {
    #[serde(default)]
    pub containers: BTreeMap<String, ContainerHistory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ContainerHistory {
    pub stack: String,
    pub service: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restarts: Vec<RestartEvent>,
    /// Set once the node stopped restarting it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<RestartEvent>,
}

fn file_name(scope: &RemoteScope) -> String {
    format!(
        "{}-{}.json",
        scope.project_id.0,
        crate::adapters::autoscale::store::safe(&scope.environment)
    )
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, MmError> {
    let Some(bytes) = read_whole(path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| MmError::State {
            message: format!("{} cannot be read: {error}", path.display()),
        })
}

fn write<T: Serialize>(folder: &Path, path: &Path, value: &T) -> Result<(), MmError> {
    directory(folder, 0o700)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| MmError::State {
        message: format!("cannot write {}: {error}", path.display()),
    })?;
    write_whole(path, &bytes, 0o600)
}

/// Keeps what a finished deployment runs here; one that runs nothing here
/// removes the environment's record.
pub fn keep_watched(root: &Path, watched: &WatchedEnvironment) -> Result<(), MmError> {
    let folder = root.join(DEPLOYED);
    let path = folder.join(file_name(&watched.scope));
    if watched.services.is_empty() {
        remove(&path)?;
        return Ok(());
    }
    write(&folder, &path, watched)
}

/// Every environment's record. One that cannot be read is skipped with a
/// warning.
pub fn watched(root: &Path) -> Result<Vec<WatchedEnvironment>, MmError> {
    let folder = root.join(DEPLOYED);
    let mut records = Vec::new();
    for name in names(&folder, ".json")? {
        match read::<WatchedEnvironment>(&folder.join(&name)) {
            Ok(Some(record)) => records.push(record),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "a record of watched services cannot be read"),
        }
    }
    Ok(records)
}

pub fn history(root: &Path, scope: &RemoteScope) -> Result<History, MmError> {
    Ok(read(&root.join(HISTORY).join(file_name(scope)))?.unwrap_or_default())
}

pub fn keep_history(root: &Path, scope: &RemoteScope, history: &History) -> Result<(), MmError> {
    let folder = root.join(HISTORY);
    let path = folder.join(file_name(scope));
    if history.containers.is_empty() {
        remove(&path)?;
        return Ok(());
    }
    write(&folder, &path, history)
}
