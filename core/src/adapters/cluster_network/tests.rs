//! The adapter against a simulated packet filter: `iptables`,
//! `iptables-restore`, and `systemctl` are answered from an in-memory model
//! that records every command line.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::*;
use crate::domain::cluster_network::{NetworkNeed, NetworkTable, SealedNetworks, bridge_name};
use crate::domain::mesh::{MeshFacts, MeshNodeStatus, MeshPeerStatus, MeshState};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Default)]
struct Simulation {
    /// Rules by (table, chain), as `iptables -S` prints them.
    chains: BTreeMap<(String, String), Vec<String>>,
    commands: Vec<String>,
    unit_fails: bool,
    /// The prefixes routed through the mesh interface.
    routes: Vec<String>,
    /// Where the bridges keep their settings, as the kernel exposes them.
    ipv4_settings: std::path::PathBuf,
}

#[derive(Default)]
struct SimulatedHost {
    state: Mutex<Simulation>,
}

fn output(code: i32, stdout: String, stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

impl Simulation {
    fn iptables(&mut self, words: &[&str]) -> CommandOutput {
        let missing = || {
            output(
                1,
                String::new(),
                "iptables: No chain/target/match by that name.",
            )
        };
        let ["-w", "-t", table, action, rest @ ..] = words else {
            return output(2, String::new(), "unexpected iptables arguments");
        };
        let key = |chain: &str| ((*table).to_owned(), chain.to_owned());
        match (*action, rest) {
            ("-S", [chain]) => match self.chains.get(&key(chain)) {
                Some(rules) => output(0, format!("-N {chain}\n{}\n", rules.join("\n")), ""),
                None => missing(),
            },
            ("-N", [chain]) => {
                self.chains.entry(key(chain)).or_default();
                output(0, String::new(), "")
            }
            ("-C", [from, "-j", chain]) => {
                let rule = format!("-A {from} -j {chain}");
                match self.chains.get(&key(from)) {
                    Some(rules) if rules.contains(&rule) => output(0, String::new(), ""),
                    _ => missing(),
                }
            }
            ("-I", [from, "1", "-j", chain]) => {
                let rule = format!("-A {from} -j {chain}");
                self.chains.entry(key(from)).or_default().insert(0, rule);
                output(0, String::new(), "")
            }
            ("-D", [from, "-j", chain]) => {
                let rule = format!("-A {from} -j {chain}");
                if let Some(rules) = self.chains.get_mut(&key(from)) {
                    rules.retain(|held| *held != rule);
                }
                output(0, String::new(), "")
            }
            ("-F", [chain]) => {
                if let Some(rules) = self.chains.get_mut(&key(chain)) {
                    rules.clear();
                }
                output(0, String::new(), "")
            }
            ("-X", [chain]) => {
                self.chains.remove(&key(chain));
                output(0, String::new(), "")
            }
            _ => output(2, String::new(), "unexpected iptables action"),
        }
    }

    /// `--noflush`: a chain the input declares is created or emptied, every
    /// other chain stays as it is.
    fn restore(&mut self, input: &str) -> CommandOutput {
        let mut table = String::new();
        for line in input.lines() {
            if let Some(name) = line.strip_prefix('*') {
                name.clone_into(&mut table);
            } else if let Some(declared) = line.strip_prefix(':') {
                let chain = declared.split(' ').next().unwrap_or_default();
                self.chains
                    .insert((table.clone(), chain.to_owned()), Vec::new());
            } else if let Some(chain) = line
                .strip_prefix("-A ")
                .and_then(|rest| rest.split(' ').next())
            {
                self.chains
                    .entry((table.clone(), chain.to_owned()))
                    .or_default()
                    .push(line.to_owned());
            }
        }
        output(0, String::new(), "")
    }
}

#[async_trait]
impl ProcessRunner for SimulatedHost {
    async fn run(&self, mut specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let program = specification.program().to_owned();
        let arguments = specification
            .arguments()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let stdin = specification.take_sensitive_stdin();
        let mut state = self.state.lock().map_err(|_| MmError::State {
            message: "the simulated host is poisoned".to_owned(),
        })?;
        state
            .commands
            .push(format!("{program} {}", arguments.join(" ")));
        let words = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(match (program.as_str(), words.as_slice()) {
            ("ip", ["-4", "netconf", "show", "dev", bridge]) => {
                let setting = state
                    .ipv4_settings
                    .join("conf")
                    .join(bridge)
                    .join("proxy_arp");
                match std::fs::read_to_string(setting) {
                    Ok(value) => output(
                        0,
                        format!(
                            "inet {bridge} forwarding on rp_filter loose mc_forwarding off proxy_neigh {} ignore_routes_with_linkdown off \n",
                            if value.trim() == "1" { "on" } else { "off" }
                        ),
                        "",
                    ),
                    Err(_) => output(
                        1,
                        String::new(),
                        &format!("Device \"{bridge}\" does not exist."),
                    ),
                }
            }
            ("ip", ["-j", "-4", "route", "show", "dev", "mm0", "proto", "static"]) => {
                let routes = state
                    .routes
                    .iter()
                    .map(|route| serde_json::json!({"dst": route, "scope": "link", "flags": []}))
                    .collect::<Vec<_>>();
                output(0, serde_json::Value::Array(routes).to_string(), "")
            }
            ("iptables", words) => state.iptables(words),
            ("iptables-restore", ["--noflush", "--wait", "5"]) => {
                state.restore(stdin.as_deref().map(String::as_str).unwrap_or_default())
            }
            ("systemctl", ["restart", "mm-network.service"]) if state.unit_fails => {
                output(1, String::new(), "Job for mm-network.service failed.")
            }
            ("systemctl", ["restart", "mm-network.service"]) => output(0, String::new(), ""),
            other => output(2, String::new(), &format!("unexpected command {other:?}")),
        })
    }
}

/// A mesh that counts how often it is brought up and reports the allowed
/// addresses it was given.
#[derive(Default)]
struct FakeMesh {
    ups: Mutex<u32>,
    allowed: Mutex<Vec<(String, Vec<String>)>>,
}

#[async_trait]
impl MeshHost for FakeMesh {
    async fn facts(&self) -> Result<MeshFacts, MmError> {
        Ok(MeshFacts::default())
    }
    async fn ensure_key(&self) -> Result<String, MmError> {
        Ok(String::new())
    }
    async fn apply(&self, _desired: &MeshState) -> Result<MeshApplied, MmError> {
        Ok(MeshApplied::default())
    }
    async fn up(&self) -> Result<MeshApplied, MmError> {
        if let Ok(mut ups) = self.ups.lock() {
            *ups += 1;
        }
        Ok(MeshApplied::default())
    }
    async fn status(&self) -> Result<MeshNodeStatus, MmError> {
        let allowed = self
            .allowed
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default();
        Ok(MeshNodeStatus {
            peers: allowed
                .into_iter()
                .map(|(name, allowed_ips)| MeshPeerStatus {
                    name: Some(name),
                    public_key: String::new(),
                    endpoint: None,
                    configured_endpoint: None,
                    allowed_ips,
                    handshake_age_seconds: None,
                    received_bytes: 0,
                    sent_bytes: 0,
                    on_interface: true,
                })
                .collect(),
            ..MeshNodeStatus::default()
        })
    }
    async fn stage_key(&self) -> Result<String, MmError> {
        Ok(String::new())
    }
    async fn activate_key(&self) -> Result<String, MmError> {
        Ok(String::new())
    }
    async fn leave(&self) -> Result<MeshApplied, MmError> {
        Ok(MeshApplied::default())
    }
}

struct Host {
    kernel: Arc<SimulatedHost>,
    mesh: Arc<FakeMesh>,
    networks: HostClusterNetworks,
    directory: tempfile::TempDir,
}

const BACKEND: &str = "0123456789ab-production-backend";

/// `app-1` of a cluster where `backend` spans it and `app-2`.
fn state() -> Result<NodeNetworks, String> {
    let space = "10.64.0.0/12".parse()?;
    let mut table = NetworkTable::default();
    for node in ["app-1", "app-2"] {
        table.assign(
            space,
            &NetworkNeed {
                key: BACKEND,
                node,
                network: "mm-engine-backend",
                internal: false,
            },
        )?;
    }
    Ok(SealedNetworks {
        space,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table,
        retired: Vec::new(),
    }
    .on_node("app-1"))
}

impl Host {
    fn new(systemd: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let kernel = Arc::new(SimulatedHost::default());
        let mesh = Arc::new(FakeMesh::default());
        if let Ok(mut allowed) = mesh.allowed.lock() {
            allowed.push((
                "app-2".to_owned(),
                vec!["10.90.0.2/32".to_owned(), "10.64.1.0/24".to_owned()],
            ));
        }
        if let Ok(mut state) = kernel.state.lock() {
            state.ipv4_settings = directory.path().join("ipv4");
            state.routes = vec!["10.64.1.0/24".to_owned()];
        }
        let systemd_marker = directory.path().join("systemd");
        if systemd {
            std::fs::create_dir_all(&systemd_marker)?;
        }
        let networks = HostClusterNetworks::new(
            &directory.path().join("state"),
            kernel.clone(),
            mesh.clone(),
        )
        .with_paths(HostPaths {
            ipv4_settings: directory.path().join("ipv4"),
            systemd: systemd_marker,
            interfaces: directory.path().join("net"),
        });
        Ok(Self {
            kernel,
            mesh,
            networks,
            directory,
        })
    }

    /// Creates the settings files of the backend bridge, as the kernel
    /// does when the engine creates the bridge.
    fn bridge(&self) -> Result<(std::path::PathBuf, std::path::PathBuf), std::io::Error> {
        let bridge = bridge_name(BACKEND);
        let ipv4 = self.directory.path().join("ipv4");
        let arp = ipv4.join("conf").join(&bridge).join("proxy_arp");
        let delay = ipv4.join("neigh").join(&bridge).join("proxy_delay");
        for (path, value) in [(&arp, "0\n"), (&delay, "80\n")] {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, value)?;
        }
        Ok((arp, delay))
    }

    fn commands(&self) -> Vec<String> {
        self.kernel
            .state
            .lock()
            .map(|state| state.commands.clone())
            .unwrap_or_default()
    }

    fn forget_commands(&self) {
        if let Ok(mut state) = self.kernel.state.lock() {
            state.commands.clear();
        }
    }

    fn chain(&self, table: &str, chain: &str) -> Option<Vec<String>> {
        self.kernel
            .state
            .lock()
            .ok()?
            .chains
            .get(&(table.to_owned(), chain.to_owned()))
            .cloned()
    }

    fn ups(&self) -> u32 {
        self.mesh.ups.lock().map(|ups| *ups).unwrap_or_default()
    }
}

#[tokio::test]
async fn up_loads_both_chains_once_and_then_changes_nothing() -> TestResult {
    let host = Host::new(false)?;
    assert!(
        host.networks.up(false).await?.actions.is_empty(),
        "nothing stored"
    );
    assert!(host.commands().is_empty());

    let state = state()?;
    assert!(host.networks.store(&state)?);
    assert!(
        !host.networks.store(&state)?,
        "storing the same state again"
    );
    assert_eq!(host.networks.stored()?, Some(state.clone()));
    let (arp, delay) = host.bridge()?;

    let applied = host.networks.up(false).await?;
    assert!(applied.changed);
    let wanted = state.firewall();
    assert_eq!(
        host.chain("filter", "MM-CLUSTER"),
        Some(wanted.filter.clone())
    );
    assert_eq!(
        host.chain("nat", "MM-CLUSTER-NAT"),
        Some(wanted.nat.clone())
    );
    // The engine's chain did not exist yet: mm creates it and the engine
    // keeps it.
    assert_eq!(
        host.chain("filter", "DOCKER-USER"),
        Some(vec!["-A DOCKER-USER -j MM-CLUSTER".to_owned()])
    );
    assert_eq!(
        host.chain("nat", "POSTROUTING"),
        Some(vec!["-A POSTROUTING -j MM-CLUSTER-NAT".to_owned()])
    );
    assert_eq!(std::fs::read_to_string(&arp)?, "1");
    assert_eq!(std::fs::read_to_string(&delay)?, "0");
    assert_eq!(host.ups(), 1);
    assert!(host.networks.drift().await?.is_empty());

    host.forget_commands();
    let again = host.networks.up(false).await?;
    assert!(!again.changed, "{:?}", again.actions);
    assert!(
        host.commands()
            .iter()
            .all(|command| command.starts_with("iptables -w -t")
                && (command.contains(" -S ") || command.contains(" -C "))),
        "only reads: {:?}",
        host.commands()
    );
    Ok(())
}

#[tokio::test]
async fn before_the_engine_only_the_chains_are_loaded() -> TestResult {
    let host = Host::new(true)?;
    host.networks.store(&state()?)?;
    let (arp, _) = host.bridge()?;
    let applied = host.networks.up(true).await?;
    assert_eq!(applied.actions.len(), 1, "{:?}", applied.actions);
    assert!(host.chain("filter", "MM-CLUSTER").is_some());
    assert_eq!(
        std::fs::read_to_string(&arp)?,
        "0\n",
        "bridges wait for the engine"
    );
    assert_eq!(host.ups(), 0, "the mesh has its own step before the engine");
    Ok(())
}

#[tokio::test]
async fn drift_names_the_rule_the_route_and_the_bridge() -> TestResult {
    let host = Host::new(false)?;
    let state = state()?;
    host.networks.store(&state)?;
    let (arp, _) = host.bridge()?;
    host.networks.up(false).await?;

    let removed = state.firewall().filter[2].clone();
    if let Ok(mut kernel) = host.kernel.state.lock() {
        let filter = kernel
            .chains
            .get_mut(&("filter".to_owned(), "MM-CLUSTER".to_owned()))
            .ok_or("no filter chain")?;
        filter.retain(|rule| *rule != removed);
        filter.push("-A MM-CLUSTER -j ACCEPT".to_owned());
        kernel
            .chains
            .insert(("nat".to_owned(), "POSTROUTING".to_owned()), Vec::new());
    }
    std::fs::write(&arp, "0\n")?;
    if let Ok(mut allowed) = host.mesh.allowed.lock() {
        allowed.clear();
        allowed.push(("app-2".to_owned(), vec!["10.90.0.2/32".to_owned()]));
    }
    if let Ok(mut kernel) = host.kernel.state.lock() {
        kernel.routes.clear();
    }
    let drift = host.networks.drift().await?;
    let details = drift
        .iter()
        .map(|drift| drift.detail.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        details,
        [
            format!("missing rule `{removed}`").as_str(),
            "unexpected rule `-A MM-CLUSTER -j ACCEPT`",
            "POSTROUTING does not enter MM-CLUSTER-NAT",
            "app-2 is not allowed to send from 10.64.1.0/24 through the mesh",
            "10.64.1.0/24 is not routed to app-2 through mm0",
            format!(
                "the bridge {} of network {BACKEND} does not answer for members on other nodes (proxy_arp is off)",
                bridge_name(BACKEND)
            )
            .as_str(),
        ]
    );
    assert_eq!(drift[0].kind, NetworkDriftKind::Firewall);
    assert_eq!(drift[3].kind, NetworkDriftKind::Route);
    assert_eq!(drift[5].kind, NetworkDriftKind::Bridge);

    // Bringing the host up again repairs the rules and the bridge; the
    // route is the mesh's part, which the fake only counts.
    let applied = host.networks.up(false).await?;
    assert!(
        applied
            .actions
            .iter()
            .any(|action| action.contains("loaded the firewall chains"))
    );
    assert_eq!(
        host.chain("filter", "MM-CLUSTER"),
        Some(state.firewall().filter)
    );
    assert_eq!(std::fs::read_to_string(&arp)?, "1");
    assert_eq!(host.ups(), 2);
    Ok(())
}

#[tokio::test]
async fn a_host_under_systemd_asks_the_unit_and_waits_for_it() -> TestResult {
    let host = Host::new(true)?;
    host.networks.store(&state()?)?;
    host.networks.converge().await?;
    assert_eq!(host.commands(), ["systemctl restart mm-network.service"]);
    assert!(
        host.chain("filter", "MM-CLUSTER").is_none(),
        "the daemon changes nothing itself"
    );

    if let Ok(mut kernel) = host.kernel.state.lock() {
        kernel.unit_fails = true;
    }
    let Err(error) = host.networks.converge().await else {
        return Err("a failed unit converged".into());
    };
    let message = error.to_string();
    assert!(
        message.contains("journalctl -u mm-network.service -n 30")
            && message.contains("Job for mm-network.service failed."),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn a_host_without_systemd_brings_itself_up() -> TestResult {
    let host = Host::new(false)?;
    host.networks.store(&state()?)?;
    host.networks.converge().await?;
    assert!(host.chain("filter", "MM-CLUSTER").is_some());
    assert_eq!(host.ups(), 1);
    assert!(
        !host
            .commands()
            .iter()
            .any(|command| command.starts_with("systemctl"))
    );
    Ok(())
}

#[tokio::test]
async fn leave_removes_exactly_what_mm_added() -> TestResult {
    let host = Host::new(false)?;
    host.networks.store(&state()?)?;
    host.networks.up(false).await?;
    if let Ok(mut kernel) = host.kernel.state.lock() {
        kernel
            .chains
            .entry(("filter".to_owned(), "DOCKER-USER".to_owned()))
            .or_default()
            .push("-A DOCKER-USER -s 192.0.2.0/24 -j DROP".to_owned());
    }
    let left = host.networks.leave().await?;
    assert_eq!(left.actions.len(), 3, "{:?}", left.actions);
    assert!(host.chain("filter", "MM-CLUSTER").is_none());
    assert!(host.chain("nat", "MM-CLUSTER-NAT").is_none());
    // Somebody else's rule in the engine's chain stays.
    assert_eq!(
        host.chain("filter", "DOCKER-USER"),
        Some(vec!["-A DOCKER-USER -s 192.0.2.0/24 -j DROP".to_owned()])
    );
    assert_eq!(host.networks.stored()?, None);
    assert!(!host.networks.leave().await?.changed);
    Ok(())
}

#[tokio::test]
async fn a_table_that_does_not_validate_is_not_stored() -> TestResult {
    let host = Host::new(false)?;
    let mut state = state()?;
    state
        .table
        .networks
        .get_mut(BACKEND)
        .ok_or("no backend")?
        .block = "10.93.0.0/20".parse()?;
    assert!(host.networks.store(&state).is_err());
    assert_eq!(host.networks.stored()?, None);
    Ok(())
}
