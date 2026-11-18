//! The WireGuard interface `mmd` owns on its host (`mm0`).
//!
//! The desired interface is a small JSON file under `mmd`'s state directory;
//! the node's private key is a file next to it that never leaves the host.
//! Bringing the interface to the stored state is incremental: only what
//! differs is changed, so applying the same state twice does nothing and a
//! change to one peer leaves the sessions of the others alone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::app::ports::{MeshHost, ProcessRunner};
use crate::domain::cluster_network::HostRange;
use crate::domain::error::MmError;
use crate::domain::mesh::{
    LEGACY_MESH_INTERFACE, MESH_INTERFACE, MeshApplied, MeshFacts, MeshNodeStatus, MeshPeer,
    MeshPeerStatus, MeshState, endpoint_host,
};

mod guard;
mod kernel;
mod legacy;
mod store;
mod sync;

pub use guard::GuardedMesh;
use kernel::Kernel;
use legacy::LegacyMesh;
use store::MeshStore;

/// Where wg-quick, and so the setup scripts of earlier mm versions, kept
/// their files.
const LEGACY_DIRECTORY: &str = "/etc/wireguard";

pub struct WireguardMeshHost {
    /// The cluster networks the last deployment stored: each slice of
    /// another node is routed to that node's peer.
    networks: crate::adapters::cluster_network::NetworkStore,
    store: MeshStore,
    legacy: LegacyMesh,
    kernel: Kernel,
    key_directory: PathBuf,
}

impl std::fmt::Debug for WireguardMeshHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WireguardMeshHost")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

fn no_mesh() -> MmError {
    MmError::Target {
        message: "this host has no mesh: mmd holds no mesh state. `mm target setup plan` and its apply create it".to_owned(),
    }
}

impl WireguardMeshHost {
    /// `state_root` is `mmd`'s own state directory; the mesh lives in its
    /// `mesh` subdirectory.
    #[must_use]
    pub fn new(state_root: &Path, process: Arc<dyn ProcessRunner>) -> Self {
        Self::with_legacy_directory(state_root, PathBuf::from(LEGACY_DIRECTORY), process)
    }

    #[must_use]
    pub fn with_legacy_directory(
        state_root: &Path,
        legacy_directory: PathBuf,
        process: Arc<dyn ProcessRunner>,
    ) -> Self {
        Self {
            networks: crate::adapters::cluster_network::NetworkStore::new(
                state_root.join("networks"),
            ),
            store: MeshStore::new(state_root.join("mesh")),
            key_directory: legacy_directory.clone(),
            legacy: LegacyMesh::new(legacy_directory),
            kernel: Kernel::new(process),
        }
    }

    fn node_private_key(&self) -> Result<Zeroizing<String>, MmError> {
        self.store.private_key()?.ok_or_else(|| MmError::State {
            message: "this node has no mesh key; run `mm target setup` again".to_owned(),
        })
    }

    /// The stored public key, derived again from the key file when the
    /// stored copy is missing.
    async fn public_key(&self) -> Result<Option<String>, MmError> {
        if let Some(key) = self.store.public_key()? {
            return Ok(Some(key));
        }
        match self.store.private_key()? {
            Some(private_key) => Ok(Some(self.kernel.derive_public_key(&private_key).await?)),
            None => Ok(None),
        }
    }

    /// Resolves each peer's endpoint to an address WireGuard can take.
    /// Returns the addresses by peer name and the peers that did not resolve.
    async fn resolve(&self, peers: &[MeshPeer]) -> (BTreeMap<String, String>, Vec<String>) {
        let mut resolved = BTreeMap::new();
        let mut unresolved = Vec::new();
        for peer in peers {
            match tokio::net::lookup_host(peer.endpoint.as_str()).await {
                Ok(addresses) => {
                    let addresses = addresses.collect::<Vec<_>>();
                    let chosen = addresses
                        .iter()
                        .find(|address| address.is_ipv4())
                        .or_else(|| addresses.first());
                    match chosen {
                        Some(address) => {
                            resolved.insert(peer.name.clone(), address.to_string());
                        }
                        None => unresolved.push(peer.name.clone()),
                    }
                }
                Err(error) => {
                    tracing::warn!(peer = %peer.name, endpoint = %peer.endpoint, %error, "mesh endpoint did not resolve");
                    unresolved.push(peer.name.clone());
                }
            }
        }
        (resolved, unresolved)
    }

    /// Creates the interface when it is missing and gives it the node's
    /// key, port, address, and MTU where they differ.
    async fn sync_interface(
        &self,
        desired: &MeshState,
        actions: &mut Vec<String>,
    ) -> Result<(), MmError> {
        let interface = desired.interface.as_str();
        let own_key = self.public_key().await?.ok_or_else(|| MmError::Target {
            message: "this node has no mesh key yet; `mmd mesh key` creates it".to_owned(),
        })?;
        if self.legacy.managed() && self.kernel.link(LEGACY_MESH_INTERFACE).await?.is_some() {
            // It holds the port and the address the new interface takes.
            self.kernel
                .ip(&["link", "delete", "dev", LEGACY_MESH_INTERFACE])
                .await?;
            actions.push(format!(
                "removed the interface {LEGACY_MESH_INTERFACE} of an earlier mm version"
            ));
        }
        let link = match self.kernel.link(interface).await? {
            Some(link) => link,
            None => {
                self.kernel
                    .ip(&["link", "add", "dev", interface, "type", "wireguard"])
                    .await?;
                actions.push(format!("created the interface {interface}"));
                self.kernel
                    .link(interface)
                    .await?
                    .ok_or_else(|| MmError::RuntimeResponse {
                        message: format!("{interface} is missing right after it was created"),
                    })?
            }
        };
        if self.kernel.public_key(interface).await?.as_deref() != Some(own_key.as_str()) {
            let private_key = self.node_private_key()?;
            self.kernel
                .set_private_key(&self.key_directory, interface, &private_key)
                .await?;
            actions.push("set the node's key on the interface".to_owned());
        }
        if self.kernel.listen_port(interface).await? != desired.port {
            let port = desired.port.to_string();
            self.kernel
                .wg(&["set", interface, "listen-port", &port])
                .await?;
            actions.push(format!("listening on UDP port {port}"));
        }
        let wanted = (desired.address, desired.prefix);
        let held = self.kernel.addresses(interface).await?;
        if !held.contains(&wanted) {
            let address = format!("{}/{}", wanted.0, wanted.1);
            self.kernel
                .ip(&["address", "add", &address, "dev", interface])
                .await?;
            actions.push(format!("address {address}"));
        }
        for (address, prefix) in held.into_iter().filter(|held| *held != wanted) {
            let address = format!("{address}/{prefix}");
            self.kernel
                .ip(&["address", "del", &address, "dev", interface])
                .await?;
            actions.push(format!("removed address {address}"));
        }
        if link.mtu != desired.mtu {
            let mtu = desired.mtu.to_string();
            self.kernel
                .ip(&["link", "set", "dev", interface, "mtu", &mtu])
                .await?;
            actions.push(format!("mtu {} -> {mtu}", link.mtu));
        }
        if !link.up {
            self.kernel
                .ip(&["link", "set", "dev", interface, "up"])
                .await?;
            actions.push(format!("brought {interface} up"));
        }
        Ok(())
    }

    /// Routes every prefix a peer may send from, outside the mesh subnet
    /// itself, through the interface, and drops routes no peer has any more.
    async fn sync_routes(
        &self,
        desired: &MeshState,
        actions: &mut Vec<String>,
    ) -> Result<(), MmError> {
        let interface = desired.interface.as_str();
        let subnet = desired.subnet();
        let wanted = desired
            .peers
            .iter()
            .flat_map(|peer| &peer.allowed_ips)
            .filter(|prefix| !(subnet.contains(prefix.address) && prefix.prefix >= subnet.prefix))
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let held = self.kernel.routes(interface).await?;
        for prefix in wanted.iter().filter(|prefix| !held.contains(prefix)) {
            self.kernel
                .ip(&[
                    "route", "replace", prefix, "dev", interface, "proto", "static",
                ])
                .await?;
            actions.push(format!("routed {prefix} through {interface}"));
        }
        for prefix in held.iter().filter(|prefix| !wanted.contains(prefix)) {
            self.kernel
                .ip(&["route", "del", prefix, "dev", interface, "proto", "static"])
                .await?;
            actions.push(format!("removed the route to {prefix}"));
        }
        Ok(())
    }

    /// Brings the kernel to `desired`. Returns what changed and the peers
    /// whose endpoint did not resolve.
    async fn sync(
        &self,
        desired: &MeshState,
        previous: Option<&MeshState>,
    ) -> Result<(Vec<String>, Vec<String>), MmError> {
        let mut actions = Vec::new();
        self.sync_interface(desired, &mut actions).await?;
        let interface = desired.interface.as_str();
        let held = self.kernel.peers(interface).await?;
        let (resolved, unresolved) = self.resolve(&desired.peers).await;
        for command in sync::peer_commands(
            interface,
            &held,
            &desired.peers,
            previous.map(|state| state.peers.as_slice()),
            &resolved,
        ) {
            let arguments = command
                .arguments
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            self.kernel.wg(&arguments).await?;
            actions.push(command.action);
        }
        self.sync_routes(desired, &mut actions).await?;
        Ok((actions, unresolved))
    }

    /// `state` plus the slices of cluster networks routed to each peer.
    /// The stored mesh state stays what setup applied; the slices come from
    /// what the last deployment stored, so neither overwrites the other.
    fn with_slices(&self, state: &MeshState) -> Result<MeshState, MmError> {
        Ok(crate::domain::cluster_network::with_routes(
            state,
            self.networks.state()?.as_ref(),
        ))
    }

    fn unresolved(state: &MeshState, names: &[String]) -> Result<(), MmError> {
        if names.is_empty() {
            return Ok(());
        }
        let hosts = state
            .peers
            .iter()
            .filter(|peer| names.contains(&peer.name))
            .map(|peer| format!("{} ({})", peer.name, endpoint_host(&peer.endpoint)))
            .collect::<Vec<_>>()
            .join(", ");
        Err(MmError::Target {
            message: format!(
                "the mesh is up, but the endpoint of {hosts} does not resolve on this host, so this node waits for that peer to connect first. Check the node's `host` or `endpoint` under the target's nodes: and this host's DNS"
            ),
        })
    }
}

#[async_trait]
impl MeshHost for WireguardMeshHost {
    async fn facts(&self) -> Result<MeshFacts, MmError> {
        let legacy = self.legacy.managed();
        let public_key = match self.public_key().await? {
            Some(key) => Some(key),
            None => match self.legacy.private_key()? {
                Some(private_key) => Some(self.kernel.derive_public_key(&private_key).await?),
                None => None,
            },
        };
        Ok(MeshFacts {
            public_key,
            pending_public_key: self.store.pending_public_key()?,
            legacy_interface: legacy.then(|| LEGACY_MESH_INTERFACE.to_owned()),
            state: match self.store.state()? {
                Some(state) => Some(state),
                None => self.legacy.state(),
            },
            host_ranges: self.host_ranges().await?,
            release: Some(crate::domain::cluster_setup::HostRelease {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                protocol: crate::domain::remote::PROTOCOL_VERSION,
                build: own_build(),
            }),
        })
    }

    fn stored(&self) -> Result<Option<MeshState>, MmError> {
        self.store.state()
    }

    async fn host_ranges(&self) -> Result<Vec<HostRange>, MmError> {
        let mut own = vec![MESH_INTERFACE.to_owned()];
        if let Some(state) = self.store.state()? {
            own.push(state.interface);
        }
        if self.legacy.managed() {
            own.push(LEGACY_MESH_INTERFACE.to_owned());
        }
        self.kernel.host_ranges(&own).await
    }

    async fn ensure_key(&self) -> Result<String, MmError> {
        if self.store.has_key()
            && let Some(key) = self.public_key().await?
        {
            if self.store.public_key()?.is_none()
                && let Some(private_key) = self.store.private_key()?
            {
                self.store.write_key_pair(&private_key, &key, false)?;
            }
            return Ok(key);
        }
        // A host an earlier mm version set up keeps its key, so the peers
        // that have not been migrated yet still recognise it.
        let private_key = match self.legacy.private_key()? {
            Some(key) => key,
            None => self.kernel.generate_key().await?,
        };
        let public_key = self.kernel.derive_public_key(&private_key).await?;
        self.store
            .write_key_pair(&private_key, &public_key, false)?;
        Ok(public_key)
    }

    async fn apply(&self, desired: &MeshState) -> Result<MeshApplied, MmError> {
        desired
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let previous = self.store.state()?;
        let routed = self.with_slices(desired)?;
        let (mut actions, unresolved) = self.sync(&routed, previous.as_ref()).await?;
        if self.store.write_state(desired)? {
            actions.push("stored the mesh state".to_owned());
        }
        if self.legacy.managed() {
            self.legacy.remove()?;
            actions.push("removed the WireGuard configuration of an earlier mm version".to_owned());
        }
        Self::unresolved(desired, &unresolved)?;
        Ok(MeshApplied {
            changed: !actions.is_empty(),
            actions,
        })
    }

    async fn up(&self) -> Result<MeshApplied, MmError> {
        let state = self.store.state()?.ok_or_else(no_mesh)?;
        // The stored state is also what was applied before, so a peer that
        // roamed keeps the endpoint WireGuard learned for it.
        let routed = self.with_slices(&state)?;
        let (actions, unresolved) = self.sync(&routed, Some(&state)).await?;
        Self::unresolved(&state, &unresolved)?;
        Ok(MeshApplied {
            changed: !actions.is_empty(),
            actions,
        })
    }

    async fn status(&self) -> Result<MeshNodeStatus, MmError> {
        let Some(state) = self.store.state()? else {
            return Ok(MeshNodeStatus::default());
        };
        let interface = state.interface.as_str();
        let mut status = MeshNodeStatus {
            node: None,
            interface: Some(interface.to_owned()),
            up: false,
            address: Some(state.address),
            port: Some(state.port),
            mtu: Some(state.mtu),
            public_key: self.public_key().await?,
            peers: Vec::new(),
            guard_drift: Vec::new(),
        };
        let held = match self.kernel.link(interface).await? {
            Some(link) => {
                status.up = link.up;
                status.mtu = Some(link.mtu);
                status.port = Some(self.kernel.listen_port(interface).await?);
                self.kernel.peers(interface).await?
            }
            None => Vec::new(),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        for peer in &held {
            let configured = state
                .peers
                .iter()
                .find(|configured| configured.public_key == peer.public_key);
            status.peers.push(MeshPeerStatus {
                name: configured.map(|configured| configured.name.clone()),
                public_key: peer.public_key.clone(),
                endpoint: peer.endpoint.clone(),
                configured_endpoint: configured.map(|configured| configured.endpoint.clone()),
                allowed_ips: peer.allowed_ips.clone(),
                handshake_age_seconds: (peer.latest_handshake != 0)
                    .then(|| now.saturating_sub(peer.latest_handshake)),
                received_bytes: peer.received_bytes,
                sent_bytes: peer.sent_bytes,
                on_interface: true,
            });
        }
        for configured in state.peers.iter().filter(|configured| {
            !held
                .iter()
                .any(|peer| peer.public_key == configured.public_key)
        }) {
            status.peers.push(MeshPeerStatus {
                name: Some(configured.name.clone()),
                public_key: configured.public_key.clone(),
                endpoint: None,
                configured_endpoint: Some(configured.endpoint.clone()),
                allowed_ips: configured
                    .allowed_ips
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                handshake_age_seconds: None,
                received_bytes: 0,
                sent_bytes: 0,
                on_interface: false,
            });
        }
        Ok(status)
    }

    async fn stage_key(&self) -> Result<String, MmError> {
        if !self.store.has_key() {
            return Err(no_mesh());
        }
        if let Some(pending) = self.store.pending_public_key()? {
            return Ok(pending);
        }
        let private_key = self.kernel.generate_key().await?;
        let public_key = self.kernel.derive_public_key(&private_key).await?;
        self.store.write_key_pair(&private_key, &public_key, true)?;
        Ok(public_key)
    }

    async fn activate_key(&self) -> Result<String, MmError> {
        let Some(pending) = self.store.pending_public_key()? else {
            return Err(MmError::State {
                message: "this node has no pending mesh key; `mm target mesh rotate` makes one"
                    .to_owned(),
            });
        };
        self.store.activate_pending()?;
        if self.kernel.link(MESH_INTERFACE).await?.is_some() {
            let private_key = self.node_private_key()?;
            self.kernel
                .set_private_key(&self.key_directory, MESH_INTERFACE, &private_key)
                .await?;
        }
        Ok(pending)
    }

    async fn leave(&self) -> Result<MeshApplied, MmError> {
        let mut actions = Vec::new();
        if self.kernel.link(MESH_INTERFACE).await?.is_some() {
            self.kernel
                .ip(&["link", "delete", "dev", MESH_INTERFACE])
                .await?;
            actions.push(format!("removed the interface {MESH_INTERFACE}"));
        }
        if self.store.state()?.is_some() || self.store.has_key() {
            actions.push("removed the mesh state and the node's key".to_owned());
        }
        self.store.remove_all()?;
        Ok(MeshApplied {
            changed: !actions.is_empty(),
            actions,
        })
    }
}

#[cfg(test)]
#[path = "mesh/tests.rs"]
mod tests;

/// The digest of the running `mmd`, read once.
fn own_build() -> Option<String> {
    static BUILD: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    BUILD
        .get_or_init(|| {
            std::env::current_exe()
                .and_then(std::fs::read)
                .ok()
                .map(|executable| crate::domain::cluster_setup::build_digest(&executable))
        })
        .clone()
}
