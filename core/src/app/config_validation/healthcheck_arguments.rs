use super::*;
use crate::domain::compose_healthcheck::{HealthcheckArgumentNote, healthcheck_argument_notes};
use crate::domain::runtime::RuntimeKind;

pub(crate) fn validate_healthcheck_arguments(
    document: &ConfigDocument,
    context: &crate::app::ResolvedOperationContext,
    report: &mut ValidationReport,
) {
    if context.runtime != RuntimeKind::PodmanCompose {
        return;
    }
    for diagnostic in healthcheck_argument_diagnostics(document, &context.environment) {
        report.push(diagnostic);
    }
}

pub(crate) fn healthcheck_argument_diagnostics(
    document: &ConfigDocument,
    environment: &str,
) -> Vec<Diagnostic> {
    healthcheck_arguments(document, environment)
        .into_iter()
        .map(|note| {
            Diagnostic::warning("MM_HEALTHCHECK_ARGUMENTS", note.line())
                .at_path(document.root.join(&note.file))
                .with_help(note.help())
        })
        .collect()
}

/// [`healthcheck_arguments`] whose argument is split whatever the
/// interpolations hold, when `selection` runs on Podman.
pub(crate) fn podman_checks(
    document: &ConfigDocument,
    environment: &str,
    selection: &crate::app::ResolvedSelection,
) -> Vec<HealthcheckArgumentNote> {
    if selection.runtime == RuntimeKind::PodmanCompose {
        healthcheck_arguments(document, environment)
            .into_iter()
            .filter(|note| note.interpolated.is_none())
            .collect()
    } else {
        Vec::new()
    }
}

/// The services of `environment` whose `CMD` health check Podman before
/// 5.8 would split. Unreadable Compose files are the stack checks' finding.
pub(crate) fn healthcheck_arguments(
    document: &ConfigDocument,
    environment: &str,
) -> Vec<HealthcheckArgumentNote> {
    let mut notes = Vec::new();
    for (stack_name, stack) in &document.config.stacks {
        let Some(models) = super::shared_volumes::compose_models(document, stack) else {
            continue;
        };
        let files = stack
            .files
            .iter()
            .map(|file| file.display().to_string())
            .zip(models)
            .collect::<Vec<_>>();
        notes.extend(healthcheck_argument_notes(stack_name, &files, |name| {
            !in_environment(document, environment, stack_name, name)
        }));
    }
    notes
}

fn in_environment(document: &ConfigDocument, environment: &str, stack: &str, name: &str) -> bool {
    document
        .config
        .modules
        .values()
        .filter_map(|module| module.services.get(name))
        .find(|service| service.stack == stack)
        .is_none_or(|service| service.is_in_environment(environment))
}
