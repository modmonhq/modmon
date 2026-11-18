//! What `mmd` says about a commit as its own checkout holds it.

use crate::adapters::config::ConfigLoader;
use crate::app::WorkspaceSelection;
use crate::domain::error::MmError;

/// A commit whose configuration does not validate on the host, with the
/// findings: the client's own `mm config validate` can pass while the host's
/// checkout of the commit lacks a file that was never committed.
pub(super) fn invalid_at_commit(selection: &WorkspaceSelection, errors: usize) -> MmError {
    let findings = ConfigLoader
        .load_from(&selection.start)
        .ok()
        .zip(selection.environment.as_deref())
        .map(|(document, environment)| {
            crate::app::config_validation::validate(&document, environment)
                .diagnostics
                .into_iter()
                .filter(|diagnostic| {
                    diagnostic.severity == crate::domain::diagnostic::Severity::Error
                })
                .take(5)
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    MmError::Environment {
        message: format!(
            "the commit's configuration has {errors} error(s) in the host's checkout: {findings}. The host checks out only what is committed, so a file or directory that exists on your machine but is not in the commit is missing there"
        ),
    }
}

/// The release a deployment from the checkout at `start` runs: its `HEAD`
/// when it is in git, with the name and inputs `request` gives.
pub(crate) fn local_release(
    start: &std::path::Path,
    request: crate::domain::release_facts::ReleaseRequest,
) -> crate::domain::release_facts::ReleaseFacts {
    crate::domain::release_facts::ReleaseFacts::new(resolve_commit(start, None).ok(), request)
}

/// The full commit SHA a remote plan deploys: `explicit` resolved in this
/// checkout, or HEAD. The host fetches it from the origin itself, or, on a
/// target with `delivery: push`, takes its files from this checkout.
pub(crate) fn resolve_commit(
    start: &std::path::Path,
    explicit: Option<&str>,
) -> Result<String, MmError> {
    let revision = explicit.unwrap_or("HEAD");
    if crate::domain::remote::is_commit_sha(revision) {
        return Ok(revision.to_owned());
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(start)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{revision}^{{commit}}"))
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|source| MmError::ProcessSpawn {
            program: "git".to_owned(),
            source,
        })?;
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || !crate::domain::remote::is_commit_sha(&sha) {
        return Err(MmError::Environment {
            message: format!(
                "could not resolve '{revision}' to a commit in this checkout; pass --commit with a full SHA"
            ),
        });
    }
    Ok(sha)
}
