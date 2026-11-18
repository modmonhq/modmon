//! Reads from the engine's API on its socket: the containers of a project
//! for a status, and a service's log lines from every container, with no
//! engine process started.
//!
//! The engine here is a small HTTP server on a Unix socket that answers
//! the way Docker Engine does: a JSON list, an inspect document per
//! container, and a chunked log answer of framed messages.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm::adapters::engine_api::EngineApi;
use mm::adapters::runtime::RuntimeOperationsAdapter;
use mm::adapters::runtime::engine_containers::project_containers;
use mm::adapters::runtime::engine_logs::LogDemux;
use mm::app::ports::{
    CommandOutput, CommandSpec, ProcessRunner, ProcessStream, RuntimeOperationDriver,
    StreamCommandSpec, StreamingProcessRunner,
};
use mm::domain::error::MmError;
use mm::domain::observability::{
    ApprovalContext, LogRecord, LogRequest, LogSource, OperationCancellation, ServiceSelector,
};
use mm::domain::runtime::{RuntimeEnvironment, RuntimeKind, RuntimeTarget};
use mm::domain::workspace::ProjectId;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixListener;
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A process runner that proves nothing was started.
struct NoProcesses;

#[async_trait::async_trait]
impl ProcessRunner for NoProcesses {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        Err(MmError::State {
            message: "a read from the engine's API started a process".to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl StreamingProcessRunner for NoProcesses {
    async fn spawn(
        &self,
        _specification: StreamCommandSpec,
        _cancellation: OperationCancellation,
    ) -> Result<ProcessStream, MmError> {
        Err(MmError::State {
            message: "a read from the engine's API started a process".to_owned(),
        })
    }
}

/// One container of the fake engine.
#[derive(Clone)]
struct Container {
    id: &'static str,
    name: &'static str,
    service: &'static str,
    state: &'static str,
    status: &'static str,
    created: &'static str,
    one_off: bool,
    tty: bool,
    /// The log answer's body, already framed.
    log: Vec<u8>,
    /// Sent after `log` when the read follows, one piece every few
    /// milliseconds; the answer then stays open.
    live: Vec<Vec<u8>>,
}

fn container(id: &'static str, service: &'static str, number: u32) -> Container {
    Container {
        id,
        name: Box::leak(format!("shop-app-{service}-{number}").into_boxed_str()),
        service,
        state: "running",
        status: "Up 2 hours (healthy)",
        created: "2026-11-14T03:00:00.000000000Z",
        one_off: false,
        tty: false,
        log: Vec::new(),
        live: Vec::new(),
    }
}

/// One framed message as the engine sends it: stream, three unused
/// bytes, the length, the payload.
fn frame(stream: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![stream, 0, 0, 0];
    bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap_or(0).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn stdout_line(second: u32, text: &str) -> Vec<u8> {
    frame(
        1,
        format!("2026-11-14T05:11:{second:02}.000000000Z {text}\n").as_bytes(),
    )
}

struct Engine {
    socket: PathBuf,
    requests: Arc<Mutex<Vec<String>>>,
    _directory: tempfile::TempDir,
}

impl Engine {
    fn api(&self) -> EngineApi {
        EngineApi::at(&self.socket)
    }

    fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .unwrap_or_default()
    }
}

fn start_engine(containers: Vec<Container>) -> Result<Engine, Box<dyn std::error::Error>> {
    let directory = temp_dirs::tempdir()?;
    let socket = directory.path().join("engine.sock");
    let listener = UnixListener::bind(&socket)?;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    let containers = Arc::new(containers);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let containers = Arc::clone(&containers);
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                if let Err(error) = serve(stream, &containers, &seen).await {
                    eprintln!("fake engine: {error}");
                }
            });
        }
    });
    Ok(Engine {
        socket,
        requests,
        _directory: directory,
    })
}

async fn serve(
    stream: tokio::net::UnixStream,
    containers: &[Container],
    seen: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let Some(request) = lines.next_line().await? else {
        return Ok(());
    };
    while let Some(header) = lines.next_line().await? {
        if header.is_empty() {
            break;
        }
    }
    let path = request.split(' ').nth(1).unwrap_or_default().to_owned();
    if let Ok(mut seen) = seen.lock() {
        seen.push(path.clone());
    }
    let (route, query) = path.split_once('?').unwrap_or((&path, ""));
    let json = |document: serde_json::Value| {
        let body = document.to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    };
    let parts = route.trim_matches('/').split('/').collect::<Vec<_>>();
    let found = |id: &str| containers.iter().find(|container| container.id == id);
    match parts.as_slice() {
        ["version"] => {
            let answer = serde_json::json!({"Version": "29.9.3", "GitCommit": "abc123"});
            writer.write_all(&json(answer)).await
        }
        ["containers", "json"] => {
            let service = query
                .contains("com.docker.compose.service%3D")
                .then(|| {
                    query
                        .split("com.docker.compose.service%3D")
                        .nth(1)
                        .and_then(|rest| rest.split('%').next())
                })
                .flatten();
            let rows = containers
                .iter()
                .filter(|container| service.is_none_or(|service| service == container.service))
                .map(|container| {
                    serde_json::json!({"Id": container.id, "Status": container.status})
                })
                .collect::<Vec<_>>();
            writer.write_all(&json(serde_json::Value::Array(rows))).await
        }
        ["containers", id, "json"] => match found(id) {
            Some(container) => writer.write_all(&json(inspect(container))).await,
            None => {
                writer
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 31\r\n\r\n{\"message\":\"No such container\"}",
                    )
                    .await
            }
        },
        ["containers", id, "logs"] => {
            let Some(container) = found(id) else {
                return writer
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\n{}")
                    .await;
            };
            writer
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await?;
            // In pieces that end in the middle of frames, as a socket
            // delivers them.
            for piece in container.log.chunks(1_000) {
                chunk(&mut writer, piece).await?;
            }
            if query.contains("follow=true") {
                for piece in &container.live {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    chunk(&mut writer, piece).await?;
                }
                // The answer stays open until the reader hangs up.
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            writer.write_all(b"0\r\n\r\n").await
        }
        _ => {
            writer
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\n{}")
                .await
        }
    }
}

async fn chunk(writer: &mut tokio::net::unix::OwnedWriteHalf, bytes: &[u8]) -> std::io::Result<()> {
    writer
        .write_all(format!("{:x}\r\n", bytes.len()).as_bytes())
        .await?;
    writer.write_all(bytes).await?;
    writer.write_all(b"\r\n").await?;
    writer.flush().await
}

fn inspect(container: &Container) -> serde_json::Value {
    let running = container.state == "running";
    let mut labels = BTreeMap::from([
        ("com.docker.compose.project", "shop-app".to_owned()),
        ("com.docker.compose.service", container.service.to_owned()),
        ("com.docker.compose.container-number", "1".to_owned()),
        ("io.mm.private", "not shown".to_owned()),
    ]);
    if container.one_off {
        labels.insert("com.docker.compose.oneoff", "True".to_owned());
    }
    serde_json::json!({
        "Id": container.id,
        "Name": format!("/{}", container.name),
        "Created": container.created,
        "RestartCount": 2,
        "Config": {"Image": "nginx:1.27", "Tty": container.tty, "Labels": labels},
        "State": {
            "Status": container.state,
            "Running": running,
            "ExitCode": if running { 0 } else { 1 },
            "Error": if container.state == "created" {
                "error while mounting volume '/var/lib/docker/volumes/mm-shared-0123-production-app-uploads-ab12cd34/_data': connection timed out"
            } else {
                ""
            },
            "StartedAt": if container.state == "created" { "0001-01-01T00:00:00Z" } else { "2026-11-14T03:00:01.5Z" },
            "FinishedAt": if running { "0001-01-01T00:00:00Z" } else { "2026-11-14T04:00:00Z" },
            "Health": {"Status": if running { "healthy" } else { "unhealthy" }},
        },
    })
}

fn target() -> RuntimeTarget {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    RuntimeTarget {
        root: root.clone(),
        project_directory: root.clone(),
        target: None,
        project_id: ProjectId(uuid::Uuid::from_u128(1)),
        environment: "production".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "shop-app".to_owned(),
        artifact_hash: "hash".to_owned(),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

fn approval() -> ApprovalContext {
    ApprovalContext {
        principal: "developer".to_owned(),
        source: "test".to_owned(),
        approved_high_risk: false,
    }
}

fn service(name: &str) -> ServiceSelector {
    ServiceSelector {
        stack: None,
        module: Some("shop".to_owned()),
        service: name.to_owned(),
        container: None,
    }
}

fn adapter(engine: &Engine) -> RuntimeOperationsAdapter {
    RuntimeOperationsAdapter::new(
        RuntimeKind::DockerCompose,
        Arc::new(NoProcesses),
        Arc::new(NoProcesses),
    )
    .reading_engine_api(Some(engine.api()))
}

async fn read_all(
    adapter: &RuntimeOperationsAdapter,
    request: LogRequest,
    redactions: Vec<Zeroizing<String>>,
) -> Result<Vec<LogRecord>, MmError> {
    let mut stream = adapter
        .logs(
            &target(),
            request,
            redactions,
            OperationCancellation::default(),
        )
        .await?;
    let mut records = Vec::new();
    while let Some(record) = stream.recv().await {
        records.push(record?);
    }
    Ok(records)
}

#[tokio::test]
async fn a_projects_containers_are_read_from_the_api_with_no_process() -> TestResult {
    let mut stopped = container("b".repeat(64).leak(), "worker", 1);
    stopped.state = "exited";
    stopped.status = "Exited (1) 3 minutes ago";
    let mut waiting = container("c".repeat(64).leak(), "files", 1);
    waiting.state = "created";
    waiting.status = "Created";
    let engine = start_engine(vec![
        container("a".repeat(64).leak(), "web", 1),
        stopped,
        waiting,
    ])?;
    let containers = project_containers(&engine.api(), "shop-app", None).await?;
    assert_eq!(containers.len(), 3);
    let by_service = |service: &str| {
        containers
            .iter()
            .map(|container| &container.observation)
            .find(|observation| observation.service == service)
            .ok_or_else(|| format!("no container of {service}"))
    };
    let web = by_service("web")?;
    assert_eq!(web.name, "shop-app-web-1");
    assert_eq!(web.image, "nginx:1.27");
    assert_eq!(web.state, "running");
    assert_eq!(web.status, "Up 2 hours (healthy)");
    assert_eq!(web.health.as_deref(), Some("healthy"));
    assert_eq!(web.started_at.as_deref(), Some("2026-11-14T03:00:01.5Z"));
    assert_eq!(web.finished_at, None);
    assert_eq!(web.exit_code, None);
    assert_eq!(web.restart_count, Some(2));
    assert!(web.is_running() && !web.waits_for_shared_storage());
    // Only the labels a reader needs leave the host.
    assert!(!web.labels.contains_key("io.mm.private"));
    assert!(web.labels.contains_key("com.docker.compose.service"));
    let worker = by_service("worker")?;
    assert_eq!(worker.exit_code, Some(1));
    assert_eq!(worker.finished_at.as_deref(), Some("2026-11-14T04:00:00Z"));
    assert_eq!(
        worker.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_UNHEALTHY")
    );
    // A container the engine could not mount a shared volume for says so.
    let files = by_service("files")?;
    assert_eq!(files.started_at, None);
    assert!(files.waits_for_shared_storage());

    // The list is asked by the project's label, and with a service, by both.
    project_containers(&engine.api(), "shop-app", Some("web")).await?;
    let requests = engine.requests();
    assert!(
        requests[0].starts_with("/containers/json?all=true&filters=%7B%22label%22%3A%5B%22com.docker.compose.project%3Dshop-app%22%5D%7D"),
        "{}",
        requests[0]
    );
    assert!(
        requests
            .iter()
            .any(|request| request.contains("com.docker.compose.service%3Dweb")),
        "{requests:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_services_log_is_every_container_in_the_order_the_lines_were_written() -> TestResult {
    let mut first = container("a".repeat(64).leak(), "web", 1);
    first.log = [
        stdout_line(1, "first-1"),
        stdout_line(3, "first-3"),
        frame(2, b"2026-11-14T05:11:05.000000000Z first-5 on stderr\n"),
    ]
    .concat();
    let mut second = container("b".repeat(64).leak(), "web", 2);
    second.log = [stdout_line(2, "second-2"), stdout_line(4, "second-4")].concat();
    // A container that ran once for a job is not the service's log.
    let mut job = container("c".repeat(64).leak(), "web", 3);
    job.one_off = true;
    job.log = stdout_line(0, "migration output");
    let engine = start_engine(vec![first, second, job])?;

    let request = LogRequest {
        tail: 4,
        ..LogRequest::new(service("web"), approval())
    };
    let records = read_all(&adapter(&engine), request, Vec::new()).await?;
    assert_eq!(
        records
            .iter()
            .map(|record| (
                record.container.as_deref().unwrap_or_default(),
                record.message.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("shop-app-web-2", "second-2"),
            ("shop-app-web-1", "first-3"),
            ("shop-app-web-2", "second-4"),
            ("shop-app-web-1", "first-5 on stderr"),
        ]
    );
    assert_eq!(
        records
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(records[3].source, LogSource::Stderr);
    assert_eq!(records[0].source, LogSource::Stdout);
    assert_eq!(records[0].identity.service, "web");
    assert_eq!(records[0].identity.stack, "app");
    assert_eq!(records[0].identity.module.as_deref(), Some("shop"));
    assert_eq!(records[0].container_label(), Some("web-2"));
    assert_eq!(
        records[0].timestamp.as_deref(),
        Some("2026-11-14T05:11:02.000000000Z")
    );
    Ok(())
}

#[tokio::test]
async fn a_read_without_a_service_covers_the_whole_project() -> TestResult {
    let mut web = container("a".repeat(64).leak(), "web", 1);
    web.log = stdout_line(2, "from web");
    let mut worker = container("b".repeat(64).leak(), "worker", 1);
    worker.log = stdout_line(1, "from worker");
    let engine = start_engine(vec![web, worker])?;
    let adapter = adapter(&engine);
    assert!(adapter.reads_whole_projects());
    let records = read_all(&adapter, LogRequest::every_service(approval()), Vec::new()).await?;
    assert_eq!(
        records
            .iter()
            .map(|record| (record.identity.service.as_str(), record.message.as_str()))
            .collect::<Vec<_>>(),
        [("worker", "from worker"), ("web", "from web")]
    );
    Ok(())
}

/// Lines of several kilobytes, and one longer than the engine's own
/// buffer, arrive whole whatever the pieces they travel in.
#[tokio::test]
async fn very_long_lines_survive_and_a_line_over_the_limit_is_cut_and_marked() -> TestResult {
    let long = "x".repeat(40_000);
    let mut web = container("a".repeat(64).leak(), "web", 1);
    // The engine splits a line at 16 KiB into messages of which only the
    // last ends in a newline, each with a timestamp of its own.
    web.log = [
        stdout_line(1, &"trace ".repeat(1_500)),
        frame(
            1,
            format!("2026-11-14T05:11:02.000000000Z {}", &long[..16_384]).as_bytes(),
        ),
        frame(
            1,
            format!("2026-11-14T05:11:02.000000001Z {}", &long[16_384..32_768]).as_bytes(),
        ),
        frame(
            1,
            format!("2026-11-14T05:11:02.000000002Z {}\n", &long[32_768..]).as_bytes(),
        ),
        stdout_line(3, "after"),
    ]
    .concat();
    let engine = start_engine(vec![web])?;
    let records = read_all(
        &adapter(&engine),
        LogRequest::new(service("web"), approval()),
        Vec::new(),
    )
    .await?;
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].message.len(), 9_000);
    assert_eq!(records[1].message, long);
    assert_eq!(
        records[1].timestamp.as_deref(),
        Some("2026-11-14T05:11:02.000000000Z")
    );
    assert!(!records[1].truncated);
    assert_eq!(records[2].message, "after");

    let mut request = LogRequest::new(service("web"), approval());
    request.limits.maximum_line_bytes = 1_000;
    let records = read_all(&adapter(&engine), request, Vec::new()).await?;
    assert_eq!(records.len(), 3);
    assert_eq!(records[1].message.len(), 1_000);
    assert!(records[1].truncated);
    assert!(!records[2].truncated);
    Ok(())
}

/// A secret whose value is an ordinary word does not blank that word
/// out of every line; a real secret value is still replaced.
#[tokio::test]
async fn log_lines_keep_ordinary_words_that_happen_to_be_secret_values() -> TestResult {
    let mut web = container("a".repeat(64).leak(), "web", 1);
    web.log = [
        stdout_line(1, "Call to a member function inDefaultSite() on null"),
        stdout_line(2, "token=sk-live-4f9a8b7c6d5e connected, cache is true"),
    ]
    .concat();
    let engine = start_engine(vec![web])?;
    let secrets = ["null", "true", "sk-live-4f9a8b7c6d5e"]
        .into_iter()
        .map(|secret| Zeroizing::new(secret.to_owned()))
        .collect();
    let records = read_all(
        &adapter(&engine),
        LogRequest::new(service("web"), approval()),
        secrets,
    )
    .await?;
    assert_eq!(
        records[0].message,
        "Call to a member function inDefaultSite() on null"
    );
    assert_eq!(
        records[1].message,
        "token=[REDACTED] connected, cache is true"
    );
    Ok(())
}

#[tokio::test]
async fn a_followed_log_merges_the_containers_as_their_lines_arrive() -> TestResult {
    let mut first = container("a".repeat(64).leak(), "web", 1);
    first.log = stdout_line(1, "first-history");
    first.live = vec![
        stdout_line(10, "first-live-10"),
        stdout_line(12, "first-live-12"),
    ];
    let mut second = container("b".repeat(64).leak(), "web", 2);
    second.log = stdout_line(2, "second-history");
    second.live = vec![stdout_line(11, "second-live-11")];
    let engine = start_engine(vec![first, second])?;
    let request = LogRequest {
        follow: true,
        ..LogRequest::new(service("web"), approval())
    };
    let cancellation = OperationCancellation::default();
    let mut stream = adapter(&engine)
        .logs(&target(), request, Vec::new(), cancellation.clone())
        .await?;
    let mut messages = Vec::new();
    while messages.len() < 5 {
        let record = tokio::time::timeout(Duration::from_secs(10), stream.recv())
            .await?
            .ok_or("the followed log ended")??;
        messages.push(record.message);
    }
    assert_eq!(
        messages,
        [
            "first-history",
            "second-history",
            "first-live-10",
            "second-live-11",
            "first-live-12"
        ]
    );
    // Nothing more comes, and the read ends when it is cancelled.
    cancellation.cancel();
    let ended = tokio::time::timeout(Duration::from_secs(5), stream.recv()).await?;
    assert!(ended.is_none());
    Ok(())
}

#[test]
fn a_terminals_output_is_read_as_the_lines_it_wrote() {
    let mut lines = LogDemux::new(true, 64 * 1024);
    let mut read = lines.push(b"2026-11-14T05:11:01.000000000Z hello\r\n2026-11-14T05:11:02.0000");
    read.extend(lines.push(b"00000Z wor"));
    read.extend(lines.push(b"ld\nno timestamp here\n"));
    read.extend(lines.finish());
    assert_eq!(
        read.iter()
            .map(|line| (
                line.timestamp.as_deref(),
                String::from_utf8_lossy(&line.bytes).into_owned()
            ))
            .collect::<Vec<_>>(),
        [
            (Some("2026-11-14T05:11:01.000000000Z"), "hello".to_owned()),
            (Some("2026-11-14T05:11:02.000000000Z"), "world".to_owned()),
            (None, "no timestamp here".to_owned()),
        ]
    );
    assert!(read.iter().all(|line| line.source == LogSource::Stdout));
}

#[test]
fn an_empty_line_and_a_line_cut_off_at_the_end_are_lines() {
    let mut lines = LogDemux::new(false, 64 * 1024);
    let mut read = lines.push(&frame(1, b"2026-11-14T05:11:01.000000000Z \n"));
    read.extend(lines.push(&frame(2, b"2026-11-14T05:11:02.000000000Z unfinished")));
    assert_eq!(read.len(), 1);
    assert!(read[0].bytes.is_empty());
    read.extend(lines.finish());
    assert_eq!(read.len(), 2);
    assert_eq!(read[1].bytes, b"unfinished");
    assert_eq!(read[1].source, LogSource::Stderr);
}

#[test]
fn the_api_is_used_only_for_the_engine_the_cli_would_talk_to() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let socket = directory.path().join("docker.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket)?;
    let from = |variables: &[(&str, &str)], default: &Path| {
        let variables = variables
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>();
        EngineApi::docker_from(|name| variables.get(name).cloned(), default)
    };
    let missing = directory.path().join("no.sock");
    // The default socket, when it is there.
    assert_eq!(from(&[], &socket), Some(EngineApi::at(&socket)));
    assert_eq!(from(&[], &missing), None);
    // DOCKER_HOST names a Unix socket, or another engine altogether.
    let host = format!("unix://{}", socket.display());
    assert_eq!(
        from(&[("DOCKER_HOST", &host)], &missing),
        Some(EngineApi::at(&socket))
    );
    assert_eq!(
        from(&[("DOCKER_HOST", "tcp://10.0.0.5:2376")], &socket),
        None
    );
    // A context other than the default is the CLI's business.
    assert_eq!(from(&[("DOCKER_CONTEXT", "remote")], &socket), None);
    assert!(from(&[("DOCKER_CONTEXT", "default")], &socket).is_some());
    let config = directory.path().join("config");
    std::fs::create_dir_all(&config)?;
    std::fs::write(
        config.join("config.json"),
        r#"{"currentContext": "desktop-linux"}"#,
    )?;
    let config = config.display().to_string();
    assert_eq!(from(&[("DOCKER_CONFIG", &config)], &socket), None);
    Ok(())
}
