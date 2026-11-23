//! A service that runs on several nodes of a cluster target, as the client
//! reads it: one status row with every node's share, and one log stream in
//! the order the lines were written, against scripted nodes.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use mm::app::ports::{ExecStream, LogStream, RemoteTransport};
use mm::app::{RemoteEnvironment, RemoteNode};
use mm::domain::build::BuildReceipt;
use mm::domain::deployment::DeploymentResult;
use mm::domain::error::MmError;
use mm::domain::observability::{
    ApprovalContext, ExecRequest, LogRecord, LogRequest, LogSource, OperationCancellation,
    ServiceSelector,
};
use mm::domain::placement::{ClusterNodes, services_on_node};
use mm::domain::remote::{
    ClusterPlanSeal, NodeFacts, RemoteAccessPlan, RemoteDeploymentPlan, RemoteIdentity, RemoteScope,
};
use mm::domain::runtime::{
    ContainerObservation, RuntimeKind, ServiceIdentity, ServiceSnapshot, ServiceState,
};
use mm::domain::users::AccessChange;
use mm::domain::workspace::{AppliedDeployment, ProjectId, StackSnapshot, WorkspaceSnapshot};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Cluster = (RemoteEnvironment, Vec<Arc<Node>>);

const NAMES: [&str; 3] = ["app-1", "app-2", "db-1"];

/// `web` runs on every node, `worker` on the first one.
const MODULES: &str = "\
shop:
  path: shop
  services:
    web: {stack: app, placement: all}
    worker: {stack: app}
";

/// How a scripted node answers a status.
enum Status {
    /// With these services and what it knows of itself.
    Answers(Box<Share>),
    /// Not at all: the request stays open.
    Silent,
    /// With a failure, at once.
    Unreachable,
}

/// One node's part of a status.
#[derive(Default)]
struct Share {
    services: Vec<ServiceSnapshot>,
    deployment: Option<AppliedDeployment>,
    drift: Vec<String>,
    network_drift: Vec<String>,
}

/// What a scripted node's log stream delivers.
enum Line {
    Written(Box<LogRecord>),
    /// The stream fails here.
    Broken(&'static str),
}

/// How a scripted node answers a log read.
enum Logs {
    /// These lines, then the stream ends.
    Lines(Vec<Line>),
    /// The stream the test feeds.
    Fed(Option<tokio::sync::mpsc::Receiver<Result<LogRecord, MmError>>>),
    Unreachable,
}

struct Node {
    name: String,
    status: Mutex<Status>,
    logs: Mutex<Logs>,
    calls: Mutex<Vec<String>>,
    /// Every log request the node was sent.
    asked: Mutex<Vec<LogRequest>>,
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Node {
    fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_owned(),
            status: Mutex::new(Status::Answers(Box::default())),
            logs: Mutex::new(Logs::Lines(Vec::new())),
            calls: Mutex::new(Vec::new()),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<String> {
        locked(&self.calls).clone()
    }

    fn answers(&self, share: Share) {
        *locked(&self.status) = Status::Answers(Box::new(share));
    }

    fn writes(&self, lines: Vec<Line>) {
        *locked(&self.logs) = Logs::Lines(lines);
    }

    /// The node's log stream stays open; the test writes to it.
    fn fed(&self) -> tokio::sync::mpsc::Sender<Result<LogRecord, MmError>> {
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        *locked(&self.logs) = Logs::Fed(Some(receiver));
        sender
    }

    fn unreachable(&self) -> MmError {
        MmError::Unreachable {
            subject: format!("node '{}'", self.name),
            problem: "did not answer (Connection timed out)".to_owned(),
            help: "Check that the host is up".to_owned(),
        }
    }
}

fn unused<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not used by this test".to_owned(),
    })
}

#[async_trait]
impl RemoteTransport for Node {
    async fn status(&self, scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        locked(&self.calls).push("status".to_owned());
        let mut snapshot = snapshot(&scope);
        let silent = match &*locked(&self.status) {
            Status::Answers(share) => {
                snapshot.stacks = vec![StackSnapshot {
                    name: "app".to_owned(),
                    compose_project_name: "shop-production-app".to_owned(),
                    artifact_hash: "b".repeat(64),
                    rendered_config_hash: "c".repeat(64),
                    services: share.services.clone(),
                    owned_resources: Vec::new(),
                }];
                snapshot.deployment.clone_from(&share.deployment);
                snapshot.drift.clone_from(&share.drift);
                snapshot.network_drift.clone_from(&share.network_drift);
                false
            }
            Status::Silent => true,
            Status::Unreachable => return Err(self.unreachable()),
        };
        if silent {
            std::future::pending::<()>().await;
        }
        Ok(snapshot)
    }

    async fn logs(
        &self,
        _scope: RemoteScope,
        request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        locked(&self.calls).push("logs".to_owned());
        locked(&self.asked).push(request);
        let receiver = match &mut *locked(&self.logs) {
            Logs::Unreachable => return Err(self.unreachable()),
            Logs::Fed(receiver) => receiver.take().ok_or_else(|| MmError::State {
                message: "the fed stream was already read".to_owned(),
            })?,
            Logs::Lines(lines) => {
                let (sender, receiver) = tokio::sync::mpsc::channel(lines.len().max(1));
                for line in lines.iter() {
                    let item = match line {
                        Line::Written(record) => Ok(record.as_ref().clone()),
                        Line::Broken(message) => Err(MmError::RuntimeResponse {
                            message: (*message).to_owned(),
                        }),
                    };
                    sender.try_send(item).map_err(|_| MmError::State {
                        message: "the scripted stream is full".to_owned(),
                    })?;
                }
                receiver
            }
        };
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        _scope: RemoteScope,
        request: ExecRequest,
        _cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        locked(&self.calls).push(format!(
            "exec {}",
            request.selector.container.as_deref().unwrap_or("-")
        ));
        unused()
    }

    async fn plan_deployment(
        &self,
        scope: RemoteScope,
        _commit: String,
        _receipt: Option<BuildReceipt>,
        _release: mm::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError> {
        // Every scripted node runs Docker, as setup installed it.
        Err(MmError::Target {
            message: format!(
                "node '{}' runs docker-compose, as `mm target setup` installed it, but environment '{}' uses podman-compose",
                self.name, scope.environment
            ),
        })
    }

    async fn apply_deployment(
        &self,
        _scope: RemoteScope,
        _remote_plan_hash: String,
    ) -> Result<Option<DeploymentResult>, MmError> {
        unused()
    }

    async fn networks(
        &self,
        _scope: RemoteScope,
    ) -> Result<mm::domain::cluster_network::NodeNetworkStatus, MmError> {
        locked(&self.calls).push("networks".to_owned());
        Ok(mm::domain::cluster_network::NodeNetworkStatus::default())
    }

    async fn seal_plan(
        &self,
        _scope: RemoteScope,
        _seal: ClusterPlanSeal,
    ) -> Result<ClusterPlanSeal, MmError> {
        unused()
    }

    async fn sealed_plan(
        &self,
        _scope: RemoteScope,
        _cluster_plan_hash: String,
    ) -> Result<ClusterPlanSeal, MmError> {
        unused()
    }

    async fn node(&self, _scope: RemoteScope) -> Result<NodeFacts, MmError> {
        unused()
    }

    async fn plan_team_sync(
        &self,
        _scope: RemoteScope,
        _commit: String,
    ) -> Result<RemoteAccessPlan, MmError> {
        unused()
    }

    async fn apply_team_sync(
        &self,
        _scope: RemoteScope,
        _remote_plan_hash: String,
    ) -> Result<AccessChange, MmError> {
        unused()
    }

    async fn whoami(&self, _scope: RemoteScope) -> Result<RemoteIdentity, MmError> {
        unused()
    }

    async fn forward(
        &self,
        _scope: RemoteScope,
        _service: String,
        _port: u16,
        _local: tokio::net::TcpStream,
    ) -> Result<mm::domain::forward::ForwardTotals, MmError> {
        unused()
    }
}

fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(7))
}

fn scope(node: &str) -> RemoteScope {
    RemoteScope {
        project_id: project(),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: Some(node.to_owned()),
    }
}

fn snapshot(scope: &RemoteScope) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        workspace: mm::domain::workspace::WorkspaceRecord {
            id: mm::domain::workspace::WorkspaceId(uuid::Uuid::from_u128(9)),
            project_id: scope.project_id,
            root: "/srv/mm/project".into(),
            config_path: "/srv/mm/project/.config/mm.yaml".into(),
        },
        project_name: "shop".to_owned(),
        target: scope.target.clone(),
        environment: scope.environment.clone(),
        config_hash: "a".repeat(64),
        target_source: None,
        runtime_kind: Some(scope.runtime),
        runtime_source: None,
        runtime: mm::domain::runtime::RuntimeProbe {
            kind: scope.runtime,
            available: true,
            engine_version: None,
            compose_provider: None,
            rootless: None,
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: Vec::new(),
        },
        validation: mm::domain::diagnostic::ValidationReport {
            diagnostics: Vec::new(),
        },
        stacks: Vec::new(),
        ingress: Vec::new(),
        recent_operations: Vec::new(),
        nodes: Vec::new(),
        names: None,
        deployment: None,
        drift: Vec::new(),
        network_drift: Vec::new(),
        optional: Vec::new(),
        tunnel: None,
    }
}

/// A three-node cluster and the scripted daemons behind it.
fn cluster() -> Result<Cluster, Box<dyn std::error::Error>> {
    let modules: BTreeMap<String, mm::domain::config::ModuleConfig> =
        serde_yaml_ng::from_str(MODULES)?;
    let groups = BTreeMap::new();
    let fakes = NAMES.iter().map(|name| Node::new(name)).collect::<Vec<_>>();
    let mut nodes = Vec::new();
    for (index, fake) in fakes.iter().enumerate() {
        let placed = services_on_node(
            &modules,
            "production",
            ClusterNodes {
                nodes: &NAMES,
                groups: &groups,
            },
            &fake.name,
        )?;
        nodes.push(RemoteNode {
            name: Some(fake.name.clone()),
            host: format!("203.0.113.{}", index + 1),
            port: 22,
            repo_address: None,
            private_address: Some(Ipv4Addr::new(10, 90, 0, u8::try_from(index + 1)?)),
            transport: Ok(Arc::clone(fake) as Arc<dyn RemoteTransport>),
            scope: scope(&fake.name),
            placed: Some(placed),
        });
    }
    Ok((
        RemoteEnvironment::new("production".to_owned(), nodes)?,
        fakes,
    ))
}

/// One container of `service` as an engine lists it.
fn container(service: &str, running: bool) -> ContainerObservation {
    ContainerObservation {
        id: format!("{service}-id"),
        name: format!("shop-production-app-{service}-1"),
        service: service.to_owned(),
        image: "registry.example.test/shop:1".to_owned(),
        state: if running { "running" } else { "exited" }.to_owned(),
        status: if running { "Up 2 hours" } else { "Exited (1)" }.to_owned(),
        health: None,
        restart_count: None,
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        revision: None,
        digest: None,
        diagnostic_code: None,
        error: None,
        labels: BTreeMap::new(),
        node: None,
        history: false,
    }
}

/// A node's own row for `service`: one container wanted, running or not
/// there at all.
fn service(name: &str, state: ServiceState) -> ServiceSnapshot {
    let running = u32::from(state == ServiceState::Running);
    ServiceSnapshot {
        stack: "app".to_owned(),
        name: name.to_owned(),
        module: Some("shop".to_owned()),
        nodes: Vec::new(),
        state,
        starting: false,
        desired_count: 1,
        running_count: running,
        healthy_count: running,
        image: None,
        revision: None,
        restart_count: None,
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        diagnostic_code: None,
        error: None,
        containers: if state == ServiceState::Missing {
            Vec::new()
        } else {
            vec![container(name, state == ServiceState::Running)]
        },
        scale: None,
        restarts: None,
    }
}

fn running(services: &[&str]) -> Share {
    Share {
        services: services
            .iter()
            .map(|name| service(name, ServiceState::Running))
            .collect(),
        ..Share::default()
    }
}

fn deployed(commit: &str, by: &str, at: i64) -> Option<AppliedDeployment> {
    Some(AppliedDeployment {
        commit: commit.repeat(40),
        deployed_at_unix_ms: at,
        deployed_by: by.to_owned(),
        plan_hash: Some("d".repeat(64)),
        build_receipt: None,
    })
}

#[tokio::test]
async fn a_service_on_three_nodes_is_one_row_with_every_nodes_share() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[0].answers(running(&["web", "worker"]));
    fakes[1].answers(running(&["web"]));
    fakes[2].answers(running(&["web"]));
    let snapshot = environment.status().await?;
    assert_eq!(snapshot.stacks.len(), 1, "one stack, whoever reports it");
    let services = &snapshot.stacks[0].services;
    assert_eq!(
        services
            .iter()
            .map(|service| service.name.as_str())
            .collect::<Vec<_>>(),
        ["web", "worker"],
        "each service is listed once"
    );
    let web = &services[0];
    assert_eq!(
        web.nodes
            .iter()
            .map(|share| (share.node.as_str(), share.state, share.running_count))
            .collect::<Vec<_>>(),
        [
            ("app-1", ServiceState::Running, 1),
            ("app-2", ServiceState::Running, 1),
            ("db-1", ServiceState::Running, 1),
        ],
        "the nodes in the target's declared order"
    );
    assert_eq!(
        (web.desired_count, web.running_count, web.healthy_count),
        (3, 3, 3)
    );
    assert_eq!(web.state, ServiceState::Running);
    assert_eq!(
        web.containers
            .iter()
            .map(|container| container.node.as_deref())
            .collect::<Vec<_>>(),
        [Some("app-1"), Some("app-2"), Some("db-1")],
        "every container says which node it is on"
    );
    let worker = &services[1];
    assert_eq!(
        worker
            .nodes
            .iter()
            .map(|share| share.node.as_str())
            .collect::<Vec<_>>(),
        ["app-1"]
    );
    assert_eq!((worker.desired_count, worker.running_count), (1, 1));

    // app-2 has no container of web: the service is degraded, and the row
    // of app-2 says it is the one.
    fakes[1].answers(Share {
        services: vec![service("web", ServiceState::Missing)],
        ..Share::default()
    });
    let snapshot = environment.status().await?;
    let web = &snapshot.stacks[0].services[0];
    assert_eq!(web.state, ServiceState::Degraded);
    assert_eq!(
        (web.desired_count, web.running_count, web.healthy_count),
        (3, 2, 2)
    );
    assert_eq!(
        web.nodes
            .iter()
            .map(|share| share.state)
            .collect::<Vec<_>>(),
        [
            ServiceState::Running,
            ServiceState::Missing,
            ServiceState::Running
        ]
    );
    assert_eq!(web.containers.len(), 2);
    Ok(())
}

#[tokio::test]
async fn status_carries_each_nodes_deployment_and_drift_from_its_own_answer() -> TestResult {
    let (environment, fakes) = cluster()?;
    let environment =
        environment.with_cluster_networks(Some(mm::domain::cluster_network::SealedNetworks {
            space: "10.64.0.0/12".parse()?,
            mesh_interface: "mm0".to_owned(),
            mtu: 1420,
            table: mm::domain::cluster_network::NetworkTable::default(),
            retired: Vec::new(),
        }));
    // The same release everywhere, applied by different people at
    // different times.
    fakes[0].answers(Share {
        deployment: deployed("c", "alice", 5),
        ..running(&["web"])
    });
    fakes[1].answers(Share {
        deployment: deployed("c", "bob", 9),
        drift: vec!["web is placed here and has no container".to_owned()],
        network_drift: vec!["the chain MM-CLUSTER is missing".to_owned()],
        ..running(&[])
    });
    fakes[2].answers(Share {
        deployment: deployed("c", "alice", 7),
        ..running(&["web"])
    });
    let snapshot = environment.status().await?;
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .map(|node| node.deployment.as_ref().map(|at| at.deployed_by.as_str()))
            .collect::<Vec<_>>(),
        [Some("alice"), Some("bob"), Some("alice")]
    );
    assert_eq!(
        snapshot.deployment, snapshot.nodes[0].deployment,
        "every node runs the same release: it is the environment's"
    );
    assert!(snapshot.nodes[0].drift.is_empty() && snapshot.nodes[0].network_drift.is_empty());
    assert_eq!(
        snapshot.nodes[1].drift,
        ["web is placed here and has no container"]
    );
    assert_eq!(
        snapshot.nodes[1].network_drift,
        ["the chain MM-CLUSTER is missing"]
    );
    for fake in &fakes {
        assert_eq!(
            fake.calls(),
            ["status"],
            "a status asks each node one thing"
        );
    }

    // A node that does not answer has no say in what the environment runs.
    *locked(&fakes[2].status) = Status::Unreachable;
    let snapshot = environment.status().await?;
    assert_eq!(snapshot.nodes[2].deployment, None);
    assert_eq!(snapshot.deployment, snapshot.nodes[0].deployment);
    assert!(snapshot.deployment.is_some());

    // One node on another commit, or one that never finished a deployment:
    // the environment has no one release.
    fakes[2].answers(Share {
        deployment: deployed("e", "alice", 7),
        ..running(&["web"])
    });
    let snapshot = environment.status().await?;
    assert_eq!(snapshot.deployment, None);
    assert_eq!(
        snapshot.nodes[2]
            .deployment
            .as_ref()
            .map(|deployment| deployment.commit.clone()),
        Some("e".repeat(40))
    );
    fakes[2].answers(running(&["web"]));
    assert_eq!(environment.status().await?.deployment, None);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_node_that_never_answers_costs_a_status_five_seconds_and_is_named() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[0].answers(running(&["web"]));
    *locked(&fakes[1].status) = Status::Silent;
    *locked(&fakes[2].status) = Status::Unreachable;
    let began = tokio::time::Instant::now();
    let so_far = Mutex::new(Vec::new());
    let snapshot = environment
        .status_reporting(Some(&|snapshot| locked(&so_far).push(snapshot)))
        .await?;
    let waited = began.elapsed();
    assert!(
        (mm::app::SILENT_NODE_PATIENCE..mm::app::SILENT_NODE_PATIENCE + Duration::from_secs(1))
            .contains(&waited),
        "waited {waited:?}"
    );
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .map(|node| (node.name.as_str(), node.reachable))
            .collect::<Vec<_>>(),
        [("app-1", true), ("app-2", false), ("db-1", false)]
    );
    let silent = snapshot.nodes[1].problem.as_deref().unwrap_or_default();
    assert!(
        silent.starts_with("has not answered after 5 seconds"),
        "{silent}"
    );
    assert!(silent.contains("mm target nodes"), "{silent}");
    assert_eq!(
        snapshot.nodes[2].problem.as_deref(),
        Some("did not answer (Connection timed out). Check that the host is up")
    );
    // What app-1 said is all there, and `web` is not whole: this checkout
    // places it on the two nodes that did not answer too, and nobody knows
    // whether it runs there.
    let web = &snapshot.stacks[0].services[0];
    assert_eq!(
        web.nodes
            .iter()
            .map(|share| (
                share.node.as_str(),
                share.state,
                share.desired_count,
                share.running_count
            ))
            .collect::<Vec<_>>(),
        [
            ("app-1", ServiceState::Running, 1, 1),
            ("app-2", ServiceState::Unknown, 1, 0),
            ("db-1", ServiceState::Unknown, 1, 0),
        ]
    );
    assert_eq!(web.state, ServiceState::Degraded);
    assert_eq!(
        (web.desired_count, web.running_count, web.healthy_count),
        (3, 1, 1)
    );
    assert_eq!(web.containers.len(), 1);
    assert!(snapshot.nodes.iter().all(|node| !node.is_pending()));
    // While app-2 was only being waited for, it was not called silent (a
    // node that is slow is not yet a node that is silent), and web was not
    // counted whole without it: its share there is not known yet.
    let so_far = locked(&so_far);
    assert!(!so_far.is_empty());
    for interim in so_far.iter() {
        let pending = interim
            .nodes
            .iter()
            .filter(|node| node.is_pending())
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        assert!(pending.contains(&"app-2"), "{pending:?}");
        let web = &interim.stacks[0].services[0];
        assert!(
            web.nodes
                .iter()
                .filter(|share| pending.contains(&share.node.as_str()))
                .all(|share| share.state == ServiceState::Unknown && share.running_count == 0),
            "a pending node's share is not known: {web:?}"
        );
        assert_eq!(web.desired_count, 3, "{web:?}");
    }
    Ok(())
}

/// The cells of the status table's row that holds `node`'s share of the
/// service: everything from the node's name on.
fn row_from(lines: &[String], node: &str) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .find_map(|cells| {
            let at = cells.iter().position(|cell| *cell == node)?;
            Some(cells[at..].iter().map(|cell| (*cell).to_owned()).collect())
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a_service_placed_on_a_node_that_does_not_answer_is_not_whole() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[0].answers(running(&["web", "worker"]));
    fakes[1].answers(running(&["web"]));
    *locked(&fakes[2].status) = Status::Unreachable;
    let snapshot = environment.status().await?;
    let web = &snapshot.stacks[0].services[0];
    assert_eq!(web.state, ServiceState::Degraded, "not running: 2 of 3");
    assert_eq!(
        (web.desired_count, web.running_count, web.healthy_count),
        (3, 2, 2)
    );
    assert_eq!(
        web.nodes
            .iter()
            .map(|share| (share.node.as_str(), share.state))
            .collect::<Vec<_>>(),
        [
            ("app-1", ServiceState::Running),
            ("app-2", ServiceState::Running),
            ("db-1", ServiceState::Unknown),
        ]
    );
    // `worker` is placed on app-1 alone, which answered: it is whole.
    let worker = &snapshot.stacks[0].services[1];
    assert_eq!(worker.state, ServiceState::Running);
    assert_eq!(worker.nodes.len(), 1);

    let table = mm::presentation::output::service_table(&snapshot);
    let first = table[1].split_whitespace().collect::<Vec<_>>();
    assert_eq!(first[..4], ["app", "web", "degraded", "2/3"], "{table:#?}");
    assert_eq!(
        row_from(&table, "db-1"),
        ["db-1", "-", "-", "no", "answer", "-"],
        "{table:#?}"
    );

    // The first node is the silent one: its share comes first all the same,
    // and the service only it runs is listed, with nothing known of it.
    *locked(&fakes[0].status) = Status::Unreachable;
    fakes[2].answers(running(&["web"]));
    let snapshot = environment.status().await?;
    let services = &snapshot.stacks[0].services;
    assert_eq!(
        services[0]
            .nodes
            .iter()
            .map(|share| (share.node.as_str(), share.state))
            .collect::<Vec<_>>(),
        [
            ("app-1", ServiceState::Unknown),
            ("app-2", ServiceState::Running),
            ("db-1", ServiceState::Running),
        ]
    );
    let worker = services
        .iter()
        .find(|service| service.name == "worker")
        .ok_or("worker is placed on the silent node and must be listed")?;
    assert_eq!(worker.state, ServiceState::Unknown);
    assert_eq!((worker.desired_count, worker.running_count), (1, 0));
    assert!(worker.containers.is_empty());
    let table = mm::presentation::output::service_table(&snapshot);
    let row = table
        .iter()
        .find(|line| line.contains("worker"))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(
        row,
        [
            "app", "worker", "unknown", "0/1", "app-1", "-", "-", "no", "answer", "-"
        ],
        "{table:#?}"
    );

    // A node that answers and has no container of a service it should
    // have still says `no container`.
    fakes[0].answers(Share {
        services: vec![service("web", ServiceState::Missing)],
        ..Share::default()
    });
    let snapshot = environment.status().await?;
    let table = mm::presentation::output::service_table(&snapshot);
    assert_eq!(
        row_from(&table, "app-1"),
        ["app-1", "-", "-", "no", "container", "-"],
        "{table:#?}"
    );
    Ok(())
}

/// During a rollout step a new container runs and its health check has
/// not passed yet. That is not a failure: the service is starting, and
/// the count is the containers that are ready.
#[tokio::test]
async fn a_container_whose_health_check_has_not_passed_yet_is_starting_not_a_failure() -> TestResult
{
    let (environment, fakes) = cluster()?;
    let healthy = |health: &str| {
        let mut web = service("web", ServiceState::Running);
        web.containers[0].health = Some(health.to_owned());
        if health != "healthy" {
            web.state = ServiceState::Degraded;
            web.healthy_count = 0;
        }
        Share {
            services: vec![web],
            ..Share::default()
        }
    };
    fakes[0].answers(healthy("starting"));
    fakes[1].answers(healthy("healthy"));
    fakes[2].answers(healthy("healthy"));
    let snapshot = environment.status().await?;
    let web = &snapshot.stacks[0].services[0];
    // The state an interface reads stays one of the five: `degraded`.
    assert_eq!(web.state, ServiceState::Degraded);
    assert_eq!(
        (web.desired_count, web.running_count, web.healthy_count),
        (3, 3, 2)
    );
    // One stored fact says it is only still coming up: the table, the
    // terminal UI and JSON all read it, for the service and for each node.
    assert!(web.starting);
    assert_eq!(
        web.nodes
            .iter()
            .map(|share| (share.node.as_str(), share.state, share.starting))
            .collect::<Vec<_>>(),
        [
            ("app-1", ServiceState::Degraded, true),
            ("app-2", ServiceState::Running, false),
            ("db-1", ServiceState::Running, false),
        ]
    );
    let json = serde_json::to_value(web)?;
    assert_eq!(json["state"], "degraded");
    assert_eq!(json["starting"], true);
    assert_eq!(json["nodes"][0]["starting"], true);
    assert_eq!(json["nodes"][1]["starting"], false);
    let table = mm::presentation::output::service_table(&snapshot);
    let first = table[1].split_whitespace().collect::<Vec<_>>();
    assert_eq!(first[..4], ["app", "web", "starting", "2/3"], "{table:#?}");
    assert_eq!(first[4..7], ["app-1", "web-1", "starting"], "{table:#?}");

    // A check that failed is a failure, and so is a container that is gone.
    fakes[0].answers(healthy("unhealthy"));
    let snapshot = environment.status().await?;
    let web = &snapshot.stacks[0].services[0];
    assert!(!web.starting);
    assert!(web.nodes.iter().all(|share| !share.starting));
    assert_eq!(serde_json::to_value(web)?["starting"], false);
    let table = mm::presentation::output::service_table(&snapshot);
    let first = table[1].split_whitespace().collect::<Vec<_>>();
    assert_eq!(first[..4], ["app", "web", "degraded", "2/3"], "{table:#?}");

    fakes[0].answers(healthy("starting"));
    fakes[1].answers(Share {
        services: vec![service("web", ServiceState::Missing)],
        ..Share::default()
    });
    let snapshot = environment.status().await?;
    let web = &snapshot.stacks[0].services[0];
    assert!(!web.starting);
    // Node 1's own share is still only coming up; the service is not.
    assert!(web.nodes[0].starting);
    Ok(())
}

fn approval() -> ApprovalContext {
    ApprovalContext {
        principal: "alice".to_owned(),
        source: "test".to_owned(),
        approved_high_risk: false,
    }
}

fn logs_of(service: &str) -> LogRequest {
    LogRequest::new(
        ServiceSelector {
            stack: None,
            module: None,
            service: service.to_owned(),
            container: None,
        },
        approval(),
    )
}

/// A line `service` wrote at `second` past the minute.
fn written(service: &str, second: u32, message: &str) -> LogRecord {
    LogRecord {
        identity: ServiceIdentity {
            project_id: project(),
            environment: "production".to_owned(),
            stack: "app".to_owned(),
            module: Some("shop".to_owned()),
            service: service.to_owned(),
        },
        sequence: 0,
        source: LogSource::Stdout,
        node: None,
        container: Some(format!("shop-production-app-{service}-1")),
        timestamp: Some(format!("2026-05-01T10:00:{second:02}.000000000Z")),
        message: message.to_owned(),
        lossy_utf8: false,
        truncated: false,
        reconnect: 0,
    }
}

fn line(second: u32, message: &str) -> Line {
    Line::Written(Box::new(written("web", second, message)))
}

/// Web's lines on the three nodes, interleaved in time.
fn three_nodes_write(fakes: &[Arc<Node>]) {
    fakes[0].writes(vec![line(1, "one"), line(4, "four"), line(7, "seven")]);
    fakes[1].writes(vec![line(2, "two"), line(5, "five")]);
    fakes[2].writes(vec![line(3, "three"), line(6, "six")]);
}

/// Everything a stream delivers until it ends.
async fn read(mut stream: LogStream) -> Result<Vec<LogRecord>, MmError> {
    let mut records = Vec::new();
    while let Some(item) = stream.recv().await {
        records.push(item?);
    }
    Ok(records)
}

fn told(records: &[LogRecord]) -> Vec<(&str, &str)> {
    records
        .iter()
        .map(|record| {
            (
                record.node.as_deref().unwrap_or_default(),
                record.message.as_str(),
            )
        })
        .collect()
}

fn numbered_from_one(records: &[LogRecord]) -> bool {
    records
        .iter()
        .zip(1_u64..)
        .all(|(record, sequence)| record.sequence == sequence)
}

#[tokio::test]
async fn the_log_of_a_service_on_three_nodes_is_one_stream_in_the_order_it_was_written()
-> TestResult {
    let (environment, fakes) = cluster()?;
    three_nodes_write(&fakes);
    let records = read(
        environment
            .logs(logs_of("web"), OperationCancellation::default())
            .await?,
    )
    .await?;
    assert_eq!(
        told(&records),
        [
            ("app-1", "one"),
            ("app-2", "two"),
            ("db-1", "three"),
            ("app-1", "four"),
            ("app-2", "five"),
            ("db-1", "six"),
            ("app-1", "seven"),
        ]
    );
    assert!(numbered_from_one(&records), "{records:?}");
    assert!(
        records
            .iter()
            .all(|record| record.source == LogSource::Stdout)
    );
    for fake in &fakes {
        assert_eq!(fake.calls(), ["logs"]);
        let asked = locked(&fake.asked);
        assert_eq!(asked[0].service(), Some("web"));
        assert!(
            asked[0].nodes.is_empty(),
            "which nodes are read is the client's business"
        );
    }

    // The tail is the newest lines of all the nodes together.
    let mut request = logs_of("web");
    request.tail = 3;
    let records = read(
        environment
            .logs(request, OperationCancellation::default())
            .await?,
    )
    .await?;
    assert_eq!(
        told(&records),
        [("app-2", "five"), ("db-1", "six"), ("app-1", "seven")]
    );
    assert!(numbered_from_one(&records), "{records:?}");
    Ok(())
}

#[tokio::test]
async fn a_log_read_asks_the_nodes_that_can_have_lines() -> TestResult {
    let (environment, fakes) = cluster()?;
    let asked = |fakes: &[Arc<Node>]| {
        fakes
            .iter()
            .filter(|fake| !std::mem::take(&mut *locked(&fake.calls)).is_empty())
            .map(|fake| fake.name.clone())
            .collect::<Vec<_>>()
    };
    let cancellation = OperationCancellation::default;

    // Every service: every node, with a request that names no service.
    three_nodes_write(&fakes);
    let every = LogRequest::every_service(approval());
    let records = read(environment.logs(every.clone(), cancellation()).await?).await?;
    assert_eq!(records.len(), 7);
    assert_eq!(asked(&fakes), NAMES);
    assert!(fakes.iter().all(|fake| {
        locked(&fake.asked)
            .iter()
            .all(|sent| sent.selector.is_none())
    }));

    // A service on one node: only that node.
    read(environment.logs(logs_of("worker"), cancellation()).await?).await?;
    assert_eq!(asked(&fakes), ["app-1"]);

    // `--node`: only the nodes it names.
    let mut only = logs_of("web");
    only.nodes = vec!["app-2".to_owned()];
    let records = read(environment.logs(only, cancellation()).await?).await?;
    assert_eq!(told(&records), [("app-2", "two"), ("app-2", "five")]);
    assert_eq!(asked(&fakes), ["app-2"]);
    let mut only = every;
    only.nodes = vec!["db-1".to_owned(), "app-1".to_owned()];
    read(environment.logs(only, cancellation()).await?).await?;
    assert_eq!(asked(&fakes), ["app-1", "db-1"]);

    // A name that is no node of the target.
    let mut unknown = logs_of("web");
    unknown.nodes = vec!["app-9".to_owned()];
    let refused = environment
        .logs(unknown, cancellation())
        .await
        .err()
        .ok_or("a node the target does not list was read")?
        .to_string();
    assert!(
        refused.contains("--node names 'app-9', which is not a node of target 'production'"),
        "{refused}"
    );
    assert!(refused.contains("app-1, app-2, db-1"), "{refused}");

    // A service no node runs, and one the named node does not run.
    let refused = environment
        .logs(logs_of("cache"), cancellation())
        .await
        .err()
        .ok_or("a service nobody runs was read")?
        .to_string();
    assert!(
        refused.contains("no node of target 'production' runs a service named 'cache'"),
        "{refused}"
    );
    let mut elsewhere = logs_of("worker");
    elsewhere.nodes = vec!["db-1".to_owned()];
    let refused = environment
        .logs(elsewhere, cancellation())
        .await
        .err()
        .ok_or("a node that does not run the service was read")?
        .to_string();
    assert!(
        refused.contains("that --node names runs a service named 'worker'"),
        "{refused}"
    );
    assert!(asked(&fakes).is_empty(), "a refused read asks no node");
    Ok(())
}

/// The lines mm added about nodes, as `(node, message)`.
fn notices(records: &[LogRecord]) -> Vec<(&str, &str)> {
    records
        .iter()
        .filter(|record| record.source == LogSource::Runtime)
        .map(|record| {
            (
                record.node.as_deref().unwrap_or_default(),
                record.message.as_str(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_node_that_does_not_answer_is_one_line_among_the_others_lines() -> TestResult {
    let (environment, fakes) = cluster()?;
    three_nodes_write(&fakes);
    *locked(&fakes[1].logs) = Logs::Unreachable;
    let records = read(
        environment
            .logs(logs_of("web"), OperationCancellation::default())
            .await?,
    )
    .await?;
    assert_eq!(
        notices(&records),
        [(
            "app-2",
            "no answer: did not answer (Connection timed out). Check that the host is up"
        )]
    );
    let lines = records
        .iter()
        .filter(|record| record.source != LogSource::Runtime)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        told(&lines),
        [
            ("app-1", "one"),
            ("db-1", "three"),
            ("app-1", "four"),
            ("db-1", "six"),
            ("app-1", "seven"),
        ]
    );
    assert_eq!(
        records.last().map(|record| record.source),
        Some(LogSource::Runtime),
        "a read that ends says it after the lines"
    );
    assert!(numbered_from_one(&records), "{records:?}");
    Ok(())
}

#[tokio::test]
async fn a_log_read_fails_only_when_no_node_answers() -> TestResult {
    let (environment, fakes) = cluster()?;
    for fake in &fakes {
        *locked(&fake.logs) = Logs::Unreachable;
    }
    let mut stream = environment
        .logs(logs_of("web"), OperationCancellation::default())
        .await?;
    let first = stream.recv().await.ok_or("the stream ended silently")?;
    let error = first.err().ok_or("a line came from nodes that are down")?;
    assert_eq!(error.code(), "MM_UNREACHABLE");
    assert!(
        error.to_string().contains("node 'app-1'"),
        "the first node in declared order says why: {error}"
    );
    assert!(stream.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn a_nodes_stream_that_breaks_keeps_its_lines_and_says_so_once() -> TestResult {
    let (environment, fakes) = cluster()?;
    three_nodes_write(&fakes);
    fakes[2].writes(vec![
        line(3, "three"),
        Line::Broken("the engine closed the connection"),
    ]);
    let records = read(
        environment
            .logs(logs_of("web"), OperationCancellation::default())
            .await?,
    )
    .await?;
    assert_eq!(
        told(&records)
            .iter()
            .map(|(_, message)| *message)
            .collect::<Vec<_>>()[..6],
        ["one", "two", "three", "four", "five", "seven"]
    );
    let said = notices(&records);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].0, "db-1");
    assert!(
        said[0].1.starts_with("the stream ended: ")
            && said[0].1.contains("the engine closed the connection"),
        "{}",
        said[0].1
    );
    assert_eq!(records.len(), 7);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_followed_log_that_loses_every_stream_fails_with_the_reason() -> TestResult {
    let (environment, fakes) = cluster()?;
    for (fake, second) in fakes.iter().zip([1, 2, 3]) {
        fake.writes(vec![
            line(second, "before"),
            Line::Broken("restricted relay disconnected"),
        ]);
    }
    let mut request = logs_of("web");
    request.follow = true;
    let mut stream = environment
        .logs(request, OperationCancellation::default())
        .await?;
    let mut records = Vec::new();
    let failure = loop {
        match stream.recv().await {
            Some(Ok(record)) => records.push(record),
            Some(Err(error)) => break error,
            None => {
                return Err("a followed log with no stream left ended as if it were done".into());
            }
        }
    };
    let message = failure.to_string();
    assert!(
        message.contains("has no node's stream left")
            && message.contains("restricted relay disconnected"),
        "{message}"
    );
    assert_eq!(notices(&records).len(), 3, "each node's end is said first");
    assert_eq!(
        told(&records)
            .iter()
            .filter(|(_, message)| *message == "before")
            .count(),
        3,
        "the lines that came are kept"
    );
    Ok(())
}

/// The stream's next record, which has to come within a second.
async fn next(stream: &mut LogStream) -> Result<LogRecord, Box<dyn std::error::Error>> {
    Ok(tokio::time::timeout(Duration::from_secs(1), stream.recv())
        .await
        .map_err(|_| "the stream delivered nothing for a second")?
        .ok_or("the stream ended")??)
}

#[tokio::test(start_paused = true)]
async fn a_followed_log_says_when_a_node_fails_and_stays_open() -> TestResult {
    let (environment, fakes) = cluster()?;
    let first = fakes[0].fed();
    *locked(&fakes[1].logs) = Logs::Unreachable;
    let third = fakes[2].fed();
    let mut request = logs_of("web");
    request.follow = true;
    let mut stream = environment
        .logs(request, OperationCancellation::default())
        .await?;

    // The node that is down is named at once, before any line.
    let record = next(&mut stream).await?;
    assert_eq!(
        (record.source, record.node.as_deref()),
        (LogSource::Runtime, Some("app-2"))
    );
    assert!(record.message.starts_with("no answer: "), "{record:?}");

    first.send(Ok(written("web", 1, "one"))).await?;
    third.send(Ok(written("web", 2, "two"))).await?;
    assert_eq!(next(&mut stream).await?.message, "one");
    assert_eq!(next(&mut stream).await?.message, "two");

    // db-1's stream breaks: one line says so when it happens.
    third
        .send(Err(MmError::RuntimeResponse {
            message: "the engine closed the connection".to_owned(),
        }))
        .await?;
    let record = next(&mut stream).await?;
    assert_eq!(
        (record.source, record.node.as_deref()),
        (LogSource::Runtime, Some("db-1"))
    );
    assert!(
        record.message.starts_with("the stream ended: "),
        "{record:?}"
    );

    // The read goes on with the node that is left.
    first.send(Ok(written("web", 3, "three"))).await?;
    let record = next(&mut stream).await?;
    assert_eq!(
        (record.node.as_deref(), record.message.as_str()),
        (Some("app-1"), "three")
    );
    assert_eq!(record.sequence, 5, "every record of the stream is numbered");
    assert!(
        tokio::time::timeout(Duration::from_secs(30), stream.recv())
            .await
            .is_err(),
        "a followed read stays open while a node's stream is"
    );

    // It ends when the last node's stream does.
    drop(first);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), stream.recv())
            .await?
            .is_none()
    );
    Ok(())
}

/// An agent that reads a status finds the same keys on every node, with
/// or without something to report.
#[test]
fn every_node_of_a_status_has_the_same_keys_in_json() -> TestResult {
    let quiet = mm::domain::workspace::NodeStatus {
        reachable: true,
        ..mm::domain::workspace::NodeStatus::pending("app-1".to_owned())
    };
    assert_eq!(
        serde_json::to_value(&quiet)?,
        serde_json::json!({
            "name": "app-1",
            "reachable": true,
            "problem": null,
            "network_drift": [],
            "deployment": null,
            "drift": [],
            "tunnel": null,
        })
    );
    // A status written by an mm that left the empty ones out still reads.
    let older: mm::domain::workspace::NodeStatus =
        serde_json::from_value(serde_json::json!({"name": "app-1", "reachable": true}))?;
    assert_eq!(older, quiet);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_status_read_again_shows_each_node_as_it_answers_and_keeps_the_others() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[0].answers(running(&["web", "worker"]));
    fakes[1].answers(running(&["web"]));
    fakes[2].answers(running(&["web"]));
    let first = environment.status().await?;
    assert!(first.nodes.iter().all(|node| node.reachable));

    // app-2 goes quiet. While the second read waits for it, what it said
    // to the first read stands: the others are fresh, nothing flickers to
    // "not answered yet".
    *locked(&fakes[1].status) = Status::Silent;
    let so_far = Mutex::new(Vec::new());
    let last = environment
        .status_reporting(Some(&|snapshot| locked(&so_far).push(snapshot)))
        .await?;
    let reports = std::mem::take(&mut *locked(&so_far));
    assert!(!reports.is_empty());
    for report in &reports {
        assert!(
            report.nodes.iter().all(|node| !node.is_pending()),
            "a node that answered before is not waited for again: {:?}",
            report.nodes
        );
        assert_eq!(report.stacks[0].services[0].running_count, 3, "{report:?}");
    }
    // Once it has had its five seconds, it is silent.
    assert_eq!(
        last.nodes
            .iter()
            .map(|node| (node.name.as_str(), node.reachable))
            .collect::<Vec<_>>(),
        [("app-1", true), ("app-2", false), ("db-1", true)]
    );
    // It counts again as soon as it answers.
    *locked(&fakes[1].status) = Status::Answers(Box::new(running(&["web"])));
    let again = environment.status().await?;
    assert!(again.nodes.iter().all(|node| node.reachable));
    Ok(())
}

#[tokio::test]
async fn a_service_the_first_read_still_waits_for_is_not_whole_yet() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[0].answers(running(&["web", "worker"]));
    fakes[1].answers(running(&["web"]));
    *locked(&fakes[2].status) = Status::Unreachable;
    let so_far = Mutex::new(Vec::new());
    environment
        .status_reporting(Some(&|snapshot| locked(&so_far).push(snapshot)))
        .await?;
    // In every report, web counts a share on each node it is placed on,
    // answered or not: never "2/2" while a node is still asked.
    for report in std::mem::take(&mut *locked(&so_far)) {
        let web = &report.stacks[0].services[0];
        assert_eq!(web.name, "web");
        assert_eq!(web.desired_count, 3, "{report:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_command_runs_on_the_node_it_names_or_where_the_checkout_places_the_service() -> TestResult
{
    let (environment, fakes) = cluster()?;
    let request = |container: Option<&str>| {
        ExecRequest::new(
            ServiceSelector {
                stack: Some("app".to_owned()),
                module: None,
                service: "web".to_owned(),
                container: container.map(str::to_owned),
            },
            vec!["true".to_owned()],
            ApprovalContext {
                principal: "test".to_owned(),
                source: "test".to_owned(),
                approved_high_risk: false,
            },
        )
    };
    let refused = environment
        .exec_on(
            request(Some("web-on-db-1")),
            Some("db-1"),
            OperationCancellation::default(),
        )
        .await;
    assert!(refused.is_err(), "the scripted node does not run commands");
    assert_eq!(fakes[2].calls(), ["exec web-on-db-1"]);
    assert!(fakes[0].calls().is_empty() && fakes[1].calls().is_empty());

    // Without a node, the first node the checkout places the service on.
    environment
        .exec(request(None), OperationCancellation::default())
        .await
        .err()
        .ok_or("the scripted node does not run commands")?;
    assert_eq!(fakes[0].calls(), ["exec -"]);

    let unknown = environment
        .exec_on(
            request(None),
            Some("app-9"),
            OperationCancellation::default(),
        )
        .await
        .err()
        .ok_or("expected a refusal")?
        .to_string();
    assert!(
        unknown.contains(
            "'app-9' is not a node of target 'production'; its nodes are app-1, app-2, db-1"
        ),
        "{unknown}"
    );
    Ok(())
}

#[tokio::test]
async fn a_command_in_a_named_container_runs_on_the_node_that_has_it() -> TestResult {
    let (environment, fakes) = cluster()?;
    // web runs on every node, each with a container of its own.
    for (index, fake) in fakes.iter().enumerate() {
        let mut share = running(&["web"]);
        share.services[0].containers[0].id = format!("{index}0f3c9a7b2e1d");
        fake.answers(share);
    }
    let request = |container: &str| {
        ExecRequest::new(
            ServiceSelector {
                stack: None,
                module: None,
                service: "web".to_owned(),
                container: Some(container.to_owned()),
            },
            vec!["true".to_owned()],
            approval(),
        )
    };
    environment
        .exec(request("20f3c9a7"), OperationCancellation::default())
        .await
        .err()
        .ok_or("the scripted node does not run commands")?;
    assert_eq!(fakes[2].calls(), ["status", "exec 20f3c9a7"]);
    for fake in &fakes[..2] {
        assert_eq!(fake.calls(), ["status"], "{}", fake.name);
    }

    // A container no node runs is named, with where to look.
    let unknown = environment
        .exec(request("ffff"), OperationCancellation::default())
        .await
        .err()
        .ok_or("expected a refusal")?
        .to_string();
    assert!(
        unknown.contains("no node runs a container ffff of service 'web'")
            && unknown.contains("`mm service status web`"),
        "{unknown}"
    );
    assert!(
        fakes
            .iter()
            .all(|fake| !fake.calls().contains(&"exec ffff".to_owned())),
        "no node was asked to run it"
    );
    Ok(())
}

#[tokio::test]
async fn a_plan_every_node_refuses_names_every_node() -> TestResult {
    let (environment, _fakes) = cluster()?;
    let refused = environment
        .plan_deployment(&"a".repeat(40), None, None)
        .await
        .err()
        .ok_or("the nodes refuse the plan")?;
    assert_eq!(refused.code(), "MM_TARGET");
    let message = refused.to_string();
    assert!(
        message.contains("node 'app-1' runs docker-compose")
            && message.ends_with("; nodes 'app-2' and 'db-1' refused it too"),
        "{message}"
    );
    Ok(())
}
