//! What `mm config validate --for-remote` reports: the guardrail findings
//! of each environment a remote host plans, refused or allowed.

use std::path::Path;

use crate::domain::config::ConfigDocument;
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::host_policy::{EnrolledProject, HostPolicy};

const LOCAL_TARGET: &str = "local";

/// The findings of `requested`, or without it of every environment a host
/// policy applies to: the ones `policy` runs for the project, or without a
/// policy every environment whose target is not `local`. With `policy`,
/// what its `allow` list grants is reported as allowed.
pub fn remote_guardrail_diagnostics(
    document: &ConfigDocument,
    requested: Option<&str>,
    policy: Option<(&Path, &HostPolicy)>,
) -> Result<Vec<Diagnostic>, MmError> {
    let enrolled = policy
        .map(|(path, policy)| {
            enrolled_project(document, path, policy).map(|project| (path, project))
        })
        .transpose()?;
    let environments = match (requested, &enrolled) {
        (Some(environment), _) => vec![environment.to_owned()],
        (None, Some((_, project))) => project
            .environments
            .iter()
            .filter(|environment| document.config.environments.contains_key(*environment))
            .cloned()
            .collect(),
        (None, None) => remote_environments(document),
    };
    let mut diagnostics = Vec::new();
    for environment in &environments {
        for finding in super::project_findings(document, environment)? {
            diagnostics.push(match &enrolled {
                Some((path, project)) if super::allows(&project.allow, &finding) => {
                    Diagnostic::info(
                        "MM_REMOTE_GUARDRAIL_ALLOWED",
                        format!(
                            "the host policy allows this in environment '{environment}': {finding}"
                        ),
                    )
                    .at_path(*path)
                }
                _ if !finding.grantable() => refused(document, environment, &finding)
                    .with_help(
                        "no host exception allows this; join a service of the same stack with `service:<service>`",
                    ),
                Some((path, _)) => refused(document, environment, &finding).with_help(format!(
                    "drop the feature, or allow it for {} under this project's `allow` in {}",
                    finding.subject,
                    path.display()
                )),
                None => refused(document, environment, &finding).with_help(
                    "drop the feature, or ask the host admin to allow it for this service in host.yaml",
                ),
            });
        }
    }
    Ok(diagnostics)
}

/// The environments whose target, their own or the project's, is a remote
/// host or cluster rather than `local`.
fn remote_environments(document: &ConfigDocument) -> Vec<String> {
    document
        .config
        .environments
        .keys()
        .filter(|environment| !on_local_target(document, environment))
        .cloned()
        .collect()
}

/// Whether `environment`'s target, its own or the project's, is `local`:
/// this machine, as far as a check without the machine's targets can tell.
pub(in crate::app::stacks) fn on_local_target(
    document: &ConfigDocument,
    environment: &str,
) -> bool {
    document
        .config
        .environments
        .get(environment)
        .and_then(|environment| environment.target.as_deref())
        .unwrap_or(&document.config.target)
        == LOCAL_TARGET
}

fn refused(
    document: &ConfigDocument,
    environment: &str,
    finding: &super::GuardrailFinding,
) -> Diagnostic {
    Diagnostic::error(
        "MM_REMOTE_GUARDRAIL",
        format!("a remote host refuses this in environment '{environment}': {finding}"),
    )
    .at_path(document.source("stacks"))
}

fn enrolled_project<'a>(
    document: &ConfigDocument,
    path: &Path,
    policy: &'a HostPolicy,
) -> Result<&'a EnrolledProject, MmError> {
    let id = document.config.project.id;
    policy
        .projects
        .get(&id)
        .ok_or_else(|| MmError::ConfigParse {
            path: path.to_path_buf(),
            message: format!(
                "the host policy enrolls no project {} (`project.id` in .config/mm.yaml), so its hosts refuse every request for this project; pass the policy of a host this project deploys to",
                id.0
            ),
        })
}
