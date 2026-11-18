//! Keys and values earlier releases accepted. Reading one fails with what
//! replaces it, where serde would only say the field is unknown.

use schemars::JsonSchema;
use serde::Deserializer;
use serde::de::Error;

/// The value of a removed key. It is never present: the key's deserializer
/// refuses any value, and an absent key defaults to this.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
pub struct Removed;

pub const DOCKER_SWARM_DRIVER: &str = "the docker-swarm runtime driver was removed; use docker-compose or podman-compose, and list several hosts under targets.<name>.nodes";

const SWARM_BLOCK: &str = "swarm: was removed; put the nodes' settings under cluster: instead (private-network.wireguard becomes mesh, storage and groups keep their names, address-pool and address-pool-mask are gone, and ingress now lists the nodes that run the ingress proxy, written like a placement)";

const MEMBERS: &str = "members: was removed; list every host of the target under nodes: instead, each with a name and a host";

fn refuse<'de, D: Deserializer<'de>>(message: &str) -> Result<Removed, D::Error> {
    Err(D::Error::custom(message))
}

pub fn swarm_block<'de, D: Deserializer<'de>>(_: D) -> Result<Removed, D::Error> {
    refuse::<D>(SWARM_BLOCK)
}

pub fn members<'de, D: Deserializer<'de>>(_: D) -> Result<Removed, D::Error> {
    refuse::<D>(MEMBERS)
}

#[cfg(test)]
#[path = "removed_tests.rs"]
mod tests;
