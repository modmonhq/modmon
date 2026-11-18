//! What the service names of an environment resolve to right now, as
//! `mm service status` shows it: each node's resolver says what it knows,
//! and the nodes' views are put next to each other.

use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Health;

/// One container behind a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct NameAddress {
    pub address: Ipv4Addr,
    /// The node the container runs on.
    pub node: String,
    /// Only a healthy container's address is given out.
    pub health: Health,
    /// Nodes whose resolver does not know this container, so lookups on
    /// those nodes miss it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_to: Vec<String>,
    /// Its node stopped restarting it (`domain::healing`): it is given out
    /// only while no other container of the service is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restart_limit: bool,
}

impl NameAddress {
    /// Whether the address is given out while the service has another.
    #[must_use]
    pub const fn answers(&self) -> bool {
        self.health.answers() && !self.restart_limit
    }
}

/// What a lookup of a service name returns on one node. Each node answers
/// its own containers, with the service's healthy containers on the same
/// node when there are any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct NodeLookup {
    /// The node whose containers ask.
    pub node: String,
    /// The addresses they get for `<service>`.
    pub addresses: Vec<Ipv4Addr>,
    /// `true` when these are the node's own containers; `false` when the
    /// node has no healthy one and answers with the other nodes'.
    pub local: bool,
}

/// A service name on one network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedName {
    /// The service's name, which containers on the network look up.
    pub name: String,
    /// The network's name in `mm.yaml`.
    pub network: String,
    /// Every container of the service on the network, on every node:
    /// what `tasks.<service>` resolves to, when healthy.
    pub addresses: Vec<NameAddress>,
    /// What `<service>` resolves to on each node that holds a part of the
    /// network, as that node's resolver says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lookups: Vec<NodeLookup>,
    /// None of the service's containers is healthy, and its running ones
    /// are answered all the same (fail open).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fail_open: bool,
}

impl ResolvedName {
    /// The addresses of the service's healthy containers on every node.
    pub fn answers(&self) -> impl Iterator<Item = &NameAddress> {
        self.addresses.iter().filter(|address| address.answers())
    }

    /// Whether the running containers are answered because none is
    /// healthy.
    #[must_use]
    pub fn fails_open(&self) -> bool {
        !self.addresses.iter().any(NameAddress::answers)
            && self
                .addresses
                .iter()
                .any(|address| address.health.running())
    }

    /// The addresses given out: the healthy ones, or, failing open, the
    /// running ones.
    pub fn routed(&self) -> impl Iterator<Item = &NameAddress> {
        let open = self.fails_open();
        self.addresses
            .iter()
            .filter(move |address| address.answers() || (open && address.health.running()))
    }
}

/// Something that keeps a node from answering names as it should.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct NameProblem {
    pub node: String,
    /// What is wrong and what to do about it.
    pub detail: String,
}

/// The names of an environment's services on its cluster networks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct NameReport {
    /// The cluster networks, by their names in `mm.yaml`, that the nodes
    /// hold a part of.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<ResolvedName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<NameProblem>,
}

pub(super) fn sorted(names: &mut [ResolvedName]) {
    for name in names.iter_mut() {
        name.addresses
            .sort_by_key(|address| (address.node.clone(), address.address));
        name.lookups
            .sort_by(|left, right| left.node.cmp(&right.node));
    }
    names.sort_by(|left, right| {
        (left.name.as_str(), left.network.as_str())
            .cmp(&(right.name.as_str(), right.network.as_str()))
    });
}

impl NameReport {
    /// The report with only the networks, by their names in `mm.yaml`, that
    /// `keep` says are the environment's: a report taken by the prefix of
    /// an environment's network keys also lists the networks of an
    /// environment whose name starts like it (`staging-eu` for `staging`).
    #[must_use]
    pub fn only_networks(mut self, keep: impl Fn(&str) -> bool) -> Self {
        self.networks.retain(|network| keep(network));
        self.names.retain(|name| keep(&name.network));
        self
    }

    /// The names answered while none of their containers is healthy, as
    /// `<service> on <network>`.
    #[must_use]
    pub fn answered_unhealthy(&self) -> Vec<String> {
        self.names
            .iter()
            .filter(|name| name.fail_open || name.fails_open())
            .map(|name| format!("{} on {}", name.name, name.network))
            .collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.networks.is_empty() && self.names.is_empty() && self.problems.is_empty()
    }

    /// One report from what each node's resolver knows. A container is
    /// listed once, with its health as its own node sees it; a node that
    /// holds a part of the container's network and does not know the
    /// container is named next to it.
    #[must_use]
    pub fn merged(views: Vec<(String, Self)>) -> Self {
        let mut merged = Self::default();
        for (viewer, view) in &views {
            merged.problems.extend(view.problems.iter().cloned());
            for network in &view.networks {
                if !merged.networks.contains(network) {
                    merged.networks.push(network.clone());
                }
            }
            for name in &view.names {
                let index = merged
                    .names
                    .iter()
                    .position(|known| known.name == name.name && known.network == name.network)
                    .unwrap_or_else(|| {
                        merged.names.push(ResolvedName {
                            addresses: Vec::new(),
                            lookups: Vec::new(),
                            ..name.clone()
                        });
                        merged.names.len() - 1
                    });
                let Some(known) = merged.names.get_mut(index) else {
                    continue;
                };
                // A node says what its own containers get, nobody else's.
                for lookup in name.lookups.iter().filter(|lookup| lookup.node == *viewer) {
                    if !known
                        .lookups
                        .iter()
                        .any(|listed| listed.node == lookup.node)
                    {
                        known.lookups.push(lookup.clone());
                    }
                }
                for address in &name.addresses {
                    match known
                        .addresses
                        .iter_mut()
                        .find(|listed| listed.address == address.address)
                    {
                        // A node knows its own containers best.
                        Some(listed) if address.node == *viewer => {
                            listed.health = address.health;
                            listed.restart_limit = address.restart_limit;
                            listed.node.clone_from(&address.node);
                        }
                        Some(_) => {}
                        None => known.addresses.push(address.clone()),
                    }
                }
            }
        }
        for name in &mut merged.names {
            name.fail_open = name.fails_open();
            for address in &mut name.addresses {
                address.unknown_to = views
                    .iter()
                    .filter(|(_, view)| {
                        view.networks.contains(&name.network)
                            && !view.names.iter().any(|seen| {
                                seen.name == name.name
                                    && seen.network == name.network
                                    && seen
                                        .addresses
                                        .iter()
                                        .any(|listed| listed.address == address.address)
                            })
                    })
                    .map(|(viewer, _)| viewer.clone())
                    .collect();
            }
        }
        merged.networks.sort();
        sorted(&mut merged.names);
        merged
    }

    /// The report with only the names of `service`.
    #[must_use]
    pub fn only(mut self, service: &str) -> Self {
        self.names.retain(|name| name.name == service);
        self
    }
}

/// `mm doctor`: each service the nodes answer while none of its
/// containers passes its health check (fail open).
#[must_use]
pub fn fail_open_diagnostics(report: &NameReport) -> Vec<crate::domain::diagnostic::Diagnostic> {
    report
        .names
        .iter()
        .filter(|name| name.fail_open || name.fails_open())
        .map(|name| {
            crate::domain::diagnostic::Diagnostic::warning(
                "MM_SERVICE_ANSWERED_UNHEALTHY",
                format!(
                    "service '{0}' on network '{1}' has no container that passes its health check, so the nodes answer and route to its running containers anyway. The check is the `healthcheck:` of service '{0}' in its Compose file; `mm service logs {0}` shows why it fails: fix the service, or the check if it is too strict",
                    name.name, name.network
                ),
            )
        })
        .collect()
}
