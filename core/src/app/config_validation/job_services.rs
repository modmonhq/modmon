use std::path::PathBuf;

use serde_yaml_ng::Value;

use super::*;
use crate::domain::config::{ServiceConfig, UpdateOrder};

const MAX_COMPOSE_FILE_BYTES: usize = 16 * 1024 * 1024;

/// A service with `role: job` is the container its jobs run in, and a
/// deployment starts none of it: what only a running container uses is
/// refused, and one that no job runs in is a warning.
pub(super) fn validate_job_services(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    for (module_name, module) in &document.config.modules {
        for (name, service) in &module.services {
            if !service.is_job_only() || !service.is_in_environment(context.environment) {
                continue;
            }
            let entry = format!("modules.{module_name}.services.{name}");
            let has_job = document
                .config
                .jobs
                .values()
                .any(|job| &job.module == module_name && &job.service == name);
            if !has_job {
                report.push(
                    Diagnostic::warning(
                        "MM_JOB_ONLY_SERVICE",
                        format!(
                            "{entry} has `role: job`, so a deployment starts no container of it, and no job runs in it. Add a job with `module: {module_name}` and `service: {name}` under jobs:, or remove `role: job`"
                        ),
                    )
                    .at_path(document.source("modules")),
                );
            }
            for problem in config_problems(context, &entry, name, service) {
                report.push(
                    Diagnostic::error("MM_JOB_ONLY_SERVICE", problem)
                        .at_path(document.source("modules")),
                );
            }
            for (path, problem) in compose_problems(document, &entry, name, service) {
                report.push(Diagnostic::error("MM_JOB_ONLY_SERVICE", problem).at_path(&path));
            }
        }
    }
}

/// The keys of mm.yaml that only mean something for a running container.
fn config_problems(
    context: &ValidationContext<'_>,
    entry: &str,
    name: &str,
    service: &ServiceConfig,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (key, set) in [
        ("replicas", service.replicas.is_some()),
        ("autoscale", service.autoscale.is_some()),
    ] {
        if set {
            problems.push(format!(
                "{entry} has `role: job` and sets `{key}`, but a deployment runs no container of it to count. Remove `{key}`: each job runs in one container, on the node `placement` names"
            ));
        }
    }
    if service.update.as_ref().and_then(|update| update.order) == Some(UpdateOrder::StartFirst) {
        problems.push(format!(
            "{entry} has `role: job` and sets `update.order: start-first`, but it has no running container to replace. Remove `update.order`"
        ));
    }
    let routes = context
        .document
        .config
        .stacks
        .get(&service.stack)
        .map(|stack| stack.ingress.routes())
        .unwrap_or_default();
    for route in routes {
        if route.service.as_deref() == Some(name) && route_is_active(route, context.environment) {
            problems.push(format!(
                "route '{}' in stack '{}' sends requests to {entry}, which has `role: job` and runs no container to answer them. Route to another service, or remove `role: job`",
                ingress_route_name(&route.domain, &route.route),
                service.stack
            ));
        }
    }
    problems
}

/// The Compose keys that only mean something for a running container:
/// the service's own `ports`, and another service's `depends_on` onto it.
fn compose_problems(
    document: &ConfigDocument,
    entry: &str,
    name: &str,
    service: &ServiceConfig,
) -> Vec<(PathBuf, String)> {
    let Some(stack) = document.config.stacks.get(&service.stack) else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    for file in &stack.files {
        let path = document.root.join(file);
        let Ok(Some(bytes)) = read_regular_file(&path, MAX_COMPOSE_FILE_BYTES) else {
            continue;
        };
        let Ok(mut model) = serde_yaml_ng::from_slice::<Value>(&bytes) else {
            continue;
        };
        if model.apply_merge().is_err() {
            continue;
        }
        let Some(services) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (other, definition) in services {
            let Some(other) = other.as_str() else {
                continue;
            };
            if other == name {
                if definition
                    .get("ports")
                    .and_then(Value::as_sequence)
                    .is_some_and(|ports| !ports.is_empty())
                {
                    problems.push((path.clone(), format!(
                        "service '{name}' sets `ports`, and {entry} has `role: job`: no container of it runs to listen on them, and its jobs run without them. Remove `ports`"
                    )));
                }
            } else if depends_on(definition).contains(&name) {
                problems.push((path.clone(), format!(
                    "service '{other}' depends on '{name}' (`depends_on`), and {entry} has `role: job`, so no container of it ever runs for '{other}' to wait for. Remove '{name}' from the `depends_on` of '{other}'"
                )));
            }
        }
    }
    problems
}

fn depends_on(definition: &Value) -> Vec<&str> {
    match definition.get("depends_on") {
        Some(Value::Mapping(map)) => map.keys().filter_map(Value::as_str).collect(),
        Some(Value::Sequence(items)) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}
