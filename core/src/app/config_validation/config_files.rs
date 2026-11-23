//! Checks that read the project's files the way a deployment plan does.
//!
//! Both ask `app::stacks` the question the compilation of a stack asks, so
//! `mm config validate` refuses what a host would refuse when it plans.

use super::*;
use crate::app::stacks::{compose_environment_refusals, config_file_entries, resolve_config_file};

/// Every `config-files` entry of the environment's services resolves: the
/// source is a file of its module, a template's placeholders are bindings
/// of the environment and none of them is a secret.
pub(super) fn validate_config_files(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    for (_, entry) in config_file_entries(document, context.environment) {
        if let Err(error) = resolve_config_file(&entry) {
            report.push(
                Diagnostic::error("MM_CONFIG_FILE", error.message)
                    .at_path(document.source("modules")),
            );
        }
    }
}

/// The `environments` of each `config-files` and `secret-files` entry are
/// declared environments.
pub(super) fn validate_file_environments(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    for (module, config) in &document.config.modules {
        for (service, config) in &config.services {
            let named = config
                .config_files
                .iter()
                .map(|(path, file)| ("config-files", path, &file.environments))
                .chain(
                    config
                        .secret_files
                        .iter()
                        .map(|(path, file)| ("secret-files", path, &file.environments)),
                );
            for (key, path, environments) in named {
                for name in environments {
                    if document.config.environments.contains_key(name) {
                        continue;
                    }
                    report.push(
                        Diagnostic::error(
                            "MM_SERVICE_ENVIRONMENT_UNKNOWN",
                            format!(
                                "modules.{module}.services.{service}.{key}.\"{path}\".environments names '{name}', which environments: does not declare; declare it or remove it from the list"
                            ),
                        )
                        .at_path(document.source("modules")),
                    );
                }
            }
        }
    }
}

/// No Compose file of the environment's stacks sets `environment` or
/// `env_file` on a service.
pub(super) fn validate_compose_environment(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    for (path, message) in compose_environment_refusals(context.document, context.environment) {
        report.push(Diagnostic::error("MM_COMPOSE_ENVIRONMENT", message).at_path(path));
    }
}
