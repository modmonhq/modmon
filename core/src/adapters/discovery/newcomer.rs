//! Lookups from an address this node knows no container at yet.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{NameResolver, leaving};

/// How long a lookup waits for its asker to show in the engine.
const MEET_WITHIN: Duration = Duration::from_secs(1);
/// How long a lookup waits for the engine at most; a listing under way goes on.
const MEET_AT_MOST: Duration = Duration::from_secs(3);
/// How soon the engine is read again while the asker does not show.
const MEET_AGAIN: Duration = Duration::from_millis(100);
/// How long an address that never showed skips the engine read.
const STRANGER_FOR: Duration = Duration::from_secs(10);
/// How many such addresses are remembered.
const STRANGERS: usize = 256;

/// The addresses whose lookups did not find their asker in the engine, and
/// the lookups still waiting for a listing, by when each began.
#[derive(Debug, Default)]
pub(super) struct Strangers {
    pinned: BTreeMap<Ipv4Addr, Instant>,
    meeting: BTreeMap<Ipv4Addr, Instant>,
}

impl Strangers {
    fn recent(&self, source: Ipv4Addr) -> bool {
        self.pinned
            .get(&source)
            .is_some_and(|since| since.elapsed() < STRANGER_FOR)
    }

    fn add(&mut self, source: Ipv4Addr) {
        self.add_at(source, Instant::now());
    }

    fn add_at(&mut self, source: Ipv4Addr, now: Instant) {
        self.pinned
            .retain(|_, since| now.saturating_duration_since(*since) < STRANGER_FOR);
        if self.pinned.len() >= STRANGERS
            && !self.pinned.contains_key(&source)
            && let Some(oldest) = self
                .pinned
                .iter()
                .min_by_key(|(_, since)| **since)
                .map(|(address, _)| *address)
        {
            self.pinned.remove(&oldest);
        }
        self.pinned.insert(source, now);
    }

    fn meet(&mut self, source: Ipv4Addr, began: Instant) {
        self.meeting
            .retain(|_, since| since.elapsed() < STRANGER_FOR);
        if self.meeting.len() < STRANGERS || self.meeting.contains_key(&source) {
            self.meeting.entry(source).or_insert(began);
        }
    }
}

/// The one listing queued for the lookups that wait, and when the last
/// listing it ran started.
#[derive(Debug)]
pub(super) struct FollowUp {
    queued: AtomicBool,
    finished: tokio::sync::watch::Sender<Option<Instant>>,
}

impl Default for FollowUp {
    fn default() -> Self {
        Self {
            queued: AtomicBool::new(false),
            finished: tokio::sync::watch::Sender::new(None),
        }
    }
}

impl NameResolver {
    /// Reads the engine until it shows a container at `source`, for at least
    /// [`MEET_WITHIN`] and until a listing started since has finished, but
    /// never longer than [`MEET_AT_MOST`].
    pub(super) async fn meet(self: &Arc<Self>, source: Ipv4Addr) {
        if self.read().strangers.recent(source) {
            return;
        }
        if tokio::time::timeout(MEET_AT_MOST, self.seek(source))
            .await
            .is_err()
        {
            tracing::debug!(%source, "the engine did not list its containers in time for a lookup");
        }
    }

    async fn seek(self: &Arc<Self>, source: Ipv4Addr) {
        let began = Instant::now();
        let mut since = began;
        loop {
            self.write().strangers.meet(source, began);
            let mut finished = self.follow_up.finished.subscribe();
            if !self.follow_up.queued.swap(true, Ordering::SeqCst) {
                tokio::spawn(Arc::clone(self).list_for_lookups());
            }
            if finished
                .wait_for(|started| started.is_some_and(|started| started >= since))
                .await
                .is_err()
            {
                return;
            }
            if self.done_meeting(source, began) {
                return;
            }
            since = Instant::now();
            tokio::time::sleep(MEET_AGAIN).await;
        }
    }

    /// Whether a lookup from `source` that began at `began` is done
    /// waiting, after a listing started since it last looked.
    fn done_meeting(&self, source: Ipv4Addr, began: Instant) -> bool {
        let mut state = self.write();
        let done =
            state.knows(source) || state.strangers.recent(source) || began.elapsed() >= MEET_WITHIN;
        if done {
            if !state.knows(source) && state.listed.is_some_and(|started| started >= began) {
                state.strangers.add(source);
            }
            state.strangers.meeting.remove(&source);
        }
        done
    }

    /// The queued listing: once it has the engine, the next lookup queues
    /// another. A lookup that began before it started and waited
    /// [`MEET_WITHIN`] is settled here, whether or not it still waits.
    async fn list_for_lookups(self: Arc<Self>) {
        let mut listing = self.listing.lock().await;
        self.follow_up.queued.store(false, Ordering::SeqCst);
        let started = Instant::now();
        *listing = Some(started);
        self.list_containers().await;
        drop(listing);
        self.follow_up.finished.send_replace(Some(started));
        let mut state = self.write();
        let listed = state.listed == Some(started);
        let meeting = std::mem::take(&mut state.strangers.meeting);
        for (source, began) in meeting {
            if state.knows(source) {
                continue;
            }
            if began.elapsed() < MEET_WITHIN || began > started || !listed {
                state.strangers.meeting.insert(source, began);
            } else {
                state.strangers.add(source);
            }
        }
    }

    /// Takes this node's containers from the engine; a failure is left to the next look.
    async fn list_containers(&self) {
        let started = Instant::now();
        let Some(networks) = self.read().networks.clone() else {
            return;
        };
        let engine_networks = networks
            .local()
            .into_iter()
            .map(|network| network.network)
            .collect::<Vec<_>>();
        let Ok(observed) = self.runtime.network_containers(&engine_networks).await else {
            return;
        };
        let mut state = self.write();
        if state.networks.as_ref() != Some(&networks) {
            return;
        }
        let before = leaving::answers(&state);
        state.take_observed(&networks, observed);
        state.listed = Some(started);
        self.settle(&mut state, before);
    }
}

#[cfg(test)]
mod tests;
