//! A member whose data is gone joining the store again, and what shows
//! that the store kept every version through it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{StoreFacts, StoreMember, StorePeer, StoreStatus};

/// The members that rejoin: they hold none of the store's data while the
/// Raft configuration a serving member reads still lists them. A member
/// that holds its data is never one of them, however long it was down.
#[must_use]
pub fn rejoining(
    members: &[StoreMember],
    facts: &BTreeMap<String, StoreFacts>,
    peers: &[StorePeer],
) -> Vec<String> {
    members
        .iter()
        .filter(|member| {
            facts
                .get(&member.name)
                .is_some_and(|facts| !facts.holds_data())
        })
        .filter(|member| peers.iter().any(|peer| peer.name == member.name))
        .map(|member| member.name.clone())
        .collect()
}

/// Whether the member that reports `status` may remove `stale`'s old
/// entry: it is another member, it serves unsealed, and it sees a leader
/// that is not `stale`.
#[must_use]
pub fn removal_through(status: &StoreStatus, stale: &str) -> bool {
    status.node != stale
        && status.initialized
        && !status.sealed
        && status
            .leader
            .as_deref()
            .is_some_and(|leader| leader != stale)
}

/// What `mmd openbao remove-peer` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StorePeerRemoval {
    pub peer: String,
    /// The node whose member removed it.
    pub through: String,
    /// False when the Raft configuration did not list it.
    pub removed: bool,
}

/// The versions the store keeps of one secret; never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreSecretVersions {
    /// Its location under the environment's path.
    pub location: String,
    pub current: u64,
    pub versions: Vec<StoreSecretVersion>,
}

/// One version, as the store's metadata lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct StoreSecretVersion {
    pub version: u64,
    pub created: String,
    pub deleted: bool,
    pub destroyed: bool,
}
