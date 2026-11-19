//! The cluster form of a repo target: nodes that `mm target setup` prepares
//! and connects over a private network.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{RepoTargetConfig, default_port, is_default_port, ssh_fingerprint, validate_endpoint};
use crate::domain::placement::ALL_NODES;

#[path = "cluster/mesh.rs"]
mod mesh;
pub use mesh::{MESH_INTERFACE, MeshConfig, WireguardMesh};

/// One host of a cluster target. `mm target setup record` fills `host-key`,
/// `daemon-fingerprint`, and `private-address`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct RepoTargetNode {
    /// The node's name and hostname: a lowercase hostname label.
    pub name: String,
    /// The public address or DNS name mm connects to.
    pub host: String,
    #[serde(default = "default_port", skip_serializing_if = "is_default_port")]
    pub port: u16,
    /// The address the other nodes send WireGuard packets to, when that is
    /// not `host`: for example an address on a network the nodes share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The node's address on the private network. Unset, mm assigns the
    /// lowest free address of the mesh subnet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_address: Option<Ipv4Addr>,
    /// The host's SSH public key, `<type> <base64>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key: Option<String>,
    /// Printed by `mmd --print-identity` on the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_fingerprint: Option<String>,
}

/// How the nodes of a cluster target work together. Every key has a default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct RepoClusterConfig {
    /// The private network between the nodes: a WireGuard mesh mm creates
    /// (`subnet`, `port`, `mtu`) or an existing `interface`.
    #[serde(default, skip_serializing_if = "MeshConfig::is_default")]
    pub mesh: MeshConfig,
    /// Address space for networks that span the nodes: a private range, a
    /// /20 or larger, that no node uses for anything else. Each network
    /// takes a /20 of it (16 nodes, room for 253 containers on each), so
    /// the default holds 256 networks.
    #[serde(
        default = "default_networks",
        skip_serializing_if = "is_default_networks"
    )]
    pub networks: Ipv4Cidr,
    /// The node that serves shared volumes. Unset means the first node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Names for sets of nodes, for a service's `placement`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<String, Vec<String>>,
    /// The nodes that run the ingress proxy and take the traffic of every
    /// `ingress:` route, written like a `placement`: a node, a group,
    /// `all`, or a list. Unset means the first node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<crate::domain::placement::PlacementSpec>,
}

impl Default for RepoClusterConfig {
    fn default() -> Self {
        Self {
            mesh: MeshConfig::default(),
            networks: default_networks(),
            storage: None,
            groups: BTreeMap::new(),
            ingress: None,
        }
    }
}

impl RepoClusterConfig {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Networks that span nodes take their addresses from here: 256 networks.
/// The range is clear of the default mesh subnet (10.90.0.0/24), of Docker's
/// default pools (172.17.0.0/16 to 172.31.0.0/16, 192.168.0.0/16), and of
/// Podman's (10.88.0.0/16, then 10.89.0.0/24 upwards).
pub const DEFAULT_CLUSTER_NETWORKS: Ipv4Cidr = Ipv4Cidr {
    address: Ipv4Addr::new(10, 64, 0, 0),
    prefix: 12,
};

const fn default_networks() -> Ipv4Cidr {
    DEFAULT_CLUSTER_NETWORKS
}

/// serde's `skip_serializing_if` passes a reference.
fn is_default_networks(networks: &Ipv4Cidr) -> bool {
    *networks == DEFAULT_CLUSTER_NETWORKS
}

/// What setup installs on every node of a cluster target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct RepoTargetSetup {
    /// The account setup connects as; the operator's mm key is authorized for it.
    #[serde(default = "default_setup_user")]
    pub user: String,
    /// Git URL every node fetches the project from. Left out with
    /// `delivery: push`, where the nodes fetch nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    /// The git server's SSH host key, `<type> <base64>`; required for SSH origins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_host_key: Option<String>,
    /// The environment's read-only deploy key, shared by every node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_key: Option<RepoDeployKey>,
    /// Recovery keys that are admin on every node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub break_glass: Vec<RepoBreakGlassKey>,
}

fn default_setup_user() -> String {
    "root".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct RepoDeployKey {
    /// The age secret holding the private key (`mm target setup deploy-key`
    /// creates it).
    pub secret: String,
    /// Its public half, as added to the git host.
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct RepoBreakGlassKey {
    pub name: String,
    /// `ssh-ed25519 AAAA… comment`; keep the private half offline.
    pub key: String,
}

/// An IPv4 network in CIDR notation, such as `10.90.0.0/24`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ipv4Cidr {
    pub address: Ipv4Addr,
    pub prefix: u8,
}

impl Ipv4Cidr {
    fn mask(self) -> u32 {
        u32::MAX
            .checked_shl(32 - u32::from(self.prefix))
            .unwrap_or(0)
    }

    #[must_use]
    pub fn contains(self, address: Ipv4Addr) -> bool {
        u32::from(address) & self.mask() == u32::from(self.address)
    }

    /// The addresses a host can hold, lowest first: all but the network and
    /// broadcast addresses.
    pub fn hosts(self) -> impl Iterator<Item = Ipv4Addr> {
        let network = u32::from(self.address);
        let broadcast = network | !self.mask();
        (network.saturating_add(1)..broadcast).map(Ipv4Addr::from)
    }

    /// Whether a host can hold `address` in this network.
    #[must_use]
    pub fn is_host(self, address: Ipv4Addr) -> bool {
        let network = u32::from(self.address);
        let address = u32::from(address);
        self.contains(Ipv4Addr::from(address))
            && address != network
            && address != (network | !self.mask())
    }

    /// The last address of the range: a network's broadcast address.
    #[must_use]
    pub fn last_address(self) -> Ipv4Addr {
        Ipv4Addr::from(u32::from(self.address) | !self.mask())
    }

    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        let mask = self.mask() & other.mask();
        u32::from(self.address) & mask == u32::from(other.address) & mask
    }
}

impl fmt::Display for Ipv4Cidr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.address, self.prefix)
    }
}

impl FromStr for Ipv4Cidr {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (address, prefix) = value
            .split_once('/')
            .ok_or_else(|| format!("'{value}' is not CIDR notation like 10.90.0.0/24"))?;
        let address = address
            .parse::<Ipv4Addr>()
            .map_err(|_| format!("'{value}' does not start with an IPv4 address"))?;
        let prefix = prefix
            .parse::<u8>()
            .ok()
            .filter(|prefix| *prefix <= 32)
            .ok_or_else(|| format!("'{value}' needs a prefix length from 0 to 32"))?;
        let cidr = Self { address, prefix };
        if u32::from(address) & cidr.mask() != u32::from(address) {
            return Err(format!(
                "'{value}' has host bits set; write the network address"
            ));
        }
        Ok(cidr)
    }
}

impl Serialize for Ipv4Cidr {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Ipv4Cidr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Ipv4Cidr {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Ipv4Cidr".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "An IPv4 network in CIDR notation, such as 10.90.0.0/24.",
            "pattern": r"^\d{1,3}(\.\d{1,3}){3}/\d{1,2}$"
        })
    }
}

impl RepoTargetConfig {
    /// The node setup starts with. The first node in declaration order.
    #[must_use]
    pub fn anchor_node(&self) -> Option<&RepoTargetNode> {
        self.nodes.first()
    }

    /// The node that serves shared volumes: `cluster.storage`, or the first
    /// node.
    #[must_use]
    pub fn storage_node(&self) -> Option<&str> {
        self.cluster
            .storage
            .as_deref()
            .or_else(|| self.nodes.first().map(|node| node.name.as_str()))
    }

    /// The nodes that run the ingress proxy, in declaration order:
    /// `cluster.ingress`, or the first node.
    pub fn ingress_nodes(&self) -> Result<Vec<String>, String> {
        let names = self
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        match &self.cluster.ingress {
            Some(ingress) => ingress
                .resolve(&names, &self.cluster.groups)
                .map_err(|problem| format!("cluster.ingress {problem}")),
            None => Ok(names
                .first()
                .map(|first| vec![(*first).to_owned()])
                .unwrap_or_default()),
        }
    }

    /// Every node's address on the private network, in declaration order. A
    /// declared `private-address` is kept; every other node takes the lowest
    /// address of the mesh subnet that no node holds yet.
    pub fn mesh_addresses(&self) -> Result<Vec<Ipv4Addr>, String> {
        let declared = self
            .nodes
            .iter()
            .filter_map(|node| node.private_address)
            .collect::<BTreeSet<_>>();
        let wireguard = match &self.cluster.mesh {
            MeshConfig::Wireguard(wireguard) => wireguard,
            MeshConfig::Interface(interface) => {
                return self
                    .nodes
                    .iter()
                    .map(|node| {
                        node.private_address.ok_or_else(|| {
                            format!(
                                "node {} needs private-address: its address on {interface}; mm assigns addresses only on a WireGuard mesh it creates",
                                node.name
                            )
                        })
                    })
                    .collect();
            }
        };
        let mut free = wireguard
            .subnet
            .hosts()
            .filter(|address| !declared.contains(address));
        self.nodes
            .iter()
            .map(|node| match node.private_address {
                Some(address) => Ok(address),
                None => free.next().ok_or_else(|| {
                    format!(
                        "mesh subnet {} has no free address for node {}; set cluster.mesh.subnet to a larger range",
                        wireguard.subnet, node.name
                    )
                }),
            })
            .collect()
    }

    /// Every node as the mesh sees it, in declaration order: its mesh
    /// address and the address its peers send to. Keys are not known here.
    pub fn mesh_members(&self) -> Result<Vec<crate::domain::mesh::MeshMember>, String> {
        Ok(self
            .nodes
            .iter()
            .zip(self.mesh_addresses()?)
            .map(|(node, address)| crate::domain::mesh::MeshMember {
                name: node.name.clone(),
                address,
                endpoint: node.endpoint.clone().unwrap_or_else(|| node.host.clone()),
                public_key: None,
                routed: Vec::new(),
            })
            .collect())
    }

    pub(super) fn validate_cluster(&self, name: &str) -> Result<(), String> {
        self.cluster_problem()
            .map_err(|message| format!("target '{name}' {message}"))
    }

    fn cluster_problem(&self) -> Result<(), String> {
        if self.host.is_some() || self.host_key.is_some() || self.daemon_fingerprint.is_some() {
            return Err(
                "mixes nodes: with host/host-key/daemon-fingerprint; a target is either one host or a list of nodes"
                    .to_owned(),
            );
        }
        let cluster = &self.cluster;
        mesh::mesh_problem(&cluster.mesh)?;
        if !is_private(cluster.networks.address) {
            return Err(format!(
                "cluster.networks {} is not a private range",
                cluster.networks
            ));
        }
        crate::domain::cluster_network::space_problem(cluster.networks)?;
        if let MeshConfig::Wireguard(wireguard) = &cluster.mesh
            && wireguard.subnet.overlaps(cluster.networks)
        {
            return Err(format!(
                "cluster.mesh.subnet {} overlaps cluster.networks {}; choose ranges that do not overlap",
                wireguard.subnet, cluster.networks
            ));
        }
        let names = self.node_problems()?;
        self.address_problems()?;
        reference_problems(cluster, &names)?;
        self.ingress_nodes()?;
        if let Some(setup) = &self.setup {
            validate_setup(setup, self.delivery).map_err(|message| format!("setup {message}"))?;
        }
        Ok(())
    }

    /// Checks every node's name, endpoint, and keys, and returns the names.
    fn node_problems(&self) -> Result<BTreeSet<&str>, String> {
        let mut names = BTreeSet::new();
        let mut endpoints = BTreeSet::new();
        for node in &self.nodes {
            validate_node_name(&node.name)?;
            let label = format!("node {}", node.name);
            if !names.insert(node.name.as_str()) {
                return Err(format!("repeats {label}"));
            }
            if node.host.trim().is_empty() || node.host.contains(char::is_whitespace) {
                return Err(format!("{label} needs a host name or address"));
            }
            if !endpoints.insert((node.host.as_str(), node.port)) {
                return Err(format!(
                    "{label} repeats {}:{}; list each host once",
                    node.host, node.port
                ));
            }
            mesh::endpoint_problem(&self.cluster.mesh, node.endpoint.as_deref())
                .map_err(|message| format!("{label} {message}"))?;
            match (&node.host_key, &node.daemon_fingerprint) {
                (Some(host_key), Some(daemon_fingerprint)) => {
                    validate_endpoint(&node.host, host_key, daemon_fingerprint)
                        .map_err(|message| format!("{label} {message}"))?;
                }
                (None, None) => {}
                _ => {
                    return Err(format!(
                        "{label} needs both host-key and daemon-fingerprint, or neither before setup"
                    ));
                }
            }
        }
        Ok(names)
    }

    /// Checks the declared private addresses and that every node gets one.
    fn address_problems(&self) -> Result<(), String> {
        let mut addresses = BTreeSet::new();
        for node in &self.nodes {
            let Some(address) = node.private_address else {
                continue;
            };
            let label = format!("node {}", node.name);
            if !is_private(address) {
                return Err(format!(
                    "{label} private-address {address} is not a private address"
                ));
            }
            if !addresses.insert(address) {
                return Err(format!("{label} repeats private-address {address}"));
            }
            if let MeshConfig::Wireguard(wireguard) = &self.cluster.mesh
                && !wireguard.subnet.is_host(address)
            {
                return Err(format!(
                    "{label} private-address {address} is not a host address of the mesh subnet {}; remove it to let mm assign one",
                    wireguard.subnet
                ));
            }
            if self.cluster.networks.contains(address) {
                return Err(format!(
                    "{label} private-address {address} is inside cluster.networks {}; choose ranges that do not overlap",
                    self.cluster.networks
                ));
            }
        }
        self.mesh_addresses().map(drop)
    }
}

/// Storage and groups name declared nodes; group names stay distinct from
/// node names and from `all`, so a placement word means one thing.
fn reference_problems(cluster: &RepoClusterConfig, names: &BTreeSet<&str>) -> Result<(), String> {
    let known = |node: &str, field: &str| {
        if names.contains(node) {
            Ok(())
        } else {
            Err(format!(
                "cluster.{field} names unknown node '{node}'; nodes: {}",
                names.iter().copied().collect::<Vec<_>>().join(", ")
            ))
        }
    };
    if let Some(storage) = &cluster.storage {
        known(storage, "storage")?;
    }
    for (group, members) in &cluster.groups {
        crate::domain::target::validate_target_name(group).map_err(|_| {
            format!("cluster.groups name '{group}' must use lowercase letters, digits, and hyphens")
        })?;
        if group == ALL_NODES {
            return Err(format!(
                "cluster.groups cannot define '{ALL_NODES}'; placement: {ALL_NODES} already means every node"
            ));
        }
        if names.contains(group.as_str()) {
            return Err(format!(
                "cluster.groups name '{group}' is also a node name; rename the group"
            ));
        }
        if members.is_empty() {
            return Err(format!("cluster.groups '{group}' lists no nodes"));
        }
        for member in members {
            known(member, &format!("groups '{group}'"))?;
        }
    }
    Ok(())
}

fn validate_setup(
    setup: &RepoTargetSetup,
    delivery: crate::domain::delivery::Delivery,
) -> Result<(), String> {
    if setup.user.is_empty()
        || setup.user.len() > 32
        || !setup
            .user
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!("user '{}' is not a valid account name", setup.user));
    }
    if delivery.is_push() {
        // The nodes fetch nothing: a key or URL here would be a second,
        // unused way to say where the files come from.
        let unused = [
            (!setup.origin.is_empty(), "origin"),
            (setup.origin_host_key.is_some(), "origin-host-key"),
            (setup.deploy_key.is_some(), "deploy-key"),
        ];
        if let Some((_, key)) = unused.iter().find(|(set, _)| *set) {
            return Err(format!(
                "{key} is not used with `delivery: push`, where mm sends the files and the nodes fetch nothing; remove it"
            ));
        }
    } else if setup.origin.trim().is_empty() || setup.origin.contains(char::is_whitespace) {
        return Err(
            "origin needs a git URL (or set `delivery: push` on the target to send the files instead)"
                .to_owned(),
        );
    }
    if let Some(key) = &setup.origin_host_key {
        ssh_fingerprint(key).map_err(|message| format!("origin-host-key: {message}"))?;
    }
    if let Some(deploy_key) = &setup.deploy_key {
        validate_secret_name(&deploy_key.secret)
            .map_err(|message| format!("deploy-key secret {message}"))?;
        ssh_fingerprint(&deploy_key.public_key)
            .map_err(|message| format!("deploy-key public-key: {message}"))?;
    }
    let mut names = BTreeSet::new();
    for entry in &setup.break_glass {
        crate::domain::host_policy::parse_break_glass(&format!("{}={}", entry.name, entry.key))?;
        if !names.insert(entry.name.as_str()) {
            return Err(format!("break-glass repeats '{}'", entry.name));
        }
    }
    Ok(())
}

/// Secret names are lowercase dotted words, such as `origin.deploy-key`.
fn validate_secret_name(name: &str) -> Result<(), String> {
    if !name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
    {
        Ok(())
    } else {
        Err(format!("'{name}' is not a valid secret name"))
    }
}

/// Node names become hostnames.
fn validate_node_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if !valid {
        return Err(format!(
            "node name '{name}' must be a lowercase hostname label (letters, digits, hyphens)"
        ));
    }
    if name == ALL_NODES {
        return Err(format!(
            "node name '{ALL_NODES}' is taken: placement: {ALL_NODES} means every node; rename the node"
        ));
    }
    Ok(())
}

/// RFC 1918 and the shared address space (RFC 6598) carriers use for private links.
pub(super) fn is_private(address: Ipv4Addr) -> bool {
    address.is_private()
        || "100.64.0.0/10"
            .parse::<Ipv4Cidr>()
            .is_ok_and(|shared| shared.contains(address))
}
