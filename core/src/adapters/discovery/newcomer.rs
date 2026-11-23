//! Lookups from an address this node knows no container at yet.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use super::{NameResolver, leaving};

/// How long a lookup waits for its asker to show in the engine.
const MEET_WITHIN: Duration = Duration::from_secs(1);
/// How soon the engine is read again while the asker does not show.
const MEET_AGAIN: Duration = Duration::from_millis(100);
/// How long an address that never showed skips the engine read.
const STRANGER_FOR: Duration = Duration::from_secs(10);
/// How many such addresses are remembered.
const STRANGERS: usize = 256;

/// The addresses whose lookups did not find their asker in the engine.
#[derive(Debug, Default)]
pub(super) struct Strangers(BTreeMap<Ipv4Addr, Instant>);

impl Strangers {
    fn recent(&self, source: Ipv4Addr) -> bool {
        self.0
            .get(&source)
            .is_some_and(|since| since.elapsed() < STRANGER_FOR)
    }

    fn add(&mut self, source: Ipv4Addr) {
        self.0.retain(|_, since| since.elapsed() < STRANGER_FOR);
        if self.0.len() >= STRANGERS
            && !self.0.contains_key(&source)
            && let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(_, since)| **since)
                .map(|(address, _)| *address)
        {
            self.0.remove(&oldest);
        }
        self.0.insert(source, Instant::now());
    }
}

impl NameResolver {
    /// Reads the engine until it shows a container at `source`, for at least
    /// [`MEET_WITHIN`] and until a listing started since has finished.
    pub(super) async fn meet(&self, source: Ipv4Addr) {
        if self.read().strangers.recent(source) {
            return;
        }
        let began = Instant::now();
        let mut since = began;
        loop {
            {
                let mut listing = self.listing.lock().await;
                if self.read().knows(source) {
                    return;
                }
                if listing.is_none_or(|started| started < since) {
                    *listing = Some(Instant::now());
                    self.list_containers().await;
                }
            }
            if self.read().knows(source) {
                return;
            }
            if began.elapsed() >= MEET_WITHIN {
                let mut state = self.write();
                if state.listed.is_some_and(|started| started >= began) && !state.knows(source) {
                    state.strangers.add(source);
                }
                return;
            }
            since = Instant::now();
            tokio::time::sleep(MEET_AGAIN).await;
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
