//! Telling the service manager that `mmd` is ready, and that it still
//! answers.
//!
//! `mmd.service` is `Type=notify` with a watchdog. The keep-alive is sent
//! by a plain thread, and only after a canary got through: a connection to
//! the daemon's own socket, which the accept loop has to take and a task
//! of the request runtime has to answer. A daemon that has stopped serving
//! requests therefore stops telling systemd it is alive, whatever state
//! its process is in, and systemd replaces it. Other parts of the daemon
//! that must keep running are asked the same way (`watching`).
//!
//! The notification protocol is a datagram of `KEY=value` lines on the
//! Unix socket named by `NOTIFY_SOCKET`; no library is needed for it.

use std::io::Read;
use std::os::unix::net::{SocketAddr, UnixDatagram, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What the daemon answers its own canary with.
pub(super) const CANARY_ANSWER: &[u8] = b"mmd answers\n";
/// The canary runs this many times per watchdog interval, so one slow
/// answer is not a restart.
const CANARIES_PER_INTERVAL: u32 = 3;
/// How long one canary may take, at most.
const CANARY_PATIENCE: Duration = Duration::from_secs(5);

/// A question to another part of the daemon: does it still run? The
/// argument is how long it may take to answer.
type Probe = Box<dyn Fn(Duration) -> Result<(), String> + Send>;

/// What the keep-alive found when it last looked, for `mm doctor`: the
/// watchdog interval, and the parts of the daemon that did not answer.
static LAST_LOOK: std::sync::Mutex<Option<(Duration, Vec<String>)>> = std::sync::Mutex::new(None);

/// The watchdog interval when a service manager watches the daemon, and
/// what did not answer at the keep-alive's last look.
pub(super) fn last_look() -> (Option<Duration>, Vec<String>) {
    LAST_LOOK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .map_or((None, Vec::new()), |(watchdog, failing)| {
            (Some(watchdog), failing)
        })
}

fn note_look(watchdog: Duration, failing: Vec<String>) {
    *LAST_LOOK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((watchdog, failing));
}

/// Where the daemon reports that it is ready and alive.
pub struct Liveness {
    notify: Option<PathBuf>,
    watchdog: Option<Duration>,
    probes: Vec<(&'static str, Probe)>,
    /// Where the request socket is kept across a restart.
    sockets: crate::adapters::kept_sockets::KeptSockets,
}

impl Liveness {
    /// What the service manager asked for: `NOTIFY_SOCKET`, and
    /// `WATCHDOG_USEC` when the unit sets `WatchdogSec`. Outside a service
    /// manager nothing is reported.
    #[must_use]
    pub fn from_environment() -> Self {
        let notify = std::env::var_os("NOTIFY_SOCKET")
            .filter(|socket| !socket.is_empty())
            .map(PathBuf::from);
        let for_this_process =
            std::env::var("WATCHDOG_PID").map_or(true, |pid| pid == std::process::id().to_string());
        let watchdog = std::env::var("WATCHDOG_USEC")
            .ok()
            .and_then(|micros| micros.parse::<u64>().ok())
            .filter(|micros| *micros > 0 && for_this_process)
            .map(Duration::from_micros);
        Self::to(notify, watchdog)
    }

    /// Reports to the notification socket `notify`, with a keep-alive
    /// that has to arrive within every `watchdog`.
    #[must_use]
    pub fn to(notify: Option<PathBuf>, watchdog: Option<Duration>) -> Self {
        Self {
            notify,
            watchdog,
            probes: Vec::new(),
            sockets: crate::adapters::kept_sockets::KeptSockets::none(),
        }
    }

    /// Keeps the daemon's request socket in `sockets` across a restart,
    /// and takes the one the previous daemon left there.
    #[must_use]
    pub fn keeping(mut self, sockets: crate::adapters::kept_sockets::KeptSockets) -> Self {
        self.sockets = sockets;
        self
    }

    pub(super) fn sockets(&self) -> &crate::adapters::kept_sockets::KeptSockets {
        &self.sockets
    }

    /// Also requires `probe` to answer before a keep-alive is sent.
    #[must_use]
    pub fn watching(
        mut self,
        what: &'static str,
        probe: impl Fn(Duration) -> Result<(), String> + Send + 'static,
    ) -> Self {
        self.probes.push((what, Box::new(probe)));
        self
    }

    /// Says the daemon is ready and starts the keep-alive, when a service
    /// manager listens. `socket` is the daemon's own, already listening.
    pub(super) fn start(self, socket: PathBuf) {
        let Some(notify) = self.notify else {
            return;
        };
        send(&notify, "READY=1\nSTATUS=serving requests\n");
        let Some(watchdog) = self.watchdog else {
            return;
        };
        let probes = self.probes;
        let started = std::thread::Builder::new()
            .name("mm-watchdog".to_owned())
            .spawn(move || keep_alive(&notify, &socket, watchdog, &probes));
        if let Err(error) = started {
            // Without the thread systemd restarts the daemon when the
            // interval ends, which says the same.
            tracing::error!(%error, "could not start the watchdog thread");
        }
    }
}

fn keep_alive(notify: &Path, socket: &Path, watchdog: Duration, probes: &[(&'static str, Probe)]) {
    let pause = watchdog / CANARIES_PER_INTERVAL;
    let patience = CANARY_PATIENCE.min(pause);
    let mut healthy = true;
    loop {
        let began = Instant::now();
        let answered = canary(socket, patience).and_then(|()| {
            probes.iter().try_for_each(|(what, probe)| {
                probe(patience).map_err(|why| format!("{what}: {why}"))
            })
        });
        note_look(
            watchdog,
            answered.as_ref().err().cloned().into_iter().collect(),
        );
        match answered {
            Ok(()) => {
                if !healthy {
                    tracing::warn!("the daemon answers again");
                }
                healthy = true;
                send(notify, "WATCHDOG=1\n");
            }
            Err(reason) => {
                healthy = false;
                tracing::error!(
                    reason,
                    watchdog_seconds = watchdog.as_secs(),
                    "the daemon does not answer its own canary; not reporting it alive, so the service manager restarts it"
                );
                send(notify, &format!("STATUS=not answering: {reason}\n"));
            }
        }
        std::thread::sleep(pause.saturating_sub(began.elapsed()));
    }
}

/// One request to the daemon's own socket, as a plain blocking client.
fn canary(socket: &Path, patience: Duration) -> Result<(), String> {
    let failed =
        |what: &str, error: std::io::Error| format!("{what} {}: {error}", socket.display());
    let mut stream = UnixStream::connect(socket).map_err(|error| failed("connecting to", error))?;
    stream
        .set_read_timeout(Some(patience))
        .map_err(|error| failed("setting a deadline on", error))?;
    let mut answer = vec![0_u8; CANARY_ANSWER.len()];
    stream.read_exact(&mut answer).map_err(|error| {
        format!(
            "no answer on {} within {} ms ({error})",
            socket.display(),
            patience.as_millis()
        )
    })?;
    if answer == CANARY_ANSWER {
        Ok(())
    } else {
        Err(format!("an unexpected answer on {}", socket.display()))
    }
}

/// Sends one notification. A service manager that cannot be told is not a
/// reason to stop serving.
fn send(notify: &Path, state: &str) {
    let sent = notify_address(notify).and_then(|address| {
        let socket = UnixDatagram::unbound()?;
        socket.send_to_addr(state.as_bytes(), &address)
    });
    if let Err(error) = sent {
        tracing::warn!(%error, socket = %notify.display(), "could not notify the service manager");
    }
}

/// A notification socket is a path, or `@name` in the abstract namespace.
pub(crate) fn notify_address(notify: &Path) -> std::io::Result<SocketAddr> {
    #[cfg(target_os = "linux")]
    {
        use std::os::linux::net::SocketAddrExt;
        use std::os::unix::ffi::OsStrExt;

        if let Some(name) = notify.as_os_str().as_bytes().strip_prefix(b"@") {
            return SocketAddr::from_abstract_name(name);
        }
    }
    SocketAddr::from_pathname(notify)
}
