//! The mesh in `mm target setup`. Each node's `mmd` owns its WireGuard
//! interface and makes its own key; setup reads public keys, renders every
//! node's peer set into the reviewed plan, and hands each node its state.

use std::collections::BTreeMap;

use zeroize::Zeroizing;

use super::ClusterSetupService;
use super::reading::READ_NODES;
use crate::app::ports::{
    ClusterNodeCommand, ClusterNodeConnection, ClusterScript, ProgressReporter,
};
use crate::domain::cluster_network::{HostRange, range_conflict};
use crate::domain::cluster_setup::{
    ClusterNodePlan, ClusterSetupPlan, ClusterSkippedNode, HostKeyOrigin,
};
use crate::domain::error::MmError;
use crate::domain::mesh::{
    MESH_INTERFACE, MeshApplied, MeshFacts, MeshMember, MeshPeer, MeshPlan, MeshState, PeerAction,
    desired_peers, endpoint_host, name_peers, plan_node,
};
use crate::domain::progress::{NodeState, ProgressEvent};
use crate::domain::repo_target::{Ipv4Cidr, MeshConfig, RepoTargetConfig, WireguardMesh};

pub(super) const MMD: &str = "/usr/libexec/mmd";

pub(super) fn mmd(words: &[&str]) -> ClusterNodeCommand {
    let mut argv = vec![MMD.to_owned()];
    argv.extend(words.iter().map(|word| (*word).to_owned()));
    ClusterNodeCommand::Program(argv)
}

/// What `mmd mesh key` prints: `{"public-key": "…"}` and, from a node
/// that makes or reports the key it joins the mesh with, the addresses and
/// routes it has outside mm.
#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PrintedKey {
    public_key: String,
    #[serde(default)]
    host_ranges: Vec<HostRange>,
}

fn printed(output: &str, node: &str) -> Result<PrintedKey, MmError> {
    let printed = parse::<PrintedKey>(output, node, "a mesh key")?;
    crate::domain::mesh::validate_public_key(&printed.public_key).map_err(|message| {
        MmError::Target {
            message: format!("node {node}: {message}"),
        }
    })?;
    Ok(printed)
}

/// The key a node printed as `{"public-key": "…"}`.
pub(super) fn printed_key(output: &str, node: &str) -> Result<String, MmError> {
    printed(output, node).map(|printed| printed.public_key)
}

pub(super) fn parse<T: serde::de::DeserializeOwned>(
    output: &str,
    node: &str,
    what: &str,
) -> Result<T, MmError> {
    serde_json::from_str(output).map_err(|error| MmError::Target {
        message: format!("node {node} printed {what} mm does not understand: {error}"),
    })
}

/// A notice for each peer the plan removes because its node left the target.
pub(super) fn departed_notices(plan: &MeshPlan) -> Vec<String> {
    let mut departed = plan
        .nodes
        .iter()
        .flat_map(|node| &node.peers)
        .filter(|peer| peer.action == PeerAction::Remove)
        .map(|peer| peer.name.as_str())
        .collect::<Vec<_>>();
    departed.sort_unstable();
    departed.dedup();
    departed
        .into_iter()
        .map(|name| {
            format!(
                "mesh: {name} is not a node of the target any more, so the remaining nodes drop it as a peer. Setup does not contact {name}: to take the mesh off that host, run `sudo {MMD} mesh leave` on it"
            )
        })
        .collect()
}

/// Refuses a mesh subnet or a network space that overlaps an address or a
/// route one of the nodes in `held` already has.
fn conflict(
    target: &str,
    held: &[(String, Vec<HostRange>)],
    mesh_subnet: Ipv4Cidr,
    networks: Ipv4Cidr,
) -> Result<(), MmError> {
    match held
        .iter()
        .find_map(|(node, _)| range_conflict(target, node, held, Some(mesh_subnet), Some(networks)))
    {
        Some(message) => Err(MmError::Target { message }),
        None => Ok(()),
    }
}

impl ClusterSetupService {
    pub(super) fn connection_as(
        &self,
        user: &str,
        node: &ClusterNodePlan,
    ) -> ClusterNodeConnection {
        ClusterNodeConnection {
            name: node.name.clone(),
            host: node.host.clone(),
            port: node.port,
            user: user.to_owned(),
            host_key: node.host_key.clone(),
            identity_file: self.dependencies.identity_file.clone(),
        }
    }

    /// What a node holds for the mesh. Changes nothing on the node.
    pub(super) async fn mesh_facts(
        &self,
        connection: &ClusterNodeConnection,
    ) -> Result<MeshFacts, MmError> {
        let output = self
            .dependencies
            .shell
            .run(
                connection,
                &ClusterNodeCommand::Script {
                    script: ClusterScript::MeshFacts,
                    arguments: Vec::new(),
                },
                None,
            )
            .await?;
        parse(&output, &connection.name, "mesh facts")
    }

    /// The mesh section of the setup plan. A node whose SSH host key the
    /// repo records is read now, so the plan shows its key and the exact
    /// difference in its peers, and refuses a mesh subnet or a network
    /// space the node already uses. A node with a new host key is read
    /// only during apply, after the operator reviewed that key.
    pub(super) async fn plan_mesh(
        &self,
        target_name: &str,
        target: &RepoTargetConfig,
        user: &str,
        (nodes, skipped): (&[ClusterNodePlan], &[ClusterSkippedNode]),
        progress: &dyn ProgressReporter,
    ) -> Result<Option<MeshPlan>, MmError> {
        let MeshConfig::Wireguard(mesh) = &target.cluster.mesh else {
            return Ok(None);
        };
        let mut members = target
            .mesh_members()
            .map_err(|message| MmError::Target { message })?;
        let recorded = nodes
            .iter()
            .filter(|node| node.host_key_origin == HostKeyOrigin::Recorded)
            .collect::<Vec<_>>();
        let reads = futures_util::future::join_all(recorded.iter().map(|node| async move {
            progress.report(ProgressEvent::node(
                READ_NODES,
                &node.name,
                NodeState::Working,
                Some("reading its mesh"),
            ));
            let read = self.mesh_facts(&self.connection_as(user, node)).await;
            if read.is_err() {
                progress.report(ProgressEvent::node(
                    READ_NODES,
                    &node.name,
                    NodeState::Failed,
                    Some("could not read its mesh"),
                ));
            }
            read
        }))
        .await;
        let mut facts = BTreeMap::new();
        for (node, read) in recorded.into_iter().zip(reads) {
            let read = read?;
            if let Some(member) = members.iter_mut().find(|member| member.name == node.name) {
                member.public_key.clone_from(&read.public_key);
            }
            facts.insert(node.name.clone(), read);
        }
        let held = facts
            .iter()
            .map(|(node, facts)| (node.clone(), facts.host_ranges.clone()))
            .collect::<Vec<_>>();
        conflict(target_name, &held, mesh.subnet, target.cluster.networks)?;
        let left_out = skipped
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        Ok(Some(MeshPlan {
            interface: MESH_INTERFACE.to_owned(),
            subnet: mesh.subnet,
            port: mesh.port,
            mtu: mesh.mtu,
            nodes: members
                .iter()
                .filter(|member| nodes.iter().any(|node| node.name == member.name))
                .map(|member| plan_node(mesh, &members, member, &left_out, facts.get(&member.name)))
                .collect(),
        }))
    }

    /// Every selected node makes or reports its key, and says with it what
    /// addresses and routes it has. A key that differs from the one the
    /// plan showed stops the apply, and so does a node that already uses
    /// the mesh subnet or the space of the cluster's networks: a node with
    /// a new host key could not be read when the plan was made.
    async fn mesh_keys(
        &self,
        plan: &ClusterSetupPlan,
        mesh: &MeshPlan,
    ) -> Result<BTreeMap<String, String>, MmError> {
        let mut keys = BTreeMap::new();
        let mut held = Vec::new();
        for node in &plan.nodes {
            let output = self
                .dependencies
                .shell
                .run(&self.connection(plan, node), &mmd(&["mesh", "key"]), None)
                .await?;
            let PrintedKey {
                public_key: key,
                host_ranges,
            } = printed(&output, &node.name)?;
            held.push((node.name.clone(), host_ranges));
            let planned = mesh
                .nodes
                .iter()
                .find(|planned| planned.node == node.name)
                .and_then(|planned| planned.public_key.as_deref());
            if planned.is_some_and(|planned| planned != key) {
                return Err(MmError::State {
                    message: format!(
                        "node {}'s WireGuard key changed after the plan was reviewed; run `mm target setup plan` again",
                        node.name
                    ),
                });
            }
            keys.insert(node.name.clone(), key);
        }
        conflict(&plan.target, &held, mesh.subnet, plan.cluster.networks)?;
        Ok(keys)
    }

    /// The entries a node holds for the nodes this run leaves out. Only
    /// read when a node is left out: otherwise every key is known.
    async fn held_peers(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
        members: &[MeshMember],
    ) -> Result<Vec<MeshPeer>, MmError> {
        if plan.skipped.is_empty() {
            return Ok(Vec::new());
        }
        let facts = self.mesh_facts(&self.connection(plan, node)).await?;
        let mut held = facts.state.map(|state| state.peers).unwrap_or_default();
        name_peers(&mut held, members);
        Ok(held)
    }

    /// Has each node's `mmd` bring its interface to the planned state.
    async fn apply_mesh(
        &self,
        plan: &ClusterSetupPlan,
        mesh: &WireguardMesh,
        planned: &MeshPlan,
    ) -> Result<(), MmError> {
        let keys = self.mesh_keys(plan, planned).await?;
        let mut members = planned
            .nodes
            .iter()
            .map(|node| MeshMember {
                name: node.node.clone(),
                address: node.address,
                endpoint: endpoint_host(&node.endpoint).to_owned(),
                public_key: keys.get(&node.node).cloned(),
                routed: Vec::new(),
            })
            .collect::<Vec<_>>();
        // Left-out nodes take part only through the entries already held.
        members.extend(plan.skipped.iter().map(|node| MeshMember {
            name: node.name.clone(),
            address: node.private_address,
            endpoint: String::new(),
            public_key: None,
            routed: Vec::new(),
        }));
        let left_out = plan
            .skipped
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        for node in &plan.nodes {
            let held = self.held_peers(plan, node, &members).await?;
            let peers = desired_peers(mesh.port, &members, &node.name, &left_out, &held)
                .iter()
                .filter_map(crate::domain::mesh::PlannedPeer::resolved)
                .collect();
            let state = MeshState::new(mesh, node.private_address, peers);
            state
                .validate()
                .map_err(|message| MmError::Target { message })?;
            let stdin = serde_json::to_string(&state).map_err(|error| MmError::State {
                message: format!("serializing the mesh state: {error}"),
            })?;
            let output = self
                .dependencies
                .shell
                .run(
                    &self.connection(plan, node),
                    &ClusterNodeCommand::Script {
                        script: ClusterScript::Mesh,
                        arguments: Vec::new(),
                    },
                    Some(Zeroizing::new(stdin)),
                )
                .await?;
            parse::<MeshApplied>(&output, &node.name, "a mesh result")?;
        }
        Ok(())
    }

    /// The mesh, unless the target names an existing private interface,
    /// then a check that every node reaches every other on its private
    /// address, with one packet as large as the mesh MTU.
    pub(super) async fn connect_nodes(&self, plan: &ClusterSetupPlan) -> Result<(), MmError> {
        let wireguard = match (&plan.cluster.mesh, &plan.mesh) {
            (MeshConfig::Wireguard(mesh), Some(planned)) => {
                self.apply_mesh(plan, mesh, planned).await?;
                Some(mesh)
            }
            (MeshConfig::Wireguard(_), None) => {
                return Err(MmError::State {
                    message: "the setup plan has no mesh section; run `mm target setup plan` again"
                        .to_owned(),
                });
            }
            (MeshConfig::Interface(_), _) => None,
        };
        for node in &plan.nodes {
            let peers = plan
                .nodes
                .iter()
                .filter(|peer| peer.name != node.name)
                .collect::<Vec<_>>();
            if peers.is_empty() {
                continue;
            }
            let mut arguments = vec![wireguard.map_or(0, |mesh| mesh.mtu).to_string()];
            arguments.extend(peers.iter().map(|peer| peer.private_address.to_string()));
            self.dependencies
                .shell
                .run(
                    &self.connection(plan, node),
                    &ClusterNodeCommand::Script {
                        script: ClusterScript::Reach,
                        arguments,
                    },
                    None,
                )
                .await
                .map_err(|error| reach_error(plan, node, &peers, error))?;
        }
        Ok(())
    }
}

/// Says which two nodes do not reach each other and what has to be open
/// between them, or that the path cannot carry packets of the mesh MTU.
pub(super) fn reach_error(
    plan: &ClusterSetupPlan,
    node: &ClusterNodePlan,
    peers: &[&ClusterNodePlan],
    error: MmError,
) -> MmError {
    let MmError::ProcessFailed { stderr, .. } = &error else {
        return error;
    };
    let finding = stderr.lines().find_map(|line| {
        let (kind, address) = line.trim().split_once(": ")?;
        let peer = peers
            .iter()
            .find(|peer| peer.private_address.to_string() == address.trim())?;
        matches!(kind, "mm-mtu" | "mm-unreachable").then_some((kind, *peer))
    });
    let Some((kind, peer)) = finding else {
        return error;
    };
    let underlay = |name: &str, host: &str| {
        plan.mesh
            .as_ref()
            .and_then(|mesh| mesh.nodes.iter().find(|planned| planned.node == name))
            .map_or_else(
                || host.to_owned(),
                |planned| endpoint_host(&planned.endpoint).to_owned(),
            )
    };
    let (here, there) = (
        underlay(&node.name, &node.host),
        underlay(&peer.name, &peer.host),
    );
    // WireGuard adds 60 bytes to a packet over IPv4 and 80 over IPv6.
    let overhead = if there.contains(':') { 80 } else { 60 };
    let message = match (&plan.cluster.mesh, kind) {
        (MeshConfig::Wireguard(mesh), "mm-mtu") => format!(
            "node {} reaches {} ({}) over the mesh, but a packet of {} bytes does not arrive: the network between {here} and {there} carries less than the {} bytes that takes. Set cluster.mesh.mtu under targets.{} to that network's MTU minus 80 (1380 fits most tunnelled and PPPoE links), then run `mm target setup plan` again",
            node.name,
            peer.name,
            peer.private_address,
            mesh.mtu,
            u32::from(mesh.mtu) + overhead,
            plan.target
        ),
        (MeshConfig::Wireguard(mesh), _) => format!(
            "node {} does not reach {} ({}) over the mesh. UDP port {} must be open between {here} and {there} in both directions: allow it in each host's firewall and in the provider's firewall or security group, then run `mm target setup plan` again. `mm target mesh status` shows each peer's last handshake",
            node.name, peer.name, peer.private_address, mesh.port
        ),
        (MeshConfig::Interface(interface), _) => format!(
            "node {} does not reach {} at {} on {interface}: check that both hosts are attached to that private network and hold the private-address the target declares",
            node.name, peer.name, peer.private_address
        ),
    };
    MmError::Target { message }
}
