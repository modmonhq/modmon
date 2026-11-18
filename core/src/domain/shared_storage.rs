//! Shared volumes: directories on a cluster's storage node that every node
//! running a service on them sees as one.
//!
//! The storage node keeps each shared volume as a directory below
//! [`STORAGE_ROOT`] and serves that root over NFSv4.2 on its mesh address.
//! A service on the storage node uses the directory itself; a service on
//! another node gets an engine volume that mounts it over the mesh. Either
//! way the Compose volume keeps its key, so the service's own `volumes:`
//! lines stay as they are.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::workspace::ProjectId;

#[path = "shared_storage/firewall.rs"]
mod firewall;
#[path = "shared_storage/keeper.rs"]
mod keeper;
#[path = "shared_storage/plan.rs"]
mod plan;
#[path = "shared_storage/status.rs"]
mod status;
pub use firewall::{STORAGE_FORWARD_CHAIN, STORAGE_INPUT_CHAIN, StorageFirewall};
pub use keeper::WaitingContainer;
pub use plan::{ClusterStoragePlan, SealedStorage, SharedVolumePlan, SharedVolumeUse};
pub use status::{
    STORAGE_REACH_TIMEOUT_MS, StorageNodeReport, StorageNodeStatus, StorageReport, StoredVolume,
    VolumeDeletePlan, VolumeDeleted, VolumeReport, mounted_volumes, storage_diagnostics,
};

/// Where the storage node keeps shared volumes, and what it exports.
pub const STORAGE_ROOT: &str = "/var/lib/mm/storage";
/// The storage node's NFS port on its mesh address.
pub const NFS_PORT: u16 = 2049;
/// How a node mounts a shared volume.
///
/// - `hard`: while the storage node is away, reads and writes wait for it
///   and go on when it is back. Nothing is reported written that was not.
///   (`soft` would hand the application an error after the timeout, and
///   data it believed written could be lost.)
/// - `timeo=100,retrans=2`: a request is sent again after 10, 20, and 30
///   seconds. This is how soon a waiting node notices that the storage node
///   is back, and how long a mount is tried before a container fails to
///   start with "connection timed out" (about half a minute, measured).
/// - `lookupcache=positive`: a name that did not exist is asked for again
///   every time. With the kernel's default a file created on one node
///   stayed "not found" for up to a minute on a node that had looked for
///   it before (measured).
pub const MOUNT_OPTIONS: &str = "nfsvers=4.2,hard,timeo=100,retrans=2,lookupcache=positive";
/// Engine volumes that hold a shared volume start with this.
pub const ENGINE_VOLUME_PREFIX: &str = "mm-shared-";
/// On a container that uses a shared volume: the storage node's mesh
/// address, or [`STORAGE_LABEL_LOCAL`] on the storage node itself.
pub const STORAGE_LABEL: &str = "io.mm.shared-storage";
pub const STORAGE_LABEL_LOCAL: &str = "local";
/// On an engine volume that holds a shared volume: `<stack>/<volume>`.
pub const VOLUME_LABEL: &str = "io.mm.shared-volume";

/// A shared volume by the names a configuration gives it.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct VolumeName {
    pub stack: String,
    pub volume: String,
}

impl VolumeName {
    #[must_use]
    pub fn new(stack: &str, volume: &str) -> Self {
        Self {
            stack: stack.to_owned(),
            volume: volume.to_owned(),
        }
    }
}

impl std::fmt::Display for VolumeName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.stack, self.volume)
    }
}

/// One part of a shared volume's directory: letters, digits, `.`, `_`, and
/// `-`, and neither `.` nor `..`.
fn directory_part(kind: &str, value: &str) -> Result<(), String> {
    let plain = !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if plain {
        Ok(())
    } else {
        Err(format!(
            "the {kind} name '{value}' cannot name a directory of shared storage; use letters, digits, '.', '_', and '-', at most 128 of them"
        ))
    }
}

/// The directory of every shared volume of one environment, below the
/// storage root: `<project id>/<environment>`.
pub fn environment_directory(project: ProjectId, environment: &str) -> Result<String, String> {
    directory_part("environment", environment)?;
    Ok(format!("{}/{environment}", project.0))
}

/// A shared volume's directory below the storage root:
/// `<project id>/<environment>/<stack>/<volume>`.
pub fn volume_directory(
    project: ProjectId,
    environment: &str,
    name: &VolumeName,
) -> Result<String, String> {
    directory_part("stack", &name.stack)?;
    directory_part("volume", &name.volume)?;
    Ok(format!(
        "{}/{}/{}",
        environment_directory(project, environment)?,
        name.stack,
        name.volume
    ))
}

/// The volume a directory below the storage root belongs to, when it is one
/// of `project`'s in `environment`. `path` may be longer at its start (a
/// mount source such as `:/…` or an absolute path).
#[must_use]
pub fn volume_of_directory(
    project: ProjectId,
    environment: &str,
    path: &str,
) -> Option<VolumeName> {
    let prefix = format!("{}/{environment}/", project.0);
    let start = path.find(&prefix)?;
    if start > 0 && !path[..start].ends_with('/') {
        return None;
    }
    let mut parts = path[start + prefix.len()..].split('/');
    let (stack, volume) = (parts.next()?, parts.next()?);
    if parts.next().is_some() || stack.is_empty() || volume.is_empty() {
        return None;
    }
    Some(VolumeName::new(stack, volume))
}

/// Where one node of a cluster finds the cluster's shared volumes. Never
/// read from a file: `mmd` on a node works it out from the target's storage
/// node and the node's own mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageAccess {
    /// This node is the storage node: a volume is its directory.
    Directory { node: String, root: PathBuf },
    /// Another node keeps the volumes; this one mounts them from `address`.
    Mesh { node: String, address: Ipv4Addr },
    /// The node cannot use shared volumes; `reason` says why and what to
    /// run.
    Unavailable { node: String, reason: String },
}

/// The engine volume that stands for a shared volume on one node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineVolume {
    pub name: String,
    pub driver_opts: BTreeMap<String, String>,
    /// The value of [`STORAGE_LABEL`] for containers that use it.
    pub storage_label: String,
    /// The volume's directory below the storage root.
    pub directory: String,
}

impl StorageAccess {
    /// The storage node's name.
    #[must_use]
    pub fn node(&self) -> &str {
        match self {
            Self::Directory { node, .. }
            | Self::Mesh { node, .. }
            | Self::Unavailable { node, .. } => node,
        }
    }

    /// Whether this node keeps the volumes itself.
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Directory { .. })
    }

    /// The engine volume for `name` on this node. Its name carries a digest
    /// of how it is mounted, so a node never keeps using a volume that
    /// points somewhere else: another storage node, or other options.
    pub fn engine_volume(
        &self,
        project: ProjectId,
        environment: &str,
        name: &VolumeName,
    ) -> Result<EngineVolume, String> {
        let directory = volume_directory(project, environment, name)?;
        let (driver_opts, storage_label) = match self {
            Self::Directory { root, .. } => (
                BTreeMap::from([
                    ("type".to_owned(), "none".to_owned()),
                    ("o".to_owned(), "bind".to_owned()),
                    (
                        "device".to_owned(),
                        root.join(&directory).to_string_lossy().into_owned(),
                    ),
                ]),
                STORAGE_LABEL_LOCAL.to_owned(),
            ),
            Self::Mesh { address, .. } => (
                BTreeMap::from([
                    ("type".to_owned(), "nfs".to_owned()),
                    ("o".to_owned(), format!("addr={address},{MOUNT_OPTIONS}")),
                    // Docker Engine mounts with the system call, which
                    // takes the server from `addr`; Podman runs mount(8),
                    // whose NFS helper takes it from the source.
                    ("device".to_owned(), format!("{address}:/{directory}")),
                ]),
                address.to_string(),
            ),
            Self::Unavailable { reason, .. } => return Err(reason.clone()),
        };
        let mut hasher = blake3::Hasher::new();
        for (key, value) in &driver_opts {
            hasher.update(&(key.len() as u64).to_be_bytes());
            hasher.update(key.as_bytes());
            hasher.update(&(value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        let digest = hasher.finalize().to_hex();
        let project = project.0.as_simple().to_string();
        Ok(EngineVolume {
            name: format!(
                "{ENGINE_VOLUME_PREFIX}{}-{}-{}-{}-{}",
                project.get(..12).unwrap_or(&project),
                engine_name_part(environment),
                engine_name_part(&name.stack),
                engine_name_part(&name.volume),
                digest.get(..8).unwrap_or(&digest),
            ),
            driver_opts,
            storage_label,
            directory,
        })
    }
}

/// A name as part of an engine volume's name: the engines take letters,
/// digits, `_`, `.`, and `-`, which is what a directory part is made of.
fn engine_name_part(value: &str) -> String {
    value.to_ascii_lowercase()
}

/// Why shared volumes need mm's own mesh.
#[must_use]
pub fn needs_mesh(target: &str, interface: &str) -> String {
    format!(
        "shared volumes travel over the WireGuard mesh mm creates, and target '{target}' uses the existing interface '{interface}' (targets.{target}.cluster.mesh.interface), where mm cannot keep the storage traffic encrypted and away from containers. Remove `interface` so mm creates the mesh, or take the volume out of `shared-volumes` and pin its service to one node with `placement:`"
    )
}

/// How to move shared storage to another node by hand, for the refusal of
/// a changed `cluster.storage`.
#[must_use]
pub fn migration_steps(target: &str, from: &str, to: &str) -> String {
    format!(
        "{from} holds shared volumes with files in them, and mm does not copy them. To move them to {to}: 1. set targets.{target}.cluster.storage back to {from} so nothing changes while you work; 2. stop the services that use shared volumes (take them out of the configuration and deploy, or `docker stop` them on every node); 3. on {to}, copy the directory from {from} with ownership and permissions, for example `rsync -aHAX --numeric-ids root@{from}:{STORAGE_ROOT}/ {STORAGE_ROOT}/` over a connection of your own; 4. on {from}, move the old copy aside: `mv {STORAGE_ROOT} {STORAGE_ROOT}.moved`; 5. set cluster.storage to {to}, run `mm target setup plan` and apply it, then deploy. Delete {STORAGE_ROOT}.moved on {from} when the services run on the copy"
    )
}
