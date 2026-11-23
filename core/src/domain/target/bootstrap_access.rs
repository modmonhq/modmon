//! The optional `mm target setup` step that installs the remote access layer:
//! `mm` and `mmd` from one release, the locked `mm` relay account, the sshd
//! drop-in, the project's host policy enrollment and deploy key, and the
//! `mmd` service.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::host_binaries::{HostBinaries, is_lower_hex_digest, validate_host_binaries};
use super::{BootstrapCapability, hash_field};
use crate::domain::host_policy::{BreakGlassKey, parse_public_key, valid_user_name};

/// At most this many break-glass keys per setup run.
pub const MAX_BREAK_GLASS_KEYS: usize = 16;

/// What the access step installs, as reviewed in the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BootstrapAccess {
    /// Git URL `mmd` fetches the project from with its own deploy key.
    /// Empty when mm pushes the project's files (`delivery: push`): the
    /// host is then enrolled without an origin and gets no deploy key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    /// The git server's SSH host key, `<type> <base64>`; required for SSH
    /// origins and pinned in the host policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_host_key: Option<String>,
    /// Recovery keys added to the host policy. They are admin on every
    /// project the host runs; keep their private halves offline.
    pub break_glass: Vec<BreakGlassKey>,
    /// `mm` and `mmd` for the host's CPU, or for each CPU when the plan
    /// could not read the host's and the play picks.
    pub binaries: Vec<HostBinaries>,
    /// `--host-binaries` as given, which the apply command repeats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_binaries: Option<PathBuf>,
    /// `--upgrade`: install over another build or an older release.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub upgrade: bool,
    /// `--downgrade`: install over a newer release.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub downgrade: bool,
    /// What setup replaces on the host, from the release its `mmd` reported
    /// when the plan was made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
    /// The AppArmor profiles setup installs in `/etc/apparmor.d` and gives
    /// rootful Podman's containers, where the host's kernel runs AppArmor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apparmor_profiles: Vec<String>,
}

impl BootstrapAccess {
    /// Whether the host takes the project's files from mm and fetches
    /// nothing.
    #[must_use]
    pub const fn pushes(&self) -> bool {
        self.origin.is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.pushes() {
            crate::domain::host_policy::validate_origin(
                &self.origin,
                self.origin_host_key.as_deref(),
            )?;
        } else if self.origin_host_key.is_some() {
            return Err("an origin host key needs an origin".to_owned());
        }
        if self
            .apparmor_profiles
            .iter()
            .any(|profile| !crate::domain::remote::CONTAINER_PROFILES.contains(&profile.as_str()))
        {
            return Err(
                "remote access setup names an AppArmor profile mm does not ship".to_owned(),
            );
        }
        if self.break_glass.is_empty() || self.break_glass.len() > MAX_BREAK_GLASS_KEYS {
            return Err(format!(
                "remote access setup needs 1-{MAX_BREAK_GLASS_KEYS} break-glass keys"
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        let mut fingerprints = std::collections::BTreeSet::new();
        for entry in &self.break_glass {
            if !valid_user_name(&entry.name) || !names.insert(entry.name.as_str()) {
                return Err(format!(
                    "break-glass name '{}' must be unique and match [a-z][a-z0-9-]{{0,62}}",
                    entry.name
                ));
            }
            // Setup passes keys to Ansible, which would template braces.
            if entry.key.contains(['{', '}']) {
                return Err(format!(
                    "break-glass key '{}' must not contain braces",
                    entry.name
                ));
            }
            let key = parse_public_key(&entry.key)
                .map_err(|message| format!("break-glass key '{}': {message}", entry.name))?;
            if !fingerprints.insert(key.fingerprint) {
                return Err(format!("break-glass key '{}' is listed twice", entry.name));
            }
        }
        validate_host_binaries(&self.binaries)?;
        if self.host_binaries.as_ref().is_some_and(|given| {
            !given.is_absolute() || given.to_string_lossy().contains(['{', '}'])
        }) {
            return Err(
                "host binaries directory must be an absolute path without braces".to_owned(),
            );
        }
        Ok(())
    }

    pub(super) fn hash(&self, hasher: &mut blake3::Hasher) -> Result<(), String> {
        hash_field(hasher, "access");
        hash_field(hasher, &self.origin);
        match &self.origin_host_key {
            Some(key) => {
                hash_field(hasher, "origin-host-key");
                hash_field(hasher, key);
            }
            None => hash_field(hasher, "no-origin-host-key"),
        }
        hasher.update(&(self.break_glass.len() as u64).to_le_bytes());
        for entry in &self.break_glass {
            hash_field(hasher, &entry.name);
            hash_field(hasher, &entry.key);
        }
        hasher.update(&(self.binaries.len() as u64).to_le_bytes());
        for set in &self.binaries {
            hash_field(hasher, &set.architecture);
            hash_field(
                hasher,
                set.directory
                    .to_str()
                    .ok_or_else(|| "host binaries directory must be valid Unicode".to_owned())?,
            );
            hash_field(hasher, &set.mm_digest);
            hash_field(hasher, &set.mmd_digest);
            hash_field(hasher, &set.source);
        }
        match &self.host_binaries {
            Some(given) => {
                hash_field(hasher, "host-binaries");
                hash_field(hasher, &given.to_string_lossy());
            }
            None => hash_field(hasher, "release-host-binaries"),
        }
        for (set, flag) in [(self.upgrade, "upgrade"), (self.downgrade, "downgrade")] {
            if set {
                hash_field(hasher, flag);
            }
        }
        if !self.apparmor_profiles.is_empty() {
            hash_field(hasher, "apparmor-profiles");
            hasher.update(&(self.apparmor_profiles.len() as u64).to_le_bytes());
            for profile in &self.apparmor_profiles {
                hash_field(hasher, profile);
            }
        }
        if !self.notices.is_empty() {
            hash_field(hasher, "notices");
            hasher.update(&(self.notices.len() as u64).to_le_bytes());
            for notice in &self.notices {
                hash_field(hasher, notice);
            }
        }
        Ok(())
    }

    /// The setup flags that request this access step again.
    #[must_use]
    pub fn arguments(&self) -> Vec<String> {
        let mut arguments = if self.pushes() {
            vec!["--delivery".to_owned(), "push".to_owned()]
        } else {
            vec!["--origin".to_owned(), shell_word(&self.origin)]
        };
        if let Some(key) = &self.origin_host_key {
            arguments.extend(["--origin-host-key".to_owned(), shell_word(key)]);
        }
        for entry in &self.break_glass {
            arguments.extend([
                "--break-glass".to_owned(),
                shell_word(&format!("{}={}", entry.name, entry.key)),
            ]);
        }
        if let Some(given) = &self.host_binaries {
            arguments.extend([
                "--host-binaries".to_owned(),
                shell_word(&given.display().to_string()),
            ]);
        }
        for (set, flag) in [(self.upgrade, "--upgrade"), (self.downgrade, "--downgrade")] {
            if set {
                arguments.push(flag.to_owned());
            }
        }
        arguments
    }
}

/// What the access step found on the host after installing it. Commit the
/// host key and daemon fingerprint as the repo target; add the deploy key to
/// the git host as a read-only key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BootstrapAccessReport {
    /// The release both `mm` and `mmd` report on the host.
    pub version: String,
    /// The host's SSH ed25519 key, `<type> <base64>`.
    pub host_key: String,
    /// `mmd --print-identity`.
    pub daemon_fingerprint: String,
    /// The public half of the key `mmd` fetches the origin with. Empty on
    /// a host that takes the project's files from mm.
    #[serde(default)]
    pub deploy_key: String,
}

impl BootstrapAccessReport {
    pub fn validate(&self) -> Result<(), String> {
        if self.version.is_empty()
            || self.version.len() > 64
            || !self
                .version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        {
            return Err("host reported an invalid mm version".to_owned());
        }
        if self.host_key.split_whitespace().count() != 2 {
            return Err("host reported an invalid SSH host key".to_owned());
        }
        crate::domain::repo_target::ssh_fingerprint(&self.host_key)
            .map_err(|message| format!("host SSH key: {message}"))?;
        if !is_lower_hex_digest(&self.daemon_fingerprint) {
            return Err("host reported an invalid daemon fingerprint".to_owned());
        }
        if !self.deploy_key.is_empty() {
            parse_public_key(&self.deploy_key)
                .map_err(|message| format!("host deploy key: {message}"))?;
        }
        Ok(())
    }
}

/// What the executor verified, and the access report when the plan installed
/// the access layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapExecution {
    pub capabilities: Vec<BootstrapCapability>,
    pub access: Option<BootstrapAccessReport>,
}

/// Quotes `value` for a POSIX shell when it is not a plain word.
fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b'.' | b'_' | b'-' | b':' | b'@' | b'=' | b'+' | b','
                )
        })
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
