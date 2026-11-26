//! What `mmd` serves at `/metrics`: the host's numbers read from `/proc`
//! fixtures, which addresses a node listens on and for whom, what each
//! scope sees, the text format and its limit, and the answers to a scrape.

use std::net::Ipv4Addr;
use std::sync::Arc;

use mm::adapters::node_metrics::{MetricsServer, ProcHostStats, Scrape, answer, read_tokens};
use mm::app::ports::{DeployedEnvironments, HostStats, MeshPeers, NodeContainers};
use mm::app::{NodeIdentity, NodeMetricsCollector, NodeMetricsSources};
use mm::domain::cluster_network::{NetworkNeed, NetworkTable, NodeNetworks, network_key};
use mm::domain::discovery::Health;
use mm::domain::error::MmError;
use mm::domain::host_policy::HostMetrics;
use mm::domain::mesh::{MeshPeer, MeshState};
use mm::domain::node_metrics::MetricsListener;
use mm::domain::node_metrics::procfs::{
    FilesystemSample, HostSample, parse_diskstats, parse_loadavg, parse_meminfo, parse_mounts,
    parse_net_dev, parse_stat,
};
use mm::domain::node_metrics::{
    DeployedEnvironment, LabelledContainer, MetricsScope, NodeSnapshot, PeerSample, RequestProblem,
    RolloutOutcome, environment_samples, listeners, parse_request, render,
};
use mm::domain::repo_target::Ipv4Cidr;
use mm::domain::workspace::ProjectId;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const STAT: &str = "cpu  4705 150 1120 16250856 1500 0 30 7 0 0
cpu0 2000 100 500 8125000 700 0 15 3 0 0
cpu1 2705 50 620 8125856 800 0 15 4 0 0
intr 114930548 113199788 3 0 5 263 0 4
ctxt 1990473
btime 1700000000
processes 2915
";

const MEMINFO: &str = "MemTotal:        2000000 kB
MemFree:          500000 kB
MemAvailable:    1200000 kB
Buffers:           20000 kB
Cached:           600000 kB
SwapCached:            0 kB
SwapTotal:       1000000 kB
SwapFree:         900000 kB
";

const MOUNTS: &str = "sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/vda1 / ext4 rw,relatime 0 0
/dev/vdb /srv/my\\040data xfs ro,relatime 0 0
tmpfs /run tmpfs rw,nosuid,nodev,size=400000k 0 0
tmpfs /var/lib/docker/containers/abc/mounts/shm tmpfs rw 0 0
overlay /var/lib/docker/overlay2/abc/merged overlay rw 0 0
/dev/vda1 /var/lib/docker/btrfs ext4 rw 0 0
10.90.0.1:/srv/mm/shared /srv/mm/mounts/shared nfs4 rw 0 0
/dev/vda1 / ext4 ro,relatime 0 0
";

const DISKSTATS: &str = "   7       0 loop0 10 0 20 0 0 0 0 0 0 0 0
 252       0 vda 1000 10 80000 500 2000 20 160000 900 0 1500 1400
 252       1 vda1 900 10 70000 450 1900 20 150000 850 0 1400 1300
 259       0 nvme0n1 5 0 40 1 6 0 48 2 0 3 3
 259       1 nvme0n1p1 5 0 40 1 6 0 48 2 0 3 3
 253       0 dm-0 7 0 56 1 8 0 64 1 0 2 2
";

const NET_DEV: &str = "Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:    1000      10    0    0    0     0          0         0     1000      10    0    0    0     0       0          0
  eth0: 5000000    4000    1    2    0     0          0         0  3000000    3000    3    4    0     0       0          0
vethab12: 100 1 0 0 0 0 0 0 100 1 0 0 0 0 0 0
   mm0:    7000      70    0    0    0     0          0         0     8000      80    0    0    0     0       0          0
";

#[test]
fn the_hosts_cpu_memory_and_load_are_read_from_proc() -> TestResult {
    let cpu = parse_stat(STAT).ok_or("no cpu line")?;
    assert_eq!(cpu.cpus, 2);
    assert_eq!(cpu.boot_time, Some(1_700_000_000));
    assert_eq!(cpu.modes.first(), Some(&("user", 47.05)));
    assert_eq!(cpu.modes.get(3), Some(&("idle", 162_508.56)));
    assert_eq!(cpu.modes.len(), 8);

    let memory = parse_meminfo(MEMINFO).ok_or("no MemTotal")?;
    assert_eq!(memory.total, 2_000_000 * 1024);
    assert_eq!(memory.available, 1_200_000 * 1024);
    assert_eq!(memory.swap_free, 900_000 * 1024);
    assert_eq!(parse_meminfo("MemFree: 1 kB\n"), None);

    assert_eq!(
        parse_loadavg("0.52 0.58 0.59 1/287 3104\n"),
        Some([0.52, 0.58, 0.59])
    );
    assert_eq!(parse_loadavg("garbage"), None);
    Ok(())
}

#[test]
fn only_the_hosts_own_local_filesystems_are_measured() {
    let mounts = parse_mounts(MOUNTS);
    let listed = mounts
        .iter()
        .map(|mount| {
            (
                mount.mountpoint.as_str(),
                mount.fstype.as_str(),
                mount.readonly,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        listed,
        vec![
            ("/", "ext4", true),
            ("/srv/my data", "xfs", true),
            ("/run", "tmpfs", false)
        ],
        "one entry per mountpoint, the last mount wins; no pseudo, container, engine or network filesystem"
    );
    assert_eq!(
        mounts.first().map(|mount| mount.device.as_str()),
        Some("/dev/vda1")
    );
}

#[test]
fn whole_disks_and_host_interfaces_are_counted() {
    let disks = parse_diskstats(DISKSTATS);
    let names = disks
        .iter()
        .map(|disk| disk.device.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["vda", "nvme0n1", "dm-0"]);
    let vda = &disks[0];
    assert_eq!((vda.reads, vda.writes), (1000, 2000));
    assert_eq!(
        (vda.read_bytes, vda.written_bytes),
        (80000 * 512, 160_000 * 512)
    );
    assert_eq!(vda.io_milliseconds, 1500);

    let interfaces = parse_net_dev(NET_DEV);
    let names = interfaces
        .iter()
        .map(|interface| interface.interface.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["lo", "eth0", "mm0"],
        "a container's link is left out"
    );
    let eth0 = &interfaces[1];
    assert_eq!(
        (
            eth0.receive_bytes,
            eth0.receive_packets,
            eth0.receive_errors,
            eth0.receive_drops
        ),
        (5_000_000, 4000, 1, 2)
    );
    assert_eq!(
        (
            eth0.transmit_bytes,
            eth0.transmit_packets,
            eth0.transmit_errors,
            eth0.transmit_drops
        ),
        (3_000_000, 3000, 3, 4)
    );
}

#[test]
fn a_host_root_is_read_whole_with_its_filesystems_measured() -> TestResult {
    let root = tempfile::tempdir()?;
    std::fs::create_dir_all(root.path().join("proc/1"))?;
    std::fs::create_dir_all(root.path().join("proc/net"))?;
    std::fs::create_dir_all(root.path().join("data"))?;
    std::fs::write(root.path().join("proc/stat"), STAT)?;
    std::fs::write(root.path().join("proc/meminfo"), MEMINFO)?;
    std::fs::write(root.path().join("proc/loadavg"), "1.00 2.00 3.00 1/1 1\n")?;
    std::fs::write(root.path().join("proc/diskstats"), DISKSTATS)?;
    std::fs::write(root.path().join("proc/net/dev"), NET_DEV)?;
    std::fs::write(
        root.path().join("proc/1/mounts"),
        "/dev/vda1 / ext4 rw 0 0\n/dev/vdb /data xfs rw 0 0\n/dev/vdc /missing xfs rw 0 0\n",
    )?;
    let host = ProcHostStats::at(root.path());
    let sample = host.read();
    let filesystems = host.filesystems();
    assert_eq!(sample.load, Some([1.0, 2.0, 3.0]));
    assert_eq!(sample.cpu.map(|cpu| cpu.cpus), Some(2));
    assert_eq!(sample.disks.len(), 3);
    assert_eq!(sample.interfaces.len(), 3);
    let measured = filesystems
        .iter()
        .map(|filesystem| filesystem.mount.mountpoint.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        measured,
        vec!["/", "/data"],
        "a mountpoint that is not there is left out"
    );
    assert!(
        filesystems
            .iter()
            .all(|filesystem| filesystem.usage.size > 0)
    );

    let empty = tempfile::tempdir()?;
    assert_eq!(
        ProcHostStats::at(empty.path()).read(),
        HostSample::default()
    );
    Ok(())
}

fn project(n: u128) -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(n))
}

fn shop() -> DeployedEnvironment {
    DeployedEnvironment {
        project_id: project(1),
        project: "shop".to_owned(),
        environment: "production".to_owned(),
        services: vec![
            ("app".to_owned(), "web".to_owned(), 2),
            ("app".to_owned(), "worker".to_owned(), 1),
        ],
        deployed_at_unix_ms: Some(1_700_000_000_000),
        rollout: Some(RolloutOutcome::Finished),
        metrics_networks: vec![network_key(project(1), "production", "monitoring")],
    }
}

fn blog() -> DeployedEnvironment {
    DeployedEnvironment {
        project_id: project(2),
        project: "blog".to_owned(),
        environment: "production".to_owned(),
        services: vec![("site".to_owned(), "ghost".to_owned(), 1)],
        deployed_at_unix_ms: None,
        rollout: None,
        metrics_networks: Vec::new(),
    }
}

fn running(project_id: ProjectId, stack: &str, service: &str, health: Health) -> LabelledContainer {
    LabelledContainer {
        project_id,
        environment: "production".to_owned(),
        stack: stack.to_owned(),
        service: service.to_owned(),
        health,
        restarts: 1,
    }
}

fn containers() -> Vec<LabelledContainer> {
    vec![
        running(project(1), "app", "web", Health::Healthy),
        running(project(1), "app", "web", Health::Unhealthy),
        running(project(1), "app", "migrate", Health::Stopped),
        running(project(2), "site", "ghost", Health::Starting),
        running(project(3), "x", "y", Health::Healthy),
    ]
}

#[test]
fn services_join_what_the_deployment_runs_with_what_the_engine_runs() {
    let samples = environment_samples(&[shop(), blog()], &containers());
    assert_eq!(
        samples.len(),
        2,
        "a container of an environment the node does not run is left out"
    );
    let shop = &samples[0];
    let services = shop
        .services
        .iter()
        .map(|service| {
            (
                service.service.as_str(),
                service.desired,
                service.running(),
                service.containers,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        services,
        vec![
            ("migrate", None, 0, [0, 0, 0, 1]),
            ("web", Some(2), 2, [1, 0, 1, 0]),
            ("worker", Some(1), 0, [0, 0, 0, 0]),
        ]
    );
    assert_eq!(shop.services[1].restarts, 2);
}

#[test]
fn a_container_is_read_from_the_engines_inspect_document() {
    let inspected = serde_json::json!({
        "Id": "abc",
        "RestartCount": 3,
        "State": {"Status": "running", "Health": {"Status": "unhealthy"}},
        "Config": {"Labels": {
            "io.mm.project-id": "00000000-0000-0000-0000-000000000001",
            "io.mm.environment": "production",
            "io.mm.stack": "app",
            "com.docker.compose.service": "web"
        }}
    });
    assert_eq!(
        LabelledContainer::from_inspected(&inspected),
        Some(LabelledContainer {
            restarts: 3,
            ..running(project(1), "app", "web", Health::Unhealthy)
        })
    );
    let mut oneoff = inspected.clone();
    oneoff["Config"]["Labels"]["com.docker.compose.oneoff"] = "True".into();
    assert_eq!(LabelledContainer::from_inspected(&oneoff), None);
    let mut foreign = inspected;
    foreign["Config"]["Labels"]["io.mm.project-id"] = "not-a-uuid".into();
    assert_eq!(LabelledContainer::from_inspected(&foreign), None);
}

fn snapshot() -> NodeSnapshot {
    NodeSnapshot {
        node: "app-1".to_owned(),
        version: "0.1.2".to_owned(),
        engine: "docker-compose".to_owned(),
        collect_seconds: 0.25,
        host: Some(HostSample {
            load: Some([0.5, 0.25, 0.125]),
            ..HostSample::default()
        }),
        filesystems: Some(Vec::new()),
        peers: Some(vec![PeerSample {
            peer: "db-1".to_owned(),
            handshake_age_seconds: Some(30),
            received_bytes: 10,
            sent_bytes: 20,
            on_interface: true,
        }]),
        environments: Some(environment_samples(&[shop(), blog()], &containers())),
    }
}

#[test]
fn a_scrape_sees_the_host_and_its_own_environment_only() {
    let scope = MetricsScope::Environment {
        project_id: project(1),
        environment: "production".to_owned(),
    };
    let text = render(&snapshot(), &scope, 1024 * 1024);
    assert!(
        text.contains(
            "mm_node_info{node=\"app-1\",version=\"0.1.2\",engine=\"docker-compose\"} 1\n"
        )
    );
    assert!(text.contains("mm_host_load1{node=\"app-1\"} 0.5\n"));
    assert!(text.contains("mm_mesh_peer_up{node=\"app-1\",peer=\"db-1\"} 1\n"));
    assert!(text.contains(
        "mm_service_desired_containers{node=\"app-1\",project=\"shop\",environment=\"production\",stack=\"app\",service=\"web\"} 2\n"
    ));
    assert!(text.contains(
        "mm_service_containers{node=\"app-1\",project=\"shop\",environment=\"production\",stack=\"app\",service=\"web\",health=\"unhealthy\"} 1\n"
    ));
    assert!(text.contains(
        "mm_environment_last_deployment_timestamp_seconds{node=\"app-1\",project=\"shop\",environment=\"production\"} 1700000000\n"
    ));
    assert!(text.contains("mm_metrics_collected{node=\"app-1\",part=\"services\"} 1\n"));
    assert!(!text.contains("blog"), "another project's names never show");
    assert!(!text.contains("ghost"));
    assert!(text.contains("# TYPE mm_service_containers gauge\n"));

    let other = MetricsScope::Environment {
        project_id: project(1),
        environment: "staging".to_owned(),
    };
    assert!(!render(&snapshot(), &other, 1024 * 1024).contains("shop"));

    let everything = render(&snapshot(), &MetricsScope::Host, 1024 * 1024);
    assert!(everything.contains("project=\"blog\""));
    assert!(everything.contains("project=\"shop\""));
}

#[test]
fn label_values_are_escaped_and_the_answer_stays_within_its_limit() {
    let mut odd = snapshot();
    odd.node = "a\"b\\c\nd".to_owned();
    let text = render(&odd, &MetricsScope::Host, 1024 * 1024);
    assert!(text.contains("node=\"a\\\"b\\\\c\\nd\""));

    let cut = render(&snapshot(), &MetricsScope::Host, 2048);
    assert!(cut.len() <= 2048, "{} bytes", cut.len());
    assert!(cut.ends_with("mm_metrics_truncated 1\n"));
    assert!(!render(&snapshot(), &MetricsScope::Host, 1024 * 1024).contains("truncated"));

    let mut missing = snapshot();
    missing.host = None;
    missing.environments = None;
    let text = render(&missing, &MetricsScope::Host, 1024 * 1024);
    assert!(text.contains("mm_metrics_collected{node=\"app-1\",part=\"host\"} 0\n"));
    assert!(!text.contains("mm_service_"));
}

fn node_networks() -> Result<NodeNetworks, String> {
    let space = "10.64.0.0/12".parse()?;
    let mut table = NetworkTable::default();
    for (key, node) in [
        (network_key(project(1), "production", "monitoring"), "app-1"),
        (network_key(project(1), "production", "monitoring"), "app-2"),
        (network_key(project(1), "production", "backend"), "app-1"),
    ] {
        table.assign(
            space,
            &NetworkNeed {
                key: &key,
                node,
                network: "mm-net",
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

fn mesh() -> Result<MeshState, Box<dyn std::error::Error>> {
    Ok(MeshState {
        interface: "mm0".to_owned(),
        address: "10.90.0.1".parse()?,
        prefix: 24,
        port: 51820,
        mtu: 1420,
        peers: vec![MeshPeer {
            name: "app-2".to_owned(),
            public_key: "key".to_owned(),
            endpoint: "203.0.113.12:51820".to_owned(),
            allowed_ips: vec!["10.90.0.2/32".parse()?, "10.64.1.0/24".parse()?],
        }],
    })
}

#[test]
fn a_node_listens_on_each_network_that_asks_and_on_the_mesh_for_its_admin() -> TestResult {
    let networks = node_networks()?;
    let mesh = mesh()?;
    let policy = HostMetrics::default();
    let wanted = listeners(&policy, Some(&networks), &[shop(), blog()], Some(&mesh));
    assert_eq!(
        wanted.len(),
        1,
        "only `monitoring` asks, and the mesh only for an admin"
    );
    let listener = &wanted[0];
    assert_eq!(listener.address, Ipv4Addr::new(10, 64, 0, 1));
    assert_eq!(
        listener.scope,
        MetricsScope::Environment {
            project_id: project(1),
            environment: "production".to_owned()
        }
    );
    assert!(listener.admits(Ipv4Addr::new(10, 64, 0, 7)));
    assert!(
        listener.admits(Ipv4Addr::new(10, 64, 1, 7)),
        "a member on another node"
    );
    assert!(
        !listener.admits(Ipv4Addr::new(10, 64, 16, 2)),
        "a container of another network"
    );
    assert!(
        !listener.admits(Ipv4Addr::new(10, 64, 0, 1)),
        "the node itself, as any process on the host reaches it"
    );
    assert!(listener.own.contains(&Ipv4Addr::new(10, 64, 16, 1)));
    assert!(listener.own.contains(&Ipv4Addr::new(10, 90, 0, 1)));

    let admin = HostMetrics {
        mesh: true,
        mesh_token_file: Some("/etc/mm/metrics-mesh-token".into()),
        ..HostMetrics::default()
    };
    let wanted = listeners(&admin, Some(&networks), &[shop()], Some(&mesh));
    let host = wanted.last().ok_or("no mesh listener")?;
    assert_eq!(
        (host.address, &host.scope),
        (Ipv4Addr::new(10, 90, 0, 1), &MetricsScope::Host)
    );
    assert!(host.admits(Ipv4Addr::new(10, 90, 0, 2)));
    assert!(
        !host.admits(Ipv4Addr::new(10, 90, 0, 1)),
        "the node's own host, where every project's host-network service runs"
    );
    assert!(
        !host.admits(Ipv4Addr::new(10, 90, 0, 9)),
        "not a node of the mesh"
    );
    assert!(
        !host.admits(Ipv4Addr::new(10, 64, 1, 7)),
        "a container is not a node"
    );

    let off = HostMetrics {
        enabled: false,
        ..admin
    };
    assert!(listeners(&off, Some(&networks), &[shop()], Some(&mesh)).is_empty());
    assert!(
        listeners(&policy, None, &[shop()], None).is_empty(),
        "no part of the network here"
    );
    Ok(())
}

#[test]
fn the_host_policy_reads_metrics_and_refuses_a_mesh_without_a_token() -> TestResult {
    let read: HostMetrics = serde_yaml_ng::from_str(
        "token-file: /etc/mm/metrics-token\nmesh: true\nmesh-token-file: /etc/mm/metrics-mesh-token\n",
    )?;
    assert!(read.enabled && read.mesh);
    read.validate()?;
    let open: HostMetrics = serde_yaml_ng::from_str("mesh: true\n")?;
    assert!(
        open.validate()
            .is_err_and(|problem| problem.contains("metrics.mesh-token-file"))
    );
    let project_token: HostMetrics =
        serde_yaml_ng::from_str("token-file: /etc/mm/metrics-token\nmesh: true\n")?;
    assert!(
        project_token.validate().is_err(),
        "the projects' token never unlocks every project"
    );
    let shared: HostMetrics =
        serde_yaml_ng::from_str("token-file: /etc/mm/t\nmesh: true\nmesh-token-file: /etc/mm/t\n")?;
    assert!(
        shared
            .validate()
            .is_err_and(|problem| problem.contains("another file"))
    );
    let relative: HostMetrics = serde_yaml_ng::from_str("token-file: token\n")?;
    assert!(relative.validate().is_err());
    assert_eq!(
        serde_yaml_ng::to_string(&HostMetrics::default())?.trim(),
        "{}"
    );
    Ok(())
}

#[test]
fn a_request_is_get_or_head_of_metrics_with_an_optional_bearer_token() {
    let request =
        parse_request("GET /metrics HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer s3cret \r\n");
    assert_eq!(
        request.as_ref().map(|request| request.bearer.as_deref()),
        Ok(Some("s3cret"))
    );
    assert_eq!(
        parse_request("HEAD /metrics?x=1 HTTP/1.0").map(|request| (request.head, request.bearer)),
        Ok((true, None))
    );
    assert_eq!(
        parse_request("GET / HTTP/1.1"),
        Err(RequestProblem::NotFound)
    );
    assert_eq!(
        parse_request("POST /metrics HTTP/1.1"),
        Err(RequestProblem::MethodNotAllowed)
    );
    assert_eq!(
        parse_request("GET /metrics"),
        Err(RequestProblem::BadRequest)
    );
    assert_eq!(
        parse_request("GET /metrics HTTP/1.1\r\nAuthorization: Basic abc")
            .map(|request| request.bearer),
        Ok(None)
    );
}

struct Fixed;

impl HostStats for Fixed {
    fn read(&self) -> HostSample {
        HostSample {
            load: Some([1.0, 1.0, 1.0]),
            ..HostSample::default()
        }
    }

    fn filesystems(&self) -> Vec<FilesystemSample> {
        Vec::new()
    }
}

/// A host one of whose filesystems hangs.
struct Hanging;

impl HostStats for Hanging {
    fn read(&self) -> HostSample {
        Fixed.read()
    }

    fn filesystems(&self) -> Vec<FilesystemSample> {
        std::thread::sleep(std::time::Duration::from_secs(8));
        Vec::new()
    }
}

#[async_trait::async_trait]
impl NodeContainers for Fixed {
    async fn containers(&self) -> Result<Vec<LabelledContainer>, MmError> {
        Ok(containers())
    }
}

#[async_trait::async_trait]
impl MeshPeers for Fixed {
    async fn peers(&self) -> Result<Option<Vec<PeerSample>>, MmError> {
        Ok(None)
    }
}

impl DeployedEnvironments for Fixed {
    fn deployed(&self) -> Result<Vec<DeployedEnvironment>, MmError> {
        Ok(vec![shop(), blog()])
    }
}

fn collector() -> Arc<NodeMetricsCollector> {
    collector_on(Arc::new(Fixed))
}

fn collector_on(host: Arc<dyn HostStats>) -> Arc<NodeMetricsCollector> {
    let fixed = Arc::new(Fixed);
    NodeMetricsCollector::new(
        NodeIdentity {
            node: "app-1".to_owned(),
            version: "0.1.2".to_owned(),
            engine: "podman-compose".to_owned(),
        },
        NodeMetricsSources {
            host,
            containers: fixed.clone(),
            mesh: fixed.clone(),
            deployed: fixed,
        },
    )
}

async fn exchange(
    request: &str,
    scrape: &Scrape<'_>,
) -> Result<String, Box<dyn std::error::Error>> {
    let collector = collector();
    let (mut client, mut server) = tokio::io::duplex(1024 * 1024);
    client.write_all(request.as_bytes()).await?;
    answer(&mut server, scrape, &collector).await?;
    drop(server);
    let mut response = String::new();
    client.read_to_string(&mut response).await?;
    Ok(response)
}

#[tokio::test]
async fn a_scrape_is_answered_only_for_an_admitted_peer_with_the_token() -> TestResult {
    let scope = MetricsScope::Environment {
        project_id: project(1),
        environment: "production".to_owned(),
    };
    let token = b"s3cret".as_slice();
    let scrape = Scrape {
        admitted: true,
        token: Some(token),
        scope: &scope,
    };
    let get = "GET /metrics HTTP/1.1\r\nAuthorization: Bearer s3cret\r\n\r\n";
    let ok = exchange(get, &scrape).await?;
    assert!(ok.starts_with("HTTP/1.1 200 OK\r\n"), "{ok}");
    assert!(ok.contains("Content-Type: text/plain; version=0.0.4"));
    assert!(ok.contains("engine=\"podman-compose\""));
    assert!(ok.contains("mm_host_load1{node=\"app-1\"} 1\n"));
    assert!(ok.contains("service=\"web\""));
    assert!(!ok.contains("ghost"));

    let wrong = exchange(
        "GET /metrics HTTP/1.1\r\nAuthorization: Bearer nope\r\n\r\n",
        &scrape,
    )
    .await?;
    assert!(wrong.starts_with("HTTP/1.1 401 Unauthorized\r\n"));
    assert!(wrong.contains("WWW-Authenticate: Bearer\r\n"));
    assert!(!wrong.contains("mm_"));
    let missing = exchange("GET /metrics HTTP/1.1\r\n\r\n", &scrape).await?;
    assert!(missing.starts_with("HTTP/1.1 401"));

    let stranger = Scrape {
        admitted: false,
        ..scrape
    };
    assert!(
        exchange(get, &stranger)
            .await?
            .starts_with("HTTP/1.1 403 Forbidden\r\n")
    );

    let open = Scrape {
        token: None,
        ..scrape
    };
    let head = exchange("HEAD /metrics HTTP/1.1\r\n\r\n", &open).await?;
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n") && head.ends_with("\r\n\r\n"));
    assert!(
        exchange("GET /other HTTP/1.1\r\n\r\n", &open)
            .await?
            .starts_with("HTTP/1.1 404")
    );
    let long = format!("GET /metrics HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(9000));
    assert!(exchange(&long, &open).await?.starts_with("HTTP/1.1 400"));
    Ok(())
}

#[tokio::test]
async fn a_filesystem_that_hangs_leaves_only_the_filesystems_out() {
    let snapshot = collector_on(Arc::new(Hanging)).snapshot().await;
    assert_eq!(
        snapshot.host.as_ref().and_then(|host| host.load),
        Some([1.0, 1.0, 1.0])
    );
    assert_eq!(snapshot.filesystems, None);
    let text = render(&snapshot, &MetricsScope::Host, 1024 * 1024);
    assert!(text.contains("mm_metrics_collected{node=\"app-1\",part=\"filesystems\"} 0\n"));
    assert!(text.contains("mm_metrics_collected{node=\"app-1\",part=\"host\"} 1\n"));
}

fn on_loopback(address: [u8; 4], own: Vec<Ipv4Addr>, scope: MetricsScope) -> MetricsListener {
    MetricsListener {
        address: Ipv4Addr::from(address),
        peers: vec![Ipv4Cidr {
            address: Ipv4Addr::new(127, 0, 0, 0),
            prefix: 8,
        }],
        own,
        scope,
    }
}

async fn scrape_at(
    address: std::net::SocketAddr,
    token: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(
            format!("GET /metrics HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n").as_bytes(),
        )
        .await?;
    let mut response = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        stream.read_to_end(&mut response),
    )
    .await?;
    Ok(String::from_utf8_lossy(&response).into_owned())
}

#[tokio::test]
async fn a_server_keeps_its_socket_closes_on_the_node_itself_and_splits_its_tokens() -> TestResult {
    let server = MetricsServer::new(collector(), 0);
    server.set_tokens(
        Some(zeroize::Zeroizing::new(b"project".to_vec())),
        Some(zeroize::Zeroizing::new(b"mesh".to_vec())),
    );
    let environment = MetricsScope::Environment {
        project_id: project(1),
        environment: "production".to_owned(),
    };
    let problems = server.serve(vec![
        on_loopback([127, 0, 0, 1], Vec::new(), environment.clone()),
        on_loopback([127, 0, 0, 2], Vec::new(), MetricsScope::Host),
    ]);
    assert!(problems.is_empty(), "{problems:?}");
    let listening = server.listening();
    let (project_address, mesh_address) = (listening[0], listening[1]);

    assert!(
        scrape_at(project_address, "project")
            .await?
            .starts_with("HTTP/1.1 200 OK")
    );
    assert!(
        scrape_at(project_address, "mesh")
            .await?
            .starts_with("HTTP/1.1 401")
    );
    assert!(
        scrape_at(mesh_address, "project")
            .await?
            .starts_with("HTTP/1.1 401"),
        "a project's token never reads every project"
    );
    let everything = scrape_at(mesh_address, "mesh").await?;
    assert!(everything.starts_with("HTTP/1.1 200 OK") && everything.contains("project=\"blog\""));

    let problems = server.serve(vec![on_loopback(
        [127, 0, 0, 1],
        vec![Ipv4Addr::new(127, 0, 0, 1)],
        environment,
    )]);
    assert!(
        problems.is_empty(),
        "the same address changes in place: {problems:?}"
    );
    assert_eq!(server.listening(), vec![project_address]);
    assert_eq!(
        scrape_at(project_address, "project")
            .await
            .unwrap_or_default(),
        "",
        "the node's own address is closed without an answer"
    );
    Ok(())
}

#[tokio::test]
async fn a_peer_holds_one_connection_of_an_address_at_a_time() -> TestResult {
    let server = MetricsServer::new(collector(), 0);
    let problems = server.serve(vec![on_loopback(
        [127, 0, 0, 3],
        Vec::new(),
        MetricsScope::Host,
    )]);
    assert!(problems.is_empty(), "{problems:?}");
    let address = server.listening()[0];
    let slow = tokio::net::TcpStream::connect(address).await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(
        scrape_at(address, "").await.unwrap_or_default(),
        "",
        "a second connection of the same peer is closed unanswered"
    );
    drop(slow);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(scrape_at(address, "").await?.starts_with("HTTP/1.1 200 OK"));
    Ok(())
}

#[test]
fn the_mesh_token_is_never_the_projects_under_another_name() -> TestResult {
    use std::os::unix::fs::PermissionsExt as _;
    let directory = tempfile::tempdir()?;
    let write = |name: &str, content: &str| -> std::io::Result<std::path::PathBuf> {
        let path = directory.path().join(name);
        std::fs::write(&path, content)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(path)
    };
    let project = write("project", "first\n")?;
    let mesh = write("mesh", "second\n")?;
    let (read_project, read_mesh) = read_tokens(Some(&project), Some(&mesh))?;
    assert_eq!(
        read_project.as_deref().map(Vec::as_slice),
        Some(b"first".as_slice())
    );
    assert_eq!(
        read_mesh.as_deref().map(Vec::as_slice),
        Some(b"second".as_slice())
    );

    let linked = directory.path().join("linked");
    std::fs::hard_link(&project, &linked)?;
    assert!(
        read_tokens(Some(&project), Some(&linked)).is_err(),
        "a hard link"
    );
    let dotted = directory.path().join("sub/../project");
    std::fs::create_dir(directory.path().join("sub"))?;
    assert!(
        read_tokens(Some(&project), Some(&dotted)).is_err(),
        "another path"
    );
    let copy = write("copy", "first")?;
    assert!(
        read_tokens(Some(&project), Some(&copy))
            .is_err_and(|problem| problem.contains("token of its own")),
        "the same token in another file"
    );
    let open = write("open", "x")?;
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o644))?;
    assert!(
        read_tokens(None, Some(&open)).is_err(),
        "readable by others"
    );
    let link = directory.path().join("symlink");
    std::os::unix::fs::symlink(&mesh, &link)?;
    assert!(read_tokens(Some(&link), None).is_err(), "a symbolic link");
    Ok(())
}
