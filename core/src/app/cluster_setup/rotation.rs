//! `mm target mesh rotate NODE`: one node replaces its WireGuard key pair.
//!
//! The node makes the new pair itself. Every other node replaces the one
//! entry it holds for that node; nothing else on any node changes, so the
//! links between the other nodes keep their sessions.

use std::path::PathBuf;

use zeroize::Zeroizing;

use super::mesh::{mmd, parse, printed_key};
use super::{ClusterSetupService, cluster_target};
use crate::app::ports::{ClusterNodeCommand, ClusterNodeConnection, ClusterScript};
use crate::domain::error::MmError;
use crate::domain::mesh::{
    MESH_INTERFACE, MeshApplied, MeshMember, MeshRotationPeer, MeshRotationPlan,
    MeshRotationResult, MeshState, peer_set,
};
use crate::domain::repo_target::{MeshConfig, RepoTargetConfig, RepoTargetNode};

/// What the operator passes to plan and apply a key rotation.
#[derive(Debug, Clone)]
pub struct MeshRotationRequest {
    pub start: PathBuf,
    pub environment: Option<String>,
    /// The node whose key is replaced.
    pub node: String,
}

/// Everything a rotation acts on, read from the nodes.
struct Rotation {
    plan: MeshRotationPlan,
    port: u16,
    members: Vec<MeshMember>,
    node: ClusterNodeConnection,
    /// Every other node with the interface state it holds now.
    peers: Vec<(ClusterNodeConnection, MeshState)>,
}

const SET_UP_FIRST: &str =
    "run `mm target setup plan`, apply it, and `mm target setup record` first";

fn target_error(message: String) -> MmError {
    MmError::Target { message }
}

impl ClusterSetupService {
    fn recorded_connection(
        &self,
        target: &RepoTargetConfig,
        node: &RepoTargetNode,
    ) -> Result<ClusterNodeConnection, MmError> {
        let host_key = node.host_key.clone().ok_or_else(|| {
            target_error(format!(
                "node {} has no recorded SSH host key, so mm cannot verify it; {SET_UP_FIRST}",
                node.name
            ))
        })?;
        Ok(ClusterNodeConnection {
            name: node.name.clone(),
            host: node.host.clone(),
            port: node.port,
            user: target
                .setup
                .as_ref()
                .map_or_else(|| "root".to_owned(), |setup| setup.user.clone()),
            host_key,
            identity_file: self.dependencies.identity_file.clone(),
        })
    }

    /// The interface state a node holds, which it must before a rotation.
    async fn current_mesh(
        &self,
        connection: &ClusterNodeConnection,
        rotated: &str,
    ) -> Result<(MeshState, Option<String>, Option<String>), MmError> {
        let facts = self.mesh_facts(connection).await.map_err(|error| match error {
            MmError::ProcessFailed { stderr, .. } | MmError::Target { message: stderr } => {
                target_error(format!(
                    "node {} could not be read ({}). Rotating {rotated}'s key needs every node: one that misses the new key loses its link to {rotated}. Bring the node back, or take it out of nodes: and apply a setup plan first",
                    connection.name,
                    stderr.trim()
                ))
            }
            other => other,
        })?;
        match (facts.state, facts.public_key) {
            (Some(state), Some(key)) if state.interface == MESH_INTERFACE => {
                Ok((state, Some(key), facts.pending_public_key))
            }
            _ => Err(target_error(format!(
                "node {} does not run the mesh interface {MESH_INTERFACE} yet; {SET_UP_FIRST}",
                connection.name
            ))),
        }
    }

    async fn read_rotation(&self, request: &MeshRotationRequest) -> Result<Rotation, MmError> {
        let document = self.load(&request.start)?;
        let environment =
            crate::domain::config::resolve_environment(&document, request.environment.as_deref())?;
        let (target_name, target) = cluster_target(&document, &environment).ok_or_else(|| {
            target_error(format!(
                "environment '{environment}' has no target that lists nodes:; only a cluster target has a mesh"
            ))
        })?;
        let mesh = match &target.cluster.mesh {
            MeshConfig::Wireguard(mesh) => mesh,
            MeshConfig::Interface(interface) => {
                return Err(target_error(format!(
                    "target '{target_name}' uses the existing interface {interface}; mm manages no WireGuard keys for it"
                )));
            }
        };
        let rotated = target
            .nodes
            .iter()
            .find(|node| node.name == request.node)
            .ok_or_else(|| {
                target_error(format!(
                    "target '{target_name}' has no node '{}'; its nodes are {}",
                    request.node,
                    target
                        .nodes
                        .iter()
                        .map(|node| node.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
        let members = target.mesh_members().map_err(target_error)?;
        let address = members
            .iter()
            .find(|member| member.name == rotated.name)
            .map(|member| member.address)
            .ok_or_else(|| target_error(format!("node {} has no mesh address", rotated.name)))?;
        let node = self.recorded_connection(target, rotated)?;
        let (_, current, pending) = self.current_mesh(&node, &rotated.name).await?;
        let mut peers = Vec::new();
        for other in target
            .nodes
            .iter()
            .filter(|other| other.name != rotated.name)
        {
            let connection = self.recorded_connection(target, other)?;
            let (state, ..) = self.current_mesh(&connection, &rotated.name).await?;
            peers.push((connection, state));
        }
        let mut plan = MeshRotationPlan {
            environment,
            target: target_name.to_owned(),
            node: rotated.name.clone(),
            address,
            current_public_key: current.unwrap_or_default(),
            pending_public_key: pending,
            peers: peers
                .iter()
                .map(|(connection, state)| MeshRotationPeer {
                    node: connection.name.clone(),
                    holds: state
                        .peers
                        .iter()
                        .find(|peer| peer.name == rotated.name)
                        .map(|peer| peer.public_key.clone()),
                })
                .collect(),
            plan_hash: String::new(),
        };
        plan.plan_hash = plan.canonical_hash().map_err(target_error)?;
        Ok(Rotation {
            plan,
            port: mesh.port,
            members,
            node,
            peers,
        })
    }

    /// Every peer replaces the key in the one entry it holds for the
    /// rotated node. Returns the peers that changed.
    async fn replace_peer_key(
        &self,
        rotation: &Rotation,
        new_key: &str,
    ) -> Result<Vec<String>, MmError> {
        let rotated = rotation.plan.node.as_str();
        let mut members = rotation.members.clone();
        if let Some(member) = members.iter_mut().find(|member| member.name == rotated) {
            member.public_key = Some(new_key.to_owned());
        }
        let mut updated = Vec::new();
        for (connection, state) in &rotation.peers {
            let mut desired = state.clone();
            match desired.peers.iter_mut().find(|peer| peer.name == rotated) {
                Some(entry) => new_key.clone_into(&mut entry.public_key),
                None => desired.peers.extend(
                    peer_set(rotation.port, &members, &connection.name)
                        .iter()
                        .filter(|peer| peer.name == rotated)
                        .filter_map(crate::domain::mesh::PlannedPeer::resolved),
                ),
            }
            desired.validate().map_err(target_error)?;
            let stdin = serde_json::to_string(&desired).map_err(|error| MmError::State {
                message: format!("serializing the mesh state: {error}"),
            })?;
            let output = self
                .dependencies
                .shell
                .run(
                    connection,
                    &ClusterNodeCommand::Script {
                        script: ClusterScript::Mesh,
                        arguments: Vec::new(),
                    },
                    Some(Zeroizing::new(stdin)),
                )
                .await?;
            if parse::<MeshApplied>(&output, &connection.name, "a mesh result")?.changed {
                updated.push(connection.name.clone());
            }
        }
        Ok(updated)
    }

    /// New sessions with the new key, from the rotated node to each peer.
    async fn check_rotated_links(&self, rotation: &Rotation) -> Result<(), MmError> {
        if rotation.peers.is_empty() {
            return Ok(());
        }
        let mut arguments = vec!["0".to_owned()];
        arguments.extend(
            rotation
                .peers
                .iter()
                .map(|(_, state)| state.address.to_string()),
        );
        self.dependencies
            .shell
            .run(
                &rotation.node,
                &ClusterNodeCommand::Script {
                    script: ClusterScript::Reach,
                    arguments,
                },
                None,
            )
            .await
            .map(drop)
            .map_err(|error| match error {
                MmError::ProcessFailed { .. } => target_error(format!(
                    "node {} switched to its new key, but does not reach every peer with it yet; `mm target mesh status` shows which, and `mm target setup plan` shows what differs",
                    rotation.plan.node
                )),
                other => other,
            })
    }

    /// Reads every node and says which entries a rotation replaces.
    pub async fn plan_mesh_rotation(
        &self,
        request: &MeshRotationRequest,
    ) -> Result<MeshRotationPlan, MmError> {
        Ok(self.read_rotation(request).await?.plan)
    }

    /// Applies the reviewed rotation: the node makes its new pair, every
    /// peer replaces its entry, the node switches over, and the links are
    /// checked. Repeating it after a failure finishes with the same key.
    pub async fn apply_mesh_rotation(
        &self,
        request: &MeshRotationRequest,
        reviewed_hash: &str,
    ) -> Result<MeshRotationResult, MmError> {
        let rotation = self.read_rotation(request).await?;
        let plan = &rotation.plan;
        if plan.plan_hash != reviewed_hash {
            return Err(MmError::State {
                message: format!(
                    "the mesh rotation plan changed (expected {reviewed_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        let shell = &self.dependencies.shell;
        let staged = shell
            .run(&rotation.node, &mmd(&["mesh", "rotate", "--stage"]), None)
            .await?;
        let new_key = printed_key(&staged, &plan.node)?;
        let updated_peers = self.replace_peer_key(&rotation, &new_key).await?;
        let activated = shell
            .run(
                &rotation.node,
                &mmd(&["mesh", "rotate", "--activate"]),
                None,
            )
            .await?;
        if printed_key(&activated, &plan.node)? != new_key {
            return Err(MmError::Security {
                message: format!(
                    "node {} activated another key than the one it staged; run `mm target mesh status` and rotate again",
                    plan.node
                ),
            });
        }
        self.check_rotated_links(&rotation).await?;
        Ok(MeshRotationResult {
            environment: plan.environment.clone(),
            node: plan.node.clone(),
            previous_public_key: plan.current_public_key.clone(),
            public_key: new_key,
            updated_peers,
        })
    }
}
