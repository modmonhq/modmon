//! Leaving containers in the resolver (`domain::leaving`): this node's own,
//! left out of its answers and listed for the other nodes, and the other
//! nodes', which this node confirms once neither its answers nor its
//! ingress proxy send them anything.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use super::{NameResolver, State};
use crate::domain::discovery::{ContainerRecord, NodeRecords, PEER_EXPIRY_MS};

/// How long a node waits for a peer that is too old to confirm: two of its
/// exchanges, after which it has dropped the container from its answers.
const UNCONFIRMING_PEER: Duration = Duration::from_secs(6);

#[derive(Debug, Default)]
pub(super) struct Leaves {
    /// This node's leaving containers, by the first twelve characters of
    /// their id: the generation they were marked at, and when.
    own: BTreeMap<String, (u64, Instant)>,
    /// The other nodes' leaving containers this node heard of, with the
    /// generation it heard of them at.
    seen: BTreeMap<String, u64>,
    /// The generation of the answers the node's ingress proxy runs; `None`
    /// on a node without one.
    proxy: Option<u64>,
    /// Counts every change of what this node answers and routes to.
    generation: u64,
}

impl Leaves {
    /// Whether the container with engine id `id` is leaving this node.
    pub(super) fn is_own(&self, id: &str) -> bool {
        self.own.contains_key(short(id))
    }

    /// What this node tells the others besides its containers.
    pub(super) fn sign(&self, records: &mut NodeRecords) {
        records.leaving = self.own.keys().cloned().collect();
        records.confirmed = Some(
            self.seen
                .iter()
                .filter(|(_, heard)| self.proxy.is_none_or(|applied| applied >= **heard))
                .map(|(id, _)| id.clone())
                .collect(),
        );
    }
}

fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

/// What the answers of a node depend on, to tell whether a look or an
/// exchange changed them.
pub(super) fn answers(state: &State) -> (Vec<ContainerRecord>, Vec<Vec<ContainerRecord>>) {
    answers_of(&state.local, &state.fresh)
}

impl NameResolver {
    /// Counts a change of the answers, and records which leaving containers
    /// of the other nodes this node has heard of by now.
    pub(super) fn settle(
        &self,
        state: &mut State,
        before: (Vec<ContainerRecord>, Vec<Vec<ContainerRecord>>),
    ) {
        let leaves = &mut state.leaves;
        if answers_of(&state.local, &state.fresh) != before {
            leaves.generation = leaves.generation.saturating_add(1);
            self.generation.send_replace(leaves.generation);
        }
        let heard = state
            .fresh
            .iter()
            .flat_map(|records| records.leaving.iter())
            .cloned()
            .collect::<BTreeSet<_>>();
        leaves.seen.retain(|id, _| heard.contains(id));
        for id in heard {
            leaves.seen.entry(id).or_insert(leaves.generation);
        }
    }

    /// Takes the containers `ids` out of this node's answers and tells the
    /// other nodes they are leaving. Marking one again changes nothing.
    pub fn mark_leaving(&self, ids: &[String]) {
        {
            let mut state = self.write();
            let fresh = ids
                .iter()
                .map(|id| short(id).to_owned())
                .filter(|id| !state.leaves.own.contains_key(id))
                .collect::<Vec<_>>();
            if fresh.is_empty() {
                return;
            }
            let before = answers(&state);
            state
                .local
                .containers
                .retain(|record| !fresh.contains(&record.id));
            // Marked at the generation that leaves them out.
            let generation = state.leaves.generation.saturating_add(1);
            for id in fresh {
                state.leaves.own.insert(id, (generation, Instant::now()));
            }
            self.settle(&mut state, before);
            if state.leaves.generation < generation {
                state.leaves.generation = generation;
                self.generation.send_replace(generation);
            }
        }
        self.changed.notify_one();
    }

    /// Forgets the leaves of `ids`: the containers are gone.
    pub fn unmark_leaving(&self, ids: &[String]) {
        let mut state = self.write();
        for id in ids {
            state.leaves.own.remove(short(id));
        }
    }

    /// Takes back the leaves of `ids`: the containers stay, and the next
    /// look, which comes at once, gives them out again.
    pub fn recall_leaving(&self, ids: &[String]) {
        self.unmark_leaving(ids);
        self.changed.notify_one();
    }

    /// Whether every node, this one included, confirmed that it no longer
    /// answers with the container `id` nor routes to it. A node not heard
    /// from for as long as the nodes keep each other's records has dropped
    /// it anyway.
    #[must_use]
    pub fn leave_confirmed(&self, id: &str) -> bool {
        // Before every other node was asked once, they are not known yet.
        if !*self.asked.borrow() {
            return false;
        }
        let id = short(id);
        let state = self.read();
        let Some((marked, at)) = state.leaves.own.get(id) else {
            return false;
        };
        if state.leaves.proxy.is_some_and(|applied| applied < *marked) {
            return false;
        }
        let expiry = Duration::from_millis(PEER_EXPIRY_MS);
        state.peers.values().all(|peer| {
            if peer.heard.elapsed() >= expiry {
                return true;
            }
            match peer.records.as_ref().map(|records| &records.confirmed) {
                None => false,
                Some(Some(confirmed)) => confirmed.iter().any(|confirmed| confirmed == id),
                Some(None) => at.elapsed() >= UNCONFIRMING_PEER,
            }
        })
    }

    /// The generation of what this node answers now, and a way to wait for
    /// the next one.
    #[must_use]
    pub fn generation(&self) -> tokio::sync::watch::Receiver<u64> {
        self.generation.subscribe()
    }

    /// Waits until the node has looked at its engine and asked every other
    /// node once, at most a few seconds: before that it knows too little to
    /// route by.
    pub async fn settled(&self) {
        let mut asked = self.asked.subscribe();
        if tokio::time::timeout(Duration::from_secs(10), asked.wait_for(|asked| *asked))
            .await
            .is_err()
        {
            tracing::debug!("routing before every other node was asked");
        }
    }

    /// The node's ingress proxy runs the answers of `generation`, or the
    /// node runs no proxy (`None`).
    pub fn proxy_runs(&self, generation: Option<u64>) {
        self.write().leaves.proxy = generation;
    }
}

fn answers_of(
    local: &NodeRecords,
    fresh: &[NodeRecords],
) -> (Vec<ContainerRecord>, Vec<Vec<ContainerRecord>>) {
    (
        local.containers.clone(),
        fresh
            .iter()
            .map(|records| records.containers.clone())
            .collect(),
    )
}
