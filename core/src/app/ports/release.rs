use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::app::ports::ProgressReporter;
use crate::domain::error::MmError;
use crate::domain::release::{ReleaseDownload, ReleaseManifest};
use crate::domain::self_update::Installation;

#[async_trait]
pub trait ReleaseManifestClient: Send + Sync {
    async fn fetch_stable(&self) -> Result<ReleaseManifest, String>;
}

/// What a download read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedFile {
    pub size: u64,
    pub sha256: String,
}

/// The files of the published releases.
#[async_trait]
pub trait ReleaseFiles: Send + Sync {
    /// The manifest of `tag`, or of the latest stable release.
    async fn manifest(&self, tag: Option<&str>) -> Result<ReleaseManifest, String>;

    async fn checksums(&self, tag: &str) -> Result<String, String>;

    /// Writes the archive to `destination`, reading at most one byte more
    /// than `limit`, and reports its bytes under the given step.
    async fn download(
        &self,
        download: &ReleaseDownload,
        destination: &Path,
        limit: u64,
        progress: (&dyn ProgressReporter, &str),
    ) -> Result<DownloadedFile, MmError>;
}

/// The files of the `mm` installation on this machine.
pub trait InstallationFiles: Send + Sync {
    fn inspect(&self, executable: &Path) -> Result<Installation, MmError>;

    /// A new private directory inside `directory`, on its filesystem.
    fn staging(&self, directory: &Path) -> Result<PathBuf, MmError>;

    /// Checks every entry of a release archive and writes the binaries
    /// `mm update` installs into `into`; returns their names.
    fn unpack(&self, archive: &Path, into: &Path) -> Result<Vec<String>, MmError>;

    /// Moves `staged` into `destination`'s place in one step, and returns
    /// where the file it replaced is kept.
    fn replace(&self, staged: &Path, destination: &Path) -> Result<PathBuf, MmError>;

    fn restore(&self, kept: &Path, destination: &Path) -> Result<(), MmError>;

    /// Removes a file or a directory; one that is gone is no error.
    fn remove(&self, path: &Path) -> Result<(), MmError>;

    fn same_file_contents(&self, left: &Path, right: &Path) -> Result<bool, MmError>;
}
