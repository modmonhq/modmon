//! What the autoscaler keeps on disk: the count of each service, and what
//! each finished deployment of an environment scales on this node.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::node_files::{directory, names, read_whole, remove, write_whole};
use crate::domain::autoscale::{ScaledService, Want};
use crate::domain::error::MmError;
use crate::domain::remote::RemoteScope;

const DEPLOYED: &str = "deployed";
const WANTS: &str = "wants";

/// What one environment's last finished deployment scales on this node,
/// and where that deployment is: the autoscaler starts containers as it
/// runs them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DeployedScaling {
    pub scope: RemoteScope,
    /// The checkout of the deployed commit.
    pub root: PathBuf,
    /// The hash of this node's section of the deployment.
    pub plan_hash: String,
    pub services: Vec<ScaledService>,
}

impl DeployedScaling {
    fn file_name(&self) -> String {
        format!(
            "{}-{}.json",
            self.scope.project_id.0,
            safe(&self.scope.environment)
        )
    }
}

/// A name made of what a file name may hold.
pub(crate) fn safe(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => character,
            '/' => '+',
            _ => '_',
        })
        .collect()
}

fn want_path(root: &Path, key: &str) -> PathBuf {
    root.join(WANTS).join(format!("{}.json", safe(key)))
}

/// Keeps what a finished deployment scales; one that scales nothing here
/// removes the environment's record. The count of a service the
/// environment autoscaled here before and does not any more (removed,
/// moved to other nodes, no longer autoscaled, or the environment places
/// nothing here now) is removed with it.
pub fn keep_deployed(root: &Path, deployed: &DeployedScaling) -> Result<(), MmError> {
    let path = root.join(DEPLOYED).join(deployed.file_name());
    let before =
        read_whole(&path)?.and_then(|bytes| serde_json::from_slice::<DeployedScaling>(&bytes).ok());
    for gone in autoscaled(before.as_ref()).difference(&autoscaled(Some(deployed))) {
        remove(&want_path(root, gone))?;
    }
    if deployed
        .services
        .iter()
        .all(|service| service.autoscale.is_none())
    {
        remove(&path)?;
        return Ok(());
    }
    directory(&root.join(DEPLOYED), 0o700)?;
    let bytes = serde_json::to_vec_pretty(deployed).map_err(|error| MmError::State {
        message: format!("cannot write what the deployment scales: {error}"),
    })?;
    write_whole(&path, &bytes, 0o600)
}

/// The keys of the services a record autoscales.
fn autoscaled(record: Option<&DeployedScaling>) -> std::collections::BTreeSet<String> {
    record
        .into_iter()
        .flat_map(|record| &record.services)
        .filter(|service| service.autoscale.is_some())
        .map(ScaledService::key)
        .collect()
}

/// Every environment's record. One that cannot be read is skipped with a
/// warning: the others still scale.
pub fn deployed(root: &Path) -> Result<Vec<DeployedScaling>, MmError> {
    let folder = root.join(DEPLOYED);
    let mut records = Vec::new();
    for name in names(&folder, ".json")? {
        let path = folder.join(&name);
        let Some(bytes) = read_whole(&path)? else {
            continue;
        };
        match serde_json::from_slice::<DeployedScaling>(&bytes) {
            Ok(record) => records.push(record),
            Err(error) => {
                tracing::warn!(file = %path.display(), %error, "a record of scaled services cannot be read");
            }
        }
    }
    Ok(records)
}

/// Removes the count of every service none of `records` autoscales: a
/// count a step wrote while a deployment took its service away. Only when
/// `records` are every record there is: one that could not be read may
/// hold the service.
pub fn sweep_wants(root: &Path, records: &[DeployedScaling]) -> Result<(), MmError> {
    if names(&root.join(DEPLOYED), ".json")?.len() != records.len() {
        return Ok(());
    }
    let kept = records
        .iter()
        .flat_map(|record| autoscaled(Some(record)))
        .map(|key| format!("{}.json", safe(&key)))
        .collect::<std::collections::BTreeSet<_>>();
    for name in names(&root.join(WANTS), ".json")? {
        if !kept.contains(&name) {
            remove(&root.join(WANTS).join(name))?;
        }
    }
    Ok(())
}

/// The count this node last knew of for the service `key`.
pub fn want(root: &Path, key: &str) -> Result<Option<Want>, MmError> {
    let path = want_path(root, key);
    let Some(bytes) = read_whole(&path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| MmError::State {
            message: format!("{} cannot be read: {error}", path.display()),
        })
}

/// Keeps a newer count for the service `key`.
pub fn keep_want(root: &Path, key: &str, want: &Want) -> Result<(), MmError> {
    directory(&root.join(WANTS), 0o700)?;
    let bytes = serde_json::to_vec_pretty(want).map_err(|error| MmError::State {
        message: format!("cannot write the count of {key}: {error}"),
    })?;
    write_whole(&want_path(root, key), &bytes, 0o600)
}
