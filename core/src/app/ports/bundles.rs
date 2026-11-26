use async_trait::async_trait;

use crate::domain::error::MmError;

/// One commit's bundle on this machine, ready to send to hosts that take
/// the project's files from the client.
#[derive(Clone)]
pub struct PreparedBundle {
    pub offer: crate::domain::delivery::BundleOffer,
    pub files: u64,
    /// The bundle's file; `None` when this checkout does not hold the
    /// commit, so there is nothing to send.
    pub path: Option<std::path::PathBuf>,
    /// Keeps the file until the last copy of this is dropped.
    pub keep: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

/// Builds the bundle of a commit from the checkout mm runs in.
#[async_trait]
pub trait CommitBundles: Send + Sync {
    async fn bundle(&self, commit: &str) -> Result<PreparedBundle, MmError>;
}
