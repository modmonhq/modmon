//! Remote targets declared in the repo's `targets:` key: public connection
//! facts only, so a fresh clone can reach every environment. Clients pin the
//! keys on first use and refuse a silent change (`mm target trust`).

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::removed::{self, Removed};
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::{
    NamedTarget, TargetBootstrap, TargetDefinition, TargetId, TargetNode, TargetTransport,
};
use crate::domain::workspace::ProjectId;

#[path = "repo_target/cluster.rs"]
mod cluster;
pub use cluster::{
    DEFAULT_CLUSTER_NETWORKS, Ipv4Cidr, MESH_INTERFACE, MeshConfig, RepoBreakGlassKey,
    RepoClusterConfig, RepoDeployKey, RepoTargetNode, RepoTargetSetup, WireguardMesh,
};

#[path = "repo_target/providers.rs"]
mod providers;
pub use providers::{DeclaredProviderAddress, TargetProviderConfig, validate_provider_address};

/// The account every remote host exposes for the relay.
pub const RELAY_ACCOUNT: &str = "mm";

/// A repo target takes one of two forms. The flat form names one host. The
/// cluster form lists `nodes` instead; `mm target setup` prepares them and
/// `mm target setup record` writes each node's keys and address back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RepoTargetConfig {
    /// Flat form: the host mm connects to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default = "default_port", skip_serializing_if = "is_default_port")]
    pub port: u16,
    /// The host's SSH public key, `<type> <base64>`; its fingerprint is derived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key: Option<String>,
    /// Printed by `mmd --print-identity` on the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_fingerprint: Option<String>,
    /// Cluster form: every node. Replaces `host`, `host-key`, and
    /// `daemon-fingerprint`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<RepoTargetNode>,
    /// Cluster form: the nodes' private network, storage node, and groups.
    #[serde(default, skip_serializing_if = "RepoClusterConfig::is_default")]
    pub cluster: RepoClusterConfig,
    /// Cluster form: what setup installs on every node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<RepoTargetSetup>,
    /// How the target's hosts get a commit's files. `push`: mm sends them
    /// over its own connection and the hosts never contact the git server.
    /// Default `fetch`: each host fetches from the origin.
    #[serde(
        default,
        skip_serializing_if = "crate::domain::delivery::Delivery::is_fetch"
    )]
    pub delivery: crate::domain::delivery::Delivery,
    /// Where this target's hosts read an external secret provider, by the
    /// provider's name under `providers:`. The hosts resolve the secrets
    /// themselves; see `mmd secrets connect`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub providers: std::collections::BTreeMap<
        crate::domain::secret_backend::ProviderInstanceName,
        TargetProviderConfig,
    >,
    /// Removed; reading it explains what to write instead.
    #[serde(default, skip_serializing, deserialize_with = "removed::members")]
    #[schemars(skip)]
    pub members: Removed,
    /// Removed; reading it explains what to write instead.
    #[serde(default, skip_serializing, deserialize_with = "removed::swarm_block")]
    #[schemars(skip)]
    pub swarm: Removed,
}

const fn default_port() -> u16 {
    22
}

/// serde's `skip_serializing_if` passes a reference.
const fn is_default_port(port: &u16) -> bool {
    *port == default_port()
}

/// What a client pinned for a repo target the first time it connected: one
/// host key and daemon identity for a target that names one host, or one
/// pair per node of a cluster target.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TargetPin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_fingerprint: Option<String>,
    /// The keys of a cluster target's nodes, in the repo's order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodePin>,
    /// Keys pinned by a development build that did not record node names.
    /// Read so those pins still cover the same hosts; never written.
    #[serde(default, skip_serializing)]
    members: Vec<UnnamedPin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct UnnamedPin {
    host_key: String,
    daemon_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodePin {
    pub name: String,
    pub host_key: String,
    pub daemon_fingerprint: String,
}

impl TargetPin {
    /// Every host key and daemon identity pair this pin trusts.
    fn identities(&self) -> BTreeSet<(&str, &str)> {
        self.host_key
            .as_deref()
            .zip(self.daemon_fingerprint.as_deref())
            .into_iter()
            .chain(
                self.nodes
                    .iter()
                    .map(|node| (node.host_key.as_str(), node.daemon_fingerprint.as_str())),
            )
            .chain(
                self.members
                    .iter()
                    .map(|pin| (pin.host_key.as_str(), pin.daemon_fingerprint.as_str())),
            )
            .collect()
    }

    /// Whether every key of `repo` was already trusted here. Removing,
    /// renaming, or reordering nodes adds no identity, so it needs no
    /// review; a new key or daemon identity does. The daemon itself refuses
    /// a request addressed to another node's name.
    #[must_use]
    pub fn covers(&self, repo: &Self) -> bool {
        repo.identities().is_subset(&self.identities())
    }
}

impl RepoTargetConfig {
    /// Whether the target lists `nodes:` rather than one host.
    #[must_use]
    pub fn is_cluster(&self) -> bool {
        !self.nodes.is_empty()
    }

    /// Whether the networks of `mm.yaml` span the target's nodes: on a
    /// cluster whose mesh mm owns. On an existing private interface, and on
    /// one host, they stay local to each host.
    #[must_use]
    pub fn spans_networks(&self) -> bool {
        self.is_cluster() && matches!(self.cluster.mesh, MeshConfig::Wireguard(_))
    }

    /// The keys a client pins on first use. A node whose keys are not
    /// recorded yet has nothing to pin; it needs `mm target trust` once
    /// `mm target setup record` adds them.
    pub fn pin(&self, name: &str) -> Result<TargetPin, String> {
        self.validate(name)?;
        if self.is_cluster() {
            return Ok(TargetPin {
                nodes: self
                    .nodes
                    .iter()
                    .filter_map(|node| {
                        Some(NodePin {
                            name: node.name.clone(),
                            host_key: node.host_key.as_deref()?.trim().to_owned(),
                            daemon_fingerprint: node.daemon_fingerprint.clone()?,
                        })
                    })
                    .collect(),
                ..TargetPin::default()
            });
        }
        Ok(TargetPin {
            host_key: self.host_key.as_deref().map(|key| key.trim().to_owned()),
            daemon_fingerprint: self.daemon_fingerprint.clone(),
            ..TargetPin::default()
        })
    }

    /// Problems with the declared facts, for `mm config validate`.
    pub fn validate(&self, name: &str) -> Result<(), String> {
        crate::domain::target::validate_target_name(name)?;
        for (provider, declared) in &self.providers {
            validate_provider_address(&declared.address).map_err(|problem| {
                format!("target '{name}' providers.{provider}.address {problem}")
            })?;
        }
        if self.is_cluster() {
            return self.validate_cluster(name);
        }
        if !self.cluster.is_default() || self.setup.is_some() {
            return Err(format!(
                "target '{name}' sets cluster or setup without nodes; list the hosts under nodes:"
            ));
        }
        let (Some(host), Some(host_key), Some(daemon_fingerprint)) =
            (&self.host, &self.host_key, &self.daemon_fingerprint)
        else {
            return Err(format!(
                "target '{name}' needs host, host-key, and daemon-fingerprint, or a nodes: list"
            ));
        };
        validate_endpoint(host, host_key, daemon_fingerprint)
            .map_err(|message| format!("target '{name}' {message}"))
    }

    /// A target definition built from the repo's facts. Runtime availability
    /// on the host is unknown to the client, so every runtime is allowed and
    /// the host decides.
    pub fn named_target(&self, project: ProjectId, name: &str) -> Result<NamedTarget, String> {
        self.validate(name)?;
        let nodes = self.target_nodes()?;
        let transport = if self.is_cluster() {
            // The first node mm can connect to stands for the target where
            // code only asks whether it is remote.
            nodes.iter().find_map(TargetNode::transport).ok_or_else(|| {
                format!(
                    "target '{name}' has no node with recorded keys yet; run `mm target setup plan`, apply it, then `mm target setup record`"
                )
            })?
        } else {
            let flat = |field: &Option<String>| field.clone().unwrap_or_default();
            let host_key = flat(&self.host_key).trim().to_owned();
            TargetTransport::Ssh {
                host: flat(&self.host),
                port: self.port,
                user: RELAY_ACCOUNT.to_owned(),
                host_key_fingerprint: ssh_fingerprint(&host_key)?,
                host_key: Some(host_key),
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: Some(flat(&self.daemon_fingerprint)),
            }
        };
        let id =
            crate::domain::host_policy::derived_id(&["repo-target", &project.0.to_string(), name]);
        Ok(NamedTarget::new(
            name.to_owned(),
            TargetDefinition {
                id: TargetId(id),
                transport,
                bootstrap: TargetBootstrap::Ansible,
                ingress: None,
                // Setup installs Docker Engine on cluster nodes, so that is
                // what a cluster runs unless the environment names a driver.
                default_runtime: self.is_cluster().then_some(RuntimeKind::DockerCompose),
                available_runtimes: vec![RuntimeKind::DockerCompose, RuntimeKind::PodmanCompose],
                runtime: None,
                nodes,
            },
        ))
    }

    /// Every node as the client addresses it, in declaration order, each
    /// with its address on the private network.
    fn target_nodes(&self) -> Result<Vec<TargetNode>, String> {
        if !self.is_cluster() {
            return Ok(Vec::new());
        }
        let addresses = self.mesh_addresses()?;
        self.nodes
            .iter()
            .zip(addresses)
            .map(|(node, private_address)| {
                let host_key = node.host_key.as_deref().map(|key| key.trim().to_owned());
                Ok(TargetNode {
                    name: node.name.clone(),
                    host: node.host.clone(),
                    port: node.port,
                    user: RELAY_ACCOUNT.to_owned(),
                    private_address: Some(private_address),
                    host_key_fingerprint: host_key.as_deref().map(ssh_fingerprint).transpose()?,
                    host_key,
                    daemon_fingerprint: node.daemon_fingerprint.clone(),
                    identity_file: None,
                })
            })
            .collect()
    }
}

pub(crate) fn validate_endpoint(
    host: &str,
    host_key: &str,
    daemon_fingerprint: &str,
) -> Result<(), String> {
    if host.trim().is_empty() || host.contains(char::is_whitespace) {
        return Err("needs a host name or address".to_owned());
    }
    ssh_fingerprint(host_key).map_err(|message| format!("host-key: {message}"))?;
    if daemon_fingerprint.len() != 64
        || !daemon_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(
            "daemon-fingerprint must be the 64-character value from `mmd --print-identity`"
                .to_owned(),
        );
    }
    Ok(())
}

/// `SHA256:<unpadded base64>` of any OpenSSH public key line, as
/// `ssh-keygen -l` prints it.
pub fn ssh_fingerprint(line: &str) -> Result<String, String> {
    use base64::Engine;
    use sha2::Digest;

    let mut parts = line.split_whitespace();
    let algorithm = parts.next().unwrap_or_default();
    let blob = parts
        .next()
        .ok_or_else(|| "expected `<type> <base64>`".to_owned())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(blob)
        .map_err(|_| "public key is not valid base64".to_owned())?;
    let declared = bytes
        .get(..4)
        .map(|length| u32::from_be_bytes([length[0], length[1], length[2], length[3]]) as usize)
        .and_then(|length| bytes.get(4..4 + length))
        .ok_or_else(|| "public key blob is truncated".to_owned())?;
    if declared != algorithm.as_bytes() {
        return Err("public key blob does not match its key type".to_owned());
    }
    Ok(format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(sha2::Sha256::digest(&bytes))
    ))
}

#[cfg(test)]
#[path = "repo_target/tests.rs"]
mod tests;
