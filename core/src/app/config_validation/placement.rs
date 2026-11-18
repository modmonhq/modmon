use super::*;

/// On a cluster target, every `placement` word of an active service names a
/// node, a group, or `all`. Elsewhere there is one host, so placement has
/// nothing to choose and is not checked.
pub(super) fn validate_placement(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    let environment = context.environment;
    let Some((target_name, target)) = crate::app::cluster_target(document, environment) else {
        return;
    };
    let nodes = target
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    for (module_name, module) in &document.config.modules {
        for (service_name, service) in &module.services {
            let Some(placement) = &service.placement else {
                continue;
            };
            if !service.is_active(environment) {
                continue;
            }
            if let Err(problem) = placement.resolve(&nodes, &target.cluster.groups) {
                report.push(
                    Diagnostic::error(
                        "MM_PLACEMENT",
                        format!(
                            "service '{module_name}.{service_name}' placement {problem} (target '{target_name}', environment '{environment}')"
                        ),
                    )
                    .at_path(document.source("modules"))
                    .with_help(format!(
                        "name a node from targets.{target_name}.nodes, a group from targets.{target_name}.cluster.groups, or `{}`",
                        crate::domain::placement::ALL_NODES
                    )),
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "placement/tests.rs"]
mod tests;
