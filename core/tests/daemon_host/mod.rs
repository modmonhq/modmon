//! A host's `mmd` for tests: a daemon on a runtime of its own with one
//! worker thread (what `mmd` has on a one-CPU host), use cases whose
//! handlers can be held open, and clients that speak the relay's protocol
//! from the test's runtime. A daemon that stops answering therefore fails
//! a test instead of hanging it.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use mm::adapters::remote::{
    DaemonConfig, DeployedState, RemoteDaemon, RootAccessSyncRequest, decode_frame, encode_frame,
    read_raw_frame, write_raw_frame,
};
use mm::app::ports::{ExecStream, LogStream, RemoteRequestContext, RemoteUseCases};
use mm::domain::build::BuildReceipt;
use mm::domain::deployment::{
    DEPLOYMENT_PLAN_SCHEMA, DeploymentPlan, DeploymentResult, DeploymentRouteDiff,
};
use mm::domain::error::MmError;
use mm::domain::observability::{
    ApprovalContext, ExecEvent, ExecOutputSource, ExecRequest, ExecStdinMode, ExecTtyMode,
    LogRequest, OperationCancellation, RestartOutcome, RestartRequest, ServiceSelector,
};
use mm::domain::remote::{
    ClientFrame, ClientHello, PROTOCOL_VERSION, RemoteMethod, RemoteResult, RemoteScope,
    ServerFrame,
};
use mm::domain::runtime::RuntimeKind;
use mm::domain::scheduler::{
    JobRun, JobRunRequest, JobRunState, JobRunsRequest, JobTrigger, StartedRuns,
};
use mm::domain::target::TargetId;
use mm::domain::traefik_dynamic::{PreparedTraefikDynamicConfig, TraefikDynamicConfigManifest};
use mm::domain::workspace::{ProjectId, WorkspaceSnapshot};
use tokio::net::UnixStream;

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The host's break-glass key: may do everything.
pub const OPS: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 ops@desk";
/// A developer the project's users file lets deploy and run commands.
pub const ALICE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";
/// An operator: restarts services and runs jobs, and deploys nothing.
pub const BOB: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIClvXO2UZe2GtDEoTsK6+mkkk+9q8ihRNwqq95JIkl2k";
/// A viewer: reads status, logs, and job runs.
pub const CAROL: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIL6AE77eRNFr7qlvN6ocnbkvGvVEDM5RfKJmIF34Q7Dx";

/// How long a test waits for an answer the daemon owes at once.
pub const PROMPT: Duration = Duration::from_secs(5);

pub fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(4))
}

pub fn fingerprint(key: &str) -> String {
    mm::domain::host_policy::parse_public_key(key)
        .map(|key| key.fingerprint)
        .unwrap_or_default()
}

/// What the tests hold open and watch inside the daemon's handlers.
#[derive(Default)]
pub struct Handlers {
    /// While set, `apply_deployment` and `roll_stack` wait inside.
    pub hold_apply: AtomicBool,
    /// While set, an exec stream stays open after its first output.
    pub hold_exec: AtomicBool,
    /// While set, `run_job` waits inside.
    pub hold_job: AtomicBool,
    /// While set, `restart` waits inside.
    pub hold_restart: AtomicBool,
    /// While set, run 77 of job `migrate` is running.
    pub job_running: AtomicBool,
    /// What `cancel_job_runs` was asked to stop, call by call.
    pub cancelled_runs: std::sync::Mutex<Vec<Vec<StartedRuns>>>,
    /// How many handlers are waiting inside right now.
    pub inside: AtomicUsize,
    /// How many handlers were dropped while they waited: the daemon gave
    /// the operation up.
    pub abandoned: AtomicUsize,
    /// How many handlers ran to their end.
    pub finished: AtomicUsize,
    /// How many times the host was returned to what it ran before.
    pub rolled_back: AtomicUsize,
    /// Each stage step the host took, as `<step> <key>…`.
    pub stages: std::sync::Mutex<Vec<String>>,
    /// The staged changes that went live, by key.
    pub live: std::sync::Mutex<Vec<String>>,
}

impl Handlers {
    /// Waits until `count` handlers are inside.
    pub async fn entered(&self, count: usize) -> TestResult {
        for _ in 0..500 {
            if self.inside.load(Ordering::SeqCst) >= count {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(format!("{count} handler(s) did not start within 5 s").into())
    }

    async fn held(&self, flag: &AtomicBool) {
        struct Inside<'a>(&'a Handlers, bool);
        impl Drop for Inside<'_> {
            fn drop(&mut self) {
                self.0.inside.fetch_sub(1, Ordering::SeqCst);
                if !self.1 {
                    self.0.abandoned.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        self.inside.fetch_add(1, Ordering::SeqCst);
        let mut inside = Inside(self, false);
        while flag.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        inside.1 = true;
    }
}

struct UseCases(Arc<Handlers>);

#[async_trait]
impl RemoteUseCases for UseCases {
    async fn status(&self, context: &RemoteRequestContext) -> Result<WorkspaceSnapshot, MmError> {
        Ok(workspace_snapshot(context))
    }

    async fn logs(
        &self,
        _context: &RemoteRequestContext,
        _request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        let (_sender, receiver) = tokio::sync::mpsc::channel(1);
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        context: &RemoteRequestContext,
        _request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let handlers = Arc::clone(&self.0);
        let principal = context.principal.clone();
        let watched = cancellation.clone();
        tokio::spawn(async move {
            let output = ExecEvent::Output {
                source: ExecOutputSource::Stdout,
                value: format!("{principal}-exec"),
                lossy_utf8: false,
            };
            if sender.send(Ok(output)).await.is_err() {
                return;
            }
            handlers.inside.fetch_add(1, Ordering::SeqCst);
            while handlers.hold_exec.load(Ordering::SeqCst) && !watched.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            handlers.inside.fetch_sub(1, Ordering::SeqCst);
            if watched.is_cancelled() {
                handlers.abandoned.fetch_add(1, Ordering::SeqCst);
            }
        });
        Ok(ExecStream::new(receiver, cancellation))
    }

    async fn plan_deployment(
        &self,
        context: &RemoteRequestContext,
        _receipt: Option<&BuildReceipt>,
    ) -> Result<DeploymentPlan, MmError> {
        deployment_plan(context)
    }

    async fn apply_deployment(
        &self,
        context: &RemoteRequestContext,
        deployment_plan_hash: &str,
        _receipt: Option<&BuildReceipt>,
    ) -> Result<DeploymentResult, MmError> {
        self.0.held(&self.0.hold_apply).await;
        self.0.finished.fetch_add(1, Ordering::SeqCst);
        Ok(DeploymentResult {
            plan_hash: deployment_plan_hash.to_owned(),
            target_name: context.scope.target.clone(),
            target_id: TargetId(uuid::Uuid::from_u128(94)),
            environment: context.scope.environment.clone(),
            applied_stacks: vec!["app".to_owned()],
            removed_stacks: Vec::new(),
            route_generation: 0,
            route_count: 0,
            moved_away: Vec::new(),
            jobs: Vec::new(),
        })
    }

    async fn begin_rollout(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _receipt: Option<&BuildReceipt>,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn finish_rollout(
        &self,
        context: &RemoteRequestContext,
        deployment_plan_hash: &str,
    ) -> Result<DeploymentResult, MmError> {
        self.apply_deployment(context, deployment_plan_hash, None)
            .await
    }

    async fn restart(
        &self,
        context: &RemoteRequestContext,
        request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        self.0.held(&self.0.hold_restart).await;
        self.0.finished.fetch_add(1, Ordering::SeqCst);
        Ok(RestartOutcome {
            runtime: context.scope.runtime,
            identity: mm::domain::runtime::ServiceIdentity {
                project_id: context.scope.project_id,
                environment: context.scope.environment.clone(),
                stack: "app".to_owned(),
                module: Some("api".to_owned()),
                service: request.selector.service,
            },
            containers: vec![format!("{}-restarted", request.approval.principal)],
            rolling: false,
            nodes: Vec::new(),
        })
    }

    async fn run_job(
        &self,
        context: &RemoteRequestContext,
        request: JobRunRequest,
    ) -> Result<JobRun, MmError> {
        self.0.held(&self.0.hold_job).await;
        self.0.finished.fetch_add(1, Ordering::SeqCst);
        let mut run = job_run(context, &request.job);
        run.id = request.run.unwrap_or(run.id);
        Ok(run)
    }

    async fn cancel_job_runs(
        &self,
        context: &RemoteRequestContext,
        started: &[StartedRuns],
    ) -> Result<Vec<JobRun>, MmError> {
        self.0
            .cancelled_runs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(started.to_vec());
        Ok(started
            .iter()
            .filter_map(|which| match which {
                StartedRuns::Run(id) => {
                    let mut run = job_run(context, "migrate");
                    run.id = *id;
                    run.state = JobRunState::Cancelled;
                    run.exit_code = Some(130);
                    Some(run)
                }
                StartedRuns::Named(wanted) => {
                    let mut run = job_run(context, "migrate");
                    let named = run.id.to_string().starts_with(wanted.as_str());
                    (named && self.0.job_running.swap(false, Ordering::SeqCst)).then(|| {
                        run.state = JobRunState::Cancelled;
                        run.exit_code = Some(130);
                        run
                    })
                }
                StartedRuns::Deployment(_) => None,
            })
            .collect())
    }

    async fn running_job_runs(
        &self,
        context: &RemoteRequestContext,
    ) -> Result<Vec<mm::domain::rollout::OperationInFlight>, MmError> {
        if !self.0.job_running.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let mut run = job_run(context, "migrate");
        run.state = JobRunState::Running;
        run.finished_at_unix_ms = None;
        run.exit_code = None;
        Ok(vec![mm::domain::scheduler::run_in_flight(
            &run,
            Duration::from_secs(600),
            chrono::Utc::now().timestamp_millis(),
        )])
    }

    async fn job_runs(
        &self,
        context: &RemoteRequestContext,
        _request: JobRunsRequest,
    ) -> Result<Vec<JobRun>, MmError> {
        Ok(vec![job_run(context, "migrate")])
    }

    async fn rollback_rollout(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _changes: Option<&mm::domain::rollout::RolloutChanges>,
    ) -> Result<bool, MmError> {
        self.0.rolled_back.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }

    async fn secret_recipient(&self, _context: &RemoteRequestContext) -> Result<String, MmError> {
        Ok("age1recipient".to_owned())
    }

    async fn stage_values(
        &self,
        _context: &RemoteRequestContext,
        stage: mm::domain::staged::ValueStage,
    ) -> Result<mm::domain::staged::StageAnswer, MmError> {
        use mm::domain::staged::{StageAnswer, StageState, StageStep};
        let (state, changes, step) = match stage.step {
            StageStep::Prepare { changes, .. } => (StageState::Prepared, changes, "prepare"),
            StageStep::Commit => (StageState::Committed, Vec::new(), "commit"),
            StageStep::Revert => (StageState::Reverted, Vec::new(), "revert"),
        };
        let keys = changes
            .iter()
            .map(|change| change.key.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        self.0
            .stages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!("{step} {keys}").trim().to_owned());
        Ok(StageAnswer {
            stage: stage.stage,
            state,
            changes,
        })
    }

    async fn host_values(
        &self,
        context: &RemoteRequestContext,
    ) -> Result<mm::domain::staged::NodeValues, MmError> {
        Ok(mm::domain::staged::NodeValues {
            node: context.scope.node.clone(),
            commit: String::new(),
            values: vec![staged_value()],
        })
    }

    async fn staged_plan(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<Vec<mm::domain::staged::StagedPlanItem>, MmError> {
        Ok(vec![mm::domain::staged::StagedPlanItem {
            key: "web.app-key".to_owned(),
            kind: mm::domain::staged::ValueKind::Secret,
            change: mm::domain::staged::StagedChangeKind::Changed,
            revision: "r2".to_owned(),
            staged_by: "alice".to_owned(),
            services: vec!["web".to_owned()],
        }])
    }

    async fn go_live(
        &self,
        _context: &RemoteRequestContext,
        staged: &[mm::domain::staged::StagedPlanItem],
    ) -> Result<(), MmError> {
        self.0
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(staged.iter().map(|item| item.key.clone()));
        Ok(())
    }
}

/// The secret every test host holds: live at r1, r2 staged by alice.
pub fn staged_value() -> mm::domain::staged::HostValue {
    mm::domain::staged::HostValue {
        key: "web.app-key".to_owned(),
        kind: mm::domain::staged::ValueKind::Secret,
        origin: Some(mm::domain::staged::ValueOrigin::Staged),
        provider: None,
        services: vec!["web".to_owned()],
        required: true,
        active: Some(mm::domain::staged::ValueRevision {
            revision: "r1".to_owned(),
            changed_by: Some("alice".to_owned()),
            changed_at_unix_ms: Some(1),
        }),
        staged: Some(mm::domain::staged::StagedRevision {
            revision: "r2".to_owned(),
            removal: false,
            staged_by: "alice".to_owned(),
            staged_at_unix_ms: 2,
        }),
    }
}

/// A run of `job` that succeeded, started by the request's principal.
pub fn job_run(context: &RemoteRequestContext, job: &str) -> JobRun {
    let id = uuid::Uuid::from_u128(77);
    JobRun {
        id,
        project_id: context.scope.project_id,
        environment: context.scope.environment.clone(),
        job: job.to_owned(),
        schedule: None,
        trigger: JobTrigger::Manual,
        state: JobRunState::Succeeded,
        attempt: 1,
        maximum_attempts: 1,
        fence_token: 1,
        runtime_name: mm::domain::scheduler::runtime_job_name(context.scope.project_id, id),
        runtime_id: None,
        requested_at_unix_ms: 1,
        scheduled_for_unix_ms: None,
        started_at_unix_ms: Some(2),
        finished_at_unix_ms: Some(3),
        next_attempt_at_unix_ms: None,
        exit_code: Some(0),
        error_code: None,
        output: "migrated".to_owned(),
        output_truncated: false,
        cancellation_requested: false,
        principal: context.principal.clone(),
        source: "authenticated-ssh-relay".to_owned(),
        node: None,
    }
}

/// One host: its files, its daemon, and the runtime the daemon serves on.
pub struct Host {
    pub temporary: tempfile::TempDir,
    pub config: DaemonConfig,
    pub daemon: Arc<RemoteDaemon>,
    pub handlers: Arc<Handlers>,
    /// The commit whose users file lets alice deploy.
    pub commit: String,
    runtime: Option<tokio::runtime::Runtime>,
}

impl Drop for Host {
    fn drop(&mut self) {
        // A daemon thread that is stuck must not keep the test from ending.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl Host {
    /// A host that runs `production` of the test project, with alice's
    /// access synced from the project's users file.
    pub async fn with_alice() -> TestResult<Self> {
        let host = Self::new()?;
        let request = RootAccessSyncRequest {
            project_id: project(),
            environment: "production".to_owned(),
            commit: host.commit.clone(),
            dry_run: false,
        };
        let daemon = Arc::clone(&host.daemon);
        host.on_daemon(async move { daemon.sync_access_as_root(&request).await })
            .await??;
        Ok(host)
    }

    pub fn new() -> TestResult<Self> {
        let temporary = super::temp_dirs::tempdir()?;
        let commit = origin(temporary.path())?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let config = daemon_config(temporary.path(), &std::fs::canonicalize(root)?)?;
        let handlers = Arc::new(Handlers::default());
        let use_cases: Arc<dyn RemoteUseCases> = Arc::new(UseCases(Arc::clone(&handlers)));
        let daemon = Arc::new(RemoteDaemon::new(config.clone(), use_cases)?);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("test-mmd-worker")
            .enable_all()
            .build()?;
        Ok(Self {
            temporary,
            config,
            daemon,
            handlers,
            commit,
            runtime: Some(runtime),
        })
    }

    /// Another daemon on this host's files, as `mmd` started again.
    pub fn start_again(&self) -> Result<RemoteDaemon, MmError> {
        RemoteDaemon::new(
            self.config.clone(),
            Arc::new(UseCases(Arc::clone(&self.handlers))),
        )
    }

    /// Runs `work` on the daemon's runtime.
    pub fn on_daemon<T: Send + 'static>(
        &self,
        work: impl std::future::Future<Output = T> + Send + 'static,
    ) -> tokio::task::JoinHandle<T> {
        match &self.runtime {
            Some(runtime) => runtime.spawn(work),
            None => tokio::spawn(work),
        }
    }

    /// A connection the daemon serves, with nothing sent on it yet.
    fn connection(&self) -> TestResult<Client> {
        let (server, client) = std::os::unix::net::UnixStream::pair()?;
        server.set_nonblocking(true)?;
        client.set_nonblocking(true)?;
        let daemon = Arc::clone(&self.daemon);
        let served = self.on_daemon(async move {
            let stream = UnixStream::from_std(server).map_err(|source| MmError::Io {
                context: "adopting the test connection".to_owned(),
                source,
            })?;
            daemon.serve_stream(stream).await
        });
        Ok(Client {
            stream: UnixStream::from_std(client)?,
            ahead: Vec::new(),
            served,
        })
    }

    fn relay_session(key: &str) -> ClientFrame {
        ClientFrame::RelaySession {
            version: PROTOCOL_VERSION,
            key_fingerprint: fingerprint(key),
            source: "127.0.0.1 10000 127.0.0.1 22".to_owned(),
            session_id: uuid::Uuid::new_v4(),
        }
    }

    fn hello(&self, client_nonce: uuid::Uuid) -> ClientFrame {
        ClientFrame::Hello {
            version: PROTOCOL_VERSION,
            minimum: PROTOCOL_VERSION,
            hello: ClientHello {
                client_nonce,
                expected_daemon_fingerprint: self.daemon.identity().fingerprint.clone(),
                client_version: String::new(),
            },
        }
    }

    pub fn request(method: RemoteMethod, deadline: Duration) -> TestResult<ClientFrame> {
        Ok(ClientFrame::Request {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::new_v4(),
            operation_nonce: uuid::Uuid::new_v4(),
            deadline_unix_ms: chrono::Utc::now().timestamp_millis()
                + i64::try_from(deadline.as_millis())?,
            scope: scope(),
            method: Box::new(method),
        })
    }

    /// A relay session of `key` with nothing else sent on it.
    pub async fn session(&self, key: &str) -> TestResult<Client> {
        let mut client = self.connection()?;
        client.send(&Self::relay_session(key)).await?;
        Ok(client)
    }

    /// A relay session of `key`, past the hello.
    pub async fn connect(&self, key: &str) -> TestResult<Client> {
        let mut client = self.connection()?;
        client.send(&Self::relay_session(key)).await?;
        let client_nonce = uuid::Uuid::new_v4();
        client.send(&self.hello(client_nonce)).await?;
        match client.next(PROMPT).await? {
            ServerFrame::Hello { hello, .. } if hello.client_nonce == client_nonce => Ok(client),
            other => Err(format!("the daemon did not answer the hello: {other:?}").into()),
        }
    }

    /// Sends one request as `key` and returns the open session.
    pub async fn ask(&self, key: &str, method: RemoteMethod) -> TestResult<Client> {
        self.ask_within(key, method, Duration::from_secs(60)).await
    }

    /// As `ask`, also returning the request's id: what a client needs to
    /// send data frames after its request.
    pub async fn ask_with_id(
        &self,
        key: &str,
        method: RemoteMethod,
    ) -> TestResult<(Client, uuid::Uuid)> {
        let mut client = self.connect(key).await?;
        let frame = Self::request(method, Duration::from_secs(60))?;
        let ClientFrame::Request { request_id, .. } = &frame else {
            return Err("the request frame is not a request".into());
        };
        let request_id = *request_id;
        client.send(&frame).await?;
        Ok((client, request_id))
    }

    /// As `ask`, with the request's own deadline.
    pub async fn ask_within(
        &self,
        key: &str,
        method: RemoteMethod,
        deadline: Duration,
    ) -> TestResult<Client> {
        let mut client = self.connect(key).await?;
        client.send(&Self::request(method, deadline)?).await?;
        Ok(client)
    }

    /// As `ask`, by a client that does not wait for the daemon's hello:
    /// the session, the hello with `client_nonce`, and the request reach
    /// the daemon in one write. Nothing has been read from the session.
    pub async fn ask_at_once(
        &self,
        key: &str,
        client_nonce: uuid::Uuid,
        method: RemoteMethod,
    ) -> TestResult<Client> {
        use tokio::io::AsyncWriteExt;

        let mut client = self.connection()?;
        let mut bytes = Vec::new();
        for frame in [
            Self::relay_session(key),
            self.hello(client_nonce),
            Self::request(method, Duration::from_secs(60))?,
        ] {
            write_raw_frame(&mut bytes, &encode_frame(&frame)?).await?;
        }
        client.stream.write_all(&bytes).await?;
        Ok(client)
    }

    /// Plans a deployment of the host's commit as `key` and returns the
    /// hash to apply.
    pub async fn plan(&self, key: &str) -> TestResult<String> {
        let method = RemoteMethod::PlanDeployment {
            commit: self.commit.clone(),
            receipt: None,
            release: Default::default(),
        };
        match self.ask(key, method).await?.result(PROMPT).await? {
            RemoteResult::DeploymentPlan(plan) => Ok(plan.binding.plan_hash),
            other => Err(format!("unexpected plan result: {other:?}").into()),
        }
    }

    /// Plans a deployment of `commit` as `key` and returns the hash to
    /// apply.
    pub async fn plan_at(&self, key: &str, commit: &str) -> TestResult<String> {
        let method = RemoteMethod::PlanDeployment {
            commit: commit.to_owned(),
            receipt: None,
            release: Default::default(),
        };
        match self.ask(key, method).await?.result(PROMPT).await? {
            RemoteResult::DeploymentPlan(plan) => Ok(plan.binding.plan_hash),
            other => Err(format!("unexpected plan result: {other:?}").into()),
        }
    }

    /// A new commit on the origin's `main` whose `mm.yaml` has `to` in
    /// place of `from`.
    pub fn commit_with(&self, from: &str, to: &str) -> TestResult<String> {
        let work = self.temporary.path().join("origin-work");
        let path = work.join(".config/mm.yaml");
        let text = std::fs::read_to_string(&path)?;
        if !text.contains(from) {
            return Err(format!("mm.yaml has no {from}").into());
        }
        std::fs::write(&path, text.replace(from, to))?;
        git(&work, &["commit", "--quiet", "--all", "-m", "change"])?;
        let origin = self.temporary.path().join("origin.git");
        git(
            &work,
            &["push", "--quiet", &origin.to_string_lossy(), "main"],
        )?;
        git(&work, &["rev-parse", "HEAD"])
    }

    /// A new commit on the origin's `main` that writes each file, by its
    /// path in the project, with the text given.
    pub fn commit_files(&self, files: &[(&str, &str)]) -> TestResult<String> {
        let work = self.temporary.path().join("origin-work");
        for (path, text) in files {
            std::fs::write(work.join(path), text)?;
        }
        git(&work, &["add", "--all"])?;
        git(&work, &["commit", "--quiet", "-m", "change files"])?;
        let origin = self.temporary.path().join("origin.git");
        git(
            &work,
            &["push", "--quiet", &origin.to_string_lossy(), "main"],
        )?;
        git(&work, &["rev-parse", "HEAD"])
    }

    /// Plans an access sync of the host's commit as `key` and returns the
    /// hash to apply.
    pub async fn sync_plan(&self, key: &str) -> TestResult<String> {
        let method = RemoteMethod::PlanTeamSync {
            commit: self.commit.clone(),
        };
        match self.ask(key, method).await?.result(PROMPT).await? {
            RemoteResult::AccessPlan(plan) => Ok(plan.binding.plan_hash),
            other => Err(format!("unexpected access plan result: {other:?}").into()),
        }
    }

    /// Host access state as `mmd access …` run as root changes it: another
    /// process, as far as the daemon can tell.
    pub fn root(&self) -> mm::adapters::remote::HostAccessStore {
        mm::adapters::remote::HostAccessStore::new(&self.config)
    }

    /// Blocks `key` in production, or lifts the block, as host root.
    pub async fn block(&self, key: &str, blocked: bool) -> TestResult {
        self.root()
            .set_block(
                project(),
                "production",
                &fingerprint(key),
                blocked,
                chrono::Utc::now().timestamp_millis(),
            )
            .await?;
        Ok(())
    }

    /// A second daemon over the same host state, as the one that starts
    /// after the first ended.
    pub fn next_daemon(&self) -> TestResult<RemoteDaemon> {
        let use_cases: Arc<dyn RemoteUseCases> = Arc::new(UseCases(Arc::clone(&self.handlers)));
        Ok(RemoteDaemon::new(self.config.clone(), use_cases)?)
    }

    /// The audit records the daemon wrote, oldest first.
    pub fn audit(&self) -> TestResult<Vec<serde_json::Value>> {
        let text = std::fs::read_to_string(self.config.state_root.join("audit.jsonl"))?;
        // The daemon may be in the middle of its next record.
        let complete = text.rfind('\n').map_or("", |end| &text[..end]);
        Ok(complete
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?)
    }

    /// The commit the host records as deployed.
    pub fn deployed_commit(&self) -> TestResult<String> {
        let path = self
            .config
            .state_root
            .join("projects")
            .join(project().0.to_string())
            .join("deployed/production.json");
        let state: DeployedState = serde_json::from_slice(&std::fs::read(path)?)?;
        Ok(state.commit)
    }
}

/// One relay session with the daemon.
pub struct Client {
    stream: UnixStream,
    /// A byte `is_silent` read ahead of the frame it belongs to.
    ahead: Vec<u8>,
    served: tokio::task::JoinHandle<Result<(), MmError>>,
}

impl Client {
    pub async fn send(&mut self, frame: &ClientFrame) -> TestResult {
        write_raw_frame(&mut self.stream, &encode_frame(frame)?).await?;
        Ok(())
    }

    /// Sends `frame` as written, such as one from a newer client.
    pub async fn send_json(&mut self, frame: &serde_json::Value) -> TestResult {
        write_raw_frame(&mut self.stream, &serde_json::to_vec(frame)?).await?;
        Ok(())
    }

    /// The daemon's next frame as written.
    pub async fn next_json(&mut self, patience: Duration) -> TestResult<serde_json::Value> {
        use tokio::io::AsyncReadExt;

        let ahead = std::mem::take(&mut self.ahead);
        let mut reader = ahead.as_slice().chain(&mut self.stream);
        let bytes = tokio::time::timeout(patience, read_raw_frame(&mut reader))
            .await
            .map_err(|_| "the daemon sent nothing")??
            .ok_or("the daemon closed the session without an answer")?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Whether the daemon ended the session.
    pub async fn ended(self) -> TestResult<Result<(), MmError>> {
        drop(self.stream);
        Ok(tokio::time::timeout(PROMPT, self.served).await??)
    }

    /// The daemon's next frame, which has to arrive within `patience`.
    pub async fn next(&mut self, patience: Duration) -> TestResult<ServerFrame> {
        use tokio::io::AsyncReadExt;

        let ahead = std::mem::take(&mut self.ahead);
        let mut reader = ahead.as_slice().chain(&mut self.stream);
        match tokio::time::timeout(patience, read_raw_frame(&mut reader)).await {
            Err(_) => {
                Err(format!("the daemon sent nothing for {} ms", patience.as_millis()).into())
            }
            Ok(Ok(Some(bytes))) => Ok(decode_frame(&bytes)?),
            Ok(Ok(None)) => Err("the daemon closed the session without an answer".into()),
            Ok(Err(error)) => Err(error.into()),
        }
    }

    /// Whether the daemon stays silent for `quiet`: it neither sends a
    /// byte nor ends the session.
    pub async fn is_silent(&mut self, quiet: Duration) -> bool {
        if !self.ahead.is_empty() {
            return false;
        }
        let until = tokio::time::Instant::now() + quiet;
        loop {
            if tokio::time::timeout_at(until, self.stream.readable())
                .await
                .is_err()
            {
                return true;
            }
            // Readiness can be stale; only a read tells.
            let mut byte = [0_u8; 1];
            match self.stream.try_read(&mut byte) {
                Ok(read) => {
                    self.ahead.extend_from_slice(&byte[..read]);
                    return false;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => return false,
            }
        }
    }

    /// The request's result; an error frame is an `Err` with its code.
    pub async fn result(&mut self, patience: Duration) -> TestResult<RemoteResult> {
        match self.next(patience).await? {
            ServerFrame::Result { result, .. } => Ok(*result),
            ServerFrame::Error { error, .. } => {
                Err(format!("{}: {}", error.code, error.message).into())
            }
            other => Err(format!("unexpected frame: {other:?}").into()),
        }
    }

    /// The code and message of the error the daemon answers with.
    pub async fn refusal(&mut self, patience: Duration) -> TestResult<(String, String)> {
        match self.next(patience).await? {
            ServerFrame::Error { error, .. } => Ok((error.code, error.message)),
            other => Err(format!("expected a refusal, got: {other:?}").into()),
        }
    }

    /// Ends the session as a client that went away does.
    pub fn hang_up(self) {
        drop(self.stream);
    }

    /// Stops reading the session while it stays open, as a client whose
    /// connection broke on the way back does: the daemon's answer cannot
    /// be delivered.
    pub fn stop_reading(self) -> TestResult<Self> {
        let stream = self.stream.into_std()?;
        stream.shutdown(std::net::Shutdown::Read)?;
        Ok(Self {
            stream: UnixStream::from_std(stream)?,
            ahead: self.ahead,
            served: self.served,
        })
    }
}

pub fn scope() -> RemoteScope {
    RemoteScope {
        project_id: project(),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    }
}

pub fn exec_request() -> ExecRequest {
    ExecRequest {
        selector: ServiceSelector {
            stack: Some("app".to_owned()),
            module: Some("api".to_owned()),
            service: "web".to_owned(),
            container: None,
        },
        argv: vec!["true".to_owned()],
        user: None,
        workdir: None,
        environment: Vec::new(),
        tty: ExecTtyMode::Disabled,
        stdin: ExecStdinMode::Closed,
        timeout: Duration::from_secs(1),
        approval: ApprovalContext {
            principal: "forged".to_owned(),
            source: "forged".to_owned(),
            approved_high_risk: false,
        },
        shell_wrapper: false,
    }
}

fn git(directory: &Path, arguments: &[&str]) -> TestResult<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.test"])
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// A bare origin whose `main` lets alice deploy and run commands in
/// production. Returns the commit.
fn origin(path: &Path) -> TestResult<String> {
    let work = path.join("origin-work");
    std::fs::create_dir_all(work.join(".config"))?;
    git(&work, &["init", "--quiet", "--initial-branch=main"])?;
    std::fs::write(
        work.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: {}\n  name: example\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nenvironments:\n  production: {{}}\nmodules:\n  api:\n    path: .\n    services:\n      web: {{stack: app}}\nusers:\n  alice:\n    keys: [\"{ALICE}\"]\n    access:\n      production: {{level: deploy, exec: [web], secrets: true}}\n  bob:\n    keys: [\"{BOB}\"]\n    access:\n      production: {{level: operate}}\n  carol:\n    keys: [\"{CAROL}\"]\n    access:\n      production: {{level: view}}\n",
            project().0
        ),
    )?;
    std::fs::write(
        work.join("compose.yaml"),
        "services:\n  web:\n    image: busybox\n",
    )?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "alice may deploy"])?;
    let commit = git(&work, &["rev-parse", "HEAD"])?;
    git(
        path,
        &["clone", "--quiet", "--bare", "origin-work", "origin.git"],
    )?;
    Ok(commit)
}

fn daemon_config(path: &Path, root: &Path) -> TestResult<DaemonConfig> {
    use std::os::unix::fs::PermissionsExt;

    let uid = rustix::process::getuid().as_raw();
    let gid = rustix::process::getgid().as_raw();
    let etc = path.join("etc");
    std::fs::create_dir_all(&etc)?;
    std::fs::set_permissions(&etc, std::fs::Permissions::from_mode(0o700))?;
    let host_policy_path = etc.join("host.yaml");
    std::fs::write(
        &host_policy_path,
        format!(
            "schema-version: 1\nbreak-glass:\n  - name: ops\n    key: {OPS}\nprojects:\n  {}:\n    name: example\n    origin: {}\n    environments: [production]\n",
            project().0,
            path.join("origin.git").display()
        ),
    )?;
    std::fs::set_permissions(&host_policy_path, std::fs::Permissions::from_mode(0o600))?;
    let state_root = path.join("state");
    let deployed = state_root
        .join("projects")
        .join(project().0.to_string())
        .join("deployed");
    std::fs::create_dir_all(&deployed)?;
    std::fs::write(
        deployed.join("production.json"),
        serde_json::to_vec(&DeployedState {
            project_id: project(),
            environment: "production".to_owned(),
            commit: "a".repeat(40),
            root: root.to_path_buf(),
            deployed_at_unix_ms: 1,
            deployed_by: "test".to_owned(),
            runs_services: true,
        })?,
    )?;
    Ok(DaemonConfig {
        schema_version: 1,
        socket_path: path.join("daemon.sock"),
        state_root,
        mm_home: path.join("home"),
        socket_uid: uid,
        socket_gid: gid,
        idle_timeout_ms: 5_000,
        host_policy_path,
        relay_uid: uid,
        authorized_keys_path: path.join("ssh/authorized_keys"),
        relay_command: "/usr/bin/mm relay serve --stdio".to_owned(),
        projects_root: path.join("projects"),
        node_env_path: path.join("node.env"),
    })
}

/// A plan of the engine that changes nothing: the daemon's own steps
/// around it are what these tests look at.
fn deployment_plan(context: &RemoteRequestContext) -> Result<DeploymentPlan, MmError> {
    let target_id = TargetId(uuid::Uuid::from_u128(94));
    let dynamic = PreparedTraefikDynamicConfig {
        root: PathBuf::from("/nonexistent/dynamic"),
        path: PathBuf::from("/nonexistent/dynamic/dynamic.yaml"),
        manifest: TraefikDynamicConfigManifest {
            target_id,
            claim_count: 0,
            router_count: 0,
            service_count: 0,
            digest: "b".repeat(64),
        },
    };
    let mut plan = DeploymentPlan {
        excluded_services: Vec::new(),
        schema: DEPLOYMENT_PLAN_SCHEMA.to_owned(),
        project_id: context.scope.project_id,
        project_name: "example".to_owned(),
        target_name: context.scope.target.clone(),
        target_id,
        target_fingerprint: context.scope.target_fingerprint.clone(),
        environment: context.scope.environment.clone(),
        runtime: Some(context.scope.runtime),
        runtime_source: Some("test".to_owned()),
        workload_intent_hash: Some("d".repeat(64)),
        networks: Vec::new(),
        expected_route_generation: 0,
        expected_route_config_digest: None,
        expected_previous_plan_hash: None,
        stacks: Vec::new(),
        removed_stacks: Vec::new(),
        preserved_volumes: Vec::new(),
        removed_preserved_volumes: Vec::new(),
        previous_owner_claims: Vec::new(),
        desired_route_claims: Vec::new(),
        complete_route_claims: Vec::new(),
        desired_middlewares: std::collections::BTreeMap::new(),
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
        registries: Default::default(),
        node: None,
        rollout: Default::default(),
        config_files: Vec::new(),
        runtime_changes: Vec::new(),
        release: None,
        plan_hash: String::new(),
    };
    plan.plan_hash = plan
        .canonical_hash()
        .map_err(|message| MmError::Security { message })?;
    Ok(plan)
}

fn workspace_snapshot(context: &RemoteRequestContext) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        workspace: mm::domain::workspace::WorkspaceRecord {
            id: mm::domain::workspace::WorkspaceId(uuid::Uuid::from_u128(720)),
            project_id: context.scope.project_id,
            root: context.project_root.clone(),
            config_path: context.project_root.join(".config/mm.yaml"),
        },
        project_name: "remote-test".to_owned(),
        target: context.scope.target.clone(),
        environment: context.scope.environment.clone(),
        config_hash: "a".repeat(64),
        target_source: Some("test".to_owned()),
        runtime_kind: Some(context.scope.runtime),
        runtime_source: Some("test".to_owned()),
        runtime: mm::domain::runtime::RuntimeProbe {
            kind: context.scope.runtime,
            available: true,
            engine_version: Some("test".to_owned()),
            compose_provider: None,
            rootless: Some(true),
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
