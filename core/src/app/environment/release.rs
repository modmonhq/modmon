use super::*;
use crate::domain::release_facts::{INPUT_PREFIX, ReleaseValue, release_source, release_value};

/// The release facts the environment's active templates read, when a
/// deployment is planned: one the release does not have refuses the plan.
/// Every input they read and the plan was not given is named at once; an
/// input no template of this environment reads may be left out.
pub(super) fn check_release_keys(context: &EnvironmentContext) -> Result<(), MmError> {
    let config = &context.document.config;
    if config.release.is_none() {
        return Ok(());
    }
    let mut inputs = BTreeMap::<&str, String>::new();
    let mut other = None;
    for variable in context
        .index
        .modules
        .iter()
        .flat_map(|module| &module.variables)
    {
        let Some(key) = &variable.global_key else {
            continue;
        };
        let Some(ReleaseValue::Missing(reason)) = release_value(config, &context.environment, key)
        else {
            continue;
        };
        let at = format!(
            "{} line {}",
            variable.location.path.display(),
            variable.location.line
        );
        match release_source(config, &context.environment, key)
            .and_then(|source| source.strip_prefix(INPUT_PREFIX))
        {
            Some(input) => {
                inputs.entry(input).or_insert(at);
            }
            None => {
                other.get_or_insert_with(|| {
                    format!(
                        "{at}: variable '{}' reads {key}, but {reason}",
                        variable.name
                    )
                });
            }
        }
    }
    if !inputs.is_empty() {
        return Err(MmError::ReleaseInput {
            message: format!(
                "environment '{}' reads release inputs the plan was not given: {}. Plan with {}",
                context.environment,
                inputs
                    .iter()
                    .map(|(input, at)| format!("{input} ({at})"))
                    .collect::<Vec<_>>()
                    .join(", "),
                inputs
                    .keys()
                    .map(|input| format!("--release-input {input}=VALUE"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        });
    }
    other.map_or(Ok(()), |message| Err(MmError::ReleaseInput { message }))
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
