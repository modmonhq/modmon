//! Wiring for targets whose hosts take the project's files from mm
//! (`delivery: push`) instead of fetching them.

use std::sync::Arc;

use crate::adapters::remote::GitCommitBundles;
use crate::app::ports::CommitBundles;
use crate::domain::config::ConfigDocument;

/// What builds the bundles mm sends to the hosts of the repo target
/// `target`, when that target is delivered by push.
pub(super) fn pushed_commits(
    document: &ConfigDocument,
    target: &str,
) -> Option<Arc<dyn CommitBundles>> {
    let repo = document.config.targets.get(target)?;
    repo.delivery.is_push().then(|| {
        let bundles: Arc<dyn CommitBundles> =
            Arc::new(GitCommitBundles::new(document.root.clone()));
        bundles
    })
}
