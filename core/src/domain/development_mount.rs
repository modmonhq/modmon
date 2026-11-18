use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DevelopmentMountMode {
    #[default]
    ReadWrite,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DevelopmentMountPreset {
    Laravel,
    Node,
    Dotnet,
    Rust,
}

impl DevelopmentMountPreset {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Laravel => "laravel",
            Self::Node => "node",
            Self::Dotnet => "dotnet",
            Self::Rust => "rust",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PreservedContainerPath {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DevelopmentBindMount {
    /// Relative to the module path, like the primary source.
    pub source: PathBuf,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DevelopmentMountConfig {
    /// Relative to the module path, never the workstation root.
    pub source: PathBuf,
    /// Absolute POSIX path in the selected container services.
    pub target: String,
    #[serde(default)]
    pub mode: DevelopmentMountMode,
    /// Additional source directories mounted into the same services with the same mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional: Vec<DevelopmentBindMount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserve: Vec<PreservedContainerPath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safe_exceptions: Vec<PathBuf>,
    pub services: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<DevelopmentMountPreset>,
}

impl DevelopmentMountConfig {
    pub fn sources(&self) -> impl Iterator<Item = (&std::path::Path, &str)> {
        std::iter::once((self.source.as_path(), self.target.as_str())).chain(
            self.additional
                .iter()
                .map(|mount| (mount.source.as_path(), mount.target.as_str())),
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DevelopmentMountPolicy {
    /// Additional case-insensitive glob-like names (`*` matches any substring).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitive_patterns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MountEvidenceSource {
    Explicit,
    ComposeWorkingDirectory,
    DockerfileWorkdir,
    FrameworkPreset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MountConfidence {
    High,
    Medium,
    Low,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentMountEvidence {
    pub source: MountEvidenceSource,
    pub location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentMountProposal {
    pub module: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<DevelopmentMountConfig>,
    pub evidence: Vec<DevelopmentMountEvidence>,
    pub confidence: MountConfidence,
    pub confirmation_required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confirmations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DevelopmentMountPlan {
    pub schema: String,
    pub project_id: crate::domain::workspace::ProjectId,
    pub project_name: String,
    pub config_hash: String,
    pub environment: String,
    pub target: String,
    pub local_target: bool,
    pub development_environment: bool,
    pub runtime: crate::domain::runtime::RuntimeKind,
    pub proposal: DevelopmentMountProposal,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PreservedVolumeManifest {
    pub module: String,
    pub container_path: String,
    pub logical_name: String,
    pub physical_name: String,
    pub resource: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPlatform {
    Unix,
    Windows,
}

impl HostPlatform {
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(windows)]
        {
            Self::Windows
        }
        #[cfg(not(windows))]
        {
            Self::Unix
        }
    }

    pub fn normalize_relative(self, value: &str) -> Result<PathBuf, String> {
        let normalized = value.replace('\\', "/");
        if normalized.is_empty()
            || normalized.starts_with('/')
            || normalized.starts_with("//")
            || normalized.as_bytes().get(1) == Some(&b':')
        {
            return Err("development mount sources must be portable relative paths".to_owned());
        }
        let mut output = PathBuf::new();
        for component in normalized.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    return Err(
                        "development mount sources cannot contain parent traversal".to_owned()
                    );
                }
                component if component.chars().any(char::is_control) => {
                    return Err(
                        "development mount sources cannot contain control characters".to_owned(),
                    );
                }
                component => output.push(component),
            }
        }
        if output.as_os_str().is_empty() {
            Ok(PathBuf::from("."))
        } else {
            Ok(output)
        }
    }
}

pub fn is_absolute_container_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() > 1
        && !value.contains("//")
        && !value.split('/').any(|component| component == "..")
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests;
