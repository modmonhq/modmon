use super::*;
use crate::domain::release_facts::{ReleaseValue, release_value};

/// The release facts the environment's templates read, when a deployment
/// is planned: one the release does not have refuses the plan.
pub(super) fn check_release_keys(context: &EnvironmentContext) -> Result<(), MmError> {
    if context.document.config.release.is_none() {
        return Ok(());
    }
    for variable in context
        .index
        .modules
        .iter()
        .flat_map(|module| &module.variables)
    {
        let Some(key) = &variable.global_key else {
            continue;
        };
        if let Some(ReleaseValue::Missing(reason)) =
            release_value(&context.document.config, &context.environment, key)
        {
            return Err(MmError::ReleaseInput {
                message: format!(
                    "{} line {}: variable '{}' reads {key}, but {reason}",
                    variable.location.path.display(),
                    variable.location.line,
                    variable.name
                ),
            });
        }
    }
    Ok(())
}

/// A release fact is the deployment's, never this machine's.
pub(super) fn refuse_release_override(
    context: &EnvironmentContext,
    key: &str,
) -> Result<(), MmError> {
    if release_value(&context.document.config, &context.environment, key).is_none() {
        return Ok(());
    }
    Err(MmError::Environment {
        message: format!(
            "{key} is a fact of the release each deployment runs, so this machine cannot override it; a deployment takes its inputs from `mm deploy plan --release-input NAME=VALUE`"
        ),
    })
}
