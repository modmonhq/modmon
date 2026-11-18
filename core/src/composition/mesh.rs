//! Wiring for what `mmd` owns on its own host as a cluster node: the
//! WireGuard interface, the cluster networks, and the name resolver.

use std::sync::Arc;

use crate::adapters::cluster_network::HostClusterNetworks;
use crate::adapters::discovery::NameResolver;
use crate::adapters::mesh::{GuardedMesh, WireguardMeshHost};
use crate::adapters::process::TokioProcessRunner;
use crate::adapters::remote::DaemonConfig;
use crate::app::ports::{ClusterNetworkHost, MeshHost};

/// The host's mesh, kept under the daemon's state directory.
#[must_use]
pub fn mesh_host(config: &DaemonConfig) -> Arc<dyn MeshHost> {
    // With the firewall chains that keep the mesh between the hosts: they
    // are loaded and removed with the interface, and the forward chain is
    // entered where the engine setup installed lets mm's rules in.
    Arc::new(
        GuardedMesh::new(
            &config.state_root,
            Arc::new(WireguardMeshHost::new(
                &config.state_root,
                Arc::new(TokioProcessRunner),
            )),
            Arc::new(TokioProcessRunner),
        )
        .for_engine(config.host_runtime()),
    )
}

/// The host's cluster networks, kept under the daemon's state directory,
/// with mm's chains where the host's engine lets them in.
#[must_use]
pub fn cluster_networks(config: &DaemonConfig) -> Arc<dyn ClusterNetworkHost> {
    Arc::new(
        HostClusterNetworks::new(
            &config.state_root,
            Arc::new(TokioProcessRunner),
            mesh_host(config),
        )
        .for_engine(config.host_runtime()),
    )
}

/// The engine of this host, as setup recorded it: what `mmd` looks at
/// outside a request.
#[must_use]
pub(crate) fn host_engine(config: &DaemonConfig) -> Arc<dyn crate::app::ports::RuntimeDriver> {
    super::RuntimeComposition::new().driver(config.host_runtime())
}

/// The host's name resolver. It serves once [`NameResolver::run`] is
/// spawned, and only on a host that holds cluster networks. It looks at
/// the engine setup installed on the host. Its sockets stay open in
/// `sockets` while `mmd` restarts.
#[must_use]
pub fn name_resolver(
    config: &DaemonConfig,
    sockets: crate::adapters::kept_sockets::KeptSockets,
) -> Arc<NameResolver> {
    NameResolver::keeping(
        (
            cluster_networks(config),
            mesh_host(config),
            host_engine(config),
        ),
        crate::adapters::discovery::ResolverOptions::default(),
        sockets,
    )
}

/// The keeper of the containers that leave this node (`domain::leaving`).
#[must_use]
pub fn leave_keeper(
    config: &DaemonConfig,
    names: Arc<NameResolver>,
) -> Arc<crate::adapters::leaving::LeaveKeeper> {
    crate::adapters::leaving::LeaveKeeper::new(
        &config.state_root,
        names,
        Arc::new(TokioProcessRunner),
        config.host_runtime(),
    )
}

/// The node's ingress proxy and the keeper of its routes
/// (`domain::cluster_ingress`).
#[must_use]
pub fn ingress_keeper(
    config: &DaemonConfig,
    names: Arc<NameResolver>,
) -> Arc<crate::adapters::cluster_ingress::IngressKeeper> {
    crate::adapters::cluster_ingress::IngressKeeper::new(
        crate::adapters::cluster_ingress::IngressOptions::on_node(
            &config.state_root,
            config.host_runtime(),
        ),
        names,
        Arc::new(TokioProcessRunner),
    )
}

/// What `mmd container leave` does: writes the leave of `container` for
/// the running `mmd` to carry out; with `restart`, a restart that drops
/// no request.
pub async fn request_leave(
    config: &DaemonConfig,
    container: &str,
    drain: Option<std::time::Duration>,
    restart: bool,
) -> Result<crate::domain::leaving::LeaveRecord, crate::domain::error::MmError> {
    crate::adapters::leaving::request_leave(
        &config.state_root,
        (&TokioProcessRunner, config.host_runtime()),
        container,
        (drain, restart),
    )
    .await
}

/// What `mmd network show` prints on a node: the cluster networks it
/// stored, and what its kernel and its engine have that differs. This is
/// what the node answers a client's `Networks` request with, so the two
/// never disagree about drift.
pub async fn node_network_status(
    config: &DaemonConfig,
) -> Result<crate::domain::cluster_network::NodeNetworkStatus, crate::domain::error::MmError> {
    let runtime = host_engine(config);
    let mut status =
        super::cluster_networks::status(cluster_networks(config).as_ref(), runtime.as_ref())
            .await?;
    status.node = config.node_env()?.map(|node| node.name);
    status.host_ranges = mesh_host(config).host_ranges().await?;
    Ok(status)
}

/// The node's restarts of containers that keep failing their health check
/// (`domain::healing`): it reads the API of the engine setup installed.
#[must_use]
pub fn healer(
    config: &DaemonConfig,
    names: Arc<NameResolver>,
) -> Arc<crate::adapters::healing::Healer> {
    crate::adapters::healing::Healer::new(
        &config.state_root,
        names,
        super::engine_api::host_api(config.host_runtime()),
    )
}

/// The node's autoscaler (`domain::autoscale`): it reads the API of the
/// engine setup installed (Docker's socket, or rootful Podman's), the
/// node's metrics and what the other nodes say, and has containers start
/// through the daemon and leave through `leaves`.
#[must_use]
pub fn autoscaler(
    config: &DaemonConfig,
    names: Arc<NameResolver>,
    metrics: Arc<dyn crate::app::ports::Metrics>,
    leaves: Arc<crate::adapters::leaving::LeaveKeeper>,
) -> Arc<crate::adapters::autoscale::Autoscaler> {
    crate::adapters::autoscale::Autoscaler::new(
        &config.state_root,
        names,
        metrics,
        leaves,
        super::engine_api::host_api(config.host_runtime()),
    )
}
