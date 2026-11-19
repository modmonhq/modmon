//! The `mm` and `mmd` setup installs on hosts: from `--host-binaries`, from
//! the directory of this `mm`, or from this `mm`'s own release, per CPU.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::ports::{InstallationFiles, ProgressReporter, ReleaseCache, ReleaseFiles};
use crate::domain::error::MmError;
use crate::domain::progress::{ProgressEvent, StepOutcome};
use crate::domain::release::ReleaseDownload;
use crate::domain::self_update::MAX_ARCHIVE_BYTES;
use crate::domain::target::{HOST_ARCHITECTURES, HostBinaries, linux_architecture};

const ELF_HEADER_BYTES: u64 = 64;
const MAX_HOST_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD: &str = "host-binaries";

/// A host setup installs on, and its CPU when the plan could read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCpu {
    pub host: String,
    pub cpu: Option<String>,
}

/// Where the host binaries come from when `--host-binaries` is not given.
#[derive(Clone)]
pub struct HostBinarySource {
    releases: Arc<dyn ReleaseFiles>,
    files: Arc<dyn InstallationFiles>,
    cache: Arc<dyn ReleaseCache>,
    beside: Option<PathBuf>,
    version: String,
}

impl HostBinarySource {
    /// `beside` is the directory of this `mm`; `version` is its release.
    #[must_use]
    pub fn new(
        releases: Arc<dyn ReleaseFiles>,
        files: Arc<dyn InstallationFiles>,
        cache: Arc<dyn ReleaseCache>,
        beside: Option<PathBuf>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            releases,
            files,
            cache,
            beside,
            version: version.into(),
        }
    }

    /// One set per CPU the hosts need, in CPU order: both CPUs when a host's
    /// CPU is not known, so the play can pick. With `given`, that directory
    /// only, refused for a host of another CPU.
    pub async fn resolve(
        &self,
        given: Option<&Path>,
        hosts: &[HostCpu],
        progress: &dyn ProgressReporter,
    ) -> Result<Vec<HostBinaries>, MmError> {
        if let Some(directory) = given {
            return Ok(vec![given_binaries(directory, hosts)?]);
        }
        let mut sets = Vec::new();
        for cpu in needed_cpus(hosts) {
            sets.push(self.for_cpu(cpu, progress).await?);
        }
        sets.sort_by(|left, right| left.architecture.cmp(&right.architecture));
        Ok(sets)
    }

    async fn for_cpu(
        &self,
        cpu: &str,
        progress: &dyn ProgressReporter,
    ) -> Result<HostBinaries, MmError> {
        if let Some(beside) = &self.beside {
            let source = format!("this mm's directory {}", beside.display());
            if let Ok(set) = read_set(beside, source)
                && set.architecture == cpu
            {
                return Ok(set);
            }
        }
        self.release_for(cpu, progress).await
    }

    async fn release_for(
        &self,
        cpu: &str,
        progress: &dyn ProgressReporter,
    ) -> Result<HostBinaries, MmError> {
        let tag = format!("v{}", self.version);
        let target = format!("{cpu}-unknown-linux-gnu");
        let not_released = |problem: String| MmError::Target {
            message: format!(
                "mm {} has no release to take mm and mmd for {cpu} from ({problem}). Pass --host-binaries DIR with the Linux mm and mmd for {cpu}, or run setup from a released mm (`mm update`)",
                self.version
            ),
        };
        let (directory, kept) = self.cache.entry(&tag, &target)?;
        let download = match self.releases.manifest(Some(&tag)).await {
            Ok(manifest) => manifest
                .download(&target)
                .map_err(not_released)?
                .ok_or_else(|| not_released(format!("release {tag} has no {target} archive")))?,
            Err(_) if kept.is_some() => return cached(&directory, &tag, cpu),
            Err(problem) => return Err(not_released(problem)),
        };
        if kept.as_deref() == Some(download.sha256.as_str()) {
            return cached(&directory, &tag, cpu);
        }
        progress.report(ProgressEvent::started(
            DOWNLOAD,
            format!("Downloading mm and mmd {tag} for {cpu}"),
        ));
        let staging = self.cache.staging(&tag)?;
        let stored = self.store(&download, &staging, progress).await;
        let removed = self.cache.remove(&staging);
        progress.report(match &stored {
            Ok(_) => ProgressEvent::finished(
                DOWNLOAD,
                StepOutcome::Done,
                Some(format!("Downloaded and checked mm and mmd {tag} for {cpu}")),
            ),
            Err(_) => ProgressEvent::finished(DOWNLOAD, StepOutcome::Failed, None),
        });
        let directory = stored?;
        removed?;
        cached(&directory, &tag, cpu)
    }

    async fn store(
        &self,
        download: &ReleaseDownload,
        staging: &Path,
        progress: &dyn ProgressReporter,
    ) -> Result<PathBuf, MmError> {
        let archive = staging.join(&download.file);
        let file = self
            .releases
            .download(
                download,
                &archive,
                download.size.unwrap_or(MAX_ARCHIVE_BYTES),
                (progress, DOWNLOAD),
            )
            .await?;
        crate::app::self_update::check_download(download, &file.sha256, file.size)?;
        let unpacked = staging.join("unpacked");
        let found = self.files.unpack(&archive, &unpacked)?;
        if let Some(missing) = ["mm", "mmd"]
            .into_iter()
            .find(|name| !found.iter().any(|found| found == name))
        {
            return Err(MmError::Target {
                message: format!(
                    "the {} archive of {} holds no {missing}; pass --host-binaries DIR with the Linux mm and mmd",
                    download.target, download.tag
                ),
            });
        }
        self.cache
            .keep(&download.tag, &download.target, &unpacked, &download.sha256)
    }
}

/// `--host-binaries DIR`, read as it is, refused for a host of another CPU.
pub fn given_binaries(directory: &Path, hosts: &[HostCpu]) -> Result<HostBinaries, MmError> {
    let canonical = std::fs::canonicalize(directory).map_err(|source| MmError::Io {
        context: format!(
            "resolving the host binaries directory {}",
            directory.display()
        ),
        source,
    })?;
    let source = canonical.display().to_string();
    let set = read_set(&canonical, source)?;
    refuse_other_cpus(&set, hosts, env!("CARGO_PKG_VERSION"))?;
    Ok(set)
}

fn read_set(directory: &Path, source: String) -> Result<HostBinaries, MmError> {
    Ok(HostBinaries {
        mm_digest: host_binary_digest(&directory.join("mm"))?,
        mmd_digest: host_binary_digest(&directory.join("mmd"))?,
        architecture: host_binaries_architecture(directory)?,
        directory: directory.to_path_buf(),
        source,
    })
}

fn cached(directory: &Path, tag: &str, cpu: &str) -> Result<HostBinaries, MmError> {
    let set = read_set(
        directory,
        format!("release {tag} for {cpu}, cached in {}", directory.display()),
    )?;
    if set.architecture != cpu {
        return Err(MmError::Security {
            message: format!(
                "the {cpu} archive of release {tag} holds mm and mmd for {}; remove {} and plan again",
                set.architecture,
                directory.display()
            ),
        });
    }
    Ok(set)
}

fn needed_cpus(hosts: &[HostCpu]) -> Vec<&'static str> {
    if hosts.is_empty() || hosts.iter().any(|host| host.cpu.is_none()) {
        return HOST_ARCHITECTURES.to_vec();
    }
    HOST_ARCHITECTURES
        .into_iter()
        .filter(|cpu| hosts.iter().any(|host| host.cpu.as_deref() == Some(*cpu)))
        .collect()
}

fn refuse_other_cpus(set: &HostBinaries, hosts: &[HostCpu], version: &str) -> Result<(), MmError> {
    let Some((host, cpu)) = hosts.iter().find_map(|host| {
        host.cpu
            .as_deref()
            .filter(|cpu| *cpu != set.architecture)
            .map(|cpu| (host.host.as_str(), cpu))
    }) else {
        return Ok(());
    };
    Err(MmError::Target {
        message: format!(
            "{host} is {cpu}, and {} holds mm and mmd for {}. Pass --host-binaries with the Linux pair for {cpu} (from mm-v{version}-{cpu}-unknown-linux-gnu.tar.gz), or leave it out to use this mm's release for each host",
            set.directory.display(),
            set.architecture
        ),
    })
}

/// The CPU both `mm` and `mmd` in `directory` are built for: `x86_64` or
/// `aarch64`. Anything else, or two different CPUs, is refused with the
/// release archives to take them from.
pub(crate) fn host_binaries_architecture(directory: &Path) -> Result<String, MmError> {
    let mut found = Vec::new();
    for name in ["mm", "mmd"] {
        let path = directory.join(name);
        let mut header = Vec::new();
        std::fs::File::open(&path)
            .and_then(|file| file.take(ELF_HEADER_BYTES).read_to_end(&mut header))
            .map_err(|source| MmError::Io {
                context: format!("reading host binary {}", path.display()),
                source,
            })?;
        let architecture = linux_architecture(&header).map_err(|why| MmError::Target {
            message: format!(
                "{} {why}; setup installs it on Linux hosts. {RELEASE_ARCHIVES}",
                path.display()
            ),
        })?;
        found.push(architecture);
    }
    match found.as_slice() {
        [mm, mmd] if mm == mmd => Ok((*mm).to_owned()),
        [mm, mmd] => Err(MmError::Target {
            message: format!(
                "{} holds mm for {mm} and mmd for {mmd}; setup installs them together. {RELEASE_ARCHIVES}",
                directory.display()
            ),
        }),
        _ => Err(MmError::Target {
            message: format!("{} holds no mm and mmd", directory.display()),
        }),
    }
}

const RELEASE_ARCHIVES: &str = "Pass --host-binaries with a directory holding mm and mmd from the Linux release archive for the hosts' CPU: mm-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz or mm-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz, or leave --host-binaries out to use this mm's release";

/// `--host-binaries` without a source for the default: the directory is
/// required then.
pub(crate) fn given_host_binaries(
    given: Option<&std::path::Path>,
    hosts: &[crate::app::HostCpu],
) -> Result<crate::domain::target::HostBinaries, MmError> {
    let directory = given.ok_or_else(|| MmError::Target {
        message: "pass --host-binaries DIR with mm and mmd built for the host".to_owned(),
    })?;
    crate::app::given_binaries(directory, hosts)
}

/// BLAKE3 of a host binary, read without following a final symlink.
pub fn host_binary_digest(path: &std::path::Path) -> Result<String, MmError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!(
            "reading host binary {}; pass --host-binaries with mm and mmd built for the host",
            path.display()
        ),
        source,
    })?;
    if !metadata.file_type().is_file() {
        return Err(MmError::Target {
            message: format!(
                "host binary '{}' must be a regular file, not a link or directory",
                path.display()
            ),
        });
    }
    if metadata.len() > MAX_HOST_BINARY_BYTES {
        return Err(MmError::Target {
            message: format!(
                "host binary '{}' exceeds {MAX_HOST_BINARY_BYTES} bytes",
                path.display()
            ),
        });
    }
    let file = std::fs::File::open(path).map_err(|source| MmError::Io {
        context: format!("opening host binary {}", path.display()),
        source,
    })?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(
        &mut std::io::Read::take(file, MAX_HOST_BINARY_BYTES),
        &mut hasher,
    )
    .map_err(|source| MmError::Io {
        context: format!("hashing host binary {}", path.display()),
        source,
    })?;
    Ok(hasher.finalize().to_hex().to_string())
}
