
/// What the relay attests about a connection: the SSH key that
/// authenticated and where it came from.
#[derive(Debug, Clone)]
struct AuthenticatedSession {
    key_fingerprint: String,
    source: String,
    session_id: uuid::Uuid,
}

/// What the client's hello and this daemon agreed on.
struct Agreed {
    /// The protocol of every later frame.
    protocol: u16,
    /// The newest protocol the client offered, which a request it sent with
    /// its hello, before it read the answer, has.
    offered: u16,
    client_release: String,
}

/// A client's hello, from whatever release: what this daemon does not know
/// of it is left out.
fn read_hello(bytes: &[u8]) -> Result<HelloOffer, MmError> {
    if decode_frame::<FrameHead>(bytes)?.kind != "hello" {
        return Err(protocol("relay session must be followed by a client hello"));
    }
    decode_frame(bytes)
}

/// Answers a request this daemon cannot read whole and acts on none of it:
/// a client newer than this daemon is told to upgrade the host, any other
/// is told what is wrong with it.
async fn refuse_unread<W: AsyncWrite + Unpin + ?Sized>(
    writer: &mut W,
    agreed: &Agreed,
    bytes: &[u8],
    unread: MmError,
) -> Result<(), MmError> {
    let head = match decode_frame::<FrameHead>(bytes) {
        Ok(head) if head.kind == "request" => decode_frame::<RequestHead>(bytes),
        Ok(_) => return Err(protocol("hello must be followed by one request")),
        Err(error) => Err(error),
    };
    let Ok(head) = head else {
        return Err(unread);
    };
    if agreed.offered <= PROTOCOL_VERSION {
        send_operation_error(writer, (agreed.protocol, head.request_id), &unread, MutationState::NotStarted).await?;
        return Err(unread);
    }
    tracing::warn!(method = ?head.method.method, client = %agreed.client_release, "a newer mm sent a request this daemon cannot read");
    let refusal = request_not_understood(&head.method.method, env!("CARGO_PKG_VERSION"), &agreed.client_release);
    send_error(
        writer,
        (agreed.protocol, Some(head.request_id)),
        remote_error(UPGRADE_REQUIRED_CODE, &refusal, false, MutationState::NotStarted),
    )
    .await
}

/// The daemon's request socket: the one the previous daemon left with the
/// service manager when its file is still in place (requests sent during
/// the restart wait in it), else a new one, which the service manager
/// keeps from now on.
fn bind_socket(
    config: &DaemonConfig,
    sockets: &crate::adapters::kept_sockets::KeptSockets,
) -> Result<UnixListener, MmError> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

    let parent = config
        .socket_path
        .parent()
        .ok_or_else(|| MmError::Security {
            message: "daemon socket has no parent directory".to_owned(),
        })?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(|source| MmError::Io {
        context: format!("inspecting daemon socket directory {}", parent.display()),
        source,
    })?;
    if !parent_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || parent_metadata.uid() != config.socket_uid
        || parent_metadata.permissions().mode() & 0o022 != 0
    {
        return Err(MmError::Security {
            message: "daemon socket directory is not safely owned or is world-writable".to_owned(),
        });
    }
    let kept = sockets.take_unix(&config.socket_path);
    match fs::symlink_metadata(&config.socket_path) {
        Ok(metadata)
            if metadata.file_type().is_socket()
                && metadata.uid() == config.socket_uid
                && metadata.gid() == config.socket_gid =>
        {
            if let Some(kept) = kept {
                return kept
                    .set_nonblocking(true)
                    .and_then(|()| UnixListener::from_std(kept))
                    .map_err(|source| MmError::Io {
                        context: format!(
                            "taking over daemon socket {}",
                            config.socket_path.display()
                        ),
                        source,
                    });
            }
            fs::remove_file(&config.socket_path).map_err(|source| MmError::Io {
                context: format!(
                    "removing stale daemon socket {}",
                    config.socket_path.display()
                ),
                source,
            })?;
        }
        Ok(_) => {
            return Err(MmError::Security {
                message: "daemon socket path already exists with an unsafe type or owner"
                    .to_owned(),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(MmError::Io {
                context: format!("inspecting daemon socket {}", config.socket_path.display()),
                source,
            });
        }
    }
    let listener = UnixListener::bind(&config.socket_path).map_err(|source| MmError::Io {
        context: format!("binding daemon socket {}", config.socket_path.display()),
        source,
    })?;
    fs::set_permissions(&config.socket_path, fs::Permissions::from_mode(0o660)).map_err(
        |source| MmError::Io {
            context: format!(
                "setting daemon socket mode {}",
                config.socket_path.display()
            ),
            source,
        },
    )?;
    rustix::fs::chown(
        &config.socket_path,
        Some(rustix::process::Uid::from_raw(config.socket_uid)),
        Some(rustix::process::Gid::from_raw(config.socket_gid)),
    )
    .map_err(|source| MmError::Io {
        context: format!(
            "setting daemon socket owner {}",
            config.socket_path.display()
        ),
        source: std::io::Error::from_raw_os_error(source.raw_os_error()),
    })?;
    sockets.keep_unix(&config.socket_path, &listener);
    Ok(listener)
}

fn method_capability(method: &RemoteMethod) -> (Capability, Option<&str>) {
    use AccessAction as A;
    use AccessResourceKind as R;
    match method {
        RemoteMethod::Status => (Capability::new(A::Status, R::Project), None),
        // A read of every service has no one service to name here; the
        // daemon then checks each service by itself (`authorize_every_log`).
        RemoteMethod::Logs(request) => (Capability::new(A::Logs, R::Service), request.service()),
        RemoteMethod::Exec(request) => (
            Capability::new(A::Exec, R::Service),
            Some(request.selector.service.as_str()),
        ),
        // A plan needs the commit on the host; on a host that does not
        // fetch, whoever may plan may send it.
        RemoteMethod::PlanDeployment { .. } | RemoteMethod::WhoAmI | RemoteMethod::Deliver(_) => {
            (Capability::new(A::Plan, R::Project), None)
        }
        RemoteMethod::ApplyDeployment { .. }
        | RemoteMethod::FinishMoves { .. }
        | RemoteMethod::Rollout { .. }
        // Whoever may deploy to the environment may stop what runs in it.
        | RemoteMethod::Cancel { .. } => (Capability::new(A::DeployApply, R::Project), None),
        RemoteMethod::PlanTeamSync { .. } | RemoteMethod::ApplyTeamSync { .. } => {
            (Capability::new(A::TeamSync, R::Project), None)
        }
        RemoteMethod::Forward { service, .. } => {
            (Capability::new(A::Forward, R::Service), Some(service.as_str()))
        }
        // Sealing needs what planning that kind of plan needs.
        RemoteMethod::SealPlan(seal) => (plan_capability(seal.kind), None),
        RemoteMethod::SealedPlan { .. }
        | RemoteMethod::Node
        | RemoteMethod::MeshStatus
        | RemoteMethod::Networks
        | RemoteMethod::DeploymentStatus
        | RemoteMethod::Names
        | RemoteMethod::StorageStatus { .. }
        | RemoteMethod::Runtimes => {
            (Capability::new(A::Status, R::Project), None)
        }
        // Deleting shared data is the cluster's administration, not a
        // deployment.
        RemoteMethod::PlanVolumeDelete { .. } | RemoteMethod::DeleteVolume { .. } => {
            (Capability::new(A::TargetAdmin, R::Target), None)
        }
        // What `operate` adds to `view`: restarts and the declared jobs.
        RemoteMethod::Restart(request) => (
            Capability::new(A::Restart, R::Service),
            Some(request.selector.service.as_str()),
        ),
        RemoteMethod::RunJob(_) => (Capability::new(A::JobRun, R::Job), None),
        RemoteMethod::JobRuns(_) | RemoteMethod::HostValues => {
            (Capability::new(A::Status, R::Project), None)
        }
        RemoteMethod::SecretRecipient
        | RemoteMethod::StageValues(_)
        | RemoteMethod::StoreMigrate { .. } => (Capability::new(A::SecretSet, R::Secret), None),
        RemoteMethod::StoreSnapshot { .. } | RemoteMethod::StoreRestore(_) => {
            (Capability::new(A::TargetAdmin, R::Target), None)
        }
    }
}

/// The kind of stored plan an apply method consumes.
const fn applied_plan_kind(method: &RemoteMethod) -> Option<RemotePlanKind> {
    match method {
        RemoteMethod::ApplyDeployment { .. }
        | RemoteMethod::Rollout { step: crate::domain::rollout::RolloutStep::Begin, .. } => {
            Some(RemotePlanKind::Deployment)
        }
        RemoteMethod::ApplyTeamSync { .. } => Some(RemotePlanKind::TeamSync),
        _ => None,
    }
}

/// The capability a plan of `kind` was made with, rechecked at apply.
const fn plan_capability(kind: RemotePlanKind) -> Capability {
    use AccessAction as A;
    use AccessResourceKind as R;
    match kind {
        RemotePlanKind::Deployment => Capability::new(A::Plan, R::Project),
        RemotePlanKind::TeamSync => Capability::new(A::TeamSync, R::Project),
    }
}

fn validate_relay_attribution(fingerprint: &str, source: &str) -> Result<(), MmError> {
    let valid_fingerprint = fingerprint.len() <= 72
        && fingerprint.strip_prefix("SHA256:").is_some_and(|digest| {
            !digest.is_empty()
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        });
    let valid_source = !source.is_empty()
        && source.len() <= 512
        && source
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b' ' | b'-'));
    if !valid_fingerprint || !valid_source {
        return Err(MmError::Security {
            message: "relay key fingerprint or connection source attribution is invalid".to_owned(),
        });
    }
    Ok(())
}

fn bind_principal(method: &mut RemoteMethod, principal: &str) {
    match method {
        RemoteMethod::Logs(request) => {
            request.approval.principal = principal.to_owned();
            request.approval.source = "authenticated-ssh-relay".to_owned();
        }
        RemoteMethod::Exec(request) => {
            request.approval.principal = principal.to_owned();
            request.approval.source = "authenticated-ssh-relay".to_owned();
        }
        RemoteMethod::Restart(request) => {
            request.approval.principal = principal.to_owned();
            request.approval.source = "authenticated-ssh-relay".to_owned();
        }
        RemoteMethod::Status
        | RemoteMethod::SecretRecipient
        | RemoteMethod::StageValues(_)
        | RemoteMethod::HostValues
        | RemoteMethod::StoreSnapshot { .. }
        | RemoteMethod::StoreRestore(_)
        | RemoteMethod::StoreMigrate { .. }
        | RemoteMethod::PlanDeployment { .. }
        | RemoteMethod::ApplyDeployment { .. }
        | RemoteMethod::PlanTeamSync { .. }
        | RemoteMethod::ApplyTeamSync { .. }
        | RemoteMethod::WhoAmI
        | RemoteMethod::Forward { .. }
        | RemoteMethod::SealPlan(_)
        | RemoteMethod::SealedPlan { .. }
        | RemoteMethod::Node
        | RemoteMethod::MeshStatus
        | RemoteMethod::Networks
        | RemoteMethod::FinishMoves { .. }
        | RemoteMethod::Rollout { .. }
        | RemoteMethod::DeploymentStatus
        | RemoteMethod::Names
        | RemoteMethod::StorageStatus { .. }
        | RemoteMethod::Runtimes
        | RemoteMethod::PlanVolumeDelete { .. }
        | RemoteMethod::DeleteVolume { .. }
        | RemoteMethod::Deliver(_)
        | RemoteMethod::RunJob(_)
        | RemoteMethod::JobRuns(_)
        | RemoteMethod::Cancel { .. } => {}
    }
}

async fn timed<T>(
    timeout: Duration,
    future: impl std::future::Future<Output = Result<T, MmError>>,
) -> Result<T, MmError> {
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| deadline_passed(timeout))?
}

/// A request that ran out of its time on the host: a timeout, never a
/// protocol or state error.
fn deadline_passed(timeout: Duration) -> MmError {
    MmError::Timeout {
        message: format!(
            "the request did not finish on this host within its deadline of {} s and was stopped; its child processes were ended",
            timeout.as_secs()
        ),
    }
}

/// How long a request may still run before its own deadline.
fn remaining(context: &RemoteRequestContext) -> Result<Duration, MmError> {
    let left = context.deadline_unix_ms.saturating_sub(now_unix_ms()?);
    match u64::try_from(left) {
        Ok(left) if left > 0 => Ok(Duration::from_millis(left)),
        _ => Err(deadline_passed(Duration::ZERO)),
    }
}

fn add_stream_bytes(current: u64, frame: usize, count: u64) -> Result<u64, MmError> {
    let bytes = current.saturating_add(u64::try_from(frame).unwrap_or(u64::MAX));
    if count > MAX_STREAM_FRAMES || bytes > MAX_STREAM_BYTES {
        return Err(protocol("stream exceeded its frame or byte limit"));
    }
    Ok(bytes)
}

fn is_cancel(
    frame: Option<ClientFrame>,
    (agreed, request_id): (u16, uuid::Uuid),
) -> Result<bool, MmError> {
    match frame {
        Some(ClientFrame::Cancel {
            version,
            request_id: cancelled,
        }) if version == agreed && cancelled == request_id => Ok(true),
        None => Ok(true),
        _ => Err(protocol(
            "only a matching cancellation frame is valid during a stream",
        )),
    }
}

async fn send_result<W: AsyncWrite + Unpin + ?Sized>(
    writer: &mut W,
    context: &RemoteRequestContext,
    result: RemoteResult,
) -> Result<(), MmError> {
    write_frame(
        writer,
        &ServerFrame::Result {
            version: context.protocol,
            request_id: context.request_id,
            result: Box::new(result),
        },
    )
    .await
    .map(|_| ())
}

async fn send_end<W: AsyncWrite + Unpin + ?Sized>(
    writer: &mut W,
    context: &RemoteRequestContext,
    frames: u64,
    bytes: u64,
) -> Result<(), MmError> {
    write_frame(
        writer,
        &ServerFrame::End {
            version: context.protocol,
            request_id: context.request_id,
            frames,
            bytes,
        },
    )
    .await
    .map(|_| ())
}

async fn send_operation_error<W: AsyncWrite + Unpin + ?Sized>(
    writer: &mut W,
    (version, request_id): (u16, uuid::Uuid),
    error: &MmError,
    mutation_state: MutationState,
) -> Result<(), MmError> {
    send_error(
        writer,
        (version, Some(request_id)),
        remote_error(
            error.code(),
            &redacted_error_message(error),
            false,
            mutation_state,
        ),
    )
    .await
}

async fn send_error<W: AsyncWrite + Unpin + ?Sized>(
    writer: &mut W,
    (version, request_id): (u16, Option<uuid::Uuid>),
    error: RemoteError,
) -> Result<(), MmError> {
    write_frame(
        writer,
        &ServerFrame::Error {
            version,
            request_id,
            error,
        },
    )
    .await
    .map(|_| ())
}

fn remote_error(
    code: &str,
    message: &str,
    retryable: bool,
    mutation_state: MutationState,
) -> RemoteError {
    RemoteError {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable,
        mutation_state,
    }
}

fn redacted_error_message(error: &MmError) -> String {
    match error {
        MmError::Secret { .. } | MmError::SecretCompatibility { .. } => {
            "secret operation failed; inspect target-side audit diagnostics".to_owned()
        }
        _ => error.to_string(),
    }
}

fn now_unix_ms() -> Result<i64, MmError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MmError::State {
            message: "system clock is before the Unix epoch".to_owned(),
        })?;
    i64::try_from(elapsed.as_millis()).map_err(|_| MmError::State {
        message: "system clock is outside the supported range".to_owned(),
    })
}

fn protocol(message: &str) -> MmError {
    MmError::RuntimeResponse {
        message: format!("remote protocol error: {message}"),
    }
}

/// Why a host refuses an environment it was not set up for, with the step
/// that sets it up.
fn not_set_up(environment: &str) -> String {
    format!(
        "this host does not run environment '{environment}'; set it up here with `mm --environment {environment} target setup plan` and `apply`"
    )
}
