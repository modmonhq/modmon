//! The networks that span a cluster target: what the client plans with, and
//! what a node's `mmd` stores and reports.

use crate::app::ports::{ClusterNetworkHost, RuntimeDriver};
use crate::domain::cluster_network::{
    NetworkDrift, NetworkDriftKind, NetworkTable, NodeNetworkStatus, SealedNetworks,
};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::repo_target::MeshConfig;

/// The private interface the nodes of the repo target `target` already
/// share, when it names one instead of having mm create a mesh.
pub(super) fn existing_interface(document: &ConfigDocument, target: &str) -> Option<String> {
    let repo = document.config.targets.get(target)?;
    match &repo.cluster.mesh {
        MeshConfig::Interface(interface) if repo.is_cluster() => Some(interface.clone()),
        _ => None,
    }
}

/// The address space, mesh interface, and MTU of the networks that span
/// `target`. On a cluster whose mesh mm owns, the networks of `mm.yaml`
/// span the nodes; on an existing private interface, and on a target that
/// names one host, they stay local to each host.
pub(super) fn settings(document: &ConfigDocument, target: &str) -> Option<SealedNetworks> {
    let repo = document
        .config
        .targets
        .get(target)
        .filter(|repo| repo.spans_networks())?;
    match &repo.cluster.mesh {
        MeshConfig::Wireguard(mesh) => Some(SealedNetworks {
            space: repo.cluster.networks,
            mesh_interface: crate::domain::mesh::MESH_INTERFACE.to_owned(),
            mtu: mesh.mtu,
            table: NetworkTable::default(),
            retired: Vec::new(),
        }),
        MeshConfig::Interface(_) => None,
    }
}

/// What this host stored for the cluster's networks and what differs from
/// it: in the kernel, and in the addresses the engine's networks have.
pub(super) async fn status(
    host: &dyn ClusterNetworkHost,
    runtime: &dyn RuntimeDriver,
) -> Result<NodeNetworkStatus, MmError> {
    let stored = host.stored()?;
    let locals = stored
        .iter()
        .flat_map(|stored| stored.local())
        .collect::<Vec<_>>();
    // The kernel and the engine are asked at the same time.
    let (drift, observed) = tokio::join!(host.drift(), runtime.observe_cluster_networks(&locals));
    let mut drift = drift?;
    for (local, observed) in locals.iter().zip(observed?) {
        if let Some(observed) = observed.filter(|observed| !observed.current) {
            drift.push(NetworkDrift::new(
                NetworkDriftKind::Network,
                format!(
                    "network {} ({}) has {} on the engine instead of {} of {}",
                    local.key, local.network, observed.addresses, local.slice, local.block
                ),
            ));
        }
    }
    Ok(NodeNetworkStatus {
        node: None,
        stored,
        drift,
        host_ranges: Vec::new(),
    })
}

/// Takes the networks a finished rollout retired out of what this node
/// stored, with their routes and firewall rules, and removes each one's
/// bridge network from the engine. A network a container is still on stays
/// until a later deployment. Returns the networks taken out.
pub(super) async fn retire(
    host: &dyn ClusterNetworkHost,
    retired: &[String],
) -> Result<Vec<String>, MmError> {
    let Some(stored) = host.stored()? else {
        return Ok(Vec::new());
    };
    let mut desired = stored.clone();
    let mut taken = Vec::new();
    for key in retired {
        if !stored.table.networks.contains_key(key) {
            continue;
        }
        if let Some(local) = stored.local_network(key)
            && !host.remove_bridge(&local.network).await?
        {
            continue;
        }
        desired.table.networks.remove(key);
        taken.push(key.clone());
    }
    if !taken.is_empty() {
        host.store(&desired)?;
        host.converge().await?;
    }
    Ok(taken)
}

/// Stores the addresses a reviewed cluster plan gives the networks, as node
/// `node`. An address never changes once given, so a plan made before
/// another deployment gave some out is refused and has to be made again.
pub(super) async fn store(
    host: &dyn ClusterNetworkHost,
    node: &str,
    networks: &SealedNetworks,
    converge: bool,
) -> Result<(), MmError> {
    networks
        .validate()
        .map_err(|message| MmError::Security { message })?;
    let stored = host.stored()?;
    // A network the plan retires stays until the rollout finished here.
    let desired = networks.on_node_keeping(node, stored.as_ref());
    if let Some(stored) = stored
        && !stored.table.is_kept_by(&desired.table)
    {
        return Err(MmError::State {
            message: format!(
                "node '{node}' gave out network addresses after this plan was made (another deployment was applied); run `mm deploy plan` again"
            ),
        });
    }
    host.store(&desired)?;
    if converge {
        host.converge().await?;
    }
    Ok(())
}

/// Which network names of a name report are the request's environment's,
/// by the environments of the project at `root`: one whose name starts
/// like it (`staging-eu` for `staging`) keeps its own. Without the
/// project's file, every name the report has.
pub(super) fn environment_networks(
    context: &crate::app::ports::RemoteRequestContext,
    root: &std::path::Path,
) -> impl Fn(&str) -> bool + use<> {
    let environments = crate::adapters::config::ConfigLoader
        .load_from(root)
        .map(|document| document.config.environments.into_keys().collect::<Vec<_>>())
        .unwrap_or_default();
    let (project, environment) = (context.scope.project_id, context.scope.environment.clone());
    move |name: &str| {
        let key = crate::domain::cluster_network::network_key(project, &environment, name);
        crate::domain::cluster_network::environment_network(
            &key,
            project,
            &environment,
            &environments,
        )
        .is_some()
    }
}
