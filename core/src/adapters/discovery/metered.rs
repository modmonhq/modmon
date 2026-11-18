//! The containers the node's metrics are sampled for: the ones the
//! resolver already lists, so sampling asks the engine nothing.

use crate::app::ports::MeteredContainers;
use crate::domain::discovery::Health;

use super::NameResolver;

impl MeteredContainers for NameResolver {
    /// Every running container the node's last look at its engine found,
    /// by the first twelve characters of its id. A container that is
    /// being drained is no longer among them.
    fn running(&self) -> Vec<String> {
        self.read()
            .local
            .containers
            .iter()
            .filter(|container| container.health != Health::Stopped && !container.id.is_empty())
            .map(|container| container.id.clone())
            .collect()
    }
}
