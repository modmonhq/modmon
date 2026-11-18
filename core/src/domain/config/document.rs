//! A loaded project configuration and the files it came from.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::MmConfig;

/// Top-level keys that may move from `.config/mm.yaml` to their own
/// `.config/mm/<key>.yaml`, in schema order. Only `schema-version` must stay in
/// the anchor file.
pub const SPLITTABLE_CONFIG_KEYS: &[&str] = &[
    "project",
    "default-environment",
    "target",
    "runtime",
    "build",
    "networks",
    "stacks",
    "modules",
    "providers",
    "secrets",
    "bindings",
    "environment-delivery",
    "environments",
    "ingress",
    "development-mounts",
    "development-mail",
    "jobs",
    "schedules",
    "gates",
    "users",
    "agents",
    "targets",
    "registries",
    "hardening",
    "release-inputs",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDocument {
    pub config: MmConfig,
    pub root: PathBuf,
    /// The anchor `.config/mm.yaml`.
    pub path: PathBuf,
    /// The file that declares each top-level key present in the configuration:
    /// `path`, or `.config/mm/<key>.yaml` when the key is split out. Keys left
    /// at their defaults are absent.
    pub sources: BTreeMap<String, PathBuf>,
    /// Fingerprint of every configuration source, including split files and
    /// any local override. This is an operation identity, not necessarily the
    /// hash of `path` alone.
    pub source_hash: String,
}

impl ConfigDocument {
    /// The file that declares top-level `key`, or the anchor when the key is
    /// not split out. Diagnostics use it so errors name the right file.
    #[must_use]
    pub fn source(&self, key: &str) -> &std::path::Path {
        self.sources.get(key).unwrap_or(&self.path)
    }

    /// Top-level keys that live in their own `.config/mm/<key>.yaml` file.
    pub fn split_sources(&self) -> impl Iterator<Item = (&str, &std::path::Path)> {
        self.sources
            .iter()
            .filter(|(_, path)| **path != self.path)
            .map(|(key, path)| (key.as_str(), path.as_path()))
    }
}
