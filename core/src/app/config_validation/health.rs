use super::*;
use crate::domain::healing::{INVALID, OnFailure, RestartPolicy};

/// `health:` on a service. Its node restarts containers on a cluster target;
/// on a target of one host (`targets.<t>` with `host:`) a `health:` that
/// restarts is refused with what to write instead, and a local target
/// (`mm dev`) ignores it.
pub(super) fn validate_health(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    let environment = context.environment;
    let one_host = super::scaling::one_host_target(document, environment);
    for (module_name, module) in &document.config.modules {
        for (name, service) in &module.services {
            let Some(health) = service
                .health
                .as_ref()
                .filter(|_| service.is_in_environment(environment))
            else {
                continue;
            };
            let entry = format!("modules.{module_name}.services.{name}");
            let mut problems = RestartPolicy::resolve(Some(health), &entry)
                .err()
                .unwrap_or_default();
            if let Some(target) = one_host
                && health.on_failure != Some(OnFailure::None)
            {
                problems.push(format!(
                    "{entry}.health restarts containers on the nodes of a cluster target, and environment '{environment}' deploys to target '{target}', one host. Write targets.{target} as a cluster: `nodes: [{{name: <node>, host: <address>}}]` (one node is a cluster too), or remove `health`"
                ));
            }
            for problem in problems {
                report
                    .push(Diagnostic::error(INVALID, problem).at_path(document.source("modules")));
            }
        }
    }
}
