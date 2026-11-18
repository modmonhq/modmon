//! Keeping the SSH connection to a host open between requests.
//!
//! Every request used to pay for a new SSH connection: key exchange and
//! authentication, a dozen round trips. With reuse, the first connection to
//! a host stays open in the background (OpenSSH connection sharing) and
//! later requests, from this `mm` or the next one, open a session on it.
//!
//! What a kept connection is: a way to open relay sessions on that host as
//! the key that made it, without the key. So:
//!
//! - Its socket lives in `MM_HOME/run/ssh`, a directory only the user can
//!   enter, and OpenSSH creates the socket readable and writable by the
//!   user alone. `mm` refuses a directory that is anything else.
//! - Only a key that `mm` can use with nobody present is kept: a key file
//!   that is not encrypted, or this device's own key, whose passphrase
//!   `mm` takes from a vault that answers any program running as the user
//!   (`adapters/keys/vault.rs`). Such a program can use the key through
//!   the file or the vault, so the socket gives it nothing new, and it
//!   gives less: sessions with the relay of one host, for five idle
//!   minutes, and nothing to carry away. A security key, which asks for a
//!   touch per connection, a key file under a passphrase `mm` does not
//!   keep, and a key passed in `MM_SSH_KEY` always connect anew. A vault
//!   that told programs apart would end this for the device key too.
//! - The agent key is never kept. `mm mcp serve` connects anew for every
//!   call, so no call can ride on a connection another one authorized.
//! - The host still decides per request: each session runs the relay
//!   again, and `mmd` authenticates the key and checks the policy for every
//!   request, so access removed on the host ends with the next request.
//! - It closes after `IDLE_SECONDS` without a session, when the key file
//!   it was made with is gone or replaced (checked whenever `mm` connects),
//!   and when `mm` changes keys or access (`close_all`). Closing takes no
//!   new session on it and lets the open ones finish: another `mm` may be
//!   in the middle of a request on the same connection, such as a long
//!   upload, and the next request authenticates anew either way.
//! - `MM_SSH_REUSE=off` turns reuse off and closes what was kept.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use zeroize::Zeroizing;

use crate::domain::key::{KeptConnection as KeptConnectionRecord, PrivateKeyHeader};

/// Set to `off` (or `0`, `no`, `false`) to connect anew for every request.
pub const SSH_REUSE_ENVIRONMENT: &str = "MM_SSH_REUSE";

/// How long a connection stays open with no session on it.
const IDLE_SECONDS: u64 = 300;

/// The longest socket path that fits `sun_path` on every platform (104
/// bytes on macOS, with its terminator), less the 17 bytes OpenSSH appends
/// while it creates the socket.
const SOCKET_PATH_LIMIT: usize = 104 - 1 - 17;

/// Where connections are kept, if they are.
#[derive(Debug, Clone, Default)]
pub struct ConnectionReuse {
    directory: Option<PathBuf>,
}

/// The host and account a connection goes to.
pub(super) struct Peer<'a> {
    pub(super) user: &'a str,
    pub(super) host: &'a str,
    pub(super) port: u16,
    pub(super) host_key: &'a str,
}

impl ConnectionReuse {
    /// No connection is kept.
    #[must_use]
    pub fn off() -> Self {
        Self::default()
    }

    /// Connections are kept under `run_directory` (`MM_HOME/run`), unless
    /// `MM_SSH_REUSE` turns that off; then the ones kept earlier are closed.
    /// Connections whose key file is gone or was replaced are closed too.
    #[must_use]
    pub fn under(run_directory: &Path) -> Self {
        if turned_off(std::env::var(SSH_REUSE_ENVIRONMENT).ok().as_deref()) {
            Self::close_all_under(run_directory);
            return Self::off();
        }
        let reuse = Self::kept_under(run_directory);
        reuse.close_orphans();
        reuse
    }

    /// As `under`, whatever the environment says.
    #[must_use]
    pub fn kept_under(run_directory: &Path) -> Self {
        Self {
            directory: Some(run_directory.join("ssh")),
        }
    }

    /// Closes every connection kept under `run_directory`. For key and
    /// access changes: nothing made before them stays usable.
    pub fn close_all_under(run_directory: &Path) {
        for (socket, owner) in Self::kept_under(run_directory).kept() {
            close(&socket, &owner);
        }
    }

    /// The kept connection for `peer` and the key file at `key`, or `None`
    /// when this connection must be made anew every time. `managed` says
    /// that `mm` keeps the key's passphrase, so it can use the key with
    /// nobody present although the file is encrypted.
    pub(super) fn connection(
        &self,
        peer: &Peer<'_>,
        key: &Path,
        managed: bool,
    ) -> Option<KeptConnection> {
        let directory = self.directory.as_ref()?;
        let public = unattended_key(key, managed)?;
        let name = blake3::hash(
            format!(
                "mm-ssh-1\0{}\0{}\0{}\0{}\0{}\0{}",
                peer.user,
                peer.host,
                peer.port,
                peer.host_key,
                key.display(),
                public
            )
            .as_bytes(),
        )
        .to_hex()[..16]
            .to_owned();
        let socket = directory.join(&name);
        if socket.as_os_str().len() > SOCKET_PATH_LIMIT {
            tracing::debug!(
                socket = %socket.display(),
                "the mm home is too deep for a connection socket; connecting anew every time"
            );
            return None;
        }
        if let Err(error) = private_directory(directory) {
            tracing::debug!(%error, "connections are not kept");
            return None;
        }
        Some(KeptConnection {
            socket,
            owner: directory.join(format!("{name}.{OWNER_EXTENSION}")),
            key: key.to_path_buf(),
            public,
            peer: format!("{}@{}:{}", peer.user, peer.host, peer.port),
            managed,
        })
    }

    /// The connections kept under `run_directory`, for `mm target
    /// connections`.
    #[must_use]
    pub fn list_under(run_directory: &Path) -> Vec<KeptConnectionRecord> {
        Self::kept_under(run_directory)
            .kept()
            .into_iter()
            .filter(|(socket, _)| socket.exists())
            .map(|(socket, owner)| {
                let recorded = Owner::read(&owner);
                KeptConnectionRecord {
                    peer: recorded.as_ref().and_then(|owner| owner.peer.clone()),
                    key: recorded.map(|owner| owner.key),
                    open: answers(&socket),
                    age_seconds: std::fs::metadata(&socket)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|made| made.elapsed().ok())
                        .map(|age| age.as_secs()),
                }
            })
            .collect()
    }

    /// Closes the connections whose key file is gone, was replaced, or can
    /// no longer be used unattended.
    fn close_orphans(&self) {
        for (socket, owner) in self.kept() {
            let current = Owner::read(&owner).is_some_and(|recorded| {
                unattended_key(&recorded.key, recorded.managed).as_deref()
                    == Some(recorded.public.as_str())
            });
            if !current {
                close(&socket, &owner);
            }
        }
    }

    /// Every socket in the directory with the file that names its key,
    /// whichever of the two is there.
    fn kept(&self) -> Vec<(PathBuf, PathBuf)> {
        let Some(directory) = &self.directory else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Vec::new();
        };
        let names = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                Some(path.file_stem()?.to_str()?.to_owned())
            })
            .collect::<std::collections::BTreeSet<_>>();
        names
            .into_iter()
            .map(|name| {
                (
                    directory.join(&name),
                    directory.join(format!("{name}.{OWNER_EXTENSION}")),
                )
            })
            .collect()
    }

    /// Where the socket of a kept connection would be; for tests and
    /// diagnostics.
    #[must_use]
    pub fn directory(&self) -> Option<&Path> {
        self.directory.as_deref()
    }
}

/// The file next to a socket that names the key its connection was made with.
const OWNER_EXTENSION: &str = "owner";

fn turned_off(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        ["off", "0", "no", "false"]
            .iter()
            .any(|off| value.trim().eq_ignore_ascii_case(off))
    })
}

/// One host's kept connection: the socket OpenSSH shares it through.
#[derive(Debug, Clone)]
pub(super) struct KeptConnection {
    socket: PathBuf,
    /// Names the key the connection was made with, for `close_orphans`.
    owner: PathBuf,
    key: PathBuf,
    public: String,
    /// `user@host:port`.
    peer: String,
    managed: bool,
}

/// What the file next to a socket records: a hash of the public half of
/// the key, the key file, the peer, and whether `mm` keeps the key's
/// passphrase. One per line; a file written before the last two existed
/// has neither.
struct Owner {
    public: String,
    key: PathBuf,
    peer: Option<String>,
    managed: bool,
}

impl Owner {
    fn read(path: &Path) -> Option<Self> {
        let recorded = std::fs::read_to_string(path).ok()?;
        let mut lines = recorded.lines();
        Some(Self {
            public: lines.next()?.to_owned(),
            key: PathBuf::from(lines.next()?),
            peer: lines.next().map(str::to_owned),
            managed: lines.next() == Some(MANAGED),
        })
    }
}

/// Marks a connection made with a key whose passphrase `mm` keeps.
const MANAGED: &str = "managed";

impl KeptConnection {
    /// The `ssh` options that use the kept connection, or make it.
    pub(super) fn options(&self) -> [String; 3] {
        [
            "ControlMaster=auto".to_owned(),
            format!("ControlPath={}", self.socket.display()),
            format!("ControlPersist={IDLE_SECONDS}"),
        ]
    }

    /// Whether a connection is there to be used.
    pub(super) fn is_open(&self) -> bool {
        self.socket.exists()
    }

    pub(super) fn socket(&self) -> &Path {
        &self.socket
    }

    /// Records which key the connection is made with, before it is made.
    pub(super) fn record_owner(&self) {
        if self.owner.exists() {
            return;
        }
        let recorded = format!(
            "{}\n{}\n{}\n{}\n",
            self.public,
            self.key.display(),
            self.peer,
            if self.managed { MANAGED } else { "file" }
        );
        if let Err(error) = write_private(&self.owner, recorded.as_bytes()) {
            tracing::debug!(%error, "could not record a kept connection's key");
        }
    }

    /// Ends the kept connection, as after a session on it failed.
    pub(super) fn close(&self) {
        close(&self.socket, &self.owner);
    }
}

/// Whether the background `ssh` behind `socket` still answers.
fn answers(socket: &Path) -> bool {
    Command::new("ssh")
        .arg("-F")
        .arg("/dev/null")
        .arg("-o")
        .arg(format!("ControlPath={}", socket.display()))
        .args(["-O", "check", "mm"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Asks the background `ssh` to take no more sessions and removes its
/// socket. Sessions already open on it, which can be another `mm`'s,
/// finish; the background `ssh` ends with the last of them. (`-O exit`
/// would cut them.)
fn close(socket: &Path, owner: &Path) {
    if socket.exists() {
        // `-O stop` talks to the socket only; the host name is not used.
        let asked = Command::new("ssh")
            .arg("-F")
            .arg("/dev/null")
            .arg("-o")
            .arg(format!("ControlPath={}", socket.display()))
            .args(["-O", "stop", "mm"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(wait_briefly);
        if let Err(error) = asked {
            tracing::debug!(%error, "could not ask a kept connection to close");
        }
    }
    for path in [socket, owner] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::debug!(%error, path = %path.display(), "could not remove a kept connection's file");
            }
        }
    }
}

/// Gives the request to close a connection a moment, and leaves it to a
/// thread after that: a background `ssh` whose connection went dead can
/// take seconds to answer, and nothing should wait for it. The socket is
/// removed either way, so nobody can use the connection again.
fn wait_briefly(mut child: std::process::Child) -> std::io::Result<()> {
    use std::time::{Duration, Instant};

    fn finished_by(child: &mut std::process::Child, deadline: Instant) -> std::io::Result<bool> {
        while child.try_wait()?.is_none() {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(true)
    }

    if finished_by(&mut child, Instant::now() + Duration::from_millis(250))? {
        return Ok(());
    }
    std::thread::Builder::new()
        .name("mm-ssh-close".to_owned())
        .spawn(move || {
            let closed = finished_by(&mut child, Instant::now() + Duration::from_secs(5)).and_then(
                |finished| {
                    if finished {
                        return Ok(());
                    }
                    child.kill()?;
                    child.wait().map(|_| ())
                },
            );
            if let Err(error) = closed {
                tracing::debug!(%error, "a request to close a kept connection did not end");
            }
        })
        .map(|_| ())
}

/// Creates the socket directory for the user alone, and refuses one that
/// is anything else: a link, another owner, or open to group or others.
#[cfg(unix)]
fn private_directory(directory: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

    if !directory.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)
            .map_err(|error| format!("creating {}: {error}", directory.display()))?;
    }
    let metadata = std::fs::symlink_metadata(directory)
        .map_err(|error| format!("inspecting {}: {error}", directory.display()))?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(format!(
            "{} is not a directory that only this user can enter",
            directory.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_directory(directory: &Path) -> Result<(), String> {
    Err(format!(
        "{} cannot hold a connection socket on this platform",
        directory.display()
    ))
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?
        .write_all(contents)
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

/// A hash of the public half of the key file at `path` when `mm` can use
/// the key with nobody present: an OpenSSH private key that is not a
/// security key, and that is not encrypted unless `mm` keeps its
/// passphrase (`managed`). `None` for anything else, also for a file mm
/// cannot tell.
fn unattended_key(path: &Path, managed: bool) -> Option<String> {
    let text = Zeroizing::new(std::fs::read_to_string(path).ok()?);
    let header = PrivateKeyHeader::parse(&text)?;
    if header.is_security_key() || (header.encrypted && !managed) {
        return None;
    }
    Some(blake3::hash(&header.public_blob).to_hex().to_string())
}
