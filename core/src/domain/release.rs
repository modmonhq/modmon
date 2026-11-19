use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};

pub const RELEASE_MANIFEST_URL: &str =
    "https://github.com/modmonhq/modmon/releases/latest/download/manifest.json";
pub const INSTALLER_URL: &str = "https://modmon.dev/install.sh";
pub const INSTALLER_COMMAND: &str =
    "curl --proto '=https' --tlsv1.2 -LsSf https://modmon.dev/install.sh | sh";
pub const READER_SCHEMA: u32 = 1;
pub const RELEASE_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];
pub const RELEASE_BINARIES: [&str; 3] = ["mm", "mm-ingress-helper", "mmd"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub product: String,
    pub channel: String,
    pub version: String,
    pub tag: String,
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub source_date_epoch: u64,
    pub minimum_installer_schema: u32,
    #[serde(deserialize_with = "known_artifacts")]
    pub artifacts: BTreeMap<String, ReleaseArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ReleaseFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sbom: Option<ReleaseFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_certificate: Option<ReleaseFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ReleaseFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseArtifact {
    pub path: String,
    pub format: ArchiveFormat,
    pub sha256: String,
    pub size: u64,
    pub binaries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    #[serde(other)]
    Unknown,
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

#[derive(Deserialize)]
struct SchemaHeader {
    schema_version: u32,
    minimum_installer_schema: u32,
    #[serde(default)]
    tag: Option<String>,
}

impl ReleaseManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let header: SchemaHeader = serde_json::from_slice(bytes)
            .map_err(|error| format!("the release manifest is not one mm can read: {error}"))?;
        check_reader_schema(&header)?;
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("the release manifest is not one mm can read: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        check_reader_schema(&SchemaHeader {
            schema_version: self.schema_version,
            minimum_installer_schema: self.minimum_installer_schema,
            tag: Some(self.tag.clone()),
        })?;
        if self.product != "mm" || self.channel != "stable" {
            return Err("release manifest must describe the mm stable channel".to_owned());
        }
        let version = stable_version(&self.version)?;
        if self.tag != format!("v{version}") {
            return Err("release tag and semantic version differ".to_owned());
        }
        Ok(())
    }

    /// The archive for `target`, checked; `None` when the release has none.
    pub fn artifact(&self, target: &str) -> Result<Option<&ReleaseArtifact>, String> {
        let Some(artifact) = self.artifacts.get(target) else {
            return Ok(None);
        };
        artifact.validate(target, &self.tag)?;
        Ok(Some(artifact))
    }
}

impl ReleaseArtifact {
    fn validate(&self, target: &str, tag: &str) -> Result<(), String> {
        release_file_name(&self.path, tag)?;
        validate_digest_and_size(&self.sha256, self.size)?;
        match self.format {
            ArchiveFormat::TarGz if self.path.ends_with(".tar.gz") => {}
            ArchiveFormat::TarGz => {
                return Err("artifact path does not match its archive format".to_owned());
            }
            ArchiveFormat::Zip | ArchiveFormat::Unknown => {
                return Err(format!(
                    "release {tag} packs mm for {target} in a format this mm cannot unpack; install it with the installer: {INSTALLER_COMMAND}"
                ));
            }
        }
        let names = self.binaries.iter().collect::<BTreeSet<_>>();
        if names.len() != self.binaries.len() || !self.binaries.iter().any(|name| name == "mm") {
            return Err(format!(
                "the {target} archive of release {tag} does not list mm once"
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    #[must_use]
    pub fn known_binaries(&self) -> Vec<String> {
        RELEASE_BINARIES
            .iter()
            .filter(|name| self.binaries.iter().any(|binary| binary == *name))
            .map(|name| (*name).to_owned())
            .collect()
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
    let artifact = match target.as_deref() {
        Some(target) => manifest.artifact(target)?,
        None => None,
    };
    let (status, message) = if artifact.is_none() {
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

pub fn stable_version(value: &str) -> Result<Version, String> {
    let version =
        Version::parse(value).map_err(|_| format!("invalid semantic version: {value}"))?;
    if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != value {
        return Err(format!("version is not canonical stable SemVer: {value}"));
    }
    Ok(version)
}

fn check_reader_schema(header: &SchemaHeader) -> Result<(), String> {
    if header.minimum_installer_schema > READER_SCHEMA {
        let release = header
            .tag
            .as_deref()
            .map_or_else(|| "this release".to_owned(), |tag| format!("release {tag}"));
        return Err(format!(
            "{release} needs a newer mm to update (its manifest needs readers of schema {}); run the installer: {INSTALLER_COMMAND}",
            header.minimum_installer_schema
        ));
    }
    if header.minimum_installer_schema == 0 || header.schema_version < READER_SCHEMA {
        return Err("unsupported release manifest schema".to_owned());
    }
    Ok(())
}

fn known_artifacts<'de, D>(deserializer: D) -> Result<BTreeMap<String, ReleaseArtifact>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Artifacts;
    impl<'de> serde::de::Visitor<'de> for Artifacts {
        type Value = BTreeMap<String, ReleaseArtifact>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("release artifacts by target")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut seen = BTreeSet::new();
            let mut artifacts = BTreeMap::new();
            while let Some(target) = map.next_key::<String>()? {
                if !seen.insert(target.clone()) {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate release target: {target}"
                    )));
                }
                if RELEASE_TARGETS.contains(&target.as_str()) {
                    artifacts.insert(target, map.next_value::<ReleaseArtifact>()?);
                } else {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
            Ok(artifacts)
        }
    }
    deserializer.deserialize_map(Artifacts)
}

fn release_file_name<'a>(path: &'a str, tag: &str) -> Result<&'a str, String> {
    let name = path
        .strip_prefix("releases/")
        .and_then(|rest| rest.strip_prefix(tag))
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|name| {
            !name.is_empty()
                && !name.starts_with('.')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        });
    name.ok_or_else(|| format!("unsafe immutable release path: {path}"))
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

#[cfg(test)]
mod tests;
