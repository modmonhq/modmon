//! A container that could not start because shared storage was away.
//!
//! An engine mounts a shared volume when it starts a container. When the
//! storage node does not answer, the mount fails after about half a minute,
//! the container stays created, and the engine does not try again: its
//! restart policy covers a container that ran and stopped, not one that
//! never started (measured on Docker Engine 29). So the node's `mmd` looks
//! for such containers and starts them once the storage node answers.

use std::net::Ipv4Addr;

use super::ENGINE_VOLUME_PREFIX;

/// A container that carries [`super::STORAGE_LABEL`] and is not running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingContainer {
    pub id: String,
    pub name: String,
    /// The label's value: the storage node's mesh address, or `local`.
    pub storage: String,
    /// The engine's state of the container, such as `created` or `exited`.
    pub state: String,
    /// What the engine said when the container last failed to start.
    pub error: String,
    /// The container's restart policy, such as `unless-stopped`.
    pub restart_policy: String,
}

impl WaitingContainer {
    /// The storage node's address, when this container is one to start
    /// once that node answers: the engine could not mount one of mm's
    /// shared volumes for it, and its restart policy says it should run. A
    /// container someone stopped, or one that failed for another reason,
    /// is left alone.
    #[must_use]
    pub fn waits_for(&self) -> Option<Ipv4Addr> {
        let stopped = matches!(self.state.as_str(), "created" | "exited");
        let restarts = matches!(
            self.restart_policy.as_str(),
            "always" | "unless-stopped" | "on-failure"
        );
        if !stopped || !restarts || !self.error.contains(ENGINE_VOLUME_PREFIX) {
            return None;
        }
        self.storage.parse().ok()
    }
}
