//! The host paths a host policy lists, as this host resolves them.

use std::path::Path;

use crate::domain::error::MmError;
use crate::domain::host_policy::HostPolicy;

/// Refuses a `host-path` entry that does not resolve on this host, or whose
/// target through symlinks is a path a host may not list.
pub(super) fn check_resolved(path: &Path, policy: &HostPolicy) -> Result<(), MmError> {
    for project in policy.projects.values() {
        for exception in &project.allow {
            for entry in &exception.paths {
                let refused = |problem: String| MmError::ConfigParse {
                    path: path.to_path_buf(),
                    message: format!(
                        "project '{}' allow entry for {}: host path '{entry}' {problem}",
                        project.name, exception.service
                    ),
                };
                let resolved = std::fs::canonicalize(entry.trim()).map_err(|error| {
                    refused(format!(
                        "does not resolve on this host ({error}); list a path that exists"
                    ))
                })?;
                HostPolicy::check_host_path(&resolved.to_string_lossy()).map_err(|problem| {
                    refused(format!(
                        "resolves to {}, which {problem}",
                        resolved.display()
                    ))
                })?;
            }
        }
    }
    Ok(())
}
