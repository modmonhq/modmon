//! `mm update`: which release replaces the `mm` on this machine, and what
//! an archive of it may hold.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::release::{RELEASE_BINARIES, ReleaseDownload, ReleaseTag, stable_version};

pub const RECEIPT_FILE: &str = "install-receipt.txt";
pub const PRIVILEGED_HELPER: &str = "/usr/local/libexec/mm/mm-ingress-helper";
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const HOSTS_NOTE: &str = "Hosts on mm 0.1.1 or later keep working with this mm. Upgrade them when you are ready: `mm target setup plan --upgrade`, then apply that plan with --upgrade.";

/// What `mm update` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdateRequest {
    pub current: String,
    pub executable: PathBuf,
    pub target: Option<String>,
    pub version: Option<ReleaseTag>,
    pub force: bool,
}

/// The installation `mm update` replaces, as its files show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub directory: PathBuf,
    pub executable: PathBuf,
    pub beside: Vec<String>,
    pub checkout: Option<String>,
    pub writable: bool,
    pub privileged_helper: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", tag = "action")]
pub enum SelfUpdatePlan {
    UpToDate { current: String, latest: String },
    InstalledNewer { current: String, latest: String },
    Replace(Box<Replacement>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ReplacementKind {
    Upgrade,
    Reinstall,
    Downgrade,
}

/// The binaries `mm update` replaces, and the archive they come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Replacement {
    pub kind: ReplacementKind,
    pub current: String,
    pub version: String,
    pub directory: PathBuf,
    pub executable: PathBuf,
    pub binaries: Vec<String>,
    pub download: ReleaseDownload,
    pub checkout: Option<String>,
    pub privileged_helper: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct SelfUpdateResult {
    pub previous: String,
    pub version: String,
    pub tag: String,
    pub directory: PathBuf,
    pub binaries: Vec<String>,
    pub helper_refresh: Option<String>,
    pub hosts: String,
}

/// How the release compares with this `mm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionStep {
    Replace(ReplacementKind),
    UpToDate,
    InstalledNewer,
}

/// What `mm update` does with `tag` while `current` runs: a named, older
/// release only with `force`, and the same release again only with `force`.
pub fn version_step(
    current: &str,
    tag: &ReleaseTag,
    named: bool,
    force: bool,
) -> Result<VersionStep, String> {
    let current_version = stable_version(current)?;
    Ok(match current_version.cmp(&tag.version) {
        std::cmp::Ordering::Less => VersionStep::Replace(ReplacementKind::Upgrade),
        std::cmp::Ordering::Equal if force => VersionStep::Replace(ReplacementKind::Reinstall),
        std::cmp::Ordering::Equal => VersionStep::UpToDate,
        std::cmp::Ordering::Greater if !named => VersionStep::InstalledNewer,
        std::cmp::Ordering::Greater if force => VersionStep::Replace(ReplacementKind::Downgrade),
        std::cmp::Ordering::Greater => {
            return Err(format!(
                "mm {current} is newer than {}; pass --force with --version {} to install the older release",
                tag.tag, tag.tag
            ));
        }
    })
}

/// The binaries to replace: `mm`, and those of the others the release
/// holds that sit beside it now. A release that lists no binaries (a
/// candidate, read from `SHA256SUMS`) is taken to hold them all.
#[must_use]
pub fn binaries_to_replace(beside: &[String], download: &ReleaseDownload) -> Vec<String> {
    RELEASE_BINARIES
        .iter()
        .filter(|name| **name == "mm" || beside.iter().any(|present| present == *name))
        .filter(|name| {
            download.binaries.is_empty() || download.binaries.iter().any(|held| held == *name)
        })
        .map(|name| (*name).to_owned())
        .collect()
}

/// One entry of a release archive: its top directory, and the binary it is
/// when `mm update` installs it. Absolute paths, `.` and `..`, backslashes,
/// and empty names are refused.
pub fn archive_member(path: &str) -> Result<(&str, Option<&str>), String> {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    let parts = trimmed.split('/').collect::<Vec<_>>();
    if trimmed.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(format!("the archive holds an unsafe path: {path}"));
    }
    let binary = match parts.as_slice() {
        [_, name] if RELEASE_BINARIES.contains(name) => Some(*name),
        _ => None,
    };
    Ok((parts[0], binary))
}

/// Whether `printed` is what `name --version` prints for `version`.
#[must_use]
pub fn reports_version(printed: &str, name: &str, version: &str) -> bool {
    printed.lines().next().map(str::trim) == Some(format!("{name} {version}").as_str())
}

/// The installation-receipt line that names the checkout an `mm` was built
/// from, when there is one.
#[must_use]
pub fn receipt_checkout(receipt: &str) -> String {
    receipt
        .lines()
        .find_map(|line| line.strip_prefix("source="))
        .map_or_else(|| "a checkout".to_owned(), str::to_owned)
}

#[must_use]
pub fn helper_refresh_command(helper: &std::path::Path, privileged: &std::path::Path) -> String {
    format!(
        "sudo install -o root -g root -m 755 {} {}",
        shell_word(&helper.display().to_string()),
        shell_word(&privileged.display().to_string())
    )
}

fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-+,:@=".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
