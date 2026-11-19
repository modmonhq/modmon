//! A remote environment as the client drives it: planning and applying
//! across nodes, access syncs, status, logs, and node reports, against
//! scripted nodes.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use mm::app::ports::{ExecStream, LogStream, RemoteTransport};
use mm::app::{NodeUnavailable, RemoteEnvironment, RemoteNode};
use mm::domain::build::BuildReceipt;
use mm::domain::deployment::{DeploymentResult, LeavingService};
use mm::domain::error::MmError;
use mm::domain::host_policy::EnvironmentAccessRecord;
use mm::domain::observability::{
    ApprovalContext, ExecRequest, LogRequest, OperationCancellation, ServiceSelector,
};
use mm::domain::placement::{ClusterNodes, PlacedService, services_on_node};
use mm::domain::remote::{
    ClusterPlanSeal, NodeApplyState, NodeDeployment, NodeFacts, RemoteAccessPlan,
    RemoteDeploymentPlan, RemoteIdentity, RemotePlanBinding, RemotePlanKind,
};
use mm::domain::remote::{NodeState, RemoteScope};
use mm::domain::runtime::RuntimeKind;
use mm::domain::users::AccessChange;
use mm::domain::workspace::{ProjectId, WorkspaceSnapshot};

const COMMIT: &str = "cccccccccccccccccccccccccccccccccccccccc";
const NAMES: [&str; 3] = ["app-1", "app-2", "db-1"];

/// One node's `mmd`, scripted: it records every call, plans with a hash
/// made of its digit, and keeps the seal it is sent.
struct FakeNode {
    name: String,
    digit: &'static str,
    /// The name the host says it has, when it differs from `name`.
    answers_as: Option<String>,
    up: AtomicBool,
    fail_apply: AtomicBool,
    /// The services this node's plan moves to other nodes.
    moved_away: Mutex<Vec<LeavingService>>,
    fail_finish: AtomicBool,
    calls: Mutex<Vec<String>>,
    seal: Mutex<Option<ClusterPlanSeal>>,
    /// A peer this node's mesh never heard from.
    silent_peer: Mutex<Option<String>>,
    /// The cluster networks this node stored, and what differs on it.
    networks: Mutex<mm::domain::cluster_network::NodeNetworkStatus>,
    /// What this node's resolver knows.
    names: Mutex<Option<mm::domain::discovery::NameReport>>,
}

impl FakeNode {
    fn new(name: &str, digit: &'static str) -> Arc<Self> {
        Self::answering_as(name, digit, None)
    }

    /// A host reached at `name`'s address that says it is `answers_as`.
    fn answering_as(name: &str, digit: &'static str, answers_as: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_owned(),
            digit,
            answers_as: answers_as.map(str::to_owned),
            up: AtomicBool::new(true),
            fail_apply: AtomicBool::new(false),
            moved_away: Mutex::new(Vec::new()),
            fail_finish: AtomicBool::new(false),
            calls: Mutex::new(Vec::new()),
            seal: Mutex::new(None),
            silent_peer: Mutex::new(None),
            networks: Mutex::new(mm::domain::cluster_network::NodeNetworkStatus::default()),
            names: Mutex::new(None),
        })
    }

    fn call(&self, method: &str, scope: &RemoteScope) -> Result<(), MmError> {
        locked(&self.calls).push(method.to_owned());
        assert_eq!(
            scope.node.as_deref(),
            Some(self.name.as_str()),
            "a request names the node it is sent to"
        );
        if self.up.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(MmError::Unreachable {
                subject: format!("node '{}' (203.0.113.{}:22)", self.name, self.digit),
                problem: "did not answer (Connection timed out)".to_owned(),
                help: "Check that the host is up".to_owned(),
            })
        }
    }

    fn calls(&self) -> Vec<String> {
        locked(&self.calls).clone()
    }

    fn binding(&self, scope: &RemoteScope, kind: RemotePlanKind) -> RemotePlanBinding {
        RemotePlanBinding {
            protocol_version: mm::domain::remote::PROTOCOL_VERSION,
            daemon_version: "0.1.0".to_owned(),
            daemon_fingerprint: self.digit.repeat(64),
            target_fingerprint: scope.target_fingerprint.clone(),
            target: scope.target.clone(),
            node: scope.node.clone(),
            kind,
            project_id: scope.project_id,
            environment: scope.environment.clone(),
            runtime: scope.runtime,
            commit: COMMIT.to_owned(),
            deployment_plan_hash: "e".repeat(64),
            access_change_hash: "e".repeat(64),
            principal_id: mm::domain::access::PrincipalId(uuid::Uuid::from_u128(1)),
            authorization_revision: 1,
            authorization_hash: "f".repeat(64),
            plan_nonce: uuid::Uuid::from_u128(2),
            expires_at_unix_ms: 1_000 + self.digit.parse::<i64>().unwrap_or_default(),
            bundle_hash: None,
            plan_hash: self.digit.repeat(64),
        }
    }
}

fn access(scope: &RemoteScope) -> AccessChange {
    AccessChange::between(
        None,
        EnvironmentAccessRecord {
            schema_version: 1,
            project_id: scope.project_id,
            environment: scope.environment.clone(),
            commit: COMMIT.to_owned(),
            users: BTreeMap::new(),
            deploy_from: None,
            reconciled_at_unix_ms: 0,
            reconciled_by: "test".to_owned(),
        },
    )
}

fn unused<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not used by this test".to_owned(),
    })
}

#[async_trait]
impl RemoteTransport for FakeNode {
    async fn status(&self, scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        self.call("status", &scope)?;
        let mut snapshot = snapshot(&scope);
        if self.digit == "1" {
            snapshot.stacks = vec![stack("app", &["web", "worker"])];
        }
        snapshot.names.clone_from(&locked(&self.names));
        // A node says what differs in its cluster networks with its status.
        snapshot.network_drift = locked(&self.networks)
            .drift
            .iter()
            .map(|drift| drift.detail.clone())
            .collect();
        Ok(snapshot)
    }

    async fn logs(
        &self,
        scope: RemoteScope,
        _request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        self.call("logs", &scope)?;
        let (_, receiver) = tokio::sync::mpsc::channel(1);
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        scope: RemoteScope,
        _request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        self.call("exec", &scope)?;
        let (_, receiver) = tokio::sync::mpsc::channel(1);
        Ok(ExecStream::new(receiver, cancellation))
    }

    async fn plan_deployment(
        &self,
        scope: RemoteScope,
        _commit: String,
        receipt: Option<BuildReceipt>,
        _release: mm::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError> {
        self.call("plan_deployment", &scope)?;
        Ok(RemoteDeploymentPlan {
            binding: self.binding(&scope, RemotePlanKind::Deployment),
            plan: None,
            access: access(&scope),
            receipt,
            unfinished: None,
            once: Default::default(),
            staged: Vec::new(),
            release: None,
        })
    }

    async fn apply_deployment(
        &self,
        scope: RemoteScope,
        remote_plan_hash: String,
    ) -> Result<Option<DeploymentResult>, MmError> {
        self.call("apply_deployment", &scope)?;
        let sealed = locked(&self.seal).clone();
        assert_eq!(
            sealed.map(|seal| seal.cluster_plan_hash),
            Some(remote_plan_hash),
            "a node is asked to apply the cluster plan it sealed"
        );
        if self.fail_apply.load(Ordering::SeqCst) {
            return Err(MmError::Security {
                message: "remote deployment failed: the healthcheck did not pass".to_owned(),
            });
        }
        let moved_away = locked(&self.moved_away).clone();
        if moved_away.is_empty() {
            return Ok(None);
        }
        Ok(Some(DeploymentResult {
            plan_hash: self.digit.repeat(64),
            target_name: "local".to_owned(),
            target_id: mm::domain::target::TargetId(uuid::Uuid::from_u128(7)),
            environment: scope.environment,
            applied_stacks: Vec::new(),
            removed_stacks: Vec::new(),
            route_generation: 0,
            route_count: 0,
            moved_away,
            jobs: Vec::new(),
        }))
    }

    async fn rollout(
        &self,
        scope: RemoteScope,
        plan_hash: String,
        step: mm::domain::rollout::RolloutStep,
        _patience: std::time::Duration,
    ) -> Result<mm::domain::rollout::RolloutAnswer, MmError> {
        use mm::domain::rollout::{RolloutAnswer, RolloutStep};
        self.call(&format!("rollout:{}", step.name()), &scope)?;
        assert_eq!(
            locked(&self.seal)
                .clone()
                .map(|seal| seal.cluster_plan_hash),
            Some(plan_hash),
            "a node is asked to roll out the cluster plan it sealed"
        );
        Ok(match step {
            RolloutStep::Hold => RolloutAnswer::Held,
            RolloutStep::Networks => RolloutAnswer::Addressed,
            RolloutStep::Storage => RolloutAnswer::StorageReady {
                created: Vec::new(),
            },
            RolloutStep::Release => RolloutAnswer::Released,
            RolloutStep::Rollback => RolloutAnswer::RolledBack,
            RolloutStep::Begin => {
                if self.fail_apply.load(Ordering::SeqCst) {
                    return Err(MmError::Security {
                        message: "remote deployment failed: the healthcheck did not pass"
                            .to_owned(),
                    });
                }
                RolloutAnswer::Begun
            }
            RolloutStep::Start { .. } | RolloutStep::Apply { .. } => RolloutAnswer::Applied {
                addresses: Vec::new(),
            },
            RolloutStep::Ahead { .. } => RolloutAnswer::Applied {
                addresses: Vec::new(),
            },
            RolloutStep::Job { job, .. } => {
                return Err(MmError::State {
                    message: format!("this node runs no job '{job}'"),
                });
            }
            RolloutStep::Finish => {
                let moved_away = locked(&self.moved_away).clone();
                RolloutAnswer::Finished((!moved_away.is_empty()).then(|| {
                    Box::new(DeploymentResult {
                        plan_hash: self.digit.repeat(64),
                        target_name: "local".to_owned(),
                        target_id: mm::domain::target::TargetId(uuid::Uuid::from_u128(7)),
                        environment: scope.environment,
                        applied_stacks: Vec::new(),
                        removed_stacks: Vec::new(),
                        route_generation: 0,
                        route_count: 0,
                        moved_away,
                        jobs: Vec::new(),
                    })
                }))
            }
        })
    }

    async fn deployment_status(
        &self,
        scope: RemoteScope,
    ) -> Result<mm::domain::rollout::NodeDeploymentStatus, MmError> {
        self.call("deployment_status", &scope)?;
        Ok(mm::domain::rollout::NodeDeploymentStatus::default())
    }

    async fn finish_moves(
        &self,
        scope: RemoteScope,
        cluster_plan_hash: String,
    ) -> Result<Vec<PlacedService>, MmError> {
        self.call("finish_moves", &scope)?;
        assert_eq!(
            locked(&self.seal)
                .clone()
                .map(|seal| seal.cluster_plan_hash),
            Some(cluster_plan_hash),
            "a node finishes the cluster plan it applied"
        );
        if self.fail_finish.load(Ordering::SeqCst) {
            return Err(MmError::RuntimeResponse {
                message: "the engine did not stop the container".to_owned(),
            });
        }
        Ok(locked(&self.moved_away)
            .iter()
            .map(LeavingService::placed)
            .collect())
    }

    async fn networks(
        &self,
        scope: RemoteScope,
    ) -> Result<mm::domain::cluster_network::NodeNetworkStatus, MmError> {
        self.call("networks", &scope)?;
        Ok(locked(&self.networks).clone())
    }

    async fn seal_plan(
        &self,
        scope: RemoteScope,
        seal: ClusterPlanSeal,
    ) -> Result<ClusterPlanSeal, MmError> {
        self.call("seal_plan", &scope)?;
        assert_eq!(
            seal.section(&self.name)
                .map(|section| section.plan_hash.clone()),
            Some(self.digit.repeat(64)),
            "the seal covers the plan this node made"
        );
        *locked(&self.seal) = Some(seal.clone());
        Ok(seal)
    }

    async fn sealed_plan(
        &self,
        scope: RemoteScope,
        cluster_plan_hash: String,
    ) -> Result<ClusterPlanSeal, MmError> {
        self.call("sealed_plan", &scope)?;
        locked(&self.seal)
            .clone()
            .filter(|seal| seal.cluster_plan_hash == cluster_plan_hash)
            .ok_or_else(|| MmError::Security {
                message: format!("holds no reviewed cluster plan {cluster_plan_hash}"),
            })
    }

    async fn mesh_status(
        &self,
        scope: RemoteScope,
    ) -> Result<mm::domain::mesh::MeshNodeStatus, MmError> {
        self.call("mesh_status", &scope)?;
        let silent = locked(&self.silent_peer).clone();
        let key = |name: &str| format!("{name}-key");
        Ok(mm::domain::mesh::MeshNodeStatus {
            node: Some(self.name.clone()),
            interface: Some("mm0".to_owned()),
            up: true,
            address: None,
            port: Some(51820),
            mtu: Some(1420),
            public_key: Some(key(&self.name)),
            peers: NAMES
                .iter()
                .zip(1..)
                .filter(|(name, _)| **name != self.name)
                .map(|(name, last)| mm::domain::mesh::MeshPeerStatus {
                    name: Some((*name).to_owned()),
                    public_key: key(name),
                    endpoint: Some(format!("203.0.113.{last}:51820")),
                    configured_endpoint: Some(format!("203.0.113.{last}:51820")),
                    allowed_ips: vec![format!("10.90.0.{last}/32")],
                    handshake_age_seconds: (silent.as_deref() != Some(*name)).then_some(20),
                    received_bytes: 10,
                    sent_bytes: 20,
                    on_interface: true,
                })
                .collect(),
            guard_drift: Vec::new(),
        })
    }

    async fn node(&self, scope: RemoteScope) -> Result<NodeFacts, MmError> {
        self.call("node", &scope)?;
        Ok(NodeFacts {
            node: Some(self.answers_as.clone().unwrap_or_else(|| self.name.clone())),
            daemon_version: format!("0.{}.0", self.digit),
            private_address: None,
            deployed: Some(NodeDeployment {
                commit: COMMIT.to_owned(),
                deployed_at_unix_ms: 5,
                deployed_by: "alice".to_owned(),
                runs_services: self.digit == "1",
            }),
            rollout: None,
            engine: None,
            daemon: None,
            secret_store: None,
        })
    }

    async fn plan_team_sync(
        &self,
        scope: RemoteScope,
        _commit: String,
    ) -> Result<RemoteAccessPlan, MmError> {
        self.call("plan_team_sync", &scope)?;
        Ok(RemoteAccessPlan {
            binding: self.binding(&scope, RemotePlanKind::TeamSync),
            access: access(&scope),
        })
    }

    async fn apply_team_sync(
        &self,
        scope: RemoteScope,
        _remote_plan_hash: String,
    ) -> Result<AccessChange, MmError> {
        self.call("apply_team_sync", &scope)?;
        Ok(access(&scope))
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

fn scope(node: Option<&str>) -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(uuid::Uuid::from_u128(7)),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: node.map(str::to_owned),
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

fn stack(name: &str, services: &[&str]) -> mm::domain::workspace::StackSnapshot {
    mm::domain::workspace::StackSnapshot {
        name: name.to_owned(),
        compose_project_name: format!("mm-{name}"),
        artifact_hash: "b".repeat(64),
        rendered_config_hash: "c".repeat(64),
        services: services
            .iter()
            .map(|service| mm::domain::runtime::ServiceSnapshot {
                stack: name.to_owned(),
                name: (*service).to_owned(),
                module: None,
                nodes: Vec::new(),
                state: mm::domain::runtime::ServiceState::Running,
                starting: false,
                desired_count: 1,
                running_count: 1,
                healthy_count: 1,
                image: None,
                revision: None,
                restart_count: None,
                exit_code: None,
                created_at: None,
                started_at: None,
                finished_at: None,
                diagnostic_code: None,
                error: None,
                containers: Vec::new(),
                scale: None,
                restarts: None,
            })
            .collect(),
        owned_resources: Vec::new(),
    }
}

const MODULES: &str = "\
shop:
  path: shop
  services:
    web: {stack: app}
    worker: {stack: app}
";

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Cluster = (RemoteEnvironment, Vec<Arc<FakeNode>>);

/// The guard of a test mutex, whether or not another test thread panicked.
fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A three-node cluster and the scripted daemons behind it.
fn cluster() -> Result<Cluster, Box<dyn std::error::Error>> {
    cluster_of(vec![
        FakeNode::new("app-1", "1"),
        FakeNode::new("app-2", "2"),
        FakeNode::new("db-1", "3"),
    ])
}

fn cluster_of(fakes: Vec<Arc<FakeNode>>) -> Result<Cluster, Box<dyn std::error::Error>> {
    let modules: BTreeMap<String, mm::domain::config::ModuleConfig> =
        serde_yaml_ng::from_str(MODULES)?;
    let groups = BTreeMap::new();
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
            private_address: Some(Ipv4Addr::new(10, 90, 0, u8::try_from(index + 1)?)),
            transport: Ok(Arc::clone(fake) as Arc<dyn RemoteTransport>),
            scope: scope(Some(&fake.name)),
            placed: Some(placed),
        });
    }
    Ok((
        RemoteEnvironment::new("production".to_owned(), nodes)?,
        fakes,
    ))
}

fn names(nodes: &[&str]) -> Vec<String> {
    nodes.iter().map(|node| (*node).to_owned()).collect()
}

#[tokio::test]
async fn a_cluster_plan_has_a_section_per_node_under_one_sealed_hash() -> TestResult {
    let (environment, fakes) = cluster()?;
    assert!(environment.is_cluster());
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    assert_eq!(
        plan.nodes
            .iter()
            .map(|section| section.node.as_deref())
            .collect::<Vec<_>>(),
        [Some("app-1"), Some("app-2"), Some("db-1")],
        "sections follow the declared node order"
    );
    assert!(plan.skipped.is_empty());
    assert_eq!(plan.expires_at_unix_ms, 1_001, "the earliest node expiry");

    // The hash is the one over the ordered node sections, and every node
    // keeps exactly that seal.
    let sealed = fakes
        .iter()
        .map(|fake| locked(&fake.seal).clone())
        .collect::<Option<Vec<_>>>()
        .ok_or("every planned node keeps the seal")?;
    assert!(sealed.iter().all(|seal| *seal == sealed[0]));
    assert_eq!(sealed[0].cluster_plan_hash, plan.plan_hash);
    sealed[0].validate()?;
    assert_eq!(
        sealed[0]
            .sections
            .iter()
            .map(|section| (section.node.as_str(), section.plan_hash.clone()))
            .collect::<Vec<_>>(),
        [
            ("app-1", "1".repeat(64)),
            ("app-2", "2".repeat(64)),
            ("db-1", "3".repeat(64))
        ]
    );
    for fake in &fakes {
        assert_eq!(fake.calls(), ["plan_deployment", "seal_plan"]);
    }
    Ok(())
}

fn network_settings() -> Result<mm::domain::cluster_network::SealedNetworks, String> {
    Ok(mm::domain::cluster_network::SealedNetworks {
        space: "10.64.0.0/12".parse()?,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table: mm::domain::cluster_network::NetworkTable::default(),
        retired: Vec::new(),
    })
}

/// What a node stored after a deployment that put `backend` on `nodes`.
fn stored_networks(
    node: &str,
    nodes: &[&str],
) -> Result<mm::domain::cluster_network::NodeNetworks, String> {
    let mut sealed = network_settings()?;
    for holder in nodes {
        sealed.table.assign(
            sealed.space,
            &mm::domain::cluster_network::NetworkNeed {
                key: "0123456789ab-production-backend",
                node: holder,
                network: "mm-engine-backend",
                internal: false,
            },
        )?;
    }
    Ok(sealed.on_node(node))
}

#[tokio::test]
async fn a_cluster_plan_seals_the_network_addresses_the_nodes_hold() -> TestResult {
    let (environment, fakes) = cluster()?;
    let environment = environment.with_cluster_networks(Some(network_settings()?));
    // Only app-1 remembers that db-1 holds a slice; db-1 is left out of the
    // plan and does not answer.
    locked(&fakes[0].networks).stored = Some(stored_networks("app-1", &["app-1", "db-1"])?);
    fakes[2].up.store(false, Ordering::SeqCst);
    let only = names(&["app-1", "app-2"]);
    let plan = environment
        .plan_deployment(COMMIT, None, Some(&only))
        .await?;
    let networks = plan.networks.ok_or("the plan has no network section")?;
    let backend = &networks.sealed.table.networks["0123456789ab-production-backend"];
    assert_eq!(backend.nodes["db-1"].slice, "10.64.1.0/24".parse()?);
    assert_eq!(
        networks
            .nodes
            .iter()
            .map(|node| node.node.as_str())
            .collect::<Vec<_>>(),
        ["app-1", "app-2"]
    );
    // app-2 held nothing: it learns both routes and gets the chains.
    assert!(networks.nodes[0].routes.is_empty() && networks.nodes[0].firewall.is_empty());
    assert_eq!(
        networks.nodes[1].routes.added,
        ["10.64.0.0/24 to app-1", "10.64.1.0/24 to db-1"]
    );
    // The seal every planned node keeps carries the same addresses, and
    // the hash covers them.
    let seal = locked(&fakes[1].seal)
        .clone()
        .ok_or("app-2 keeps the seal")?;
    assert_eq!(seal.networks.as_ref(), Some(&networks.sealed));
    seal.validate()?;
    let mut tampered = seal.clone();
    if let Some(sealed) = &mut tampered.networks {
        sealed.mtu = 1400;
    }
    assert!(tampered.validate().is_err(), "the hash covers the networks");
    assert_eq!(
        fakes[1].calls(),
        ["plan_deployment", "networks", "seal_plan"]
    );

    // A node of the plan that cannot say what it holds fails the plan.
    locked(&fakes[1].networks).stored = None;
    let (environment, fakes) = cluster()?;
    let environment = environment.with_cluster_networks(Some(network_settings()?));
    fakes[1].up.store(false, Ordering::SeqCst);
    assert!(
        environment
            .plan_deployment(COMMIT, None, None)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn a_cluster_plan_refuses_a_space_a_planned_node_already_uses() -> TestResult {
    use mm::domain::cluster_network::{HostRange, HostRangeKind};

    let (environment, fakes) = cluster()?;
    let environment = environment.with_cluster_networks(Some(network_settings()?));
    let provider = vec![HostRange {
        range: "10.70.0.0/20".parse()?,
        kind: HostRangeKind::Route,
        interface: Some("eth1".to_owned()),
    }];
    // db-1 has it too, but is left out of the plan: nothing changes there.
    locked(&fakes[2].networks).host_ranges.clone_from(&provider);
    let only = names(&["app-1", "app-2"]);
    environment
        .plan_deployment(COMMIT, None, Some(&only))
        .await?;
    locked(&fakes[1].networks).host_ranges = provider;
    let error = environment
        .plan_deployment(COMMIT, None, Some(&only))
        .await
        .err()
        .ok_or("a space the node uses was planned")?;
    assert_eq!(
        error.to_string(),
        "target error: node 'app-2' already has the route 10.70.0.0/20 through eth1, which overlaps cluster.networks 10.64.0.0/12 (the addresses of networks that span the nodes). Set `cluster.networks` under targets.production.cluster to a private range no node uses, such as 10.32.0.0/12, then plan again"
    );
    let sealed = fakes
        .iter()
        .flat_map(|fake| fake.calls())
        .filter(|call| call == "seal_plan")
        .count();
    assert_eq!(
        sealed, 2,
        "only the first plan was sealed, on its two nodes"
    );
    Ok(())
}

#[tokio::test]
async fn status_puts_the_nodes_views_of_the_names_together() -> TestResult {
    use mm::domain::discovery::{Health, NameAddress, NameProblem, NameReport, ResolvedName};

    let (environment, fakes) = cluster()?;
    assert_eq!(
        environment.status().await?.names,
        None,
        "no node answers names"
    );

    let address = |address: [u8; 4], node: &str| NameAddress {
        address: address.into(),
        node: node.to_owned(),
        health: Health::Healthy,
        unknown_to: Vec::new(),
        restart_limit: false,
    };
    let view = |addresses: Vec<NameAddress>, problems: Vec<NameProblem>| NameReport {
        networks: vec!["backend".to_owned()],
        names: vec![ResolvedName {
            name: "web".to_owned(),
            network: "backend".to_owned(),
            addresses,
            lookups: Vec::new(),
            fail_open: false,
        }],
        problems,
    };
    // app-1 knows both containers; app-2 has not heard of app-1's.
    *locked(&fakes[0].names) = Some(view(
        vec![
            address([10, 64, 0, 2], "app-1"),
            address([10, 64, 1, 2], "app-2"),
        ],
        Vec::new(),
    ));
    *locked(&fakes[1].names) = Some(view(
        vec![address([10, 64, 1, 2], "app-2")],
        vec![NameProblem {
            node: "app-2".to_owned(),
            detail: "never heard from node 'app-1'".to_owned(),
        }],
    ));
    let names = environment.status().await?.names.ok_or("no names")?;
    assert_eq!(names.names.len(), 1);
    assert_eq!(
        names.names[0].addresses,
        vec![
            NameAddress {
                unknown_to: vec!["app-2".to_owned()],
                ..address([10, 64, 0, 2], "app-1")
            },
            address([10, 64, 1, 2], "app-2"),
        ]
    );
    assert_eq!(names.problems.len(), 1);
    Ok(())
}

#[tokio::test]
async fn status_carries_what_drifted_on_a_node() -> TestResult {
    let (environment, fakes) = cluster()?;
    let environment = environment.with_cluster_networks(Some(network_settings()?));
    locked(&fakes[1].networks).drift = vec![mm::domain::cluster_network::NetworkDrift::new(
        mm::domain::cluster_network::NetworkDriftKind::Firewall,
        "the chain MM-CLUSTER is missing",
    )];
    let snapshot = environment.status().await?;
    assert!(snapshot.nodes[0].network_drift.is_empty());
    assert_eq!(
        snapshot.nodes[1].network_drift,
        ["the chain MM-CLUSTER is missing"]
    );
    // It came with the node's own answer: the status asked nothing else.
    for fake in &fakes {
        assert_eq!(fake.calls(), ["status"]);
    }
    let report = environment.network_report().await;
    assert_eq!(report.problems.len(), 1);
    assert_eq!(report.problems[0].code, "MM_NETWORK_DRIFT");
    Ok(())
}

#[tokio::test]
async fn an_unreachable_node_fails_the_plan_and_names_the_way_around_it() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[1].up.store(false, Ordering::SeqCst);
    let error = environment
        .plan_deployment(COMMIT, None, None)
        .await
        .err()
        .ok_or("expected a failure")?;
    assert_eq!(error.code(), "MM_UNREACHABLE");
    let message = error.to_string();
    for expected in [
        "node 'app-2' (203.0.113.2:22)",
        "did not answer (Connection timed out)",
        "Check that the host is up",
        "add `--nodes app-1,db-1`",
        "app-2 keeps what it runs now",
    ] {
        assert!(message.contains(expected), "{expected}: {message}");
    }
    assert!(
        fakes
            .iter()
            .all(|fake| !fake.calls().contains(&"seal_plan".to_owned())),
        "nothing is sealed when a node did not plan"
    );

    // Two nodes down are both named, with the one node left to act on.
    fakes[2].up.store(false, Ordering::SeqCst);
    let message = environment
        .plan_team_sync(COMMIT, None)
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(message.contains("node 'app-2'"), "{message}");
    assert!(
        message.contains("node 'db-1' (203.0.113.3:22)"),
        "{message}"
    );
    assert!(message.contains("add `--nodes app-1`"), "{message}");
    Ok(())
}

#[tokio::test]
async fn a_subset_plan_leaves_the_other_nodes_alone_and_says_so() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[1].up.store(false, Ordering::SeqCst);
    let only = names(&["db-1", "app-1"]);
    let plan = environment
        .plan_deployment(COMMIT, None, Some(&only))
        .await?;
    assert_eq!(
        plan.nodes
            .iter()
            .map(|section| section.node.as_deref())
            .collect::<Vec<_>>(),
        [Some("app-1"), Some("db-1")],
        "declared order, whatever order --nodes was written in"
    );
    assert_eq!(plan.skipped, ["app-2"]);
    assert!(
        fakes[1].calls().is_empty(),
        "a left-out node is not contacted"
    );
    let seal = locked(&fakes[0].seal)
        .clone()
        .ok_or("the first node keeps the seal")?;
    assert_eq!(seal.skipped, ["app-2"]);
    assert!(seal.section("app-2").is_none());

    // The full plan and the subset plan are different plans.
    fakes[1].up.store(true, Ordering::SeqCst);
    let full = environment.plan_deployment(COMMIT, None, None).await?;
    assert_ne!(full.plan_hash, plan.plan_hash);

    let unknown = environment
        .plan_deployment(COMMIT, None, Some(&names(&["app-9"])))
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(unknown.contains("'app-9', which is not a node of target 'production'"));
    assert!(unknown.contains("app-1, app-2, db-1"), "{unknown}");
    Ok(())
}

#[tokio::test]
async fn a_node_that_cannot_begin_rolls_the_earlier_ones_back_and_spares_the_later() -> TestResult {
    let (environment, fakes) = cluster()?;
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    fakes[1].fail_apply.store(true, Ordering::SeqCst);
    let visited = Mutex::new(Vec::new());
    let report = environment
        .apply_deployment(&plan.plan_hash, &|node| {
            locked(&visited).push(node.to_string());
        })
        .await?;
    assert_eq!(
        locked(&visited)[1..],
        [
            "app-1: preparing",
            "app-2: preparing",
            "app-2: rolling back",
            "app-1: rolling back"
        ]
    );
    assert_eq!(
        report
            .nodes
            .iter()
            .map(|node| (node.node.as_str(), node.state))
            .collect::<Vec<_>>(),
        [
            ("app-1", NodeApplyState::RolledBack),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::NotStarted),
        ]
    );
    assert!(
        report.nodes[1]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("the healthcheck did not pass"))
    );
    assert!(report.nodes[1].rolled_back);
    assert_eq!(
        fakes[2]
            .calls()
            .into_iter()
            .filter(|call| call.starts_with("rollout:"))
            .collect::<Vec<_>>(),
        ["rollout:hold", "rollout:networks", "rollout:release"],
        "a node after the failure took the lock and the plan's addresses, and gave the lock up"
    );
    assert!(!fakes[2].calls().contains(&"rollout:begin".to_owned()));
    let failure = report.failure("mm deploy plan").unwrap_or_default();
    assert!(failure.contains("failed on node 'app-2'"), "{failure}");
    assert!(
        failure.contains("Rolled back to what they ran before: app-1, app-2."),
        "{failure}"
    );
    assert!(failure.contains("Not changed: db-1."), "{failure}");
    Ok(())
}

fn moved(service: &str, to: &str) -> LeavingService {
    LeavingService {
        stack: "app".to_owned(),
        service: service.to_owned(),
        compose_project_name: "shop-production-app".to_owned(),
        moves_to: vec![to.to_owned()],
    }
}

/// Every call any node received, in order, as `node:method`.
fn applies_and_finishes(fakes: &[Arc<FakeNode>]) -> Vec<Vec<String>> {
    fakes
        .iter()
        .map(|fake| {
            fake.calls()
                .into_iter()
                .filter(|call| call == "rollout:finish" || call == "finish_moves")
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn a_moved_service_stops_on_its_old_node_only_after_every_node_applied() -> TestResult {
    let (environment, fakes) = cluster()?;
    // worker leaves app-1 for app-2: app-1 is first in the walk, so only a
    // step after the walk can stop it there once app-2 runs it.
    *locked(&fakes[0].moved_away) = vec![moved("worker", "app-2")];
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    let steps = Mutex::new(Vec::new());
    let report = environment
        .apply_deployment(&plan.plan_hash, &|step| {
            locked(&steps).push(step.to_string());
        })
        .await?;
    assert_eq!(
        locked(&steps)[1..],
        [
            "app-1: preparing",
            "app-2: preparing",
            "db-1: preparing",
            "app-1: finishing",
            "app-2: finishing",
            "db-1: finishing",
            "app-1: stopping worker (now on app-2)"
        ],
        "the stop comes after the last node finished"
    );
    assert_eq!(
        applies_and_finishes(&fakes),
        [
            vec!["rollout:finish".to_owned(), "finish_moves".to_owned()],
            vec!["rollout:finish".to_owned()],
            vec!["rollout:finish".to_owned()],
        ],
        "only the node the service left is asked to stop it"
    );
    assert!(report.succeeded());
    assert_eq!(
        report.nodes[0].stopped,
        [PlacedService {
            stack: "app".to_owned(),
            service: "worker".to_owned()
        }]
    );
    assert!(report.nodes[1].stopped.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_failed_walk_stops_nothing_so_the_service_keeps_running_where_it_was() -> TestResult {
    let (environment, fakes) = cluster()?;
    *locked(&fakes[0].moved_away) = vec![moved("worker", "app-2")];
    fakes[1].fail_apply.store(true, Ordering::SeqCst);
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    let report = environment
        .apply_deployment(&plan.plan_hash, &|_| {})
        .await?;
    assert!(!report.succeeded());
    assert!(
        !fakes[0].calls().contains(&"finish_moves".to_owned()),
        "app-2 never started worker, so app-1 keeps it"
    );
    Ok(())
}

#[tokio::test]
async fn a_stop_that_fails_is_reported_on_the_node_that_still_runs_the_service() -> TestResult {
    let (environment, fakes) = cluster()?;
    *locked(&fakes[0].moved_away) = vec![moved("worker", "app-2")];
    fakes[0].fail_finish.store(true, Ordering::SeqCst);
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    let report = environment
        .apply_deployment(&plan.plan_hash, &|_| {})
        .await?;
    assert_eq!(report.nodes[0].state, NodeApplyState::Failed);
    let error = report.nodes[0].error.clone().unwrap_or_default();
    assert!(error.contains("the node applied its section"), "{error}");
    assert!(error.contains("worker (now on app-2)"), "{error}");
    assert!(
        error.contains("the engine did not stop the container"),
        "{error}"
    );
    assert!(error.contains("still run on this node"), "{error}");
    assert_eq!(report.nodes[1].state, NodeApplyState::Applied);
    assert!(report.failure("mm deploy plan").is_some());
    Ok(())
}

#[tokio::test]
async fn apply_changes_nothing_unless_every_node_still_holds_the_reviewed_plan() -> TestResult {
    let (environment, fakes) = cluster()?;
    let plan = environment.plan_deployment(COMMIT, None, None).await?;

    // A hash nobody sealed is refused by the nodes themselves.
    let unknown = environment
        .apply_deployment(&"9".repeat(64), &|_| {})
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(
        unknown.contains("holds no reviewed cluster plan"),
        "{unknown}"
    );

    // One node lost its seal (it expired, or the node was reinstalled).
    *locked(&fakes[2].seal) = None;
    let lost = environment
        .apply_deployment(&plan.plan_hash, &|_| {})
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(lost.contains("node 'db-1'"), "{lost}");

    // One node of the plan does not answer any more.
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    fakes[1].up.store(false, Ordering::SeqCst);
    let down = environment
        .apply_deployment(&plan.plan_hash, &|_| {})
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(down.contains("node 'app-2'"), "{down}");
    assert!(down.contains("Nothing was applied"), "{down}");
    assert!(
        fakes
            .iter()
            .all(|fake| !fake.calls().contains(&"rollout:begin".to_owned())),
        "no node was changed by any of the refused applies"
    );

    // A team sync plan is not a deployment plan.
    fakes[1].up.store(true, Ordering::SeqCst);
    let sync = environment.plan_team_sync(COMMIT, None).await?;
    let wrong_kind = environment
        .apply_deployment(&sync.plan_hash, &|_| {})
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(wrong_kind.contains("mm team sync --plan"), "{wrong_kind}");
    Ok(())
}

#[tokio::test]
async fn a_subset_plan_applies_only_to_its_nodes() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[2].up.store(false, Ordering::SeqCst);
    let plan = environment
        .plan_team_sync(COMMIT, Some(&names(&["app-1", "app-2"])))
        .await?;
    // The left-out node is still down at apply time; the apply does not
    // need it.
    let report = environment
        .apply_team_sync(&plan.plan_hash, &|_| {})
        .await?;
    assert!(report.succeeded());
    assert_eq!(
        report
            .nodes
            .iter()
            .map(|node| node.node.as_str())
            .collect::<Vec<_>>(),
        ["app-1", "app-2"]
    );
    assert_eq!(report.skipped, ["db-1"]);
    assert!(!fakes[2].calls().contains(&"apply_team_sync".to_owned()));
    Ok(())
}

#[tokio::test]
async fn status_asks_every_node_and_says_which_nodes_run_each_service() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[2].up.store(false, Ordering::SeqCst);
    let snapshot = environment.status().await?;
    assert_eq!(
        snapshot.stacks[0]
            .services
            .iter()
            .map(|service| {
                let nodes = service.nodes.iter().map(|share| share.node.as_str());
                (service.name.as_str(), nodes.collect::<Vec<_>>())
            })
            .collect::<Vec<_>>(),
        [("web", vec!["app-1"]), ("worker", vec!["app-1"])]
    );
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .map(|node| (node.name.as_str(), node.reachable))
            .collect::<Vec<_>>(),
        [("app-1", true), ("app-2", true), ("db-1", false)]
    );
    assert!(
        snapshot.nodes[2]
            .problem
            .as_deref()
            .is_some_and(|problem| problem.contains("did not answer"))
    );

    // When no node answers, the status fails with the first node's reason.
    for fake in &fakes {
        fake.up.store(false, Ordering::SeqCst);
    }
    let error = environment
        .status()
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(error.contains("node 'app-1'"), "{error}");
    Ok(())
}

#[tokio::test]
async fn logs_and_commands_go_to_the_node_that_runs_the_service() -> TestResult {
    // (A service on several nodes is read from all of them:
    // `service_across_nodes.rs`.)
    let (environment, fakes) = cluster()?;
    let selector = |service: &str| ServiceSelector {
        stack: None,
        module: None,
        service: service.to_owned(),
        container: None,
    };
    let approval = || ApprovalContext {
        principal: "alice".to_owned(),
        source: "test".to_owned(),
        approved_high_risk: false,
    };
    let mut log = environment
        .logs(
            LogRequest::new(selector("worker"), approval()),
            OperationCancellation::default(),
        )
        .await?;
    // The node is asked while the stream is read; it has no lines.
    assert!(log.recv().await.is_none());
    environment
        .exec(
            ExecRequest::new(selector("web"), vec!["true".to_owned()], approval()),
            OperationCancellation::default(),
        )
        .await?;
    assert_eq!(fakes[0].calls(), ["logs", "exec"]);
    assert!(fakes[1].calls().is_empty() && fakes[2].calls().is_empty());
    assert_eq!(
        environment.node_running("web")?.name.as_deref(),
        Some("app-1")
    );

    let missing = environment
        .logs(
            LogRequest::new(selector("cache"), approval()),
            OperationCancellation::default(),
        )
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(missing.contains("no node of target 'production' runs a service named 'cache'"));
    Ok(())
}

#[tokio::test]
async fn node_reports_cover_every_node_reachable_or_not() -> TestResult {
    let (environment, fakes) = cluster_of(vec![
        FakeNode::new("app-1", "1"),
        FakeNode::new("app-2", "2"),
        FakeNode::answering_as("db-1", "3", Some("app-1")),
    ])?;
    fakes[1].up.store(false, Ordering::SeqCst);
    let mut nodes = environment.nodes().to_vec();
    nodes.push(RemoteNode {
        name: Some("new-1".to_owned()),
        host: "203.0.113.9".to_owned(),
        port: 2222,
        private_address: None,
        transport: Err(NodeUnavailable {
            subject: "node 'new-1' (203.0.113.9:2222)".to_owned(),
            problem: "has no recorded keys yet".to_owned(),
            help: "Run `mm target setup plan`".to_owned(),
        }),
        scope: scope(Some("new-1")),
        placed: None,
    });
    let environment = RemoteEnvironment::new("production".to_owned(), nodes)?;

    let reports = environment.node_reports().await;
    assert_eq!(
        reports
            .iter()
            .map(|report| (report.name.as_str(), report.state, report.reachable))
            .collect::<Vec<_>>(),
        [
            ("app-1", NodeState::Ready, true),
            ("app-2", NodeState::Unreachable, false),
            ("db-1", NodeState::WrongNode, false),
            ("new-1", NodeState::NotSetUp, false),
        ]
    );
    let ready = &reports[0];
    assert_eq!(ready.mmd_version.as_deref(), Some("0.1.0"));
    assert_eq!(ready.mesh_address, Some(Ipv4Addr::new(10, 90, 0, 1)));
    assert_eq!(
        ready
            .deployed
            .as_ref()
            .map(|deployed| deployed.commit.as_str()),
        Some(COMMIT)
    );
    assert!(
        reports[1]
            .problem
            .as_deref()
            .unwrap_or_default()
            .contains("did not answer")
    );
    let mismatch = reports[2].problem.as_deref().unwrap_or_default();
    assert!(mismatch.contains("for node 'db-1'") && mismatch.contains("is node 'app-1'"));
    assert!(
        reports[3]
            .problem
            .as_deref()
            .unwrap_or_default()
            .contains("mm target setup plan")
    );

    let json = serde_json::to_value(&reports)?;
    assert_eq!(json[1]["state"], "unreachable");
    assert_eq!(json[0]["mesh_address"], "10.90.0.1");
    assert_eq!(json[0]["deployed"]["runs_services"], true);
    Ok(())
}

#[tokio::test]
async fn the_mesh_report_asks_every_node_and_explains_a_silent_pair() -> TestResult {
    let (environment, fakes) = cluster()?;
    let report = environment.mesh_report().await;
    assert!(report.healthy(), "{:?}", report.problems);
    assert_eq!(report.target, "production");
    assert_eq!(
        report
            .nodes
            .iter()
            .map(|node| (node.name.as_str(), node.summary()))
            .collect::<Vec<_>>(),
        [
            ("app-1", Some("mesh 2/2 peers".to_owned())),
            ("app-2", Some("mesh 2/2 peers".to_owned())),
            ("db-1", Some("mesh 2/2 peers".to_owned())),
        ]
    );
    assert!(fakes.iter().all(|fake| fake.calls() == ["mesh_status"]));

    // app-1 and db-1 do not hear each other, and app-2's mmd is down.
    *locked(&fakes[0].silent_peer) = Some("db-1".to_owned());
    *locked(&fakes[2].silent_peer) = Some("app-1".to_owned());
    fakes[1].up.store(false, Ordering::SeqCst);
    let report = environment.mesh_report().await;
    assert!(!report.healthy());
    let unreachable = &report.nodes[1];
    assert!(!unreachable.reachable && unreachable.status.is_none());
    assert!(
        unreachable
            .problem
            .as_deref()
            .unwrap_or_default()
            .contains("did not answer"),
        "{unreachable:?}"
    );
    assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
    assert_eq!(report.problems[0].code, "MM_MESH_NO_HANDSHAKE");
    let help = report.problems[0].help.as_deref().unwrap_or_default();
    assert!(
        help.contains(
            "UDP port 51820 must be open between 203.0.113.1 (app-1) and 203.0.113.3 (db-1)"
        ),
        "{help}"
    );
    let json = serde_json::to_value(&report)?;
    assert_eq!(json["nodes"][0]["status"]["peers"][1]["name"], "db-1");
    assert_eq!(
        json["nodes"][0]["status"]["peers"][0]["handshake_age_seconds"],
        20
    );
    assert!(
        json["nodes"][0]["status"]["peers"][1]
            .get("handshake_age_seconds")
            .is_none()
    );
    assert_eq!(json["problems"][0]["code"], "MM_MESH_NO_HANDSHAKE");

    // A target on a private network the nodes already share has no mesh.
    let existing = environment.on_existing_interface(Some("eth1".to_owned()));
    for fake in &fakes {
        locked(&fake.calls).clear();
    }
    let report = existing.mesh_report().await;
    assert_eq!(report.existing_interface.as_deref(), Some("eth1"));
    assert!(report.nodes.is_empty() && report.healthy());
    assert!(
        fakes.iter().all(|fake| fake.calls().is_empty()),
        "no node is asked"
    );
    Ok(())
}

/// A target that names one host: one unnamed node, no seal.
struct OneHost(Mutex<Vec<String>>);

#[async_trait]
impl RemoteTransport for OneHost {
    async fn status(&self, scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        Ok(snapshot(&scope))
    }

    async fn logs(
        &self,
        _scope: RemoteScope,
        _request: LogRequest,
        _cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        unused()
    }

    async fn exec(
        &self,
        _scope: RemoteScope,
        _request: ExecRequest,
        _cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        unused()
    }

    async fn plan_deployment(
        &self,
        _scope: RemoteScope,
        _commit: String,
        _receipt: Option<BuildReceipt>,
        _release: mm::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError> {
        unused()
    }

    async fn apply_deployment(
        &self,
        _scope: RemoteScope,
        _remote_plan_hash: String,
    ) -> Result<Option<DeploymentResult>, MmError> {
        Err(MmError::Security {
            message: "the host refused".to_owned(),
        })
    }

    async fn seal_plan(
        &self,
        _scope: RemoteScope,
        _seal: ClusterPlanSeal,
    ) -> Result<ClusterPlanSeal, MmError> {
        locked(&self.0).push("seal_plan".to_owned());
        unused()
    }

    async fn sealed_plan(
        &self,
        _scope: RemoteScope,
        _cluster_plan_hash: String,
    ) -> Result<ClusterPlanSeal, MmError> {
        locked(&self.0).push("sealed_plan".to_owned());
        unused()
    }

    async fn node(&self, _scope: RemoteScope) -> Result<NodeFacts, MmError> {
        unused()
    }

    async fn plan_team_sync(
        &self,
        scope: RemoteScope,
        _commit: String,
    ) -> Result<RemoteAccessPlan, MmError> {
        assert_eq!(scope.node, None);
        Ok(RemoteAccessPlan {
            binding: FakeNode::new("host", "4").binding(&scope, RemotePlanKind::TeamSync),
            access: access(&scope),
        })
    }

    async fn apply_team_sync(
        &self,
        scope: RemoteScope,
        remote_plan_hash: String,
    ) -> Result<AccessChange, MmError> {
        locked(&self.0).push(remote_plan_hash);
        Ok(access(&scope))
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

#[tokio::test]
async fn a_single_host_plans_and_applies_with_its_own_plan_hash() -> TestResult {
    let host = Arc::new(OneHost(Mutex::new(Vec::new())));
    let environment = RemoteEnvironment::new(
        "staging".to_owned(),
        vec![RemoteNode {
            name: None,
            host: "staging.example.test".to_owned(),
            port: 22,
            private_address: None,
            transport: Ok(Arc::clone(&host) as Arc<dyn RemoteTransport>),
            scope: scope(None),
            placed: None,
        }],
    )?;
    assert!(!environment.is_cluster());

    let plan = environment.plan_team_sync(COMMIT, None).await?;
    assert_eq!(plan.plan_hash, "4".repeat(64), "the host's own plan hash");
    assert_eq!(plan.nodes.len(), 1);
    assert_eq!(plan.nodes[0].node, None);
    let report = environment
        .apply_team_sync(&plan.plan_hash, &|_| {})
        .await?;
    assert!(report.succeeded());
    assert_eq!(report.nodes[0].node, "staging");
    assert_eq!(*locked(&host.0), ["4".repeat(64)], "no seal round trip");

    // A failure on the one host is the command's error, as before.
    let refused = environment.apply_deployment(&"4".repeat(64), &|_| {}).await;
    assert!(matches!(refused, Err(MmError::Security { .. })));

    let nodes = environment
        .plan_team_sync(COMMIT, Some(&names(&["app-1"])))
        .await
        .err()
        .ok_or("expected a failure")?
        .to_string();
    assert!(nodes.contains("names one host; --nodes applies"), "{nodes}");
    assert_eq!(
        environment.node_running("anything")?.name,
        None,
        "a single host runs every service"
    );
    Ok(())
}

#[tokio::test]
async fn status_says_what_is_known_each_time_a_node_answers() -> TestResult {
    let (environment, fakes) = cluster()?;
    fakes[2].up.store(false, Ordering::SeqCst);
    let so_far = Mutex::new(Vec::new());
    let report = |snapshot: WorkspaceSnapshot| locked(&so_far).push(snapshot);
    let last = environment.status_reporting(Some(&report)).await?;

    // The last word is the same status as without anyone listening, and it
    // is the result, not one more report.
    assert_eq!(last, environment.status().await?);
    let reports = std::mem::take(&mut *locked(&so_far));
    assert!(!reports.is_empty() && reports.len() < fakes.len());
    for (index, snapshot) in reports.iter().enumerate() {
        // Every node is listed from the first report on, in declared order.
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["app-1", "app-2", "db-1"]
        );
        // One more node has answered or failed with each report, and the
        // others are neither reachable nor failed: they are waited for.
        let waited_for = snapshot
            .nodes
            .iter()
            .filter(|node| node.is_pending())
            .count();
        assert!(
            waited_for > 0 && waited_for <= fakes.len() - 1 - index,
            "{snapshot:?}"
        );
    }
    assert!(last.nodes.iter().all(|node| !node.is_pending()));

    // One host answers once; there is nothing to report on the way.
    let host: Arc<dyn RemoteTransport> = Arc::new(OneHost(Mutex::new(Vec::new())));
    let one = RemoteEnvironment::new(
        "staging".to_owned(),
        vec![RemoteNode {
            name: None,
            host: "staging.example.test".to_owned(),
            port: 22,
            private_address: None,
            transport: Ok(host),
            scope: scope(None),
            placed: None,
        }],
    )?;
    one.status_reporting(Some(&report)).await?;
    assert!(locked(&so_far).is_empty());
    Ok(())
}

#[tokio::test]
async fn the_terminal_uis_review_is_what_deploy_plan_prints() -> TestResult {
    let (environment, _fakes) = cluster()?;
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    let lines = mm::presentation::output::remote_plan_lines(&plan, true);
    assert_eq!(
        lines.first().map(String::as_str),
        Some(format!("Cluster deployment plan {}", plan.plan_hash).as_str())
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "  target:   production (app-1, app-2, db-1)"),
        "{lines:#?}"
    );
    // Each node's section, in declared order, with its own plan.
    let sections = lines
        .iter()
        .filter(|line| line.starts_with("Node "))
        .map(|line| line.split_whitespace().nth(1).unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(sections, ["app-1", "app-2", "db-1"], "{lines:#?}");
    // How to apply it is the command's last line, not the review's.
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("deploy apply --plan"))
    );

    // One host: the host's own plan, as `mm deploy plan` prints it.
    let host = mm::presentation::output::remote_plan_lines(&plan, false);
    assert!(
        host.first()
            .is_some_and(|line| line.starts_with("Remote deployment plan ")),
        "{host:#?}"
    );
    Ok(())
}

/// What a node of a hardened push target plans for `app`: `db` keeps a
/// writable root (its exception), `web` mounts a config file rendered
/// from a template, and the host lets `vpn` use its network.
fn hardened_node_plan() -> mm::domain::deployment::DeploymentPlan {
    use mm::domain::config_file::ConfigFileUse;
    use mm::domain::deployment::{
        DEPLOYMENT_PLAN_SCHEMA, DeploymentPlan, DeploymentRouteDiff, DeploymentStackPlan,
    };
    use mm::domain::hardening::{HardeningException, HardeningKey, ServiceSecurity};
    use mm::domain::stack::{HostNetworkUse, PreparedStackArtifact, StackArtifactManifest};
    use mm::domain::target::TargetId;
    use mm::domain::traefik_dynamic::{PreparedTraefikDynamicConfig, TraefikDynamicConfigManifest};

    let scope = scope(Some("app-1"));
    let target_id = TargetId(uuid::Uuid::from_u128(7));
    let dynamic = PreparedTraefikDynamicConfig {
        root: "/var/lib/mm-home/routes".into(),
        path: "/var/lib/mm-home/routes/dynamic.yaml".into(),
        manifest: TraefikDynamicConfigManifest {
            target_id,
            claim_count: 0,
            router_count: 0,
            service_count: 0,
            digest: "b".repeat(64),
        },
    };
    let db = ServiceSecurity {
        service: "db".to_owned(),
        hardened: true,
        capabilities: Some(Vec::new()),
        no_new_privileges: true,
        tmpfs: vec!["/tmp".to_owned()],
        exceptions: vec![HardeningException {
            key: HardeningKey::ReadOnly,
            reason: "Postgres writes its socket under /var/run".to_owned(),
        }],
        ..ServiceSecurity::default()
    };
    let manifest = StackArtifactManifest {
        project_id: scope.project_id,
        project_name: "shop".to_owned(),
        target: scope.target.clone(),
        target_id,
        environment: scope.environment.clone(),
        stack: "app".to_owned(),
        compose_project_name: "shop-production-app".to_owned(),
        user_files: Vec::new(),
        generated_overlay_digest: "d".repeat(64),
        services: Vec::new(),
        routes: Vec::new(),
        middleware_inputs: Vec::new(),
        preserved_volumes: Vec::new(),
        security: vec![db],
        low_ports: Vec::new(),
        host_network: vec![HostNetworkUse {
            service: "vpn".to_owned(),
            reason: None,
        }],
        artifact_hash: "e".repeat(64),
    };
    let artifact_root = std::path::PathBuf::from("/var/lib/mm-home/compose/app");
    DeploymentPlan {
        excluded_services: Vec::new(),
        schema: DEPLOYMENT_PLAN_SCHEMA.to_owned(),
        project_id: scope.project_id,
        project_name: "shop".to_owned(),
        target_name: scope.target.clone(),
        target_id,
        target_fingerprint: scope.target_fingerprint.clone(),
        environment: scope.environment.clone(),
        runtime: Some(scope.runtime),
        runtime_source: None,
        workload_intent_hash: None,
        networks: Vec::new(),
        expected_route_generation: 0,
        expected_route_config_digest: None,
        expected_previous_plan_hash: None,
        stacks: vec![DeploymentStackPlan {
            stack: "app".to_owned(),
            compose_project_name: "shop-production-app".to_owned(),
            artifact_hash: "e".repeat(64),
            project_directory: "/srv/mm/projects/shop".into(),
            artifact: PreparedStackArtifact {
                manifest_path: artifact_root.join("manifest.json"),
                generated_overlay_path: artifact_root.join("overlay.yaml"),
                root: artifact_root,
                compose_files: Vec::new(),
                manifest,
            },
        }],
        removed_stacks: Vec::new(),
        preserved_volumes: Vec::new(),
        removed_preserved_volumes: Vec::new(),
        previous_owner_claims: Vec::new(),
        desired_route_claims: Vec::new(),
        complete_route_claims: Vec::new(),
        desired_middlewares: BTreeMap::new(),
        environment_review: Default::default(),
        runtime_secret_plan: None,
        scheduler_reconciliation: None,
        environment_bundle_root: None,
        previous_dynamic_config: dynamic.clone(),
        dynamic_config: dynamic,
        local_certificates: Vec::new(),
        static_ingress: None,
        removed_ingress: None,
        route_diff: DeploymentRouteDiff::default(),
        ingress: Vec::new(),
        route_state_changed: false,
        build_receipt: None,
        local_build: None,
        registries: BTreeMap::new(),
        node: None,
        rollout: Default::default(),
        config_files: vec![ConfigFileUse {
            stack: "app".to_owned(),
            service: "web".to_owned(),
            target: "/etc/nginx/nginx.conf".to_owned(),
            source: "web/nginx.conf".to_owned(),
            template: true,
            version: "b".repeat(64),
            previous: None,
        }],
        runtime_changes: Vec::new(),
        unknown_config_keys: Vec::new(),
        release: None,
        plan_hash: "f".repeat(64),
    }
}

/// The lines hardening, config files and push delivery add to a plan are
/// written by the functions `mm deploy plan` prints with, so the terminal
/// UI's review shows them as well.
#[tokio::test]
async fn the_review_shows_hardening_config_files_and_the_pushed_bundle() -> TestResult {
    use mm::presentation::tui::StartReview;
    use mm::presentation::tui::driver::ScriptedUi;

    let (environment, _fakes) = cluster()?;
    let mut plan = environment.plan_deployment(COMMIT, None, None).await?;
    let bundle = "4b".repeat(32);
    plan.bundle = Some(mm::domain::delivery::PlanBundle {
        bundle_hash: bundle.clone(),
        bytes: 3 * 1024 * 1024 / 2,
        files: 812,
        sent_to: names(&["app-1", "db-1"]),
    });
    for section in &mut plan.nodes {
        section.plan.binding.bundle_hash = Some(bundle.clone());
    }
    let first = plan.nodes.first_mut().ok_or("the plan has no node")?;
    first.plan.plan = Some(hardened_node_plan());
    let planned = [
        "    security: db: writable root, no capabilities, no new privileges, default seccomp, tmpfs /tmp",
        "    security: db: not read-only (the root filesystem stays writable): Postgres writes its socket under /var/run",
        "    network: vpn uses the host's network, which the host's policy allows (network-host)",
        "  config file: app/web /etc/nginx/nginx.conf from template web/nginx.conf (version bbbbbbbbbbbb): new",
    ];
    let sent = format!("  bundle:   {bundle} (812 files, 1.5 MiB; sent to app-1, db-1)");
    let lines = mm::presentation::output::remote_plan_lines(&plan, true);
    for line in planned.iter().copied().chain([sent.as_str()]) {
        assert!(
            lines.iter().any(|shown| shown == line),
            "{line:?} is not in {lines:#?}"
        );
    }

    let mut ui = ScriptedUi::new(snapshot(&scope(None)), Some("production"));
    ui.press("3");
    assert_eq!(ui.press("u"), "Stack(Review)");
    ui.reviewed(StartReview {
        plan_hash: plan.plan_hash.clone(),
        target: plan.target.clone(),
        stacks: vec!["app".to_owned()],
        routes: 0,
        remote: true,
        nodes: names(&NAMES),
        lines,
        rollback: None,
    });
    let screen = ui.screen(200, 90)?;
    for line in planned.iter().copied().chain([sent.as_str()]) {
        assert!(
            screen.contains(line.trim()),
            "{line:?} is not on:\n{screen}"
        );
    }

    // One host: its own plan says which bundle it planned from.
    let host = mm::presentation::output::remote_plan_lines(&plan, false);
    let held = format!("  bundle:   {bundle} (pushed by mm; the host fetched nothing)");
    for line in planned.iter().copied().chain([held.as_str()]) {
        assert!(
            host.iter().any(|shown| shown == line),
            "{line:?} is not in {host:#?}"
        );
    }
    Ok(())
}
