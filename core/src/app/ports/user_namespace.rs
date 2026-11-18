use std::collections::{BTreeMap, BTreeSet};

use crate::domain::error::MmError;
use crate::domain::user_namespace::{ServiceIdsChoice, ServiceIdsOwner};

/// The user ids of the services on a host that runs rootful Podman: each
/// service's own 65536 host ids, kept on the host from one deployment to
/// the next.
pub trait ServiceUserIds: std::fmt::Debug + Send + Sync {
    /// The ids of each service, keeping what a service already has and
    /// recording what a new one gets. Refused with what to change when the
    /// host has no range for mm or none left.
    fn reserve(
        &self,
        wanted: &BTreeSet<ServiceIdsOwner>,
    ) -> Result<BTreeMap<ServiceIdsOwner, ServiceIdsChoice>, MmError>;

    /// Forgets the ids of `owners`, which a refused plan gave them.
    fn release(&self, owners: &BTreeSet<ServiceIdsOwner>) -> Result<(), MmError>;
}
