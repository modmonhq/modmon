use super::*;

pub(super) fn validate_selected_environment(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let environment = context.environment;
    if document.config.environments.is_empty() {
        report.push(
            Diagnostic::error(
                "MM_ENVIRONMENTS_EMPTY",
                "at least one environment must be configured",
            )
            .at_path(document.source("environments")),
        );
    } else if !document.config.environments.contains_key(environment) {
        report.push(
            Diagnostic::error(
                "MM_ENVIRONMENT_UNKNOWN",
                format!("environment '{environment}' is not configured"),
            )
            .at_path(document.source("environments"))
            .with_help(format!(
                "available environments: {}",
                document
                    .config
                    .environments
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        );
    }

    validate_project_middlewares(document, environment, report);
}

#[cfg(test)]
mod tests;
