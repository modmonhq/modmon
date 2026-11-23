//! Two name resolvers as two nodes of a cluster run them, on loopback
//! addresses: lookups over datagrams and streams, the exchange between the
//! nodes, the host's own resolvers for other names, who gets no answer, and
//! a container that turns unhealthy.
#![cfg(target_os = "linux")]

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use mm::adapters::discovery::{NameResolver, ResolverOptions};
use mm::app::ports::{
    ClusterNetworkHost, MeshHost, MeteredContainers, RuntimeDriver, ServiceNames,
};
use mm::domain::cluster_network::{
    NetworkAddresses, NetworkDrift, NetworkTable, NodeAddresses, NodeNetworks,
};
use mm::domain::diagnostic::Diagnostic;
use mm::domain::discovery::wire;
use mm::domain::discovery::{
    Attachment, Health, NameReport, NodeRecords, ObservedContainer, Reach,
};
use mm::domain::error::MmError;
use mm::domain::mesh::{MeshApplied, MeshFacts, MeshNodeStatus, MeshPeer, MeshState};
use mm::domain::repo_target::Ipv4Cidr;
use mm::domain::runtime::{
    ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy, ObservedProject,
    RenderedProject, RuntimeKind, RuntimeProbe, RuntimeTarget,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, UdpSocket};
use tokio::sync::Notify;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const KEY: &str = "0123456789ab-production-backend";
// The network's block is 127.77.0.0/20: node a holds 127.77.0.0/24 and
// answers on 127.77.0.1, node b holds 127.77.1.0/24 and answers on
// 127.77.1.1. Their mesh addresses are 127.77.99.1 and 127.77.99.2.
const GATEWAY_A: Ipv4Addr = Ipv4Addr::new(127, 77, 0, 1);
const API_A: Ipv4Addr = Ipv4Addr::new(127, 77, 0, 5);
const WEB_B: Ipv4Addr = Ipv4Addr::new(127, 77, 1, 6);
const MESH_A: Ipv4Addr = Ipv4Addr::new(127, 77, 99, 1);
const MESH_B: Ipv4Addr = Ipv4Addr::new(127, 77, 99, 2);
const UPSTREAM: Ipv4Addr = Ipv4Addr::new(127, 77, 53, 1);

fn unsupported<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not part of this test".to_owned(),
    })
}

struct Networks(NodeNetworks);

#[async_trait]
impl ClusterNetworkHost for Networks {
    fn stored(&self) -> Result<Option<NodeNetworks>, MmError> {
        Ok(Some(self.0.clone()))
    }
    fn store(&self, _desired: &NodeNetworks) -> Result<bool, MmError> {
        unsupported()
    }
    fn resolver_files(&self) -> std::path::PathBuf {
        std::path::PathBuf::new()
    }
    async fn converge(&self) -> Result<(), MmError> {
        unsupported()
    }
    async fn up(&self, _before_engine: bool) -> Result<MeshApplied, MmError> {
        unsupported()
    }
    async fn drift(&self) -> Result<Vec<NetworkDrift>, MmError> {
        unsupported()
    }
    async fn leave(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
}

/// `Networks` that counts how often the resolver settles the bridges.
struct Settling(NodeNetworks, std::sync::atomic::AtomicUsize);

#[async_trait]
impl ClusterNetworkHost for Settling {
    fn stored(&self) -> Result<Option<NodeNetworks>, MmError> {
        Ok(Some(self.0.clone()))
    }
    fn store(&self, _desired: &NodeNetworks) -> Result<bool, MmError> {
        unsupported()
    }
    fn resolver_files(&self) -> std::path::PathBuf {
        std::path::PathBuf::new()
    }
    async fn converge(&self) -> Result<(), MmError> {
        unsupported()
    }
    async fn up(&self, _before_engine: bool) -> Result<MeshApplied, MmError> {
        unsupported()
    }
    async fn drift(&self) -> Result<Vec<NetworkDrift>, MmError> {
        unsupported()
    }
    async fn settle_bridges(&self) -> Result<bool, MmError> {
        self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(false)
    }
    async fn leave(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
}

struct Mesh(MeshState);

#[async_trait]
impl MeshHost for Mesh {
    /// The facts include the kernel's routes, which a real host reads
    /// with a child process: the resolver must never ask for them.
    async fn facts(&self) -> Result<MeshFacts, MmError> {
        unsupported()
    }
    fn stored(&self) -> Result<Option<MeshState>, MmError> {
        Ok(Some(self.0.clone()))
    }
    async fn ensure_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn apply(&self, _desired: &MeshState) -> Result<MeshApplied, MmError> {
        unsupported()
    }
    async fn up(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
    async fn status(&self) -> Result<MeshNodeStatus, MmError> {
        unsupported()
    }
    async fn stage_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn activate_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn leave(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
}

/// An engine whose containers the test sets, and that says when they change.
struct Engine {
    containers: Mutex<Vec<ObservedContainer>>,
    changed: Notify,
    /// How many times the resolver looked at the engine.
    looks: std::sync::atomic::AtomicUsize,
    /// How long each listing of the containers takes, in milliseconds.
    slowness: std::sync::atomic::AtomicU64,
    failing: std::sync::atomic::AtomicBool,
}

impl Engine {
    fn new(containers: Vec<ObservedContainer>) -> Arc<Self> {
        Arc::new(Self {
            containers: Mutex::new(containers),
            changed: Notify::new(),
            looks: std::sync::atomic::AtomicUsize::new(0),
            slowness: std::sync::atomic::AtomicU64::new(0),
            failing: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn set(&self, containers: Vec<ObservedContainer>) -> Result<(), &'static str> {
        *self.containers.lock().map_err(|_| "poisoned")? = containers;
        self.changed.notify_one();
        Ok(())
    }
}

#[async_trait]
impl RuntimeDriver for Engine {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::DockerCompose
    }
    async fn probe(&self) -> RuntimeProbe {
        RuntimeProbe::unavailable(
            RuntimeKind::DockerCompose,
            Diagnostic::error("MM_TEST", "not part of this test"),
        )
    }
    async fn render(&self, _target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        unsupported()
    }
    async fn observe(&self, _target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        unsupported()
    }
    async fn wait_for_convergence(
        &self,
        _target: &RuntimeTarget,
        _policy: ConvergencePolicy,
        _cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        unsupported()
    }
    async fn network_containers(
        &self,
        _networks: &[String],
    ) -> Result<Vec<ObservedContainer>, MmError> {
        self.looks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let slowness = self.slowness.load(std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(slowness)).await;
        if self.failing.load(std::sync::atomic::Ordering::SeqCst) {
            return unsupported();
        }
        self.containers
            .lock()
            .map(|containers| containers.clone())
            .map_err(|_| MmError::State {
                message: "the test engine's lock was poisoned".to_owned(),
            })
    }
    async fn watch_containers(&self, changed: &(dyn Fn() + Send + Sync)) -> Result<(), MmError> {
        loop {
            self.changed.notified().await;
            changed();
        }
    }
}

fn cidr(text: &str) -> Result<Ipv4Cidr, String> {
    text.parse()
}

fn networks(node: &str) -> Result<NodeNetworks, String> {
    let held = |slice: &str, network: &str| {
        Ok::<_, String>(NodeAddresses {
            slice: cidr(slice)?,
            network: network.to_owned(),
        })
    };
    Ok(NodeNetworks {
        node: node.to_owned(),
        space: cidr("127.77.0.0/16")?,
        mesh_interface: "lo".to_owned(),
        mtu: 1420,
        table: NetworkTable {
            networks: [(
                KEY.to_owned(),
                NetworkAddresses {
                    block: cidr("127.77.0.0/20")?,
                    internal: false,
                    nodes: [
                        ("a".to_owned(), held("127.77.0.0/24", "engine-backend")?),
                        ("b".to_owned(), held("127.77.1.0/24", "engine-backend")?),
                    ]
                    .into(),
                },
            )]
            .into(),
        },
    })
}

fn mesh(address: Ipv4Addr, peer: &str, peer_address: Ipv4Addr) -> MeshState {
    MeshState {
        interface: "lo".to_owned(),
        address,
        prefix: 24,
        port: 51820,
        mtu: 1420,
        peers: vec![MeshPeer {
            name: peer.to_owned(),
            public_key: String::new(),
            endpoint: String::new(),
            allowed_ips: vec![
                Ipv4Cidr {
                    address: peer_address,
                    prefix: 32,
                },
                Ipv4Cidr {
                    address: Ipv4Addr::new(127, 77, 1, 0),
                    prefix: 24,
                },
            ],
        }],
    }
}

fn container(name: &str, service: &str, address: Ipv4Addr, health: Health) -> ObservedContainer {
    ObservedContainer {
        id: format!("{name}-id"),
        name: name.to_owned(),
        service: Some(service.to_owned()),
        health,
        attachments: vec![Attachment {
            network: "engine-backend".to_owned(),
            address,
            names: vec![name.to_owned(), service.to_owned()],
        }],
    }
}

fn query(id: u16, name: &str) -> Vec<u8> {
    let mut message = id.to_be_bytes().to_vec();
    message.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.') {
        message.push(u8::try_from(label.len()).unwrap_or(u8::MAX));
        message.extend_from_slice(label.as_bytes());
    }
    message.extend_from_slice(&[0, 0, 1, 0, 1]);
    message
}

/// The response code and the addresses of a reply to `query(_, name)`.
fn answer(reply: &[u8], name: &str) -> (u8, Vec<Ipv4Addr>) {
    let records = reply.get(12 + name.len() + 6..).unwrap_or_default();
    (
        reply[3] & 0x0f,
        records
            .chunks(16)
            .filter(|record| record.len() == 16)
            .map(|record| Ipv4Addr::new(record[12], record[13], record[14], record[15]))
            .collect(),
    )
}

/// Asks `server` for `name` from the address `from`; `None` when no answer
/// comes within `patience`.
async fn ask(
    from: Ipv4Addr,
    server: SocketAddrV4,
    name: &str,
    patience: Duration,
) -> Result<Option<(u8, Vec<Ipv4Addr>)>, std::io::Error> {
    let socket = UdpSocket::bind(SocketAddrV4::new(from, 0)).await?;
    socket.send_to(&query(0x4d4d, name), server).await?;
    let mut buffer = [0_u8; 1024];
    match tokio::time::timeout(patience, socket.recv_from(&mut buffer)).await {
        Ok(received) => {
            let (length, _) = received?;
            assert_eq!(&buffer[..2], &[0x4d, 0x4d], "the asker's own identifier");
            Ok(Some(answer(&buffer[..length], name)))
        }
        Err(_) => Ok(None),
    }
}

/// Asks until the answer is `wanted`, for up to ten seconds.
async fn becomes(
    from: Ipv4Addr,
    server: SocketAddrV4,
    name: &str,
    wanted: &[Ipv4Addr],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last = None;
    for _ in 0..100 {
        last = ask(from, server, name, Duration::from_millis(300)).await?;
        if let Some((0, addresses)) = &last {
            let mut addresses = addresses.clone();
            addresses.sort();
            if addresses == wanted {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("{name} resolves to {last:?} instead of {wanted:?}").into())
}

/// A resolver of the host: answers `outside.example` with 192.0.2.7, refuses
/// a name without a dot, and says nothing to anything else.
async fn upstream(socket: UdpSocket) {
    let mut buffer = [0_u8; 1024];
    while let Ok((length, peer)) = socket.recv_from(&mut buffer).await {
        let wire::Query::Question(question) = wire::parse(&buffer[..length]) else {
            continue;
        };
        let reply = match question.name.as_str() {
            "outside.example" => question.answer(&[Ipv4Addr::new(192, 0, 2, 7)], 60, 512),
            name if !name.contains('.') => {
                let mut refusal = question.failure(wire::Failure::Server);
                refusal[3] = (refusal[3] & 0xf0) | 5;
                refusal
            }
            _ => continue,
        };
        if socket.send_to(&reply, peer).await.is_err() {
            break;
        }
    }
}

/// A free port on the test's addresses.
async fn free_port(address: Ipv4Addr) -> Result<u16, std::io::Error> {
    Ok(UdpSocket::bind(SocketAddrV4::new(address, 0))
        .await?
        .local_addr()?
        .port())
}

/// Reads the exchange port of node a from the address `from`.
async fn exchange(from: Ipv4Addr, port: u16) -> Result<Vec<u8>, std::io::Error> {
    let socket = TcpSocket::new_v4()?;
    socket.bind(SocketAddrV4::new(from, 0).into())?;
    let mut stream = socket
        .connect(SocketAddrV4::new(MESH_A, port).into())
        .await?;
    let mut read = Vec::new();
    stream.read_to_end(&mut read).await?;
    Ok(read)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_nodes_answer_their_own_containers_with_what_both_run() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    tokio::spawn(upstream(
        UdpSocket::bind(SocketAddrV4::new(UPSTREAM, options.upstream_port)).await?,
    ));
    let engine_a = Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]);
    let engine_b = Engine::new(vec![container("web-1", "web", WEB_B, Health::Healthy)]);
    let node_a = NameResolver::with_options(
        Arc::new(Networks(networks("a")?)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        engine_a.clone(),
        options.clone(),
    );
    let node_b = NameResolver::with_options(
        Arc::new(Networks(networks("b")?)),
        Arc::new(Mesh(mesh(MESH_B, "a", MESH_A))),
        engine_b.clone(),
        options.clone(),
    );
    tokio::spawn(node_a.clone().run());
    tokio::spawn(node_b.clone().run());
    let server_a = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    let server_b = SocketAddrV4::new(Ipv4Addr::new(127, 77, 1, 1), options.lookup_port);
    let short = Duration::from_millis(400);

    // Each node answers with the containers of both, in every form of name:
    // neither node has a container of the other's service.
    becomes(API_A, server_a, "web", &[WEB_B]).await?;
    becomes(API_A, server_a, "tasks.web", &[WEB_B]).await?;
    becomes(API_A, server_a, "web.b", &[WEB_B]).await?;
    becomes(API_A, server_a, "api", &[API_A]).await?;
    becomes(WEB_B, server_b, "api", &[API_A]).await?;
    becomes(WEB_B, server_b, "api.a", &[API_A]).await?;
    // A container's own name stays on its node.
    becomes(API_A, server_a, "api-1", &[API_A]).await?;
    // The node's metrics are sampled for the containers it lists itself.
    assert_eq!(node_a.running(), ["api-1-id"]);
    assert_eq!(node_b.running(), ["web-1-id"]);
    assert_eq!(
        ask(WEB_B, server_b, "api-1", Duration::from_secs(3)).await?,
        Some((3, Vec::new())),
        "the host's resolver refuses a name without a dot: no such name"
    );

    // Other names: the host's resolver answers, refuses, or stays silent.
    assert_eq!(
        ask(API_A, server_a, "outside.example", Duration::from_secs(3)).await?,
        Some((0, vec![Ipv4Addr::new(192, 0, 2, 7)]))
    );
    assert_eq!(
        ask(API_A, server_a, "silent.example", Duration::from_secs(4)).await?,
        Some((2, Vec::new())),
        "no resolver answered: a server failure, so the client tries again"
    );

    // Only a node's own containers are answered.
    for stranger in [WEB_B, MESH_B, Ipv4Addr::LOCALHOST] {
        assert_eq!(
            ask(stranger, server_a, "web", short).await?,
            None,
            "{stranger}"
        );
        assert_eq!(
            ask(stranger, server_a, "outside.example", short).await?,
            None
        );
    }

    // The same over a stream, two lookups on one connection.
    let socket = TcpSocket::new_v4()?;
    socket.bind(SocketAddrV4::new(API_A, 0).into())?;
    let mut stream = socket.connect(SocketAddr::V4(server_a)).await?;
    for name in ["web", "api"] {
        let message = query(7, name);
        stream
            .write_all(&u16::try_from(message.len())?.to_be_bytes())
            .await?;
        stream.write_all(&message).await?;
        let mut length = [0_u8; 2];
        stream.read_exact(&mut length).await?;
        let mut reply = vec![0_u8; usize::from(u16::from_be_bytes(length))];
        stream.read_exact(&mut reply).await?;
        assert_eq!(answer(&reply, name).1.len(), 1, "{name} over a stream");
    }

    // The exchange: a mesh peer reads what node a runs; anyone else reads
    // nothing.
    let records: NodeRecords =
        serde_json::from_slice(&exchange(MESH_B, options.exchange_port).await?)?;
    assert_eq!(records.node, "a");
    assert_eq!(records.containers.len(), 1);
    assert_eq!(records.containers[0].endpoints[0].network, KEY);
    assert!(
        exchange(API_A, options.exchange_port).await?.is_empty(),
        "a container"
    );
    assert!(
        exchange(Ipv4Addr::LOCALHOST, options.exchange_port)
            .await?
            .is_empty()
    );

    // What `mm service status` shows, from either node.
    let report = node_a.report("0123456789ab-production-");
    assert_eq!(report.networks, ["backend"]);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let names = report
        .names
        .iter()
        .map(|name| (name.name.as_str(), name.addresses[0].node.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(names, [("api", "a"), ("web", "b")]);
    assert!(node_b.report("another-").is_empty());

    // web on node b, the service's only container, fails its health
    // check: both nodes go on answering with it (fail open), and the
    // container itself finds its own name.
    engine_b.set(vec![container("web-1", "web", WEB_B, Health::Unhealthy)])?;
    let another_on_b = Ipv4Addr::new(127, 77, 1, 16);
    becomes(another_on_b, server_b, "web", &[WEB_B]).await?;
    becomes(API_A, server_a, "web", &[WEB_B]).await?;
    becomes(WEB_B, server_b, "web", &[WEB_B]).await?;
    // Stopped, it leaves the answers on both nodes, and the name stays
    // known.
    engine_b.set(vec![container("web-1", "web", WEB_B, Health::Stopped)])?;
    becomes(another_on_b, server_b, "web", &[]).await?;
    becomes(API_A, server_a, "web", &[]).await?;
    engine_b.set(vec![container("web-1", "web", WEB_B, Health::Healthy)])?;
    becomes(API_A, server_a, "web", &[WEB_B]).await?;

    // Node a gets a web of its own: its containers stay on the node, and
    // `tasks.` still names both. Node b keeps answering with its own.
    let web_a = Ipv4Addr::new(127, 77, 0, 9);
    engine_a.set(vec![
        container("api-1", "api", API_A, Health::Healthy),
        container("web-9", "web", web_a, Health::Healthy),
    ])?;
    becomes(API_A, server_a, "web", &[web_a]).await?;
    becomes(API_A, server_a, "tasks.web", &[web_a, WEB_B]).await?;
    becomes(another_on_b, server_b, "tasks.web", &[web_a, WEB_B]).await?;
    becomes(another_on_b, server_b, "web", &[WEB_B]).await?;
    let report = NameReport::merged(vec![
        ("a".to_owned(), node_a.report("0123456789ab-production-")),
        ("b".to_owned(), node_b.report("0123456789ab-production-")),
    ]);
    let web = report
        .names
        .iter()
        .find(|name| name.name == "web")
        .ok_or("no web in the report")?;
    assert_eq!(
        web.lookups
            .iter()
            .map(|lookup| (lookup.node.as_str(), lookup.addresses.clone(), lookup.local))
            .collect::<Vec<_>>(),
        [("a", vec![web_a], true), ("b", vec![WEB_B], true)]
    );
    // What a proxy next to the resolver would ask.
    assert_eq!(
        node_a.addresses(KEY, "web", Reach::LocalFirst),
        (vec![web_a], true)
    );
    assert_eq!(
        node_a.addresses(KEY, "web", Reach::EveryNode),
        (vec![web_a, WEB_B], false)
    );
    assert_eq!(
        node_a.addresses(KEY, "api", Reach::LocalFirst),
        (vec![API_A], true)
    );
    assert_eq!(
        node_b.addresses(KEY, "api", Reach::LocalFirst),
        (vec![API_A], false)
    );
    // Its own web turns unhealthy: the next lookups on node a get node
    // b's, with no empty answer in between.
    engine_a.set(vec![
        container("api-1", "api", API_A, Health::Healthy),
        container("web-9", "web", web_a, Health::Unhealthy),
    ])?;
    let mut seen = Vec::new();
    for _ in 0..40 {
        let answer = ask(API_A, server_a, "web", Duration::from_secs(3))
            .await?
            .map(|(_, addresses)| addresses)
            .unwrap_or_default();
        if seen.last() != Some(&answer) {
            seen.push(answer);
        }
        if seen.last().is_some_and(|last| *last == vec![WEB_B]) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        seen.last(),
        Some(&vec![WEB_B]),
        "the other node's in the end"
    );
    assert!(
        seen.iter()
            .all(|answer| *answer == vec![web_a] || *answer == vec![WEB_B]),
        "its own, then the other node's, and nothing else in between: {seen:?}"
    );
    Ok(())
}

/// `mmd` answers names on a thread of its own. Here the runtime that
/// started the resolver stands in for the one that serves requests: its
/// only thread is held, as a thread that waits in the kernel is, and a
/// container's lookups are answered all the same.
#[tokio::test]
async fn lookups_are_answered_while_the_runtime_that_serves_requests_is_blocked() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    let node = NameResolver::with_options(
        Arc::new(Networks(networks("a")?)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]),
        options.clone(),
    );
    let names = mm::adapters::own_runtime::OwnRuntime::start("test-names")?;
    names.spawn(node.run());
    let server = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    becomes(API_A, server, "api", &[API_A]).await?;

    // From here on nothing of this runtime runs until the plain thread is
    // done asking.
    let asking = std::thread::spawn(move || -> std::io::Result<Vec<Duration>> {
        let socket = std::net::UdpSocket::bind(SocketAddrV4::new(API_A, 0))?;
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut took = Vec::new();
        for _ in 0..5 {
            let began = std::time::Instant::now();
            socket.send_to(&query(0x4d4d, "api"), server)?;
            let mut buffer = [0_u8; 1024];
            let (length, _) = socket.recv_from(&mut buffer)?;
            if answer(&buffer[..length], "api") != (0, vec![API_A]) {
                return Err(std::io::Error::other("the lookup got another answer"));
            }
            took.push(began.elapsed());
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(took)
    });
    let took = asking
        .join()
        .map_err(|_| "the asking thread panicked")?
        .map_err(|error| {
            format!("a lookup was not answered while requests were blocked: {error}")
        })?;
    assert_eq!(took.len(), 5);
    assert!(
        took.iter().all(|time| *time < Duration::from_millis(500)),
        "lookups took {took:?}"
    );
    assert_eq!(names.answers(Duration::from_secs(1)), Ok(()));
    Ok(())
}

/// A node whose stored networks name one the host does not have yet (Podman
/// creates a network's bridge with its first container) listens on its
/// gateway at once, so that container's first lookup is answered; and
/// one the host never gets (no deployment here uses it any more) does not
/// make the node look at its engine every second: it used to, for as long
/// as `mmd` ran.
#[tokio::test]
async fn an_address_the_host_does_not_have_is_listened_on_without_looking_every_second()
-> TestResult {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_millis(1500),
    };
    let mut stored = networks("a")?;
    // 192.0.2.0/24 is reserved for documentation: no host has 192.0.2.1.
    stored.table.networks.insert(
        "0123456789ab-production-frontend".to_owned(),
        NetworkAddresses {
            block: cidr("192.0.2.0/24")?,
            internal: false,
            nodes: [(
                "a".to_owned(),
                NodeAddresses {
                    slice: cidr("192.0.2.0/24")?,
                    network: "engine-frontend".to_owned(),
                },
            )]
            .into(),
        },
    );
    let engine = Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]);
    let node = NameResolver::with_options(
        Arc::new(Networks(stored)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        Arc::clone(&engine) as Arc<dyn RuntimeDriver>,
        options.clone(),
    );
    tokio::spawn(Arc::clone(&node).run());
    // The network the host does have is answered on all the same.
    let server = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    becomes(API_A, server, "api", &[API_A]).await?;

    // Nothing is waited for: the first look, and the next ten seconds
    // later.
    tokio::time::sleep(Duration::from_millis(5500)).await;
    let looks = engine.looks.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        (1..=2).contains(&looks),
        "the resolver looked at the engine {looks} times in 5.5 s"
    );
    Ok(())
}

/// A container the node has not looked at yet may be the first of its
/// network there, whose bridge Podman has just created: its first lookup
/// has the bridges settled before it is answered, so its first connection
/// to another node finds the bridge's settings. A known container's
/// lookups cost no settling.
#[tokio::test]
async fn a_new_containers_first_lookup_settles_the_bridges_before_its_answer() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    let networks = Arc::new(Settling(
        networks("a")?,
        std::sync::atomic::AtomicUsize::new(0),
    ));
    let node = NameResolver::with_options(
        Arc::clone(&networks) as Arc<dyn ClusterNetworkHost>,
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]),
        options.clone(),
    );
    tokio::spawn(Arc::clone(&node).run());
    let server = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    becomes(API_A, server, "api", &[API_A]).await?;
    let settled = || networks.1.load(std::sync::atomic::Ordering::SeqCst);
    let before = settled();
    becomes(API_A, server, "api", &[API_A]).await?;
    assert_eq!(
        settled(),
        before,
        "a known container's lookup settles nothing"
    );
    // 127.77.0.9 is in node a's slice and no container the engine runs:
    // its first lookup waits for the engine, the next ones do not.
    let stranger = Ipv4Addr::new(127, 77, 0, 9);
    let first = ask(stranger, server, "api", Duration::from_secs(3)).await?;
    assert_eq!(first, Some((0, vec![API_A])));
    assert_eq!(settled(), before + 1);
    let again = ask(stranger, server, "api", Duration::from_millis(300)).await?;
    assert_eq!(again, Some((0, vec![API_A])));
    Ok(())
}

/// A container that resolves its own `tasks.<service>` name as its
/// entrypoint's first step finds itself in the first answer: before the
/// engine's event for it arrives, and when the engine shows it only a
/// moment after its first lookup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_container_finds_itself_at_its_first_lookup() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    let engine = Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]);
    let node = NameResolver::with_options(
        Arc::new(Networks(networks("a")?)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        Arc::clone(&engine) as Arc<dyn RuntimeDriver>,
        options.clone(),
    );
    tokio::spawn(Arc::clone(&node).run());
    let server = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    becomes(API_A, server, "api", &[API_A]).await?;

    let unannounced = |added: ObservedContainer| -> TestResult {
        engine
            .containers
            .lock()
            .map_err(|_| "poisoned")?
            .push(added);
        Ok(())
    };
    let web = Ipv4Addr::new(127, 77, 0, 7);
    unannounced(container("web-1", "web", web, Health::Starting))?;
    let first = ask(web, server, "tasks.web", Duration::from_secs(2)).await?;
    assert_eq!(first, Some((0, vec![web])));

    let worker = Ipv4Addr::new(127, 77, 0, 8);
    let late = Arc::clone(&engine);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if let Ok(mut containers) = late.containers.lock() {
            containers.push(container("worker-1", "worker", worker, Health::Starting));
        }
    });
    let first = ask(worker, server, "tasks.worker", Duration::from_secs(2)).await?;
    assert_eq!(first, Some((0, vec![worker])));
    Ok(())
}

/// On an engine that takes longer than a second to list its containers, a
/// new container's first lookup still finds it, and so do its next ones.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_container_finds_itself_on_a_slow_engine() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    let engine = Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]);
    let node = NameResolver::with_options(
        Arc::new(Networks(networks("a")?)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        Arc::clone(&engine) as Arc<dyn RuntimeDriver>,
        options.clone(),
    );
    tokio::spawn(Arc::clone(&node).run());
    let server = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    becomes(API_A, server, "api", &[API_A]).await?;

    engine
        .slowness
        .store(1500, std::sync::atomic::Ordering::SeqCst);
    let web = Ipv4Addr::new(127, 77, 0, 7);
    engine
        .containers
        .lock()
        .map_err(|_| "poisoned")?
        .push(container("web-1", "web", web, Health::Starting));
    let first = ask(web, server, "tasks.web", Duration::from_secs(6)).await?;
    assert_eq!(first, Some((0, vec![web])));
    let again = ask(web, server, "tasks.web", Duration::from_secs(1)).await?;
    assert_eq!(again, Some((0, vec![web])));
    Ok(())
}

/// A node `a` whose engine shows `api-1`, answering lookups once it looked.
async fn node_a(
    engine: &Arc<Engine>,
) -> Result<(SocketAddrV4, tempfile::TempDir), Box<dyn std::error::Error>> {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    let node = NameResolver::with_options(
        Arc::new(Networks(networks("a")?)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        Arc::clone(engine) as Arc<dyn RuntimeDriver>,
        options.clone(),
    );
    tokio::spawn(Arc::clone(&node).run());
    let server = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    becomes(API_A, server, "api", &[API_A]).await?;
    Ok((server, directory))
}

/// How long a lookup of `api` from `from` takes to be answered, within
/// `patience`.
async fn answered_in(
    from: Ipv4Addr,
    server: SocketAddrV4,
    patience: Duration,
) -> Result<Duration, String> {
    let started = std::time::Instant::now();
    let answer = ask(from, server, "api", patience)
        .await
        .map_err(|error| error.to_string())?;
    if answer.is_none() {
        return Err(format!("{from} got no answer"));
    }
    Ok(started.elapsed())
}

/// How many times the engine was asked for its containers.
fn listings(engine: &Engine) -> usize {
    engine.looks.load(std::sync::atomic::Ordering::SeqCst)
}

/// On an engine slower than a lookup's cap, the lookup is answered at the
/// cap, and the listing it started goes on: the container it shows is
/// found by the next lookup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lookup_waits_for_a_slow_engine_no_longer_than_its_cap() -> TestResult {
    let engine = Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]);
    let (server, _directory) = node_a(&engine).await?;
    let web = Ipv4Addr::new(127, 77, 0, 7);
    engine
        .containers
        .lock()
        .map_err(|_| "poisoned")?
        .push(container("web-1", "web", web, Health::Starting));
    engine
        .slowness
        .store(8000, std::sync::atomic::Ordering::SeqCst);
    answered_in(web, server, Duration::from_secs(5)).await?;
    becomes(web, server, "tasks.web", &[web]).await?;
    Ok(())
}

/// On an engine slower than a lookup's cap, an address no container has is
/// remembered once the listing its lookup started finishes without it,
/// though the lookup was answered before: its next lookup does not read the
/// engine again and again for a second, as an unknown asker's does. The
/// engine is fast again before the background look's next listing, which
/// would otherwise hold the engine through the next lookup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stray_asker_is_remembered_after_a_slow_listing() -> TestResult {
    let engine = Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]);
    let (server, _directory) = node_a(&engine).await?;
    engine
        .slowness
        .store(4000, std::sync::atomic::Ordering::SeqCst);
    let stray = Ipv4Addr::new(127, 77, 0, 9);
    answered_in(stray, server, Duration::from_secs(5)).await?;
    engine
        .slowness
        .store(0, std::sync::atomic::Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(3)).await;
    let before = listings(&engine);
    answered_in(stray, server, Duration::from_secs(5)).await?;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let during = listings(&engine) - before;
    assert!(
        during < 4,
        "a stray asker was not remembered: its lookup listed the engine {during} times"
    );
    Ok(())
}

/// An engine that cannot list its containers makes no stranger of an asker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_engine_makes_no_stranger() -> TestResult {
    let engine = Engine::new(vec![container("api-1", "api", API_A, Health::Healthy)]);
    let (server, _directory) = node_a(&engine).await?;
    engine
        .failing
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let stranger = Ipv4Addr::new(127, 77, 0, 9);
    for _ in 0..2 {
        assert!(
            answered_in(stranger, server, Duration::from_secs(8)).await?
                >= Duration::from_millis(900)
        );
    }
    Ok(())
}

/// The sockets a node answers lookups on bind a gateway address before the
/// host has it, and only that address.
#[tokio::test]
async fn lookup_sockets_bind_an_address_the_host_does_not_have_yet() -> TestResult {
    // 192.0.2.0/24 is reserved for documentation: no host has 192.0.2.1.
    let missing = SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(192, 0, 2, 1),
        free_port(GATEWAY_A).await?,
    ));
    let datagrams = mm::adapters::discovery::free_udp(missing)?;
    let streams = mm::adapters::discovery::free_tcp(missing)?;
    assert_eq!(datagrams.local_addr()?, missing);
    assert_eq!(streams.local_addr()?, missing);
    // On an address the host has, a datagram reaches the socket.
    let here = SocketAddr::V4(SocketAddrV4::new(GATEWAY_A, free_port(GATEWAY_A).await?));
    let datagrams = mm::adapters::discovery::free_udp(here)?;
    datagrams.set_read_timeout(Some(Duration::from_secs(2)))?;
    let asker = std::net::UdpSocket::bind(SocketAddrV4::new(API_A, 0))?;
    asker.send_to(b"ping", here)?;
    let mut buffer = [0_u8; 8];
    let (length, from) = datagrams.recv_from(&mut buffer)?;
    assert_eq!(
        (&buffer[..length], from.ip()),
        (&b"ping"[..], std::net::IpAddr::V4(API_A))
    );
    Ok(())
}

/// Waits up to ten seconds for `holds`.
async fn eventually(holds: impl Fn() -> bool, what: &str) -> TestResult {
    for _ in 0..100 {
        if holds() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("{what} did not happen within ten seconds").into())
}

/// A container that leaves (`domain::leaving`) drops out of its own node's
/// answers at once and out of the other node's at its next exchange; its
/// node hears every node confirm only once the other node's ingress proxy
/// runs answers without it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leaving_container_is_confirmed_once_no_node_sends_it_anything() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    let web_a = Ipv4Addr::new(127, 77, 0, 7);
    let engine_a = Engine::new(vec![
        container("api-1", "api", API_A, Health::Healthy),
        container("web-1", "web", web_a, Health::Healthy),
    ]);
    let engine_b = Engine::new(vec![container("web-2", "web", WEB_B, Health::Healthy)]);
    let node_a = NameResolver::with_options(
        Arc::new(Networks(networks("a")?)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        engine_a.clone(),
        options.clone(),
    );
    let node_b = NameResolver::with_options(
        Arc::new(Networks(networks("b")?)),
        Arc::new(Mesh(mesh(MESH_B, "a", MESH_A))),
        engine_b.clone(),
        options.clone(),
    );
    // Node b runs an ingress proxy that has run nothing of this mmd yet.
    node_b.proxy_runs(Some(0));
    tokio::spawn(node_a.clone().run());
    tokio::spawn(node_b.clone().run());
    let server_a = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    let server_b = SocketAddrV4::new(Ipv4Addr::new(127, 77, 1, 1), options.lookup_port);
    becomes(API_A, server_a, "tasks.web", &[web_a, WEB_B]).await?;
    becomes(WEB_B, server_b, "tasks.web", &[web_a, WEB_B]).await?;

    node_a.mark_leaving(&["web-1-id".to_owned()]);
    // Out of node a's answers and of what a proxy next to it routes to.
    becomes(API_A, server_a, "web", &[WEB_B]).await?;
    assert_eq!(
        node_a.addresses(KEY, "web", Reach::LocalFirst),
        (vec![WEB_B], false)
    );
    // Node b drops it at its next exchange.
    becomes(WEB_B, server_b, "tasks.web", &[WEB_B]).await?;
    eventually(
        || node_b.addresses(KEY, "web", Reach::EveryNode) == (vec![WEB_B], false),
        "node b dropping web-1",
    )
    .await?;
    // Node b's proxy has not run those answers: nothing is confirmed.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!node_a.leave_confirmed("web-1-id"), "confirmed too early");
    // Now it runs them, and node a hears it at its next exchange.
    let generation = *node_b.generation().borrow();
    node_b.proxy_runs(Some(generation));
    eventually(|| node_a.leave_confirmed("web-1-id"), "the confirmation").await?;
    // Marking it again changes nothing; the engine still runs it, and it
    // stays out of the answers.
    node_a.mark_leaving(&["web-1-id".to_owned()]);
    engine_a.set(vec![
        container("api-1", "api", API_A, Health::Healthy),
        container("web-1", "web", web_a, Health::Healthy),
    ])?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    becomes(API_A, server_a, "tasks.web", &[WEB_B]).await?;
    assert!(node_a.leave_confirmed("web-1-id"));
    // Gone and forgotten: a new container of the same name answers again.
    engine_a.set(vec![container("api-1", "api", API_A, Health::Healthy)])?;
    node_a.unmark_leaving(&["web-1-id".to_owned()]);
    engine_a.set(vec![
        container("api-1", "api", API_A, Health::Healthy),
        container("web-1", "web", web_a, Health::Healthy),
    ])?;
    becomes(API_A, server_a, "tasks.web", &[web_a, WEB_B]).await?;
    assert!(!node_a.leave_confirmed("web-1-id"), "nothing is leaving");
    Ok(())
}

/// The engine's command line as a node's leave keeper uses it: `inspect`
/// names container `api-1`, and `ps` lists it in its Compose project.
struct EngineCli {
    id: String,
}

#[async_trait]
impl mm::app::ports::ProcessRunner for EngineCli {
    async fn run(
        &self,
        specification: mm::app::ports::CommandSpec,
    ) -> Result<mm::app::ports::CommandOutput, MmError> {
        let command = specification.redacted_display();
        let stdout = if command.contains(" inspect ") {
            format!("{} /api-1 ", self.id)
        } else if command.contains(" ps ") {
            format!("{}\n", self.id)
        } else {
            String::new()
        };
        Ok(mm::app::ports::CommandOutput {
            code: Some(0),
            stdout: zeroize::Zeroizing::new(stdout),
            stderr: zeroize::Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

/// A rollback takes back a container that was leaving: `mm
/// deploy cancel` during a drain returns the node to what it ran before,
/// and the container that drained is given out for its name again at once
/// instead of being stopped after the rollback kept it.
#[tokio::test]
async fn a_rollback_takes_a_leaving_container_back_into_the_answers() -> TestResult {
    use mm::app::ports::ContainerLeaving;

    let directory = temp_dirs::tempdir()?;
    let resolv_conf = directory.path().join("resolv.conf");
    std::fs::write(&resolv_conf, format!("search .\nnameserver {UPSTREAM}\n"))?;
    let options = ResolverOptions {
        lookup_port: free_port(GATEWAY_A).await?,
        exchange_port: free_port(MESH_A).await?,
        upstream_port: free_port(UPSTREAM).await?,
        resolv_conf,
        address_patience: Duration::from_secs(30),
    };
    let id = "0123456789abcdef".repeat(4);
    let mut api = container("api-1", "api", API_A, Health::Healthy);
    api.id.clone_from(&id);
    let node = NameResolver::with_options(
        Arc::new(Networks(networks("a")?)),
        Arc::new(Mesh(mesh(MESH_A, "b", MESH_B))),
        Engine::new(vec![api]),
        options.clone(),
    );
    tokio::spawn(Arc::clone(&node).run());
    let server = SocketAddrV4::new(GATEWAY_A, options.lookup_port);
    becomes(API_A, server, "api", &[API_A]).await?;

    let keeper = mm::adapters::leaving::LeaveKeeper::new(
        directory.path(),
        Arc::clone(&node),
        Arc::new(EngineCli { id: id.clone() }),
        RuntimeKind::DockerCompose,
    );
    keeper
        .begin(&["api-1".to_owned()], Duration::from_secs(600))
        .await?;
    tokio::spawn(Arc::clone(&keeper).run());
    // Left out: the name no longer gives the container.
    let mut left = false;
    for _ in 0..100 {
        let answered = ask(API_A, server, "api", Duration::from_millis(300)).await?;
        if !matches!(&answered, Some((0, addresses)) if addresses.contains(&API_A)) {
            left = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(left, "the leaving container was never left out");

    let recalled = keeper.recall("mm-shop-production-app").await?;
    assert_eq!(recalled, ["api-1"]);
    assert!(mm::adapters::leaving::leaving_ids(directory.path()).is_empty());
    becomes(API_A, server, "api", &[API_A]).await?;
    Ok(())
}
