//! The name resolver `mmd` runs on a cluster node.
//!
//! It answers the containers of its own node on the gateway address of
//! each cluster network the node holds (UDP and TCP port 53), from what the
//! node's engine runs and what the other nodes say they run. The nodes ask
//! each other over the mesh, host to host, on the node's mesh address; that
//! port answers the other nodes of the mesh and nobody else. Names that are
//! no container's go to the resolvers in the host's `/etc/resolv.conf`.
//!
//! Nothing here is configured. The addresses to listen on come from the
//! network state the last deployment stored, the peers from the mesh state,
//! and the answers from looking at the engine.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use tokio::sync::{Notify, Semaphore};

use crate::app::ports::{ClusterNetworkHost, MeshHost, RuntimeDriver, ServiceNames};
use crate::domain::cluster_network::NodeNetworks;
use crate::domain::discovery::wire::{self, Failure, Query};
use crate::domain::discovery::{
    EXCHANGE_INTERVAL_MS, NAME_TIME_TO_LIVE, NameProblem, NameReport, NodeRecords, PEER_EXPIRY_MS,
    Resolution, ServiceMap,
};
use crate::domain::mesh::MeshState;

mod exchange;
mod held;
mod leaving;
mod listeners;
mod metered;
mod scaling;
mod upstream;

use listeners::Listeners;
pub use listeners::{free_tcp, free_udp};
use upstream::Forwarded;

/// How long the node goes without looking at its engine when the engine
/// reports no change.
const LOOK_AGAIN: Duration = Duration::from_secs(10);
/// How soon the node tries again to listen on an address it does not have
/// yet: its mesh address (a gateway is listened on before it exists).
const ADDRESS_AGAIN: Duration = Duration::from_secs(1);
/// How long the node keeps trying that often. At boot the mesh comes up
/// within seconds; an address that is still missing after this is tried
/// again with every other look.
const ADDRESS_PATIENCE: Duration = Duration::from_secs(30);
/// Changes come in bursts (a deployment starts several containers): wait
/// this long after the first one before looking.
const SETTLE: Duration = Duration::from_millis(150);
/// After this many looks in a row that could not list the engine's
/// containers, the node stops answering with the ones it saw last.
const UNSEEN_LIMIT: u8 = 3;
/// How long to wait before starting the engine's event stream again.
const WATCH_AGAIN: Duration = Duration::from_secs(5);
/// Lookups served at once; more than this are dropped, and the clients ask
/// again.
const IN_FLIGHT: usize = 512;
/// How long a lookup waits, right after `mmd` started, for the other nodes
/// to say what they run. Answering before that would call a service that
/// runs on another node unknown.
const FIRST_ANSWERS: Duration = Duration::from_secs(3);

/// Where the resolver listens and whom it asks. The defaults are what a
/// node uses; tests choose ports an ordinary user may bind.
#[derive(Debug, Clone)]
pub struct ResolverOptions {
    /// The port lookups are answered on, on each network's gateway address.
    pub lookup_port: u16,
    /// The port the nodes tell each other what they run on.
    pub exchange_port: u16,
    /// The port of the host's own resolvers.
    pub upstream_port: u16,
    /// The host's resolver configuration.
    pub resolv_conf: PathBuf,
    /// How long an address the host does not have yet is tried again every
    /// second before it is left to the ordinary looks.
    pub address_patience: Duration,
}

impl Default for ResolverOptions {
    fn default() -> Self {
        Self {
            lookup_port: crate::domain::discovery::RESOLVER_PORT,
            exchange_port: crate::domain::discovery::EXCHANGE_PORT,
            upstream_port: 53,
            resolv_conf: PathBuf::from("/etc/resolv.conf"),
            address_patience: ADDRESS_PATIENCE,
        }
    }
}

struct Peer {
    address: Ipv4Addr,
    records: Option<NodeRecords>,
    /// When the peer last sent its records, or when this node first asked.
    heard: Instant,
    answered: bool,
    problem: Option<String>,
}

#[derive(Default)]
struct State {
    networks: Option<NodeNetworks>,
    /// Whether the stored state and the engine were read at least once.
    looked: bool,
    mesh_address: Option<Ipv4Addr>,
    local: NodeRecords,
    /// How many looks in a row could not list the engine's containers.
    unseen: u8,
    peers: BTreeMap<String, Peer>,
    /// The records of the peers heard from recently: what lookups use.
    fresh: Vec<NodeRecords>,
    /// What keeps this node from answering, by what it is about.
    problems: BTreeMap<String, String>,
    /// The containers leaving this node and the other nodes.
    leaves: leaving::Leaves,
    /// What this node tells the others about its scaled services.
    scaling: Vec<crate::domain::autoscale::ScaleReport>,
    /// The engine ids of this node's containers past their restart limit.
    held: std::collections::BTreeSet<String>,
}

impl State {
    fn refresh_fresh(&mut self) {
        let expiry = Duration::from_millis(PEER_EXPIRY_MS);
        self.fresh = self
            .peers
            .values()
            .filter(|peer| peer.heard.elapsed() < expiry)
            .filter_map(|peer| peer.records.clone())
            .collect();
    }

    fn note(&mut self, about: &str, problem: Option<String>) {
        match problem {
            Some(problem) => self.problems.insert(about.to_owned(), problem),
            None => self.problems.remove(about),
        };
    }
}

pub struct NameResolver {
    networks: Arc<dyn ClusterNetworkHost>,
    mesh: Arc<dyn MeshHost>,
    runtime: Arc<dyn RuntimeDriver>,
    options: ResolverOptions,
    upstream: upstream::Upstream,
    state: RwLock<State>,
    changed: Notify,
    /// Signalled each time the stored state and the engine were read.
    looked: Notify,
    /// True once every other node was asked a first time.
    asked: tokio::sync::watch::Sender<bool>,
    /// Counts every change of what this node answers (`leaving.rs`).
    generation: tokio::sync::watch::Sender<u64>,
    in_flight: Arc<Semaphore>,
    rotation: AtomicUsize,
    /// Where the listening sockets are kept across a restart of `mmd`.
    sockets: crate::adapters::kept_sockets::KeptSockets,
    /// One settling of the bridges at a time (a look, or a new container's
    /// first lookup).
    settling: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for NameResolver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NameResolver")
            .finish_non_exhaustive()
    }
}

impl NameResolver {
    /// A resolver for the host whose cluster networks, mesh, and engine
    /// these are. It does nothing until [`NameResolver::run`] is spawned.
    #[must_use]
    pub fn new(
        networks: Arc<dyn ClusterNetworkHost>,
        mesh: Arc<dyn MeshHost>,
        runtime: Arc<dyn RuntimeDriver>,
    ) -> Arc<Self> {
        Self::with_options(networks, mesh, runtime, ResolverOptions::default())
    }

    #[must_use]
    pub fn with_options(
        networks: Arc<dyn ClusterNetworkHost>,
        mesh: Arc<dyn MeshHost>,
        runtime: Arc<dyn RuntimeDriver>,
        options: ResolverOptions,
    ) -> Arc<Self> {
        Self::keeping(
            (networks, mesh, runtime),
            options,
            crate::adapters::kept_sockets::KeptSockets::none(),
        )
    }

    /// A resolver whose sockets `sockets` keeps across a restart of
    /// `mmd`, and that takes the ones the previous `mmd` left there.
    #[must_use]
    pub fn keeping(
        (networks, mesh, runtime): (
            Arc<dyn ClusterNetworkHost>,
            Arc<dyn MeshHost>,
            Arc<dyn RuntimeDriver>,
        ),
        options: ResolverOptions,
        sockets: crate::adapters::kept_sockets::KeptSockets,
    ) -> Arc<Self> {
        Arc::new(Self {
            networks,
            mesh,
            runtime,
            upstream: upstream::Upstream::new(options.resolv_conf.clone(), options.upstream_port),
            options,
            state: RwLock::new(State::default()),
            changed: Notify::new(),
            looked: Notify::new(),
            asked: tokio::sync::watch::Sender::new(false),
            generation: tokio::sync::watch::Sender::new(0),
            in_flight: Arc::new(Semaphore::new(IN_FLIGHT)),
            rotation: AtomicUsize::new(0),
            sockets,
            settling: tokio::sync::Mutex::new(()),
        })
    }

    fn read(&self) -> RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, State> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Serves until the process ends. On a host that holds no cluster
    /// network this only reads one small file every few seconds.
    pub async fn run(self: Arc<Self>) {
        tokio::spawn(Arc::clone(&self).watch_engine());
        tokio::spawn(Arc::clone(&self).ask_peers());
        let mut listeners = Listeners::default();
        let mut missing_since = None::<Instant>;
        let mut first = true;
        loop {
            // An address the node is to listen on and does not have yet (at
            // boot the mesh may come up after mmd started) is tried again
            // soon: the other nodes are already asking. Not for ever: a
            // host that never gets the address would look at its engine
            // every second for nothing.
            let missing = self.look(&mut listeners).await;
            // The sockets the previous mmd left that the first look did
            // not take are on addresses nobody listens on any more.
            if std::mem::take(&mut first) {
                self.sockets.release_unclaimed();
            }
            let patience = if missing {
                let since = *missing_since.get_or_insert_with(Instant::now);
                if since.elapsed() < self.options.address_patience {
                    ADDRESS_AGAIN
                } else {
                    LOOK_AGAIN
                }
            } else {
                missing_since = None;
                LOOK_AGAIN
            };
            if tokio::time::timeout(patience, self.changed.notified())
                .await
                .is_ok()
            {
                tokio::time::sleep(SETTLE).await;
            }
        }
    }

    /// Follows the engine's events so a change shows in the answers at
    /// once. Only a node that holds cluster networks runs an engine command.
    async fn watch_engine(self: Arc<Self>) {
        loop {
            if self.read().networks.is_some() {
                let changed = || self.changed.notify_one();
                if let Err(error) = self.runtime.watch_containers(&changed).await {
                    tracing::debug!(%error, "the engine's event stream ended");
                }
                // The stream ends when the engine stops: look at once.
                self.changed.notify_one();
            }
            tokio::time::sleep(WATCH_AGAIN).await;
        }
    }

    /// Reads the stored network and mesh state, binds what is to be
    /// listened on, and looks at the engine. Returns whether an address to
    /// listen on is still missing on the host.
    async fn look(self: &Arc<Self>, listeners: &mut Listeners) -> bool {
        let networks = match self.networks.stored() {
            Ok(networks) => networks,
            Err(error) => {
                self.write().note(
                    "networks",
                    Some(format!("cannot read the stored networks: {error}")),
                );
                return false;
            }
        };
        let Some(networks) = networks else {
            listeners.close(self);
            let mut state = self.write();
            // Leaves and their count outlive the networks.
            let leaves = std::mem::take(&mut state.leaves);
            *state = State::default();
            state.leaves = leaves;
            return false;
        };
        // A bridge the engine created again gets its settings back before
        // containers on it ask for members on other nodes: Podman makes a
        // network's bridge with its first container. Only a bridge with a
        // new interface index costs more than reading sysfs.
        self.settle_bridges().await;
        // The stored state only: asking the host for its facts would also
        // read every route of the kernel, with a child process per look.
        let mesh = match self.mesh.stored() {
            Ok(state) => state,
            Err(error) => {
                tracing::debug!(%error, "cannot read the mesh state");
                None
            }
        };
        let gateways = networks
            .local()
            .iter()
            .map(|network| network.gateway)
            .collect::<Vec<_>>();
        let (waiting, unbound) = listeners
            .bind(self, &gateways, mesh.as_ref().map(|mesh| mesh.address))
            .await;
        let engine_networks = networks
            .local()
            .into_iter()
            .map(|network| network.network)
            .collect::<Vec<_>>();
        let observed = self.runtime.network_containers(&engine_networks).await;
        let mut state = self.write();
        let before = leaving::answers(&state);
        state.note("listen", unbound);
        state.local.node.clone_from(&networks.node);
        match observed {
            Ok(mut observed) => {
                // A leaving container stays out until its leave is over.
                observed.retain(|container| !state.leaves.is_own(&container.id));
                state.local = NodeRecords::observed(&networks, observed);
                let held = state.held.clone();
                state.local.hold(&held);
                state.unseen = 0;
                state.note("containers", None);
            }
            Err(error) => {
                // One failed look keeps what was seen last; an engine that
                // stays away has stopped its containers with it.
                state.unseen = state.unseen.saturating_add(1);
                if state.unseen >= UNSEEN_LIMIT {
                    state.local.containers.clear();
                }
                state.note(
                    "containers",
                    Some(format!(
                        "cannot list this node's containers, so its answers may be out of date: {error}"
                    )),
                );
            }
        }
        state.mesh_address = mesh.as_ref().map(|mesh| mesh.address);
        set_peers(&mut state, mesh.as_ref());
        state.networks = Some(networks);
        state.looked = true;
        state.refresh_fresh();
        self.settle(&mut state, before);
        drop(state);
        self.looked.notify_one();
        waiting
    }

    /// Has each bridge the engine created since it was last settled get its
    /// settings (`ClusterNetworkHost::settle_bridges`), one caller at a
    /// time.
    async fn settle_bridges(&self) {
        let _one = self.settling.lock().await;
        if let Err(error) = self.networks.settle_bridges().await {
            tracing::warn!(%error, "a cluster network's bridge did not get its settings");
        }
    }

    /// Asks every other node what it runs, again and again.
    async fn ask_peers(self: Arc<Self>) {
        let interval = Duration::from_millis(EXCHANGE_INTERVAL_MS);
        loop {
            let (looked, from, peers) = {
                let state = self.read();
                let peers = state
                    .peers
                    .iter()
                    .map(|(name, peer)| (name.clone(), peer.address))
                    .collect::<Vec<_>>();
                (state.looked, state.mesh_address, peers)
            };
            if !looked {
                // Nothing is known yet, or this host holds no cluster
                // network: wait for the next look.
                self.looked.notified().await;
                continue;
            }
            // Without a mesh address there is nobody to ask.
            let answers =
                futures_util::future::join_all(peers.iter().filter_map(|(_, address)| {
                    from.map(|from| {
                        exchange::fetch(from, *address, self.options.exchange_port, interval)
                    })
                }))
                .await;
            if answers.len() == peers.len() {
                let mut state = self.write();
                let before = leaving::answers(&state);
                for ((name, _), answer) in peers.iter().zip(answers) {
                    let accepted = match (answer, &state.networks) {
                        (Ok(records), Some(networks)) => records
                            .accepted_from(name, networks)
                            .ok_or_else(|| "it sent the records of another node".to_owned()),
                        (Ok(_), None) => continue,
                        (Err(problem), _) => Err(problem),
                    };
                    let Some(peer) = state.peers.get_mut(name) else {
                        continue;
                    };
                    match accepted {
                        Ok(records) => {
                            peer.records = Some(records);
                            peer.heard = Instant::now();
                            peer.answered = true;
                            peer.problem = None;
                        }
                        Err(problem) => peer.problem = Some(problem),
                    }
                }
                state.refresh_fresh();
                self.settle(&mut state, before);
            }
            self.asked.send_replace(true);
            tokio::time::sleep(interval).await;
        }
    }

    /// Whether `address` is the mesh address of this node or of a peer:
    /// the only addresses the exchange port answers.
    fn is_mesh_member(&self, address: IpAddr) -> bool {
        let IpAddr::V4(address) = address else {
            return false;
        };
        let state = self.read();
        state.mesh_address == Some(address)
            || state.peers.values().any(|peer| peer.address == address)
    }

    /// What this node tells the other nodes, as JSON.
    fn records_for_peers(&self) -> Option<Vec<u8>> {
        let state = self.read();
        state.networks.as_ref()?;
        let mut records = state.local.for_peers();
        state.leaves.sign(&mut records);
        records.scaling.clone_from(&state.scaling);
        match serde_json::to_vec(&records) {
            Ok(records) => Some(records),
            Err(error) => {
                tracing::warn!(%error, "cannot serialize the node's records");
                None
            }
        }
    }

    /// The reply to one lookup, or `None` for a message that gets none: a
    /// lookup that does not come from a container of this node is not
    /// answered at all.
    async fn reply(&self, message: &[u8], source: IpAddr, limit: usize) -> Option<Vec<u8>> {
        let IpAddr::V4(source) = source else {
            return None;
        };
        let mut asked = self.asked.subscribe();
        if tokio::time::timeout(FIRST_ANSWERS, asked.wait_for(|asked| *asked))
            .await
            .is_err()
        {
            tracing::debug!("answering a lookup before every node was asked");
        }
        let (question, resolution, newcomer) = {
            let state = self.read();
            let map = ServiceMap {
                networks: state.networks.as_ref()?,
                local: &state.local,
                peers: &state.fresh,
            };
            match wire::parse(message) {
                Query::Noise => return None,
                Query::Refused { reply } => {
                    return (map.resolve(source, "") != Resolution::NotAsked).then_some(reply);
                }
                Query::Question(question) => {
                    let resolution = map.resolve(source, &question.name);
                    let newcomer = !state.local.containers.iter().any(|container| {
                        container
                            .endpoints
                            .iter()
                            .any(|endpoint| endpoint.address == source)
                    });
                    (question, resolution, newcomer)
                }
            }
        };
        // A container this node has not looked at yet is new, and so may be
        // its network's bridge (Podman creates it with the network's first
        // container, which asks for a name at once). The bridge gets its
        // settings before the answer, so the container's first connection
        // to another node's member finds it.
        if newcomer && resolution != Resolution::NotAsked {
            self.settle_bridges().await;
        }
        match resolution {
            Resolution::NotAsked => None,
            Resolution::Addresses(mut addresses) if question.class == wire::CLASS_IN => {
                // Start at another address each time: many clients take
                // the first one.
                if !addresses.is_empty() {
                    let turn = self.rotation.fetch_add(1, Ordering::Relaxed) % addresses.len();
                    addresses.rotate_left(turn);
                }
                Some(question.answer(&addresses, NAME_TIME_TO_LIVE, limit))
            }
            Resolution::Addresses(_) | Resolution::Elsewhere => Some(
                match self
                    .upstream
                    .forward(message, limit > wire::DATAGRAM_LIMIT)
                    .await
                {
                    Forwarded::Reply(reply) => reply,
                    // A definite "no such name" lets the client go on (to
                    // its next search domain, or to its error) at once. A
                    // refusal would have some C libraries wait and retry.
                    Forwarded::Refused => question.failure(Failure::NoSuchName),
                    Forwarded::Silent => question.failure(Failure::Server),
                },
            ),
        }
    }
}

/// Brings the peer list to the mesh's. A peer that stays keeps what it
/// last sent.
fn set_peers(state: &mut State, mesh: Option<&MeshState>) {
    let wanted = mesh
        .into_iter()
        .flat_map(|mesh| &mesh.peers)
        .filter_map(|peer| {
            let address = peer
                .allowed_ips
                .iter()
                .find(|allowed| allowed.prefix == 32)?;
            Some((peer.name.clone(), address.address))
        })
        .collect::<BTreeMap<_, _>>();
    state.peers.retain(|name, _| wanted.contains_key(name));
    for (name, address) in wanted {
        state
            .peers
            .entry(name)
            .and_modify(|peer| peer.address = address)
            .or_insert_with(|| Peer {
                address,
                records: None,
                heard: Instant::now(),
                answered: false,
                problem: None,
            });
    }
}

impl ServiceNames for NameResolver {
    fn addresses(
        &self,
        network: &str,
        service: &str,
        reach: crate::domain::discovery::Reach,
    ) -> (Vec<Ipv4Addr>, bool) {
        let state = self.read();
        let Some(networks) = state.networks.as_ref() else {
            return (Vec::new(), false);
        };
        ServiceMap {
            networks,
            local: &state.local,
            peers: &state.fresh,
        }
        .service_addresses(network, service, reach)
    }

    fn report(&self, prefix: &str) -> NameReport {
        let state = self.read();
        let Some(networks) = state.networks.as_ref() else {
            return NameReport::default();
        };
        let mut report = ServiceMap {
            networks,
            local: &state.local,
            peers: &state.fresh,
        }
        .report(prefix);
        let expiry = Duration::from_millis(PEER_EXPIRY_MS);
        let silent = state
            .peers
            .iter()
            .filter(|(_, peer)| peer.heard.elapsed() >= expiry)
            .map(|(name, peer)| {
                let reason = peer.problem.as_deref().unwrap_or("no answer");
                let since = if peer.answered { "has not heard" } else { "never heard" };
                format!(
                    "{since} from node '{name}' at {}:{} ({reason}), so that node's containers are left out of the answers here; `mm target mesh status` shows whether the two nodes reach each other, and `mm target setup plan` installs the same mmd on every node",
                    peer.address,
                    self.options.exchange_port
                )
            });
        report.problems = state
            .problems
            .values()
            .cloned()
            .chain(silent)
            .map(|detail| NameProblem {
                node: networks.node.clone(),
                detail,
            })
            .collect();
        report
    }
}
