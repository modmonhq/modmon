//! The host paths an `allow` entry lets a service bind-mount read-only.

use std::path::{Component, Path};

use super::HostPolicy;

/// Paths that hold the engines' sockets and state, mm's own policy and
/// keys, or people's homes. A mount of one, inside one or above one gives the
/// service root on the host.
const PROTECTED: [&str; 11] = [
    "/run",
    "/var/run",
    "/var/lib/docker",
    "/var/lib/containers",
    "/var/lib/containerd",
    "/etc/mm",
    "/var/lib/mm",
    "/root",
    "/home",
    "/etc/shadow",
    "/etc/ssh",
];

impl HostPolicy {
    pub(super) fn validate_host_paths(&self) -> Result<(), String> {
        for project in self.projects.values() {
            for exception in &project.allow {
                for entry in &exception.paths {
                    Self::check_host_path(entry).map_err(|problem| {
                        format!(
                            "project '{}' allow entry for {}: host path '{entry}' {problem}",
                            project.name, exception.service
                        )
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Whether a host may list `entry` under `paths`, by its text. The host
    /// checks it again once it resolves symlinks.
    pub fn check_host_path(entry: &str) -> Result<(), String> {
        check(Path::new(entry.trim()))
    }
}

fn check(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err("must be an absolute path without `.` or `..`".to_owned());
    }
    if path.starts_with("/proc") && path != Path::new("/proc") {
        return Err(
            "is inside /proc, which reaches other processes' files and the host's root; list /proc itself"
                .to_owned(),
        );
    }
    match PROTECTED
        .iter()
        .map(Path::new)
        .find(|protected| path.starts_with(protected) || protected.starts_with(path))
    {
        Some(protected) => Err(format!(
            "holds or is inside {}, which would give the service root on the host; list only paths without sockets or secrets, such as /proc and /sys",
            protected.display()
        )),
        None => Ok(()),
    }
}
