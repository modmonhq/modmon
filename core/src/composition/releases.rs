//! Clients of the published releases, for the update check and `mm update`.

use std::sync::Arc;

use crate::adapters::process::TokioProcessRunner;
use crate::adapters::release::ReqwestReleaseClient;
use crate::adapters::release_cache::FilesystemReleaseCache;
use crate::adapters::self_update::LocalInstallation;
use crate::adapters::state::MmHome;
use crate::app::{HostBinarySource, SelfUpdateService, UpdateCheckService};
use crate::domain::error::MmError;
use crate::domain::release::RELEASE_ORIGIN;

const TEST_ORIGIN: &str = "MM_RELEASE_TEST_ORIGIN";
const ALLOW_TEST_ORIGIN: &str = "MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN";

/// GitHub's releases. The installer's tests serve a release from a local
/// origin instead, and so do mm's, only when both variables are set.
fn release_client() -> Result<Arc<ReqwestReleaseClient>, MmError> {
    let test_origin = std::env::var(TEST_ORIGIN)
        .ok()
        .filter(|origin| !origin.is_empty())
        .filter(|_| std::env::var(ALLOW_TEST_ORIGIN).as_deref() == Ok("1"));
    ReqwestReleaseClient::new(test_origin.as_deref().unwrap_or(RELEASE_ORIGIN))
        .map(Arc::new)
        .map_err(|message| MmError::Update { message })
}

pub(crate) fn update_check_service() -> Result<UpdateCheckService, MmError> {
    Ok(UpdateCheckService::new(release_client()?))
}

pub(crate) fn self_update_service() -> Result<SelfUpdateService, MmError> {
    Ok(SelfUpdateService::new(
        release_client()?,
        Arc::new(LocalInstallation),
        Arc::new(TokioProcessRunner),
    ))
}

/// Host binaries for setup without `--host-binaries`: the directory of this
/// `mm`, or its release, kept under the mm home.
pub(crate) fn host_binary_source(home: &MmHome) -> Result<HostBinarySource, MmError> {
    let beside = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()
        .and_then(|executable| executable.parent().map(std::path::Path::to_path_buf));
    Ok(HostBinarySource::new(
        release_client()?,
        Arc::new(LocalInstallation),
        Arc::new(FilesystemReleaseCache::new(home.root.join("releases"))),
        beside,
        env!("CARGO_PKG_VERSION"),
    ))
}
