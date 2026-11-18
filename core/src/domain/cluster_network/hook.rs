//! Where mm's rules for what a host forwards for its containers enter the
//! host's packet filter. This is the one place the two engines differ for
//! mm's chains: the networks' `MM-CLUSTER` and shared storage's
//! `MM-STORAGE` are entered from here.
//!
//! Docker Engine keeps its rules in iptables' filter table and evaluates
//! `DOCKER-USER`, a chain it keeps for the host's own rules, before them.
//! Podman's netavark has no such chain: it keeps its rules in a table of
//! its own (its nftables driver) or appends them to the filter table's
//! `FORWARD` (its iptables driver), where a later network can put them
//! ahead of mm's. So on a Podman host mm's chains are entered from the
//! mangle table's `FORWARD`, which every forwarded packet passes before any
//! filter: a drop there is final, whatever the engine accepts afterwards.
//! Nothing mm accepts has to win over netavark, which accepts everything
//! that leaves or enters a network's own subnet (a cluster network's
//! whole block) and drops nothing else of it.

use crate::domain::runtime::RuntimeKind;

/// A table and a chain of iptables that mm's own chain is entered from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardHook {
    /// The table both the chain and mm's chain live in.
    pub table: &'static str,
    /// The chain whose first rule enters mm's chain.
    pub chain: &'static str,
    /// The chain is the engine's own and does not exist before the engine
    /// first starts: mm creates it then, and the engine keeps a chain it
    /// finds.
    pub create: bool,
}

/// Where mm's forward rules enter on a host whose engine is `engine`.
#[must_use]
pub const fn forward_hook(engine: RuntimeKind) -> ForwardHook {
    match engine {
        RuntimeKind::DockerCompose => ForwardHook {
            table: "filter",
            chain: "DOCKER-USER",
            create: true,
        },
        RuntimeKind::PodmanCompose => ForwardHook {
            table: "mangle",
            chain: "FORWARD",
            create: false,
        },
    }
}
