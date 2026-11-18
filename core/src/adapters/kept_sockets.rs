//! Listening sockets that outlive a restart of `mmd`.
//!
//! `mmd` answers the names of its node's containers, so the moments between
//! one `mmd` and the next (an upgrade, a crash, the watchdog) must not
//! refuse a lookup. Under systemd, each socket `mmd` listens on (lookups on
//! each cluster network's gateway, the exchange between nodes on the mesh
//! address, the request socket) is also handed to the service manager's
//! file descriptor store. When `mmd` stops for a restart, systemd keeps the
//! sockets open and hands them to the next `mmd`: a lookup or a request
//! that arrives in between waits in its socket and is answered, instead of
//! being refused. `systemctl stop mmd` closes them.
//!
//! The protocol is sd_notify(3)'s: a datagram `FDSTORE=1` with the
//! descriptor and `FDNAME=` naming it, `FDSTOREREMOVE=1` to drop one, and
//! `LISTEN_FDS` at the next start. Outside a service manager nothing is
//! kept and every socket is bound anew.

use std::collections::BTreeMap;
use std::io::IoSlice;
use std::mem::MaybeUninit;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::os::fd::AsFd;
use std::os::unix::net::{UnixDatagram, UnixListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

/// The sockets the previous `mmd` handed over, and where to hand the new
/// ones. Cloning shares them.
#[derive(Clone, Default)]
pub struct KeptSockets {
    store: Option<Arc<Store>>,
}

struct Store {
    notify: PathBuf,
    inherited: Mutex<Inherited>,
}

#[derive(Default)]
struct Inherited {
    datagrams: BTreeMap<SocketAddr, UdpSocket>,
    streams: BTreeMap<SocketAddr, TcpListener>,
    unix: Vec<UnixListener>,
}

impl Inherited {
    /// Takes the descriptor at `index` as the kind of socket it is. One
    /// that is none of these stays open and unused.
    fn adopt(&mut self, handed: &mut listenfd::ListenFd, index: usize) {
        if let Ok(Some(socket)) = handed.take_udp_socket(index) {
            match socket.local_addr() {
                Ok(address) => {
                    self.datagrams.insert(address, socket);
                }
                Err(error) => tracing::debug!(%error, "a socket handed over has no address"),
            }
            return;
        }
        if let Ok(Some(listener)) = handed.take_tcp_listener(index) {
            match listener.local_addr() {
                Ok(address) => {
                    self.streams.insert(address, listener);
                }
                Err(error) => tracing::debug!(%error, "a socket handed over has no address"),
            }
            return;
        }
        if let Ok(Some(listener)) = handed.take_unix_listener(index) {
            self.unix.push(listener);
        }
    }
}

impl std::fmt::Debug for KeptSockets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeptSockets")
            .field("store", &self.store.as_ref().map(|store| &store.notify))
            .finish()
    }
}

impl KeptSockets {
    /// Keeps nothing: every socket is bound anew and closes with the
    /// process.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// What the service manager handed over and where it keeps sockets:
    /// `LISTEN_FDS` and `NOTIFY_SOCKET`. Call it once, before any thread
    /// reads the environment: it takes `LISTEN_FDS` out of it. Outside a
    /// service manager it keeps nothing.
    #[must_use]
    pub fn from_environment() -> Self {
        let Some(notify) = std::env::var_os("NOTIFY_SOCKET")
            .filter(|socket| !socket.is_empty())
            .map(PathBuf::from)
        else {
            return Self::none();
        };
        let mut handed = listenfd::ListenFd::from_env();
        let mut inherited = Inherited::default();
        for index in 0..handed.len() {
            inherited.adopt(&mut handed, index);
        }
        Self::keeping(notify, inherited)
    }

    /// Hands every socket to the store at `notify`, with nothing handed
    /// over: what a first start under a service manager does.
    #[must_use]
    pub fn to(notify: PathBuf) -> Self {
        Self::keeping(notify, Inherited::default())
    }

    fn keeping(notify: PathBuf, inherited: Inherited) -> Self {
        Self {
            store: Some(Arc::new(Store {
                notify,
                inherited: Mutex::new(inherited),
            })),
        }
    }

    fn inherited(&self) -> Option<std::sync::MutexGuard<'_, Inherited>> {
        self.store.as_ref().map(|store| {
            store
                .inherited
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
        })
    }

    /// A datagram socket on `address`: the one handed over, or a new one,
    /// which the store keeps from now on. Blocking; callers set it
    /// otherwise.
    pub fn udp(&self, address: SocketAddr) -> std::io::Result<UdpSocket> {
        self.udp_bound_by(address, UdpSocket::bind)
    }

    /// [`KeptSockets::udp`], with a new socket made by `bind`.
    pub fn udp_bound_by(
        &self,
        address: SocketAddr,
        bind: impl FnOnce(SocketAddr) -> std::io::Result<UdpSocket>,
    ) -> std::io::Result<UdpSocket> {
        let taken = self
            .inherited()
            .and_then(|mut inherited| inherited.datagrams.remove(&address));
        let socket = match taken {
            Some(socket) => socket,
            None => bind(address)?,
        };
        self.keep(&datagram_name(socket.local_addr()?), &socket);
        Ok(socket)
    }

    /// A listening stream socket on `address`, handed over or new, as
    /// [`KeptSockets::udp`].
    pub fn tcp(&self, address: SocketAddr) -> std::io::Result<TcpListener> {
        self.tcp_bound_by(address, TcpListener::bind)
    }

    /// [`KeptSockets::tcp`], with a new listener made by `bind`.
    pub fn tcp_bound_by(
        &self,
        address: SocketAddr,
        bind: impl FnOnce(SocketAddr) -> std::io::Result<TcpListener>,
    ) -> std::io::Result<TcpListener> {
        let taken = self
            .inherited()
            .and_then(|mut inherited| inherited.streams.remove(&address));
        let listener = match taken {
            Some(listener) => listener,
            None => bind(address)?,
        };
        self.keep(&stream_name(listener.local_addr()?), &listener);
        Ok(listener)
    }

    /// The listening Unix socket handed over for `path`, if there is one.
    /// A new one is handed to the store with [`KeptSockets::keep_unix`].
    #[must_use]
    pub fn take_unix(&self, path: &Path) -> Option<UnixListener> {
        let mut inherited = self.inherited()?;
        let position = inherited.unix.iter().position(|listener| {
            listener
                .local_addr()
                .is_ok_and(|address| address.as_pathname() == Some(path))
        })?;
        Some(inherited.unix.swap_remove(position))
    }

    /// Hands the listening Unix socket on `path` to the store.
    pub fn keep_unix(&self, path: &Path, listener: &impl AsFd) {
        self.keep(&unix_name(path), listener);
    }

    /// Stops keeping the sockets on `address`: lookups there end with
    /// this process.
    pub fn forget(&self, address: SocketAddr) {
        self.remove(&datagram_name(address));
        self.remove(&stream_name(address));
    }

    /// Closes the network sockets handed over that nobody took, and stops
    /// keeping them: their addresses are no longer listened on. Unix
    /// sockets are the daemon's, which takes or drops its own at start.
    pub fn release_unclaimed(&self) {
        let (datagrams, streams) = match self.inherited() {
            Some(mut inherited) => (
                std::mem::take(&mut inherited.datagrams),
                std::mem::take(&mut inherited.streams),
            ),
            None => return,
        };
        for address in datagrams.into_keys().chain(streams.into_keys()) {
            self.forget(address);
        }
    }

    fn keep(&self, name: &str, socket: &impl AsFd) {
        let Some(store) = &self.store else {
            return;
        };
        let message = format!("FDSTORE=1\nFDNAME={name}\n");
        if let Err(error) = send_with(&store.notify, &message, Some(socket)) {
            tracing::warn!(%error, name, "the service manager did not take a socket; a restart closes it");
        }
    }

    fn remove(&self, name: &str) {
        let Some(store) = &self.store else {
            return;
        };
        let message = format!("FDSTOREREMOVE=1\nFDNAME={name}\n");
        if let Err(error) = send_with(&store.notify, &message, None::<&UdpSocket>) {
            tracing::debug!(%error, name, "could not tell the service manager to drop a socket");
        }
    }
}

/// The names in the store. Every character is one systemd accepts in a
/// descriptor's name: no `:`, nothing unprintable.
fn datagram_name(address: SocketAddr) -> String {
    format!("udp-{}-{}", address.ip(), address.port())
}

fn stream_name(address: SocketAddr) -> String {
    format!("tcp-{}-{}", address.ip(), address.port())
}

fn unix_name(path: &Path) -> String {
    let file = path
        .file_name()
        .map(|name| name.to_string_lossy().replace(':', "_"))
        .unwrap_or_default();
    format!("unix-{file}")
}

/// Sends `message` to the notification socket, with `socket`'s descriptor
/// attached when there is one.
fn send_with(notify: &Path, message: &str, socket: Option<&impl AsFd>) -> std::io::Result<()> {
    let datagram = UnixDatagram::unbound()?;
    datagram.connect_addr(&crate::adapters::remote::notify_address(notify)?)?;
    let descriptors = socket.map(AsFd::as_fd).into_iter().collect::<Vec<_>>();
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = rustix::net::SendAncillaryBuffer::new(&mut space);
    if !descriptors.is_empty()
        && !control.push(rustix::net::SendAncillaryMessage::ScmRights(&descriptors))
    {
        return Err(std::io::Error::other("no room for the descriptor"));
    }
    rustix::net::sendmsg(
        &datagram,
        &[IoSlice::new(message.as_bytes())],
        &mut control,
        rustix::net::SendFlags::empty(),
    )?;
    Ok(())
}
