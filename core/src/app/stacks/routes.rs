use super::*;

pub(super) fn resolve_ingress_services(
    document: &ConfigDocument,
    environment: &str,
    stack_name: &str,
    stack: &StackConfig,
) -> Result<Vec<ResolvedIngressRoute>, MmError> {
    let configured_services = document
        .config
        .modules
        .values()
        .flat_map(|module| &module.services)
        .filter(|(_, service)| service.stack == stack_name && service.is_active(environment))
        .map(|(service, _)| service.clone())
        .collect::<BTreeSet<_>>();
    let domains = effective_domains(&document.config, environment);
    stack
        .ingress
        .routes()
        .into_iter()
        .filter(|route| route_is_active(route, environment))
        .map(|route| {
            let host = domains
                .get(route.domain.as_str())
                .copied()
                .ok_or_else(|| MmError::Environment {
                    message: format!(
                        "ingress route in stack '{stack_name}' references unknown domain '{}'",
                        route.domain
                    ),
                })?
                .to_owned();
            let path = route.route.clone();
            let name = ingress_route_name(&route.domain, &path);
            let service = match route.service.clone() {
                Some(service) if configured_services.contains(&service) => service,
                Some(service) => {
                    return Err(MmError::Environment {
                        message: format!(
                            "ingress route '{name}' in stack '{stack_name}' selects service '{service}', but that service is not configured for the stack in environment '{environment}' (it may be excluded by services.<name>.environments)"
                        ),
                    });
                }
                None => {
                    let mut services = configured_services.iter();
                    match (services.next(), services.next()) {
                        (Some(service), None) => service.clone(),
                        (None, _) => {
                            return Err(MmError::Environment {
                                message: format!(
                                    "ingress route '{name}' in stack '{stack_name}' omits service, but no configured service belongs to that stack; set ingress.service explicitly after configuring the service under modules"
                                ),
                            });
                        }
                        (Some(_), Some(_)) => {
                            return Err(MmError::Environment {
                                message: format!(
                                    "ingress route '{name}' in stack '{stack_name}' omits service, but the stack has multiple configured services ({}); set ingress.service explicitly",
                                    configured_services.iter().cloned().collect::<Vec<_>>().join(", ")
                                ),
                            });
                        }
                    }
                }
            };
            let balance = route
                .balance
                .unwrap_or_else(|| default_balance(document, environment, stack_name, &service));
            let mut headers = document
                .config
                .environments
                .get(environment)
                .and_then(|config| config.ingress.as_ref())
                .map(|ingress| ingress.headers.clone())
                .unwrap_or_default();
            headers.extend(route.headers.clone());
            Ok(ResolvedIngressRoute {
                name,
                host,
                priority: route.priority.unwrap_or_else(|| route_priority(&path)),
                path_prefix: path,
                service,
                port: route.port,
                tls: route.https,
                balance,
                headers,
                allow: route.allow.clone(),
            })
        })
        .collect()
}

/// A route's balance when it does not set one: the environment's, else
/// `spread` for an autoscaled service, whose added containers on other
/// nodes would otherwise get no request from an ingress node that runs one.
pub(super) fn default_balance(
    document: &ConfigDocument,
    environment: &str,
    stack: &str,
    service: &str,
) -> crate::domain::cluster_ingress::Balance {
    use crate::domain::cluster_ingress::Balance;
    let autoscaled = || {
        document
            .config
            .modules
            .values()
            .flat_map(|module| &module.services)
            .any(|(name, config)| {
                name == service && config.stack == stack && config.autoscale.is_some()
            })
    };
    document
        .config
        .environments
        .get(environment)
        .and_then(|config| config.ingress.as_ref())
        .and_then(|ingress| ingress.balance)
        .unwrap_or_else(|| {
            if autoscaled() {
                Balance::Spread
            } else {
                Balance::LocalFirst
            }
        })
}

pub(super) fn resolve_ingress_ports(
    document: &ConfigDocument,
    environment: &str,
    stack_name: &str,
    routes: &[ResolvedIngressRoute],
    services: &[StackServiceManifest],
    middlewares: &[String],
) -> Result<Vec<StackRouteManifest>, MmError> {
    routes
        .iter()
        .map(|route| {
            let service = services
                .iter()
                .find(|service| service.service == route.service)
                .ok_or_else(|| {
                    port_inference_error(
                        stack_name,
                        &route.service,
                        "the configured service disappeared from the canonical service model",
                    )
                })?;
            let port = match route.port {
                Some(port) => port,
                None => selected_http_port(service)
                    .map_err(|detail| port_inference_error(stack_name, &route.service, &detail))?,
            };
            let router_name = router_name(document, environment, stack_name, &route.name);
            let ownership =
                project_route_ownership(document, environment, stack_name, &route.service);
            Ok(StackRouteManifest {
                name: route.name.clone(),
                host: route.host.clone(),
                path_prefix: route.path_prefix.clone(),
                service: route.service.clone(),
                port,
                tls: route.tls,
                provenance: IngressRouteProvenance::ExplicitConfig,
                certificate_reference: certificate_reference(&route.host, route.tls),
                ownership,
                priority: route.priority,
                backend_alias: format!("mm-ingress-backend-{router_name}"),
                router_name,
                middlewares: middlewares.to_vec(),
                balance: route.balance,
                headers: route.headers.clone(),
                allow: route.allow.clone(),
            })
        })
        .collect()
}

pub(super) fn selected_http_port(service: &StackServiceManifest) -> Result<u16, String> {
    if let Some(problem) = &service.http_evidence.problem {
        return Err(problem.clone());
    }
    match service.http_evidence.candidates.as_slice() {
        [candidate] if candidate.protocol == TransportProtocol::Tcp => Ok(candidate.port),
        [candidate] => Err(format!(
            "the service declares only {}/{}; HTTP requires one TCP target port",
            candidate.port,
            protocol_label(candidate.protocol)
        )),
        [] => Err(
            "the service declares no target port from service.expose or service.ports".to_owned(),
        ),
        candidates
            if candidates
                .iter()
                .all(|candidate| candidate.protocol == TransportProtocol::Tcp) =>
        {
            Err(format!(
                "the service declares multiple target ports ({})",
                candidates
                    .iter()
                    .map(|candidate| candidate.port.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
        candidates => Err(format!(
            "the service has multiple port/protocol candidates ({})",
            candidates
                .iter()
                .map(|candidate| format!(
                    "{}/{}",
                    candidate.port,
                    protocol_label(candidate.protocol)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[derive(Default)]
pub(super) struct EffectiveServicePorts {
    pub(super) declared: bool,
    pub(super) targets: BTreeMap<(u16, TransportProtocol), BTreeSet<HttpEvidenceSource>>,
}

#[cfg(test)]
mod tests;
