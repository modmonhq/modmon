// Connecting to one host's relay, and saying why when that fails. An
// unreachable host is the failure people meet most, so the message names the
// host, what happened, and what to run next.

/// A daemon's answer that ends the handshake.
enum HandshakeRefusal {
    /// The daemon answered with an error frame.
    Daemon(crate::domain::remote::RemoteError),
    /// The hello did not carry the expected nonce, identity, or version.
    Mismatch,
}

/// How long the daemon may take to answer the hello on a new connection.
const HELLO_PATIENCE: Duration = Duration::from_secs(15);
/// The same on a kept connection, which has no key exchange to wait for: a
/// kept connection that says nothing for this long is taken for dead.
const KEPT_HELLO_PATIENCE: Duration = Duration::from_secs(5);

/// Why one attempt to connect produced no session.
enum ConnectFailure {
    /// The daemon answered, and refused.
    Refused(MmError),
    /// The connection ended or stalled before the daemon answered; what
    /// `ssh` and the relay wrote says why.
    Ended(String),
    Local(MmError),
}

/// How one attempt uses the connection `mm` keeps to the host.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sharing {
    /// A kept connection is there: open a session on it.
    Kept,
    /// None is there: connect, and keep the connection if it may be kept.
    New,
    /// Connect the way `mm` did before it kept connections.
    Never,
}

impl SshEndpoint {
    /// A session with the host's daemon, hellos exchanged. With a kept
    /// connection this is a new session on it; one that turns out dead
    /// (the machine slept, the network changed) is closed and replaced by a
    /// new connection. And when `ssh` cannot share a connection here at all
    /// (a file system without sockets, say), `mm` connects without sharing:
    /// keeping connections must never make a host unreachable. Nothing was
    /// asked of the host at any of those points, so trying again cannot
    /// repeat a request.
    async fn connect(&self, read: Option<&ReadRequest>) -> Result<Connection, MmError> {
        let kept = self.kept.as_ref().filter(|kept| kept.is_open());
        let _phase = crate::app::timing::phase_of(
            "ssh.connect",
            &format_args!(
                "{}{}",
                self.subject,
                if kept.is_some() { ", kept" } else { "" }
            ),
        );
        let first = if kept.is_some() {
            Sharing::Kept
        } else {
            Sharing::New
        };
        let mut failure = match self.attempt(first, read).await {
            Ok(connection) => return Ok(connection),
            Err(failure) => failure,
        };
        if let (ConnectFailure::Ended(stderr), Some(kept)) = (&failure, kept) {
            tracing::debug!(subject = %self.subject, %stderr, "the kept connection is dead; connecting again");
            kept.close();
            failure = match self.attempt(Sharing::New, read).await {
                Ok(connection) => return Ok(connection),
                Err(failure) => failure,
            };
        }
        if let ConnectFailure::Ended(stderr) = &failure
            && self.kept.is_some()
            && blames_sharing(stderr)
        {
            tracing::debug!(subject = %self.subject, %stderr, "ssh cannot share a connection here; connecting without");
            failure = match self.attempt(Sharing::Never, read).await {
                Ok(connection) => return Ok(connection),
                Err(failure) => failure,
            };
        }
        Err(match failure {
            ConnectFailure::Refused(error) | ConnectFailure::Local(error) => error,
            ConnectFailure::Ended(stderr) => {
                let (problem, help) = connect_failure(&stderr);
                let (problem, help) = if self.agent_key && problem == REFUSED_KEY {
                    (REFUSED_AGENT_KEY.to_owned(), REFUSED_AGENT_KEY_HELP.to_owned())
                } else {
                    (problem, help)
                };
                MmError::Unreachable {
                    subject: self.subject.clone(),
                    problem,
                    help,
                }
            }
        })
    }

    /// Starts `ssh` and exchanges hellos once. `read` goes out with the
    /// hello, under an id of its own for this attempt.
    async fn attempt(
        &self,
        sharing: Sharing,
        read: Option<&ReadRequest>,
    ) -> Result<Connection, ConnectFailure> {
        let local = ConnectFailure::Local;
        let read = read.map(ReadRequest::frame).transpose().map_err(local)?;
        if let Some(connection) = self.kept.as_ref().filter(|_| sharing != Sharing::Never) {
            connection.record_owner();
        }
        let known_hosts = self.pinned_known_hosts().map_err(local)?;
        // A session on a kept connection authenticates nothing. Anything
        // else with a key mm keeps the passphrase of gets a helper that
        // answers `ssh` once; the passphrase leaves the vault here, and
        // only when a connection is really made.
        let askpass = match (&self.identity, sharing) {
            (Some(IdentitySource::File(path)), Sharing::New | Sharing::Never) => {
                self.keys.askpass_for(path).await.map_err(local)?
            }
            _ => None,
        };
        let shared = sharing != Sharing::Never;
        let (mut command, inline_key) =
            (self.ssh_command(known_hosts.path(), shared, askpass.as_ref())).map_err(local)?;
        let mut child = command
            .spawn()
            .map_err(|source| MmError::ProcessSpawn {
                program: "ssh".to_owned(),
                source,
            })
            .map_err(local)?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(local(protocol("SSH standard streams were not captured")));
        };
        // What `ssh` and the relay say when a connection fails. It is read
        // to the end, and used only to explain a failed handshake.
        let diagnostics = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut stderr = stderr.take(64 * 1024);
            let mut bytes = Vec::new();
            if let Err(error) = stderr.read_to_end(&mut bytes).await {
                tracing::debug!(%error, "restricted SSH relay diagnostics could not be read");
            }
            String::from_utf8_lossy(&bytes).into_owned()
        });
        let mut connection = Connection {
            subject: self.subject.clone(),
            child,
            stdin,
            stdout: FrameReader::new(stdout),
            sent: None,
            _known_hosts: known_hosts,
            _inline_key: inline_key,
            askpass,
        };
        let patience = if sharing == Sharing::Kept {
            KEPT_HELLO_PATIENCE
        } else {
            HELLO_PATIENCE
        };
        match connection
            .handshake(
                &self.daemon_fingerprint,
                patience,
                read.as_ref().map(|(_, frame)| frame),
            )
            .await
        {
            Ok(()) => {
                connection.sent = read.map(|(request_id, _)| request_id);
                connection.askpass = None;
                Ok(connection)
            }
            Err(Some(refusal)) => Err(ConnectFailure::Refused(self.refused(refusal))),
            Err(None) => {
                drop(connection);
                // Ending `ssh` closes its stderr, so the wait is short. Not
                // on a kept connection: the background `ssh` holds the
                // session's streams too, and one whose connection went dead
                // lets go of them late. What it would say is not needed;
                // the caller connects again.
                let waited = if sharing == Sharing::Kept {
                    Duration::ZERO
                } else {
                    Duration::from_secs(3)
                };
                Err(ConnectFailure::Ended(
                    match tokio::time::timeout(waited, diagnostics).await {
                        Ok(Ok(stderr)) => stderr,
                        _ => String::new(),
                    },
                ))
            }
        }
    }

    /// A known-hosts file that names this host's recorded key and nothing
    /// else, for the one connection.
    fn pinned_known_hosts(&self) -> Result<tempfile::NamedTempFile, MmError> {
        let known_hosts = tempfile::NamedTempFile::new().map_err(|source| MmError::Io {
            context: "creating an ephemeral strict known-hosts file".to_owned(),
            source,
        })?;
        let known_name = if self.port == 22 {
            self.host.clone()
        } else {
            format!("[{}]:{}", self.host, self.port)
        };
        std::fs::write(
            known_hosts.path(),
            format!("{known_name} {}\n", self.host_key),
        )
        .map_err(|source| MmError::Io {
            context: "writing an ephemeral strict known-hosts file".to_owned(),
            source,
        })?;
        Ok(known_hosts)
    }

    fn refused(&self, refusal: HandshakeRefusal) -> MmError {
        let (problem, help) = match refusal {
            HandshakeRefusal::Daemon(error) if error.code == "MM_REMOTE_UPGRADE_REQUIRED" => (
                crate::domain::remote::PROTOCOL_REFUSED.to_owned(),
                error.message,
            ),
            HandshakeRefusal::Daemon(error) if error.code == "MM_REMOTE_IDENTITY" => (
                crate::domain::remote::IDENTITY_REFUSED.to_owned(),
                REPLACED_HOST_HELP.to_owned(),
            ),
            HandshakeRefusal::Daemon(error) => return server_error(error),
            HandshakeRefusal::Mismatch => (
                crate::domain::remote::HANDSHAKE_MISMATCH.to_owned(),
                REPLACED_HOST_HELP.to_owned(),
            ),
        };
        MmError::Unreachable {
            subject: self.subject.clone(),
            problem,
            help,
        }
    }
}

/// Whether `ssh` says the connection failed over sharing rather than over
/// the host: it could not create or use the control socket, or could not
/// put the shared connection in the background (`control_persist_detach:
/// fork: …` where no more processes may be started).
fn blames_sharing(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    ["control", "unix_listener", "mux"]
        .iter()
        .any(|needle| lower.contains(needle))
}

const REPLACED_HOST_HELP: &str = "If the host was rebuilt, run `mm target setup plan --trust-new-host-keys`, apply it, then `mm target setup record`. If it was not, something else answers on that address: stop until you know why";

const REFUSED_KEY: &str = "refused this device's key";
/// The same for the key the MCP server connects with.
const REFUSED_AGENT_KEY: &str = "refused this device's agent key";
const REFUSED_AGENT_KEY_HELP: &str = "The users file the host last synced does not list the key `mm key show --agent` prints; a rotated key is a new key. Add it to your entry with `mm team join --agent`, commit the change, and have an admin run `mm team sync`";

/// Why a connection ended before the daemon answered, and what to do, from
/// what `ssh` and the relay wrote to standard error.
fn connect_failure(stderr: &str) -> (String, String) {
    let lower = stderr.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);
    // The last line that is not a banner or a warning says the most. sshd's
    // remark that the relay account has no home directory is in every
    // session and never the reason.
    let last = stderr
        .lines()
        .map(str::trim)
        .rfind(|line| {
            let line = line.to_ascii_lowercase();
            !line.is_empty()
                && !line.starts_with("warning:")
                && !line.starts_with("could not chdir to home directory")
        })
        .map(|line| {
            crate::domain::error::printed(
                &line
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(300)
                    .collect::<String>(),
            )
        })
        .unwrap_or_default();
    if has("host key verification failed") || has("remote host identification has changed") {
        return (
            "presented an SSH host key that differs from the recorded one".to_owned(),
            REPLACED_HOST_HELP.to_owned(),
        );
    }
    if has("rejected a client protocol version") {
        return (
            format!(
                "runs an mmd that speaks an older remote protocol than this mm ({PROTOCOL_VERSION})"
            ),
            "Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade".to_owned(),
        );
    }
    if has("relay daemon socket") || has("connecting restricted relay") {
        return (
            "accepts SSH, but mmd is not running there (the relay found no daemon socket)"
                .to_owned(),
            "Start it on the host with `systemctl start mmd`; `journalctl -u mmd` shows why it stopped"
                .to_owned(),
        );
    }
    if has("incorrect passphrase") || has("bad passphrase") {
        return (
            "was not asked: the passphrase mm keeps for this device's key does not open it"
                .to_owned(),
            "The key cannot be recovered. Run `mm key rotate`, then `mm key show` and have an admin register the new public key"
                .to_owned(),
        );
    }
    if has("ssh_askpass") {
        return (
            format!("was not asked: ssh could not run mm's passphrase helper ({last})"),
            "Check that programs may run from the mm home directory (it must not be mounted noexec)"
                .to_owned(),
        );
    }
    if has("permission denied") {
        return (
            REFUSED_KEY.to_owned(),
            "Ask an admin to add the key `mm key show` prints to the users file and run `mm team sync`. In CI, set MM_SSH_KEY to a key the users file lists"
                .to_owned(),
        );
    }
    let network = [
        "connection timed out",
        "operation timed out",
        "connection refused",
        "no route to host",
        "network is unreachable",
        "could not resolve hostname",
        "name or service not known",
        "connection reset",
        "connection closed by",
    ];
    if network.iter().any(|needle| has(needle)) {
        return (
            format!("did not answer ({last})"),
            "Check that the host is up and that this machine can reach its SSH port".to_owned(),
        );
    }
    let problem = if last.is_empty() {
        "closed the connection before mmd answered".to_owned()
    } else {
        format!("closed the connection before mmd answered ({last})")
    };
    (
        problem,
        "Check `systemctl status mmd` and `journalctl -u mmd` on the host".to_owned(),
    )
}
