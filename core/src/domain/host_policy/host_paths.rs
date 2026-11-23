//! The host paths an `allow` entry lets a service bind-mount read-only.

use std::path::{Component, Path};

use super::HostPolicy;

/// Paths that hold the engines' sockets and state, mm's own policy and
/// keys, or people's homes. A mount of one, inside one or above one gives the
/// service root on the host.
const PROTECTED: [&str; 9] = [
    "/run",
    "/var/run",
    "/var/lib/docker",
    "/var/lib/containers",
    "/etc/mm",
    "/var/lib/mm",
    "/root",
    "/home",
    "/etc/shadow",
];

impl HostPolicy {
    pub(super) fn validate_host_paths(&self) -> Result<(), String> {
        for project in self.projects.values() {
            for exception in &project.allow {
                for entry in &exception.paths {
                    check(entry).map_err(|problem| {
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
}

fn check(entry: &str) -> Result<(), String> {
    let path = Path::new(entry.trim());
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err("must be an absolute path without `.` or `..`".to_owned());
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
