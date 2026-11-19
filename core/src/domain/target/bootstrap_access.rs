//! The optional `mm target setup` step that installs the remote access layer:
//! `mm` and `mmd` from one release, the locked `mm` relay account, the sshd
//! drop-in, the project's host policy enrollment and deploy key, and the
//! `mmd` service.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{BootstrapCapability, hash_field};
use crate::domain::host_policy::{BreakGlassKey, parse_public_key, valid_user_name};

/// At most this many break-glass keys per setup run.
pub const MAX_BREAK_GLASS_KEYS: usize = 16;

/// The CPUs a release builds `mm` and `mmd` for, as `uname -m` names them.
pub const HOST_ARCHITECTURES: [&str; 2] = ["x86_64", "aarch64"];

/// The CPU a Linux executable is built for, from the first bytes of its
/// ELF header; why it is not one setup can install otherwise.
pub fn linux_architecture(header: &[u8]) -> Result<&'static str, String> {
    match header {
        [0x7f, b'E', b'L', b'F', 2, 1, ..] if header.len() >= 20 => {
            match u16::from_le_bytes([header[18], header[19]]) {
                62 => Ok("x86_64"),
                183 => Ok("aarch64"),
                machine => Err(format!(
                    "is a Linux executable for a CPU mm has no release for (ELF machine {machine})"
                )),
            }
        }
        [0x7f, b'E', b'L', b'F', ..] => {
            Err("is not a 64-bit little-endian Linux executable".to_owned())
        }
        [0xcf, 0xfa, 0xed, 0xfe, ..] | [0xca, 0xfe, 0xba, 0xbe, ..] => {
            Err("is built for macOS, not Linux".to_owned())
        }
        _ => Err("is not a Linux executable".to_owned()),
    }
}

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
    /// Local directory holding `mm` and `mmd` built for the host.
    pub binaries: PathBuf,
    /// BLAKE3 of `<binaries>/mm`.
    pub mm_digest: String,
    /// BLAKE3 of `<binaries>/mmd`.
    pub mmd_digest: String,
    /// The CPU `mm` and `mmd` are built for, as `uname -m` names it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub architecture: String,
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
        let binaries = self
            .binaries
            .to_str()
            .ok_or_else(|| "host binaries directory must be valid Unicode".to_owned())?;
        if !self.binaries.is_absolute() || binaries.contains(['{', '}']) {
            return Err(
                "host binaries directory must be an absolute path without braces".to_owned(),
            );
        }
        for digest in [&self.mm_digest, &self.mmd_digest] {
            if !is_lower_hex_digest(digest) {
                return Err("host binary digest must be a lowercase BLAKE3 digest".to_owned());
            }
        }
        if !HOST_ARCHITECTURES.contains(&self.architecture.as_str()) {
            return Err(format!(
                "host binaries must be built for x86_64 or aarch64, not '{}'",
                self.architecture
            ));
        }
        Ok(())
    }

    /// The local file for `name`, `mm` or `mmd`.
    #[must_use]
    pub fn binary(&self, name: &str) -> PathBuf {
        self.binaries.join(name)
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
        hash_field(
            hasher,
            self.binaries
                .to_str()
                .ok_or_else(|| "host binaries directory must be valid Unicode".to_owned())?,
        );
        hash_field(hasher, &self.mm_digest);
        hash_field(hasher, &self.mmd_digest);
        hash_field(hasher, &self.architecture);
        for (set, flag) in [(self.upgrade, "upgrade"), (self.downgrade, "downgrade")] {
            if set {
                hash_field(hasher, flag);
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
        arguments.extend([
            "--host-binaries".to_owned(),
            shell_word(&self.binaries.display().to_string()),
        ]);
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

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
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
