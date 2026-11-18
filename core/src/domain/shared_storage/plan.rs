//! Shared volumes in a deployment plan: what each node says it uses, and
//! what the cluster plan makes of it.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{STORAGE_ROOT, VolumeName};

/// One shared volume as one node's plan uses it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SharedVolumeUse {
    pub stack: String,
    pub volume: String,
    /// The node that keeps the volume's files.
    pub storage_node: String,
    /// Whether this node is that node and uses the directory itself;
    /// otherwise it mounts the directory over the mesh.
    pub local: bool,
    /// The volume's directory below the storage node's storage root.
    pub directory: String,
    /// The engine volume that stands for it on this node.
    pub engine_volume: String,
    /// The services of this node that mount it.
    pub services: Vec<String>,
}

impl SharedVolumeUse {
    #[must_use]
    pub fn name(&self) -> VolumeName {
        VolumeName::new(&self.stack, &self.volume)
    }
}

/// One shared volume in a cluster plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SharedVolumePlan {
    pub stack: String,
    pub volume: String,
    /// The volume's directory on the storage node.
    pub path: String,
    /// Whether a service on the storage node uses the directory itself.
    pub used_on_storage_node: bool,
    /// The nodes that mount the directory over the mesh, in the plan's
    /// node order.
    pub mounted_on: Vec<String>,
    /// The services that use the volume, each with the nodes it runs on.
    pub services: BTreeMap<String, Vec<String>>,
}

/// The shared volumes of a cluster plan: the node that keeps them, and
/// which nodes mount each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClusterStoragePlan {
    /// The node that keeps the files and serves them over the mesh.
    pub node: String,
    pub volumes: Vec<SharedVolumePlan>,
}

/// What the nodes of a cluster plan keep about its shared volumes: the
/// storage node creates exactly these directories before any node is
/// replaced. Part of the cluster plan's hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SealedStorage {
    pub node: String,
    /// Directories below the storage root, sorted.
    pub directories: Vec<String>,
    /// The nodes that mount a directory over the mesh.
    pub mounting: Vec<String>,
}

impl SealedStorage {
    pub fn validate(&self) -> Result<(), String> {
        crate::domain::target::validate_target_name(&self.node)
            .map_err(|_| "the plan's storage node has an invalid name".to_owned())?;
        for directory in &self.directories {
            let plain = !directory.is_empty()
                && directory.split('/').count() == 4
                && directory.split('/').all(|part| {
                    !part.is_empty()
                        && part != "."
                        && part != ".."
                        && part.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                        })
                });
            if !plain {
                return Err(format!(
                    "the plan names '{directory}', which is not a shared volume's directory"
                ));
            }
        }
        Ok(())
    }
}

impl ClusterStoragePlan {
    /// The shared volumes of a plan over `nodes`: each node's name with
    /// what its own plan uses, in the plan's node order. `None` when no
    /// node uses a shared volume.
    pub fn of(nodes: &[(&str, &[SharedVolumeUse])]) -> Result<Option<Self>, String> {
        let storage_nodes = nodes
            .iter()
            .flat_map(|(_, uses)| uses.iter().map(|used| used.storage_node.as_str()))
            .collect::<BTreeSet<_>>();
        let mut storage_nodes = storage_nodes.into_iter();
        let Some(node) = storage_nodes.next() else {
            return Ok(None);
        };
        if let Some(other) = storage_nodes.next() {
            return Err(format!(
                "the nodes do not agree on the storage node ({node} and {other}); run `mm target setup plan` and apply it, then plan again"
            ));
        }
        let mut volumes = BTreeMap::<VolumeName, SharedVolumePlan>::new();
        for (name, uses) in nodes {
            for used in *uses {
                let path = format!("{STORAGE_ROOT}/{}", used.directory);
                let volume = volumes
                    .entry(used.name())
                    .or_insert_with(|| SharedVolumePlan {
                        stack: used.stack.clone(),
                        volume: used.volume.clone(),
                        path: path.clone(),
                        used_on_storage_node: false,
                        mounted_on: Vec::new(),
                        services: BTreeMap::new(),
                    });
                if volume.path != path {
                    return Err(format!(
                        "the nodes do not agree on the directory of shared volume {} ({} and {path}); plan again",
                        used.name(),
                        volume.path
                    ));
                }
                if used.local {
                    volume.used_on_storage_node = true;
                } else if !volume.mounted_on.iter().any(|mounted| mounted == name) {
                    volume.mounted_on.push((*name).to_owned());
                }
                for service in &used.services {
                    let on = volume.services.entry(service.clone()).or_default();
                    if !on.iter().any(|listed| listed == name) {
                        on.push((*name).to_owned());
                    }
                }
            }
        }
        Ok(Some(Self {
            node: node.to_owned(),
            volumes: volumes.into_values().collect(),
        }))
    }

    /// What the nodes keep of it under the plan's hash.
    #[must_use]
    pub fn sealed(&self) -> SealedStorage {
        let prefix = format!("{STORAGE_ROOT}/");
        let mut directories = self
            .volumes
            .iter()
            .map(|volume| {
                volume
                    .path
                    .strip_prefix(&prefix)
                    .unwrap_or(&volume.path)
                    .to_owned()
            })
            .collect::<Vec<_>>();
        directories.sort();
        let mut mounting = Vec::new();
        for node in self.volumes.iter().flat_map(|volume| &volume.mounted_on) {
            if !mounting.contains(node) {
                mounting.push(node.clone());
            }
        }
        SealedStorage {
            node: self.node.clone(),
            directories,
            mounting,
        }
    }

    /// One line per volume, as a plan prints it:
    /// `app/uploads: kept on app-1 in …, mounted over the mesh on app-2`.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.volumes
            .iter()
            .map(|volume| {
                let mut line = format!(
                    "{}/{}: kept on {} in {}",
                    volume.stack, volume.volume, self.node, volume.path
                );
                if volume.used_on_storage_node {
                    line.push_str(", used there directly");
                }
                if !volume.mounted_on.is_empty() {
                    line.push_str(&format!(
                        ", mounted over the mesh on {}",
                        volume.mounted_on.join(", ")
                    ));
                }
                let services = volume.services.keys().cloned().collect::<Vec<_>>();
                if !services.is_empty() {
                    line.push_str(&format!(" (for {})", services.join(", ")));
                }
                line
            })
            .collect()
    }
}
