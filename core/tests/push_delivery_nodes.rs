//! Push-mode delivery across the nodes of a cluster, against scripted
//! nodes, and the configuration that turns it on: every node of a plan gets
//! the commit's bundle from the client before it is asked to plan, a node
//! that holds the commit is sent nothing, and a node that planned from
//! other files is refused.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::app::ports::{CommitBundles, ExecStream, LogStream, PreparedBundle, RemoteTransport};
use mm::app::{RemoteEnvironment, RemoteNode};
use mm::domain::build::BuildReceipt;
use mm::domain::delivery::{BundleOffer, Delivered, Delivery, OfferedBundle};
use mm::domain::deployment::DeploymentResult;
use mm::domain::error::MmError;
use mm::domain::host_policy::{EnvironmentAccessRecord, HostPolicy};
use mm::domain::observability::{ExecRequest, LogRequest, OperationCancellation};
use mm::domain::remote::{
    ClusterPlanSeal, NodeFacts, RemoteAccessPlan, RemoteDeploymentPlan, RemoteIdentity,
    RemotePlanBinding, RemotePlanKind, RemoteScope,
};
use mm::domain::repo_target::RepoTargetConfig;
use mm::domain::runtime::RuntimeKind;
use mm::domain::users::AccessChange;
use mm::domain::workspace::{ProjectId, WorkspaceSnapshot};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const COMMIT: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn bundle_hash() -> String {
    "b".repeat(64)
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn unused<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not used by this test".to_owned(),
    })
}

/// One node's `mmd`, scripted: it records the order of its calls, takes a
/// delivery, and plans from the bundle it holds.
struct Node {
    name: String,
    digit: &'static str,
    /// Whether the node holds the commit before this test delivers it.
    holds: AtomicBool,
    /// The bundle the node says it planned from, when not the delivered one.
    plans_from: Mutex<Option<String>>,
    calls: Mutex<Vec<String>>,
    offers: Mutex<Vec<(BundleOffer, bool)>>,
}

impl Node {
    fn new(name: &str, digit: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_owned(),
            digit,
            holds: AtomicBool::new(false),
            plans_from: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
            offers: Mutex::new(Vec::new()),
        })
    }

    fn call(&self, method: &str, scope: &RemoteScope) {
        assert_eq!(scope.node.as_deref(), Some(self.name.as_str()));
        locked(&self.calls).push(method.to_owned());
    }

    fn calls(&self) -> Vec<String> {
        locked(&self.calls).clone()
    }

    fn binding(&self, scope: &RemoteScope, kind: RemotePlanKind) -> RemotePlanBinding {
        let planned_from = locked(&self.plans_from).clone();
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
            expires_at_unix_ms: 1_000,
            bundle_hash: planned_from.or_else(|| {
                // A node plans from what it was delivered.
                self.calls()
                    .iter()
                    .any(|call| call == "deliver")
                    .then(bundle_hash)
            }),
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

#[async_trait]
impl RemoteTransport for Node {
    async fn status(&self, _scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        unused()
    }

    async fn deliver(
        &self,
        scope: RemoteScope,
        offer: BundleOffer,
        bundle: Option<std::path::PathBuf>,
    ) -> Result<Delivered, MmError> {
        self.call("deliver", &scope);
        locked(&self.offers).push((offer.clone(), bundle.is_some()));
        Ok(Delivered {
            commit: offer.commit,
            bundle_hash: bundle_hash(),
            bytes: 2048,
            files: 3,
            received: !self.holds.load(Ordering::SeqCst),
        })
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
        scope: RemoteScope,
        _commit: String,
        _receipt: Option<BuildReceipt>,
        _release: mm::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError> {
        self.call("plan_deployment", &scope);
        Ok(RemoteDeploymentPlan {
            binding: self.binding(&scope, RemotePlanKind::Deployment),
            plan: None,
            access: access(&scope),
            receipt: None,
            unfinished: None,
            once: Default::default(),
            staged: Vec::new(),
            release: None,
        })
    }

    async fn apply_deployment(
        &self,
        _scope: RemoteScope,
        _remote_plan_hash: String,
    ) -> Result<Option<DeploymentResult>, MmError> {
        unused()
    }

    async fn seal_plan(
        &self,
        scope: RemoteScope,
        seal: ClusterPlanSeal,
    ) -> Result<ClusterPlanSeal, MmError> {
        self.call("seal_plan", &scope);
        Ok(seal)
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
        scope: RemoteScope,
        _commit: String,
    ) -> Result<RemoteAccessPlan, MmError> {
        self.call("plan_team_sync", &scope);
        Ok(RemoteAccessPlan {
            binding: self.binding(&scope, RemotePlanKind::TeamSync),
            access: access(&scope),
        })
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

/// The client's checkout: it builds one bundle per plan, however many
/// nodes there are.
#[derive(Default)]
struct Checkout {
    built: Mutex<Vec<String>>,
    /// A clone that does not hold the commit.
    shallow: bool,
}

#[async_trait]
impl CommitBundles for Checkout {
    async fn bundle(&self, commit: &str) -> Result<PreparedBundle, MmError> {
        locked(&self.built).push(commit.to_owned());
        let bundle = (!self.shallow).then(|| OfferedBundle {
            hash: bundle_hash(),
            bytes: 2048,
            commit_object: String::new(),
        });
        Ok(PreparedBundle {
            offer: BundleOffer {
                commit: commit.to_owned(),
                branches: vec!["main".to_owned()],
                bundle,
            },
            files: 3,
            path: (!self.shallow).then(|| std::path::PathBuf::from("/nonexistent/bundle.tar")),
            keep: None,
        })
    }
}

fn scope(node: &str) -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(uuid::Uuid::from_u128(7)),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: Some(node.to_owned()),
    }
}

fn cluster(checkout: Option<Arc<Checkout>>) -> TestResult<(RemoteEnvironment, Vec<Arc<Node>>)> {
    let fakes = vec![
        Node::new("app-1", "1"),
        Node::new("app-2", "2"),
        Node::new("db-1", "3"),
    ];
    let nodes = fakes
        .iter()
        .enumerate()
        .map(|(index, fake)| RemoteNode {
            name: Some(fake.name.clone()),
            host: format!("203.0.113.{}", index + 1),
            port: 22,
            private_address: None,
            transport: Ok(Arc::clone(fake) as Arc<dyn RemoteTransport>),
            scope: scope(&fake.name),
            placed: None,
        })
        .collect();
    let bundles = checkout.map(|checkout| checkout as Arc<dyn CommitBundles>);
    Ok((
        RemoteEnvironment::new("production".to_owned(), nodes)?.with_pushed_commits(bundles),
        fakes,
    ))
}

#[tokio::test]
async fn every_node_of_a_plan_is_sent_the_bundle_before_it_plans() -> TestResult {
    let checkout = Arc::new(Checkout::default());
    let (environment, nodes) = cluster(Some(Arc::clone(&checkout)))?;
    assert!(environment.pushes());
    // One node deployed this commit before and still holds it.
    nodes[2].holds.store(true, Ordering::SeqCst);
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    for node in &nodes {
        assert_eq!(
            node.calls()[..2],
            ["deliver".to_owned(), "plan_deployment".to_owned()],
            "{} is delivered to, then asked to plan",
            node.name
        );
        let offers = locked(&node.offers);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].0.commit, COMMIT);
        assert_eq!(offers[0].0.branches, ["main"]);
        assert!(offers[0].1, "each node is offered the bundle file");
    }
    assert_eq!(
        locked(&checkout.built).as_slice(),
        [COMMIT],
        "one bundle for the whole plan"
    );
    let bundle = plan.bundle.ok_or("the plan names no bundle")?;
    assert_eq!(bundle.bundle_hash, bundle_hash());
    assert_eq!((bundle.files, bundle.bytes), (3, 2048));
    assert_eq!(bundle.sent_to, ["app-1", "app-2"], "db-1 already held it");
    for section in &plan.nodes {
        assert_eq!(section.plan.binding.bundle_hash, Some(bundle_hash()));
    }
    Ok(())
}

#[tokio::test]
async fn nodes_left_out_of_a_plan_are_sent_nothing() -> TestResult {
    let (environment, nodes) = cluster(Some(Arc::new(Checkout::default())))?;
    let only = vec!["app-1".to_owned()];
    let plan = environment.plan_team_sync(COMMIT, Some(&only)).await?;
    assert_eq!(plan.skipped, ["app-2", "db-1"]);
    assert_eq!(
        nodes[0].calls()[..2],
        ["deliver".to_owned(), "plan_team_sync".to_owned()]
    );
    assert!(nodes[1].calls().is_empty() && nodes[2].calls().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_node_that_planned_from_other_files_stops_the_plan() -> TestResult {
    let (environment, nodes) = cluster(Some(Arc::new(Checkout::default())))?;
    *locked(&nodes[1].plans_from) = Some("d".repeat(64));
    let refused = environment
        .plan_deployment(COMMIT, None, None)
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        refused.contains("'app-2' did not plan commit") && refused.contains("host.yaml"),
        "{refused}"
    );
    for node in &nodes {
        assert!(
            !node.calls().contains(&"seal_plan".to_owned()),
            "nothing is sealed, so nothing can be applied"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_rollback_from_a_clone_without_the_commit_asks_the_nodes_what_they_hold() -> TestResult {
    let checkout = Arc::new(Checkout {
        shallow: true,
        ..Checkout::default()
    });
    let (environment, nodes) = cluster(Some(checkout))?;
    for node in &nodes {
        node.holds.store(true, Ordering::SeqCst);
    }
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    for node in &nodes {
        let offers = locked(&node.offers);
        assert!(offers[0].0.bundle.is_none() && !offers[0].1);
    }
    let bundle = plan.bundle.ok_or("the plan names no bundle")?;
    assert!(bundle.sent_to.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_target_whose_hosts_fetch_is_planned_without_a_delivery() -> TestResult {
    let (environment, nodes) = cluster(None)?;
    assert!(!environment.pushes());
    let plan = environment.plan_deployment(COMMIT, None, None).await?;
    assert!(plan.bundle.is_none());
    for node in &nodes {
        assert!(!node.calls().contains(&"deliver".to_owned()));
    }
    Ok(())
}

// Configuration.

const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";

fn target(yaml: &str) -> TestResult<RepoTargetConfig> {
    Ok(serde_yaml_ng::from_str(yaml)?)
}

#[test]
fn delivery_push_is_the_whole_configuration() -> TestResult {
    // One host.
    let one = target(&format!(
        "host: prod.example.test\nhost-key: {HOST_KEY}\ndaemon-fingerprint: {}\ndelivery: push\n",
        "a".repeat(64)
    ))?;
    assert_eq!(one.delivery, Delivery::Push);
    one.validate("production")?;

    // A cluster, with the setup block of a target that fetches nothing.
    let nodes =
        "nodes:\n  - {name: app-1, host: 203.0.113.11}\n  - {name: app-2, host: 203.0.113.12}\n";
    let cluster = target(&format!(
        "{nodes}delivery: push\nsetup:\n  break-glass:\n    - {{name: ops, key: {HOST_KEY}}}\n"
    ))?;
    assert_eq!(cluster.delivery, Delivery::Push);
    cluster.validate("production")?;
    assert!(serde_yaml_ng::to_string(&cluster)?.contains("delivery: push"));

    // Nothing to write for the default, and nothing is written.
    let fetched = target(&format!(
        "{nodes}setup:\n  origin: https://git.example.test/shop.git\n"
    ))?;
    assert_eq!(fetched.delivery, Delivery::Fetch);
    fetched.validate("production")?;
    assert!(!serde_yaml_ng::to_string(&fetched)?.contains("delivery"));

    // A second way to say where the files come from is refused, by key.
    for (line, key) in [
        ("  origin: https://git.example.test/shop.git\n", "origin"),
        (
            "  deploy-key: {secret: origin.deploy-key, public-key: PLACEHOLDER}\n",
            "deploy-key",
        ),
    ] {
        let line = line.replace("PLACEHOLDER", HOST_KEY);
        let both = target(&format!("{nodes}delivery: push\nsetup:\n{line}"))?;
        let problem = both.validate("production").err().unwrap_or_default();
        assert!(
            problem.contains(&format!("{key} is not used with `delivery: push`")),
            "{problem}"
        );
    }

    // A target that fetches still needs its origin, and is told the other way.
    let neither = target(&format!("{nodes}setup: {{}}\n"))?;
    let problem = neither.validate("production").err().unwrap_or_default();
    assert!(
        problem.contains("origin needs a git URL") && problem.contains("delivery: push"),
        "{problem}"
    );

    assert!(target(&format!("{nodes}delivery: rsync\n")).is_err());
    Ok(())
}

#[test]
fn a_host_policy_has_a_pushed_project_without_an_origin() -> TestResult {
    let policy = |project: &str| -> TestResult<HostPolicy> {
        Ok(serde_yaml_ng::from_str(&format!(
            "schema-version: 1\nbreak-glass:\n  - name: ops\n    key: {HOST_KEY}\nprojects:\n  00000000-0000-0000-0000-000000000004:\n    name: shop\n    environments: [production]\n{project}"
        ))?)
    };
    let pushed = policy("    delivery: push\n")?;
    pushed.validate()?;
    let project = pushed.projects.values().next().ok_or("no project")?;
    assert!(project.delivery.is_push() && project.origin.is_empty());

    let problem = policy("")?.validate().err().unwrap_or_default();
    assert!(
        problem.contains("git origin, or `delivery: push`"),
        "{problem}"
    );

    // Enrolling a project as pushed takes no origin, and switching an
    // enrolled project over drops the origin it had.
    let mut fetched = policy("    origin: /srv/git/shop.git\n")?;
    fetched.validate()?;
    let id = *fetched.projects.keys().next().ok_or("no project")?;
    let mut enrollment = mm::domain::host_policy::ProjectEnrollment {
        project_id: id,
        name: "shop".to_owned(),
        origin: String::new(),
        origin_host_key: None,
        delivery: Delivery::Push,
        environments: std::collections::BTreeSet::from(["production".to_owned()]),
        break_glass: Vec::new(),
    };
    fetched.enroll(&enrollment)?;
    let project = fetched.projects.get(&id).ok_or("no project")?;
    assert!(project.delivery.is_push() && project.origin.is_empty());
    assert!(!serde_yaml_ng::to_string(&fetched)?.contains("origin"));

    enrollment.origin = "https://git.example.test/shop.git".to_owned();
    let problem = fetched.enroll(&enrollment).err().unwrap_or_default();
    assert!(problem.contains("takes no origin"), "{problem}");
    Ok(())
}

#[test]
fn files_named_dotenv_never_travel_and_their_templates_do() {
    use mm::domain::delivery::withheld;

    for path in [".env", "api/.env", ".env.production", "deploy/.env.local"] {
        assert!(withheld(path).is_some(), "{path}");
    }
    for path in [
        ".env.example",
        "api/.env.template",
        "env.template",
        ".envrc",
        "docs/environment.md",
        "src/env.rs",
    ] {
        assert!(withheld(path).is_none(), "{path}");
    }
}
