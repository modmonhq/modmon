use serde_yaml_ng::Value;

use super::*;
use crate::domain::config::{DeployConfig, UpdateOrder};
use crate::domain::runtime::RuntimeKind;

const MAX_COMPOSE_FILE_BYTES: usize = 16 * 1024 * 1024;

/// The keys that say how a deployment rolls out: `after:` on a stack,
/// `deploy.convergence-timeout` on the environment, and `update:` and
/// `drain` on a service. `start-first` runs two containers of a service for a moment,
/// so it is refused where two cannot run side by side. On Podman the two
/// share the service's fixed user ids, so the new one keeps the old one's
/// volumes (measured on Podman 5.4 with Docker Compose).
pub(super) fn validate_rollout(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    let environment = context.environment;
    let config = &document.config;
    if let Err(problem) = crate::domain::rollout::stack_order(&config.stacks) {
        report
            .push(Diagnostic::error("MM_STACK_ORDER", problem).at_path(document.source("stacks")));
    }
    validate_deploy_settings(context, report);
    let podman = config
        .environments
        .get(environment)
        .and_then(|environment| environment.runtime.as_ref())
        .and_then(|runtime| runtime.driver)
        .or(config.runtime.driver)
        == Some(RuntimeKind::PodmanCompose);
    for (module_name, module) in &config.modules {
        for (name, service) in &module.services {
            if let Some(Err(problem)) = service
                .drain
                .as_ref()
                .filter(|_| service.is_in_environment(environment))
                .map(|drain| crate::domain::leaving::drain_of(Some(drain)))
            {
                report.push(
                    Diagnostic::error(
                        "MM_DRAIN",
                        format!(
                            "modules.{module_name}.services.{name}.drain {problem}; write a time such as 30s or 1m"
                        ),
                    )
                    .at_path(document.source("modules")),
                );
            }
            let Some(update) = service
                .update
                .as_ref()
                .filter(|_| service.is_in_environment(environment))
            else {
                continue;
            };
            let entry = format!("modules.{module_name}.services.{name}.update");
            let mut problems = Vec::new();
            if update.parallelism == Some(0) {
                problems.push(format!(
                    "{entry}.parallelism is 0; write how many nodes may be replaced at the same time, at least 1"
                ));
            }
            if update.order == Some(UpdateOrder::StartFirst) && !service.is_job_only() {
                problems.extend(
                    side_by_side_problems(document, &service.stack, name, podman)
                        .into_iter()
                        .map(|problem| {
                            format!(
                                "{entry}.order is start-first, but {problem}. Remove that from the Compose file, or remove `order` to stop the old container first"
                            )
                        }),
                );
            }
            for problem in problems {
                report.push(
                    Diagnostic::error("MM_UPDATE_ORDER", problem)
                        .at_path(document.source("modules")),
                );
            }
        }
    }
}

/// The environment's `deploy:` keys: its two timeouts, its failed-log
/// lines, and its pinned release of mm.
fn validate_deploy_settings(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    let environment = context.environment;
    let config = &document.config;
    let deploy = config
        .environments
        .get(environment)
        .and_then(|environment| environment.deploy.as_ref());
    for (key, checked) in [
        (
            "convergence-timeout",
            DeployConfig::convergence_timeout(deploy),
        ),
        ("progress-timeout", DeployConfig::progress_timeout(deploy)),
    ] {
        if let Err(problem) = checked {
            report.push(
                Diagnostic::error(
                    "MM_DEPLOY_TIMEOUT",
                    format!(
                        "environments.{environment}.deploy.{key} {problem}; write a time such as 90s, 10m, or 1h"
                    ),
                )
                .at_path(document.source("environments")),
            );
        }
    }
    if let Err(problem) = DeployConfig::failure_log_lines(deploy) {
        report.push(
            Diagnostic::error(
                "MM_DEPLOY_FAILURE_LOG_LINES",
                format!("environments.{environment}.deploy.failure-log-lines {problem}"),
            )
            .at_path(document.source("environments")),
        );
    }
    if let Some(Err(problem)) = deploy
        .and_then(|deploy| deploy.mm_version.as_deref())
        .map(crate::domain::version_pin::parse)
    {
        report.push(
            Diagnostic::error(
                "MM_DEPLOY_MM_VERSION",
                format!("environments.{environment}.deploy.mm-version {problem}"),
            )
            .at_path(document.source("environments")),
        );
    }
}

/// Why two containers of `service` cannot run next to each other, from the
/// stack's Compose files.
fn side_by_side_problems(
    document: &ConfigDocument,
    stack: &str,
    service: &str,
    podman: bool,
) -> Vec<String> {
    let Some(stack_config) = document.config.stacks.get(stack) else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    for file in &stack_config.files {
        // Missing and malformed files are reported by the stack checks.
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
        let Some(config) = model
            .get("services")
            .and_then(|services| services.get(service))
        else {
            continue;
        };
        if let Some(name) = config.get("container_name").and_then(Value::as_str) {
            problems.push(format!(
                "service '{service}' sets `container_name: {name}`, and two containers cannot share a name"
            ));
        }
        if let Some(mode) = config
            .get("userns_mode")
            .and_then(Value::as_str)
            .filter(|mode| podman && *mode != "host")
        {
            problems.push(format!(
                "service '{service}' sets `userns_mode: {mode}`, which on Podman gives the new container other user ids than the old one, so it could not change what the old one wrote (without it mm gives the service fixed ids of its own)"
            ));
        }
        if config.get("network_mode").and_then(Value::as_str) == Some("host") {
            problems.push(format!(
                "service '{service}' runs on the host's network, where two containers cannot both hold its ports"
            ));
        }
        if let Some(port) = published_host_port(config) {
            problems.push(format!(
                "service '{service}' publishes host port {port}, which two containers cannot both hold"
            ));
        }
        let scale = config
            .get("scale")
            .or_else(|| {
                config
                    .get("deploy")
                    .and_then(|deploy| deploy.get("replicas"))
            })
            .and_then(Value::as_u64);
        if let Some(scale) = scale.filter(|scale| *scale != 1) {
            problems.push(format!(
                "service '{service}' runs {scale} containers (`scale` or `deploy.replicas`), and start-first replaces exactly one"
            ));
        }
    }
    problems
}

/// The first fixed host port a Compose service publishes. `"8080"` alone
/// lets the engine pick a host port, which never collides.
fn published_host_port(service: &Value) -> Option<String> {
    service
        .get("ports")
        .and_then(Value::as_sequence)?
        .iter()
        .find_map(|port| match port {
            Value::String(text) => {
                let mapping = text.split('/').next().unwrap_or(text);
                let mut parts = mapping.rsplitn(2, ':');
                parts.next()?;
                let host = parts.next()?;
                let host = host.rsplit(':').next().unwrap_or(host);
                (!host.is_empty()).then(|| host.to_owned())
            }
            Value::Mapping(_) => match port.get("published") {
                Some(Value::Number(number)) => Some(number.to_string()),
                Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
                _ => None,
            },
            _ => None,
        })
}

#[cfg(test)]
#[path = "rollout_tests.rs"]
mod tests;
