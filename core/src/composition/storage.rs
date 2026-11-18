//! Wiring for shared volumes: what `mmd` owns on its own host as a node of
//! a cluster target, and the reads and the one delete a client asks for.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;

use super::{RemoteIdentity, remote_environment_as, resolve_runtime};
use crate::adapters::config::ConfigLoader;
use crate::adapters::process::TokioProcessRunner;
use crate::adapters::remote::DaemonConfig;
use crate::adapters::shared_storage::{HostSharedStorage, StorageKeeper};
use crate::app::ports::{MeshHost, RemoteRequestContext, SharedStorageHost};
use crate::app::{RemoteEnvironment, SharedStorageNode, StorageScope, WorkspaceSelection};
use crate::domain::config::resolve_environment;
use crate::domain::error::MmError;
use crate::domain::shared_storage::{StorageReport, VolumeDeletePlan, VolumeDeleted, VolumeName};

/// The host's shared storage: its directories, mounts, and firewall rules.
#[must_use]
pub fn shared_storage_host(config: &DaemonConfig) -> Arc<dyn SharedStorageHost> {
    Arc::new(
        HostSharedStorage::new(Arc::new(TokioProcessRunner), super::mesh_host(config))
            .for_engine(config.host_runtime()),
    )
}

/// What the daemon does for shared volumes on its own host.
#[must_use]
pub fn shared_storage(config: &DaemonConfig) -> SharedStorageNode {
    SharedStorageNode::new(shared_storage_host(config), super::mesh_host(config))
}

/// Starts the containers that could not start while the storage node was
/// away, once [`StorageKeeper::run`] is spawned. It looks at the engine
/// setup installed on the host.
#[must_use]
pub fn storage_keeper(config: &DaemonConfig) -> Arc<StorageKeeper> {
    StorageKeeper::new(
        super::mesh::host_engine(config),
        shared_storage_host(config),
        super::mesh_host(config),
    )
}

/// The node that keeps the shared volumes of the repo target `target`:
/// `cluster.storage`, or its first node. `None` for a target that names
/// one host.
pub(super) fn storage_node(
    document: &crate::domain::config::ConfigDocument,
    target: &str,
) -> Option<String> {
    document
        .config
        .targets
        .get(target)
        .filter(|repo| repo.is_cluster())
        .and_then(|repo| repo.storage_node().map(str::to_owned))
}

/// The mesh address of every node this host's mesh knows, its own under
/// `node`: where a node mounts shared volumes from.
pub(super) fn mesh_addresses(
    mesh: Option<&dyn MeshHost>,
    node: &str,
) -> Result<BTreeMap<String, Ipv4Addr>, MmError> {
    let Some(state) = mesh.map(MeshHost::stored).transpose()?.flatten() else {
        return Ok(BTreeMap::new());
    };
    let mut addresses = state.peer_addresses();
    addresses.insert(node.to_owned(), state.address);
    Ok(addresses)
}

pub(super) fn node(storage: Option<&SharedStorageNode>) -> Result<&SharedStorageNode, MmError> {
    storage.ok_or_else(|| MmError::Environment {
        message: "this daemon does not keep shared volumes".to_owned(),
    })
}

pub(super) fn scope(context: &RemoteRequestContext) -> StorageScope<'_> {
    StorageScope {
        node: context.scope.node.as_deref(),
        project: context.scope.project_id,
        environment: &context.scope.environment,
    }
}

/// The selected environment's remote hosts and the shared volumes its
/// checkout lists, for the commands about shared storage.
fn storage_environment(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    identity: RemoteIdentity,
) -> Result<(RemoteEnvironment, Vec<VolumeName>), MmError> {
    let runtime = resolve_runtime(
        &selection.start,
        home,
        selection.runtime,
        selection.target.as_deref(),
        selection.environment.as_deref(),
        false,
        true,
    )?;
    let environment =
        remote_environment_as(selection, home, runtime, identity)?.ok_or_else(|| {
            MmError::Environment {
                message: "the selected environment runs on this machine; shared volumes are served between the nodes of a remote target, so pass --environment with an environment whose target lists nodes:"
                    .to_owned(),
            }
        })?;
    let document = ConfigLoader.load_from(&selection.start)?;
    let name = resolve_environment(&document, selection.environment.as_deref())?;
    let mut declared = Vec::new();
    for (stack, config) in &document.config.stacks {
        // A stack none of whose services is in the environment shares
        // nothing there.
        let active = document
            .config
            .modules
            .values()
            .flat_map(|module| module.services.values())
            .any(|service| &service.stack == stack && service.is_in_environment(&name));
        for volume in config.shared_volumes.iter().filter(|_| active) {
            let volume = VolumeName::new(stack, volume);
            if !declared.contains(&volume) {
                declared.push(volume);
            }
        }
    }
    Ok((environment, declared))
}

/// The shared storage of the selected environment's cluster target, for
/// `mm target storage status`, the terminal UI, and the MCP server alike.
/// `sizes` has the storage node count each volume's files.
pub(crate) async fn target_storage(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    identity: RemoteIdentity,
    sizes: bool,
) -> Result<StorageReport, MmError> {
    let (environment, declared) = storage_environment(selection, home, identity)?;
    environment.storage_report(&declared, sizes).await
}

/// What deleting the shared volume `volume` would remove. `stack` may be
/// left out when only one stack keeps a volume of that name.
pub(crate) async fn plan_volume_delete(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    (stack, volume): (Option<&str>, &str),
) -> Result<VolumeDeletePlan, MmError> {
    let (environment, declared) = storage_environment(selection, home, RemoteIdentity::User)?;
    let name = volume_named(&environment, &declared, stack, volume).await?;
    environment.plan_volume_delete(&name, &declared).await
}

/// Deletes the shared volume once its plan was reviewed.
pub(crate) async fn delete_volume(
    selection: &WorkspaceSelection,
    home: Option<&Path>,
    (stack, volume): (Option<&str>, &str),
    plan_hash: &str,
) -> Result<VolumeDeleted, MmError> {
    let (environment, declared) = storage_environment(selection, home, RemoteIdentity::User)?;
    let name = volume_named(&environment, &declared, stack, volume).await?;
    environment.delete_volume(&name, &declared, plan_hash).await
}

/// The volume a `delete` names: `stack` and `volume`, or the one volume
/// called `volume` among those the storage node keeps.
async fn volume_named(
    environment: &RemoteEnvironment,
    declared: &[VolumeName],
    stack: Option<&str>,
    volume: &str,
) -> Result<VolumeName, MmError> {
    if let Some(stack) = stack {
        return Ok(VolumeName::new(stack, volume));
    }
    let report = environment.storage_report(declared, false).await?;
    let mut named = report
        .volumes
        .iter()
        .filter(|kept| kept.stored && kept.volume == volume);
    match (named.next(), named.next()) {
        (Some(only), None) => Ok(VolumeName::new(&only.stack, &only.volume)),
        (Some(first), Some(second)) => Err(MmError::Environment {
            message: format!(
                "stacks {} and {} each keep a shared volume '{volume}'; say which with `--stack`",
                first.stack, second.stack
            ),
        }),
        _ => Err(MmError::Environment {
            message: format!(
                "'{}' keeps no shared volume '{volume}' for this environment; `mm target storage status` lists the ones it keeps",
                report.storage_node
            ),
        }),
    }
}
