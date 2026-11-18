use std::collections::{BTreeMap, VecDeque};
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use mm::adapters::runtime::{RuntimeOperationsAdapter, TcpIngressVerifier};
use mm::app::LocalRuntimeOperationAuthorizer;
use mm::app::ports::{
    CommandOutput, CommandSpec, IngressVerifier, ProcessOutputSource, ProcessRunner, ProcessStream,
    ProcessStreamEvent, RedactedCommandRecord, RuntimeJobDriver, RuntimeOperationAuthorizer,
    RuntimeOperationDriver, StreamCommandSpec, StreamingProcessRunner,
};
use mm::domain::error::MmError;
use mm::domain::observability::{
    ApprovalContext, AuthorizationRequest, ExecEvent, ExecRequest, ExecStdinMode, ExecTtyMode,
    LogLimits, LogMode, LogOutputPolicy, LogRequest, OperationCancellation, ResolvedExecRequest,
    RuntimeOperation, ServiceSelector, validate_exec_request,
};
use mm::domain::runtime::{RuntimeEnvironment, RuntimeKind, RuntimeTarget};
use mm::domain::scheduler::{JobLaunch, RuntimeJobState, runtime_job_name};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;
use zeroize::Zeroizing;

#[tokio::test]
async fn tcp_ingress_verifier_handles_listener_route_retry_and_malformed_responses()
-> Result<(), Box<dyn std::error::Error>> {
    let listener_response = b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_vec();
    let port = serve_ingress_responses(vec![listener_response])?;
    TcpIngressVerifier::new(1, Duration::ZERO, Duration::from_secs(1))
        .verify_listener(port)
        .await?;

    let route_response =
        b"HTTP/1.1 200 OK\r\nX-MM-Route-Verified: 1\r\nContent-Length: 0\r\n\r\n".to_vec();
    let port = serve_ingress_responses(vec![
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
        route_response,
    ])?;
    TcpIngressVerifier::new(2, Duration::from_millis(1), Duration::from_secs(1))
        .verify_route(port, "app.example.test", "/ready")
        .await?;

    let verifier = TcpIngressVerifier::new(1, Duration::ZERO, Duration::from_millis(50));
    assert!(verifier.verify_route(80, "bad\nhost", "/").await.is_err());
    let port = serve_ingress_responses(vec![b"malformed".to_vec()])?;
    assert!(verifier.verify_listener(port).await.is_err());
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let unavailable = listener.local_addr()?.port();
    drop(listener);
    assert!(verifier.verify_listener(unavailable).await.is_err());
    assert!(
        TcpIngressVerifier::new(0, Duration::ZERO, Duration::ZERO)
            .verify_listener(80)
            .await
            .is_err()
    );
    Ok(())
}

fn serve_ingress_responses(responses: Vec<Vec<u8>>) -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(&response);
        }
    });
    Ok(port)
}

struct QueueProcess {
    outputs: Mutex<VecDeque<CommandOutput>>,
    records: Mutex<Vec<RedactedCommandRecord>>,
}

#[async_trait::async_trait]
impl ProcessRunner for QueueProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.records
            .lock()
            .map_err(|_| state("process records"))?
            .push(specification.redacted_record());
        self.outputs
            .lock()
            .map_err(|_| state("process outputs"))?
            .pop_front()
            .ok_or_else(|| state("process output queue is empty"))
    }
}

impl QueueProcess {
    fn new(outputs: impl IntoIterator<Item = CommandOutput>) -> Arc<Self> {
        Arc::new(Self {
            outputs: Mutex::new(outputs.into_iter().collect()),
            records: Mutex::new(Vec::new()),
        })
    }

    fn records(&self) -> Result<Vec<RedactedCommandRecord>, MmError> {
        self.records
            .lock()
            .map(|records| records.clone())
            .map_err(|_| state("process records"))
    }
}

struct QueueStreaming {
    sessions: Mutex<VecDeque<Vec<ProcessStreamEvent>>>,
    records: Mutex<Vec<(RedactedCommandRecord, String)>>,
}

#[async_trait::async_trait]
impl StreamingProcessRunner for QueueStreaming {
    async fn spawn(
        &self,
        specification: StreamCommandSpec,
        cancellation: OperationCancellation,
    ) -> Result<ProcessStream, MmError> {
        self.records
            .lock()
            .map_err(|_| state("stream records"))?
            .push((
                specification.redacted_record(),
                format!("{specification:?}"),
            ));
        let events = self
            .sessions
            .lock()
            .map_err(|_| state("stream sessions"))?
            .pop_front()
            .ok_or_else(|| state("stream session queue is empty"))?;
        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        let task_cancellation = cancellation.clone();
        tokio::spawn(async move {
            for event in events {
                if task_cancellation.is_cancelled() || sender.send(event).await.is_err() {
                    break;
                }
            }
        });
        Ok(ProcessStream::new(receiver, cancellation))
    }
}

impl QueueStreaming {
    fn new(sessions: impl IntoIterator<Item = Vec<ProcessStreamEvent>>) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(sessions.into_iter().collect()),
            records: Mutex::new(Vec::new()),
        })
    }

    fn records(&self) -> Result<Vec<(RedactedCommandRecord, String)>, MmError> {
        self.records
            .lock()
            .map(|records| records.clone())
            .map_err(|_| state("stream records"))
    }
}

fn output(stdout: &str) -> CommandOutput {
    CommandOutput {
        code: Some(0),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(String::new()),
        truncated: false,
    }
}

fn failed_output(stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(1),
        stdout: Zeroizing::new(String::new()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

fn target(_kind: RuntimeKind) -> RuntimeTarget {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    RuntimeTarget {
        root: root.clone(),
        project_directory: root.clone(),
        target: None,
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "local".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "mm-demo-app".to_owned(),
        artifact_hash: "hash".to_owned(),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

fn approval(approved_high_risk: bool) -> ApprovalContext {
    ApprovalContext {
        principal: "developer".to_owned(),
        source: "test".to_owned(),
        approved_high_risk,
    }
}

fn log_request() -> LogRequest {
    LogRequest {
        selector: Some(ServiceSelector {
            stack: Some("app".to_owned()),
            module: Some("backend".to_owned()),
            service: "api".to_owned(),
            container: None,
        }),
        nodes: Vec::new(),
        tail: 12,
        since_unix_ms: Some(1_000),
        until_unix_ms: Some(2_000),
        follow: false,
        timestamps: true,
        output: LogOutputPolicy::Both,
        mode: LogMode::Redacted,
        limits: LogLimits {
            maximum_line_bytes: 16,
            buffered_records: 2,
            maximum_bytes_per_second: Some(1024),
            maximum_lines_per_second: Some(100),
        },
        approval: approval(false),
    }
}

#[tokio::test]
async fn compose_jobs_are_stable_owned_bounded_and_secret_safe()
-> Result<(), Box<dyn std::error::Error>> {
    const SENTINEL: &str = "job-runtime-secret-sentinel-42";
    let project_id = ProjectId(Uuid::from_u128(1));
    let run_id = Uuid::from_u128(2);
    let name = runtime_job_name(project_id, run_id);
    let container_id = "a".repeat(64);
    let inspect = serde_json::json!([{
        "Id": container_id.clone(),
        "Name": format!("/{name}"),
        "Config": { "Labels": { "com.docker.compose.project": "mm-demo-app" } },
        "State": { "Running": false, "ExitCode": 7 }
    }]);
    let process = QueueProcess::new([
        failed_output("Error: No such container"),
        output(&container_id),
        output(&inspect.to_string()),
        output(&format!("public line\n{SENTINEL}\n")),
        output(&inspect.to_string()),
        output(""),
    ]);
    let streaming = QueueStreaming::new([]);
    let adapter =
        RuntimeOperationsAdapter::new(RuntimeKind::DockerCompose, process.clone(), streaming);
    let launch = JobLaunch {
        run_id,
        fence_token: 9,
        runtime_name: name.clone(),
        runtime: RuntimeKind::DockerCompose,
        target: target(RuntimeKind::DockerCompose),
        module: "backend".to_owned(),
        service: "api".to_owned(),
        argv: vec!["bin/job".to_owned(), "--once".to_owned()],
        user: None,
        timeout: Duration::from_secs(30),
        output_limit_bytes: 1024,
        environment: RuntimeEnvironment::from_sensitive_values(BTreeMap::from([
            ("MM_JOB_RUN_ID".to_owned(), run_id.to_string()),
            ("SECRET_RUNTIME_VALUE".to_owned(), SENTINEL.to_owned()),
        ])),
        redactions: vec![Zeroizing::new(SENTINEL.to_owned())],
    };
    let debug = format!("{launch:?}");
    assert!(debug.contains("argv_count: 2"));
    assert!(debug.contains("redaction_count: 1"));
    assert!(debug.contains("SECRET_RUNTIME_VALUE"));
    assert!(!debug.contains(SENTINEL));
    assert!(!debug.contains("bin/job"));
    let runtime_id = adapter.launch(launch).await?;
    assert_eq!(runtime_id, container_id);
    let state = adapter
        .inspect(
            &target(RuntimeKind::DockerCompose),
            &name,
            Some(&runtime_id),
            1024,
            vec![Zeroizing::new(SENTINEL.to_owned())],
        )
        .await?;
    assert!(matches!(
        state,
        RuntimeJobState::Exited { exit_code: 7, ref output, truncated: false }
            if output.contains("public line") && output.contains("[REDACTED]") && !output.contains(SENTINEL)
    ));
    adapter
        .cleanup(
            &target(RuntimeKind::DockerCompose),
            &name,
            Some(&runtime_id),
        )
        .await?;
    let records = process.records()?;
    let encoded = serde_json::to_string(&records)?;
    assert!(!encoded.contains(SENTINEL));
    assert!(records.iter().any(|record| {
        record.arguments.contains(&"run".to_owned())
            && record.arguments.contains(&"--detach".to_owned())
            && record.arguments.contains(&"--no-deps".to_owned())
            && record
                .environment_keys
                .contains(&"SECRET_RUNTIME_VALUE".to_owned())
    }));
    Ok(())
}

#[tokio::test]
async fn podman_missing_container_exit_allows_first_job_launch()
-> Result<(), Box<dyn std::error::Error>> {
    let run_id = Uuid::from_u128(20);
    let runtime_name = runtime_job_name(ProjectId(Uuid::from_u128(1)), run_id);
    let container_id = "b".repeat(64);
    let process = QueueProcess::new([
        CommandOutput {
            code: Some(125),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(format!(
                "Error: no container with name or ID {runtime_name} found: no such container"
            )),
            truncated: false,
        },
        output(&container_id),
    ]);
    let adapter =
        RuntimeOperationsAdapter::new(RuntimeKind::PodmanCompose, process, QueueStreaming::new([]));

    let launched = adapter
        .launch(JobLaunch {
            run_id,
            fence_token: 1,
            runtime_name,
            runtime: RuntimeKind::PodmanCompose,
            target: target(RuntimeKind::PodmanCompose),
            module: "backend".to_owned(),
            service: "api".to_owned(),
            argv: vec!["bin/job".to_owned()],
            user: None,
            timeout: Duration::from_secs(30),
            output_limit_bytes: 1024,
            environment: RuntimeEnvironment::default(),
            redactions: Vec::new(),
        })
        .await?;
    assert_eq!(launched, container_id);
    Ok(())
}

#[tokio::test]
async fn every_provider_generates_bounded_log_commands_and_redacts_split_non_utf8_lines()
-> Result<(), Box<dyn std::error::Error>> {
    for kind in RuntimeKind::ALL {
        let process = QueueProcess::new([]);
        let streaming = QueueStreaming::new([vec![
            ProcessStreamEvent::Output {
                source: ProcessOutputSource::Stdout,
                bytes: b"SECRET o".to_vec(),
            },
            ProcessStreamEvent::Output {
                source: ProcessOutputSource::Stdout,
                bytes: vec![b'k', 0xff, b'\n'],
            },
            ProcessStreamEvent::Output {
                source: ProcessOutputSource::Stderr,
                bytes: b"12345678901234567890\n".to_vec(),
            },
            ProcessStreamEvent::Exited { code: Some(0) },
        ]]);
        let driver = RuntimeOperationsAdapter::new(kind, process, streaming.clone());
        let mut stream = driver
            .logs(
                &target(kind),
                log_request(),
                vec![Zeroizing::new("SECRET".to_owned())],
                OperationCancellation::default(),
            )
            .await?;
        let first = stream
            .recv()
            .await
            .ok_or_else(|| state("missing first log"))??;
        let second = stream
            .recv()
            .await
            .ok_or_else(|| state("missing second log"))??;
        assert_eq!(first.message, "[REDACTED] ok�");
        assert!(first.lossy_utf8);
        assert_eq!(second.message, "1234567890123456");
        assert!(second.truncated);
        let records = streaming.records()?;
        let args = &records[0].0.arguments;
        assert!(args.windows(2).any(|pair| pair == ["--tail", "12"]));
        assert!(args.windows(2).any(|pair| pair == ["--since", "1.000"]));
        assert!(args.windows(2).any(|pair| pair == ["--until", "2.000"]));
        assert!(args.contains(&"--timestamps".to_owned()));
        assert!(!serde_json::to_string(&records)?.contains("SECRET"));
    }
    Ok(())
}

#[tokio::test]
async fn follow_reconnect_is_bounded_and_pre_cancel_never_spawns()
-> Result<(), Box<dyn std::error::Error>> {
    let streaming =
        QueueStreaming::new((0..4).map(|_| vec![ProcessStreamEvent::Exited { code: Some(0) }]));
    let driver = RuntimeOperationsAdapter::new(
        RuntimeKind::DockerCompose,
        QueueProcess::new([]),
        streaming.clone(),
    );
    let mut request = log_request();
    request.follow = true;
    let mut stream = driver
        .logs(
            &target(RuntimeKind::DockerCompose),
            request.clone(),
            Vec::new(),
            OperationCancellation::default(),
        )
        .await?;
    let error = match stream
        .recv()
        .await
        .ok_or_else(|| state("missing reconnect error"))?
    {
        Ok(_) => {
            return Err(Box::<dyn std::error::Error>::from(state(
                "bounded reconnect unexpectedly succeeded",
            )));
        }
        Err(error) => error,
    };
    assert_eq!(error.code(), "MM_RUNTIME_RESPONSE");
    assert_eq!(streaming.records()?.len(), 4);

    let cancelled = OperationCancellation::default();
    cancelled.cancel();
    let empty = QueueStreaming::new([]);
    let driver = RuntimeOperationsAdapter::new(
        RuntimeKind::DockerCompose,
        QueueProcess::new([]),
        empty.clone(),
    );
    let mut stream = driver
        .logs(
            &target(RuntimeKind::DockerCompose),
            request,
            Vec::new(),
            cancelled,
        )
        .await?;
    assert!(stream.recv().await.is_none());
    assert!(empty.records()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn high_volume_is_stopped_by_rate_and_backpressure_bounds()
-> Result<(), Box<dyn std::error::Error>> {
    let streaming = QueueStreaming::new([vec![ProcessStreamEvent::Output {
        source: ProcessOutputSource::Stdout,
        bytes: b"one\ntwo\nthree\nfour\n".to_vec(),
    }]]);
    let driver =
        RuntimeOperationsAdapter::new(RuntimeKind::DockerCompose, QueueProcess::new([]), streaming);
    let mut request = log_request();
    request.limits.maximum_lines_per_second = Some(2);
    let mut stream = driver
        .logs(
            &target(RuntimeKind::DockerCompose),
            request,
            Vec::new(),
            OperationCancellation::default(),
        )
        .await?;
    assert_eq!(
        stream
            .recv()
            .await
            .ok_or_else(|| state("missing first bounded record"))??
            .message,
        "one"
    );
    assert_eq!(
        stream
            .recv()
            .await
            .ok_or_else(|| state("missing second bounded record"))??
            .message,
        "two"
    );
    let error = stream
        .recv()
        .await
        .ok_or_else(|| state("missing rate-limit error"))?
        .err()
        .ok_or_else(|| state("high-volume stream unexpectedly succeeded"))?;
    assert_eq!(error.code(), "MM_RUNTIME_RESPONSE");
    Ok(())
}

#[tokio::test]
async fn exec_preserves_argv_options_and_remote_exit_without_a_shell()
-> Result<(), Box<dyn std::error::Error>> {
    let container = "0123456789abcdef0123456789abcdef";
    let process = QueueProcess::new([output(&format!(
        r#"[{{"ID":"{container}","State":"running"}}]"#
    ))]);
    let streaming = QueueStreaming::new([vec![
        ProcessStreamEvent::Output {
            source: ProcessOutputSource::Stdout,
            bytes: b"ok\n".to_vec(),
        },
        ProcessStreamEvent::Exited { code: Some(42) },
    ]]);
    let driver = RuntimeOperationsAdapter::new(
        RuntimeKind::DockerCompose,
        process.clone(),
        streaming.clone(),
    );
    let request = ExecRequest {
        selector: ServiceSelector {
            stack: Some("app".to_owned()),
            module: Some("backend".to_owned()),
            service: "api".to_owned(),
            container: None,
        },
        argv: vec!["printf".to_owned(), "; touch /tmp/injected".to_owned()],
        user: Some("1000:1000".to_owned()),
        workdir: Some(PathBuf::from("/workspace")),
        environment: Vec::new(),
        tty: ExecTtyMode::Disabled,
        stdin: ExecStdinMode::Closed,
        timeout: Duration::from_secs(9),
        approval: approval(false),
        shell_wrapper: false,
    };
    let mut stream = driver
        .exec(
            &target(RuntimeKind::DockerCompose),
            ResolvedExecRequest {
                request,
                environment: RuntimeEnvironment::default(),
            },
            OperationCancellation::default(),
        )
        .await?;
    let mut exit = None;
    while let Some(event) = stream.recv().await {
        if let ExecEvent::Finished(outcome) = event? {
            exit = Some(outcome.exit_code);
        }
    }
    assert_eq!(exit, Some(42));
    let records = streaming.records()?;
    assert_eq!(records[0].0.program, "docker");
    assert!(
        records[0]
            .0
            .arguments
            .windows(2)
            .any(|pair| pair == ["--user", "1000:1000"])
    );
    assert!(
        records[0]
            .0
            .arguments
            .windows(2)
            .any(|pair| pair == ["--workdir", "/workspace"])
    );
    assert_eq!(
        records[0].0.arguments.last().map(String::as_str),
        Some("; touch /tmp/injected")
    );
    assert!(!records[0].0.arguments.contains(&"sh".to_owned()));
    assert!(
        process.records()?[0]
            .arguments
            .contains(&"status=running".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn exec_reports_the_selected_container_and_secret_environment_never_enters_argv()
-> Result<(), Box<dyn std::error::Error>> {
    let container = "fedcbafedcbafedcbafedcbafe";
    let process = QueueProcess::new([output(&format!(r#"{{"ID":"{container}"}}"#))]);
    let streaming = QueueStreaming::new([vec![ProcessStreamEvent::Exited { code: Some(0) }]]);
    let driver =
        RuntimeOperationsAdapter::new(RuntimeKind::DockerCompose, process, streaming.clone());
    let request = ExecRequest {
        selector: ServiceSelector {
            stack: Some("app".to_owned()),
            module: Some("backend".to_owned()),
            service: "api".to_owned(),
            container: None,
        },
        argv: vec!["env".to_owned()],
        user: None,
        workdir: None,
        environment: Vec::new(),
        tty: ExecTtyMode::Disabled,
        stdin: ExecStdinMode::Closed,
        timeout: Duration::from_secs(10),
        approval: approval(false),
        shell_wrapper: false,
    };
    let environment = RuntimeEnvironment::from_sensitive_values(BTreeMap::from([(
        "TOKEN".to_owned(),
        "secret-sentinel".to_owned(),
    )]));
    let mut stream = driver
        .exec(
            &target(RuntimeKind::DockerCompose),
            ResolvedExecRequest {
                request,
                environment,
            },
            OperationCancellation::default(),
        )
        .await?;
    let selected = stream
        .recv()
        .await
        .ok_or_else(|| state("missing selection"))??;
    assert!(
        matches!(selected, ExecEvent::Selected { container_id, .. } if container_id == container)
    );
    let encoded = serde_json::to_string(&streaming.records()?)?;
    assert!(encoded.contains("TOKEN"));
    assert!(!encoded.contains("secret-sentinel"));
    assert!(
        streaming.records()?[0]
            .0
            .arguments
            .windows(2)
            .any(|pair| pair == ["--env", "TOKEN"])
    );
    Ok(())
}

#[test]
fn validation_and_authorization_fail_closed_before_adapters() {
    let authorizer = LocalRuntimeOperationAuthorizer;
    let denied = authorizer.authorize(&AuthorizationRequest {
        principal: "developer".to_owned(),
        source: "test".to_owned(),
        operation: RuntimeOperation::Shell,
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "local".to_owned(),
        selector: None,
        approved_high_risk: false,
    });
    assert!(matches!(denied, Err(MmError::Security { .. })));

    let invalid = ExecRequest {
        selector: ServiceSelector {
            stack: None,
            module: None,
            service: "api".to_owned(),
            container: None,
        },
        argv: vec!["sh\0bad".to_owned()],
        user: None,
        workdir: None,
        environment: Vec::new(),
        tty: ExecTtyMode::Disabled,
        stdin: ExecStdinMode::Closed,
        timeout: Duration::from_secs(1),
        approval: approval(false),
        shell_wrapper: false,
    };
    assert!(validate_exec_request(&invalid).is_err());
}

fn state(message: &str) -> MmError {
    MmError::State {
        message: message.to_owned(),
    }
}

// Found on the test VMs: restarting one container of a cluster service from
// the terminal UI failed on its node with "the requested container is not
// owned by the selected service". The UI sends the container's full id (as
// a status read has it), and `docker compose ps --format json` lists the
// short one.
#[tokio::test]
async fn a_container_named_by_its_full_id_is_restarted() -> Result<(), Box<dyn std::error::Error>> {
    let full = "1cc6724642f6aa0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b";
    let process = QueueProcess::new([output(r#"{"ID":"1cc6724642f6"}"#), output("")]);
    let streaming = QueueStreaming::new([]);
    let driver =
        RuntimeOperationsAdapter::new(RuntimeKind::DockerCompose, process.clone(), streaming);
    let outcome = driver
        .restart(
            &target(RuntimeKind::DockerCompose),
            &ServiceSelector {
                stack: Some("app".to_owned()),
                module: Some("web".to_owned()),
                service: "web".to_owned(),
                container: Some(full.to_owned()),
            },
        )
        .await?;
    assert_eq!(outcome.containers, ["1cc6724642f6"]);
    let records = process.records()?;
    let restart = records.last().ok_or("no restart command")?;
    assert!(
        restart
            .arguments
            .iter()
            .any(|argument| argument == "1cc6724642f6"),
        "{:?}",
        restart.arguments
    );
    Ok(())
}
