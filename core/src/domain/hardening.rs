//! Container hardening and the OCI runtime a service runs under.
//!
//! `hardening: strict` in `mm.yaml` gives every service of a deployment one
//! set of defaults: a read-only root filesystem, no Linux capabilities, no
//! new privileges, the engine's default seccomp profile, and a tmpfs at
//! `/tmp`. A service that cannot take one of them says which and why. The
//! host says which OCI runtimes its engine has (`runtimes:` in
//! `/etc/mm/host.yaml`) and may put every service that is reachable from
//! outside under one of them. What comes out is a [`ServiceSecurity`] per
//! service, which a plan shows.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[path = "hardening/changes.rs"]
mod changes;
pub use changes::{RuntimeChange, runtime_changes};
#[path = "hardening/report.rs"]
mod report;
pub use report::{HostRuntimeStatus, NodeRuntimeReport, RuntimeCheck, runtime_diagnostics};

/// `hardening` at the top of `mm.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Hardening {
    /// Every service of a deployment gets a read-only root filesystem,
    /// drops all Linux capabilities, cannot gain privileges, keeps the
    /// engine's default seccomp profile, and gets a tmpfs at `/tmp`.
    Strict,
}

/// One default of `hardening: strict`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum HardeningKey {
    ReadOnly,
    CapDrop,
    NoNewPrivileges,
    Seccomp,
    Tmpfs,
}

impl HardeningKey {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::CapDrop => "cap-drop",
            Self::NoNewPrivileges => "no-new-privileges",
            Self::Seccomp => "seccomp",
            Self::Tmpfs => "tmpfs",
        }
    }

    /// What the service runs with when it does not take the default.
    #[must_use]
    pub const fn without(self) -> &'static str {
        match self {
            Self::ReadOnly => "the root filesystem stays writable",
            Self::CapDrop => "the engine's default capabilities stay",
            Self::NoNewPrivileges => "processes may gain privileges",
            Self::Seccomp => "the seccomp profile is the Compose file's",
            Self::Tmpfs => "no tmpfs is added at /tmp",
        }
    }
}

/// `hardening` of one service in `mm.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceHardening {
    /// The defaults of `hardening: strict` this service does not take, each
    /// with the reason, which the plan shows.
    #[serde(default, skip_serializing_if = "HardeningExceptions::is_empty")]
    pub except: HardeningExceptions,
}

impl ServiceHardening {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.except.is_empty()
    }
}

/// A present key without a value is an opt-out without a reason, which
/// validation refuses; reading it as absent would harden silently.
fn reason<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(
        Option::<String>::deserialize(deserializer)?.unwrap_or_default(),
    ))
}

/// Each key is a default the service leaves out; its value says why.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HardeningExceptions {
    /// Keep the root filesystem writable. Volumes and bind mounts are
    /// writable either way: prefer a volume for the paths the service
    /// writes.
    #[serde(
        default,
        deserialize_with = "reason",
        skip_serializing_if = "Option::is_none"
    )]
    pub read_only: Option<String>,
    /// Keep the engine's default capabilities. A service that needs a few
    /// keeps them with `cap_add` in its Compose file and needs no
    /// exception.
    #[serde(
        default,
        deserialize_with = "reason",
        skip_serializing_if = "Option::is_none"
    )]
    pub cap_drop: Option<String>,
    /// Let processes gain privileges through setuid binaries.
    #[serde(
        default,
        deserialize_with = "reason",
        skip_serializing_if = "Option::is_none"
    )]
    pub no_new_privileges: Option<String>,
    /// Let the Compose file choose another seccomp profile than the
    /// engine's default.
    #[serde(
        default,
        deserialize_with = "reason",
        skip_serializing_if = "Option::is_none"
    )]
    pub seccomp: Option<String>,
    /// Add no tmpfs at `/tmp`.
    #[serde(
        default,
        deserialize_with = "reason",
        skip_serializing_if = "Option::is_none"
    )]
    pub tmpfs: Option<String>,
}

impl HardeningExceptions {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries().next().is_none()
    }

    /// The reason the service gives for leaving `key` out.
    #[must_use]
    pub fn reason(&self, key: HardeningKey) -> Option<&str> {
        match key {
            HardeningKey::ReadOnly => self.read_only.as_deref(),
            HardeningKey::CapDrop => self.cap_drop.as_deref(),
            HardeningKey::NoNewPrivileges => self.no_new_privileges.as_deref(),
            HardeningKey::Seccomp => self.seccomp.as_deref(),
            HardeningKey::Tmpfs => self.tmpfs.as_deref(),
        }
    }

    /// Every exception with its reason, in a fixed order.
    pub fn entries(&self) -> impl Iterator<Item = (HardeningKey, &str)> {
        [
            HardeningKey::ReadOnly,
            HardeningKey::CapDrop,
            HardeningKey::NoNewPrivileges,
            HardeningKey::Seccomp,
            HardeningKey::Tmpfs,
        ]
        .into_iter()
        .filter_map(|key| self.reason(key).map(|reason| (key, reason)))
    }
}

/// What a host says about OCI runtimes, as `mmd` reads it from its host
/// policy for the plans it makes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostRuntimes {
    /// The runtimes a service may name in `runtime:`.
    pub advertised: BTreeSet<String>,
    /// The runtime every service that publishes a port or takes ingress
    /// traffic runs under.
    pub exposed: Option<String>,
}

impl HostRuntimes {
    /// The advertised runtimes in words: `runsc, crun`, or that there are
    /// none.
    #[must_use]
    pub fn offered(&self) -> String {
        if self.advertised.is_empty() {
            "no runtime besides its engine's default".to_owned()
        } else {
            self.advertised
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}

/// A runtime's name as an engine knows it: `runsc`, `crun`, `kata-runtime`,
/// `io.containerd.runsc.v1`.
pub fn validate_runtime_name(name: &str) -> Result<(), String> {
    let mut bytes = name.bytes();
    let valid = bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && name.len() <= 63
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "'{name}' is not a runtime name: use the name the engine knows it by, such as runsc or crun"
        ))
    }
}

/// One default a service does not take, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HardeningException {
    pub key: HardeningKey,
    pub reason: String,
}

/// The security options one service runs with, after mm's defaults, the
/// service's exceptions, its Compose files, and the host's policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceSecurity {
    pub service: String,
    /// The OCI runtime; absent is the engine's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    /// The host chose the runtime: the service publishes a port or takes
    /// ingress traffic and sets none itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub runtime_from_host: bool,
    /// Whether `hardening: strict` applies to the service.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hardened: bool,
    /// The Compose file sets `privileged: true`: every capability and no
    /// seccomp profile, whatever the other fields say.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub privileged: bool,
    /// The root filesystem is read-only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    /// The capabilities the container keeps; absent is the engine's
    /// default set, empty is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_new_privileges: bool,
    /// The seccomp profile when it is not the engine's default:
    /// `unconfined` or the profile the Compose file names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seccomp: Option<String>,
    /// Where the container has a tmpfs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tmpfs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exceptions: Vec<HardeningException>,
}

impl ServiceSecurity {
    /// The lines a plan shows for the service: its effective options, then
    /// each default it leaves out with the reason.
    #[must_use]
    pub fn describe(&self) -> Vec<String> {
        let mut options = Vec::new();
        if self.hardened {
            options.push(
                if self.read_only {
                    "read-only root"
                } else {
                    "writable root"
                }
                .to_owned(),
            );
            options.push(match self.capabilities.as_deref() {
                _ if self.privileged => "privileged (every capability)".to_owned(),
                None => "the engine's default capabilities".to_owned(),
                Some([]) => "no capabilities".to_owned(),
                Some(kept) => format!("capabilities {} only", kept.join(", ")),
            });
            options.push(
                if self.no_new_privileges {
                    "no new privileges"
                } else {
                    "may gain privileges"
                }
                .to_owned(),
            );
            options.push(match &self.seccomp {
                _ if self.privileged => "no seccomp profile".to_owned(),
                None => "default seccomp".to_owned(),
                Some(profile) => format!("seccomp {profile}"),
            });
            if !self.tmpfs.is_empty() {
                options.push(format!("tmpfs {}", self.tmpfs.join(", ")));
            }
        }
        match (&self.runtime, self.runtime_from_host) {
            (Some(runtime), true) => options.push(format!(
                "runtime {runtime} (this host runs what publishes a port or takes ingress traffic under it)"
            )),
            (Some(runtime), false) => options.push(format!("runtime {runtime}")),
            (None, _) => {}
        }
        let mut lines = vec![format!("{}: {}", self.service, options.join(", "))];
        lines.extend(self.exceptions.iter().map(|exception| {
            format!(
                "{}: not {} ({}): {}",
                self.service,
                exception.key.as_str(),
                exception.key.without(),
                exception.reason
            )
        }));
        lines
    }
}
