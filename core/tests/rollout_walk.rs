//! A cluster deployment as the client rolls it out: which node is asked
//! for which step in which order, what happens when a step fails, and who
//! holds the environment meanwhile. The nodes are scripted.

use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use mm::app::ports::{ExecStream, LogStream, RemoteTransport};
use mm::app::{RemoteEnvironment, RemoteNode};
use mm::domain::build::BuildReceipt;
use mm::domain::deployment::DeploymentResult;
use mm::domain::discovery::{Health, NameAddress, NameReport, ResolvedName};
use mm::domain::error::MmError;
use mm::domain::forward::ForwardTotals;
use mm::domain::observability::{ExecRequest, LogRequest, OperationCancellation};
use mm::domain::remote::{
    ClusterPlanSeal, ClusterPlanSection, NodeApplyState, NodeFacts, RemoteAccessPlan,
    RemoteDeploymentPlan, RemoteIdentity, RemotePlanKind, RemoteScope,
};
use mm::domain::rollout::{
    ClusterRollout, DeploymentRecord, NodeDeploymentStatus, RolloutAnswer, RolloutStage,
    RolloutStep, StepFailure,
};
use mm::domain::runtime::RuntimeKind;
use mm::domain::users::AccessChange;
use mm::domain::workspace::{ProjectId, WorkspaceSnapshot};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COMMIT: &str = "cccccccccccccccccccccccccccccccccccccccc";
const NODES: [&str; 3] = ["app-1", "app-2", "db-1"];

/// Every step any node was asked for, in order, as `node:step[:stack]`.
type Steps = Arc<Mutex<Vec<String>>>;

#[derive(Default)]
struct Script {
    /// The `start-first` services do not change on this node.
    unchanged: bool,
    /// The stack that does not become healthy here, and why.
    sick: Option<StepFailure>,
    /// A job that fails here.
    failing_job: Option<String>,
    /// Another deployment holds the environment on this node.
    busy: bool,
    /// The node cannot reach the storage node.
    cut_off_from_storage: bool,
    cannot_roll_back: bool,
    /// The address the node's new container gets.
    address: Option<Ipv4Addr>,
    /// What this node's resolver says, look after look; the last stays.
    names: VecDeque<NameReport>,
    status: NodeDeploymentStatus,
    /// A stack step on this node never answers.
    hang: bool,
    /// Nor does a look at its deployments: its `mmd` is frozen.
    frozen: bool,
    /// A step (by name) that takes this long here, such as a migration or
    /// a stack step that waits for a container's drain.
    slow: Option<(&'static str, Duration)>,
    /// The host sees its listed operations at work: each look at its
    /// deployments finds them a moment further on.
    at_work: bool,
    /// Why the node refuses to begin, before it changes anything: its
    /// secret store does not answer when it plans again.
    store_down: Option<String>,
    /// What the node answers a cancellation with; `None` when it does not
    /// answer.
    cancelled: Option<mm::domain::rollout::CancelOutcome>,
    /// The commits this node was asked to plan.
    planned: Vec<(String, bool)>,
}

struct Node {
    name: String,
    steps: Steps,
    /// How long the client gave each step, as `(step, patience)`.
    patience: Mutex<Vec<(String, Duration)>>,
    seal: Mutex<Option<ClusterPlanSeal>>,
    script: Mutex<Script>,
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn unused<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "the rollout does not make this call".to_owned(),
    })
}

#[async_trait]
impl RemoteTransport for Node {
    async fn rollout(
        &self,
        _scope: RemoteScope,
        _plan_hash: String,
        step: RolloutStep,
        patience: Duration,
    ) -> Result<RolloutAnswer, MmError> {
        locked(&self.patience).push((step.name().to_owned(), patience));
        let stack = match &step {
            RolloutStep::Start { stack } | RolloutStep::Apply { stack } => format!(":{stack}"),
            RolloutStep::Job { stack, job } => format!(":{stack}/{job}"),
            RolloutStep::Ahead { stack } => format!(":{stack}"),
            _ => String::new(),
        };
        locked(&self.steps).push(format!("{}:{}{stack}", self.name, step.name()));
        // A real node takes its time: let the other nodes of a batch work.
        tokio::task::yield_now().await;
        let hangs = locked(&self.script).hang && matches!(step, RolloutStep::Apply { .. });
        if hangs {
            std::future::pending::<()>().await;
        }
        let slow = locked(&self.script)
            .slow
            .filter(|(name, _)| *name == step.name())
            .map(|(_, takes)| takes);
        if let Some(takes) = slow {
            tokio::time::sleep(takes).await;
        }
        let script = locked(&self.script);
        let addresses = script.address.into_iter().collect::<Vec<_>>();
        match step {
            RolloutStep::Hold if script.busy => Err(MmError::Remote {
                code: "MM_DEPLOY_RUNNING",
                message: "bob started deploying plan 999999999999 and is still at it".to_owned(),
            }),
            RolloutStep::Hold => Ok(RolloutAnswer::Held),
            RolloutStep::Networks => Ok(RolloutAnswer::Addressed),
            RolloutStep::Storage if script.cut_off_from_storage => Err(MmError::Remote {
                code: "MM_STORAGE_UNREACHABLE",
                message: "10.90.0.1:2049 did not answer within 3 s".to_owned(),
            }),
            RolloutStep::Storage => Ok(RolloutAnswer::StorageReady {
                created: if self.name == "app-1" {
                    vec!["p/production/app/uploads".to_owned()]
                } else {
                    Vec::new()
                },
            }),
            RolloutStep::Release => Ok(RolloutAnswer::Released),
            RolloutStep::Begin if script.store_down.is_some() => Err(MmError::Remote {
                code: "MM_SECRET_STORE_UNAVAILABLE",
                message: script.store_down.clone().unwrap_or_default(),
            }),
            RolloutStep::Begin => Ok(RolloutAnswer::Begun),
            RolloutStep::Start { stack } => Ok(match &script.sick {
                Some(failure)
                    if failure.stack == stack && failure.reason.contains("new container") =>
                {
                    RolloutAnswer::Failed(failure.clone())
                }
                _ => RolloutAnswer::Started {
                    services: if script.unchanged {
                        Vec::new()
                    } else {
                        vec!["web".to_owned()]
                    },
                    addresses,
                },
            }),
            RolloutStep::Apply { stack } => Ok(match &script.sick {
                Some(failure) if failure.stack == stack => RolloutAnswer::Failed(failure.clone()),
                _ => RolloutAnswer::Applied { addresses },
            }),
            RolloutStep::Job { stack, job } if script.failing_job.as_ref() == Some(&job) => {
                Ok(RolloutAnswer::Failed(StepFailure {
                    stack,
                    service: None,
                    reason: format!("job '{job}' exited with 1"),
                    log: vec!["relation \"orders\" does not exist".to_owned()],
                    code: None,
                }))
            }
            RolloutStep::Job { job, .. } => Ok(RolloutAnswer::JobRan(Box::new(ran(&job)))),
            RolloutStep::Ahead { .. } => Ok(RolloutAnswer::Applied { addresses }),
            RolloutStep::Finish => Ok(RolloutAnswer::Finished(None::<Box<DeploymentResult>>)),
            RolloutStep::Rollback if script.cannot_roll_back => Err(MmError::RuntimeResponse {
                message: "the engine does not answer".to_owned(),
            }),
            RolloutStep::Rollback => Ok(RolloutAnswer::RolledBack),
        }
    }

    async fn names(&self, _scope: RemoteScope) -> Result<NameReport, MmError> {
        locked(&self.steps).push(format!("{}:names", self.name));
        let mut script = locked(&self.script);
        Ok(if script.names.len() > 1 {
            script.names.pop_front().unwrap_or_default()
        } else {
            script.names.front().cloned().unwrap_or_default()
        })
    }

    async fn deployment_status(
        &self,
        _scope: RemoteScope,
    ) -> Result<NodeDeploymentStatus, MmError> {
        let frozen = locked(&self.script).frozen;
        if frozen {
            std::future::pending::<()>().await;
        }
        let mut script = locked(&self.script);
        if script.at_work {
            for operation in &mut script.status.operations {
                operation.alive_at_unix_ms += 2_000;
            }
        }
        Ok(script.status.clone())
    }

    async fn cancel(
        &self,
        _scope: RemoteScope,
        operation: Option<String>,
    ) -> Result<mm::domain::rollout::CancelOutcome, MmError> {
        locked(&self.steps).push(format!(
            "{}:cancel:{}",
            self.name,
            operation.unwrap_or_default()
        ));
        locked(&self.script)
            .cancelled
            .clone()
            .ok_or_else(|| MmError::Unreachable {
                subject: format!("node '{}' (203.0.113.10:22)", self.name),
                problem: "did not answer".to_owned(),
                help: "Check that the host is up".to_owned(),
            })
    }

    async fn sealed_plan(
        &self,
        _scope: RemoteScope,
        hash: String,
    ) -> Result<ClusterPlanSeal, MmError> {
        locked(&self.seal)
            .clone()
            .filter(|seal| seal.cluster_plan_hash == hash)
            .ok_or_else(|| MmError::Security {
                message: format!("holds no reviewed cluster plan {hash}"),
            })
    }

    async fn seal_plan(
        &self,
        _scope: RemoteScope,
        seal: ClusterPlanSeal,
    ) -> Result<ClusterPlanSeal, MmError> {
        *locked(&self.seal) = Some(seal.clone());
        Ok(seal)
    }

    async fn plan_deployment(
        &self,
        scope: RemoteScope,
        commit: String,
        receipt: Option<BuildReceipt>,
        _release: mm::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError> {
        locked(&self.script)
            .planned
            .push((commit.clone(), receipt.is_some()));
        let digit = self.name.chars().last().unwrap_or('1').to_string();
        let record = mm::domain::host_policy::EnvironmentAccessRecord {
            schema_version: 1,
            project_id: scope.project_id,
            environment: scope.environment.clone(),
            commit: commit.clone(),
            users: Default::default(),
            deploy_from: None,
            reconciled_at_unix_ms: 0,
            reconciled_by: "test".to_owned(),
        };
        Ok(RemoteDeploymentPlan {
            binding: mm::domain::remote::RemotePlanBinding {
                protocol_version: mm::domain::remote::PROTOCOL_VERSION,
                daemon_version: "0.1.0".to_owned(),
                daemon_fingerprint: digit.repeat(64),
                target_fingerprint: scope.target_fingerprint.clone(),
                target: scope.target.clone(),
                node: scope.node.clone(),
                kind: RemotePlanKind::Deployment,
                project_id: scope.project_id,
                environment: scope.environment.clone(),
                runtime: scope.runtime,
                commit,
                deployment_plan_hash: "e".repeat(64),
                access_change_hash: "e".repeat(64),
                principal_id: mm::domain::access::PrincipalId(uuid::Uuid::from_u128(1)),
                authorization_revision: 1,
                authorization_hash: "f".repeat(64),
                plan_nonce: uuid::Uuid::from_u128(2),
                expires_at_unix_ms: 1_000,
                bundle_hash: None,
                plan_hash: digit.repeat(64),
            },
            plan: None,
            access: AccessChange::between(None, record),
            receipt,
            unfinished: None,
            once: Default::default(),
            staged: Vec::new(),
            release: None,
        })
    }

    async fn status(&self, _scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        unused()
    }
    async fn logs(
        &self,
        _: RemoteScope,
        _: LogRequest,
        _: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        unused()
    }
    async fn exec(
        &self,
        _: RemoteScope,
        _: ExecRequest,
        _: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        unused()
    }
    async fn apply_deployment(
        &self,
        _: RemoteScope,
        _: String,
    ) -> Result<Option<DeploymentResult>, MmError> {
        unused()
    }
    async fn node(&self, _scope: RemoteScope) -> Result<NodeFacts, MmError> {
        unused()
    }
    async fn plan_team_sync(&self, _: RemoteScope, _: String) -> Result<RemoteAccessPlan, MmError> {
        unused()
    }
    async fn apply_team_sync(&self, _: RemoteScope, _: String) -> Result<AccessChange, MmError> {
        unused()
    }
    async fn whoami(&self, _scope: RemoteScope) -> Result<RemoteIdentity, MmError> {
        unused()
    }
    async fn forward(
        &self,
        _: RemoteScope,
        _: String,
        _: u16,
        _: tokio::net::TcpStream,
    ) -> Result<ForwardTotals, MmError> {
        unused()
    }
}

fn scope(node: Option<&str>) -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(uuid::Uuid::from_u128(4)),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: node.map(str::to_owned),
    }
}

struct Cluster {
    environment: RemoteEnvironment,
    nodes: Vec<Arc<Node>>,
    steps: Steps,
}

fn cluster() -> Result<Cluster, MmError> {
    let steps = Steps::default();
    let nodes = NODES
        .iter()
        .map(|name| {
            Arc::new(Node {
                name: (*name).to_owned(),
                steps: Arc::clone(&steps),
                patience: Mutex::new(Vec::new()),
                seal: Mutex::new(None),
                script: Mutex::new(Script {
                    status: NodeDeploymentStatus {
                        current: Some(finished('0', 1, '0')),
                        ..NodeDeploymentStatus::default()
                    },
                    ..Script::default()
                }),
            })
        })
        .collect::<Vec<_>>();
    let environment = RemoteEnvironment::new(
        "production".to_owned(),
        nodes
            .iter()
            .map(|node| RemoteNode {
                name: Some(node.name.clone()),
                host: "203.0.113.10".to_owned(),
                port: 22,
                repo_address: None,
                private_address: None,
                transport: Ok(Arc::clone(node) as Arc<dyn RemoteTransport>),
                scope: scope(Some(&node.name)),
                placed: None,
            })
            .collect(),
    )?;
    Ok(Cluster {
        environment,
        nodes,
        steps,
    })
}

fn stage(stack: &str, batches: &[&[&str]]) -> RolloutStage {
    RolloutStage {
        stack: stack.to_owned(),
        batches: batches
            .iter()
            .map(|batch| batch.iter().map(|node| (*node).to_owned()).collect())
            .collect(),
        start_first: Vec::new(),
        pause_on_failure: Vec::new(),
        jobs: Vec::new(),
        ahead: Vec::new(),
    }
}

/// A run of `job` that succeeded.
fn ran(job: &str) -> mm::domain::scheduler::JobRun {
    let id = uuid::Uuid::from_u128(5);
    mm::domain::scheduler::JobRun {
        id,
        project_id: ProjectId(uuid::Uuid::from_u128(7)),
        environment: "production".to_owned(),
        job: job.to_owned(),
        schedule: None,
        trigger: mm::domain::scheduler::JobTrigger::BeforeDeploy,
        state: mm::domain::scheduler::JobRunState::Succeeded,
        attempt: 1,
        maximum_attempts: 1,
        fence_token: 1,
        runtime_name: mm::domain::scheduler::runtime_job_name(
            ProjectId(uuid::Uuid::from_u128(7)),
            id,
        ),
        runtime_id: None,
        requested_at_unix_ms: 1,
        scheduled_for_unix_ms: None,
        started_at_unix_ms: None,
        finished_at_unix_ms: None,
        next_attempt_at_unix_ms: None,
        exit_code: Some(0),
        error_code: None,
        output: String::new(),
        output_truncated: false,
        cancellation_requested: false,
        principal: "deployment".to_owned(),
        source: "deployment 111111111111".to_owned(),
        node: None,
    }
}

/// `stage`, with a job that runs on `node` before the stage's nodes.
fn stage_with_job(stack: &str, batches: &[&[&str]], job: &str, node: &str) -> RolloutStage {
    RolloutStage {
        jobs: vec![mm::domain::rollout::StageJob {
            job: job.to_owned(),
            node: node.to_owned(),
            timeout_seconds: 60,
        }],
        ..stage(stack, batches)
    }
}

impl Cluster {
    /// Has every node hold a reviewed plan that rolls out in `stages`.
    fn reviewed(&self, stages: Vec<RolloutStage>) -> Result<String, String> {
        self.reviewed_with(stages, None)
    }

    /// The same for a plan that shares volumes.
    fn reviewed_with(
        &self,
        stages: Vec<RolloutStage>,
        storage: Option<mm::domain::shared_storage::SealedStorage>,
    ) -> Result<String, String> {
        let seal = ClusterPlanSeal::new(
            RemotePlanKind::Deployment,
            &scope(None),
            COMMIT,
            NODES
                .iter()
                .enumerate()
                .map(|(index, node)| ClusterPlanSection {
                    node: (*node).to_owned(),
                    plan_hash: (index + 1).to_string().repeat(64),
                })
                .collect(),
            Vec::new(),
            mm::domain::remote::SealCarried {
                networks: None,
                rollout: Some(ClusterRollout {
                    convergence_timeout_seconds: 60,
                    pull_timeout_seconds: None,
                    stages,
                    ran_once: Default::default(),
                    once: Vec::new(),
                }),
                storage,
                ingress: None,
            },
        )?;
        for node in &self.nodes {
            *locked(&node.seal) = Some(seal.clone());
        }
        Ok(seal.cluster_plan_hash)
    }

    /// The steps that lock a node or change what it runs, without the
    /// lookups between and without the addresses every node takes first.
    fn steps(&self) -> Vec<String> {
        locked(&self.steps)
            .iter()
            .filter(|step| !step.ends_with(":names") && !step.ends_with(":networks"))
            .cloned()
            .collect()
    }

    fn steps_of(&self, node: &str) -> Vec<String> {
        self.steps()
            .into_iter()
            .filter_map(|step| step.strip_prefix(&format!("{node}:")).map(str::to_owned))
            .collect()
    }
}

fn sick(stack: &str, service: &str) -> StepFailure {
    StepFailure {
        stack: stack.to_owned(),
        service: Some(service.to_owned()),
        reason: "its health check fails".to_owned(),
        log: vec!["connection refused".to_owned()],
        code: None,
    }
}

fn states(
    report: &mm::domain::remote::ClusterApplyReport<DeploymentResult>,
) -> Vec<(&str, NodeApplyState)> {
    report
        .nodes
        .iter()
        .map(|node| (node.node.as_str(), node.state))
        .collect()
}

#[tokio::test]
async fn stacks_roll_in_the_plans_order_and_each_stack_one_node_at_a_time() -> TestResult {
    let cluster = cluster()?;
    let hash = cluster.reviewed(vec![
        stage("db", &[&["db-1"]]),
        stage("app", &[&["app-1"], &["app-2"]]),
    ])?;
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| locked(&said).push(line.to_string()))
        .await?;
    assert!(report.succeeded());
    let steps = cluster.steps();
    assert_eq!(
        steps[..3]
            .iter()
            .map(|step| step.split(':').nth(1))
            .collect::<Vec<_>>(),
        [Some("hold"); 3],
        "every node of the plan is locked before any is changed"
    );
    // Then every node takes the plan's network addresses, before the first
    // one is replaced.
    let all = locked(&cluster.steps).clone();
    let first_begin = all.iter().position(|step| step.ends_with(":begin"));
    assert_eq!(
        all.iter()
            .take(first_begin.unwrap_or_default())
            .filter(|step| step.ends_with(":networks"))
            .count(),
        3
    );
    assert_eq!(
        steps[3..],
        [
            "db-1:begin",
            "db-1:apply:db",
            "app-1:begin",
            "app-1:apply:app",
            "app-2:begin",
            "app-2:apply:app",
            "app-1:finish",
            "app-2:finish",
            "db-1:finish",
        ],
        "a node begins when its first stack is due, and nothing is final before every stack is healthy"
    );
    assert_eq!(
        locked(&said).first().cloned().unwrap_or_default(),
        format!(
            "operation {short}: deploying to 'production'. `mm deploy status` shows where it is; `mm deploy cancel {short}` stops it",
            short = &hash[..12]
        ),
        "the operation's ID comes first"
    );
    assert_eq!(
        locked(&said)[1..],
        [
            "db-1: preparing",
            "db-1: db",
            "db-1: db: healthy",
            "app-1: preparing",
            "app-1: app",
            "app-1: app: healthy",
            "app-2: preparing",
            "app-2: app",
            "app-2: app: healthy",
            "app-1: finishing",
            "app-2: finishing",
            "db-1: finishing",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_failure_on_the_second_node_rolls_both_back_and_never_touches_the_third() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[1].script).sick = Some(sick("app", "web"));
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"], &["db-1"]])])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(
        states(&report),
        [
            ("app-1", NodeApplyState::RolledBack),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::NotStarted),
        ]
    );
    assert_eq!(
        cluster.steps_of("app-1"),
        ["hold", "begin", "apply:app", "rollback"]
    );
    assert_eq!(
        cluster.steps_of("app-2"),
        ["hold", "begin", "apply:app", "rollback"]
    );
    assert_eq!(
        cluster.steps_of("db-1"),
        ["hold", "release"],
        "the third node only held the lock"
    );
    let steps = cluster.steps();
    let position = |step: &str| steps.iter().position(|taken| taken == step);
    assert!(
        position("app-2:rollback") < position("app-1:rollback"),
        "the newest change is undone first"
    );

    let failed = &report.nodes[1];
    assert!(failed.rolled_back);
    assert_eq!(
        failed.error.as_deref(),
        Some("service 'web' of stack 'app': its health check fails")
    );
    assert_eq!(failed.log, ["connection refused"]);
    let failure = report.failure("mm deploy plan").unwrap_or_default();
    for expected in [
        "failed on node 'app-2': service 'web' of stack 'app': its health check fails",
        "Its last log lines: connection refused",
        "Rolled back to what they ran before: app-1, app-2.",
        "Not changed: db-1.",
    ] {
        assert!(failure.contains(expected), "{expected}: {failure}");
    }
    Ok(())
}

/// The patience the client gave `node` for each of `steps`, in order.
async fn patience_after_failure(
    compose_wait: Duration,
) -> Result<Vec<Duration>, Box<dyn std::error::Error>> {
    let mut cluster = cluster()?;
    cluster.environment = cluster.environment.with_compose_wait(compose_wait);
    locked(&cluster.nodes[0].script).sick = Some(sick("app", "web"));
    let hash = cluster.reviewed(vec![stage("db", &[&["db-1"]]), stage("app", &[&["app-1"]])])?;
    cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    Ok(locked(&cluster.nodes[0].patience)
        .iter()
        .filter(|(step, _)| step == "apply" || step == "rollback")
        .map(|(_, patience)| *patience)
        .collect())
}

#[tokio::test]
async fn a_stack_step_and_a_rollback_wait_as_long_as_compose_may_take() -> TestResult {
    let without = patience_after_failure(Duration::ZERO).await?;
    let with = patience_after_failure(Duration::from_secs(600)).await?;
    assert_eq!(without.len(), 2, "{without:?}");
    assert_eq!(
        with[0] - without[0],
        Duration::from_secs(600),
        "one stack step"
    );
    assert_eq!(
        with[1] - without[1],
        Duration::from_secs(2 * 600),
        "a rollback of two stages"
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_first_deployment_leaves_every_node_as_it_is_and_says_what_to_run() -> TestResult {
    let cluster = cluster()?;
    for node in &cluster.nodes {
        locked(&node.script).status = NodeDeploymentStatus::default();
    }
    locked(&cluster.nodes[1].script).sick = Some(sick("app", "web"));
    let hash = cluster.reviewed(vec![
        stage("db", &[&["db-1"]]),
        stage("app", &[&["app-1"], &["app-2"]]),
    ])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(
        states(&report),
        [
            ("app-1", NodeApplyState::Unfinished),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::Unfinished),
        ]
    );
    for node in ["app-1", "app-2", "db-1"] {
        let steps = cluster.steps_of(node);
        assert_eq!(
            steps.last().map(String::as_str),
            Some("release"),
            "{node}: {steps:?}"
        );
        assert!(
            !steps.iter().any(|step| step == "rollback"),
            "{node}: {steps:?}"
        );
    }
    let failed = &report.nodes[1];
    assert!(!failed.rolled_back);
    let error = failed.error.as_deref().unwrap_or_default();
    for expected in [
        "service 'web' of stack 'app': its health check fails",
        "nothing was rolled back",
        "`mm service logs web --environment production`",
        "`mm deploy plan --environment production`",
    ] {
        assert!(error.contains(expected), "{expected}: {error}");
    }
    assert!(failed.kept);
    let failure = report.failure("mm deploy plan").unwrap_or_default();
    assert!(
        failure.ends_with("fix the cause, then `mm deploy plan` deploys over what they run"),
        "{failure}"
    );
    assert!(!failure.contains("mm deploy rollback"), "{failure}");
    Ok(())
}

#[tokio::test]
async fn a_new_node_of_a_serving_environment_is_still_rolled_back() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[1].script).status = NodeDeploymentStatus::default();
    locked(&cluster.nodes[1].script).sick = Some(sick("app", "web"));
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"]])])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(
        states(&report),
        [
            ("app-1", NodeApplyState::RolledBack),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::NotStarted),
        ]
    );
    assert!(report.nodes[1].rolled_back);
    Ok(())
}

#[tokio::test]
async fn a_service_that_pauses_on_failure_leaves_every_node_as_it_is() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[1].script).sick = Some(sick("app", "worker"));
    let mut app = stage("app", &[&["app-1"], &["app-2"], &["db-1"]]);
    app.pause_on_failure = vec!["worker".to_owned()];
    let hash = cluster.reviewed(vec![app])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(
        states(&report),
        [
            ("app-1", NodeApplyState::Unfinished),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::NotStarted),
        ]
    );
    assert!(!report.nodes[1].rolled_back);
    assert!(
        !cluster
            .steps()
            .iter()
            .any(|step| step.ends_with(":rollback")),
        "nothing is undone"
    );
    for node in NODES {
        assert_eq!(
            cluster.steps_of(node).last().map(String::as_str),
            Some("release"),
            "{node} gives up the lock"
        );
    }
    let failure = report.failure("mm deploy plan").unwrap_or_default();
    assert!(
        failure.contains("Changed and left as they are: app-1, app-2."),
        "{failure}"
    );
    assert!(failure.contains("`mm deploy rollback`"), "{failure}");
    Ok(())
}

fn view(network: &str, service: &str, addresses: &[(&str, Ipv4Addr)]) -> NameReport {
    NameReport {
        networks: vec![network.to_owned()],
        names: vec![ResolvedName {
            name: service.to_owned(),
            network: network.to_owned(),
            addresses: addresses
                .iter()
                .map(|(node, address)| NameAddress {
                    address: *address,
                    node: (*node).to_owned(),
                    health: Health::Healthy,
                    unknown_to: Vec::new(),
                    restart_limit: false,
                })
                .collect(),
            lookups: Vec::new(),
            fail_open: false,
        }],
        problems: Vec::new(),
    }
}

#[tokio::test]
async fn start_first_waits_for_health_and_for_every_node_to_resolve_the_new_container() -> TestResult
{
    let cluster = cluster()?;
    let (old, new) = (Ipv4Addr::new(10, 64, 0, 2), Ipv4Addr::new(10, 64, 0, 3));
    let both = view("backend", "web", &[("app-1", old), ("app-1", new)]);
    {
        let mut first = locked(&cluster.nodes[0].script);
        first.address = Some(new);
        first.names = VecDeque::from([both.clone()]);
    }
    // app-2 learns of the new container one look later than app-1.
    locked(&cluster.nodes[1].script).names =
        VecDeque::from([view("backend", "web", &[("app-1", old)]), both.clone()]);
    // db-1 holds no part of the network, so nobody waits for it.
    let mut app = stage("app", &[&["app-1"]]);
    app.start_first = vec!["web".to_owned()];
    let hash = cluster.reviewed(vec![app])?;
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| locked(&said).push(line.to_string()))
        .await?;
    assert!(report.succeeded(), "{:?}", report.failure("mm deploy plan"));
    assert_eq!(
        locked(&said)[1..4],
        [
            "app-1: preparing",
            "app-1: app: started the new web next to the old",
            "app-1: app",
        ]
    );
    let steps = locked(&cluster.steps).clone();
    let position = |step: &str| steps.iter().position(|taken| taken == step);
    let lookups_before_apply = steps
        .iter()
        .take(position("app-1:apply:app").unwrap_or_default())
        .filter(|step| *step == "app-2:names")
        .count();
    assert!(position("app-1:start:app") < position("app-1:apply:app"));
    assert_eq!(
        lookups_before_apply, 2,
        "the old container is not stopped before app-2 resolves the new one"
    );
    Ok(())
}

#[tokio::test]
async fn a_new_container_that_is_not_healthy_leaves_the_old_one_running() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[0].script).sick = Some(StepFailure {
        reason: "the new container's health check fails".to_owned(),
        ..sick("app", "web")
    });
    let mut app = stage("app", &[&["app-1"], &["app-2"]]);
    app.start_first = vec!["web".to_owned()];
    let hash = cluster.reviewed(vec![app])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(
        cluster.steps_of("app-1"),
        ["hold", "begin", "start:app", "rollback"]
    );
    assert_eq!(cluster.steps_of("app-2"), ["hold", "release"]);
    assert_eq!(report.nodes[0].state, NodeApplyState::Failed);
    Ok(())
}

#[tokio::test]
async fn nodes_of_a_batch_are_replaced_together_and_the_next_batch_waits() -> TestResult {
    let cluster = cluster()?;
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1", "app-2"], &["db-1"]])])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert!(report.succeeded());
    let steps = cluster.steps();
    let position = |step: &str| steps.iter().position(|taken| taken == step);
    assert!(position("app-1:apply:app") < position("db-1:begin"));
    assert!(position("app-2:apply:app") < position("db-1:begin"));
    assert!(
        position("app-2:begin") < position("app-1:apply:app"),
        "both nodes of the batch work at the same time"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_deployment_is_refused_and_changes_nothing() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[1].script).busy = true;
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"]])])?;
    let error = cluster
        .environment
        .apply_deployment(&hash, &|_| {})
        .await
        .err()
        .ok_or("the apply went through")?;
    assert_eq!(error.code(), "MM_DEPLOY_RUNNING");
    let message = error.to_string();
    assert!(message.contains("node 'app-2'"), "{message}");
    assert!(message.contains("bob started deploying plan"), "{message}");
    assert_eq!(cluster.steps_of("app-1"), ["hold", "release"]);
    assert_eq!(cluster.steps_of("app-2"), ["hold"]);
    assert_eq!(cluster.steps_of("db-1"), ["hold", "release"]);

    // A node the plan leaves out must not be deploying either.
    let cluster = self::cluster()?;
    locked(&cluster.nodes[2].script).status.rollout = Some(mm::domain::rollout::NodeRollout {
        plan_hash: "9".repeat(64),
        commit: COMMIT.to_owned(),
        started_by: "bob".to_owned(),
        started_at_unix_ms: 1,
        heard_at_unix_ms: 2,
        active: true,
        state: mm::domain::rollout::RolloutState::Running,
        stacks: Vec::new(),
        failure: None,
    });
    let seal = ClusterPlanSeal::new(
        RemotePlanKind::Deployment,
        &scope(None),
        COMMIT,
        vec![
            ClusterPlanSection {
                node: "app-1".to_owned(),
                plan_hash: "1".repeat(64),
            },
            ClusterPlanSection {
                node: "app-2".to_owned(),
                plan_hash: "2".repeat(64),
            },
        ],
        vec!["db-1".to_owned()],
        (None, Some(ClusterRollout::default())),
    )?;
    for node in &cluster.nodes[..2] {
        *locked(&node.seal) = Some(seal.clone());
    }
    let error = cluster
        .environment
        .apply_deployment(&seal.cluster_plan_hash, &|_| {})
        .await
        .err()
        .ok_or("the apply went through")?;
    assert_eq!(error.code(), "MM_DEPLOY_RUNNING");
    assert!(
        error
            .to_string()
            .contains("node 'db-1', which this plan leaves out"),
        "{error}"
    );
    assert!(!cluster.steps().iter().any(|step| step.ends_with(":begin")));
    Ok(())
}

#[tokio::test]
async fn a_node_that_cannot_be_rolled_back_is_reported_as_left_changed() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[0].script).cannot_roll_back = true;
    locked(&cluster.nodes[1].script).sick = Some(sick("app", "web"));
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"]])])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(report.nodes[0].state, NodeApplyState::Unfinished);
    let stuck = report.nodes[0].error.clone().unwrap_or_default();
    assert!(
        stuck.contains("could not be rolled back: ")
            && stuck.contains("the engine does not answer"),
        "{stuck}"
    );
    assert_eq!(report.nodes[1].state, NodeApplyState::Failed);
    assert!(report.nodes[1].rolled_back);
    let failure = report.failure("mm deploy plan").unwrap_or_default();
    assert!(
        failure.contains("Changed and left as they are: app-1."),
        "{failure}"
    );
    assert!(
        failure.contains("Rolled back to what they ran before: app-2."),
        "{failure}"
    );
    Ok(())
}

fn finished(commit: char, at: i64, plan: char) -> DeploymentRecord {
    DeploymentRecord {
        commit: commit.to_string().repeat(40),
        deployed_at_unix_ms: at,
        deployed_by: "alice".to_owned(),
        runs_services: true,
        plan_hash: Some(plan.to_string().repeat(64)),
        receipt: None,
        release: None,
    }
}

#[tokio::test]
async fn a_rollback_plans_the_earlier_commit_on_the_nodes_of_the_last_deployment() -> TestResult {
    let cluster = cluster()?;
    // Everything ran a; the last deployment put b on app-1 and app-2.
    for (index, node) in cluster.nodes.iter().enumerate() {
        let mut script = locked(&node.script);
        script.status.current = Some(if index < 2 {
            finished('b', 20, '2')
        } else {
            finished('a', 10, '1')
        });
        script.status.previous = (index < 2).then(|| finished('a', 10, '1'));
    }
    let status = cluster.environment.deployment_status().await;
    assert_eq!(status.nodes.len(), 3);
    assert!(
        status
            .nodes
            .iter()
            .all(|row| row.status.is_some() && row.problem.is_none())
    );

    let (target, plan) = cluster.environment.plan_rollback(None).await?;
    assert_eq!(target.commit, "a".repeat(40));
    assert_eq!(target.nodes, ["app-1", "app-2"]);
    assert_eq!(plan.commit, "a".repeat(40));
    assert_eq!(
        plan.skipped,
        ["db-1"],
        "the node the last deployment left alone keeps what it runs"
    );
    assert!(
        plan.rollout.is_some(),
        "an ordinary plan: sealed, with its rollout order"
    );
    assert_eq!(
        locked(&cluster.nodes[0].script).planned,
        [("a".repeat(40), false)]
    );
    assert!(locked(&cluster.nodes[2].script).planned.is_empty());
    assert!(
        locked(&cluster.nodes[0].seal)
            .as_ref()
            .is_some_and(|seal| seal.cluster_plan_hash == plan.plan_hash),
        "applied with its hash like any other plan"
    );

    // A node name that is not one of the target's is refused before
    // anything is asked.
    let unknown = ["app-9".to_owned()];
    let error = cluster
        .environment
        .plan_rollback(Some(&unknown))
        .await
        .err()
        .ok_or("accepted")?;
    assert!(
        error.to_string().contains("'app-9', which is not a node"),
        "{error}"
    );
    Ok(())
}

fn shared() -> mm::domain::shared_storage::SealedStorage {
    mm::domain::shared_storage::SealedStorage {
        node: "app-1".to_owned(),
        directories: vec!["p/production/app/uploads".to_owned()],
        mounting: vec!["app-2".to_owned()],
    }
}

#[tokio::test]
async fn shared_volumes_are_ready_on_the_storage_node_before_any_node_is_replaced() -> TestResult {
    let cluster = cluster()?;
    let hash = cluster.reviewed_with(
        vec![stage("app", &[&["app-1"], &["app-2"]])],
        Some(shared()),
    )?;
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| locked(&said).push(line.to_string()))
        .await?;
    assert!(report.succeeded());
    let all = locked(&cluster.steps).clone();
    let position = |step: &str| {
        all.iter()
            .position(|done| done == step)
            .ok_or_else(|| format!("no step {step} in {all:?}"))
    };
    let storage_node = position("app-1:storage")?;
    assert!(
        all.iter()
            .rposition(|step| step.ends_with(":networks"))
            .is_some_and(|networks| networks < position("app-2:storage").unwrap_or_default()),
        "{all:?}"
    );
    for other in ["app-2:storage", "db-1:storage"] {
        assert!(
            position(other)? < storage_node,
            "the nodes that mount check that they reach storage before the storage node creates anything: {all:?}"
        );
    }
    assert!(
        storage_node < position("app-1:begin")?,
        "the directories are there before any container changes: {all:?}"
    );
    assert!(
        locked(&said)
            .iter()
            .any(|line| line == "app-1: created p/production/app/uploads"),
        "{:?}",
        locked(&said)
    );
    assert_eq!(
        all.iter().filter(|step| step.ends_with(":storage")).count(),
        3,
        "once per node: no later step and no rollback touches a volume"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_that_cannot_reach_storage_stops_the_rollout_before_it_begins() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[1].script).cut_off_from_storage = true;
    let hash = cluster.reviewed_with(
        vec![stage("app", &[&["app-1"], &["app-2"]])],
        Some(shared()),
    )?;
    let Err(error) = cluster.environment.apply_deployment(&hash, &|_| {}).await else {
        return Err("a node without storage refuses the rollout".into());
    };
    let text = error.to_string();
    assert!(
        text.contains("app-2") && text.contains("did not answer"),
        "{text}"
    );
    let steps = cluster.steps();
    assert!(
        !steps.iter().any(|step| step.ends_with(":begin")),
        "no node was changed: {steps:?}"
    );
    assert!(
        !steps.iter().any(|step| step == "app-1:storage"),
        "the storage node was not asked to create anything: {steps:?}"
    );
    for node in NODES {
        assert_eq!(
            cluster.steps_of(node).last().map(String::as_str),
            Some("release"),
            "{node} gives up the lock"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_stage_job_runs_on_its_node_before_any_node_of_the_stage_is_replaced() -> TestResult {
    let cluster = cluster()?;
    let hash = cluster.reviewed(vec![
        stage("data", &[&["db-1"]]),
        stage_with_job("app", &[&["app-1"], &["app-2"]], "migrate", "app-2"),
    ])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert!(
        states(&report)
            .iter()
            .all(|(_, state)| *state == NodeApplyState::Applied)
    );
    let steps = cluster.steps();
    let position = |step: &str| steps.iter().position(|taken| taken == step);
    assert!(
        position("db-1:apply:data") < position("app-2:job:app/migrate"),
        "the job runs after the stacks before its own: {steps:?}"
    );
    assert!(
        position("app-2:job:app/migrate") < position("app-1:apply:app"),
        "the job runs before any node of its stage is replaced: {steps:?}"
    );
    assert_eq!(
        cluster.steps_of("app-2"),
        ["hold", "begin", "job:app/migrate", "apply:app", "finish"],
        "the job's node begins for the job and does not begin twice"
    );
    Ok(())
}

#[tokio::test]
async fn a_job_that_fails_stops_the_deployment_before_its_stack_rolls() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[1].script).failing_job = Some("migrate".to_owned());
    let hash = cluster.reviewed(vec![
        stage("data", &[&["db-1"]]),
        stage_with_job("app", &[&["app-1"], &["app-2"]], "migrate", "app-2"),
    ])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(
        states(&report),
        [
            ("app-1", NodeApplyState::NotStarted),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::RolledBack),
        ]
    );
    assert!(
        !cluster
            .steps()
            .iter()
            .any(|step| step.ends_with("apply:app")),
        "no node of the job's stage was replaced: {:?}",
        cluster.steps()
    );
    let failure = report.failure("mm deploy plan").unwrap_or_default();
    for expected in [
        "job 'migrate' exited with 1",
        "relation \"orders\" does not exist",
    ] {
        assert!(failure.contains(expected), "{expected}: {failure}");
    }
    Ok(())
}

#[tokio::test]
async fn what_a_job_needs_starts_on_its_nodes_before_the_job_and_the_rest_after() -> TestResult {
    let cluster = cluster()?;
    let mut app = stage_with_job(
        "app",
        &[&["app-1"], &["app-2"], &["db-1"]],
        "migrate",
        "app-2",
    );
    app.ahead = vec!["db-1".to_owned()];
    let hash = cluster.reviewed(vec![app])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert!(
        states(&report)
            .iter()
            .all(|(_, state)| *state == NodeApplyState::Applied)
    );
    let steps = cluster.steps();
    let position = |step: &str| steps.iter().position(|taken| taken == step);
    assert!(
        position("db-1:ahead:app") < position("app-2:job:app/migrate"),
        "db starts before the migration: {steps:?}"
    );
    assert!(
        position("app-2:job:app/migrate") < position("app-1:apply:app"),
        "the stack follows the migration: {steps:?}"
    );
    Ok(())
}

/// A host whose `mmd` is frozen answers neither its step nor the looks at
/// its deployments. On the VMs (10a.2, SIGSTOP) the client then printed
/// nothing for 30 s: each line waited for its look, whose own patience is a
/// heartbeat. The line now comes every heartbeat (plus a second's grace),
/// and the host is given up after `deploy.progress-timeout` as before.
#[tokio::test(start_paused = true)]
async fn a_frozen_host_still_gets_a_line_every_heartbeat() -> TestResult {
    let mut cluster = cluster()?;
    cluster.environment = cluster
        .environment
        .clone()
        .with_progress_timeout(Duration::from_secs(60));
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"]])])?;
    {
        let mut script = locked(&cluster.nodes[1].script);
        script.hang = true;
        script.frozen = true;
    }
    let started = tokio::time::Instant::now();
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| {
            locked(&said).push((started.elapsed(), line.to_string()));
        })
        .await?;
    let said = locked(&said)
        .iter()
        .filter(|(_, line)| line.starts_with("app-2: app"))
        .cloned()
        .collect::<Vec<_>>();
    assert!(said.len() >= 4, "{said:?}");
    for pair in said.windows(2) {
        assert!(
            pair[1].0 - pair[0].0 <= Duration::from_secs(16),
            "more than 16 s without a line: {said:?}"
        );
    }
    assert!(
        said.iter().any(|(_, line)| line.ends_with(
            "still running after 16s (the host did not say how it is doing: no answer yet)"
        )),
        "{said:?}"
    );
    assert_eq!(report.nodes[1].code.as_deref(), Some("MM_TIMEOUT"));
    let error = report.nodes[1].error.clone().unwrap_or_default();
    assert!(error.contains("its mmd did not answer"), "{error}");
    Ok(())
}

/// A host whose step stops showing progress is given up after
/// `deploy.progress-timeout` with a timeout that names the node, the step,
/// and the operation; a line says how the step is doing every 15 seconds
/// meanwhile, and the nodes that changed are rolled back.
#[tokio::test(start_paused = true)]
async fn a_step_that_shows_no_progress_ends_the_wait_with_a_timeout() -> TestResult {
    let mut cluster = cluster()?;
    cluster.environment = cluster
        .environment
        .clone()
        .with_progress_timeout(Duration::from_secs(30));
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"]])])?;
    {
        let mut script = locked(&cluster.nodes[1].script);
        script.hang = true;
        // The host lists the step, and never sees it at work again.
        script.status.operations = vec![mm::domain::rollout::OperationInFlight {
            id: hash.clone(),
            what: "deployment (stack 'app')".to_owned(),
            principal: "alice".to_owned(),
            started_at_unix_ms: 1,
            alive_at_unix_ms: 1,
            deadline_unix_ms: i64::MAX,
            stopping: None,
            doing: None,
        }];
    }
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| locked(&said).push(line.to_string()))
        .await?;
    let said = locked(&said).clone();
    assert!(
        said.contains(
            &"app-2: app, still running after 15s (the host is working on it)".to_owned()
        ),
        "{said:?}"
    );
    assert!(
        said.contains(
            &"app-2: app, still running after 30s (the host has not seen it at work since the last look)"
                .to_owned()
        ),
        "{said:?}"
    );
    assert_eq!(
        states(&report),
        [
            ("app-1", NodeApplyState::RolledBack),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::NotStarted),
        ]
    );
    let failed = &report.nodes[1];
    assert_eq!(failed.code.as_deref(), Some("MM_TIMEOUT"));
    let error = failed.error.clone().unwrap_or_default();
    assert!(
        error.contains(&format!(
            "node 'app-2' showed no progress on step 'app' of operation {} for 30s (deploy.progress-timeout)",
            &hash[..12]
        )),
        "{error}"
    );
    assert!(error.contains("mm deploy status"), "{error}");
    let stopped = report
        .stopped("mm deploy plan")
        .ok_or("the apply did not stop")?;
    assert_eq!(stopped.code(), "MM_TIMEOUT");
    Ok(())
}

#[tokio::test]
async fn cancel_asks_every_node_and_names_the_one_that_does_not_answer() -> TestResult {
    let cluster = cluster()?;
    let hash = "9".repeat(64);
    locked(&cluster.nodes[0].script).cancelled = Some(mm::domain::rollout::CancelOutcome {
        rolled_back: Some(hash.clone()),
        ..Default::default()
    });
    locked(&cluster.nodes[1].script).cancelled =
        Some(mm::domain::rollout::CancelOutcome::default());
    let (report, unanswered) = cluster.environment.cancel(Some(&hash[..12])).await;
    assert_eq!(
        cluster.steps(),
        [
            format!("app-1:cancel:{}", &hash[..12]),
            format!("app-2:cancel:{}", &hash[..12]),
            format!("db-1:cancel:{}", &hash[..12]),
        ]
    );
    assert!(!report.found_nothing());
    assert_eq!(
        report.nodes[0]
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.rolled_back.clone()),
        Some(hash)
    );
    assert!(
        report.nodes[2]
            .problem
            .as_deref()
            .is_some_and(|problem| problem.contains("did not answer"))
    );
    assert_eq!(unanswered.map(|error| error.code()), Some("MM_UNREACHABLE"));
    Ok(())
}

/// The step a host lists as its operation in flight, as `mmd` lists it.
fn in_flight(hash: &str, what: &str) -> mm::domain::rollout::OperationInFlight {
    mm::domain::rollout::OperationInFlight {
        id: hash.to_owned(),
        what: what.to_owned(),
        principal: "alice".to_owned(),
        started_at_unix_ms: 1,
        alive_at_unix_ms: 1,
        deadline_unix_ms: i64::MAX,
        stopping: None,
        doing: None,
    }
}

/// A `before-deploy` migration that runs ten minutes while its host sees
/// it at work outlasts `deploy.progress-timeout` (two minutes) and is not
/// given up; the client says every 15 seconds how it is doing.
#[tokio::test(start_paused = true)]
async fn a_long_job_its_host_sees_at_work_is_not_given_up() -> TestResult {
    let cluster = cluster()?;
    let hash = cluster.reviewed(vec![
        stage("data", &[&["db-1"]]),
        stage_with_job("app", &[&["app-1"], &["app-2"]], "migrate", "app-2"),
    ])?;
    {
        let mut script = locked(&cluster.nodes[1].script);
        script.slow = Some(("job", Duration::from_secs(10 * 60)));
        script.status.operations = vec![in_flight(&hash, "deployment (job 'migrate')")];
        script.at_work = true;
    }
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| locked(&said).push(line.to_string()))
        .await?;
    assert!(
        states(&report)
            .iter()
            .all(|(_, state)| *state == NodeApplyState::Applied),
        "{:?}",
        report.failure("mm deploy plan")
    );
    let said = locked(&said).clone();
    assert!(
        said.contains(
            &"app-2: app: job migrate, still running after 9m45s (the host is working on it)"
                .to_owned()
        ),
        "{said:?}"
    );
    Ok(())
}

/// A stack step that waits for a container's drain (`drain` up to 15m, 13)
/// while its host sees it at work outlasts `deploy.progress-timeout` and
/// finishes.
#[tokio::test(start_paused = true)]
async fn a_stack_step_that_drains_for_minutes_is_not_given_up() -> TestResult {
    let cluster = cluster()?;
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"]])])?;
    {
        let mut script = locked(&cluster.nodes[0].script);
        script.slow = Some(("apply", Duration::from_secs(16 * 60)));
        script.status.operations = vec![in_flight(&hash, "deployment (stack 'app')")];
        script.at_work = true;
    }
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| locked(&said).push(line.to_string()))
        .await?;
    assert!(
        states(&report)
            .iter()
            .all(|(_, state)| *state == NodeApplyState::Applied),
        "{:?}",
        report.failure("mm deploy plan")
    );
    let said = locked(&said).clone();
    assert!(
        said.contains(
            &"app-1: app, still running after 15m45s (the host is working on it)".to_owned()
        ),
        "{said:?}"
    );
    Ok(())
}

/// A node whose secret store does not answer when the apply plans again
/// refuses before it changes anything (18): the report lists it as not
/// changed, with why, and never as rolled back.
#[tokio::test]
async fn a_node_refused_by_its_secret_store_is_listed_as_not_changed() -> TestResult {
    let cluster = cluster()?;
    let refusal = mm::app::refused_at_apply(MmError::Remote {
        code: "MM_SECRET_STORE_UNAVAILABLE",
        message: "secret store 'team-secrets' did not answer, so environment 'production' has no version of its secrets to plan from; nothing was planned. Then plan again.".to_owned(),
    });
    locked(&cluster.nodes[1].script).store_down = Some(refusal.to_string());
    let hash = cluster.reviewed(vec![stage("app", &[&["app-1"], &["app-2"]])])?;
    let report = cluster.environment.apply_deployment(&hash, &|_| {}).await?;
    assert_eq!(
        states(&report),
        [
            ("app-1", NodeApplyState::RolledBack),
            ("app-2", NodeApplyState::Failed),
            ("db-1", NodeApplyState::NotStarted),
        ]
    );
    assert!(report.nodes[1].unchanged, "app-2 changed nothing");
    let failure = report.failure("mm deploy plan").unwrap_or_default();
    assert!(failure.contains("Not changed: app-2, db-1"), "{failure}");
    assert!(
        failure.contains("Rolled back to what they ran before: app-1."),
        "{failure}"
    );
    assert!(
        failure.contains(
            "planned again before changing anything and was refused, so nothing was changed"
        ),
        "{failure}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_where_a_start_first_service_does_not_change_says_nothing_of_it() -> TestResult {
    let cluster = cluster()?;
    locked(&cluster.nodes[1].script).unchanged = true;
    let mut app = stage("app", &[&["app-1"], &["app-2"]]);
    app.start_first = vec!["web".to_owned()];
    let hash = cluster.reviewed(vec![app])?;
    let said = Mutex::new(Vec::new());
    let report = cluster
        .environment
        .apply_deployment(&hash, &|line| locked(&said).push(line.to_string()))
        .await?;
    assert!(report.succeeded(), "{:?}", report.failure("mm deploy plan"));
    let said = locked(&said).clone();
    assert!(
        said.iter()
            .any(|line| line == "app-1: app: started the new web next to the old"),
        "{said:?}"
    );
    assert!(
        !said
            .iter()
            .any(|line| line.starts_with("app-2: app: started")),
        "{said:?}"
    );
    assert!(said.iter().any(|line| line == "app-2: app"), "{said:?}");
    Ok(())
}
