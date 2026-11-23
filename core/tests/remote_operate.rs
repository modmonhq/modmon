//! Restarting a service and running a job on a cluster target, as the
//! client orders it: a service restarts on one node at a time, each
//! healthy again (its node's answer) before the next is asked, and the
//! first node that fails stops the restart. A job runs on the node that
//! runs its service.

mod temp_dirs;

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
    ApprovalContext, ExecRequest, LogRequest, OperationCancellation, RestartOutcome,
    RestartRequest, ServiceSelector,
};
use mm::domain::placement::{ClusterNodes, services_on_node};
use mm::domain::remote::{
    ClusterPlanSeal, NodeFacts, RemoteAccessPlan, RemoteDeploymentPlan, RemoteIdentity, RemoteScope,
};
use mm::domain::runtime::{RuntimeKind, ServiceIdentity};
use mm::domain::scheduler::{
    JobRun, JobRunRequest, JobRunState, JobRunsRequest, JobTrigger, runtime_job_name,
};
use mm::domain::users::AccessChange;
use mm::domain::workspace::{ProjectId, WorkspaceSnapshot};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMES: [&str; 3] = ["app-1", "app-2", "db-1"];

/// `web` runs on the two app nodes, `db` on its own node.
const MODULES: &str = "\
shop:
  path: shop
  services:
    web: {stack: app, placement: [app-1, app-2]}
    db: {stack: app, placement: db-1}
";

/// Every call any node received, in the order they arrived.
type Journal = Arc<Mutex<Vec<String>>>;

struct Node {
    name: String,
    journal: Journal,
    /// The restart fails here.
    fails: bool,
    /// The node refuses the restart: the key may not restart here.
    refuses: bool,
    /// The restart is cancelled here (`mm deploy cancel`).
    cancelled: bool,
}

fn record(journal: &Journal, call: String) {
    journal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(call);
}

fn calls(journal: &Journal) -> Vec<String> {
    journal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn unused<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not used by this test".to_owned(),
    })
}

fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(7))
}

#[async_trait]
impl RemoteTransport for Node {
    async fn restart(
        &self,
        scope: RemoteScope,
        request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        if self.refuses {
            return Err(MmError::Security {
                message: "no grant matches this capability and exact scope".to_owned(),
            });
        }
        if self.cancelled {
            return Err(MmError::Cancelled {
                message: "restart of service 'web' 53410f2d-599 was cancelled by operator"
                    .to_owned(),
            });
        }
        record(&self.journal, format!("{} restart begins", self.name));
        // The node answers only once its containers are healthy again.
        tokio::time::sleep(Duration::from_millis(20)).await;
        record(&self.journal, format!("{} restart healthy", self.name));
        if self.fails {
            return Err(MmError::RuntimeResponse {
                message: "service 'web' was restarted and is not healthy: unhealthy".to_owned(),
            });
        }
        Ok(RestartOutcome {
            runtime: scope.runtime,
            identity: ServiceIdentity {
                project_id: scope.project_id,
                environment: scope.environment,
                stack: "app".to_owned(),
                module: Some("shop".to_owned()),
                service: request.selector.service,
            },
            containers: vec![format!("{}-web-1", self.name)],
            rolling: false,
            nodes: Vec::new(),
        })
    }

    async fn run_job(
        &self,
        scope: RemoteScope,
        request: JobRunRequest,
        _patience: Duration,
    ) -> Result<JobRun, MmError> {
        record(&self.journal, format!("{} runs {}", self.name, request.job));
        Ok(run(&scope, &request.job, 1))
    }

    async fn job_runs(
        &self,
        scope: RemoteScope,
        _request: JobRunsRequest,
    ) -> Result<Vec<JobRun>, MmError> {
        if self.refuses {
            return Err(MmError::Remote {
                code: mm::domain::error::NOT_DEPLOYED_CODE,
                message: "nothing is deployed to 'production' on this host yet; run `mm deploy plan --environment production`".to_owned(),
            }
            .in_own_words());
        }
        let at = match self.name.as_str() {
            "app-1" => 30,
            "app-2" => 10,
            _ => 20,
        };
        Ok(vec![run(&scope, "migrate", at)])
    }

    async fn status(&self, _scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        unused()
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
        unused()
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

fn run(scope: &RemoteScope, job: &str, requested_at_unix_ms: i64) -> JobRun {
    let id = uuid::Uuid::from_u128(u128::try_from(requested_at_unix_ms).unwrap_or_default());
    JobRun {
        id,
        project_id: scope.project_id,
        environment: scope.environment.clone(),
        job: job.to_owned(),
        schedule: None,
        trigger: JobTrigger::Manual,
        state: JobRunState::Succeeded,
        attempt: 1,
        maximum_attempts: 1,
        fence_token: 1,
        runtime_name: runtime_job_name(scope.project_id, id),
        runtime_id: None,
        requested_at_unix_ms,
        scheduled_for_unix_ms: None,
        started_at_unix_ms: None,
        finished_at_unix_ms: None,
        next_attempt_at_unix_ms: None,
        exit_code: Some(0),
        error_code: None,
        output: String::new(),
        output_truncated: false,
        cancellation_requested: false,
        principal: "operator".to_owned(),
        source: "test".to_owned(),
        node: None,
    }
}

/// A three-node cluster whose nodes write every call to one journal;
/// the node named `failing` fails its restart.
fn cluster(
    failing: Option<&str>,
) -> Result<(RemoteEnvironment, Journal), Box<dyn std::error::Error>> {
    cluster_with(failing, None)
}

/// `cluster`, and the node named `refusing` refuses to restart.
fn cluster_with(
    failing: Option<&str>,
    refusing: Option<&str>,
) -> Result<(RemoteEnvironment, Journal), Box<dyn std::error::Error>> {
    cluster_built(failing, refusing, None)
}

/// `cluster_with`, and the restart on the node named `cancelled` is
/// cancelled.
fn cluster_built(
    failing: Option<&str>,
    refusing: Option<&str>,
    cancelled: Option<&str>,
) -> Result<(RemoteEnvironment, Journal), Box<dyn std::error::Error>> {
    let modules: BTreeMap<String, mm::domain::config::ModuleConfig> =
        serde_yaml_ng::from_str(MODULES)?;
    let groups = BTreeMap::new();
    let journal = Journal::default();
    let mut nodes = Vec::new();
    for (index, name) in NAMES.iter().enumerate() {
        let placed = services_on_node(
            &modules,
            "production",
            ClusterNodes {
                nodes: &NAMES,
                groups: &groups,
            },
            name,
        )?;
        let node = Arc::new(Node {
            name: (*name).to_owned(),
            journal: Arc::clone(&journal),
            fails: failing == Some(*name),
            refuses: refusing == Some(*name),
            cancelled: cancelled == Some(*name),
        });
        nodes.push(RemoteNode {
            name: Some((*name).to_owned()),
            host: format!("203.0.113.{}", index + 1),
            port: 22,
            repo_address: None,
            private_address: Some(Ipv4Addr::new(10, 90, 0, u8::try_from(index + 1)?)),
            transport: Ok(node as Arc<dyn RemoteTransport>),
            scope: RemoteScope {
                project_id: project(),
                environment: "production".to_owned(),
                target: "production".to_owned(),
                target_fingerprint: "a".repeat(64),
                runtime: RuntimeKind::DockerCompose,
                node: Some((*name).to_owned()),
            },
            placed: Some(placed),
        });
    }
    Ok((
        RemoteEnvironment::new("production".to_owned(), nodes)?,
        journal,
    ))
}

fn restart_web() -> RestartRequest {
    RestartRequest {
        selector: ServiceSelector {
            stack: None,
            module: None,
            service: "web".to_owned(),
            container: None,
        },
        approval: ApprovalContext {
            principal: "operator".to_owned(),
            source: "test".to_owned(),
            approved_high_risk: false,
        },
    }
}

#[tokio::test]
async fn a_service_restarts_on_one_node_at_a_time_each_healthy_before_the_next() -> TestResult {
    let (environment, journal) = cluster(None)?;
    let outcome = environment.restart(restart_web()).await?;
    assert_eq!(
        calls(&journal),
        [
            "app-1 restart begins",
            "app-1 restart healthy",
            "app-2 restart begins",
            "app-2 restart healthy",
        ],
        "db-1 does not run web, and app-2 waits for app-1"
    );
    assert_eq!(outcome.nodes, ["app-1", "app-2"]);
    assert_eq!(outcome.containers, ["app-1-web-1", "app-2-web-1"]);
    assert!(outcome.rolling);
    Ok(())
}

#[tokio::test]
async fn the_first_node_that_fails_stops_the_restart_and_says_what_was_done() -> TestResult {
    let (environment, journal) = cluster(Some("app-1"))?;
    let Err(error) = environment.restart(restart_web()).await else {
        return Err("a restart that failed on app-1 succeeded".into());
    };
    let message = error.to_string();
    assert!(message.contains("node 'app-1'"), "{message}");
    assert!(message.contains("Not restarted: app-2"), "{message}");
    assert!(
        !calls(&journal).iter().any(|call| call.starts_with("app-2")),
        "app-2 was asked after app-1 failed"
    );
    Ok(())
}

// Found on the test VMs: a user at `view` was told
// "error[MM_RUNTIME_RESPONSE]: runtime response was invalid: node
// 'mm-cluster-1': security policy rejected the operation: …", and a restart
// that failed its health check said "runtime response was invalid" twice.
#[tokio::test]
async fn a_node_that_refuses_the_restart_is_a_refusal_and_its_failure_is_said_once() -> TestResult {
    let (environment, _journal) = cluster_with(None, Some("app-1"))?;
    let Err(error) = environment.restart(restart_web()).await else {
        return Err("a refused restart succeeded".into());
    };
    assert_eq!(error.code(), "MM_SECURITY", "{error}");
    let message = error.to_string();
    assert!(message.contains("node 'app-1'"), "{message}");
    assert!(message.contains("Not restarted: app-2"), "{message}");

    let (environment, _journal) = cluster(Some("app-2"))?;
    let Err(error) = environment.restart(restart_web()).await else {
        return Err("a failed restart succeeded".into());
    };
    let message = error.to_string();
    assert_eq!(
        message.matches("runtime response was invalid").count(),
        1,
        "{message}"
    );
    assert!(
        message.contains("Restarted and healthy before it: app-1"),
        "{message}"
    );
    Ok(())
}

// On the VMs (10a item 11d) `mm deploy cancel` of a rolling restart made
// the restart end with "error[MM_RUNTIME_RESPONSE]: runtime response was
// invalid: node 'mm-cluster-1': restart of service 'web' … was cancelled":
// the node's MM_CANCELLED became a malformed answer.
#[tokio::test]
async fn a_cancelled_restart_ends_as_cancelled() -> TestResult {
    let (environment, _journal) = cluster_built(None, None, Some("app-1"))?;
    let Err(error) = environment.restart(restart_web()).await else {
        return Err("a cancelled restart succeeded".into());
    };
    assert_eq!(error.code(), "MM_CANCELLED", "{error}");
    let message = error.to_string();
    assert!(message.contains("node 'app-1'"), "{message}");
    assert!(message.contains("Not restarted: app-2"), "{message}");
    assert!(!message.contains("runtime response"), "{message}");
    Ok(())
}

#[tokio::test]
async fn a_job_runs_on_the_node_of_its_service_and_runs_are_read_from_every_node() -> TestResult {
    let (environment, journal) = cluster(None)?;
    let run = environment
        .run_job(
            Some("db-1"),
            JobRunRequest::manual("migrate", true),
            Duration::from_secs(60),
        )
        .await?;
    assert_eq!(calls(&journal), ["db-1 runs migrate"]);
    assert_eq!(run.node.as_deref(), Some("db-1"));
    let runs = environment
        .job_runs(JobRunsRequest {
            job: None,
            run: None,
            limit: 2,
        })
        .await?;
    let nodes = runs
        .iter()
        .map(|run| run.node.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(nodes, ["app-1", "db-1"], "newest first, at most the limit");
    Ok(())
}

#[tokio::test]
async fn a_nodes_own_refusal_of_a_read_keeps_its_words_and_names_the_node() -> TestResult {
    let (environment, _journal) = cluster_with(None, Some("app-2"))?;
    let Err(error) = environment
        .job_runs(JobRunsRequest {
            job: None,
            run: None,
            limit: 10,
        })
        .await
    else {
        return Err("a refused read succeeded".into());
    };
    assert_eq!(error.code(), "MM_NOT_DEPLOYED");
    assert_eq!(
        error.refusal().as_deref(),
        Some(
            "node 'app-2': nothing is deployed to 'production' on this host yet; run `mm deploy plan --environment production`"
        )
    );
    Ok(())
}

#[test]
fn a_runs_output_keeps_its_end_within_a_bound() {
    let scope = RemoteScope {
        project_id: project(),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    };
    let mut short = run(&scope, "migrate", 1);
    short.output = "migrated\n".to_owned();
    short.keep_output_end(64);
    assert_eq!(short.output, "migrated\n");
    assert!(!short.output_truncated);

    let mut long = run(&scope, "migrate", 1);
    long.output = "line one\nüber failed\n".to_owned();
    long.keep_output_end(12);
    assert_eq!(
        long.output, "ber failed\n",
        "cut at a character, not inside one"
    );
    assert!(long.output_truncated);
}

#[tokio::test]
async fn a_project_without_jobs_reads_no_runs_and_asks_no_host() -> TestResult {
    // A read of the jobs of a remote environment whose project
    // declares none is an empty list, never a failure.
    let (environment, journal) = cluster(None)?;
    let root = temp_dirs::tempdir()?;
    let document = mm::adapters::config::ConfigLoader
        .initialize(root.path(), None)?
        .document;
    let gateway = mm::app::JobGateway::Remote {
        environment: Box::new(environment),
        document: Box::new(document),
    };
    let approval = ApprovalContext {
        principal: "agent".to_owned(),
        source: "mcp".to_owned(),
        approved_high_risk: false,
    };
    let runs = gateway
        .runs(
            JobRunsRequest {
                job: None,
                run: None,
                limit: 10,
            },
            &approval,
        )
        .await?;
    assert!(runs.is_empty());
    // Interrupting a wait for a run on a remote host leaves it running
    // there: a note, not an error. `mm job cancel` of it is a refusal.
    let id = uuid::Uuid::new_v4();
    let note = gateway
        .left_running(id)
        .ok_or("a remote run was cancelled")?;
    assert!(
        note.starts_with(&format!("job run '{id}' goes on on the remote host"))
            && note.contains(&format!("`mm job status {id}`")),
        "{note}"
    );
    assert!(gateway.cancel(id, &approval).is_err());
    assert!(calls(&journal).is_empty(), "no host was asked");
    let Err(error) = gateway
        .run(JobRunRequest::manual("migrate", true), &approval)
        .await
    else {
        return Err("a job that is not declared ran".into());
    };
    assert!(
        error.to_string().contains("the configuration has no jobs:"),
        "{error}"
    );
    Ok(())
}
