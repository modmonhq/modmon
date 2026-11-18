use super::*;
use crate::domain::config::ServiceConfig;
use crate::domain::development_units::select_units;

/// In the development environment: a service that runs may not need one
/// that `development:` keeps off, and every name the local overrides turn
/// on must exist.
pub(super) fn validate_development(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    let config = &document.config;
    if config.project.development_environment.as_deref() != Some(context.environment) {
        return;
    }
    let overrides = document.path.with_file_name("mm.overrides.yaml");
    for name in &config.development.enabled {
        if select_units(config, name).is_empty() {
            report.push(
                Diagnostic::warning(
                    "MM_DEVELOPMENT_UNIT",
                    format!(
                        "the local overrides turn on '{name}', which is no module, service or job; run `mm dev disable {name}`"
                    ),
                )
                .at_path(&overrides),
            );
        }
    }
    let services = config
        .modules
        .iter()
        .flat_map(|(module, config)| {
            config.services.iter().map(move |(name, service)| {
                (
                    (service.stack.as_str(), name.as_str()),
                    (module.as_str(), service),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    if !services
        .values()
        .any(|(_, service)| service.off_locally_in(context.environment).is_some())
    {
        return;
    }
    let dependencies = crate::app::development_units::compose_dependencies(document);
    for ((stack, name), needed) in &dependencies {
        let Some((module, service)) = services.get(&(stack.as_str(), name.as_str())) else {
            continue;
        };
        if !service.is_in_environment(context.environment) {
            continue;
        }
        for dependency in needed {
            if let Some((dependency_module, dependency_service)) =
                services.get(&(stack.as_str(), dependency.as_str()))
                && let Some(problem) = dependency_problem(
                    context.environment,
                    (module, name),
                    (dependency_module, dependency, dependency_service),
                )
            {
                report.push(
                    Diagnostic::error("MM_DEVELOPMENT_DEPENDENCY", problem)
                        .at_path(document.source("modules")),
                );
            }
        }
    }
}

fn dependency_problem(
    environment: &str,
    (module, name): (&str, &str),
    (dependency_module, dependency, service): (&str, &str, &ServiceConfig),
) -> Option<String> {
    let off = service.off_locally_in(environment)?;
    let needs = format!(
        "{module}/{name} runs in {environment} and needs {dependency_module}/{dependency} (Compose `depends_on`, `links`, `volumes_from` or a `service:` mode)"
    );
    Some(match off.enable_command() {
        Some(command) => format!(
            "{needs}, which is `development: optional` and off on this machine. Run `{command}`, or set `development: optional` on {module}/{name} too"
        ),
        None => format!(
            "{needs}, which is `development: off` through {}. Remove the dependency, or set `development: optional` there",
            off.unit
        ),
    })
}
