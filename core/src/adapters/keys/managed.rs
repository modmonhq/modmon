//! Authenticating with a key mm manages: the passphrase comes out of the
//! vault here, and goes to the one OpenSSH process that needs it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use super::askpass::Askpass;
use crate::app::ports::KeyUnlock;
use crate::domain::error::MmError;
use crate::domain::key::{KeyPassphrase, PrivateKeyHeader};

/// The keys this process may unlock, if any. Cheap to clone; the clones
/// share one passphrase, read from the vault the first time a connection
/// has to authenticate and kept for as long as they live.
#[derive(Clone, Default)]
pub struct ManagedKeys {
    inner: Option<Arc<Inner>>,
}

struct Inner {
    unlock: Arc<dyn KeyUnlock>,
    /// `MM_HOME/run`, where the passphrase helper keeps its pipes.
    run_directory: PathBuf,
    passphrase: Mutex<Option<(PathBuf, KeyPassphrase)>>,
}

impl std::fmt::Debug for ManagedKeys {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedKeys")
            .field("unlocks", &self.inner.is_some())
            .finish()
    }
}

impl ManagedKeys {
    /// Unlocks nothing: every key is used as the file it is.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn new(unlock: Arc<dyn KeyUnlock>, run_directory: PathBuf) -> Self {
        Self {
            inner: Some(Arc::new(Inner {
                unlock,
                run_directory,
                passphrase: Mutex::new(None),
            })),
        }
    }

    /// Whether `path` is a key whose passphrase mm keeps.
    #[must_use]
    pub fn manages(&self, path: &Path) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.unlock.manages(path))
    }

    /// The passphrase of the key at `path` when it is one mm manages and
    /// it is encrypted; `None` for a key that needs none from mm.
    fn passphrase(&self, path: &Path) -> Result<Option<(KeyPassphrase, &Inner)>, MmError> {
        let Some(inner) = self.inner.as_deref() else {
            return Ok(None);
        };
        if !inner.unlock.manages(path) {
            return Ok(None);
        }
        let encrypted = std::fs::read_to_string(path)
            .map(Zeroizing::new)
            .ok()
            .and_then(|text| PrivateKeyHeader::parse(&text))
            .is_some_and(|header| header.encrypted && !header.is_security_key());
        if !encrypted {
            return Ok(None);
        }
        let mut cached = inner
            .passphrase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((key, passphrase)) = cached.as_ref()
            && key == path
        {
            return Ok(Some((passphrase.clone(), inner)));
        }
        let passphrase = inner.unlock.passphrase(path)?;
        *cached = Some((path.to_path_buf(), passphrase.clone()));
        Ok(Some((passphrase, inner)))
    }

    /// What one OpenSSH process needs to authenticate with the key at
    /// `path`: a helper that answers its passphrase prompt once. `None`
    /// for a key that needs no passphrase from mm.
    pub(crate) fn askpass(&self, path: &Path) -> Result<Option<Askpass>, MmError> {
        // A rotation in progress: the connection offers the new key too,
        // and works with it alone when the current key cannot be opened.
        let next = self.next_passphrase(path)?;
        let current = match self.passphrase(path) {
            Ok(current) => current,
            Err(error) if next.is_some() => {
                tracing::warn!(%error, "the current key cannot be opened; offering the new key of the rotation alone");
                None
            }
            Err(error) => return Err(error),
        };
        let Some(inner) = current
            .as_ref()
            .map(|(_, inner)| *inner)
            .or(next.as_ref().map(|(_, inner)| *inner))
        else {
            return Ok(None);
        };
        Askpass::serve_keys(
            &inner.run_directory,
            (
                current.map(|(passphrase, _)| passphrase),
                next.map(|(passphrase, _)| passphrase),
            ),
            None,
        )
        .map(Some)
    }

    /// The new key a rotation made beside the key at `path`, when there is
    /// one mm manages: connections offer it first and the current key
    /// after it, so hosts that already list it use it and the others still
    /// accept the device.
    #[must_use]
    pub fn next_key(&self, path: &Path) -> Option<PathBuf> {
        let next = crate::domain::key::next_key_path(path);
        (next.is_file() && self.manages(&next)).then_some(next)
    }

    fn next_passphrase(&self, path: &Path) -> Result<Option<(KeyPassphrase, &Inner)>, MmError> {
        let Some(next) = self.next_key(path) else {
            return Ok(None);
        };
        let Some(inner) = self.inner.as_deref() else {
            return Ok(None);
        };
        let encrypted = std::fs::read_to_string(&next)
            .map(Zeroizing::new)
            .ok()
            .and_then(|text| PrivateKeyHeader::parse(&text))
            .is_some_and(|header| header.encrypted && !header.is_security_key());
        if !encrypted {
            return Ok(None);
        }
        Ok(Some((inner.unlock.passphrase(&next)?, inner)))
    }

    /// As `askpass`, off the async runtime's threads: asking a keychain
    /// starts a process and waits for it.
    pub async fn askpass_for(&self, path: &Path) -> Result<Option<Askpass>, MmError> {
        if !self.manages(path) {
            return Ok(None);
        }
        let keys = self.clone();
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || keys.askpass(&path))
            .await
            .map_err(|error| MmError::State {
                message: format!("unlocking a key did not finish: {error}"),
            })?
    }

    /// A private `ssh-agent` that holds the key at `path` until it is
    /// dropped, for a tool that starts many OpenSSH processes itself.
    /// `None` for a key that needs no passphrase from mm.
    pub async fn agent_for(&self, path: &Path) -> Result<Option<PrivateAgent>, MmError> {
        if !self.manages(path) {
            return Ok(None);
        }
        let keys = self.clone();
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            keys.passphrase(&path)?
                .map(|(passphrase, inner)| {
                    PrivateAgent::start(&inner.run_directory, &path, passphrase)
                })
                .transpose()
        })
        .await
        .map_err(|error| MmError::State {
            message: format!("unlocking a key did not finish: {error}"),
        })?
    }
}

/// How long the private agent keeps the key when nothing ends it first.
const AGENT_LIFETIME_SECONDS: &str = "3600";
/// The longest socket path every platform takes (104 bytes on macOS).
const SOCKET_PATH_LIMIT: usize = 100;

/// An `ssh-agent` of mm's own, holding one key, reachable through a socket
/// in a directory only the user can enter. It ends when this is dropped.
///
/// While it runs, a program running as the user that finds the socket can
/// have the key sign. mm uses it only for `mm target setup`, where Ansible
/// opens the connections, and only for as long as that runs.
pub struct PrivateAgent {
    child: std::process::Child,
    socket: PathBuf,
    _directory: tempfile::TempDir,
}

impl PrivateAgent {
    fn start(run_directory: &Path, key: &Path, passphrase: KeyPassphrase) -> Result<Self, MmError> {
        let directory = agent_directory(run_directory)?;
        let socket = directory.path().join("agent");
        let child = Command::new("ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| MmError::ProcessSpawn {
                program: "ssh-agent".to_owned(),
                source,
            })?;
        let mut agent = Self {
            child,
            socket,
            _directory: directory,
        };
        agent.wait_for_socket()?;
        let askpass = Askpass::serve(run_directory, Some(passphrase), None)?;
        let output = Command::new("ssh-add")
            .args(["-q", "-t", AGENT_LIFETIME_SECONDS])
            .arg(key)
            .env("LC_ALL", "C")
            .env("SSH_AUTH_SOCK", &agent.socket)
            .envs(askpass.environment())
            .stdin(Stdio::null())
            .output()
            .map_err(|source| MmError::ProcessSpawn {
                program: "ssh-add".to_owned(),
                source,
            })?;
        if !output.status.success() {
            return Err(MmError::Secret {
                message: format!(
                    "the passphrase mm keeps for {} did not open it: {}",
                    key.display(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        Ok(agent)
    }

    fn wait_for_socket(&mut self) -> Result<(), MmError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.socket.exists() {
            let ended = self.child.try_wait().map_err(|source| MmError::Io {
                context: "waiting for ssh-agent".to_owned(),
                source,
            })?;
            if ended.is_some() || Instant::now() >= deadline {
                return Err(MmError::Environment {
                    message: "ssh-agent did not start".to_owned(),
                });
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    /// The value of `SSH_AUTH_SOCK` for the tool that uses the key.
    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for PrivateAgent {
    fn drop(&mut self) {
        if let Err(error) = self
            .child
            .kill()
            .and_then(|()| self.child.wait().map(|_| ()))
        {
            tracing::warn!(%error, "the private ssh-agent could not be ended");
        }
    }
}

/// A private directory for the agent's socket: under `run_directory`, or
/// under the temporary directory when that path would be too long for a
/// socket.
fn agent_directory(run_directory: &Path) -> Result<tempfile::TempDir, MmError> {
    let under = |parent: &Path| super::private_directory_in(parent, "agent-");
    let fits = |directory: &tempfile::TempDir| {
        directory.path().join("agent").as_os_str().len() <= SOCKET_PATH_LIMIT
    };
    match under(run_directory) {
        Ok(directory) if fits(&directory) => Ok(directory),
        _ => under(&std::env::temp_dir()).map_err(|source| MmError::Io {
            context: "creating a private directory for ssh-agent".to_owned(),
            source,
        }),
    }
}
