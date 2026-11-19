use std::fs;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};

use crate::adapters::remote::codec::{decode_frame, read_raw_frame, write_frame, write_raw_frame};
use crate::domain::error::MmError;
use crate::domain::remote::{ClientFrame, FrameHead, PROTOCOL_VERSION};

pub async fn serve_stdio_relay(
    socket_path: &Path,
    expected_daemon_uid: u32,
    idle_timeout: Duration,
) -> Result<(), MmError> {
    validate_forced_command_environment()?;
    let session = authenticated_session()?;
    validate_socket_path(socket_path, expected_daemon_uid)?;
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(|source| MmError::Io {
            context: format!("connecting restricted relay to {}", socket_path.display()),
            source,
        })?;
    let credentials = stream.peer_cred().map_err(|source| MmError::Io {
        context: "authenticating daemon Unix peer".to_owned(),
        source,
    })?;
    if credentials.uid() != expected_daemon_uid {
        return Err(MmError::Security {
            message: "relay connected to a Unix peer with the wrong daemon UID".to_owned(),
        });
    }
    write_frame(
        &mut stream,
        &ClientFrame::RelaySession {
            version: PROTOCOL_VERSION,
            key_fingerprint: session.key_fingerprint,
            source: session.source,
            session_id: uuid::Uuid::new_v4(),
        },
    )
    .await?;
    let (mut socket_reader, mut socket_writer) = stream.into_split();
    let mut stdin = ThreadStdin::spawn()?;
    let mut stdout = tokio::io::stdout();
    relay_session(
        (&mut stdin, &mut stdout),
        (&mut socket_reader, &mut socket_writer),
        idle_timeout,
    )
    .await
}

/// How long the daemon gets to end the session after the client is gone.
const CLIENT_GONE_GRACE: Duration = Duration::from_secs(2);

/// Relays frames both ways until the daemon ends the session, or until
/// the client is gone. The relay never outlives its client: when standard
/// input ends, the client or its SSH session went away and nobody reads
/// what the daemon still sends, so the daemon is told (its socket is
/// half-closed), given `CLIENT_GONE_GRACE` to hang up, and left to itself.
/// Before, a relay whose daemon did not answer stayed for ever.
pub async fn relay_session<I, O, R, W>(
    (stdin, stdout): (&mut I, &mut O),
    (socket_reader, socket_writer): (&mut R, &mut W),
    idle_timeout: Duration,
) -> Result<(), MmError>
where
    I: AsyncRead + Unpin + ?Sized,
    O: AsyncWrite + Unpin + ?Sized,
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    // The idle limit guards the handshake. Once a request is relayed, the
    // daemon's request deadline governs: a forward, `logs -f` or an apply
    // may be quiet for a long time.
    let requested = RequestRelayed::new();
    let upstream = relay_client_frames(stdin, socket_writer, idle_timeout, &requested);
    let downstream = relay_server_frames(socket_reader, stdout, idle_timeout, &requested);
    tokio::pin!(upstream);
    tokio::pin!(downstream);
    tokio::select! {
        result = &mut upstream => {
            let drained = tokio::time::timeout(CLIENT_GONE_GRACE, downstream).await;
            match (result, drained) {
                (Ok(()), Ok(result)) => result,
                (Ok(()), Err(_)) | (Err(_), Ok(Ok(()))) => Ok(()),
                (Err(error), _) => Err(error),
            }
        }
        result = &mut downstream => result,
    }
}

async fn relay_client_frames<R, W>(
    reader: &mut R,
    writer: &mut W,
    idle_timeout: Duration,
    requested: &RequestRelayed,
) -> Result<(), MmError>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    loop {
        let frame = requested
            .read_frame(reader, idle_timeout, "restricted relay client idle timeout")
            .await?;
        let Some(bytes) = frame else {
            writer.shutdown().await.map_err(|source| MmError::Io {
                context: "half-closing relay Unix socket".to_owned(),
                source,
            })?;
            return Ok(());
        };
        let frame: FrameHead = decode_frame(&bytes)?;
        write_raw_frame(writer, &bytes).await?;
        if frame.kind == "request" {
            requested.mark();
        }
    }
}

async fn relay_server_frames<R, W>(
    reader: &mut R,
    writer: &mut W,
    idle_timeout: Duration,
    requested: &RequestRelayed,
) -> Result<(), MmError>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    let mut session = None;
    loop {
        let frame = requested
            .read_frame(reader, idle_timeout, "restricted relay daemon idle timeout")
            .await?;
        let Some(bytes) = frame else {
            writer.shutdown().await.map_err(|source| MmError::Io {
                context: "half-closing relay standard output".to_owned(),
                source,
            })?;
            return Ok(());
        };
        let frame: FrameHead = decode_frame(&bytes)?;
        if *session.get_or_insert(frame.version) != frame.version {
            return Err(protocol(
                "restricted relay rejected a daemon protocol version other than the session's",
            ));
        }
        write_raw_frame(writer, &bytes).await?;
    }
}

/// Whether the client's request has gone through to the daemon. Until it
/// has, every read is bounded by the idle limit; after it, the daemon's
/// request deadline governs.
///
/// The daemon's reply is read while the request is still on its way: the
/// daemon's hello arrives first, and the client sends its request only after
/// reading it. So a read that is already waiting when the request goes
/// through drops its idle limit then. Re-checking only between frames cut off
/// every reply that took longer than the limit, such as a long apply.
pub(super) struct RequestRelayed(watch::Sender<bool>);

impl RequestRelayed {
    pub(super) fn new() -> Self {
        Self(watch::channel(false).0)
    }

    fn mark(&self) {
        self.0.send_replace(true);
    }

    async fn read_frame<R>(
        &self,
        reader: &mut R,
        idle_timeout: Duration,
        idle_message: &'static str,
    ) -> Result<Option<Vec<u8>>, MmError>
    where
        R: AsyncRead + Unpin + ?Sized,
    {
        let mut relayed = self.0.subscribe();
        // The read is never cancelled, so no partial frame is lost.
        let read = read_raw_frame(reader);
        tokio::pin!(read);
        if !*relayed.borrow_and_update() {
            tokio::select! {
                frame = &mut read => return frame,
                () = tokio::time::sleep(idle_timeout) => return Err(protocol(idle_message)),
                Ok(_) = relayed.wait_for(|relayed| *relayed) => {}
            }
        }
        read.await
    }
}

/// Standard input, read on a plain thread.
///
/// `tokio::io::stdin` reads on the runtime's blocking pool, and shutting the
/// runtime down waits for that read. When the daemon side ended first, the
/// relay could not exit: it kept SSH's standard output open, and the client
/// waited for a reply that would never come. A plain thread does not
/// hold the process when `main` returns.
struct ThreadStdin {
    chunks: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    pending: Vec<u8>,
    offset: usize,
}

impl ThreadStdin {
    fn spawn() -> Result<Self, MmError> {
        use std::io::Read as _;

        let (sender, chunks) = mpsc::channel(4);
        std::thread::Builder::new()
            .name("mm-relay-stdin".to_owned())
            .spawn(move || {
                let mut stdin = std::io::stdin().lock();
                let mut buffer = vec![0_u8; 64 * 1024];
                loop {
                    let chunk = match stdin.read(&mut buffer) {
                        Ok(0) => return,
                        Ok(read) => Ok(buffer[..read].to_vec()),
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) => Err(error),
                    };
                    let failed = chunk.is_err();
                    if sender.blocking_send(chunk).is_err() || failed {
                        return;
                    }
                }
            })
            .map_err(|source| MmError::Io {
                context: "starting the relay's standard input reader".to_owned(),
                source,
            })?;
        Ok(Self {
            chunks,
            pending: Vec::new(),
            offset: 0,
        })
    }
}

impl AsyncRead for ThreadStdin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            if self.offset < self.pending.len() {
                let available = self.pending.len() - self.offset;
                let copied = available.min(buffer.remaining());
                let start = self.offset;
                buffer.put_slice(&self.pending[start..start + copied]);
                self.offset += copied;
                return Poll::Ready(Ok(()));
            }
            match self.chunks.poll_recv(context) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.pending = chunk;
                    self.offset = 0;
                }
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Err(error)),
                // End of input: the reader thread has finished.
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

fn validate_forced_command_environment() -> Result<(), MmError> {
    if std::env::var_os("SSH_CONNECTION").is_none() {
        return Err(MmError::Security {
            message: "relay serve is available only as an SSH forced command".to_owned(),
        });
    }
    if std::env::var_os("SSH_ORIGINAL_COMMAND").is_some_and(|value| !value.is_empty()) {
        return Err(MmError::Security {
            message: "restricted relay rejects every requested SSH command".to_owned(),
        });
    }
    Ok(())
}

struct AuthenticatedSession {
    key_fingerprint: String,
    source: String,
}

fn authenticated_session() -> Result<AuthenticatedSession, MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let path = std::env::var_os("SSH_USER_AUTH")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| MmError::Security {
            message: "sshd ExposeAuthInfo did not provide authenticated key attribution".to_owned(),
        })?;
    let metadata = fs::symlink_metadata(&path).map_err(|source| MmError::Io {
        context: "inspecting OpenSSH authentication attribution".to_owned(),
        source,
    })?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::getuid().as_raw()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > 16 * 1024
    {
        return Err(MmError::Security {
            message: "OpenSSH authentication attribution has an unsafe type, owner, mode, or size"
                .to_owned(),
        });
    }
    let contents = fs::read_to_string(&path).map_err(|source| MmError::Io {
        context: "reading OpenSSH authentication attribution".to_owned(),
        source,
    })?;
    let fingerprint = authentication_fingerprint(&contents)?;
    if fingerprint.len() > 72
        || !fingerprint.strip_prefix("SHA256:").is_some_and(|digest| {
            !digest.is_empty()
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        })
    {
        return Err(MmError::Security {
            message: "OpenSSH authentication fingerprint is invalid".to_owned(),
        });
    }
    let source = std::env::var("SSH_CONNECTION").map_err(|_| MmError::Security {
        message: "OpenSSH connection attribution is unavailable".to_owned(),
    })?;
    if source.is_empty()
        || source.len() > 512
        || !source
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b' ' | b'-'))
    {
        return Err(MmError::Security {
            message: "OpenSSH connection attribution is invalid".to_owned(),
        });
    }
    Ok(AuthenticatedSession {
        key_fingerprint: fingerprint,
        source,
    })
}

fn authentication_fingerprint(contents: &str) -> Result<String, MmError> {
    use base64::Engine as _;
    use sha2::Digest as _;

    for line in contents.lines().rev() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if let Some(fingerprint) = fields.iter().find(|field| field.starts_with("SHA256:")) {
            return Ok((*fingerprint).to_owned());
        }
        if fields.first() == Some(&"publickey")
            && fields
                .get(1)
                .is_some_and(|kind| kind.starts_with("ssh-") || kind.starts_with("ecdsa-"))
        {
            let encoded = fields.get(2).ok_or_else(|| MmError::Security {
                message: "OpenSSH authentication attribution contains an incomplete public key"
                    .to_owned(),
            })?;
            let blob = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| MmError::Security {
                    message: "OpenSSH authentication attribution contains invalid key material"
                        .to_owned(),
                })?;
            return Ok(format!(
                "SHA256:{}",
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(sha2::Sha256::digest(blob))
            ));
        }
    }
    Err(MmError::Security {
        message: "OpenSSH authentication attribution contains no public key".to_owned(),
    })
}

fn validate_socket_path(path: &Path, expected_uid: u32) -> Result<(), MmError> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    if !path.is_absolute() {
        return Err(MmError::Security {
            message: "relay daemon socket path must be absolute".to_owned(),
        });
    }
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting relay daemon socket {}", path.display()),
        source,
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_socket()
        || metadata.uid() != expected_uid
    {
        return Err(MmError::Security {
            message: "relay socket is a symlink, non-socket, or owned by a foreign UID".to_owned(),
        });
    }
    Ok(())
}

fn protocol(message: &str) -> MmError {
    MmError::RuntimeResponse {
        message: message.to_owned(),
    }
}

#[cfg(test)]
#[path = "relay_tests.rs"]
mod tests;
