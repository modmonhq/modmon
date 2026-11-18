//! The cluster networks on `mmd`'s own host.
//!
//! The daemon stores what a reviewed plan gave the networks and asks for
//! the host to be brought to it. The bringing is `mmd network up`, run as
//! root by `mm-network.service` (and, for the firewall chains alone, by
//! `mm-mesh.service` before the engine starts): it routes the other nodes'
//! slices through the mesh, replaces mm's two iptables chains as a whole,
//! and has each network's bridge answer for the members on other nodes.
//! The forward chain is entered where the host's engine lets the host's
//! own rules in ([`forward_hook`]).
//!
//! Everything mm adds to the host's packet filter is in the chains
//! `MM-CLUSTER` and `MM-CLUSTER-NAT` and the two rules that enter them, so
//! removing mm's rules removes exactly those.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::app::ports::{ClusterNetworkHost, CommandOutput, CommandSpec, MeshHost, ProcessRunner};
use crate::domain::cluster_network::{
    FILTER_CHAIN, Firewall, ForwardHook, NAT_CHAIN, NetworkDrift, NetworkDriftKind, NodeNetworks,
    forward_hook,
};
use crate::domain::error::MmError;
use crate::domain::mesh::MeshApplied;

mod names;
mod store;

pub use names::ResolverFiles;
pub use store::NetworkStore;

const TIMEOUT: Duration = Duration::from_secs(30);
/// `iptables` lives in an sbin directory, which the default search path of a
/// process started without an environment does not include.
const SEARCH_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// The unit that runs `mmd network up`.
const UNIT: &str = "mm-network.service";

/// Where the host exposes what this adapter reads and writes besides
/// commands; tests point them at a directory.
#[derive(Debug, Clone)]
pub struct HostPaths {
    /// `/proc/sys/net/ipv4`.
    pub ipv4_settings: PathBuf,
    /// Exists on a host that systemd manages.
    pub systemd: PathBuf,
    /// `/sys/class/net`: one directory per network interface, readable by
    /// the daemon where `/proc/sys` is not.
    pub interfaces: PathBuf,
}

impl Default for HostPaths {
    fn default() -> Self {
        Self {
            ipv4_settings: PathBuf::from("/proc/sys/net/ipv4"),
            systemd: PathBuf::from("/run/systemd/system"),
            interfaces: PathBuf::from("/sys/class/net"),
        }
    }
}

pub struct HostClusterNetworks {
    store: NetworkStore,
    resolver_files: ResolverFiles,
    process: Arc<dyn ProcessRunner>,
    mesh: Arc<dyn MeshHost>,
    paths: HostPaths,
    hook: ForwardHook,
    /// The engine setup installed, whose networks mm removes.
    engine: crate::domain::runtime::RuntimeKind,
    /// Each bridge the host was last brought to the stored state with, by
    /// the kernel's index of the interface: a bridge the engine created
    /// again has another.
    settled: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
}

impl std::fmt::Debug for HostClusterNetworks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostClusterNetworks")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

fn failed(program: &str, arguments: &[&str], output: &CommandOutput) -> MmError {
    MmError::ProcessFailed {
        program: format!("{program} {}", arguments.join(" ")),
        code: output.code,
        stderr: output.stderr.trim().to_owned(),
    }
}

/// The `-A` lines of `iptables -S CHAIN`.
fn rules_of(listing: &str) -> Vec<String> {
    listing
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("-A "))
        .map(str::to_owned)
        .collect()
}

/// One bridge setting: its file under the IPv4 settings and the value it
/// must hold.
fn bridge_settings(bridge: &str) -> [(String, &'static str, &'static str); 2] {
    [
        // The bridge answers a container that asks for a member of its
        // network on another node, and forwards to it through the mesh.
        (format!("conf/{bridge}/proxy_arp"), "1", "proxy_arp"),
        // Without the kernel's random delay of up to 0.8 s per answer.
        (format!("neigh/{bridge}/proxy_delay"), "0", "proxy_delay"),
    ]
}

impl HostClusterNetworks {
    /// `state_root` is `mmd`'s own state directory; the networks live in
    /// its `networks` subdirectory.
    #[must_use]
    pub fn new(
        state_root: &Path,
        process: Arc<dyn ProcessRunner>,
        mesh: Arc<dyn MeshHost>,
    ) -> Self {
        Self {
            store: NetworkStore::new(state_root.join("networks")),
            resolver_files: ResolverFiles::new(
                state_root.join("networks").join("resolv"),
                PathBuf::from("/etc/resolv.conf"),
            ),
            process,
            mesh,
            paths: HostPaths::default(),
            hook: forward_hook(crate::domain::runtime::RuntimeKind::DockerCompose),
            engine: crate::domain::runtime::RuntimeKind::DockerCompose,
            settled: std::sync::Mutex::default(),
        }
    }

    #[must_use]
    pub fn with_paths(mut self, paths: HostPaths) -> Self {
        self.paths = paths;
        self
    }

    /// The host runs `engine`, whose packet filter mm's chains enter.
    #[must_use]
    pub fn for_engine(mut self, engine: crate::domain::runtime::RuntimeKind) -> Self {
        self.hook = forward_hook(engine);
        self.engine = engine;
        self
    }

    async fn run(
        &self,
        program: &str,
        arguments: &[&str],
        stdin: Option<String>,
    ) -> Result<CommandOutput, MmError> {
        let mut specification = CommandSpec::new(program)
            .args(arguments.iter().copied())
            .environment("LC_ALL", "C")
            .environment("PATH", SEARCH_PATH)
            .timeout(TIMEOUT);
        if let Some(stdin) = stdin {
            specification = specification.sensitive_stdin(Zeroizing::new(stdin));
        }
        self.process
            .run(specification)
            .await
            .map_err(|error| match error {
                MmError::ProcessSpawn { program, .. } => MmError::Target {
                    message: format!(
                        "`{program}` is not installed on this host; cluster networks need the iptables package: install it with `apt-get install iptables`"
                    ),
                },
                other => other,
            })
    }

    /// The rules of one chain as the kernel holds them; `None` when the
    /// chain does not exist.
    async fn chain(&self, table: &str, chain: &str) -> Result<Option<Vec<String>>, MmError> {
        let output = self
            .run("iptables", &["-w", "-t", table, "-S", chain], None)
            .await?;
        Ok(output.success().then(|| rules_of(&output.stdout)))
    }

    async fn enters(&self, table: &str, from: &str, chain: &str) -> Result<bool, MmError> {
        Ok(self
            .run(
                "iptables",
                &["-w", "-t", table, "-C", from, "-j", chain],
                None,
            )
            .await?
            .success())
    }

    /// What the two chains and their entry rules have that differs from
    /// `wanted`, in words.
    async fn firewall_drift(&self, wanted: &Firewall) -> Result<Vec<String>, MmError> {
        let mut drift = Vec::new();
        for (table, chain, from, rules) in [
            (
                self.hook.table,
                FILTER_CHAIN,
                self.hook.chain,
                &wanted.filter,
            ),
            ("nat", NAT_CHAIN, "POSTROUTING", &wanted.nat),
        ] {
            let Some(held) = self.chain(table, chain).await? else {
                drift.push(format!("the chain {chain} is missing"));
                continue;
            };
            for rule in rules.iter().filter(|rule| !held.contains(rule)) {
                drift.push(format!("missing rule `{rule}`"));
            }
            for rule in held.iter().filter(|rule| !rules.contains(rule)) {
                drift.push(format!("unexpected rule `{rule}`"));
            }
            if drift.is_empty() && held != *rules {
                drift.push(format!("the rules of {chain} are in another order"));
            }
            if !self.enters(table, from, chain).await? {
                drift.push(format!("{from} does not enter {chain}"));
            }
        }
        Ok(drift)
    }

    /// Replaces both chains as a whole and makes sure they are entered.
    async fn load_firewall(&self, wanted: &Firewall) -> Result<(), MmError> {
        let arguments = ["--noflush", "--wait", "5"];
        let output = self
            .run(
                "iptables-restore",
                &arguments,
                Some(wanted.restore_input(self.hook)),
            )
            .await?;
        if !output.success() {
            return Err(failed("iptables-restore", &arguments, &output));
        }
        // Before the engine has started for the first time its chain does
        // not exist; the engine keeps a chain it finds.
        if self.hook.create
            && self
                .chain(self.hook.table, self.hook.chain)
                .await?
                .is_none()
        {
            self.iptables(&["-w", "-t", self.hook.table, "-N", self.hook.chain])
                .await?;
        }
        for (table, from, chain) in [
            (self.hook.table, self.hook.chain, FILTER_CHAIN),
            ("nat", "POSTROUTING", NAT_CHAIN),
        ] {
            if !self.enters(table, from, chain).await? {
                self.iptables(&["-w", "-t", table, "-I", from, "1", "-j", chain])
                    .await?;
            }
        }
        Ok(())
    }

    async fn iptables(&self, arguments: &[&str]) -> Result<(), MmError> {
        let output = self.run("iptables", arguments, None).await?;
        if output.success() {
            Ok(())
        } else {
            Err(failed("iptables", arguments, &output))
        }
    }

    /// Whether a bridge answers for addresses routed elsewhere, read over
    /// netlink, which works where `/proc/sys` is hidden from the daemon;
    /// `None` when the engine has not created the bridge.
    async fn bridge_answers(&self, bridge: &str) -> Result<Option<bool>, MmError> {
        let output = self
            .run("ip", &["-4", "netconf", "show", "dev", bridge], None)
            .await?;
        if !output.success() {
            return Ok(None);
        }
        let mut words = output.stdout.split_whitespace();
        Ok(words
            .by_ref()
            .find(|word| *word == "proxy_neigh")
            .and_then(|_| words.next())
            .map(|value| value == "on"))
    }

    /// The prefixes routed through the mesh interface.
    async fn mesh_routes(&self, interface: &str) -> Result<Vec<String>, MmError> {
        let arguments = [
            "-j", "-4", "route", "show", "dev", interface, "proto", "static",
        ];
        let output = self.run("ip", &arguments, None).await?;
        if !output.success() {
            // No interface: no route.
            return Ok(Vec::new());
        }
        let routes: Vec<serde_json::Value> =
            serde_json::from_str(output.stdout.trim()).map_err(|_| MmError::RuntimeResponse {
                message: "`ip -j route show` printed a route list mmd does not understand"
                    .to_owned(),
            })?;
        Ok(routes
            .iter()
            .filter_map(|route| route["dst"].as_str().map(str::to_owned))
            .collect())
    }

    /// The bridges that exist and whose settings differ, as (bridge,
    /// setting file, wanted value, setting name).
    fn bridges_off(
        &self,
        state: &NodeNetworks,
    ) -> Vec<(String, PathBuf, &'static str, &'static str)> {
        let mut off = Vec::new();
        for local in state.local() {
            for (file, wanted, name) in bridge_settings(&local.bridge) {
                let path = self.paths.ipv4_settings.join(file);
                // No file: the engine has not created the bridge (yet).
                if let Ok(held) = std::fs::read_to_string(&path)
                    && held.trim() != wanted
                {
                    off.push((local.bridge.clone(), path, wanted, name));
                }
            }
        }
        off
    }
}

#[async_trait]
impl ClusterNetworkHost for HostClusterNetworks {
    fn stored(&self) -> Result<Option<NodeNetworks>, MmError> {
        self.store.state()
    }

    fn store(&self, desired: &NodeNetworks) -> Result<bool, MmError> {
        desired
            .table
            .validate()
            .map_err(|message| MmError::Security { message })?;
        // Before the state, so a node that holds a network always has the
        // file its containers mount.
        self.resolver_files.write(desired)?;
        self.store.write(desired)
    }

    fn resolver_files(&self) -> PathBuf {
        self.resolver_files.directory().to_path_buf()
    }

    async fn converge(&self) -> Result<(), MmError> {
        if !self.paths.systemd.exists() {
            // A host without systemd has no unit to ask; mmd is then an
            // ordinary root process and does the work itself.
            return self.up(false).await.map(drop);
        }
        let arguments = ["restart", UNIT];
        let output = self.run("systemctl", &arguments, None).await?;
        if output.success() {
            return Ok(());
        }
        Err(MmError::Target {
            message: format!(
                "the node's cluster networks could not be brought up: `systemctl restart {UNIT}` failed ({}). On the node, `journalctl -u {UNIT} -n 30` says why; `mm target setup plan` and its apply install the unit when it is missing",
                output.stderr.trim()
            ),
        })
    }

    async fn up(&self, before_engine: bool) -> Result<MeshApplied, MmError> {
        let Some(state) = self.store.state()? else {
            return Ok(MeshApplied::default());
        };
        let mut actions = Vec::new();
        let firewall = state.firewall();
        let drift = self.firewall_drift(&firewall).await?;
        if !drift.is_empty() {
            self.load_firewall(&firewall).await?;
            actions.push(format!(
                "loaded the firewall chains {FILTER_CHAIN} and {NAT_CHAIN} ({})",
                drift.join("; ")
            ));
        }
        if !before_engine {
            for (bridge, path, wanted, name) in self.bridges_off(&state) {
                std::fs::write(&path, wanted).map_err(|source| MmError::Io {
                    context: format!("writing {}", path.display()),
                    source,
                })?;
                actions.push(format!("set {name} to {wanted} on the bridge {bridge}"));
            }
            // Last, so a peer whose name does not resolve yet does not keep
            // the rules from loading.
            actions.extend(self.mesh.up().await?.actions);
        }
        Ok(MeshApplied {
            changed: !actions.is_empty(),
            actions,
        })
    }

    async fn drift(&self) -> Result<Vec<NetworkDrift>, MmError> {
        let Some(state) = self.store.state()? else {
            return Ok(Vec::new());
        };
        let mut drift = self
            .firewall_drift(&state.firewall())
            .await?
            .into_iter()
            .map(|detail| NetworkDrift::new(NetworkDriftKind::Firewall, detail))
            .collect::<Vec<_>>();
        let mesh = self.mesh.status().await?;
        let routes = self.mesh_routes(&state.mesh_interface).await?;
        for (node, slices) in state.routed() {
            let Some(peer) = mesh
                .peers
                .iter()
                .find(|peer| peer.name.as_deref() == Some(node.as_str()))
            else {
                continue;
            };
            for slice in slices {
                let slice = slice.to_string();
                if !peer.allowed_ips.contains(&slice) {
                    drift.push(NetworkDrift::new(
                        NetworkDriftKind::Route,
                        format!("{node} is not allowed to send from {slice} through the mesh"),
                    ));
                }
                if !routes.contains(&slice) {
                    drift.push(NetworkDrift::new(
                        NetworkDriftKind::Route,
                        format!(
                            "{slice} is not routed to {node} through {}",
                            state.mesh_interface
                        ),
                    ));
                }
            }
        }
        for local in state.local() {
            if self.bridge_answers(&local.bridge).await? == Some(false) {
                drift.push(NetworkDrift::new(
                    NetworkDriftKind::Bridge,
                    format!(
                        "the bridge {} of network {} does not answer for members on other nodes (proxy_arp is off)",
                        local.bridge, local.key
                    ),
                ));
            }
        }
        Ok(drift)
    }

    async fn settle_bridges(&self) -> Result<bool, MmError> {
        let Some(state) = self.store.state()? else {
            return Ok(false);
        };
        // The index of each bridge that exists now; a bridge the engine
        // has not created has none.
        let present = state
            .local()
            .into_iter()
            .filter_map(|local| {
                let index = std::fs::read_to_string(
                    self.paths.interfaces.join(&local.bridge).join("ifindex"),
                )
                .ok()?;
                Some((local.bridge, index.trim().to_owned()))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let new = {
            let settled = self
                .settled
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            present
                .iter()
                .any(|(bridge, index)| settled.get(bridge) != Some(index))
        };
        if !new {
            return Ok(false);
        }
        self.converge().await?;
        *self
            .settled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = present;
        Ok(true)
    }

    async fn remove_bridge(&self, network: &str) -> Result<bool, MmError> {
        let output = crate::adapters::node_files::engine(
            self.process.as_ref(),
            self.engine,
            &["network", "rm", network],
            TIMEOUT,
        )
        .await?;
        let said = output.stderr.to_ascii_lowercase();
        // Gone already counts; one a container is on stays (Docker: "has
        // active endpoints"; Podman: "is being used").
        Ok(output.success() || said.contains("not found") || said.contains("no such network"))
    }

    async fn leave(&self) -> Result<MeshApplied, MmError> {
        let mut actions = Vec::new();
        for (table, from, chain) in [
            (self.hook.table, self.hook.chain, FILTER_CHAIN),
            ("nat", "POSTROUTING", NAT_CHAIN),
        ] {
            if self.chain(table, chain).await?.is_none() {
                continue;
            }
            while self.enters(table, from, chain).await? {
                self.iptables(&["-w", "-t", table, "-D", from, "-j", chain])
                    .await?;
            }
            self.iptables(&["-w", "-t", table, "-F", chain]).await?;
            self.iptables(&["-w", "-t", table, "-X", chain]).await?;
            actions.push(format!("removed the firewall chain {chain}"));
        }
        if self.store.remove()? {
            actions.push("removed the stored network state".to_owned());
        }
        // The chains that guard the mesh name each network the node held.
        if self.mesh.stored()?.is_some() {
            actions.extend(self.mesh.up().await?.actions);
        }
        Ok(MeshApplied {
            changed: !actions.is_empty(),
            actions,
        })
    }
}

#[cfg(test)]
mod tests;
