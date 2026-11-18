use super::*;

type RouteClaim = (String, String, String, String);

pub(super) fn validate_stacks(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let mut compose_project_owners = BTreeMap::new();
    let mut route_claims = Vec::new();
    for (stack_name, stack) in &context.document.config.stacks {
        validate_stack(
            context,
            stack_name,
            stack,
            &mut compose_project_owners,
            report,
        );
        for route in stack.ingress.routes() {
            let route_name = ingress_route_name(&route.domain, &route.route);
            validate_route_declaration(context, stack_name, route, &route_name, report);
            if route_is_active(route, context.environment) {
                validate_active_route(
                    context,
                    stack_name,
                    route,
                    route_name,
                    &mut route_claims,
                    report,
                );
            }
        }
    }
    validate_host_transport_consistency(context, report);
}

fn validate_stack<'a>(
    context: &ValidationContext<'a>,
    stack_name: &'a str,
    stack: &'a crate::domain::config::StackConfig,
    compose_project_owners: &mut BTreeMap<String, &'a str>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let config_file = document.source("stacks");
    let environment = context.environment;
    validate_name(stack_name, "stack name", config_file, report);
    validate_network_attachments(
        document,
        config_file,
        &stack.networks,
        &format!("stack '{stack_name}'"),
        report,
    );
    if stack.files.is_empty() {
        report.push(
            Diagnostic::error(
                "MM_COMPOSE_FILES_EMPTY",
                format!("stack '{stack_name}' must contain at least one Compose file"),
            )
            .at_path(config_file),
        );
    }
    let mut files = BTreeSet::new();
    for compose_file in &stack.files {
        if !files.insert(compose_file) {
            report.push(
                Diagnostic::error(
                    "MM_COMPOSE_FILE_DUPLICATE",
                    format!(
                        "stack '{stack_name}' lists Compose file '{}' more than once",
                        compose_file.display()
                    ),
                )
                .at_path(config_file),
            );
        }
        validate_relative_path(compose_file, "Compose file", config_file, report);
        let resolved = document.root.join(compose_file);
        validate_no_symlinks(&resolved, "Compose file", report);
        if !resolved.is_file() {
            report.push(
                Diagnostic::error(
                    "MM_COMPOSE_FILE_MISSING",
                    format!(
                        "Compose file for stack '{stack_name}' does not exist: {}",
                        resolved.display()
                    ),
                )
                .at_path(resolved),
            );
        }
    }
    let project_name = compose_project_name(document, environment, stack_name);
    if !is_compose_project_name(&project_name) {
        report.push(
                Diagnostic::error(
                    "MM_COMPOSE_PROJECT_NAME_INVALID",
                    format!(
                        "resolved Compose project name '{project_name}' for stack '{stack_name}' must start with a lowercase letter or digit and contain only lowercase letters, digits, '-' or '_'"
                    ),
                )
                .at_path(config_file),
            );
    }
    if let Some(owner) = compose_project_owners.insert(project_name.clone(), stack_name) {
        report.push(
                Diagnostic::error(
                    "MM_COMPOSE_PROJECT_NAME_DUPLICATE",
                    format!(
                        "stacks '{owner}' and '{stack_name}' resolve to the same Compose project name '{project_name}'"
                    ),
                )
                .at_path(config_file),
            );
    }
}

fn validate_route_declaration(
    context: &ValidationContext<'_>,
    stack_name: &str,
    route: &crate::domain::config::IngressRouteConfig,
    route_name: &str,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let config_file = document.source("stacks");
    let configured_services = &context.indexes.configured_services;
    validate_name(&route.domain, "domain key", config_file, report);
    for allowed in &route.environments {
        if !document.config.environments.contains_key(allowed) {
            report.push(
                        Diagnostic::error(
                            "MM_INGRESS_ENVIRONMENT_UNKNOWN",
                            format!(
                                "route '{route_name}' in stack '{stack_name}' allowlists unknown environment '{allowed}'"
                            ),
                        )
                        .at_path(config_file),
                    );
        }
    }
    if !is_route_path(&route.route) {
        report.push(
            Diagnostic::error(
                "MM_INGRESS_PATH_INVALID",
                format!(
                    "route '{route_name}' in stack '{stack_name}' has invalid path prefix '{}'",
                    route.route
                ),
            )
            .at_path(config_file),
        );
    }
    if route.port == Some(0) {
        report.push(
            Diagnostic::error(
                "MM_INGRESS_PORT_INVALID",
                format!(
                    "route '{route_name}' in stack '{stack_name}' must use a non-zero backend port"
                ),
            )
            .at_path(config_file),
        );
    }
    for problem in
        crate::domain::cluster_ingress::rule_problems(&route.headers, &route.allow, route.priority)
    {
        report.push(
            Diagnostic::error(
                "MM_INGRESS_RULE_INVALID",
                format!("route '{route_name}' in stack '{stack_name}': {problem}"),
            )
            .at_path(config_file),
        );
    }
    if !route.https {
        report.push(
            Diagnostic::warning(
                "MM_INGRESS_HTTP_OPT_OUT",
                format!("route '{route_name}' in stack '{stack_name}' explicitly disables HTTPS"),
            )
            .at_path(config_file)
            .with_help("remove https: false to use trusted HTTPS and automatic HTTP redirect"),
        );
    }
    if let Some(service) = &route.service
        && !configured_services.contains(&(stack_name.to_owned(), service.clone()))
    {
        report.push(
                    Diagnostic::error(
                        "MM_INGRESS_SERVICE_UNKNOWN",
                        format!(
                            "route '{route_name}' in stack '{stack_name}' references unknown service '{service}'"
                        ),
                    )
                    .at_path(config_file),
                );
    }
}

fn validate_active_route(
    context: &ValidationContext<'_>,
    stack_name: &str,
    route: &crate::domain::config::IngressRouteConfig,
    route_name: String,
    route_claims: &mut Vec<RouteClaim>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let config_file = document.source("stacks");
    let environment = context.environment;
    if let Some(name) = &route.service
        && document
            .config
            .modules
            .values()
            .flat_map(|module| &module.services)
            .any(|(service_name, service)| {
                service_name == name
                    && service.stack == stack_name
                    && !service.is_active(environment)
            })
    {
        report.push(Diagnostic::error("MM_INGRESS_SERVICE_EXCLUDED",
            format!("route '{route_name}' selects service '{name}' excluded from environment '{environment}'; align the route and service environments"))
            .at_path(config_file));
        return;
    }
    let Some(host) = effective_domains(&document.config, environment)
        .get(route.domain.as_str())
        .copied()
    else {
        report.push(
            Diagnostic::error(
                "MM_INGRESS_DOMAIN_UNKNOWN",
                format!(
                    "route '{route_name}' in stack '{stack_name}' references unknown domain '{}'",
                    route.domain
                ),
            )
            .at_path(config_file),
        );
        return;
    };
    if !is_route_host(host) {
        report.push(
                    Diagnostic::error(
                        "MM_INGRESS_HOST_INVALID",
                        format!(
                            "route '{route_name}' in stack '{stack_name}' resolves domain '{}' to invalid lowercase hostname '{host}'",
                            route.domain
                        ),
                    )
                    .at_path(config_file),
                );
    }
    if is_reserved_mm_host(host) {
        report.push(
                    Diagnostic::error(
                        "MM_INGRESS_HOST_RESERVED",
                        format!(
                            "route '{route_name}' in stack '{stack_name}' resolves to reserved mm host '{host}'"
                        ),
                    )
                    .at_path(config_file)
                    .with_help("use a project service host outside the mm-managed namespace"),
                );
    }
    for (claimed_host, path, owner_stack, owner_route) in route_claims.iter() {
        if claimed_host == host && path == &route.route {
            report.push(
                        Diagnostic::error(
                            "MM_INGRESS_ROUTE_CONFLICT",
                            format!(
                                "route '{route_name}' in stack '{stack_name}' duplicates route '{owner_route}' in stack '{owner_stack}' at host '{host}'"
                            ),
                        )
                        .at_path(config_file),
                    );
        }
    }
    route_claims.push((
        host.to_owned(),
        route.route.clone(),
        stack_name.to_owned(),
        route_name,
    ));
}

fn validate_host_transport_consistency(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let environment = context.environment;
    let mut host_https = BTreeMap::<&str, (bool, String)>::new();
    for (stack_name, stack) in &document.config.stacks {
        for route in stack.ingress.routes() {
            if !route_is_active(route, environment) {
                continue;
            }
            let Some(host) = effective_domains(&document.config, environment)
                .get(route.domain.as_str())
                .copied()
            else {
                continue;
            };
            let route_name = ingress_route_name(&route.domain, &route.route);
            if let Some((https, owner)) = host_https.get(host)
                && *https != route.https
            {
                report.push(
                    Diagnostic::error(
                        "MM_INGRESS_HTTPS_CONFLICT",
                        format!(
                            "route '{route_name}' in stack '{stack_name}' has https={} but host '{host}' already uses https={https} ({owner})",
                            route.https
                        ),
                    )
                    .at_path(document.source("stacks")),
                );
            } else {
                host_https.entry(host).or_insert((
                    route.https,
                    format!("route '{route_name}' in stack '{stack_name}'"),
                ));
            }
        }
    }
}

pub(crate) fn ingress_route_name(host: &str, path: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [host, path] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    format!("route-{}", &hasher.finalize().to_hex()[..16])
}

pub(super) fn is_route_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && label
                    .bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

pub(super) fn is_route_path(path: &str) -> bool {
    path.starts_with('/')
        && (path == "/" || !path.ends_with('/'))
        && !path.contains("//")
        && !path.split('/').any(|segment| matches!(segment, "." | ".."))
        && path.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~')
        })
}

pub(super) fn validate_network_catalog(document: &ConfigDocument, report: &mut ValidationReport) {
    for name in document.config.networks.keys() {
        validate_name(name, "network name", document.source("networks"), report);
    }
}

pub(super) fn validate_network_attachments(
    document: &ConfigDocument,
    source: &Path,
    attachments: &[String],
    owner: &str,
    report: &mut ValidationReport,
) {
    let mut seen = BTreeSet::new();
    for name in attachments {
        if !document.config.networks.contains_key(name) {
            report.push(
                Diagnostic::error(
                    "MM_NETWORK_UNKNOWN",
                    format!("{owner} attaches unknown network '{name}'"),
                )
                .at_path(source),
            );
        }
        if !seen.insert(name) {
            report.push(
                Diagnostic::error(
                    "MM_NETWORK_DUPLICATE",
                    format!("{owner} attaches network '{name}' more than once"),
                )
                .at_path(source),
            );
        }
    }
}

#[cfg(test)]
mod tests;
