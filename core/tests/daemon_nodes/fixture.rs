//! A daemon for these tests: the use cases a host would run, scripted, a
//! host policy with one enrolled project and a break-glass key, an origin
//! to fetch from, and a client that speaks the relay's protocol. The same
//! as the fixture of the daemon's unit tests, which keep it for what they
//! check of the daemon's private parts.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use mm::adapters::remote::{
    DaemonConfig, RemoteDaemon, decode_frame, encode_frame, read_raw_frame, write_raw_frame,
};
use mm::app::ports::{ExecStream, LogStream, RemoteRequestContext, RemoteUseCases};
use mm::domain::build::BuildReceipt;
use mm::domain::deployment::{
    DEPLOYMENT_PLAN_SCHEMA, DeploymentPlan, DeploymentResult, DeploymentRouteDiff,
};
use mm::domain::error::MmError;
use mm::domain::observability::{
    ExecEvent, ExecOutputSource, ExecRequest, LogRecord, LogRequest, LogSource,
    OperationCancellation,
};
use mm::domain::remote::{
    ClientFrame, ClientHello, PROTOCOL_VERSION, RemoteMethod, RemoteScope, ServerFrame,
};
use mm::domain::runtime::{RuntimeKind, ServiceIdentity};
use mm::domain::target::TargetId;
use mm::domain::traefik_dynamic::{PreparedTraefikDynamicConfig, TraefikDynamicConfigManifest};
use mm::domain::workspace::{ProjectId, WorkspaceSnapshot};
use tokio::net::UnixStream;

async fn read_frame<R, T>(reader: &mut R) -> Result<Option<T>, MmError>
where
    R: tokio::io::AsyncRead + Unpin + ?Sized,
    T: serde::de::DeserializeOwned,
{
    read_raw_frame(reader)
        .await?
        .map(|bytes| decode_frame(&bytes))
        .transpose()
}

async fn write_frame<W, T>(writer: &mut W, frame: &T) -> Result<(), MmError>
where
    W: tokio::io::AsyncWrite + Unpin + ?Sized,
    T: serde::Serialize,
{
    write_raw_frame(writer, &encode_frame(frame)?).await
}

pub(super) struct FakeUseCases {
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) stopped: Arc<StoppedSteps>,
}

/// What the host does after a step was stopped, as the fake sees it. The
/// stack `draining` has a container in its drain until its step is
/// stopped.
#[derive(Default)]
pub(super) struct StoppedSteps {
    /// The stacks whose leaves were taken back, in order.
    pub(super) taken_back: std::sync::Mutex<Vec<String>>,
    /// How many rollbacks began.
    pub(super) rollbacks: std::sync::atomic::AtomicUsize,
    /// While set, taking leaves back takes its time.
    pub(super) slow_take_back: AtomicBool,
    /// While set, a rollback takes its time.
    pub(super) slow_rollback: AtomicBool,
}

impl StoppedSteps {
    pub(super) fn taken_back(&self) -> Vec<String> {
        self.taken_back
            .lock()
            .map(|taken| taken.clone())
            .unwrap_or_default()
    }

    async fn wait_while(flag: &AtomicBool) {
        while flag.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

#[async_trait]
impl RemoteUseCases for FakeUseCases {
    async fn mesh_status(
        &self,
        _context: &RemoteRequestContext,
    ) -> Result<mm::domain::mesh::MeshNodeStatus, MmError> {
        Ok(mm::domain::mesh::MeshNodeStatus {
            interface: Some("mm0".to_owned()),
            up: true,
            port: Some(51820),
            ..mm::domain::mesh::MeshNodeStatus::default()
        })
    }

    async fn status(&self, context: &RemoteRequestContext) -> Result<WorkspaceSnapshot, MmError> {
        if context.request_id == uuid::Uuid::from_u128(700) {
            return Err(unused());
        }
        Ok(workspace_snapshot(context))
    }

    async fn logs(
        &self,
        context: &RemoteRequestContext,
        _request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        // Nothing of the project runs on app-2: a read of a service that
        // is placed on other nodes finds no container and so no line.
        if context.scope.node.as_deref() == Some("app-2") {
            return Ok(LogStream::new(receiver, cancellation));
        }
        let project_id = context.scope.project_id;
        let cancelled = Arc::clone(&self.cancelled);
        let task_cancellation = cancellation.clone();
        tokio::spawn(async move {
            let _ = sender
                .send(Ok(LogRecord {
                    identity: ServiceIdentity {
                        project_id,
                        environment: "production".to_owned(),
                        stack: "app".to_owned(),
                        module: Some("api".to_owned()),
                        service: "web".to_owned(),
                    },
                    sequence: 1,
                    source: LogSource::Stdout,
                    node: None,
                    container: None,
                    timestamp: None,
                    message: "bounded".to_owned(),
                    lossy_utf8: false,
                    truncated: false,
                    reconnect: 0,
                }))
                .await;
            while !task_cancellation.is_cancelled() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            cancelled.store(true, Ordering::SeqCst);
        });
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        context: &RemoteRequestContext,
        _request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        sender
            .send(Ok(ExecEvent::Output {
                source: ExecOutputSource::Stdout,
                value: format!("{}-exec", context.principal),
                lossy_utf8: false,
            }))
            .await
            .map_err(|_| unused())?;
        drop(sender);
        Ok(ExecStream::new(receiver, cancellation))
    }

    async fn plan_deployment(
        &self,
        context: &RemoteRequestContext,
        receipt: Option<&BuildReceipt>,
    ) -> Result<DeploymentPlan, MmError> {
        let mut plan = deployment_plan(context)?;
        plan.build_receipt = receipt.map(BuildReceipt::origin);
        // On app-2 the engine finds a service that moved to app-1.
        if context.scope.node.as_deref() == Some("app-2") {
            plan.node = Some(mm::domain::deployment::NodeProjectPlan {
                node: "app-2".to_owned(),
                services: Vec::new(),
                starting: Vec::new(),
                leaving: vec![mm::domain::deployment::LeavingService {
                    stack: "app".to_owned(),
                    service: "worker".to_owned(),
                    compose_project_name: "example-production-app".to_owned(),
                    moves_to: vec!["app-1".to_owned()],
                }],
                dropped_dependencies: Vec::new(),
                networks: Vec::new(),
                restart_defaults: Vec::new(),
                shared_volumes: Vec::new(),
                ingress: None,
                scaled: Vec::new(),
                health: Vec::new(),
            });
        }
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Security { message })?;
        Ok(plan)
    }

    async fn finish_moves(
        &self,
        context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
    ) -> Result<Vec<mm::domain::placement::PlacedService>, MmError> {
        assert!(
            context.project_root.is_absolute(),
            "the step runs in the checkout of the applied commit"
        );
        Ok(vec![mm::domain::placement::PlacedService {
            stack: "app".to_owned(),
            service: "worker".to_owned(),
        }])
    }

    async fn begin_rollout(
        &self,
        context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _receipt: Option<&BuildReceipt>,
    ) -> Result<(), MmError> {
        assert!(
            context.project_root.is_absolute(),
            "a rollout runs in the checkout of the planned commit"
        );
        Ok(())
    }

    /// The stack named `sick` never becomes healthy; `draining` waits for
    /// its old container's drain until the step is stopped.
    async fn roll_stack(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        stack: &str,
        _step: mm::app::StackStep,
    ) -> Result<mm::app::StackOutcome, MmError> {
        if stack == "draining" {
            std::future::pending::<()>().await;
        }
        if stack == "sick" {
            return Ok(mm::app::StackOutcome::Failed(
                mm::domain::rollout::StepFailure {
                    stack: stack.to_owned(),
                    service: Some("web".to_owned()),
                    reason: "its health check fails".to_owned(),
                    log: vec!["cannot reach the database".to_owned()],
                    code: None,
                },
            ));
        }
        Ok(mm::app::StackOutcome::Done {
            started: Vec::new(),
            unchanged: false,
            addresses: vec![std::net::Ipv4Addr::new(10, 64, 0, 2)],
        })
    }

    async fn finish_rollout(
        &self,
        context: &RemoteRequestContext,
        deployment_plan_hash: &str,
    ) -> Result<DeploymentResult, MmError> {
        self.apply_deployment(context, deployment_plan_hash, None)
            .await
    }

    async fn rollback_rollout(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        _changes: Option<&mm::domain::rollout::RolloutChanges>,
    ) -> Result<bool, MmError> {
        self.stopped.rollbacks.fetch_add(1, Ordering::SeqCst);
        StoppedSteps::wait_while(&self.stopped.slow_rollback).await;
        Ok(true)
    }

    async fn take_back_leaves(
        &self,
        _context: &RemoteRequestContext,
        _deployment_plan_hash: &str,
        stack: &str,
    ) -> Result<Vec<String>, MmError> {
        self.stopped
            .taken_back
            .lock()
            .map_err(|_| unused())?
            .push(stack.to_owned());
        StoppedSteps::wait_while(&self.stopped.slow_take_back).await;
        Ok(vec![format!("example-production-{stack}-web-1")])
    }

    async fn apply_deployment(
        &self,
        context: &RemoteRequestContext,
        deployment_plan_hash: &str,
        receipt: Option<&BuildReceipt>,
    ) -> Result<DeploymentResult, MmError> {
        // Reports the receipt it was handed in place of a stack name.
        let stack = receipt.map_or_else(
            || "app".to_owned(),
            |receipt| format!("receipt:{}", receipt.receipt_hash),
        );
        Ok(DeploymentResult {
            plan_hash: deployment_plan_hash.to_owned(),
            target_name: context.scope.target.clone(),
            target_id: TargetId(uuid::Uuid::from_u128(94)),
            environment: context.scope.environment.clone(),
            applied_stacks: vec![stack],
            removed_stacks: Vec::new(),
            route_generation: 0,
            route_count: 0,
            moved_away: Vec::new(),
            jobs: Vec::new(),
        })
    }
}

/// One request in `scope`, and the first frame the daemon answers with.
pub(super) async fn call_in(
    daemon: &Arc<RemoteDaemon>,
    id: u128,
    scope: RemoteScope,
    method: RemoteMethod,
) -> Result<ServerFrame, Box<dyn std::error::Error>> {
    let (task, mut client) = connected_client(daemon).await?;
    write_frame(
        &mut client,
        &ClientFrame::Request {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::from_u128(id),
            operation_nonce: uuid::Uuid::from_u128(id + 1),
            deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
            scope,
            method: Box::new(method),
        },
    )
    .await?;
    let frame = read_frame::<_, ServerFrame>(&mut client)
        .await?
        .ok_or("daemon closed without a response")?;
    task.await??;
    Ok(frame)
}

/// Sends one request in `scope` and returns the session, still open: the
/// caller reads the answer with `next_frame`, or hangs up by dropping it.
pub(super) async fn ask_in(
    daemon: &Arc<RemoteDaemon>,
    id: u128,
    scope: RemoteScope,
    method: RemoteMethod,
) -> Result<(tokio::task::JoinHandle<Result<(), MmError>>, UnixStream), Box<dyn std::error::Error>>
{
    let (task, mut client) = connected_client(daemon).await?;
    write_frame(
        &mut client,
        &ClientFrame::Request {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::from_u128(id),
            operation_nonce: uuid::Uuid::from_u128(id + 1),
            deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
            scope,
            method: Box::new(method),
        },
    )
    .await?;
    Ok((task, client))
}

/// The next frame the daemon sends on a session.
pub(super) async fn next_frame(
    client: &mut UnixStream,
) -> Result<ServerFrame, Box<dyn std::error::Error>> {
    Ok(read_frame::<_, ServerFrame>(client)
        .await?
        .ok_or("daemon closed without a response")?)
}

/// Sends a request and stops reading at once, as a client whose way back
/// broke the moment it asked: the daemon's answer cannot reach it. Returns
/// once the daemon ended the session.
pub(super) async fn ask_and_stop_reading(
    daemon: &Arc<RemoteDaemon>,
    id: u128,
    scope: RemoteScope,
    method: RemoteMethod,
) -> Result<(), Box<dyn std::error::Error>> {
    let (task, mut client) = connected_client(daemon).await?;
    write_frame(
        &mut client,
        &ClientFrame::Request {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::from_u128(id),
            operation_nonce: uuid::Uuid::from_u128(id + 1),
            deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
            scope,
            method: Box::new(method),
        },
    )
    .await?;
    let client = client.into_std()?;
    client.shutdown(std::net::Shutdown::Read)?;
    // The client is still connected; only its way back is gone.
    let _client = UnixStream::from_std(client)?;
    let served = tokio::time::timeout(std::time::Duration::from_secs(10), task)
        .await
        .map_err(|_| "the daemon did not end the session")?;
    // Its answer could not be written, so the session may end in an error.
    drop(served);
    Ok(())
}

/// As `call_in`, with every frame the daemon answers with until it ends
/// the request.
pub(super) async fn frames_in(
    daemon: &Arc<RemoteDaemon>,
    id: u128,
    scope: RemoteScope,
    method: RemoteMethod,
) -> Result<Vec<ServerFrame>, Box<dyn std::error::Error>> {
    let (task, mut client) = connected_client(daemon).await?;
    write_frame(
        &mut client,
        &ClientFrame::Request {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::from_u128(id),
            operation_nonce: uuid::Uuid::from_u128(id + 1),
            deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
            scope,
            method: Box::new(method),
        },
    )
    .await?;
    let mut frames = Vec::new();
    loop {
        let frame = read_frame::<_, ServerFrame>(&mut client)
            .await?
            .ok_or("daemon closed before it ended the request")?;
        let last = matches!(frame, ServerFrame::End { .. } | ServerFrame::Error { .. });
        frames.push(frame);
        if last {
            break;
        }
    }
    task.await??;
    Ok(frames)
}

/// Break-glass key the test sessions authenticate with: admin, exec,
/// and forward on every enrolled project.
const OPS_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 ops@desk";

fn ops_fingerprint() -> String {
    mm::domain::host_policy::parse_public_key(OPS_KEY)
        .map(|key| key.fingerprint)
        .unwrap_or_default()
}

fn git(
    directory: &std::path::Path,
    arguments: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
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

pub(super) fn origin(
    path: &std::path::Path,
) -> Result<(std::path::PathBuf, String), Box<dyn std::error::Error>> {
    let bare = path.join("origin.git");
    let commit_file = path.join("origin.commit");
    if bare.is_dir() {
        return Ok((bare, std::fs::read_to_string(commit_file)?));
    }
    let work = path.join("origin-work");
    std::fs::create_dir_all(work.join(".config"))?;
    git(&work, &["init", "--quiet", "--initial-branch=main"])?;
    std::fs::write(
        work.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: {}\n  name: example\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nenvironments:\n  production: {{}}\nmodules:\n  api:\n    path: .\n    services:\n      web: {{stack: app}}\ntargets:\n  cluster:\n    nodes:\n      - {{name: app-1, host: 203.0.113.11}}\n      - {{name: app-2, host: 203.0.113.12}}\n",
            uuid::Uuid::from_u128(4)
        ),
    )?;
    std::fs::write(
        work.join("compose.yaml"),
        "services:\n  web:\n    image: busybox\n",
    )?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "initial"])?;
    let commit = git(&work, &["rev-parse", "HEAD"])?;
    git(
        path,
        &["clone", "--quiet", "--bare", "origin-work", "origin.git"],
    )?;
    std::fs::write(commit_file, &commit)?;
    Ok((bare, commit))
}

pub(super) fn config(
    path: &std::path::Path,
    root: &std::path::Path,
    uid: u32,
    gid: u32,
) -> Result<DaemonConfig, Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let (origin, _) = origin(path)?;
    let etc = path.join(format!("etc-{uid}"));
    std::fs::create_dir_all(&etc)?;
    std::fs::set_permissions(&etc, std::fs::Permissions::from_mode(0o700))?;
    let host_policy_path = etc.join("host.yaml");
    std::fs::write(
        &host_policy_path,
        format!(
            "schema-version: 1\nbreak-glass:\n  - name: ops\n    key: {OPS_KEY}\nprojects:\n  {}:\n    name: example\n    origin: {}\n    environments: [production]\n",
            uuid::Uuid::from_u128(4),
            origin.display()
        ),
    )?;
    std::fs::set_permissions(&host_policy_path, std::fs::Permissions::from_mode(0o600))?;
    let state_root = path.join(format!("state-{uid}"));
    let project = ProjectId(uuid::Uuid::from_u128(4));
    let deployed = state_root
        .join("projects")
        .join(project.0.to_string())
        .join("deployed");
    std::fs::create_dir_all(&deployed)?;
    std::fs::write(
        deployed.join("production.json"),
        serde_json::to_vec(&mm::adapters::remote::DeployedState {
            project_id: project,
            environment: "production".to_owned(),
            commit: "a".repeat(40),
            root: std::fs::canonicalize(root)?,
            deployed_at_unix_ms: 1,
            deployed_by: "test".to_owned(),
            runs_services: true,
        })?,
    )?;
    Ok(DaemonConfig {
        schema_version: 1,
        socket_path: path.join("daemon.sock"),
        state_root,
        mm_home: path.join(format!("home-{uid}")),
        socket_uid: uid,
        socket_gid: gid,
        idle_timeout_ms: 1_000,
        host_policy_path,
        relay_uid: uid,
        authorized_keys_path: path.join(format!("ssh-{uid}/authorized_keys")),
        relay_command: "/usr/bin/mm relay serve --stdio".to_owned(),
        projects_root: path.join(format!("projects-{uid}")),
        node_env_path: path.join(format!("node-{uid}.env")),
    })
}

async fn send_session(stream: &mut UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    write_frame(
        stream,
        &ClientFrame::RelaySession {
            version: PROTOCOL_VERSION,
            key_fingerprint: ops_fingerprint(),
            source: "127.0.0.1 10000 127.0.0.1 22".to_owned(),
            session_id: uuid::Uuid::from_u128(89),
        },
    )
    .await?;
    Ok(())
}

pub(super) async fn connected_client(
    daemon: &Arc<RemoteDaemon>,
) -> Result<(tokio::task::JoinHandle<Result<(), MmError>>, UnixStream), Box<dyn std::error::Error>>
{
    let fingerprint = daemon.identity().fingerprint.clone();
    let (server, mut client) = unix_pair()?;
    let task = tokio::spawn({
        let daemon = Arc::clone(daemon);
        async move { daemon.serve_stream(server).await }
    });
    send_session(&mut client).await?;
    let client_nonce = uuid::Uuid::from_u128(95);
    write_frame(
        &mut client,
        &ClientFrame::Hello {
            version: PROTOCOL_VERSION,
            minimum: PROTOCOL_VERSION,
            hello: ClientHello {
                client_nonce,
                expected_daemon_fingerprint: fingerprint,
                client_version: String::new(),
            },
        },
    )
    .await?;
    if !matches!(
        read_frame::<_, ServerFrame>(&mut client).await?,
        Some(ServerFrame::Hello { hello, .. }) if hello.client_nonce == client_nonce
    ) {
        return Err("daemon did not complete the test handshake".into());
    }
    Ok((task, client))
}

pub(super) fn remote_scope(_root: std::path::PathBuf) -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(uuid::Uuid::from_u128(4)),
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    }
}

fn deployment_plan(context: &RemoteRequestContext) -> Result<DeploymentPlan, MmError> {
    let target_id = TargetId(uuid::Uuid::from_u128(94));
    let dynamic = PreparedTraefikDynamicConfig {
        root: context.project_root.join("dynamic"),
        path: context.project_root.join("dynamic/dynamic.yaml"),
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
        adopted_route_claims: Vec::new(),
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
        stack_after: Default::default(),
        config_files: Vec::new(),
        runtime_changes: Vec::new(),
        unknown_config_keys: Vec::new(),
        secret_file_owners: Vec::new(),
        healthcheck_arguments: Vec::new(),
        release: None,
        plan_hash: String::new(),
    };
    plan.plan_hash = plan
        .canonical_hash()
        .map_err(|message| MmError::Security { message })?;
    Ok(plan)
}

fn unix_pair() -> Result<(UnixStream, UnixStream), std::io::Error> {
    let (left, right) = std::os::unix::net::UnixStream::pair()?;
    left.set_nonblocking(true)?;
    right.set_nonblocking(true)?;
    Ok((UnixStream::from_std(left)?, UnixStream::from_std(right)?))
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

fn unused() -> MmError {
    MmError::State {
        message: "unused fake use case".to_owned(),
    }
}
