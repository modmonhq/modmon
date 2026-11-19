use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::adapters::keys::ManagedKeys;
use crate::adapters::remote::codec::{FrameReader, encode_frame, write_frame};
use crate::app::ports::{ExecStream, LogStream, RemoteTransport};
use crate::domain::deployment::DeploymentResult;
use crate::domain::error::MmError;
use crate::domain::observability::{
    ExecRequest, LogRequest, OperationCancellation, RestartOutcome, RestartRequest,
};
use crate::domain::remote::{
    ClientFrame, ClientHello, DEFAULT_REQUEST_TIMEOUT_MS, MAX_REQUEST_TIMEOUT_MS, MAX_STREAM_BYTES,
    MAX_STREAM_FRAMES, MINIMUM_PROTOCOL_VERSION, MutationState, PROTOCOL_VERSION, ProtocolRange,
    RemoteAccessPlan, RemoteDeploymentPlan, RemoteIdentity, RemoteMethod, RemoteResult,
    RemoteScope, RemoteStreamItem, ServerFrame,
};
use crate::domain::scheduler::{JobRun, JobRunRequest, JobRunsRequest};
use crate::domain::users::AccessChange;

/// Planning fetches and checks out a commit on the host before planning.
const PLAN_REQUEST_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
/// How long a deployment apply may run before the client gives up waiting.
/// This leaves room for several stacks and their image pulls, while a reply
/// that will never come fails the run within the hour instead of holding it
/// for a day.
const APPLY_REQUEST_TIMEOUT_MS: u64 = 60 * 60 * 1_000;
/// A restart waits for the service to be healthy again, as long as a
/// deployment waits for a stack.
const RESTART_REQUEST_TIMEOUT_MS: u64 = 30 * 60 * 1_000;
/// An access sync waits for its turn behind whatever changes the host,
/// such as a deployment step; it is short once it runs.
const TEAM_SYNC_REQUEST_TIMEOUT_MS: u64 = 30 * 60 * 1_000;
/// A cancellation waits for what it stops to end (ten seconds, then it is
/// dropped) and for a deployment it undoes to be rolled back.
const CANCEL_REQUEST_TIMEOUT_MS: u64 = 30 * 60 * 1_000;

/// Environment variable holding a private key for CI, used instead of the
/// mm-managed key. mm writes it to a 0600 temporary file per connection.
pub const SSH_KEY_ENVIRONMENT: &str = "MM_SSH_KEY";
use crate::domain::target::{NamedTarget, TargetTransport, validate_target_definition};
use crate::domain::workspace::WorkspaceSnapshot;

/// One host's SSH relay: a target that names one host, or one node of a
/// cluster target. A request goes to this host and nowhere else; mm never
/// retries it on another node, so a deployment cannot run twice.
#[derive(Debug, Clone)]
pub struct SshRemoteTransport {
    endpoint: SshEndpoint,
}

/// The key a connection authenticates with.
#[derive(Debug, Clone)]
enum IdentitySource {
    File(PathBuf),
    /// Private key text from `MM_SSH_KEY`.
    Inline(String),
}

/// Which key the client offers a host.
#[derive(Debug, Clone)]
pub enum ClientKey {
    /// `MM_SSH_KEY`, else the host's configured key file, else this device's
    /// mm-managed key when it exists.
    Default(Option<PathBuf>),
    /// This key and nothing else: not `MM_SSH_KEY`, not a configured key
    /// file. The MCP server uses it with the agent key, so a host sees an
    /// agent as an agent.
    Only(PathBuf),
}

impl ClientKey {
    fn source(&self, configured: Option<&PathBuf>) -> Option<IdentitySource> {
        match self {
            Self::Only(path) => Some(IdentitySource::File(path.clone())),
            Self::Default(device) => {
                let inline = std::env::var(SSH_KEY_ENVIRONMENT)
                    .ok()
                    .filter(|key| !key.trim().is_empty());
                match (inline, configured) {
                    (Some(key), _) => Some(IdentitySource::Inline(key)),
                    (None, Some(path)) => Some(IdentitySource::File(path.clone())),
                    (None, None) => device
                        .clone()
                        .filter(|path| path.is_file())
                        .map(IdentitySource::File),
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SshEndpoint {
    /// How messages name the host: `target 'staging' (host:22)` or
    /// `node 'app-2' (host:22)`.
    subject: String,
    host: String,
    port: u16,
    user: String,
    host_key: String,
    identity: Option<IdentitySource>,
    /// Whether the key is this device's agent key, offered alone.
    agent_key: bool,
    daemon_fingerprint: String,
    /// The connection kept open between requests, when this one may be.
    kept: Option<KeptConnection>,
    /// Opens the key when it is one mm keeps the passphrase of.
    keys: ManagedKeys,
}

/// How a transport authenticates and whether its connection is kept.
#[derive(Debug, Clone, Copy)]
pub struct ClientAccess<'a> {
    pub key: &'a ClientKey,
    pub reuse: &'a ConnectionReuse,
    /// The keys mm unlocks itself: `ManagedKeys::none()` uses every key
    /// file as it is.
    pub keys: &'a ManagedKeys,
}

impl SshRemoteTransport {
    pub fn new(target: &NamedTarget) -> Result<Self, MmError> {
        Self::for_target(
            target,
            ClientAccess {
                key: &ClientKey::Default(None),
                reuse: &ConnectionReuse::off(),
                keys: &ManagedKeys::none(),
            },
        )
    }

    /// The transport to a target that names one host.
    pub fn for_target(target: &NamedTarget, access: ClientAccess<'_>) -> Result<Self, MmError> {
        validate_target_definition(&target.definition)
            .map_err(|message| MmError::Target { message })?;
        Ok(Self {
            endpoint: SshEndpoint::new(
                format!("target '{}'", target.name),
                &target.definition.transport,
                access,
            )?,
        })
    }

    /// The socket of the connection this transport keeps open between
    /// requests, or `None` when it connects anew every time.
    #[must_use]
    pub fn kept_connection(&self) -> Option<&std::path::Path> {
        self.endpoint.kept.as_ref().map(KeptConnection::socket)
    }

    /// The transport to one node of a cluster target. Fails for a node whose
    /// keys the repo does not record yet.
    pub fn for_node(
        target: &NamedTarget,
        node: &crate::domain::target::TargetNode,
        access: ClientAccess<'_>,
    ) -> Result<Self, MmError> {
        validate_target_definition(&target.definition)
            .map_err(|message| MmError::Target { message })?;
        let transport = node.transport().ok_or_else(|| MmError::Unreachable {
            subject: format!("node '{}' ({})", node.name, node.address()),
            problem: "has no recorded keys yet, so mm cannot connect to it".to_owned(),
            help: "Run `mm target setup plan`, apply it, then `mm target setup record`".to_owned(),
        })?;
        Ok(Self {
            endpoint: SshEndpoint::new(format!("node '{}'", node.name), &transport, access)?,
        })
    }

    async fn connection(&self) -> Result<Connection, MmError> {
        self.endpoint.connect(None).await
    }

    /// A session that has `method`, and the request's id.
    ///
    /// A read goes out together with the hello, so the daemon's answer to
    /// both comes back in one round trip instead of two. It goes only to
    /// the host whose SSH key is pinned, and its answer counts only after
    /// the daemon's hello proved its identity. A request that changes the
    /// host is sent once the daemon has answered the hello: connecting is
    /// tried again when a kept connection turns out dead, and what changes
    /// a host must never be sent twice. So is a read that not every daemon
    /// this mm speaks to can read, once the hello said this one can.
    async fn open(
        &self,
        scope: RemoteScope,
        method: RemoteMethod,
        timeout: Duration,
        disconnect_state: MutationState,
    ) -> Result<(Connection, uuid::Uuid), MmError> {
        if method.is_mutation() || method.protocol() > MINIMUM_PROTOCOL_VERSION {
            let mut connection = self.connection().await?;
            let request_id = connection
                .ask(scope, method, timeout, disconnect_state)
                .await?;
            return Ok((connection, request_id));
        }
        let read = ReadRequest {
            scope,
            method,
            timeout,
        };
        let connection = self.endpoint.connect(Some(&read)).await?;
        let request_id = connection
            .sent
            .ok_or_else(|| protocol("the read was not sent with the hello"))?;
        Ok((connection, request_id))
    }
}

/// A request that changes nothing on the host, sent with the hello.
struct ReadRequest {
    scope: RemoteScope,
    method: RemoteMethod,
    timeout: Duration,
}

impl ReadRequest {
    /// The request's frame, with an id and a nonce of its own each time it
    /// is sent.
    fn frame(&self) -> Result<(uuid::Uuid, ClientFrame), MmError> {
        let request_id = uuid::Uuid::new_v4();
        let frame = request_frame(
            PROTOCOL_VERSION,
            request_id,
            self.scope.clone(),
            self.method.clone(),
            self.timeout,
        )?;
        Ok((request_id, frame))
    }
}

impl SshEndpoint {
    /// The timing phase of one request: from sending it to its answer, or
    /// to the start of its stream.
    fn request_phase(&self, method: &RemoteMethod) -> tracing::Span {
        crate::app::timing::phase_of("rpc", &format_args!("{} {}", method.name(), self.subject))
    }
}

impl SshEndpoint {
    fn new(
        label: String,
        transport: &TargetTransport,
        ClientAccess { key, reuse, keys }: ClientAccess<'_>,
    ) -> Result<Self, MmError> {
        let TargetTransport::Ssh {
            host,
            port,
            user,
            host_key,
            identity_file,
            daemon_fingerprint,
            ..
        } = transport
        else {
            return Err(MmError::Target {
                message: "remote transport requires an SSH target".to_owned(),
            });
        };
        let host_key = host_key.clone().ok_or_else(|| MmError::Security {
            message:
                "SSH target has no enrolled host public key; update the target before remote use"
                    .to_owned(),
        })?;
        let identity = key.source(identity_file.as_ref());
        // Only the user's own key file is ever kept open: not the agent
        // key, and not a key passed in the environment (`reuse.rs`).
        let kept = match (key, &identity) {
            (ClientKey::Default(_), Some(IdentitySource::File(path))) => reuse.connection(
                &reuse::Peer {
                    user,
                    host,
                    port: *port,
                    host_key: &host_key,
                },
                path,
                keys.manages(path),
            ),
            _ => None,
        };
        Ok(Self {
            subject: format!("{label} ({host}:{port})"),
            host: host.clone(),
            port: *port,
            user: user.clone(),
            host_key,
            identity,
            agent_key: matches!(key, ClientKey::Only(_)),
            daemon_fingerprint: daemon_fingerprint.clone().ok_or_else(|| MmError::Security {
                message: "SSH target has no enrolled daemon fingerprint; update the target before remote use"
                    .to_owned(),
            })?,
            kept,
            keys: keys.clone(),
        })
    }

    /// `ssh` with only the options mm allows: the pinned host key, this
    /// endpoint's key, and nothing from `~/.ssh` or an agent. With
    /// `askpass`, the key is one mm keeps the passphrase of: `ssh` may ask
    /// for it once, and only the helper answers.
    fn ssh_command(
        &self,
        known_hosts: &std::path::Path,
        share: bool,
        askpass: Option<&Askpass>,
    ) -> Result<(Command, Option<tempfile::NamedTempFile>), MmError> {
        let mut command = Command::new("ssh");
        command
            .kill_on_drop(true)
            .arg("-F")
            .arg("/dev/null")
            .arg("-T")
            .arg("-p")
            .arg(self.port.to_string())
            .arg("-o")
            // In batch mode `ssh` never asks for a passphrase, so an
            // encrypted key cannot be used; without it, every other
            // question stays switched off below and the helper answers
            // nothing but a passphrase prompt.
            .arg(if askpass.is_some() {
                "BatchMode=no"
            } else {
                "BatchMode=yes"
            })
            .arg("-o")
            .arg("NumberOfPasswordPrompts=1")
            .arg("-o")
            .arg("IdentitiesOnly=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=yes")
            .arg("-o")
            .arg(format!("UserKnownHostsFile={}", known_hosts.display()))
            .arg("-o")
            .arg("GlobalKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("CheckHostIP=no")
            .arg("-o")
            .arg("ClearAllForwardings=yes")
            .arg("-o")
            .arg("PermitLocalCommand=no")
            .arg("-o")
            .arg("RequestTTY=no")
            .arg("-o")
            .arg("ConnectTimeout=10")
            // A long apply sends nothing for minutes, and middleboxes drop
            // idle connections silently: probe so a dead link ends the
            // request instead of leaving the client waiting forever.
            .arg("-o")
            .arg("ServerAliveInterval=15")
            .arg("-o")
            .arg("ServerAliveCountMax=4")
            .arg("-o")
            .arg("TCPKeepAlive=yes");
        // mm never uses ~/.ssh or an agent: the only credential is the key
        // it was told to use. A connection is shared only through mm's own
        // socket, and only for a key that may be kept (`reuse.rs`).
        match self.kept.as_ref().filter(|_| share) {
            Some(kept) => {
                for option in kept.options() {
                    command.arg("-o").arg(option);
                }
                // The connection outlives this command: it must not hold
                // the directory mm was started in.
                command.current_dir("/");
            }
            None => {
                command
                    .arg("-o")
                    .arg("ControlMaster=no")
                    .arg("-o")
                    .arg("ControlPath=none");
            }
        }
        for option in [
            "IdentityAgent=none",
            "ForwardAgent=no",
            "ForwardX11=no",
            "PasswordAuthentication=no",
            "KbdInteractiveAuthentication=no",
            "UpdateHostKeys=no",
            "ServerAliveInterval=15",
        ] {
            command.arg("-o").arg(option);
        }
        let inline_key = self.identify(&mut command, askpass)?;
        command
            .arg("--")
            .arg(format!("{}@{}", self.user, self.host))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Ok((command, inline_key))
    }

    /// Names the key `ssh` offers, and the helper that answers its
    /// passphrase prompt when mm keeps the passphrase. Returns the file a
    /// key from `MM_SSH_KEY` was written to, which lives as long as the
    /// connection.
    fn identify(
        &self,
        command: &mut Command,
        askpass: Option<&Askpass>,
    ) -> Result<Option<tempfile::NamedTempFile>, MmError> {
        let inline_key = match &self.identity {
            Some(IdentitySource::File(path)) => {
                // A rotation in progress: the new key first, so a host
                // that lists it uses it, and the current key for the
                // hosts that do not yet.
                if let Some(next) = self.keys.next_key(path) {
                    command.arg("-i").arg(next);
                }
                command.arg("-i").arg(path);
                None
            }
            Some(IdentitySource::Inline(key)) => {
                let file = private_key_file(key)?;
                command.arg("-i").arg(file.path());
                Some(file)
            }
            None => None,
        };
        if let Some(askpass) = askpass {
            command.envs(askpass.environment());
        }
        Ok(inline_key)
    }
}

impl SshRemoteTransport {
    async fn unary(
        &self,
        scope: RemoteScope,
        method: RemoteMethod,
        timeout: Duration,
    ) -> Result<RemoteResult, MmError> {
        scope
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let disconnect_state = if method.is_mutation() {
            MutationState::Unknown
        } else {
            MutationState::NotStarted
        };
        let phase = self.endpoint.request_phase(&method);
        let asked = method.name();
        let (mut connection, request_id) =
            self.open(scope, method, timeout, disconnect_state).await?;
        let _phase = phase;
        let frame = tokio::time::timeout(timeout, connection.stdout.next())
            .await
            .map_err(|_| late(&connection.subject, asked, timeout, disconnect_state))??
            .ok_or_else(|| disconnect(&connection.subject, disconnect_state))?;
        if let Err(error) = connection.stdin.shutdown().await {
            tracing::debug!(%request_id, %error, "remote unary stdin shutdown failed");
        }
        let agreed = connection.protocol;
        match frame {
            ServerFrame::Result {
                version,
                request_id: received,
                result,
            } if version == agreed && received == request_id => Ok(*result),
            ServerFrame::Error {
                version,
                request_id: received,
                error,
            } if version == agreed && (received.is_none() || received == Some(request_id)) => {
                Err(server_error(error))
            }
            _ => Err(protocol("remote response did not match the request")),
        }
    }

    async fn stream(
        &self,
        scope: RemoteScope,
        method: RemoteMethod,
        timeout: Duration,
        cancellation: OperationCancellation,
        kind: StreamKind,
    ) -> Result<StreamReceiver, MmError> {
        scope
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let disconnect_state = if method.is_mutation() {
            MutationState::Unknown
        } else {
            MutationState::NotStarted
        };
        let phase = self.endpoint.request_phase(&method);
        let asked = method.name();
        let (mut connection, request_id) =
            self.open(scope, method, timeout, disconnect_state).await?;
        let start = tokio::time::timeout(timeout, connection.stdout.next())
            .await
            .map_err(|_| late(&connection.subject, asked, timeout, disconnect_state))??
            .ok_or_else(|| disconnect(&connection.subject, disconnect_state))?;
        let agreed = connection.protocol;
        match start {
            ServerFrame::Result {
                version,
                request_id: received,
                result,
            } if version == agreed
                && received == request_id
                && *result == RemoteResult::StreamStarted => {}
            ServerFrame::Error {
                version,
                request_id: received,
                error,
            } if version == agreed && (received.is_none() || received == Some(request_id)) => {
                return Err(server_error(error));
            }
            _ => return Err(protocol("remote stream start response is invalid")),
        }
        drop(phase);
        let (sender, receiver) = tokio::sync::mpsc::channel(128);
        let task_cancellation = cancellation.clone();
        tokio::spawn(receive_stream(StreamReceiveContext {
            connection,
            request_id,
            timeout,
            disconnect_state,
            kind,
            cancellation: task_cancellation,
            sender,
        }));
        Ok(StreamReceiver {
            receiver,
            cancellation,
        })
    }
}

include!("client/connect.rs");
include!("client/transport.rs");
include!("client/forward.rs");
include!("client/deliver.rs");
include!("client/stream.rs");

#[path = "client/reuse.rs"]
mod reuse;
use crate::adapters::keys::Askpass;
use reuse::KeptConnection;
pub use reuse::{ConnectionReuse, SSH_REUSE_ENVIRONMENT};

struct Connection {
    /// How messages name the host: `node 'app-2' (host:22)`.
    subject: String,
    /// The protocol the daemon's hello agreed on, which every later frame
    /// of the session has.
    protocol: u16,
    /// The release of the host's `mmd`, as its hello said.
    daemon_version: String,
    child: Child,
    stdin: ChildStdin,
    /// Frames are read in loops that also wake for other things; the
    /// reader keeps a frame that arrives in pieces.
    stdout: FrameReader<ChildStdout>,
    /// The id of the read that went out with the hello, when one did.
    sent: Option<uuid::Uuid>,
    _known_hosts: tempfile::NamedTempFile,
    _inline_key: Option<tempfile::NamedTempFile>,
    /// Answers the key's passphrase prompt until the daemon has answered:
    /// by then `ssh` has authenticated.
    askpass: Option<Askpass>,
}

/// Writes `MM_SSH_KEY` to a 0600 file that lives as long as the connection.
fn private_key_file(key: &str) -> Result<tempfile::NamedTempFile, MmError> {
    let mut file = tempfile::Builder::new()
        .prefix("mm-ssh-key-")
        .permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .tempfile()
        .map_err(|source| MmError::Io {
            context: format!("creating a private file for {SSH_KEY_ENVIRONMENT}"),
            source,
        })?;
    let mut text = key.trim().to_owned();
    text.push('\n');
    std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|source| MmError::Io {
        context: format!("writing {SSH_KEY_ENVIRONMENT} to a private file"),
        source,
    })?;
    Ok(file)
}

impl Connection {
    /// Exchanges hellos. `Err(None)` means the connection ended or stalled
    /// before the daemon answered; the caller explains that from what `ssh`
    /// wrote.
    async fn handshake(
        &mut self,
        daemon_fingerprint: &str,
        patience: Duration,
        read: Option<&ClientFrame>,
    ) -> Result<(), Option<HandshakeRefusal>> {
        let nonce = uuid::Uuid::new_v4();
        let hello = ClientFrame::Hello {
            version: PROTOCOL_VERSION,
            minimum: MINIMUM_PROTOCOL_VERSION,
            hello: ClientHello {
                client_nonce: nonce,
                expected_daemon_fingerprint: daemon_fingerprint.to_owned(),
                client_version: env!("CARGO_PKG_VERSION").to_owned(),
            },
        };
        // Both frames in one write: they travel together.
        let mut bytes = Vec::new();
        for frame in std::iter::once(&hello).chain(read) {
            let Ok(encoded) = encode_frame(frame) else {
                return Err(None);
            };
            let Ok(length) = u32::try_from(encoded.len()) else {
                return Err(None);
            };
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(&encoded);
        }
        if self.stdin.write_all(&bytes).await.is_err() || self.stdin.flush().await.is_err() {
            return Err(None);
        }
        let frame = tokio::time::timeout(patience, self.stdout.next())
            .await
            .map_err(|_| None)?
            .map_err(|_| None)?
            .ok_or(None)?;
        (self.protocol, self.daemon_version) = agreed(frame, nonce, daemon_fingerprint)?;
        Ok(())
    }

    /// Fails before sending what needs protocol `needed` when the host's
    /// daemon agreed on an older one.
    fn require(&self, needed: u16, what: &str) -> Result<(), MmError> {
        crate::domain::remote::require_protocol(
            self.protocol,
            needed,
            (&self.subject, &self.daemon_version),
            what,
        )
    }

    /// Sends `method` once the host's daemon is known to read it, and
    /// returns its request's id. A connection lost while sending is
    /// reported with `lost`.
    async fn ask(
        &mut self,
        scope: RemoteScope,
        method: RemoteMethod,
        timeout: Duration,
        lost: MutationState,
    ) -> Result<uuid::Uuid, MmError> {
        self.require(method.protocol(), &format!("`{}`", method.name()))?;
        let request_id = uuid::Uuid::new_v4();
        self.send_request(request_id, scope, method, timeout)
            .await
            .map_err(|_| disconnect(&self.subject, lost))?;
        Ok(request_id)
    }

    async fn send_request(
        &mut self,
        request_id: uuid::Uuid,
        scope: RemoteScope,
        method: RemoteMethod,
        timeout: Duration,
    ) -> Result<(), MmError> {
        let frame = request_frame(self.protocol, request_id, scope, method, timeout)?;
        write_frame(&mut self.stdin, &frame).await.map(|_| ())
    }
}

/// One request's frame, of protocol `version`: its deadline is `timeout`
/// from now.
fn request_frame(
    version: u16,
    request_id: uuid::Uuid,
    scope: RemoteScope,
    method: RemoteMethod,
    timeout: Duration,
) -> Result<ClientFrame, MmError> {
    let timeout_ms =
        i64::try_from(timeout.as_millis()).map_err(|_| protocol("timeout overflow"))?;
    Ok(ClientFrame::Request {
        version,
        request_id,
        operation_nonce: uuid::Uuid::new_v4(),
        deadline_unix_ms: now_unix_ms()?.saturating_add(timeout_ms),
        scope,
        method: Box::new(method),
    })
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Err(error) = self.child.start_kill() {
            tracing::warn!(%error, "SSH relay shutdown failed");
        }
    }
}

/// The host's side of the connection ended before it answered: a host,
/// relay, or daemon that went away, never a state or protocol error.
fn disconnect(subject: &str, state: MutationState) -> MmError {
    MmError::Unreachable {
        subject: subject.to_owned(),
        problem:
            "ended the connection before it answered (mmd restarted, or the connection dropped)"
                .to_owned(),
        help: after_lost_answer(state),
    }
}

/// The host did not answer `method` within `timeout`.
fn late(subject: &str, method: &str, timeout: Duration, state: MutationState) -> MmError {
    MmError::Timeout {
        message: format!(
            "{subject} did not answer the request '{method}' within {} s. {}",
            timeout.as_secs(),
            after_lost_answer(state)
        ),
    }
}

/// What to do when an answer did not come: a change may have gone through.
fn after_lost_answer(state: MutationState) -> String {
    match state {
        MutationState::NotStarted => {
            "Nothing was changed. Run the command again; `mm target nodes` shows whether the host answers"
                .to_owned()
        }
        _ => "The change may or may not have been applied and is never retried automatically: `mm deploy status` shows what the host runs and anything still in progress"
            .to_owned(),
    }
}

fn protocol(message: &str) -> MmError {
    MmError::RuntimeResponse {
        message: format!("remote protocol error: {message}"),
    }
}

fn now_unix_ms() -> Result<i64, MmError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| protocol("system clock is before the Unix epoch"))?;
    i64::try_from(elapsed.as_millis()).map_err(|_| protocol("system clock is out of range"))
}

#[path = "client/errors.rs"]
mod errors;
use errors::server_error;

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "client/tests/connect.rs"]
mod connect_tests;
