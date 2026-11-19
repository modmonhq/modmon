//! What `mm config validate --for-remote` reports: the guardrail findings
//! of each environment a remote host plans.

use crate::domain::config::ConfigDocument;
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;

const LOCAL_TARGET: &str = "local";

/// The findings of `requested`, or without it of every environment whose
/// target is not `local`.
pub fn remote_guardrail_diagnostics(
    document: &ConfigDocument,
    requested: Option<&str>,
) -> Result<Vec<Diagnostic>, MmError> {
    let environments = match requested {
        Some(environment) => vec![environment.to_owned()],
        None => remote_environments(document),
    };
    let mut diagnostics = Vec::new();
    for environment in &environments {
        for finding in super::project_findings(document, environment)? {
            diagnostics.push(refused(document, environment, &finding).with_help(
                "drop the feature, or ask the host admin to allow it for this service in host.yaml",
            ));
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
        .iter()
        .filter(|(_, environment)| {
            environment
                .target
                .as_deref()
                .unwrap_or(&document.config.target)
                != LOCAL_TARGET
        })
        .map(|(name, _)| name.clone())
        .collect()
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
