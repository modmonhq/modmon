//! `mm target storage status`: what every node says about the cluster's
//! shared volumes, and what is wrong with it in words that say what to do.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{NFS_PORT, VolumeName, volume_of_directory};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::workspace::ProjectId;

/// How long a node waits for the storage node's port before it calls it
/// unreachable.
pub const STORAGE_REACH_TIMEOUT_MS: u64 = 3_000;

/// A shared volume's directory on the storage node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StoredVolume {
    pub stack: String,
    pub volume: String,
    pub path: String,
    /// Disk space the files take; absent when the request asked for no
    /// sizes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<u64>,
    /// `false` when counting stopped at its time limit: the volume holds
    /// at least `bytes` in at least `files` files.
    #[serde(default = "counted_in_full", skip_serializing_if = "is_true")]
    pub counted: bool,
}

const fn counted_in_full() -> bool {
    true
}

const fn is_true(value: &bool) -> bool {
    *value
}

/// What one node's `mmd` says about shared storage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StorageNodeStatus {
    /// The node's own name from `/etc/mm/node.env`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Whether this node is the storage node the request named.
    pub serves: bool,
    /// On the storage node: whether setup exported the storage root here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported: Option<bool>,
    /// The storage node's mesh address as this node's mesh has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<Ipv4Addr>,
    /// Whether the storage node's storage port answered this node.
    pub reachable: bool,
    /// How long the answer took, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_ms: Option<u64>,
    /// Why the storage node did not answer, or why this node cannot tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The environment's shared volumes this node has mounted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounted: Vec<VolumeName>,
    /// On the storage node: every volume directory of the environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<StoredVolume>,
    /// On the storage node: free and total space of the disk that holds
    /// the volumes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
}

/// The shared volumes of `project` in `environment` that a host has
/// mounted, read from the text of `/proc/self/mountinfo`: a volume mounted
/// over the mesh has its directory as mount source, one used on the storage
/// node as the root of a bind mount.
#[must_use]
pub fn mounted_volumes(mountinfo: &str, project: ProjectId, environment: &str) -> Vec<VolumeName> {
    let mut mounted = Vec::new();
    for line in mountinfo.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let mut after = after.split(' ');
        let (kind, source) = (
            after.next().unwrap_or_default(),
            after.next().unwrap_or_default(),
        );
        let root = before.split(' ').nth(3).unwrap_or_default();
        let directory = if kind.starts_with("nfs") {
            source
        } else {
            root
        };
        if let Some(volume) = volume_of_directory(project, environment, directory)
            && !mounted.contains(&volume)
        {
            mounted.push(volume);
        }
    }
    mounted.sort();
    mounted
}

/// One node in `mm target storage status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StorageNodeReport {
    pub name: String,
    /// Whether the node's `mmd` answered.
    pub answered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StorageNodeStatus>,
}

impl StorageNodeReport {
    /// The node's shared storage in a few words for a node list, such as
    /// `keeps shared volumes` or `2 shared volumes mounted`; `None` when
    /// there is nothing to say: the node did not answer (its line says so
    /// already), or the target shares no volume and nothing is wrong.
    #[must_use]
    pub fn summary(&self, shares: bool) -> Option<String> {
        let status = self.status.as_ref()?;
        match (status.serves, status.reachable) {
            (true, false) if status.exported == Some(false) => {
                shares.then(|| "serves no shared storage yet".to_owned())
            }
            (true, false) => Some("storage server not answering".to_owned()),
            (true, true) => shares.then(|| "keeps shared volumes".to_owned()),
            (false, false) => shares.then(|| "cannot reach shared storage".to_owned()),
            (false, true) => match status.mounted.len() {
                0 => None,
                1 => Some("1 shared volume mounted".to_owned()),
                count => Some(format!("{count} shared volumes mounted")),
            },
        }
    }
}

/// One shared volume in `mm target storage status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VolumeReport {
    pub stack: String,
    pub volume: String,
    /// The volume's directory on the storage node; absent while the storage
    /// node has not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether the checkout's configuration lists the volume under
    /// `shared-volumes`.
    pub declared: bool,
    /// Whether the directory exists on the storage node.
    pub stored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<u64>,
    #[serde(default = "counted_in_full", skip_serializing_if = "is_true")]
    pub counted: bool,
    /// The nodes that have it mounted, the storage node included when a
    /// service there uses the directory.
    pub mounted_on: Vec<String>,
}

/// `mm target storage status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StorageReport {
    pub target: String,
    pub environment: String,
    /// The node that keeps shared volumes: `cluster.storage`, or the
    /// target's first node.
    pub storage_node: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<Ipv4Addr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    pub volumes: Vec<VolumeReport>,
    pub nodes: Vec<StorageNodeReport>,
    pub problems: Vec<Diagnostic>,
}

impl StorageReport {
    /// Puts the nodes' answers together. `declared` are the shared volumes
    /// the checkout's configuration lists for the environment.
    #[must_use]
    pub fn new(
        (target, environment, storage_node): (String, String, String),
        declared: &[VolumeName],
        nodes: Vec<StorageNodeReport>,
    ) -> Self {
        let serving = nodes
            .iter()
            .find(|node| node.name == storage_node)
            .and_then(|node| node.status.as_ref());
        let mut volumes = BTreeMap::<VolumeName, VolumeReport>::new();
        let blank = |name: &VolumeName, declared: bool| VolumeReport {
            stack: name.stack.clone(),
            volume: name.volume.clone(),
            path: None,
            declared,
            stored: false,
            bytes: None,
            files: None,
            counted: true,
            mounted_on: Vec::new(),
        };
        for name in declared {
            volumes.insert(name.clone(), blank(name, true));
        }
        for stored in serving.into_iter().flat_map(|status| &status.volumes) {
            let name = VolumeName::new(&stored.stack, &stored.volume);
            let volume = volumes
                .entry(name.clone())
                .or_insert_with(|| blank(&name, false));
            volume.path = Some(stored.path.clone());
            volume.stored = true;
            volume.bytes = stored.bytes;
            volume.files = stored.files;
            volume.counted = stored.counted;
        }
        for node in &nodes {
            for name in node.status.iter().flat_map(|status| &status.mounted) {
                volumes
                    .entry(name.clone())
                    .or_insert_with(|| blank(name, false))
                    .mounted_on
                    .push(node.name.clone());
            }
        }
        let mut report = Self {
            target,
            environment,
            address: serving.and_then(|status| status.address),
            free_bytes: serving.and_then(|status| status.free_bytes),
            total_bytes: serving.and_then(|status| status.total_bytes),
            storage_node,
            volumes: volumes.into_values().collect(),
            nodes,
            problems: Vec::new(),
        };
        report.problems = storage_diagnostics(&report);
        report
    }

    /// Whether every node answered and reaches the storage node.
    #[must_use]
    pub fn healthy(&self) -> bool {
        !self
            .problems
            .iter()
            .any(|problem| problem.severity == crate::domain::diagnostic::Severity::Error)
    }

    /// The volumes that have files on the storage node and are not in the
    /// configuration any more.
    pub fn undeclared(&self) -> impl Iterator<Item = &VolumeReport> {
        self.volumes
            .iter()
            .filter(|volume| volume.stored && !volume.declared)
    }
}

/// What is wrong with shared storage, each with what to do about it.
#[must_use]
pub fn storage_diagnostics(report: &StorageReport) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    let target = &report.target;
    let storage = &report.storage_node;
    let uses_storage = !report.volumes.is_empty();
    for node in &report.nodes {
        let Some(status) = &node.status else {
            let consequence = if node.name == *storage {
                format!(
                    "node '{}' keeps the shared volumes and its mmd does not answer",
                    node.name
                )
            } else {
                format!("node '{}' does not answer", node.name)
            };
            problems.push(
                Diagnostic::error(
                    "MM_STORAGE_NODE_SILENT",
                    format!(
                        "{consequence}: {}",
                        node.problem.as_deref().unwrap_or("no reason given")
                    ),
                )
                .with_help(format!(
                    "`mm target nodes` shows every node of '{target}'; check the host and `systemctl status mmd` on it"
                )),
            );
            continue;
        };
        if status.serves && status.exported == Some(false) {
            problems.push(
                Diagnostic::error(
                    "MM_STORAGE_NOT_SERVED",
                    format!(
                        "node '{}' is the storage node of '{target}' and does not export shared storage",
                        node.name
                    ),
                )
                .with_help(
                    "run `mm target setup plan` and apply it: setup installs the storage server on the storage node",
                ),
            );
            continue;
        }
        if status.reachable {
            continue;
        }
        let reason = status.problem.as_deref().unwrap_or("it did not answer");
        let message = if status.serves {
            format!(
                "the storage server on node '{}' does not answer on its own address: {reason}",
                node.name
            )
        } else {
            format!(
                "node '{}' cannot reach shared storage on {storage}: {reason}",
                node.name
            )
        };
        let help = if status.serves {
            format!(
                "on {storage}, `systemctl status nfs-server` says why; `mm target setup plan` and its apply set the server up again"
            )
        } else {
            format!(
                "check that {storage} is up and `mm target mesh status` shows a handshake between the two nodes; the storage server listens on TCP port {NFS_PORT} of {storage}'s mesh address"
            )
        };
        // A cluster that shares no volume loses nothing while it cannot.
        let diagnostic = if uses_storage {
            Diagnostic::error("MM_STORAGE_UNREACHABLE", message)
        } else {
            Diagnostic::warning("MM_STORAGE_UNREACHABLE", message)
        };
        problems.push(diagnostic.with_help(help));
    }
    problems
}

/// What `mm target storage delete` would remove, as the storage node sees
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VolumeDeletePlan {
    /// The storage node.
    pub node: String,
    pub stack: String,
    pub volume: String,
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    /// `false` when counting stopped at its time limit.
    pub counted: bool,
    /// Whether the checkout's configuration still lists the volume: the
    /// next deployment then creates it again, empty.
    #[serde(default)]
    pub declared: bool,
    /// The hash `mm target storage delete --plan` takes.
    pub plan_hash: String,
}

impl VolumeDeletePlan {
    /// The hash that names deleting this volume on this storage node.
    #[must_use]
    pub fn hash_of(project: ProjectId, environment: &str, node: &str, path: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        for value in [
            "mm-shared-volume-delete-v1",
            &project.0.to_string(),
            environment,
            node,
            path,
        ] {
            hasher.update(&(value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// What deleting a shared volume removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VolumeDeleted {
    pub node: String,
    pub stack: String,
    pub volume: String,
    pub path: String,
}
