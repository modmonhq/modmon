use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};

pub const RELEASE_MANIFEST_URL: &str =
    "https://github.com/modmonhq/modmon/releases/latest/download/manifest.json";
pub const INSTALLER_URL: &str = "https://modmon.dev/install.sh";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub product: String,
    pub channel: String,
    pub version: String,
    pub tag: String,
    pub commit: String,
    pub published_at: String,
    pub source_date_epoch: u64,
    pub minimum_installer_schema: u32,
    pub artifacts: BTreeMap<String, ReleaseArtifact>,
    pub source: ReleaseFile,
    pub sbom: ReleaseFile,
    pub release_certificate: ReleaseFile,
    pub provenance: Vec<ReleaseFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub path: String,
    pub format: ArchiveFormat,
    pub sha256: String,
    pub size: u64,
    pub binaries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ArchiveFormat {
    #[serde(rename = "tar.gz")]
    TarGz,
    #[serde(rename = "zip")]
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateStatus {
    UpToDate,
    UpdateAvailable,
    InstalledVersionNewer,
    UnsupportedPlatform,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCheckReport {
    pub status: UpdateStatus,
    pub current: String,
    pub latest: Option<String>,
    pub target: Option<String>,
    pub message: String,
}

impl ReleaseManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        for target in [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ] {
            let key = format!("\"{target}\"");
            if bytes
                .windows(key.len())
                .filter(|window| *window == key.as_bytes())
                .count()
                > 1
            {
                return Err(format!("duplicate release target: {target}"));
            }
        }
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("release manifest is not schema 1 JSON: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.minimum_installer_schema != 1 {
            return Err("unsupported release manifest schema".to_owned());
        }
        if self.product != "mm" || self.channel != "stable" {
            return Err("release manifest must describe the mm stable channel".to_owned());
        }
        let version = stable_version(&self.version)?;
        if self.tag != format!("v{version}") {
            return Err("release tag and semantic version differ".to_owned());
        }
        if self.commit.len() != 40
            || !self
                .commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("release commit must be a full hexadecimal Git object ID".to_owned());
        }
        if self.source_date_epoch == 0
            || !publication_time_matches(&self.published_at, self.source_date_epoch)
        {
            return Err("release publication time/source epoch is invalid".to_owned());
        }
        if self.artifacts.is_empty() {
            return Err("release manifest has no platform artifacts".to_owned());
        }
        for (target, artifact) in &self.artifacts {
            validate_target(target)?;
            artifact.validate(target, &self.tag)?;
        }
        self.source.validate(&self.tag)?;
        self.sbom.validate(&self.tag)?;
        self.release_certificate.validate(&self.tag)?;
        if self.provenance.is_empty() {
            return Err("release manifest has no provenance inventory".to_owned());
        }
        for provenance in &self.provenance {
            provenance.validate(&self.tag)?;
        }
        Ok(())
    }
}

impl ReleaseArtifact {
    fn validate(&self, target: &str, tag: &str) -> Result<(), String> {
        validate_release_path(&self.path, tag)?;
        validate_digest_and_size(&self.sha256, self.size)?;
        let extension = match self.format {
            ArchiveFormat::TarGz => ".tar.gz",
            ArchiveFormat::Zip => ".zip",
        };
        if !self.path.ends_with(extension) {
            return Err("artifact path does not match its archive format".to_owned());
        }
        let binaries = self.binaries.iter().cloned().collect::<BTreeSet<_>>();
        let linux = target.ends_with("-unknown-linux-gnu");
        if binaries.len() != self.binaries.len()
            || binaries.is_empty()
            || !binaries.iter().all(|name| match name.as_str() {
                "mm" | "mmd" => true,
                "mm-ingress-helper" => linux,
                _ => false,
            })
            || !binaries.contains("mm")
        {
            return Err("artifact binary inventory is invalid".to_owned());
        }
        Ok(())
    }
}

impl ReleaseFile {
    fn validate(&self, tag: &str) -> Result<(), String> {
        validate_release_path(&self.path, tag)?;
        validate_digest_and_size(&self.sha256, self.size)
    }
}

pub fn compare_installed(
    current: &str,
    target: Option<&str>,
    manifest: &ReleaseManifest,
) -> Result<UpdateCheckReport, String> {
    let current_version = stable_version(current)?;
    let latest_version = stable_version(&manifest.version)?;
    let target = target.map(str::to_owned);
    let (status, message) = if target
        .as_ref()
        .is_none_or(|value| !manifest.artifacts.contains_key(value))
    {
        (
            UpdateStatus::UnsupportedPlatform,
            format!(
                "stable {} has no artifact for {}",
                manifest.version,
                target.as_deref().unwrap_or("this platform")
            ),
        )
    } else {
        match current_version.cmp(&latest_version) {
            std::cmp::Ordering::Less => (
                UpdateStatus::UpdateAvailable,
                format!(
                    "mm {current} can be updated to {}; rerun the installer, {INSTALLER_URL}",
                    manifest.version
                ),
            ),
            std::cmp::Ordering::Equal => (
                UpdateStatus::UpToDate,
                format!("mm {current} is the latest stable release"),
            ),
            std::cmp::Ordering::Greater => (
                UpdateStatus::InstalledVersionNewer,
                format!(
                    "installed mm {current} is newer than stable {}; no downgrade is recommended",
                    manifest.version
                ),
            ),
        }
    };
    Ok(UpdateCheckReport {
        status,
        current: current.to_owned(),
        latest: Some(manifest.version.clone()),
        target,
        message,
    })
}

#[must_use]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn installed_target() -> Option<&'static str> {
    Some("x86_64-unknown-linux-gnu")
}

#[must_use]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub fn installed_target() -> Option<&'static str> {
    Some("aarch64-unknown-linux-gnu")
}

#[must_use]
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub fn installed_target() -> Option<&'static str> {
    Some("x86_64-apple-darwin")
}

#[must_use]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn installed_target() -> Option<&'static str> {
    Some("aarch64-apple-darwin")
}

#[must_use]
#[cfg(not(any(
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
    all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
)))]
pub fn installed_target() -> Option<&'static str> {
    None
}

fn stable_version(value: &str) -> Result<Version, String> {
    let version =
        Version::parse(value).map_err(|_| format!("invalid semantic version: {value}"))?;
    if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != value {
        return Err(format!("version is not canonical stable SemVer: {value}"));
    }
    Ok(version)
}

fn validate_target(target: &str) -> Result<(), String> {
    if matches!(
        target,
        "x86_64-unknown-linux-gnu"
            | "aarch64-unknown-linux-gnu"
            | "x86_64-apple-darwin"
            | "aarch64-apple-darwin"
    ) {
        Ok(())
    } else {
        Err(format!("unsupported release target: {target}"))
    }
}

fn validate_release_path(path: &str, tag: &str) -> Result<(), String> {
    let prefix = format!("releases/{tag}/");
    if !path.starts_with(&prefix)
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte))
    {
        return Err(format!("unsafe immutable release path: {path}"));
    }
    Ok(())
}

fn validate_digest_and_size(digest: &str, size: u64) -> Result<(), String> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || size == 0
    {
        return Err("release file checksum or size is invalid".to_owned());
    }
    Ok(())
}

fn publication_time_matches(value: &str, source_date_epoch: u64) -> bool {
    let Ok(timestamp) =
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
    else {
        return false;
    };
    timestamp.offset() == time::UtcOffset::UTC
        && timestamp.nanosecond() == 0
        && u64::try_from(timestamp.unix_timestamp()) == Ok(source_date_epoch)
}

#[cfg(test)]
mod tests;
