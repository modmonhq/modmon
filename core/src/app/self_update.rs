//! `mm update`: replaces this machine's `mm`, and the `mmd` and
//! `mm-ingress-helper` beside it, with a verified release.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::app::ports::{
    CommandSpec, InstallationFiles, ProcessRunner, ProgressReporter, ReleaseFiles,
};
use crate::domain::error::MmError;
use crate::domain::progress::{ProgressEvent, StepOutcome};
use crate::domain::release::{
    INSTALLER_COMMAND, ReleaseDownload, ReleaseTag, checksums_download, installed_target,
};
use crate::domain::self_update::{
    HOSTS_NOTE, Installation, MAX_ARCHIVE_BYTES, RECEIPT_FILE, Replacement, SelfUpdatePlan,
    SelfUpdateRequest, SelfUpdateResult, VersionStep, binaries_to_replace, helper_refresh_command,
    reports_version, version_step,
};

const DOWNLOAD: &str = "download";
const UNPACK: &str = "unpack";
const REPLACE: &str = "replace";
const VERSION_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct SelfUpdateService {
    releases: Arc<dyn ReleaseFiles>,
    files: Arc<dyn InstallationFiles>,
    process: Arc<dyn ProcessRunner>,
}

impl SelfUpdateService {
    #[must_use]
    pub fn new(
        releases: Arc<dyn ReleaseFiles>,
        files: Arc<dyn InstallationFiles>,
        process: Arc<dyn ProcessRunner>,
    ) -> Self {
        Self {
            releases,
            files,
            process,
        }
    }

    /// What `mm update` would replace; reads the release and changes nothing.
    pub async fn plan(&self, request: &SelfUpdateRequest) -> Result<SelfUpdatePlan, MmError> {
        let target = request.target.clone().ok_or_else(|| {
            refusal(format!(
                "mm publishes no release for this platform ({} {}); build mm from a checkout with ./install.sh",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))
        })?;
        let (tag, download) = self.release(request.version.as_ref(), &target).await?;
        let step = version_step(
            &request.current,
            &tag,
            request.version.is_some(),
            request.force,
        )
        .map_err(refusal)?;
        let kind = match step {
            VersionStep::UpToDate => {
                return Ok(SelfUpdatePlan::UpToDate {
                    current: request.current.clone(),
                    latest: tag.binary_version(),
                });
            }
            VersionStep::InstalledNewer => {
                return Ok(SelfUpdatePlan::InstalledNewer {
                    current: request.current.clone(),
                    latest: tag.binary_version(),
                });
            }
            VersionStep::Replace(kind) => kind,
        };
        let installation = self.files.inspect(&request.executable)?;
        refuse_installation(&installation, &tag, request.force)?;
        Ok(SelfUpdatePlan::Replace(Box::new(Replacement {
            kind,
            current: request.current.clone(),
            version: tag.binary_version(),
            binaries: binaries_to_replace(&installation.beside, &download),
            directory: installation.directory,
            executable: installation.executable,
            download,
            checkout: installation.checkout,
            privileged_helper: installation.privileged_helper,
        })))
    }

    /// Downloads, checks, and swaps in what `plan` named. Either every
    /// binary is replaced or none is.
    pub async fn apply(
        &self,
        plan: &Replacement,
        progress: &dyn ProgressReporter,
    ) -> Result<SelfUpdateResult, MmError> {
        let staging = self.files.staging(&plan.directory)?;
        let replaced = self
            .stage(plan, &staging, progress)
            .await
            .and_then(|staged| self.replace_all(plan, &staged, progress));
        let cleaned = self.files.remove(&staging);
        replaced?;
        cleaned?;
        if plan.checkout.is_some() {
            self.files.remove(&plan.directory.join(RECEIPT_FILE))?;
        }
        Ok(SelfUpdateResult {
            previous: plan.current.clone(),
            version: plan.version.clone(),
            tag: plan.download.tag.clone(),
            directory: plan.directory.clone(),
            binaries: plan.binaries.clone(),
            helper_refresh: self.helper_refresh(plan)?,
            hosts: HOSTS_NOTE.to_owned(),
        })
    }

    async fn release(
        &self,
        requested: Option<&ReleaseTag>,
        target: &str,
    ) -> Result<(ReleaseTag, ReleaseDownload), MmError> {
        if let Some(tag) = requested.filter(|tag| tag.is_candidate()) {
            let sums = self
                .releases
                .checksums(&tag.tag)
                .await
                .map_err(unavailable)?;
            let download = checksums_download(&sums, tag, target).map_err(refusal)?;
            return Ok((tag.clone(), download));
        }
        let manifest = self
            .releases
            .manifest(requested.map(|tag| tag.tag.as_str()))
            .await
            .map_err(unavailable)?;
        if let Some(tag) = requested.filter(|tag| tag.tag != manifest.tag) {
            return Err(refusal(format!(
                "the manifest of {} describes {}; nothing was changed",
                tag.tag, manifest.tag
            )));
        }
        let tag = ReleaseTag::parse(&manifest.tag).map_err(refusal)?;
        let download = manifest
            .download(target)
            .map_err(refusal)?
            .ok_or_else(|| {
                refusal(format!(
                    "release {} has no archive for {target}; install another release with --version, or build mm from a checkout with ./install.sh",
                    tag.tag
                ))
            })?;
        Ok((tag, download))
    }

    /// The archive downloaded and checked, and its binaries unpacked and
    /// run once, in `staging`.
    async fn stage(
        &self,
        plan: &Replacement,
        staging: &Path,
        progress: &dyn ProgressReporter,
    ) -> Result<PathBuf, MmError> {
        let download = &plan.download;
        let archive = staging.join(&download.file);
        progress.report(ProgressEvent::started(
            DOWNLOAD,
            format!("Downloading {}", download.file),
        ));
        let fetched = self
            .releases
            .download(
                download,
                &archive,
                download.size.unwrap_or(MAX_ARCHIVE_BYTES),
                (progress, DOWNLOAD),
            )
            .await
            .and_then(|file| check_download(download, &file.sha256, file.size));
        finish(
            progress,
            DOWNLOAD,
            &fetched,
            "Downloaded and checked the archive",
        );
        fetched?;
        progress.report(ProgressEvent::started(
            UNPACK,
            format!("Unpacking and running mm {}", plan.version),
        ));
        let unpacked = staging.join("unpacked");
        let checked = self.unpack_and_run(plan, &archive, &unpacked).await;
        finish(progress, UNPACK, &checked, "Unpacked mm and ran it");
        checked.map(|()| unpacked)
    }

    async fn unpack_and_run(
        &self,
        plan: &Replacement,
        archive: &Path,
        unpacked: &Path,
    ) -> Result<(), MmError> {
        let found = self.files.unpack(archive, unpacked)?;
        for name in &plan.binaries {
            if !found.contains(name) {
                return Err(refusal(format!(
                    "the archive of {} holds no {name}, which sits beside mm in {}; nothing was changed",
                    plan.download.tag,
                    plan.directory.display()
                )));
            }
            let binary = unpacked.join(name);
            let output = self
                .process
                .run(
                    CommandSpec::new(binary.display().to_string())
                        .arg("--version")
                        .timeout(VERSION_TIMEOUT),
                )
                .await?;
            if !output.success() || !reports_version(&output.stdout, name, &plan.version) {
                return Err(MmError::Security {
                    message: format!(
                        "{name} from {} printed '{}' for --version, not '{name} {}'; nothing was changed",
                        plan.download.file,
                        output.stdout.lines().next().unwrap_or_default().trim(),
                        plan.version
                    ),
                });
            }
        }
        Ok(())
    }

    fn replace_all(
        &self,
        plan: &Replacement,
        staged: &Path,
        progress: &dyn ProgressReporter,
    ) -> Result<(), MmError> {
        progress.report(ProgressEvent::started(
            REPLACE,
            format!("Replacing {}", plan.binaries.join(", ")),
        ));
        let mut kept = Vec::new();
        let mut failure = None;
        for name in &plan.binaries {
            let destination = destination(plan, name);
            match self.files.replace(&staged.join(name), &destination) {
                Ok(backup) => kept.push((backup, destination)),
                Err(error) => {
                    failure = Some((name.clone(), error));
                    break;
                }
            }
        }
        let result = match failure {
            None => kept
                .iter()
                .try_for_each(|(backup, _)| self.files.remove(backup)),
            Some((name, error)) => Err(self.roll_back(&kept, &name, error)),
        };
        finish(progress, REPLACE, &result, "Replaced the binaries");
        result
    }

    fn roll_back(&self, kept: &[(PathBuf, PathBuf)], failed: &str, error: MmError) -> MmError {
        let mut stuck = Vec::new();
        for (backup, destination) in kept.iter().rev() {
            if let Err(restore) = self.files.restore(backup, destination) {
                stuck.push(format!(
                    "{} (the old one is at {}: {restore})",
                    destination.display(),
                    backup.display()
                ));
            }
        }
        let message = if stuck.is_empty() {
            format!("replacing {failed} failed, so every binary was put back as it was: {error}")
        } else {
            format!(
                "replacing {failed} failed ({error}), and these could not be put back: {}; move each old one back by hand, or run the installer: {INSTALLER_COMMAND}",
                stuck.join("; ")
            )
        };
        MmError::Update { message }
    }

    fn helper_refresh(&self, plan: &Replacement) -> Result<Option<String>, MmError> {
        let Some(privileged) = &plan.privileged_helper else {
            return Ok(None);
        };
        if !plan.binaries.iter().any(|name| name == "mm-ingress-helper") {
            return Ok(None);
        }
        let helper = plan.directory.join("mm-ingress-helper");
        if self.files.same_file_contents(&helper, privileged)? {
            return Ok(None);
        }
        Ok(Some(helper_refresh_command(&helper, privileged)))
    }
}

/// The refusals of an installation `mm update` cannot or should not replace.
fn refuse_installation(
    installation: &Installation,
    tag: &ReleaseTag,
    force: bool,
) -> Result<(), MmError> {
    let directory = installation.directory.display();
    if !installation.writable {
        return Err(refusal(format!(
            "{directory} is not writable, so mm update cannot replace mm there; run `sudo mm update`, or install into a directory you own: {INSTALLER_COMMAND} -s -- --install-dir ~/.local/bin"
        )));
    }
    if let Some(checkout) = installation.checkout.as_deref().filter(|_| !force) {
        return Err(refusal(format!(
            "mm in {directory} was built from {checkout}; update it there with `git pull && ./install.sh`, or pass --force to replace it with release {}",
            tag.tag
        )));
    }
    Ok(())
}

fn destination(plan: &Replacement, name: &str) -> PathBuf {
    if name == "mm" {
        plan.executable.clone()
    } else {
        plan.directory.join(name)
    }
}

fn check_download(download: &ReleaseDownload, sha256: &str, size: u64) -> Result<(), MmError> {
    if sha256 != download.sha256 || download.size.is_some_and(|expected| expected != size) {
        return Err(MmError::Security {
            message: format!(
                "checksum mismatch for {}: the download does not match the release's {}; nothing was changed",
                download.file, download.checked_against
            ),
        });
    }
    Ok(())
}

fn finish<T>(progress: &dyn ProgressReporter, step: &str, result: &Result<T, MmError>, done: &str) {
    progress.report(match result {
        Ok(_) => ProgressEvent::finished(step, StepOutcome::Done, Some(done.to_owned())),
        Err(_) => ProgressEvent::finished(step, StepOutcome::Failed, None),
    });
}

fn refusal(message: String) -> MmError {
    MmError::Update { message }
}

fn unavailable(message: String) -> MmError {
    MmError::Update {
        message: format!("{message}; nothing was changed"),
    }
}

/// The request for this `mm`, as it runs.
#[must_use]
pub fn self_update_request(
    executable: PathBuf,
    version: Option<ReleaseTag>,
    force: bool,
) -> SelfUpdateRequest {
    SelfUpdateRequest {
        current: env!("CARGO_PKG_VERSION").to_owned(),
        executable,
        target: installed_target().map(str::to_owned),
        version,
        force,
    }
}
