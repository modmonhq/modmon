//! The AppArmor profile Podman gives a remote host's containers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::Diagnostic;

/// The profile `mm target setup` installs and points Podman at.
pub const MM_CONTAINER_PROFILE: &str = "mm-containers";

/// Podman's own profile under mm's name, for a container that shares a
/// process namespace beyond its own.
pub const MM_SHARED_PID_PROFILE: &str = "mm-containers-shared-pid";

/// Every profile setup installs, in the order it loads them.
pub const CONTAINER_PROFILES: [&str; 2] = [MM_SHARED_PID_PROFILE, MM_CONTAINER_PROFILE];

const PODMAN_PROFILE_PREFIX: &str = "containers-default";

/// The first Podman release whose containers.conf loader merges drop-ins
/// from every directory by file name.
const PODMAN_MERGED_DROP_INS: (u32, u32) = (6, 0);

/// What a host whose kernel runs AppArmor confines Podman's containers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContainerAppArmor {
    /// `apparmor_profile` as containers.conf sets it; empty for Podman's own.
    pub profile: String,
    /// Whether the kernel holds a profile of that name.
    pub loaded: bool,
}

impl ContainerAppArmor {
    /// The containers.conf files rootful Podman `release` reads, in the
    /// order it applies them. `exists` says whether a file is there and
    /// `listing` names a directory's `.conf` files, sorted.
    #[must_use]
    pub fn podman_config_files(
        release: Option<(u32, u32)>,
        exists: &dyn Fn(&str) -> bool,
        listing: &dyn Fn(&str) -> Vec<String>,
    ) -> Vec<String> {
        let mut files = Vec::new();
        if release.is_some_and(|release| release >= PODMAN_MERGED_DROP_INS) {
            files.extend(
                [
                    "/etc/containers/containers.conf",
                    "/usr/share/containers/containers.conf",
                ]
                .into_iter()
                .find(|path| exists(path))
                .map(str::to_owned),
            );
            let mut drop_ins = std::collections::BTreeMap::new();
            for directory in [
                "/usr/share/containers/containers.conf.d",
                "/usr/share/containers/containers.rootful.conf.d",
                "/etc/containers/containers.conf.d",
                "/etc/containers/containers.rootful.conf.d",
            ] {
                for name in listing(directory) {
                    drop_ins.insert(name.clone(), format!("{directory}/{name}"));
                }
            }
            files.extend(drop_ins.into_values());
        } else {
            for path in [
                "/usr/share/containers/containers.conf",
                "/etc/containers/containers.conf",
            ] {
                if exists(path) {
                    files.push(path.to_owned());
                }
            }
            let directory = "/etc/containers/containers.conf.d";
            files.extend(
                listing(directory)
                    .into_iter()
                    .map(|name| format!("{directory}/{name}")),
            );
        }
        files
    }

    /// The profile from containers.conf files in the order Podman applies
    /// them: the last `apparmor_profile` under `[containers]` wins.
    #[must_use]
    pub fn configured_profile<'a>(files: impl IntoIterator<Item = &'a str>) -> String {
        let mut profile = String::new();
        for file in files {
            let mut in_containers = false;
            for line in file.lines() {
                let line = uncommented(line);
                if let Some(table) = line.strip_prefix('[') {
                    in_containers = table.trim_end_matches(']').trim() == "containers";
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if in_containers && key.trim() == "apparmor_profile" {
                    profile = value.trim().trim_matches(['"', '\'']).to_owned();
                }
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

fn uncommented(line: &str) -> &str {
    let mut quote = None;
    for (index, character) in line.char_indices() {
        match (quote, character) {
            (None, '"' | '\'') => quote = Some(character),
            (Some(open), _) if open == character => quote = None,
            (None, '#') => return line[..index].trim(),
            _ => {}
        }
    }
    line.trim()
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
