use super::*;
use crate::domain::release_facts::{FACT_PREFIX, INPUT_PREFIX, is_release_key};

/// `release-inputs:` and the bindings that meet the keys mm keeps for a
/// release: `MM.` facts and `RELEASE.<NAME>` inputs.
pub(super) fn validate_release_inputs(document: &ConfigDocument, report: &mut ValidationReport) {
    let config = &document.config;
    for (name, input) in &config.release_inputs {
        let mut problems = Vec::new();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            problems.push(format!(
                "release input '{name}' has to be one upper-case word such as BUILD_ID: templates read it as RELEASE.<NAME>"
            ));
        }
        if let Err(problem) = input.matcher() {
            problems.push(format!("release-inputs.{name}.pattern {problem}"));
        }
        for problem in problems {
            report.push(
                Diagnostic::error("MM_RELEASE_INPUT", problem)
                    .at_path(document.source("release-inputs")),
            );
        }
    }
    let scopes = std::iter::once(("bindings".to_owned(), &config.bindings)).chain(
        config.environments.iter().map(|(name, environment)| {
            (
                format!("environments.{name}.bindings"),
                &environment.bindings,
            )
        }),
    );
    for (scope, bindings) in scopes {
        for (key, binding) in bindings {
            if let Some(problem) = binding_problem(config, key, binding) {
                report.push(
                    Diagnostic::error("MM_ENV_KEY_INVALID", format!("{scope}.{key}: {problem}"))
                        .at_path(document.source(if scope == "bindings" {
                            "bindings"
                        } else {
                            "environments"
                        })),
                );
            }
        }
    }
}

fn binding_problem(
    config: &crate::domain::config::MmConfig,
    key: &str,
    binding: &EnvironmentBinding,
) -> Option<String> {
    if key.starts_with(FACT_PREFIX) {
        return Some(format!(
            "keys that start with {FACT_PREFIX} are the facts mm supplies for a release (MM.COMMIT, MM.COMMIT_SHORT, MM.RELEASE, MM.IMAGE.<module>, MM.IMAGE_DIGEST.<module>); give the binding another name"
        ));
    }
    if let Some(name) = key
        .strip_prefix(INPUT_PREFIX)
        .filter(|name| config.release_inputs.contains_key(*name))
    {
        return Some(format!(
            "{key} already reads release input {name}, which `mm deploy plan --release-input {name}=VALUE` supplies; remove the binding or give it another name"
        ));
    }
    let EnvironmentBinding::Reference(reference) = binding else {
        return None;
    };
    (!is_release_key(config, &reference.value_ref)).then(|| {
        format!(
            "value-ref '{}' names no release fact. Use MM.COMMIT, MM.COMMIT_SHORT, MM.RELEASE, MM.IMAGE.<module>, MM.IMAGE_DIGEST.<module>, or RELEASE.<NAME> for an input that release-inputs: declares; a domain needs `cast: url` beside value-ref",
            reference.value_ref
        )
    })
}
