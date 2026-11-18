//! The packet filter rules that keep the mesh between the hosts.
//!
//! The mesh carries two kinds of traffic: what the nodes themselves send
//! each other (the name exchange, shared storage, anything a host serves on
//! its mesh address), and what containers on a network that spans the nodes
//! send to that network's members elsewhere. Nothing else belongs on it,
//! and two things would otherwise get there (both measured on Docker
//! Engine 29):
//!
//! - A container on a network of its own host (the engine's default bridge,
//!   a Compose network that is not in `networks:`) leaves through the mesh
//!   with its node's mesh address as source, so the other nodes take it for
//!   the node. The forward chain drops everything a host forwards out of
//!   the mesh interface, except what a cluster network's containers on this
//!   node send to that network's block.
//! - Linux takes a packet for any of its addresses on any interface, so a
//!   neighbour on the public network that routes a node's mesh address, or
//!   the address a node has on a cluster network, at the node's public
//!   interface reaches whatever listens there. The input chain takes the
//!   mesh address from the mesh subnet on the mesh interface only, and each
//!   cluster network's address on that network's own bridge only.
//!
//! The rules follow from the node's mesh state and the cluster networks it
//! stored, so a node that holds no cluster network has them too.

use crate::domain::cluster_network::NodeNetworks;
use crate::domain::mesh::MeshState;

/// Entered from the chain [`crate::domain::cluster_network::forward_hook`]
/// names for the host's engine: what the host forwards.
pub const MESH_FORWARD_CHAIN: &str = "MM-MESH";
/// Entered from `INPUT`: what arrives for the host itself.
pub const MESH_INPUT_CHAIN: &str = "MM-MESH-IN";

const COMMENT: &str = "mm mesh guard";

/// The rules of both chains, written the way `iptables -S` prints them, so
/// what is loaded can be compared with what is wanted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshGuard {
    pub forward: Vec<String>,
    pub input: Vec<String>,
}

impl MeshGuard {
    /// The rules of the node whose mesh interface is `state` and which
    /// stored `networks` at its last deployment.
    #[must_use]
    pub fn of(state: &MeshState, networks: Option<&NodeNetworks>) -> Self {
        let mesh = state.interface.as_str();
        let own = state.address;
        let subnet = state.subnet();
        let local = networks.map(NodeNetworks::local).unwrap_or_default();
        let comment = |text: &str| format!("-m comment --comment \"{text}\"");
        let guard = comment(COMMENT);

        // A connection the mesh let in (a published port a peer reached)
        // is answered through it.
        let mut forward = vec![format!(
            "-A {MESH_FORWARD_CHAIN} -m conntrack --ctstate RELATED,ESTABLISHED {} -j RETURN",
            comment("mm replies")
        )];
        let mut input = vec![
            format!("-A {MESH_INPUT_CHAIN} -i lo {guard} -j RETURN"),
            format!("-A {MESH_INPUT_CHAIN} -s {subnet} -d {own}/32 -i {mesh} {guard} -j RETURN"),
        ];
        for network in &local {
            let name = comment(&format!("mm network {}", network.key));
            forward.push(format!(
                "-A {MESH_FORWARD_CHAIN} -s {} -d {} -o {mesh} {name} -j RETURN",
                network.slice, network.block
            ));
            input.push(format!(
                "-A {MESH_INPUT_CHAIN} -d {}/32 -i {} {name} -j RETURN",
                network.gateway, network.bridge
            ));
        }
        forward.push(format!("-A {MESH_FORWARD_CHAIN} -o {mesh} {guard} -j DROP"));
        input.push(format!("-A {MESH_INPUT_CHAIN} -d {own}/32 {guard} -j DROP"));
        for network in &local {
            input.push(format!(
                "-A {MESH_INPUT_CHAIN} -d {}/32 {} -j DROP",
                network.gateway,
                comment(&format!("mm network {}", network.key))
            ));
        }
        Self { forward, input }
    }

    /// Input for `iptables-restore --noflush`: both chains are created when
    /// missing and replaced as a whole, and nothing else is touched. The
    /// forward chain lives in the table `hook` names (the filter table on
    /// Docker Engine, mangle on Podman), the input chain in the filter
    /// table.
    #[must_use]
    pub fn restore_input(&self, hook: crate::domain::cluster_network::ForwardHook) -> String {
        let separate = hook.table != "filter";
        let mut input = format!("*{}\n:{MESH_FORWARD_CHAIN} - [0:0]\n", hook.table);
        if separate {
            push_rules(&mut input, &self.forward);
            input.push_str("COMMIT\n*filter\n");
        }
        input.push_str(&format!(":{MESH_INPUT_CHAIN} - [0:0]\n"));
        if !separate {
            push_rules(&mut input, &self.forward);
        }
        push_rules(&mut input, &self.input);
        input.push_str("COMMIT\n");
        input
    }

    /// What `listing`, the output of `iptables -S` for the table of the
    /// forward chain followed by the filter table's when that is another,
    /// has that differs from these rules, in words; empty when the host
    /// holds exactly them and both chains are entered. `engine_hook` is the
    /// chain that enters the forward chain (`DOCKER-USER`, or mangle's
    /// `FORWARD` on Podman).
    #[must_use]
    pub fn drift(&self, listing: &str, engine_hook: &str) -> Vec<String> {
        let lines = listing.lines().map(str::trim).collect::<Vec<_>>();
        let mut drift = Vec::new();
        for (chain, from, wanted) in [
            (MESH_FORWARD_CHAIN, engine_hook, &self.forward),
            (MESH_INPUT_CHAIN, "INPUT", &self.input),
        ] {
            let declared = format!("-N {chain}");
            if !lines.contains(&declared.as_str()) {
                drift.push(format!("the chain {chain} is missing"));
                continue;
            }
            let prefix = format!("-A {chain} ");
            let held = lines
                .iter()
                .filter(|line| line.starts_with(&prefix))
                .collect::<Vec<_>>();
            for rule in wanted.iter().filter(|rule| !held.contains(&&rule.as_str())) {
                drift.push(format!("missing rule `{rule}`"));
            }
            for rule in held
                .iter()
                .filter(|rule| !wanted.iter().any(|wanted| wanted == **rule))
            {
                drift.push(format!("unexpected rule `{rule}`"));
            }
            if drift.is_empty()
                && !held
                    .iter()
                    .map(|rule| **rule)
                    .eq(wanted.iter().map(String::as_str))
            {
                drift.push(format!("the rules of {chain} are in another order"));
            }
            let entered = format!("-A {from} -j {chain}");
            if !lines.contains(&entered.as_str()) {
                drift.push(format!("{from} does not enter {chain}"));
            }
        }
        drift
    }
}

fn push_rules(input: &mut String, rules: &[String]) {
    for rule in rules {
        input.push_str(rule);
        input.push('\n');
    }
}
