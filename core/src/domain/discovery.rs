//! Service names across the nodes of a cluster target.
//!
//! Every node's `mmd` answers the name lookups of the containers on its own
//! node. It knows the containers of its engine by looking at them, and the
//! containers of the other nodes because the nodes tell each other, host to
//! host over the mesh, what they run. Nothing is declared: a container that
//! starts, stops, or changes health changes the answers.
//!
//! A container gets an answer for the networks it is on and no other:
//!
//! - `<service>`: the service's healthy containers on the asking
//!   container's own node, and when that node has none, the healthy
//!   containers on every node ([`Reach::LocalFirst`]);
//! - `tasks.<service>`: every healthy container of the service, on every
//!   node ([`Reach::EveryNode`]);
//! - `<service>.<node>`: the service's healthy container on that node;
//! - a network alias: like `<service>`, for the containers that carry it;
//! - a container's own name or short id, on the container's node;
//! - [`NODE_NAME`]: the asking container's own node, at its address on
//!   the network the container asks from.
//!
//! Each node answers its own containers, so the answer for `<service>`
//! differs between nodes on purpose: a lookup stays on the node when it
//! can. Record order would not do that, because resolvers and C libraries
//! sort what they get.
//!
//! Healthy means running and, for a container with a health check, passing
//! it. A name that matches a container but no healthy one has no address,
//! except for the container itself, which always finds its own; a name
//! that matches nothing is the host's resolvers' to answer.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::cluster_network::NodeNetworks;

#[path = "discovery/report.rs"]
mod report;
#[path = "discovery/wire.rs"]
pub mod wire;

pub use report::{
    NameAddress, NameProblem, NameReport, NodeLookup, ResolvedName, fail_open_diagnostics,
};

/// The port the node's resolver answers on, on the gateway address of each
/// cluster network of the node.
pub const RESOLVER_PORT: u16 = 53;
/// The TCP port, on the node's mesh address, where a node tells the other
/// nodes what it runs.
pub const EXCHANGE_PORT: u16 = 7953;
/// The name a container finds its own node by.
pub const NODE_NAME: &str = "mmd.internal";
/// How often a node asks each other node what it runs.
pub const EXCHANGE_INTERVAL_MS: u64 = 2_000;
/// How long a node keeps answering with another node's containers after it
/// last heard from that node.
pub const PEER_EXPIRY_MS: u64 = 10_000;
/// How long a client may keep an answer, in seconds.
pub const NAME_TIME_TO_LIVE: u32 = 5;
/// The largest record list a node accepts from another node.
pub const EXCHANGE_LIMIT_BYTES: u64 = 4 * 1024 * 1024;

/// The name of the file, in the node's resolver directory, that the
/// containers of the cluster network `key` get as `/etc/resolv.conf`.
#[must_use]
pub fn resolver_file_name(key: &str) -> String {
    format!("{key}.conf")
}

/// What that file holds: the node's resolver on the network's gateway
/// address, the search domains of the host's own `resolv_conf`, and names
/// looked up as they are written, as on any network of the engine.
#[must_use]
pub fn resolver_file(gateway: Ipv4Addr, host_resolv_conf: &str) -> String {
    let search = host_resolv_conf
        .lines()
        .map(str::split_whitespace)
        .filter_map(|mut words| (words.next() == Some("search")).then_some(words))
        .flatten()
        .filter(|domain| *domain != ".")
        .collect::<Vec<_>>();
    let mut file = format!(
        "# Written by mmd: this node answers the names of the cluster's services\n# and passes every other name to the host's resolvers.\nnameserver {gateway}\n"
    );
    if !search.is_empty() {
        file.push_str(&format!("search {}\n", search.join(" ")));
    }
    file.push_str("options ndots:0\n");
    file
}

const TASKS_PREFIX: &str = "tasks.";
/// Marks a network that stays on its node. A cluster network's name never
/// contains a slash.
const HOST_NETWORK_PREFIX: &str = "host/";

/// Whether a container takes part in the answers for its names.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Health {
    /// Running, and passing its health check when it has one.
    Healthy,
    /// Running; its health check has not passed yet.
    Starting,
    /// Running and failing its health check.
    Unhealthy,
    /// Not running.
    Stopped,
}

impl Health {
    /// From the engine's container state (`running`, `exited`, …) and, for
    /// a container with a health check, its health (`healthy`, …).
    #[must_use]
    pub fn from_engine(state: &str, health: Option<&str>) -> Self {
        if !state.eq_ignore_ascii_case("running") {
            return Self::Stopped;
        }
        match health.map(str::to_ascii_lowercase).as_deref() {
            None | Some("" | "none" | "healthy") => Self::Healthy,
            Some("starting") => Self::Starting,
            Some(_) => Self::Unhealthy,
        }
    }

    /// Whether the container's address is given out while the service
    /// has a healthy container.
    #[must_use]
    pub const fn answers(self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Whether the container runs, healthy or not: what is answered when
    /// none of its service's containers is healthy (fail open).
    #[must_use]
    pub const fn running(self) -> bool {
        !matches!(self, Self::Stopped)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Starting => "starting",
            Self::Unhealthy => "unhealthy",
            Self::Stopped => "stopped",
        }
    }
}

/// A container on one of the engine's networks, in the engine's terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The network's name on the engine.
    pub network: String,
    pub address: Ipv4Addr,
    /// Every name the engine resolves to the container on this network.
    pub names: Vec<String>,
}

/// A container as its node's engine reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedContainer {
    pub id: String,
    pub name: String,
    /// The Compose service the container belongs to; `None` for a job and
    /// for a container Compose did not start.
    pub service: Option<String>,
    pub health: Health,
    pub attachments: Vec<Attachment>,
}

/// One address of a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Endpoint {
    /// The network's name across the cluster.
    pub network: String,
    pub address: Ipv4Addr,
    /// The names the container has on the network besides its service's
    /// and its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

/// A container with its addresses on the networks of its node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ContainerRecord {
    pub name: String,
    /// The first twelve characters of the engine's id, which the engine
    /// resolves like a name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    pub health: Health,
    pub endpoints: Vec<Endpoint>,
    /// Its node stopped restarting it at its service's restart limit
    /// (`domain::healing`): it is answered only when its service has no
    /// other container to answer with.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restart_limit: bool,
}

impl ContainerRecord {
    /// Whether the container is answered while its service has another.
    #[must_use]
    pub const fn answers(&self) -> bool {
        self.health.answers() && !self.restart_limit
    }
}

/// What one node runs. The nodes send each other this.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeRecords {
    pub node: String,
    #[serde(default)]
    pub containers: Vec<ContainerRecord>,
    /// The ids (first twelve characters) of this node's containers that
    /// are leaving (`domain::leaving`): they are not in `containers`, and
    /// every other node is to confirm that it dropped them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leaving: Vec<String>,
    /// The leaving containers of the other nodes that this node no longer
    /// answers names with and whose ingress proxy, if it runs one, no
    /// longer sends them requests. `None` from a node too old to say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<Vec<String>>,
    /// The node's scaled services (`domain::autoscale`): what its own
    /// containers of each use and how many run, and the count it goes by.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scaling: Vec<crate::domain::autoscale::ScaleReport>,
}

impl NodeRecords {
    /// What `node` runs, from what its engine reports. A network the node
    /// holds a slice of gets its name across the cluster; every other
    /// network of the engine stays a network of this host.
    #[must_use]
    pub fn observed(networks: &NodeNetworks, observed: Vec<ObservedContainer>) -> Self {
        let local = networks.local();
        let containers = observed
            .into_iter()
            .map(|container| {
                let id = container.id.chars().take(12).collect::<String>();
                let endpoints = container
                    .attachments
                    .into_iter()
                    .map(|attachment| Endpoint {
                        network: local
                            .iter()
                            .find(|network| network.network == attachment.network)
                            .map_or_else(
                                || format!("{HOST_NETWORK_PREFIX}{}", attachment.network),
                                |network| network.key.clone(),
                            ),
                        address: attachment.address,
                        aliases: attachment
                            .names
                            .into_iter()
                            .filter(|name| {
                                !same(name, &container.name)
                                    && !same(name, &id)
                                    && !container
                                        .service
                                        .as_deref()
                                        .is_some_and(|service| same(name, service))
                            })
                            .collect(),
                    })
                    .collect();
                ContainerRecord {
                    name: container.name,
                    id,
                    service: container.service,
                    health: container.health,
                    endpoints,
                    restart_limit: false,
                }
            })
            .filter(|container: &ContainerRecord| !container.endpoints.is_empty())
            .collect();
        Self {
            node: networks.node.clone(),
            containers,
            ..Self::default()
        }
    }

    /// Marks the containers whose engine ids start with one of `held` as
    /// past their restart limit.
    pub fn hold(&mut self, held: &BTreeSet<String>) {
        for container in &mut self.containers {
            container.restart_limit = !container.id.is_empty()
                && held.iter().any(|id| id.starts_with(container.id.as_str()));
        }
    }

    /// The part the other nodes get: the addresses on cluster networks.
    #[must_use]
    pub fn for_peers(&self) -> Self {
        Self {
            node: self.node.clone(),
            leaving: self.leaving.clone(),
            confirmed: self.confirmed.clone(),
            scaling: self.scaling.clone(),
            containers: self
                .containers
                .iter()
                .filter_map(|container| {
                    let endpoints = container
                        .endpoints
                        .iter()
                        .filter(|endpoint| !endpoint.network.starts_with(HOST_NETWORK_PREFIX))
                        .cloned()
                        .collect::<Vec<_>>();
                    (!endpoints.is_empty()).then(|| ContainerRecord {
                        endpoints,
                        ..container.clone()
                    })
                })
                .collect(),
        }
    }

    /// What this node takes from the records the node `peer` sent: only
    /// addresses inside the slices that `networks` gives that node. A node
    /// can therefore never put an address of another node, or of anything
    /// else, behind a name. `None` when the records name another node.
    #[must_use]
    pub fn accepted_from(self, peer: &str, networks: &NodeNetworks) -> Option<Self> {
        if self.node != peer {
            return None;
        }
        let Self {
            node,
            containers,
            leaving,
            confirmed,
            scaling,
        } = self;
        let containers = containers
            .into_iter()
            .filter_map(|mut container| {
                container.endpoints.retain(|endpoint| {
                    networks
                        .table
                        .networks
                        .get(&endpoint.network)
                        .and_then(|network| network.nodes.get(peer))
                        .is_some_and(|held| held.slice.contains(endpoint.address))
                });
                (!container.endpoints.is_empty()).then_some(container)
            })
            .collect();
        Some(Self {
            node,
            containers,
            leaving,
            confirmed,
            scaling,
        })
    }
}

fn same(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

/// Which of a service's healthy containers a node answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The ones on the asking node, and when it has none, the ones on
    /// every node: `<service>`.
    LocalFirst,
    /// The ones on every node: `tasks.<service>`.
    EveryNode,
}

/// The healthy addresses behind a name, by where they run.
#[derive(Debug, Default)]
struct Found {
    here: BTreeSet<Ipv4Addr>,
    elsewhere: BTreeSet<Ipv4Addr>,
}

impl Found {
    fn add(&mut self, own: bool, address: Ipv4Addr) {
        if own {
            self.here.insert(address);
        } else {
            self.elsewhere.insert(address);
        }
    }

    /// The addresses to answer with, and whether they are this node's
    /// own because it has some.
    fn reach(mut self, reach: Reach) -> (BTreeSet<Ipv4Addr>, bool) {
        match reach {
            Reach::LocalFirst if !self.here.is_empty() => (self.here, true),
            Reach::LocalFirst => (self.elsewhere, false),
            Reach::EveryNode => {
                self.here.append(&mut self.elsewhere);
                (self.here, false)
            }
        }
    }
}

/// What a node does with a name lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The name is a container's on a network of the asker: these are the
    /// addresses of the healthy ones, possibly none.
    Addresses(Vec<Ipv4Addr>),
    /// Not a name of the cluster: the host's resolvers answer.
    Elsewhere,
    /// The lookup does not come from a container of this node.
    NotAsked,
}

/// Everything a node knows about who runs what: its own containers and the
/// records of the nodes it heard from recently.
#[derive(Debug, Clone, Copy)]
pub struct ServiceMap<'a> {
    pub networks: &'a NodeNetworks,
    pub local: &'a NodeRecords,
    pub peers: &'a [NodeRecords],
}

impl ServiceMap<'_> {
    /// The container of this node that holds the address `source`.
    fn asker(&self, source: Ipv4Addr) -> Option<&ContainerRecord> {
        self.local.containers.iter().find(|container| {
            container
                .endpoints
                .iter()
                .any(|endpoint| endpoint.address == source)
        })
    }

    /// The networks a lookup from `source` is answered for: every network
    /// of the local container with that address, or, for a container the
    /// node has not seen yet, the cluster network whose slice holds the
    /// address. `None` for an address that is no container of this node.
    fn networks_of(&self, source: Ipv4Addr) -> Option<BTreeSet<&str>> {
        if let Some(asker) = self.asker(source) {
            return Some(
                asker
                    .endpoints
                    .iter()
                    .map(|endpoint| endpoint.network.as_str())
                    .collect(),
            );
        }
        self.networks
            .table
            .networks
            .iter()
            .find(|(_, network)| {
                network
                    .nodes
                    .get(&self.networks.node)
                    .is_some_and(|held| held.slice.contains(source))
            })
            .map(|(key, _)| BTreeSet::from([key.as_str()]))
    }

    fn nodes(&self) -> impl Iterator<Item = (&NodeRecords, bool)> {
        std::iter::once((self.local, true)).chain(self.peers.iter().map(|peer| (peer, false)))
    }

    /// Whether `container` is answered on `network`: when healthy and not
    /// past its restart limit, and when it runs and none of its service's
    /// containers on the network is answered so on any node (fail open: a
    /// health check that fails everywhere at once is more often the check
    /// than the service). A container that leaves is in no record, and one
    /// that is stopped is never answered.
    fn answered(&self, container: &ContainerRecord, network: &str) -> bool {
        if container.answers() {
            return true;
        }
        let Some(service) = container.service.as_deref() else {
            return false;
        };
        container.health.running() && !self.healthy_anywhere(service, network)
    }

    /// Whether `service` has a healthy container on `network` on any node
    /// this node knows of.
    fn healthy_anywhere(&self, service: &str, network: &str) -> bool {
        self.nodes()
            .flat_map(|(records, _)| &records.containers)
            .filter(|container| {
                container.answers()
                    && container
                        .service
                        .as_deref()
                        .is_some_and(|named| same(named, service))
            })
            .flat_map(|container| &container.endpoints)
            .any(|endpoint| endpoint.network == network)
    }

    /// Answers the lookup of `name` by the container with address `source`.
    #[must_use]
    pub fn resolve(&self, source: Ipv4Addr, name: &str) -> Resolution {
        let Some(asked) = self.networks_of(source) else {
            return Resolution::NotAsked;
        };
        let asker = self.asker(source);
        let name = name.trim_end_matches('.');
        if same(name, NODE_NAME) {
            return Resolution::Addresses(
                self.networks
                    .local()
                    .into_iter()
                    .filter(|network| network.slice.contains(source))
                    .map(|network| network.gateway)
                    .collect(),
            );
        }
        // Names compare without case, the prefix too.
        let service = name
            .split_at_checked(TASKS_PREFIX.len())
            .filter(|(prefix, _)| same(prefix, TASKS_PREFIX))
            .map(|(_, service)| service);
        // `<service>.<node>`, unless the whole name is itself a service.
        let on_node = name.rsplit_once('.').filter(|_| service.is_none());
        let mut known = false;
        // A node's name, a container's own name, and `tasks.` say where;
        // a service name or an alias alone stays on this node when it can.
        let mut exact = Found::default();
        let mut steered = Found::default();
        for (records, own) in self.nodes() {
            for container in &records.containers {
                let named = container.service.as_deref();
                let by_service = named.is_some_and(|named| same(named, service.unwrap_or(name)));
                let by_node = on_node.is_some_and(|(wanted, node)| {
                    same(node, &records.node) && named.is_some_and(|named| same(named, wanted))
                });
                let by_own_name = service.is_none()
                    && own
                    && (same(&container.name, name) || same(&container.id, name));
                for endpoint in &container.endpoints {
                    if !asked.contains(endpoint.network.as_str()) {
                        continue;
                    }
                    let by_alias =
                        service.is_none() && endpoint.aliases.iter().any(|alias| same(alias, name));
                    if !(by_service || by_node || by_own_name || by_alias) {
                        continue;
                    }
                    known = true;
                    // A container always finds itself, healthy or not:
                    // its health check may be the one asking.
                    let itself = asker.is_some_and(|asker| std::ptr::eq(asker, container));
                    if !(self.answered(container, &endpoint.network) || itself) {
                        continue;
                    }
                    if by_node || by_own_name || service.is_some() {
                        exact.add(own, endpoint.address);
                    } else {
                        steered.add(own, endpoint.address);
                    }
                }
            }
        }
        if !known {
            return Resolution::Elsewhere;
        }
        let (mut addresses, _) = exact.reach(Reach::EveryNode);
        addresses.append(&mut steered.reach(Reach::LocalFirst).0);
        Resolution::Addresses(addresses.into_iter().collect())
    }

    /// The addresses of `service`'s healthy containers on the cluster
    /// network `network` (its name across the cluster), as this node
    /// answers with them, and whether they are the node's own because it
    /// has some. An ingress proxy on the node asks the same question:
    /// [`Reach::LocalFirst`] for the backends next to it, and
    /// [`Reach::EveryNode`] for all of them.
    #[must_use]
    pub fn service_addresses(
        &self,
        network: &str,
        service: &str,
        reach: Reach,
    ) -> (Vec<Ipv4Addr>, bool) {
        let mut found = Found::default();
        for (records, own) in self.nodes() {
            let containers = records.containers.iter().filter(|container| {
                container
                    .service
                    .as_deref()
                    .is_some_and(|named| same(named, service))
            });
            for container in containers {
                for endpoint in &container.endpoints {
                    if endpoint.network == network && self.answered(container, network) {
                        found.add(own, endpoint.address);
                    }
                }
            }
        }
        let (addresses, local) = found.reach(reach);
        (addresses.into_iter().collect(), local)
    }

    /// What this node's resolver knows about the networks whose name
    /// across the cluster starts with `prefix`: the ones the node holds a
    /// part of, and what every service name on them resolves to.
    #[must_use]
    pub fn report(&self, prefix: &str) -> NameReport {
        NameReport {
            networks: self
                .networks
                .local()
                .iter()
                .filter_map(|network| network.key.strip_prefix(prefix).map(str::to_owned))
                .collect(),
            names: self.names(prefix),
            problems: Vec::new(),
        }
    }

    fn names(&self, prefix: &str) -> Vec<ResolvedName> {
        let mut names = Vec::<ResolvedName>::new();
        let held = self.networks.local();
        for (records, _) in self.nodes() {
            for container in &records.containers {
                let Some(service) = container.service.as_deref() else {
                    continue;
                };
                for endpoint in &container.endpoints {
                    let Some(network) = endpoint.network.strip_prefix(prefix) else {
                        continue;
                    };
                    let address = NameAddress {
                        address: endpoint.address,
                        node: records.node.clone(),
                        health: container.health,
                        unknown_to: Vec::new(),
                        restart_limit: container.restart_limit,
                    };
                    match names
                        .iter_mut()
                        .find(|known| known.name == service && known.network == network)
                    {
                        Some(known) => known.addresses.push(address),
                        None => names.push(ResolvedName {
                            name: service.to_owned(),
                            network: network.to_owned(),
                            addresses: vec![address],
                            fail_open: false,
                            // What this node's own containers get, when
                            // the node holds a part of the network.
                            lookups: held
                                .iter()
                                .filter(|local| local.key == endpoint.network)
                                .map(|local| {
                                    let (addresses, own) = self.service_addresses(
                                        &local.key,
                                        service,
                                        Reach::LocalFirst,
                                    );
                                    NodeLookup {
                                        node: self.networks.node.clone(),
                                        addresses,
                                        local: own,
                                    }
                                })
                                .collect(),
                        }),
                    }
                }
            }
        }
        for name in &mut names {
            name.fail_open = name.fails_open();
        }
        report::sorted(&mut names);
        names
    }
}
