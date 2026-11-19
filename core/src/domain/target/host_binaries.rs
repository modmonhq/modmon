//! The `mm` and `mmd` setup installs on a host, by the CPU they are built for.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The CPUs a release builds `mm` and `mmd` for, as `uname -m` names them.
pub const HOST_ARCHITECTURES: [&str; 2] = ["x86_64", "aarch64"];

/// `mm` and `mmd` for one CPU, as a plan reviews them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HostBinaries {
    /// `x86_64` or `aarch64`.
    pub architecture: String,
    /// The local directory that holds them.
    pub directory: PathBuf,
    /// BLAKE3 of `<directory>/mm`.
    pub mm_digest: String,
    /// BLAKE3 of `<directory>/mmd`.
    pub mmd_digest: String,
    /// Where they come from: `--host-binaries`, this mm's directory, or
    /// this mm's release.
    pub source: String,
}

/// What a host said before setup changes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostFacts {
    /// The release its `mmd` reports; `None` without one that does.
    pub release: Option<crate::domain::cluster_setup::HostRelease>,
    /// Its CPU, as [`host_cpu`] names it.
    pub cpu: Option<String>,
}

impl HostBinaries {
    #[must_use]
    pub fn binary(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }

    pub fn validate(&self) -> Result<(), String> {
        let directory = self
            .directory
            .to_str()
            .ok_or_else(|| "host binaries directory must be valid Unicode".to_owned())?;
        if !self.directory.is_absolute() || directory.contains(['{', '}']) {
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
        if self.source.contains(['{', '}']) {
            return Err("host binaries source must not contain braces".to_owned());
        }
        Ok(())
    }
}

/// One or two sets of host binaries, each for another CPU, in CPU order.
pub fn validate_host_binaries(sets: &[HostBinaries]) -> Result<(), String> {
    if sets.is_empty() || sets.len() > HOST_ARCHITECTURES.len() {
        return Err("setup needs mm and mmd for one or two CPUs".to_owned());
    }
    for set in sets {
        set.validate()?;
    }
    if sets
        .windows(2)
        .any(|pair| pair[0].architecture >= pair[1].architecture)
    {
        return Err("host binaries must be listed once per CPU, in order".to_owned());
    }
    Ok(())
}

/// The sets a host of `cpu` may get: the one for its CPU, or every set when
/// its CPU is not known yet and the play picks by the host's facts.
#[must_use]
pub fn binaries_for<'a>(sets: &'a [HostBinaries], cpu: Option<&str>) -> Vec<&'a HostBinaries> {
    match cpu {
        Some(cpu) => sets.iter().filter(|set| set.architecture == cpu).collect(),
        None => sets.iter().collect(),
    }
}

/// The CPU `uname -m` names, as setup names it; `None` for a CPU mm has
/// no release for.
#[must_use]
pub fn host_cpu(machine: &str) -> Option<&'static str> {
    match machine.trim() {
        "x86_64" | "amd64" => Some("x86_64"),
        "aarch64" | "arm64" => Some("aarch64"),
        _ => None,
    }
}

/// The `uname -m` line the facts script prints first, and the JSON after it.
#[must_use]
pub fn split_machine(output: &str) -> (Option<&str>, &str) {
    let trimmed = output.trim_start();
    if trimmed.starts_with('{') {
        return (None, trimmed);
    }
    match trimmed.split_once('\n') {
        Some((machine, rest)) => (Some(machine.trim()), rest),
        None => (Some(trimmed.trim()), ""),
    }
}

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

pub(super) fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
