//! The firewall chains that keep the mesh between the hosts: the rules as
//! rendered, what counts as drift, and when the host loads and removes
//! them.

use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::cluster_network::NetworkStore;
use mm::adapters::mesh::GuardedMesh;
use mm::app::ports::{CommandOutput, CommandSpec, MeshHost, ProcessRunner};
use mm::domain::cluster_network::{
    NetworkNeed, NetworkTable, NodeNetworks, bridge_name, forward_hook,
};
use mm::domain::error::MmError;
use mm::domain::mesh::{
    MESH_FORWARD_CHAIN, MESH_INPUT_CHAIN, MeshApplied, MeshFacts, MeshGuard, MeshNodeReport,
    MeshNodeStatus, MeshPeer, MeshState, mesh_diagnostics,
};
use mm::domain::repo_target::Ipv4Cidr;
use mm::domain::runtime::RuntimeKind;
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const BACKEND: &str = "019c527b6242-production-backend";

/// The mesh of node 1 in a cluster of three, `10.90.0.1` to `.3`.
fn mesh() -> Result<MeshState, String> {
    let peers = (2..=3_u8)
        .map(|node| {
            Ok(MeshPeer {
                name: format!("app-{node}"),
                // Two keys that differ: 32 bytes each in base64.
                public_key: if node == 2 { "E" } else { "I" }.repeat(43) + "=",
                endpoint: format!("203.0.113.{node}:51820"),
                allowed_ips: vec![format!("10.90.0.{node}/32").parse::<Ipv4Cidr>()?],
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(MeshState {
        interface: "mm0".to_owned(),
        address: Ipv4Addr::new(10, 90, 0, 1),
        prefix: 24,
        port: 51820,
        mtu: 1420,
        peers,
    })
}

/// Node 1 holding the network `backend` together with node 2.
fn networks() -> Result<NodeNetworks, String> {
    let space: Ipv4Cidr = "10.64.0.0/12".parse()?;
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
    Ok(NodeNetworks {
        node: "app-1".to_owned(),
        space,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table,
    })
}

#[test]
fn a_node_without_cluster_networks_lets_nothing_be_forwarded_into_the_mesh() -> TestResult {
    let guard = MeshGuard::of(&mesh()?, None);
    assert_eq!(
        guard.forward,
        [
            "-A MM-MESH -m conntrack --ctstate RELATED,ESTABLISHED -m comment --comment \"mm replies\" -j RETURN",
            "-A MM-MESH -o mm0 -m comment --comment \"mm mesh guard\" -j DROP",
        ]
    );
    assert_eq!(
        guard.input,
        [
            "-A MM-MESH-IN -i lo -m comment --comment \"mm mesh guard\" -j RETURN",
            "-A MM-MESH-IN -s 10.90.0.0/24 -d 10.90.0.1/32 -i mm0 -m comment --comment \"mm mesh guard\" -j RETURN",
            "-A MM-MESH-IN -d 10.90.0.1/32 -m comment --comment \"mm mesh guard\" -j DROP",
        ]
    );
    Ok(())
}

#[test]
fn a_network_sends_from_its_slice_to_its_block_and_is_asked_on_its_bridge() -> TestResult {
    let networks = networks()?;
    let guard = MeshGuard::of(&mesh()?, Some(&networks));
    let bridge = bridge_name(BACKEND);
    let name = format!("-m comment --comment \"mm network {BACKEND}\"");
    assert_eq!(
        guard.forward,
        [
            "-A MM-MESH -m conntrack --ctstate RELATED,ESTABLISHED -m comment --comment \"mm replies\" -j RETURN".to_owned(),
            format!("-A MM-MESH -s 10.64.0.0/24 -d 10.64.0.0/20 -o mm0 {name} -j RETURN"),
            "-A MM-MESH -o mm0 -m comment --comment \"mm mesh guard\" -j DROP".to_owned(),
        ]
    );
    assert_eq!(
        guard.input,
        [
            "-A MM-MESH-IN -i lo -m comment --comment \"mm mesh guard\" -j RETURN".to_owned(),
            "-A MM-MESH-IN -s 10.90.0.0/24 -d 10.90.0.1/32 -i mm0 -m comment --comment \"mm mesh guard\" -j RETURN".to_owned(),
            format!("-A MM-MESH-IN -d 10.64.0.1/32 -i {bridge} {name} -j RETURN"),
            "-A MM-MESH-IN -d 10.90.0.1/32 -m comment --comment \"mm mesh guard\" -j DROP".to_owned(),
            format!("-A MM-MESH-IN -d 10.64.0.1/32 {name} -j DROP"),
        ]
    );
    // The drop is the last word in both chains: nothing after it lets a
    // packet through.
    assert!(
        guard
            .forward
            .last()
            .is_some_and(|rule| rule.ends_with("-j DROP"))
    );
    let restore = guard.restore_input(forward_hook(RuntimeKind::DockerCompose));
    assert!(restore.starts_with("*filter\n:MM-MESH - [0:0]\n:MM-MESH-IN - [0:0]\n"));
    assert!(restore.ends_with("COMMIT\n"));
    assert_eq!(
        restore.lines().count(),
        3 + guard.forward.len() + guard.input.len() + 1
    );
    Ok(())
}

/// The filter table as `iptables -S` prints it on a host that holds
/// `guard` and enters both chains.
fn listing(guard: &MeshGuard) -> String {
    let mut lines = vec![
        "-P INPUT ACCEPT".to_owned(),
        "-P FORWARD DROP".to_owned(),
        "-N DOCKER-USER".to_owned(),
        format!("-N {MESH_FORWARD_CHAIN}"),
        format!("-N {MESH_INPUT_CHAIN}"),
        format!("-A INPUT -j {MESH_INPUT_CHAIN}"),
        "-A FORWARD -j DOCKER-USER".to_owned(),
        format!("-A DOCKER-USER -j {MESH_FORWARD_CHAIN}"),
    ];
    lines.extend(guard.forward.iter().cloned());
    lines.extend(guard.input.iter().cloned());
    lines.join("\n")
}

#[test]
fn drift_names_what_is_missing_unexpected_or_not_entered() -> TestResult {
    let guard = MeshGuard::of(&mesh()?, Some(&networks()?));
    let held = listing(&guard);
    assert_eq!(guard.drift(&held, "DOCKER-USER"), Vec::<String>::new());

    assert_eq!(
        guard.drift("-P INPUT ACCEPT\n-N DOCKER-USER", "DOCKER-USER"),
        [
            "the chain MM-MESH is missing",
            "the chain MM-MESH-IN is missing"
        ]
    );

    let drop = "-A MM-MESH -o mm0 -m comment --comment \"mm mesh guard\" -j DROP";
    let without = held.replace(&format!("{drop}\n"), "");
    assert_eq!(
        guard.drift(&without, "DOCKER-USER"),
        [format!("missing rule `{drop}`")]
    );

    let extra = format!("{held}\n-A MM-MESH-IN -s 198.51.100.7/32 -j ACCEPT");
    assert_eq!(
        guard.drift(&extra, "DOCKER-USER"),
        ["unexpected rule `-A MM-MESH-IN -s 198.51.100.7/32 -j ACCEPT`"]
    );

    let left = held.replace("-A DOCKER-USER -j MM-MESH\n", "");
    assert_eq!(
        guard.drift(&left, "DOCKER-USER"),
        ["DOCKER-USER does not enter MM-MESH"]
    );
    let left = held.replace("-A INPUT -j MM-MESH-IN\n", "");
    assert_eq!(
        guard.drift(&left, "DOCKER-USER"),
        ["INPUT does not enter MM-MESH-IN"]
    );

    // The rules of a node that held a network are drift once it holds none.
    let alone = MeshGuard::of(&mesh()?, None);
    let drift = alone.drift(&held, "DOCKER-USER");
    assert_eq!(drift.len(), 3, "{drift:?}");
    assert!(
        drift
            .iter()
            .all(|line| line.starts_with("unexpected rule") && line.contains(BACKEND))
    );
    Ok(())
}

#[test]
fn a_node_whose_rules_drifted_is_a_finding_with_the_command_that_loads_them() -> TestResult {
    let status = |drift: Vec<String>| MeshNodeReport {
        name: "app-1".to_owned(),
        host: "203.0.113.1".to_owned(),
        reachable: true,
        problem: None,
        status: Some(MeshNodeStatus {
            interface: Some("mm0".to_owned()),
            up: true,
            guard_drift: drift,
            ..MeshNodeStatus::default()
        }),
    };
    assert!(mesh_diagnostics("production", &[status(Vec::new())]).is_empty());
    let found = mesh_diagnostics(
        "production",
        &[status(vec![
            "DOCKER-USER does not enter MM-MESH".to_owned(),
        ])],
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code, "MM_MESH_GUARD");
    assert!(
        found[0].message.contains("node 'app-1'"),
        "{}",
        found[0].message
    );
    assert!(
        found[0]
            .message
            .contains("DOCKER-USER does not enter MM-MESH")
    );
    assert!(
        found[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("systemctl reload mm-mesh")),
        "{:?}",
        found[0].help
    );
    Ok(())
}

/// What happened on the simulated host, in order.
type Events = Arc<Mutex<Vec<String>>>;

/// A packet filter that prints one listing per table and records every
/// command.
struct Filter {
    listing: Mutex<String>,
    mangle: Mutex<String>,
    events: Events,
}

#[async_trait]
impl ProcessRunner for Filter {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let record = specification.redacted_record();
        let command = format!("{} {}", record.program, record.arguments.join(" "));
        let poisoned = || MmError::State {
            message: "the test filter's lock was poisoned".to_owned(),
        };
        self.events
            .lock()
            .map_err(|_| poisoned())?
            .push(command.clone());
        let stdout = match command.as_str() {
            "iptables -w -t filter -S" => self.listing.lock().map_err(|_| poisoned())?.clone(),
            "iptables -w -t mangle -S" => self.mangle.lock().map_err(|_| poisoned())?.clone(),
            _ => String::new(),
        };
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

/// A mesh interface that records when it is brought up and removed.
struct Interface {
    state: Option<MeshState>,
    events: Events,
}

impl Interface {
    fn did(&self, what: &str) -> Result<MeshApplied, MmError> {
        self.events
            .lock()
            .map_err(|_| MmError::State {
                message: "the test interface's lock was poisoned".to_owned(),
            })?
            .push(what.to_owned());
        Ok(MeshApplied {
            changed: true,
            actions: vec![what.to_owned()],
        })
    }
}

fn unsupported<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not part of this test".to_owned(),
    })
}

#[async_trait]
impl MeshHost for Interface {
    async fn facts(&self) -> Result<MeshFacts, MmError> {
        unsupported()
    }
    fn stored(&self) -> Result<Option<MeshState>, MmError> {
        Ok(self.state.clone())
    }
    async fn ensure_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn apply(&self, _desired: &MeshState) -> Result<MeshApplied, MmError> {
        self.did("interface applied")
    }
    async fn up(&self) -> Result<MeshApplied, MmError> {
        self.did("interface up")
    }
    async fn status(&self) -> Result<MeshNodeStatus, MmError> {
        Ok(MeshNodeStatus {
            interface: Some("mm0".to_owned()),
            up: true,
            ..MeshNodeStatus::default()
        })
    }
    async fn stage_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn activate_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn leave(&self) -> Result<MeshApplied, MmError> {
        self.did("interface removed")
    }
}

struct Host {
    mesh: GuardedMesh,
    filter: Arc<Filter>,
    events: Events,
}

impl Host {
    fn new(root: &Path, state: Option<MeshState>, held: &str) -> Self {
        Self::on(
            RuntimeKind::DockerCompose,
            root,
            state,
            held,
            "-P FORWARD ACCEPT",
        )
    }

    /// A host whose engine is `engine`: `held` is its filter table,
    /// `held_mangle` its mangle table.
    fn on(
        engine: RuntimeKind,
        root: &Path,
        state: Option<MeshState>,
        held: &str,
        held_mangle: &str,
    ) -> Self {
        let events = Events::default();
        let filter = Arc::new(Filter {
            listing: Mutex::new(held.to_owned()),
            mangle: Mutex::new(held_mangle.to_owned()),
            events: events.clone(),
        });
        let interface = Arc::new(Interface {
            state,
            events: events.clone(),
        });
        Self {
            mesh: GuardedMesh::new(root, interface, filter.clone()).for_engine(engine),
            filter,
            events,
        }
    }

    /// What happened since the last call, without the reads.
    fn changes(&self) -> Result<Vec<String>, &'static str> {
        let mut events = self.events.lock().map_err(|_| "poisoned")?;
        Ok(std::mem::take(&mut *events)
            .into_iter()
            .filter(|event| !event.ends_with(" -S"))
            .collect())
    }

    fn holds(&self, listing: &str) -> Result<(), &'static str> {
        listing.clone_into(&mut *self.filter.listing.lock().map_err(|_| "poisoned")?);
        Ok(())
    }

    fn holds_in_mangle(&self, listing: &str) -> Result<(), &'static str> {
        listing.clone_into(&mut *self.filter.mangle.lock().map_err(|_| "poisoned")?);
        Ok(())
    }
}

#[tokio::test]
async fn the_chains_are_loaded_before_the_interface_and_left_alone_when_in_place() -> TestResult {
    let root = temp_dirs::tempdir()?;
    let host = Host::new(root.path(), Some(mesh()?), "-P INPUT ACCEPT");
    let applied = host.mesh.up().await?;
    assert_eq!(
        host.changes()?,
        [
            "iptables-restore --noflush --wait 5",
            "iptables -w -t filter -N DOCKER-USER",
            "iptables -w -t filter -I DOCKER-USER 1 -j MM-MESH",
            "iptables -w -t filter -I INPUT 1 -j MM-MESH-IN",
            "interface up",
        ]
    );
    assert_eq!(
        applied.actions,
        [
            "loaded the firewall chains MM-MESH and MM-MESH-IN (the chain MM-MESH is missing; the chain MM-MESH-IN is missing)",
            "interface up",
        ]
    );

    // In place: the interface is brought up and nothing is loaded.
    host.holds(&listing(&MeshGuard::of(&mesh()?, None)))?;
    host.mesh.up().await?;
    assert_eq!(host.changes()?, ["interface up"]);
    assert!(host.mesh.status().await?.guard_drift.is_empty());

    // Setup's apply loads them the same way, before the interface exists.
    host.holds("-P INPUT ACCEPT\n-N DOCKER-USER")?;
    host.mesh.apply(&mesh()?).await?;
    let changes = host.changes()?;
    assert_eq!(
        changes.first().map(String::as_str),
        Some("iptables-restore --noflush --wait 5")
    );
    assert_eq!(
        changes.last().map(String::as_str),
        Some("interface applied")
    );
    assert!(
        !changes
            .iter()
            .any(|change| change.contains("-N DOCKER-USER"))
    );
    Ok(())
}

#[tokio::test]
async fn the_chains_follow_the_networks_the_node_holds() -> TestResult {
    let root = temp_dirs::tempdir()?;
    let alone = listing(&MeshGuard::of(&mesh()?, None));
    let host = Host::new(root.path(), Some(mesh()?), &alone);
    assert!(host.mesh.status().await?.guard_drift.is_empty());

    // A deployment stored a network: the node's status says the chains
    // lack its rules, and the next `up` (which `mmd network up` ends with)
    // loads them.
    let store = NetworkStore::new(root.path().join("networks"));
    store.write(&networks()?)?;
    let drift = host.mesh.status().await?.guard_drift;
    assert_eq!(drift.len(), 3, "{drift:?}");
    assert!(
        drift
            .iter()
            .all(|line| line.starts_with("missing rule") && line.contains(BACKEND))
    );
    let applied = host.mesh.up().await?;
    assert_eq!(
        host.changes()?,
        ["iptables-restore --noflush --wait 5", "interface up"]
    );
    assert!(
        applied.actions[0].contains("missing rule"),
        "{:?}",
        applied.actions
    );

    // The network goes away: its rules are unexpected and are taken out.
    host.holds(&listing(&MeshGuard::of(&mesh()?, Some(&networks()?))))?;
    assert!(host.mesh.status().await?.guard_drift.is_empty());
    assert!(store.remove()?);
    let drift = host.mesh.status().await?.guard_drift;
    assert!(
        drift.iter().all(|line| line.starts_with("unexpected rule")),
        "{drift:?}"
    );
    host.mesh.up().await?;
    assert_eq!(
        host.changes()?,
        ["iptables-restore --noflush --wait 5", "interface up"]
    );
    Ok(())
}

#[tokio::test]
async fn leaving_the_mesh_removes_the_chains_after_the_interface() -> TestResult {
    let root = temp_dirs::tempdir()?;
    // The listing never changes in this simulation, so each chain is left
    // once: the jump is removed until the listing no longer shows it.
    let held = "-N MM-MESH\n-N MM-MESH-IN\n-N DOCKER-USER";
    let host = Host::new(root.path(), Some(mesh()?), held);
    let left = host.mesh.leave().await?;
    assert_eq!(
        host.changes()?,
        [
            "interface removed",
            "iptables -w -t filter -F MM-MESH",
            "iptables -w -t filter -X MM-MESH",
            "iptables -w -t filter -F MM-MESH-IN",
            "iptables -w -t filter -X MM-MESH-IN",
        ]
    );
    assert_eq!(
        left.actions,
        [
            "interface removed",
            "removed the firewall chain MM-MESH",
            "removed the firewall chain MM-MESH-IN",
        ]
    );

    // A host that holds no mesh and no chains: nothing to load or remove.
    let bare = Host::new(root.path(), None, "-P INPUT ACCEPT");
    bare.mesh.leave().await?;
    assert_eq!(bare.changes()?, ["interface removed"]);
    assert!(bare.mesh.status().await?.guard_drift.is_empty());
    Ok(())
}

#[test]
fn the_boot_unit_loads_the_chains_before_the_engine_and_the_daemon() {
    let unit = include_str!("../../packaging/systemd/mm-mesh.service");
    let before = unit
        .lines()
        .find(|line| line.starts_with("Before="))
        .unwrap_or_default();
    for started_later in [
        "mmd.service",
        "docker.service",
        "podman.service",
        "nfs-server.service",
    ] {
        assert!(before.contains(started_later), "{before}");
    }
    // `mmd mesh up` is the unit's first command: it loads the chains and
    // then brings the interface up.
    let first = unit
        .lines()
        .find(|line| line.starts_with("ExecStart="))
        .unwrap_or_default();
    assert!(first.ends_with(" mesh up"), "{first}");
    // After the engine, `mmd network up` ends with the same call, so the
    // chains follow a deployment's networks.
    let network = include_str!("../../packaging/systemd/mm-network.service");
    assert!(network.contains("ExecStart=/usr/libexec/mmd --config /etc/mm/mmd.json network up\n"));
}

/// The two tables of a Podman host that holds `guard`: the forward chain in
/// mangle, entered from its `FORWARD`, the input chain in filter.
fn podman_listings(guard: &MeshGuard) -> (String, String) {
    let mut mangle = vec![
        "-P FORWARD ACCEPT".to_owned(),
        format!("-N {MESH_FORWARD_CHAIN}"),
        format!("-A FORWARD -j {MESH_FORWARD_CHAIN}"),
    ];
    mangle.extend(guard.forward.iter().cloned());
    let mut filter = vec![
        "-P INPUT ACCEPT".to_owned(),
        format!("-N {MESH_INPUT_CHAIN}"),
        format!("-A INPUT -j {MESH_INPUT_CHAIN}"),
    ];
    filter.extend(guard.input.iter().cloned());
    (filter.join("\n"), mangle.join("\n"))
}

#[test]
fn on_podman_the_forward_chain_is_restored_into_the_mangle_table() -> TestResult {
    let guard = MeshGuard::of(&mesh()?, Some(&networks()?));
    let restore = guard.restore_input(forward_hook(RuntimeKind::PodmanCompose));
    let mut expected = format!("*mangle\n:{MESH_FORWARD_CHAIN} - [0:0]\n");
    for rule in &guard.forward {
        expected.push_str(&format!("{rule}\n"));
    }
    expected.push_str(&format!("COMMIT\n*filter\n:{MESH_INPUT_CHAIN} - [0:0]\n"));
    for rule in &guard.input {
        expected.push_str(&format!("{rule}\n"));
    }
    expected.push_str("COMMIT\n");
    assert_eq!(restore, expected);
    // Docker Engine keeps both in one filter section, as before.
    let docker = guard.restore_input(forward_hook(RuntimeKind::DockerCompose));
    assert!(docker.starts_with("*filter\n:MM-MESH - [0:0]\n:MM-MESH-IN - [0:0]\n"));
    assert_eq!(docker.matches("COMMIT").count(), 1);
    Ok(())
}

#[tokio::test]
async fn on_podman_the_chains_are_entered_from_mangle_forward_and_input() -> TestResult {
    let root = temp_dirs::tempdir()?;
    let host = Host::on(
        RuntimeKind::PodmanCompose,
        root.path(),
        Some(mesh()?),
        "-P INPUT ACCEPT",
        "-P FORWARD ACCEPT",
    );
    host.mesh.up().await?;
    // No DOCKER-USER: netavark has no chain for the host's rules, and
    // mangle's FORWARD is always there.
    assert_eq!(
        host.changes()?,
        [
            "iptables-restore --noflush --wait 5",
            "iptables -w -t mangle -I FORWARD 1 -j MM-MESH",
            "iptables -w -t filter -I INPUT 1 -j MM-MESH-IN",
            "interface up",
        ]
    );
    // Both tables are read for the status, and in place means no drift.
    let guard = MeshGuard::of(&mesh()?, None);
    let (filter, mangle) = podman_listings(&guard);
    host.holds(&filter)?;
    host.holds_in_mangle(&mangle)?;
    assert!(host.mesh.status().await?.guard_drift.is_empty());
    host.mesh.up().await?;
    assert_eq!(host.changes()?, ["interface up"]);
    // A chain a Docker host would use instead is drift on Podman.
    host.holds_in_mangle(&mangle.replace("-A FORWARD -j MM-MESH", "-A DOCKER-USER -j MM-MESH"))?;
    assert_eq!(
        host.mesh.status().await?.guard_drift,
        ["FORWARD does not enter MM-MESH"]
    );
    // Leaving takes each chain out of its own table. The listing never
    // changes in this simulation, so it holds the chains without the jumps.
    host.holds("-N MM-MESH-IN")?;
    host.holds_in_mangle("-N MM-MESH")?;
    host.mesh.leave().await?;
    assert_eq!(
        host.changes()?,
        [
            "interface removed",
            "iptables -w -t mangle -F MM-MESH",
            "iptables -w -t mangle -X MM-MESH",
            "iptables -w -t filter -F MM-MESH-IN",
            "iptables -w -t filter -X MM-MESH-IN",
        ]
    );
    Ok(())
}
