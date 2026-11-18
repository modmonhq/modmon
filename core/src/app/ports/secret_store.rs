use std::net::Ipv4Addr;

use zeroize::Zeroizing;

use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

/// One environment's managed secret store, as the operator's machine knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreScope<'a> {
    pub project: ProjectId,
    pub environment: &'a str,
    pub provider: &'a str,
}

/// The managed stores' authorities and recovery copies on the operator's machine.
pub trait ClusterStoreKeeper: Send + Sync {
    /// The age recipient of the operator's store on this machine.
    fn operator_recipient(&self, project: ProjectId) -> Result<String, MmError>;

    /// The environment's authority certificate, made on first use.
    fn authority(&self, scope: StoreScope<'_>) -> Result<String, MmError>;

    /// A certificate for `node` at `address` and on loopback, for the key of
    /// the node's certificate request.
    fn sign(
        &self,
        scope: StoreScope<'_>,
        request_pem: &str,
        member: (&str, Ipv4Addr),
    ) -> Result<String, MmError>;

    /// Keeps the recovery copy a first initialisation sealed to the operator.
    fn keep_recovery(&self, scope: StoreScope<'_>, sealed: &str) -> Result<(), MmError>;

    /// The recovery copy, sealed to the operator, when this machine holds one.
    fn recovery(&self, scope: StoreScope<'_>) -> Result<Option<String>, MmError>;

    /// `plaintext` sealed with age to `recipient`, as ASCII armour.
    fn seal(&self, recipient: &str, plaintext: &[u8]) -> Result<String, MmError>;

    /// What was sealed to this machine's operator, opened.
    fn open(&self, project: ProjectId, sealed: &str) -> Result<Zeroizing<Vec<u8>>, MmError>;

    /// The recovery copy opened here and sealed to a member's store.
    fn reseal_recovery(
        &self,
        scope: StoreScope<'_>,
        recipient: &str,
    ) -> Result<Zeroizing<String>, MmError>;
}
