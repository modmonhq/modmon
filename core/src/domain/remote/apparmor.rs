//! The AppArmor profile Podman gives a remote host's containers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::Diagnostic;

/// The profile `mm target setup` installs and points Podman at.
pub const MM_CONTAINER_PROFILE: &str = "mm-containers";

const PODMAN_PROFILE_PREFIX: &str = "containers-default";

/// What a host whose kernel runs AppArmor confines Podman's containers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContainerAppArmor {
    /// `apparmor_profile` as containers.conf sets it; empty for Podman's own.
    pub profile: String,
    /// Whether the kernel holds a profile of that name.
    pub loaded: bool,
}

impl ContainerAppArmor {
    /// The profile from containers.conf files, read in the order Podman reads
    /// them: the last `apparmor_profile` under `[containers]` wins.
    #[must_use]
    pub fn configured_profile<'a>(files: impl IntoIterator<Item = &'a str>) -> String {
        let mut profile = String::new();
        for file in files {
            let mut in_containers = false;
            for line in file.lines() {
                let line = line.trim();
                if line.starts_with('[') {
                    in_containers =
                        line.trim_start_matches('[').trim_end_matches(']').trim() == "containers";
                    continue;
                }
                if !in_containers {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if key.trim() != "apparmor_profile" {
                    continue;
                }
                let value = value.split('#').next().unwrap_or_default().trim();
                profile = value.trim_matches(['"', '\'']).to_owned();
            }
        }
        profile
    }

    /// Whether `profile` appears in the kernel's list of loaded profiles
    /// (`/sys/kernel/security/apparmor/profiles`).
    #[must_use]
    pub fn is_loaded(profiles: &str, profile: &str) -> bool {
        profiles.lines().any(|line| {
            line.strip_prefix(profile)
                .is_some_and(|rest| rest.starts_with(" ("))
        })
    }

    fn podmans_own(&self) -> bool {
        self.profile.is_empty() || self.profile.starts_with(PODMAN_PROFILE_PREFIX)
    }
}

/// What `mm doctor` says about the AppArmor profile of `node`'s containers.
#[must_use]
pub fn apparmor_diagnostic(node: &str, apparmor: &ContainerAppArmor) -> Option<Diagnostic> {
    if apparmor.podmans_own() {
        return Some(
            Diagnostic::warning(
                "MM_PODMAN_APPARMOR",
                format!(
                    "containers on '{node}' run under Podman's default AppArmor profile; where AppArmor stacks it with the OCI runtime's profile (Ubuntu 26.04 does), processes in a container cannot signal each other"
                ),
            )
            .with_help(
                "run `mm target setup plan` and its apply: setup installs /etc/apparmor.d/mm-containers and points Podman at it",
            ),
        );
    }
    if apparmor.profile == MM_CONTAINER_PROFILE && !apparmor.loaded {
        return Some(
            Diagnostic::error(
                "MM_PODMAN_APPARMOR",
                format!(
                    "Podman on '{node}' gives containers the AppArmor profile {MM_CONTAINER_PROFILE}, which the kernel has not loaded, so no container starts"
                ),
            )
            .with_help(
                "on the node, `apparmor_parser --replace /etc/apparmor.d/mm-containers`; `mm target setup plan` and its apply install and load it",
            ),
        );
    }
    None
}
