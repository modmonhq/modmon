//! The packet filter rules that keep shared storage between the hosts.
//!
//! The storage node's NFS server listens on its mesh address and exports
//! only to the nodes' mesh addresses. That alone does not keep a container
//! out: one on the storage node reaches the node's own mesh address, and
//! one on another node, on a network of its host, leaves through the mesh
//! with that node's address (measured on Docker Engine 29). So every node
//! drops what its containers send to the storage port through the mesh,
//! and takes storage traffic for its own mesh address from the other
//! nodes' mesh addresses only.
//!
//! The rules follow from the node's mesh state alone, so they are the same
//! on a node that serves storage and on one that does not, and they are in
//! place before any storage is used.

use std::net::Ipv4Addr;

use super::NFS_PORT;
use crate::domain::mesh::MeshState;

/// Entered where the engine lets the host's own rules for forwarded
/// traffic in ([`crate::domain::cluster_network::forward_hook`]): what
/// containers send on.
pub const STORAGE_FORWARD_CHAIN: &str = "MM-STORAGE";
/// Entered from `INPUT`: what arrives for the host itself.
pub const STORAGE_INPUT_CHAIN: &str = "MM-STORAGE-IN";

const COMMENT: &str = "mm shared storage";

/// The rules of both chains, written the way `iptables -S` prints them, so
/// what is loaded can be compared with what is wanted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFirewall {
    pub forward: Vec<String>,
    pub input: Vec<String>,
}

impl StorageFirewall {
    /// The rules of the node whose mesh interface is `state`.
    #[must_use]
    pub fn of(state: &MeshState) -> Self {
        let interface = state.interface.as_str();
        let own = state.address;
        let port = format!("-p tcp -m tcp --dport {NFS_PORT} -m comment --comment \"{COMMENT}\"");
        let mut input = vec![format!(
            "-A {STORAGE_INPUT_CHAIN} -s {own}/32 -d {own}/32 -i lo {port} -j RETURN"
        )];
        let mut peers = state
            .peers
            .iter()
            .filter_map(|peer| peer.allowed_ips.iter().find(|allowed| allowed.prefix == 32))
            .map(|allowed| allowed.address)
            .collect::<Vec<Ipv4Addr>>();
        peers.sort();
        peers.dedup();
        for peer in peers {
            input.push(format!(
                "-A {STORAGE_INPUT_CHAIN} -s {peer}/32 -d {own}/32 -i {interface} {port} -j RETURN"
            ));
        }
        input.push(format!(
            "-A {STORAGE_INPUT_CHAIN} -d {own}/32 {port} -j DROP"
        ));
        Self {
            forward: vec![format!(
                "-A {STORAGE_FORWARD_CHAIN} -o {interface} {port} -j DROP"
            )],
            input,
        }
    }

    /// Input for `iptables-restore --noflush`: both chains are created when
    /// missing and replaced as a whole, and nothing else is touched. The
    /// forward chain lives in the table `hook` names, the input chain in
    /// the filter table.
    #[must_use]
    pub fn restore_input(&self, hook: crate::domain::cluster_network::ForwardHook) -> String {
        if hook.table == "filter" {
            let mut input = format!(
                "*filter\n:{STORAGE_FORWARD_CHAIN} - [0:0]\n:{STORAGE_INPUT_CHAIN} - [0:0]\n"
            );
            for rule in self.forward.iter().chain(&self.input) {
                input.push_str(rule);
                input.push('\n');
            }
            input.push_str("COMMIT\n");
            return input;
        }
        let mut input = format!("*{}\n:{STORAGE_FORWARD_CHAIN} - [0:0]\n", hook.table);
        for rule in &self.forward {
            input.push_str(rule);
            input.push('\n');
        }
        input.push_str(&format!(
            "COMMIT\n*filter\n:{STORAGE_INPUT_CHAIN} - [0:0]\n"
        ));
        for rule in &self.input {
            input.push_str(rule);
            input.push('\n');
        }
        input.push_str("COMMIT\n");
        input
    }
}
