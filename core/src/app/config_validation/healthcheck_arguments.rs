use serde_yaml_ng::Value;

use super::*;
use crate::domain::runtime::RuntimeKind;

/// Podman before 5.8 joins the `test` of a Compose health check it gets
/// through its Docker API and splits it again at whitespace, so an
/// exec-form (`CMD`) argument that contains whitespace, or is empty, never
/// reaches the command whole.
pub(crate) fn validate_healthcheck_arguments(
    document: &ConfigDocument,
    context: &crate::app::ResolvedOperationContext,
    report: &mut ValidationReport,
) {
    if context.runtime != RuntimeKind::PodmanCompose {
        return;
    }
    for (stack_name, stack) in &document.config.stacks {
        let Some(models) = super::shared_volumes::compose_models(document, stack) else {
            continue;
        };
        for (file, model) in stack.files.iter().zip(&models) {
            let Some(services) = model.get("services").and_then(Value::as_mapping) else {
                continue;
            };
            for (name, service) in services {
                let Some(name) = name.as_str() else {
                    continue;
                };
                let Some(arguments) = split_exec_arguments(service) else {
                    continue;
                };
                if !in_environment(document, &context.environment, stack_name, name) {
                    continue;
                }
                let shell = serde_json::to_string(&shell_line(&arguments))
                    .unwrap_or_else(|_| "\"<command>\"".to_owned());
                report.push(
                    Diagnostic::warning(
                        "MM_HEALTHCHECK_ARGUMENTS",
                        format!(
                            "services.{name}.healthcheck.test in {} (stack '{stack_name}') is a `CMD` check with an argument that contains whitespace or is empty; Podman before 5.8 splits such an argument at whitespace, so the check never passes",
                            file.display()
                        ),
                    )
                    .at_path(document.root.join(file))
                    .with_help(format!(
                        "write `test: [\"CMD-SHELL\", {shell}]` (it runs with /bin/sh in the container), or run Podman 5.8 or later"
                    )),
                );
            }
        }
    }
}

/// The arguments after `CMD` when one of them would not survive.
fn split_exec_arguments(service: &Value) -> Option<Vec<String>> {
    let test = service.get("healthcheck")?.get("test")?.as_sequence()?;
    let (kind, arguments) = test.split_first()?;
    if kind.as_str() != Some("CMD") {
        return None;
    }
    let arguments = arguments
        .iter()
        .map(|argument| match argument {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    arguments
        .iter()
        .any(|argument| argument.is_empty() || argument.contains(char::is_whitespace))
        .then_some(arguments)
}

/// A service the Compose file lists and mm.yaml does not is in every
/// environment.
fn in_environment(document: &ConfigDocument, environment: &str, stack: &str, name: &str) -> bool {
    document
        .config
        .modules
        .values()
        .filter_map(|module| module.services.get(name))
        .find(|service| service.stack == stack)
        .is_none_or(|service| service.is_in_environment(environment))
}

fn shell_line(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| {
            if !argument.is_empty()
                && argument
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
            {
                argument.clone()
            } else {
                format!("'{}'", argument.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
