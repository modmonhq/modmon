//! The mesh adapter against a simulated kernel: `ip` and `wg` are answered
//! from an in-memory model that records every command line, so the tests
//! can say what changed, what did not, and what never appears anywhere.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine;
use zeroize::Zeroizing;

use super::kernel::KernelPeer;
use super::*;
use crate::app::ports::{CommandOutput, CommandSpec};
use crate::domain::repo_target::WireguardMesh;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn encode(bytes: [u8; 32]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The simulated curve: a public key is its private key with every byte
/// inverted. Enough to tell the two apart and to match pairs.
fn public_of(private_key: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(private_key.trim())
        .ok()?;
    let bytes: [u8; 32] = bytes.try_into().ok()?;
    Some(encode(bytes.map(|byte| !byte)))
}

#[derive(Debug, Default, Clone)]
struct Link {
    mtu: u16,
    up: bool,
    addresses: Vec<(String, u8)>,
    routes: Vec<String>,
    public_key: Option<String>,
    port: u16,
    peers: Vec<KernelPeer>,
}

#[derive(Default)]
struct Simulation {
    links: BTreeMap<String, Link>,
    generated: u8,
    /// Every command line as the process would see it in its arguments.
    commands: Vec<String>,
    /// What `ip -j -4 route show table all` prints.
    route_table: Vec<serde_json::Value>,
}

#[derive(Default)]
struct SimulatedKernel {
    state: Mutex<Simulation>,
}

fn ok(stdout: impl Into<String>) -> CommandOutput {
    CommandOutput {
        code: Some(0),
        stdout: Zeroizing::new(stdout.into()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

fn fails(stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(1),
        stdout: Zeroizing::new(String::new()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

impl Simulation {
    fn ip(&mut self, arguments: &[&str]) -> CommandOutput {
        let missing = |name: &str| fails(&format!("Device \"{name}\" does not exist."));
        match arguments {
            ["-j", "link", "show", "dev", name] => match self.links.get(*name) {
                Some(link) => ok(serde_json::json!([{
                    "ifname": name,
                    "mtu": link.mtu,
                    "flags": if link.up { vec!["POINTOPOINT", "NOARP", "UP", "LOWER_UP"] } else { vec!["POINTOPOINT", "NOARP"] },
                }])
                .to_string()),
                None => missing(name),
            },
            ["link", "add", "dev", name, "type", "wireguard"] => {
                self.links.insert(
                    (*name).to_owned(),
                    Link {
                        mtu: 1420,
                        ..Link::default()
                    },
                );
                ok("")
            }
            ["link", "delete", "dev", name] => match self.links.remove(*name) {
                Some(_) => ok(""),
                None => missing(name),
            },
            ["link", "set", "dev", name, "mtu", mtu] => match self.links.get_mut(*name) {
                Some(link) => {
                    link.mtu = mtu.parse().unwrap_or_default();
                    ok("")
                }
                None => missing(name),
            },
            ["link", "set", "dev", name, "up"] => match self.links.get_mut(*name) {
                Some(link) => {
                    link.up = true;
                    ok("")
                }
                None => missing(name),
            },
            ["-j", "-4", "address", "show", "dev", name] => match self.links.get(*name) {
                Some(link) => ok(serde_json::json!([{
                    "addr_info": link.addresses.iter().map(|(local, prefix)| serde_json::json!({
                        "family": "inet", "local": local, "prefixlen": prefix,
                    })).collect::<Vec<_>>(),
                }])
                .to_string()),
                None => missing(name),
            },
            ["address", action @ ("add" | "del"), address, "dev", name] => {
                let Some(link) = self.links.get_mut(*name) else {
                    return missing(name);
                };
                let (local, prefix) = address.split_once('/').unwrap_or((address, "32"));
                let entry = (local.to_owned(), prefix.parse().unwrap_or(32));
                link.addresses.retain(|held| *held != entry);
                if *action == "add" {
                    link.addresses.push(entry);
                }
                ok("")
            }
            ["-j", "-4", "route", "show", "table", "all"] => {
                ok(serde_json::Value::from(self.route_table.clone()).to_string())
            }
            ["-j", "-4", "route", "show", "dev", name, "proto", "static"] => {
                match self.links.get(*name) {
                    Some(link) => ok(serde_json::Value::from(
                        link.routes
                            .iter()
                            .map(|route| serde_json::json!({ "dst": route }))
                            .collect::<Vec<_>>(),
                    )
                    .to_string()),
                    None => missing(name),
                }
            }
            ["route", action @ ("replace" | "del"), prefix, "dev", name, "proto", "static"] => {
                let Some(link) = self.links.get_mut(*name) else {
                    return missing(name);
                };
                link.routes.retain(|held| held != prefix);
                if *action == "replace" {
                    link.routes.push((*prefix).to_owned());
                }
                ok("")
            }
            other => fails(&format!("unexpected ip {other:?}")),
        }
    }

    fn show(&self, name: &str, field: &str) -> CommandOutput {
        let Some(link) = self.links.get(name) else {
            return fails("Unable to access interface: No such device");
        };
        let per_peer = |value: &dyn Fn(&KernelPeer) -> String| {
            link.peers
                .iter()
                .map(|peer| format!("{}\t{}\n", peer.public_key, value(peer)))
                .collect::<String>()
        };
        ok(match field {
            "public-key" => format!("{}\n", link.public_key.as_deref().unwrap_or("(none)")),
            "listen-port" => format!("{}\n", link.port),
            "endpoints" => {
                per_peer(&|peer| peer.endpoint.clone().unwrap_or_else(|| "(none)".to_owned()))
            }
            "allowed-ips" => per_peer(&|peer| {
                if peer.allowed_ips.is_empty() {
                    "(none)".to_owned()
                } else {
                    peer.allowed_ips.join(" ")
                }
            }),
            "latest-handshakes" => per_peer(&|peer| peer.latest_handshake.to_string()),
            "transfer" => per_peer(&|peer| format!("{}\t{}", peer.received_bytes, peer.sent_bytes)),
            "persistent-keepalive" => per_peer(&|peer| {
                peer.keepalive
                    .map_or_else(|| "off".to_owned(), |seconds| seconds.to_string())
            }),
            // `dump` would print the private key: the adapter must never ask.
            other => return fails(&format!("the adapter asked for `wg show {other}`")),
        })
    }

    fn set(&mut self, name: &str, words: &[&str]) -> CommandOutput {
        let Some(link) = self.links.get_mut(name) else {
            return fails("Unable to modify interface: No such device");
        };
        match words {
            ["private-key", path] => {
                if !std::path::Path::new(path)
                    .parent()
                    .is_some_and(|parent| parent.ends_with("wireguard"))
                {
                    return fails(&format!("fopen: Permission denied: {path}"));
                }
                let Some(key) = std::fs::read_to_string(path)
                    .ok()
                    .and_then(|key| public_of(&key))
                else {
                    return fails("Key is not the correct length or format");
                };
                // A new key invalidates every session.
                for peer in &mut link.peers {
                    peer.latest_handshake = 0;
                }
                link.public_key = Some(key);
            }
            ["listen-port", port] => link.port = port.parse().unwrap_or_default(),
            ["peer", key, "remove"] => link.peers.retain(|peer| peer.public_key != *key),
            ["peer", key, options @ ..] => {
                let index = match link.peers.iter().position(|peer| peer.public_key == *key) {
                    Some(index) => index,
                    None => {
                        link.peers.push(KernelPeer {
                            public_key: (*key).to_owned(),
                            ..KernelPeer::default()
                        });
                        link.peers.len() - 1
                    }
                };
                for pair in options.chunks(2) {
                    let [option, value] = pair else {
                        return fails("an option without a value");
                    };
                    let peer = &mut link.peers[index];
                    match *option {
                        "persistent-keepalive" => peer.keepalive = value.parse().ok(),
                        "endpoint" => peer.endpoint = Some((*value).to_owned()),
                        "allowed-ips" => {
                            peer.allowed_ips = value.split(',').map(str::to_owned).collect();
                        }
                        other => return fails(&format!("unexpected wg option {other}")),
                    }
                }
            }
            other => return fails(&format!("unexpected wg set {other:?}")),
        }
        ok("")
    }
}

#[async_trait]
impl ProcessRunner for SimulatedKernel {
    async fn run(&self, mut specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let program = specification.program().to_owned();
        let arguments = specification
            .arguments()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let stdin = specification.take_sensitive_stdin();
        let mut state = self.state.lock().map_err(|_| MmError::State {
            message: "the simulated kernel is poisoned".to_owned(),
        })?;
        state
            .commands
            .push(format!("{program} {}", arguments.join(" ")));
        let words = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(match (program.as_str(), words.as_slice()) {
            ("ip", words) => state.ip(words),
            ("wg", ["genkey"]) => {
                state.generated += 1;
                ok(format!("{}\n", encode([state.generated; 32])))
            }
            ("wg", ["pubkey"]) => match stdin.as_deref().and_then(|key| public_of(key)) {
                Some(key) => ok(format!("{key}\n")),
                None => fails("wg: Key is not the correct length or format"),
            },
            ("wg", ["show", name, field]) => state.show(name, field),
            ("wg", ["set", name, words @ ..]) => state.set(name, words),
            other => fails(&format!("unexpected command {other:?}")),
        })
    }
}

struct Host {
    kernel: Arc<SimulatedKernel>,
    mesh: WireguardMeshHost,
    directory: tempfile::TempDir,
}

impl Host {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let kernel = Arc::new(SimulatedKernel::default());
        let mesh = WireguardMeshHost::with_legacy_directory(
            &directory.path().join("state"),
            directory.path().join("wireguard"),
            kernel.clone(),
        );
        Ok(Self {
            kernel,
            mesh,
            directory,
        })
    }

    fn link(&self, name: &str) -> Result<Option<Link>, Box<dyn std::error::Error>> {
        Ok(self
            .kernel
            .state
            .lock()
            .map_err(|_| "poisoned")?
            .links
            .get(name)
            .cloned())
    }

    fn mm0(&self) -> Result<Link, Box<dyn std::error::Error>> {
        self.link(MESH_INTERFACE)?
            .ok_or_else(|| "mm0 does not exist".into())
    }

    /// The commands run since the last call that change something.
    fn changes(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut state = self.kernel.state.lock().map_err(|_| "poisoned")?;
        let commands = std::mem::take(&mut state.commands);
        Ok(commands
            .into_iter()
            .filter(|command| {
                !(command.starts_with("wg show ")
                    || command.starts_with("ip -j ")
                    || command == "wg pubkey")
            })
            .collect())
    }

    fn all_commands(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(self
            .kernel
            .state
            .lock()
            .map_err(|_| "poisoned")?
            .commands
            .clone())
    }

    /// Marks every peer as having just shaken hands and roamed.
    fn shake_hands(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = self.kernel.state.lock().map_err(|_| "poisoned")?;
        let link = state.links.get_mut(MESH_INTERFACE).ok_or("no mm0")?;
        for peer in &mut link.peers {
            peer.latest_handshake = 1_700_000_000;
            peer.received_bytes = 1024;
            peer.sent_bytes = 2048;
        }
        Ok(())
    }

    fn private_key(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(
            std::fs::read_to_string(self.directory.path().join("state/mesh/private.key"))?
                .trim()
                .to_owned(),
        )
    }
}

fn peer(name: &str, byte: u8, last: u8) -> MeshPeer {
    MeshPeer {
        name: name.to_owned(),
        public_key: encode([byte; 32]),
        endpoint: format!("203.0.113.{}:51820", 10 + last),
        allowed_ips: vec![crate::domain::repo_target::Ipv4Cidr {
            address: Ipv4Addr::new(10, 90, 0, last),
            prefix: 32,
        }],
    }
}

/// `app-1` at 10.90.0.1 with `app-2` and `db-1` as peers.
fn state() -> MeshState {
    MeshState::new(
        &WireguardMesh::default(),
        Ipv4Addr::new(10, 90, 0, 1),
        vec![peer("app-2", 102, 2), peer("db-1", 103, 3)],
    )
}

/// What a node that holds nothing of the mesh reports: only its release.
fn nothing_held() -> MeshFacts {
    MeshFacts {
        release: Some(crate::domain::cluster_setup::HostRelease {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: crate::domain::remote::PROTOCOL_VERSION,
            build: super::own_build(),
        }),
        ..MeshFacts::default()
    }
}

#[tokio::test]
async fn a_new_node_makes_its_key_and_brings_the_interface_up() -> TestResult {
    let host = Host::new()?;
    assert_eq!(host.mesh.facts().await?, nothing_held());
    assert!(host.mesh.status().await?.interface.is_none());

    let public_key = host.mesh.ensure_key().await?;
    assert_eq!(public_of(&host.private_key()?), Some(public_key.clone()));
    assert_eq!(host.mesh.ensure_key().await?, public_key, "made once");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let key_file = host.directory.path().join("state/mesh/private.key");
        assert_eq!(
            std::fs::metadata(key_file)?.permissions().mode() & 0o777,
            0o600
        );
        let directory = host.directory.path().join("state/mesh");
        assert_eq!(
            std::fs::metadata(directory)?.permissions().mode() & 0o777,
            0o700
        );
    }

    let applied = host.mesh.apply(&state()).await?;
    assert!(applied.changed);
    let link = host.mm0()?;
    assert!(link.up);
    assert_eq!((link.mtu, link.port), (1420, 51820));
    assert_eq!(link.addresses, [("10.90.0.1".to_owned(), 24)]);
    assert_eq!(link.public_key, Some(public_key.clone()));
    assert_eq!(
        link.peers
            .iter()
            .map(|peer| (
                peer.public_key.clone(),
                peer.endpoint.clone(),
                peer.allowed_ips.join(","),
                peer.keepalive
            ))
            .collect::<Vec<_>>(),
        [
            (
                encode([102; 32]),
                Some("203.0.113.12:51820".to_owned()),
                "10.90.0.2/32".to_owned(),
                Some(25)
            ),
            (
                encode([103; 32]),
                Some("203.0.113.13:51820".to_owned()),
                "10.90.0.3/32".to_owned(),
                Some(25)
            ),
        ]
    );
    assert!(link.routes.is_empty(), "mesh addresses need no extra route");
    let facts = host.mesh.facts().await?;
    assert_eq!(facts.public_key, Some(public_key));
    assert_eq!(facts.state, Some(state()));
    Ok(())
}

#[tokio::test]
async fn applying_the_same_state_again_changes_nothing() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    host.shake_hands()?;
    host.changes()?;
    let stored = host.directory.path().join("state/mesh/mesh.json");
    let written = std::fs::metadata(&stored)?.modified()?;

    let again = host.mesh.apply(&state()).await?;
    assert_eq!(again, MeshApplied::default(), "nothing to do");
    assert_eq!(host.changes()?, Vec::<String>::new());
    assert_eq!(std::fs::metadata(&stored)?.modified()?, written);
    assert!(
        host.mm0()?
            .peers
            .iter()
            .all(|peer| peer.latest_handshake != 0),
        "every session survives"
    );
    assert_eq!(host.mesh.up().await?, MeshApplied::default());
    assert_eq!(host.changes()?, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn changing_one_peer_leaves_the_others_alone() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    host.shake_hands()?;
    host.changes()?;

    // app-2 rotated its key; db-1 is as it was.
    let mut rotated = state();
    rotated.peers[0].public_key = encode([109; 32]);
    let applied = host.mesh.apply(&rotated).await?;
    let changes = host.changes()?;
    assert_eq!(
        changes,
        [
            format!("wg set mm0 peer {} remove", encode([102; 32])),
            format!(
                "wg set mm0 peer {} persistent-keepalive 25 endpoint 203.0.113.12:51820 allowed-ips 10.90.0.2/32",
                encode([109; 32])
            ),
        ]
    );
    assert!(
        changes
            .iter()
            .all(|command| !command.contains(&encode([103; 32]))),
        "no command names the peer that did not change"
    );
    let link = host.mm0()?;
    let untouched = link
        .peers
        .iter()
        .find(|peer| peer.public_key == encode([103; 32]))
        .ok_or("db-1 is gone")?;
    assert_eq!(
        untouched.latest_handshake, 1_700_000_000,
        "its session survives"
    );
    assert_eq!(
        applied.actions,
        [
            "removed peer app-2 (key ZmZmZmZmZmZm…)",
            "added peer app-2 (key bW1tbW1tbW1t…)",
            "stored the mesh state"
        ]
    );

    // A node leaves: one removal, nothing else.
    let mut smaller = rotated.clone();
    smaller.peers.pop();
    host.mesh.apply(&smaller).await?;
    assert_eq!(
        host.changes()?,
        [format!("wg set mm0 peer {} remove", encode([103; 32]))]
    );
    // And joins again.
    host.mesh.apply(&rotated).await?;
    assert_eq!(host.changes()?.len(), 1);
    assert_eq!(host.mm0()?.peers.len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_peer_that_roamed_keeps_its_endpoint_until_the_plan_changes_it() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    {
        let mut kernel = host.kernel.state.lock().map_err(|_| "poisoned")?;
        let link = kernel.links.get_mut(MESH_INTERFACE).ok_or("no mm0")?;
        link.peers[1].endpoint = Some("198.51.100.77:40000".to_owned());
    }
    host.changes()?;
    assert!(!host.mesh.apply(&state()).await?.changed);
    assert!(!host.mesh.up().await?.changed);
    assert_eq!(host.changes()?, Vec::<String>::new());
    assert_eq!(
        host.mm0()?.peers[1].endpoint.as_deref(),
        Some("198.51.100.77:40000")
    );

    let mut moved = state();
    moved.peers[1].endpoint = "203.0.113.99:51820".to_owned();
    host.mesh.apply(&moved).await?;
    assert_eq!(
        host.changes()?,
        [format!(
            "wg set mm0 peer {} endpoint 203.0.113.99:51820",
            encode([103; 32])
        )]
    );
    Ok(())
}

#[tokio::test]
async fn routed_prefixes_become_allowed_addresses_and_routes() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    host.shake_hands()?;
    host.changes()?;

    let mut sliced = state();
    sliced.peers[1].allowed_ips.push("10.91.3.0/24".parse()?);
    host.mesh.apply(&sliced).await?;
    assert_eq!(
        host.changes()?,
        [
            format!(
                "wg set mm0 peer {} allowed-ips 10.90.0.3/32,10.91.3.0/24",
                encode([103; 32])
            ),
            "ip route replace 10.91.3.0/24 dev mm0 proto static".to_owned(),
        ]
    );
    let link = host.mm0()?;
    assert_eq!(link.routes, ["10.91.3.0/24"]);
    assert!(link.peers.iter().all(|peer| peer.latest_handshake != 0));

    host.mesh.apply(&state()).await?;
    assert_eq!(
        host.changes()?,
        [
            format!(
                "wg set mm0 peer {} allowed-ips 10.90.0.3/32",
                encode([103; 32])
            ),
            "ip route del 10.91.3.0/24 dev mm0 proto static".to_owned(),
        ]
    );
    assert!(host.mm0()?.routes.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_slices_a_deployment_stored_are_routed_and_setup_keeps_them() -> TestResult {
    use crate::domain::cluster_network::{NetworkNeed, NetworkTable, SealedNetworks};

    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    host.shake_hands()?;
    host.changes()?;

    // A deployment gives the network a slice on this node and on the
    // second peer, and stores the table.
    let names = state()
        .peers
        .iter()
        .map(|peer| peer.name.clone())
        .collect::<Vec<_>>();
    let space = "10.64.0.0/12".parse()?;
    let mut table = NetworkTable::default();
    for node in ["this-node", names[1].as_str()] {
        table.assign(
            space,
            &NetworkNeed {
                key: "0123456789ab-production-backend",
                node,
                network: "mm-engine-backend",
                internal: false,
            },
        )?;
    }
    let stored = SealedNetworks {
        space,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table,
        retired: Vec::new(),
    }
    .on_node("this-node");
    crate::adapters::cluster_network::NetworkStore::new(
        host.directory.path().join("state").join("networks"),
    )
    .write(&stored)?;

    host.mesh.up().await?;
    assert_eq!(
        host.changes()?,
        [
            format!(
                "wg set mm0 peer {} allowed-ips 10.90.0.3/32,10.64.1.0/24",
                encode([103; 32])
            ),
            "ip route replace 10.64.1.0/24 dev mm0 proto static".to_owned(),
        ]
    );
    // Setup applies the mesh state it built from the target, which knows
    // no slice: the route stays, and the stored mesh state stays setup's.
    let applied = host.mesh.apply(&state()).await?;
    assert!(!applied.changed, "{:?}", applied.actions);
    assert!(host.changes()?.is_empty());
    assert_eq!(host.mm0()?.routes, ["10.64.1.0/24"]);
    assert_eq!(host.mesh.facts().await?.state, Some(state()));
    Ok(())
}

#[tokio::test]
async fn a_changed_mtu_or_port_touches_the_interface_and_no_peer() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    host.shake_hands()?;
    host.changes()?;
    let mut tuned = state();
    tuned.mtu = 1380;
    tuned.port = 51900;
    let applied = host.mesh.apply(&tuned).await?;
    assert_eq!(
        host.changes()?,
        [
            "wg set mm0 listen-port 51900",
            "ip link set dev mm0 mtu 1380"
        ]
    );
    assert!(
        applied.actions.contains(&"mtu 1420 -> 1380".to_owned()),
        "{applied:?}"
    );
    assert!(
        host.mm0()?
            .peers
            .iter()
            .all(|peer| peer.latest_handshake != 0)
    );
    Ok(())
}

#[tokio::test]
async fn the_stored_state_brings_the_interface_back_after_a_reboot() -> TestResult {
    let host = Host::new()?;
    let public_key = host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    let before = host.mm0()?;
    // A reboot: the kernel forgets the interface, the files stay.
    host.kernel
        .state
        .lock()
        .map_err(|_| "poisoned")?
        .links
        .clear();
    let status = host.mesh.status().await?;
    assert!(!status.up);
    assert_eq!(status.peers.len(), 2);
    assert!(status.peers.iter().all(|peer| !peer.on_interface));

    let restored = host.mesh.up().await?;
    assert!(restored.changed);
    let after = host.mm0()?;
    assert_eq!(after.public_key, Some(public_key));
    assert_eq!(after.addresses, before.addresses);
    assert_eq!(
        (after.mtu, after.port, after.up),
        (before.mtu, before.port, true)
    );
    assert_eq!(
        after
            .peers
            .iter()
            .map(|peer| (&peer.public_key, &peer.endpoint, &peer.allowed_ips))
            .collect::<Vec<_>>(),
        before
            .peers
            .iter()
            .map(|peer| (&peer.public_key, &peer.endpoint, &peer.allowed_ips))
            .collect::<Vec<_>>()
    );

    // A host that never had a mesh has nothing to bring up.
    let empty = Host::new()?;
    let error = empty.mesh.up().await.err().ok_or("up without a state")?;
    assert!(
        error.to_string().contains("this host has no mesh"),
        "{error}"
    );
    Ok(())
}

const LEGACY: &str = "# Managed by mm target setup.\n[Interface]\nAddress = 10.90.0.1/24\nListenPort = 51820\nMTU = 1420\nPostUp = wg set %i private-key /etc/wireguard/%i.key\n\n[Peer]\nPublicKey = ZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmY=\nEndpoint = 203.0.113.12:51820\nAllowedIPs = 10.90.0.2/32\nPersistentKeepalive = 25\n\n[Peer]\nPublicKey = Z2dnZ2dnZ2dnZ2dnZ2dnZ2dnZ2dnZ2dnZ2dnZ2dnZ2c=\nEndpoint = 203.0.113.13:51820\nAllowedIPs = 10.90.0.3/32\nPersistentKeepalive = 25\n";

#[tokio::test]
async fn the_wg0_of_an_earlier_version_is_replaced_and_its_key_kept() -> TestResult {
    let host = Host::new()?;
    let legacy = host.directory.path().join("wireguard");
    std::fs::create_dir(&legacy)?;
    let old_private = encode([42; 32]);
    std::fs::write(legacy.join("wg0.conf"), LEGACY)?;
    std::fs::write(legacy.join("wg0.key"), format!("{old_private}\n"))?;
    host.kernel
        .state
        .lock()
        .map_err(|_| "poisoned")?
        .links
        .insert("wg0".to_owned(), Link::default());

    // The plan reads the node before anything changes on it.
    let facts = host.mesh.facts().await?;
    assert_eq!(facts.legacy_interface.as_deref(), Some("wg0"));
    assert_eq!(facts.public_key, public_of(&old_private));
    let read = facts
        .state
        .ok_or("the earlier configuration was not read")?;
    assert_eq!(read.interface, "wg0");
    assert_eq!(
        (read.address, read.prefix, read.port, read.mtu),
        (Ipv4Addr::new(10, 90, 0, 1), 24, 51820, 1420)
    );
    assert_eq!(
        read.peers
            .iter()
            .map(|peer| (
                peer.name.as_str(),
                peer.public_key.clone(),
                peer.endpoint.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("", encode([102; 32]), "203.0.113.12:51820"),
            ("", encode([103; 32]), "203.0.113.13:51820"),
        ]
    );
    assert_eq!(
        host.changes()?,
        Vec::<String>::new(),
        "reading changes nothing"
    );

    // The node keeps its key, so peers that are not migrated yet still know it.
    assert_eq!(
        host.mesh.ensure_key().await?,
        public_of(&old_private).ok_or("key")?
    );
    assert_eq!(host.private_key()?, old_private);

    let applied = host.mesh.apply(&state()).await?;
    assert!(host.link("wg0")?.is_none(), "the old interface is gone");
    assert_eq!(host.mm0()?.public_key, public_of(&old_private));
    assert!(!legacy.join("wg0.conf").exists() && !legacy.join("wg0.key").exists());
    assert!(
        applied
            .actions
            .iter()
            .any(|action| action.contains("removed the interface wg0")),
        "{applied:?}"
    );
    let first = host.changes()?;
    assert_eq!(
        first.first().map(String::as_str),
        Some("ip link delete dev wg0")
    );

    let facts = host.mesh.facts().await?;
    assert_eq!(facts.legacy_interface, None);
    assert_eq!(
        facts.state.map(|state| state.interface),
        Some("mm0".to_owned())
    );
    assert!(!host.mesh.apply(&state()).await?.changed, "converged");
    Ok(())
}

#[tokio::test]
async fn a_wireguard_configuration_mm_did_not_write_is_left_alone() -> TestResult {
    let host = Host::new()?;
    let legacy = host.directory.path().join("wireguard");
    std::fs::create_dir(&legacy)?;
    std::fs::write(legacy.join("wg0.conf"), "[Interface]\nListenPort = 51820\n")?;
    std::fs::write(legacy.join("wg0.key"), format!("{}\n", encode([42; 32])))?;
    host.kernel
        .state
        .lock()
        .map_err(|_| "poisoned")?
        .links
        .insert("wg0".to_owned(), Link::default());
    assert_eq!(host.mesh.facts().await?, nothing_held());
    let public_key = host.mesh.ensure_key().await?;
    assert_ne!(
        Some(public_key),
        public_of(&encode([42; 32])),
        "its key is not taken"
    );
    host.mesh.apply(&state()).await?;
    assert!(host.link("wg0")?.is_some());
    assert!(legacy.join("wg0.conf").exists() && legacy.join("wg0.key").exists());
    Ok(())
}

#[tokio::test]
async fn a_rotation_stages_a_key_and_switches_to_it() -> TestResult {
    let host = Host::new()?;
    let empty = host
        .mesh
        .stage_key()
        .await
        .err()
        .ok_or("staged without a mesh")?;
    assert!(
        empty.to_string().contains("this host has no mesh"),
        "{empty}"
    );

    let old = host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    host.shake_hands()?;
    let pending = host.mesh.stage_key().await?;
    assert_ne!(pending, old);
    assert_eq!(
        host.mesh.stage_key().await?,
        pending,
        "an interrupted rotation resumes"
    );
    let facts = host.mesh.facts().await?;
    assert_eq!(
        (
            facts.public_key.as_deref(),
            facts.pending_public_key.as_deref()
        ),
        (Some(old.as_str()), Some(pending.as_str()))
    );
    assert_eq!(
        host.mm0()?.public_key,
        Some(old.clone()),
        "still on the old key"
    );
    assert!(
        host.mm0()?
            .peers
            .iter()
            .all(|peer| peer.latest_handshake != 0)
    );

    assert_eq!(host.mesh.activate_key().await?, pending);
    assert_eq!(host.mm0()?.public_key, Some(pending.clone()));
    assert_eq!(host.mm0()?.peers.len(), 2, "the peers stay configured");
    let facts = host.mesh.facts().await?;
    assert_eq!(
        (facts.public_key, facts.pending_public_key),
        (Some(pending.clone()), None)
    );
    assert!(
        host.mesh.activate_key().await.is_err(),
        "nothing is pending now"
    );
    assert!(!host.mesh.apply(&state()).await?.changed);
    assert_eq!(host.mesh.ensure_key().await?, pending);
    Ok(())
}

#[tokio::test]
async fn status_reports_each_peer_by_name_with_handshake_and_traffic() -> TestResult {
    let host = Host::new()?;
    let public_key = host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    {
        let mut kernel = host.kernel.state.lock().map_err(|_| "poisoned")?;
        let link = kernel.links.get_mut(MESH_INTERFACE).ok_or("no mm0")?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        link.peers[0].latest_handshake = now - 30;
        link.peers[0].received_bytes = 4096;
        link.peers[0].sent_bytes = 8192;
        link.peers[0].endpoint = Some("198.51.100.5:7000".to_owned());
        // A peer somebody added by hand.
        link.peers.push(KernelPeer {
            public_key: encode([200; 32]),
            allowed_ips: vec!["10.90.0.50/32".to_owned()],
            ..KernelPeer::default()
        });
    }
    let status = host.mesh.status().await?;
    assert_eq!(status.interface.as_deref(), Some("mm0"));
    assert!(status.up);
    assert_eq!(status.address, Some(Ipv4Addr::new(10, 90, 0, 1)));
    assert_eq!((status.port, status.mtu), (Some(51820), Some(1420)));
    assert_eq!(status.public_key, Some(public_key));
    let first = &status.peers[0];
    assert_eq!(first.name.as_deref(), Some("app-2"));
    assert_eq!(
        first.endpoint.as_deref(),
        Some("198.51.100.5:7000"),
        "where it sends now"
    );
    assert_eq!(
        first.configured_endpoint.as_deref(),
        Some("203.0.113.12:51820")
    );
    assert!(
        first
            .handshake_age_seconds
            .is_some_and(|age| (30..40).contains(&age))
    );
    assert_eq!((first.received_bytes, first.sent_bytes), (4096, 8192));
    assert!(first.connected());
    let second = &status.peers[1];
    assert_eq!(
        (second.name.as_deref(), second.handshake_age_seconds),
        (Some("db-1"), None)
    );
    assert!(!second.connected());
    let unknown = &status.peers[2];
    assert_eq!(unknown.name, None);
    assert!(unknown.on_interface);
    Ok(())
}

#[tokio::test]
async fn leaving_removes_the_interface_the_state_and_the_key() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    host.mesh.apply(&state()).await?;
    let left = host.mesh.leave().await?;
    assert!(left.changed);
    assert!(host.link("mm0")?.is_none());
    assert!(!host.directory.path().join("state/mesh").exists());
    assert_eq!(host.mesh.facts().await?, nothing_held());
    assert!(!host.mesh.leave().await?.changed, "leaving twice is fine");
    Ok(())
}

#[tokio::test]
async fn the_private_key_is_in_one_file_and_nowhere_else() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    let applied = host.mesh.apply(&state()).await?;
    let old_private = host.private_key()?;
    host.mesh.stage_key().await?;
    let pending_private =
        std::fs::read_to_string(host.directory.path().join("state/mesh/next.key"))?
            .trim()
            .to_owned();
    let facts = host.mesh.facts().await?;
    let status = host.mesh.status().await?;
    host.mesh.activate_key().await?;
    let reapplied = host.mesh.up().await?;

    let mut shown = host.all_commands()?;
    shown.push(serde_json::to_string(&facts)?);
    shown.push(serde_json::to_string(&status)?);
    shown.push(serde_json::to_string(&applied)?);
    shown.push(serde_json::to_string(&reapplied)?);
    shown.push(std::fs::read_to_string(
        host.directory.path().join("state/mesh/mesh.json"),
    )?);
    shown.push(std::fs::read_to_string(
        host.directory.path().join("state/mesh/public.key"),
    )?);
    for private_key in [&old_private, &pending_private] {
        assert_eq!(private_key.len(), 44);
        for text in &shown {
            assert!(
                !text.contains(private_key.as_str()),
                "a private key in: {text}"
            );
        }
    }
    let key_file = host.directory.path().join("wireguard/mm0.key");
    assert!(
        shown.iter().any(|command| {
            command == &format!("wg set mm0 private-key {}", key_file.display())
        })
    );
    assert!(!key_file.exists());
    assert!(shown.iter().all(|command| !command.contains("dump")));
    Ok(())
}

#[tokio::test]
async fn a_state_for_another_interface_or_without_a_key_is_refused() -> TestResult {
    let host = Host::new()?;
    let no_key = host
        .mesh
        .apply(&state())
        .await
        .err()
        .ok_or("applied without a key")?;
    assert!(
        no_key.to_string().contains("`mmd mesh key` creates it"),
        "{no_key}"
    );
    host.mesh.ensure_key().await?;
    let mut foreign = state();
    foreign.interface = "eth0".to_owned();
    assert!(matches!(
        host.mesh.apply(&foreign).await,
        Err(MmError::Security { .. })
    ));
    assert_eq!(
        host.changes()?
            .iter()
            .filter(|command| command.contains("eth0"))
            .count(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn an_endpoint_that_does_not_resolve_leaves_the_rest_of_the_mesh_up() -> TestResult {
    let host = Host::new()?;
    host.mesh.ensure_key().await?;
    let mut named = state();
    named.peers[1].endpoint = "db-1.invalid:51820".to_owned();
    let error = host
        .mesh
        .apply(&named)
        .await
        .err()
        .ok_or("resolved .invalid")?;
    let message = error.to_string();
    assert!(
        message.contains("db-1 (db-1.invalid) does not resolve"),
        "{message}"
    );
    let link = host.mm0()?;
    assert!(link.up);
    assert_eq!(link.peers.len(), 2, "both peers are configured");
    assert_eq!(
        link.peers[0].endpoint.as_deref(),
        Some("203.0.113.12:51820")
    );
    assert_eq!(
        link.peers[1].endpoint, None,
        "it waits for db-1 to connect first"
    );
    assert_eq!(
        host.mesh.facts().await?.state,
        Some(named),
        "the state is stored for the next try"
    );
    Ok(())
}

#[test]
fn the_earlier_configuration_parser_ignores_what_it_cannot_read() {
    assert!(legacy::parse("# Managed by mm target setup.\n").is_none());
    let partial = legacy::parse(
        "[Interface]\nAddress = 10.90.0.1/24\nListenPort = 51820\nMTU = 1420\n\n[Peer]\nPublicKey = abc\n",
    );
    assert_eq!(
        partial.map(|state| state.peers.len()),
        Some(0),
        "a peer without an endpoint"
    );
}

#[test]
fn a_peer_in_place_gets_no_command() {
    let desired = state().peers;
    let held = desired
        .iter()
        .map(|peer| KernelPeer {
            public_key: peer.public_key.clone(),
            endpoint: Some("192.0.2.1:1".to_owned()),
            allowed_ips: peer.allowed_ips.iter().map(ToString::to_string).collect(),
            keepalive: Some(25),
            ..KernelPeer::default()
        })
        .collect::<Vec<_>>();
    let resolved = desired
        .iter()
        .map(|peer| (peer.name.clone(), peer.endpoint.clone()))
        .collect::<BTreeMap<_, _>>();
    assert!(
        sync::peer_commands("mm0", &held, &desired, Some(&desired), &resolved).is_empty(),
        "a roamed endpoint is not a difference"
    );
    // Without an earlier state the configured endpoint is put back.
    let commands = sync::peer_commands("mm0", &held, &desired, None, &resolved);
    assert_eq!(commands.len(), 2);
    assert!(
        commands
            .iter()
            .all(|command| command.action.contains("(endpoint)"))
    );
}

#[tokio::test]
async fn a_node_reports_the_addresses_and_routes_it_has_outside_mm() -> TestResult {
    use crate::domain::cluster_network::{HostRange, HostRangeKind};

    let host = Host::new()?;
    // As Debian prints it on a host with a provider network on eth1, the
    // engine's default bridge, the mesh, and one cluster network.
    host.kernel
        .state
        .lock()
        .map_err(|_| "poisoned")?
        .route_table = serde_json::from_str(
        r#"[
        {"dst":"default","gateway":"203.0.113.1","dev":"eth0","flags":[]},
        {"dst":"203.0.113.0/24","dev":"eth0","protocol":"kernel","scope":"link","prefsrc":"203.0.113.11","flags":[]},
        {"dst":"10.70.0.0/20","dev":"eth1","protocol":"kernel","scope":"link","prefsrc":"10.70.0.5","flags":[]},
        {"dst":"10.64.1.0/24","dev":"mm0","protocol":"static","scope":"link","flags":[]},
        {"dst":"10.64.0.0/20","dev":"mmn-0123456789a","protocol":"kernel","scope":"link","prefsrc":"10.64.0.1","flags":[]},
        {"dst":"10.90.0.0/24","dev":"mm0","protocol":"kernel","scope":"link","prefsrc":"10.90.0.1","flags":[]},
        {"dst":"172.17.0.0/16","dev":"docker0","protocol":"kernel","scope":"link","prefsrc":"172.17.0.1","flags":["linkdown"]},
        {"type":"unreachable","dst":"10.99.0.0/16","flags":[]},
        {"type":"local","dst":"10.64.0.1","dev":"mmn-0123456789a","table":"local","protocol":"kernel","scope":"host","prefsrc":"10.64.0.1","flags":[]},
        {"type":"local","dst":"10.70.0.5","dev":"eth1","table":"local","protocol":"kernel","scope":"host","prefsrc":"10.70.0.5","flags":[]},
        {"type":"broadcast","dst":"10.70.15.255","dev":"eth1","table":"local","protocol":"kernel","scope":"link","prefsrc":"10.70.0.5","flags":[]},
        {"type":"local","dst":"10.90.0.1","dev":"mm0","table":"local","protocol":"kernel","scope":"host","prefsrc":"10.90.0.1","flags":[]},
        {"type":"local","dst":"127.0.0.0/8","dev":"lo","table":"local","protocol":"kernel","scope":"host","prefsrc":"127.0.0.1","flags":[]},
        {"type":"local","dst":"203.0.113.11","dev":"eth0","table":"local","protocol":"kernel","scope":"host","prefsrc":"203.0.113.11","flags":[]}
        ]"#,
    )?;
    let range = |range: &str, kind, interface: Option<&str>| -> Result<HostRange, String> {
        Ok(HostRange {
            range: range.parse()?,
            kind,
            interface: interface.map(str::to_owned),
        })
    };
    let ranges = host.mesh.host_ranges().await?;
    assert_eq!(
        ranges,
        [
            range("203.0.113.0/24", HostRangeKind::Route, Some("eth0"))?,
            range("10.70.0.0/20", HostRangeKind::Route, Some("eth1"))?,
            range("172.17.0.0/16", HostRangeKind::Route, Some("docker0"))?,
            range("10.99.0.0/16", HostRangeKind::Route, None)?,
            range("10.70.0.5/32", HostRangeKind::Address, Some("eth1"))?,
            range("203.0.113.11/32", HostRangeKind::Address, Some("eth0"))?,
        ],
        "not the default route, the loopback, the mesh, or a cluster network's bridge"
    );
    // `mmd mesh show` carries them.
    assert_eq!(host.mesh.facts().await?.host_ranges, ranges);
    Ok(())
}
