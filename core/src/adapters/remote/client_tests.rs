use std::process::Stdio;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

use tokio::process::Command;

use crate::adapters::remote::codec::{FrameReader, encode_frame, write_frame};
use crate::app::ports::RemoteTransport as _;
use crate::domain::observability::{
    ApprovalContext, ExecEvent, ExecOutputSource, ExecRequest, ExecStdinMode, ExecTtyMode,
    LogLimits, LogMode, LogOutputPolicy, LogRecord, LogRequest, LogSource, OperationCancellation,
    ServiceSelector,
};
use crate::domain::remote::{
    ClientFrame, MutationState, PROTOCOL_VERSION, RemoteError, RemoteMethod, RemoteScope,
    RemoteStreamItem, ServerFrame,
};
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::{NamedTarget, TargetDefinition, TargetId, TargetTransport};
use crate::domain::workspace::ProjectId;

use super::{
    Connection, SshRemoteTransport, StreamKind, StreamReceiveContext, disconnect, now_unix_ms,
    protocol, receive_frame, receive_stream, send_stream_error, server_error,
};

fn local_target() -> NamedTarget {
    NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::nil())),
    )
}

fn ssh_target(host_key: Option<&str>, fingerprint: Option<&str>) -> NamedTarget {
    NamedTarget::new(
        "remote".to_owned(),
        TargetDefinition {
            id: TargetId(uuid::Uuid::from_u128(1)),
            transport: TargetTransport::Ssh {
                host: "example.test".to_owned(),
                port: 2222,
                user: "relay".to_owned(),
                host_key_fingerprint: "SHA256:kmYcvdi2GkPeWxB6XLjrZB8JHsy2Hm8luHMFp9GMvqk"
                    .to_owned(),
                host_key: host_key.map(str::to_owned),
                identity_file: None,
                remote_root: Some("/srv/project".into()),
                daemon_fingerprint: fingerprint.map(str::to_owned),
            },
            bootstrap: crate::domain::target::TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: Some(RuntimeKind::DockerCompose),
            available_runtimes: vec![RuntimeKind::DockerCompose],
            runtime: None,
            nodes: Vec::new(),
        },
    )
}

fn scope(environment: &str) -> RemoteScope {
    RemoteScope {
        project_id: ProjectId(uuid::Uuid::from_u128(2)),
        environment: environment.to_owned(),
        target: "remote".to_owned(),
        target_fingerprint: "a".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    }
}

fn selector() -> ServiceSelector {
    ServiceSelector {
        stack: None,
        module: None,
        service: "api".to_owned(),
        container: None,
    }
}

fn log_request(follow: bool) -> LogRequest {
    LogRequest {
        selector: Some(selector()),
        nodes: Vec::new(),
        tail: 10,
        since_unix_ms: None,
        until_unix_ms: None,
        follow,
        timestamps: true,
        output: LogOutputPolicy::Both,
        mode: LogMode::Redacted,
        limits: LogLimits::default(),
        approval: ApprovalContext {
            principal: "test".to_owned(),
            source: "unit".to_owned(),
            approved_high_risk: false,
        },
    }
}

fn exec_request(stdin: ExecStdinMode, tty: ExecTtyMode) -> ExecRequest {
    ExecRequest {
        selector: selector(),
        argv: vec!["true".to_owned()],
        user: None,
        workdir: None,
        environment: Vec::new(),
        tty,
        stdin,
        timeout: Duration::from_secs(1),
        approval: ApprovalContext {
            principal: "test".to_owned(),
            source: "unit".to_owned(),
            approved_high_risk: false,
        },
        shell_wrapper: false,
    }
}

fn test_connection(
    program: &str,
    argument: &str,
) -> Result<Connection, Box<dyn std::error::Error>> {
    let known_hosts = tempfile::NamedTempFile::new()?;
    let mut child = Command::new(program)
        .arg(argument)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take().ok_or("stdin missing")?;
    let stdout = child.stdout.take().ok_or("stdout missing")?;
    Ok(Connection {
        subject: "target 'remote' (example.test:2222)".to_owned(),
        protocol: PROTOCOL_VERSION,
        daemon_version: "0.1.1".to_owned(),
        child,
        stdin,
        stdout: FrameReader::new(stdout),
        sent: None,
        _known_hosts: known_hosts,
        _inline_key: None,
        askpass: None,
    })
}

#[test]
fn target_enrollment_and_error_mapping_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    assert!(SshRemoteTransport::new(&local_target()).is_err());
    assert!(SshRemoteTransport::new(&ssh_target(None, Some(&"a".repeat(64)))).is_err());
    assert!(SshRemoteTransport::new(&ssh_target(Some("ssh-ed25519 invalid"), None)).is_err());
    assert!(format!("{}", protocol("bad frame")).contains("protocol error"));
    let lost = disconnect(
        "target 'remote' (example.test:2222)",
        MutationState::Unknown,
    );
    assert_eq!(lost.code(), "MM_UNREACHABLE");
    assert!(
        lost.to_string()
            .contains("ended the connection before it answered")
    );
    assert!(lost.to_string().contains("mm deploy status"));
    assert!(
        format!(
            "{}",
            server_error(RemoteError {
                code: "MM_SECURITY".to_owned(),
                message: "denied".to_owned(),
                mutation_state: MutationState::NotStarted,
                retryable: false,
            })
        )
        .contains("denied")
    );
    assert!(now_unix_ms().is_ok());
    let transport = SshRemoteTransport::new(&ssh_target(
        Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Some(&"a".repeat(64)),
    ))?;
    assert_eq!(transport.endpoint.port, 2222);
    assert_eq!(transport.endpoint.host, "example.test");
    assert_eq!(
        transport.endpoint.subject,
        "target 'remote' (example.test:2222)"
    );
    Ok(())
}

#[tokio::test]
async fn receive_frame_reports_disconnect_decode_and_closed_receiver() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    assert!(
        receive_frame(Ok(None), ("host", MutationState::NotStarted), &sender)
            .await
            .is_none()
    );
    assert!(receiver.recv().await.is_some());

    assert!(
        receive_frame(
            Err(protocol("malformed")),
            ("host", MutationState::NotStarted),
            &sender,
        )
        .await
        .is_none()
    );
    assert!(receiver.recv().await.is_some());

    drop(receiver);
    assert!(
        receive_frame(Ok(None), ("host", MutationState::Unknown), &sender)
            .await
            .is_none()
    );
    assert!(
        receive_frame(
            Err(protocol("closed")),
            ("host", MutationState::Unknown),
            &sender
        )
        .await
        .is_none()
    );
    let cancellation = OperationCancellation::default();
    send_stream_error(&sender, &cancellation, protocol("closed")).await;
    assert!(cancellation.is_cancelled());
}

#[tokio::test]
async fn stream_receiver_rejects_mismatched_frames_and_cleans_up_child()
-> Result<(), Box<dyn std::error::Error>> {
    let mut connection = test_connection("cat", "-")?;
    let request_id = uuid::Uuid::from_u128(7);
    write_frame(
        &mut connection.stdin,
        &ServerFrame::End {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::from_u128(8),
            frames: 0,
            bytes: 0,
        },
    )
    .await?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let cancellation = OperationCancellation::default();
    receive_stream(StreamReceiveContext {
        connection,
        request_id,
        timeout: Duration::from_secs(1),
        disconnect_state: MutationState::NotStarted,
        kind: StreamKind::Logs,
        cancellation: cancellation.clone(),
        sender,
    })
    .await;
    assert!(receiver.recv().await.is_some_and(|item| item.is_err()));
    assert!(cancellation.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn stream_deadline_cancels_and_reports_error() -> Result<(), Box<dyn std::error::Error>> {
    let connection = test_connection("sleep", "2")?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let cancellation = OperationCancellation::default();
    receive_stream(StreamReceiveContext {
        connection,
        request_id: uuid::Uuid::from_u128(9),
        timeout: Duration::from_millis(5),
        disconnect_state: MutationState::Unknown,
        kind: StreamKind::Exec,
        cancellation: cancellation.clone(),
        sender,
    })
    .await;
    assert!(receiver.recv().await.is_some_and(|item| item.is_err()));
    assert!(cancellation.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn handshake_rejects_an_echoed_client_hello() -> Result<(), Box<dyn std::error::Error>> {
    let mut connection = test_connection("cat", "-")?;
    assert!(
        connection
            .handshake(&"a".repeat(64), Duration::from_secs(15), None)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn valid_log_and_exec_streams_preserve_sequence_and_reported_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let request_id = uuid::Uuid::from_u128(10);
    let log_frame = ServerFrame::Stream {
        version: PROTOCOL_VERSION,
        request_id,
        sequence: 1,
        item: Box::new(RemoteStreamItem::Log(LogRecord {
            identity: crate::domain::runtime::ServiceIdentity {
                project_id: ProjectId(uuid::Uuid::from_u128(2)),
                environment: "development".to_owned(),
                stack: "app".to_owned(),
                module: None,
                service: "api".to_owned(),
            },
            sequence: 1,
            source: LogSource::Stdout,
            node: None,
            container: None,
            timestamp: None,
            message: "ready".to_owned(),
            lossy_utf8: false,
            truncated: false,
            reconnect: 0,
        })),
    };
    let log_bytes = u64::try_from(encode_frame(&log_frame)?.len())?;
    let mut connection = test_connection("cat", "-")?;
    write_frame(&mut connection.stdin, &log_frame).await?;
    write_frame(
        &mut connection.stdin,
        &ServerFrame::End {
            version: PROTOCOL_VERSION,
            request_id,
            frames: 1,
            bytes: log_bytes,
        },
    )
    .await?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let cancellation = OperationCancellation::default();
    receive_stream(StreamReceiveContext {
        connection,
        request_id,
        timeout: Duration::from_secs(1),
        disconnect_state: MutationState::NotStarted,
        kind: StreamKind::Logs,
        cancellation: cancellation.clone(),
        sender,
    })
    .await;
    assert!(matches!(
        receiver.recv().await,
        Some(Ok(super::StreamValue::Log(_)))
    ));
    assert!(cancellation.is_cancelled());

    let exec_frame = ServerFrame::Stream {
        version: PROTOCOL_VERSION,
        request_id,
        sequence: 1,
        item: Box::new(RemoteStreamItem::Exec(ExecEvent::Output {
            source: ExecOutputSource::Stdout,
            value: "ok".to_owned(),
            lossy_utf8: false,
        })),
    };
    let exec_bytes = u64::try_from(encode_frame(&exec_frame)?.len())?;
    let mut connection = test_connection("cat", "-")?;
    write_frame(&mut connection.stdin, &exec_frame).await?;
    write_frame(
        &mut connection.stdin,
        &ServerFrame::End {
            version: PROTOCOL_VERSION,
            request_id,
            frames: 1,
            bytes: exec_bytes,
        },
    )
    .await?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    receive_stream(StreamReceiveContext {
        connection,
        request_id,
        timeout: Duration::from_secs(1),
        disconnect_state: MutationState::Unknown,
        kind: StreamKind::Exec,
        cancellation: OperationCancellation::default(),
        sender,
    })
    .await;
    assert!(matches!(
        receiver.recv().await,
        Some(Ok(super::StreamValue::Exec(_)))
    ));
    Ok(())
}

#[tokio::test]
async fn stream_rejects_type_changes_server_errors_and_closed_consumers()
-> Result<(), Box<dyn std::error::Error>> {
    let request_id = uuid::Uuid::from_u128(11);
    let cases = [
        ServerFrame::Stream {
            version: PROTOCOL_VERSION,
            request_id,
            sequence: 1,
            item: Box::new(RemoteStreamItem::Exec(ExecEvent::Output {
                source: ExecOutputSource::Stderr,
                value: "wrong type".to_owned(),
                lossy_utf8: false,
            })),
        },
        ServerFrame::Error {
            version: PROTOCOL_VERSION,
            request_id: Some(request_id),
            error: RemoteError {
                code: "MM_DENIED".to_owned(),
                message: "denied".to_owned(),
                mutation_state: MutationState::NotStarted,
                retryable: false,
            },
        },
    ];
    for frame in cases {
        let mut connection = test_connection("cat", "-")?;
        write_frame(&mut connection.stdin, &frame).await?;
        let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
        receive_stream(StreamReceiveContext {
            connection,
            request_id,
            timeout: Duration::from_secs(1),
            disconnect_state: MutationState::NotStarted,
            kind: StreamKind::Logs,
            cancellation: OperationCancellation::default(),
            sender,
        })
        .await;
        assert!(receiver.recv().await.is_some_and(|item| item.is_err()));
    }

    let frame = ServerFrame::Stream {
        version: PROTOCOL_VERSION,
        request_id,
        sequence: 1,
        item: Box::new(RemoteStreamItem::Exec(ExecEvent::Output {
            source: ExecOutputSource::Stdout,
            value: "dropped".to_owned(),
            lossy_utf8: false,
        })),
    };
    let mut connection = test_connection("cat", "-")?;
    write_frame(&mut connection.stdin, &frame).await?;
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    drop(receiver);
    let cancellation = OperationCancellation::default();
    receive_stream(StreamReceiveContext {
        connection,
        request_id,
        timeout: Duration::from_secs(1),
        disconnect_state: MutationState::Unknown,
        kind: StreamKind::Exec,
        cancellation: cancellation.clone(),
        sender,
    })
    .await;
    assert!(cancellation.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn cancellation_and_request_encoding_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let request_id = uuid::Uuid::from_u128(12);
    let mut connection = test_connection("cat", "-")?;
    connection
        .send_request(
            request_id,
            scope("development"),
            RemoteMethod::Status,
            Duration::from_millis(50),
        )
        .await?;
    let frame = connection
        .stdout
        .next::<ClientFrame>()
        .await?
        .ok_or("request missing")?;
    assert!(
        matches!(frame, ClientFrame::Request { request_id: received, .. } if received == request_id)
    );

    let connection = test_connection("sleep", "1")?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let cancellation = OperationCancellation::default();
    cancellation.cancel();
    receive_stream(StreamReceiveContext {
        connection,
        request_id,
        timeout: Duration::from_millis(60),
        disconnect_state: MutationState::Unknown,
        kind: StreamKind::Exec,
        cancellation,
        sender,
    })
    .await;
    assert!(receiver.recv().await.is_some_and(|item| item.is_err()));
    Ok(())
}

#[tokio::test]
async fn public_operations_reject_invalid_scope_and_unsupported_terminal_state()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = SshRemoteTransport::new(&ssh_target(
        Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Some(&"a".repeat(64)),
    ))?;
    let invalid = scope("");
    assert!(transport.status(invalid.clone()).await.is_err());
    assert!(
        transport
            .logs(
                invalid.clone(),
                log_request(false),
                OperationCancellation::default()
            )
            .await
            .is_err()
    );
    assert!(
        transport
            .logs(
                invalid.clone(),
                log_request(true),
                OperationCancellation::default()
            )
            .await
            .is_err()
    );
    assert!(
        transport
            .exec(
                invalid.clone(),
                exec_request(ExecStdinMode::Closed, ExecTtyMode::Disabled),
                OperationCancellation::default()
            )
            .await
            .is_err()
    );
    assert!(
        transport
            .plan_deployment(invalid.clone(), "a".repeat(40), None, Default::default())
            .await
            .is_err()
    );
    assert!(
        transport
            .apply_deployment(invalid, "plan".to_owned())
            .await
            .is_err()
    );
    assert!(
        transport
            .exec(
                scope("development"),
                exec_request(ExecStdinMode::Inherit, ExecTtyMode::Enabled),
                OperationCancellation::default(),
            )
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn handshake_maps_server_error_before_identity_acceptance()
-> Result<(), Box<dyn std::error::Error>> {
    let mut connection = test_connection("cat", "-")?;
    write_frame(
        &mut connection.stdin,
        &ServerFrame::Error {
            version: PROTOCOL_VERSION,
            request_id: None,
            error: RemoteError {
                code: "MM_HANDSHAKE".to_owned(),
                message: "rejected".to_owned(),
                mutation_state: MutationState::NotStarted,
                retryable: false,
            },
        },
    )
    .await?;
    assert!(
        connection
            .handshake(&"a".repeat(64), Duration::from_secs(15), None)
            .await
            .is_err()
    );
    Ok(())
}

#[cfg(unix)]
struct FakeSshGuard {
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl FakeSshGuard {
    fn install() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Ok(None);
        }
        let path = std::path::PathBuf::from("/usr/local/bin/ssh");
        if path.exists() {
            return Err("the deterministic fake SSH path is already occupied".into());
        }
        std::fs::write(
            &path,
            r#"#!/usr/bin/python3
import json
import struct
import sys

def read_frame():
    length = sys.stdin.buffer.read(4)
    if len(length) != 4:
        sys.exit(2)
    size = struct.unpack(">I", length)[0]
    return json.loads(sys.stdin.buffer.read(size))

def write_frame(value):
    body = json.dumps(value, separators=(",", ":")).encode()
    sys.stdout.buffer.write(struct.pack(">I", len(body)) + body)
    sys.stdout.buffer.flush()

# Answers in the protocol version the client speaks.
first = read_frame()
version = first["version"]
hello = first["hello"]
write_frame({
    "type": "hello",
    "version": version,
    "hello": {
        "client_nonce": hello["client_nonce"],
        "server_nonce": "00000000-0000-0000-0000-000000000011",
        "daemon_version": "test",
        "daemon_fingerprint": hello["expected_daemon_fingerprint"],
        "instance_id": "00000000-0000-0000-0000-000000000012"
    }
})
request = read_frame()
method = request["method"]["method"]
if method in ("logs", "exec"):
    write_frame({
        "type": "result",
        "version": version,
        "request_id": request["request_id"],
        "result": {"result": "stream_started"}
    })
else:
    write_frame({
        "type": "result",
        "version": version,
        "request_id": request["request_id"],
        "result": {"result": "stream_started"}
    })
    sys.exit(0)
write_frame({
    "type": "error",
    "version": version,
    "request_id": request.get("request_id"),
    "error": {
        "code": "MM_TEST_DENIED",
        "message": "deterministic denial",
        "retryable": False,
        "mutation_state": "not_started"
    }
})
"#,
        )?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(Some(Self { path }))
    }
}

#[cfg(unix)]
impl Drop for FakeSshGuard {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            eprintln!("could not remove deterministic fake SSH executable: {error}");
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn public_operations_use_strict_ssh_and_map_framed_server_denials()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(_guard) = FakeSshGuard::install()? else {
        return Ok(());
    };
    let transport = SshRemoteTransport::new(&ssh_target(
        Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Some(&"a".repeat(64)),
    ))?;
    let valid = scope("development");
    assert!(transport.status(valid.clone()).await.is_err());
    let mut logs = transport
        .logs(
            valid.clone(),
            log_request(false),
            OperationCancellation::default(),
        )
        .await?;
    assert!(logs.recv().await.is_some_and(|item| item.is_err()));
    let mut followed = transport
        .logs(
            valid.clone(),
            log_request(true),
            OperationCancellation::default(),
        )
        .await?;
    assert!(followed.recv().await.is_some_and(|item| item.is_err()));
    let mut exec = transport
        .exec(
            valid.clone(),
            exec_request(ExecStdinMode::Closed, ExecTtyMode::Disabled),
            OperationCancellation::default(),
        )
        .await?;
    assert!(exec.recv().await.is_some_and(|item| item.is_err()));
    assert!(
        transport
            .plan_deployment(valid.clone(), "a".repeat(40), None, Default::default())
            .await
            .is_err()
    );
    assert!(
        transport
            .apply_deployment(valid, "plan".to_owned())
            .await
            .is_err()
    );

    let mut target = ssh_target(
        Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Some(&"b".repeat(64)),
    );
    if let TargetTransport::Ssh {
        port,
        identity_file,
        ..
    } = &mut target.definition.transport
    {
        *port = 22;
        *identity_file = Some("/tmp/test-identity".into());
    }
    assert!(
        SshRemoteTransport::new(&target)?
            .status(scope("development"))
            .await
            .is_err()
    );
    Ok(())
}
