use super::*;

pub(super) fn compile_services(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    stack_name: &str,
    source_files: &[CompiledComposeSource],
    images: &BTreeMap<String, String>,
    active_mounts: &BTreeMap<String, crate::domain::development_mount::DevelopmentMountConfig>,
) -> Result<Vec<StackServiceManifest>, MmError> {
    let configured = document
        .config
        .modules
        .iter()
        .flat_map(|(module_name, module)| {
            module
                .services
                .iter()
                .filter(|(_, service)| {
                    service.stack == stack_name && service.is_active(environment)
                })
                .map(move |(service_name, service)| (module_name, service_name, service))
        })
        .collect::<Vec<_>>();
    let names = configured
        .iter()
        .map(|(_, service, _)| (*service).clone())
        .collect::<BTreeSet<_>>();
    let mut evidence = infer_compose_target_ports(document, stack_name, &names, source_files)?;
    Ok(configured
        .into_iter()
        .map(|(module, service_name, service)| {
            let http_evidence = if let Some(port) = service.http_port {
                ServiceHttpEvidence {
                    declared_in_compose: evidence
                        .get(service_name)
                        .is_some_and(|evidence| evidence.declared_in_compose),
                    candidates: vec![HttpPortCandidate {
                        port,
                        protocol: TransportProtocol::Tcp,
                        sources: vec![HttpEvidenceSource::GuidedPort],
                    }],
                    problem: None,
                }
            } else {
                evidence.remove(service_name).unwrap_or_default()
            };
            let mail_bindings =
                development_mail_bindings(document, environment, target, module, service);
            StackServiceManifest {
                module: module.clone(),
                service: service_name.clone(),
                image: images.get(module).cloned(),
                role: service.role,
                http_evidence,
                development_mount: active_mounts
                    .get(module)
                    .and_then(|mount| mount.services.contains(service_name).then(|| mount.clone())),
                mail_bindings,
            }
        })
        .collect())
}

pub(super) fn development_mail_bindings(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    module: &str,
    service: &ServiceConfig,
) -> Vec<crate::domain::development_mail::MailBindingManifest> {
    use crate::domain::development_mail::{MailBindingManifest, MailBindingProvenance};

    if !service.is_active(environment)
        || document.config.project.development_environment.as_deref() != Some(environment)
        || target.definition.transport != crate::domain::target::TargetTransport::Local
        || document.config.development_mail.enabled == Some(false)
        || document
            .config
            .modules
            .get(module)
            .is_some_and(|module| module.development_mail == Some(false))
        || service.development_mail == Some(false)
        || service.role == Some(crate::domain::config::ServiceRole::Database)
    {
        return Vec::new();
    }
    crate::domain::development_mail::configured_bindings(&document.config.development_mail)
        .into_iter()
        .map(|(key, (value, configured))| MailBindingManifest {
            provenance: if service.env.contains_key(&key) {
                MailBindingProvenance::PreservedProjectValue
            } else if configured {
                MailBindingProvenance::InjectedConfigured
            } else {
                MailBindingProvenance::InjectedDefault
            },
            key,
            value,
        })
        .collect()
}

pub(super) fn automatic_local_routes(
    document: &ConfigDocument,
    environment: &str,
    stack_name: &str,
    project_slug: &ProjectSlug,
    services: &[StackServiceManifest],
    explicit: &[ResolvedIngressRoute],
    middlewares: &[String],
) -> Result<Vec<StackRouteManifest>, MmError> {
    if document.config.project.development_environment.as_deref() != Some(environment)
        || document.config.ingress.automatic_local == Some(false)
        || document.config.environments[environment].automatic_local_ingress == Some(false)
    {
        return Ok(Vec::new());
    }
    let explicit_services = explicit
        .iter()
        .map(|route| route.service.as_str())
        .collect::<BTreeSet<_>>();
    let explicit_identities = explicit
        .iter()
        .map(|route| (route.host.as_str(), route.path_prefix.as_str()))
        .collect::<BTreeSet<_>>();
    let mut routes = Vec::new();
    for service in services {
        let config = service_config(document, &service.module, &service.service)?;
        if explicit_services.contains(service.service.as_str())
            || config.automatic_local_ingress == Some(false)
            || config
                .role
                .is_some_and(|role| role.excludes_automatic_ingress())
            || !service.http_evidence.declared_in_compose
        {
            continue;
        }
        let Ok(port) = selected_http_port(service) else {
            continue;
        };
        let service_slug = ServiceSlug::normalize(&service.service)
            .map_err(|message| MmError::Environment { message })?;
        let host = format!("{}.{}.test", service_slug.as_str(), project_slug.as_str());
        if explicit_identities.contains(&(host.as_str(), "/")) {
            continue;
        }
        let tls = config.local_https.unwrap_or(true);
        let name = ingress_route_name(&host, "/");
        let router_name = router_name(document, environment, stack_name, &name);
        routes.push(StackRouteManifest {
            name,
            host: host.clone(),
            path_prefix: "/".to_owned(),
            service: service.service.clone(),
            port,
            tls,
            provenance: IngressRouteProvenance::AutomaticConvention,
            certificate_reference: certificate_reference(&host, tls),
            ownership: project_route_ownership(document, environment, stack_name, &service.service),
            priority: route_priority("/"),
            backend_alias: format!("mm-ingress-backend-{router_name}"),
            router_name,
            middlewares: middlewares.to_vec(),
            balance: super::routes::default_balance(
                document,
                environment,
                stack_name,
                &service.service,
            ),
            headers: BTreeMap::new(),
            allow: Vec::new(),
        });
    }
    Ok(routes)
}

pub(super) fn service_config<'a>(
    document: &'a ConfigDocument,
    module: &str,
    service: &str,
) -> Result<&'a ServiceConfig, MmError> {
    document
        .config
        .modules
        .get(module)
        .and_then(|module| module.services.get(service))
        .ok_or_else(|| MmError::Environment {
            message: format!(
                "canonical service '{module}/{service}' disappeared during ingress compilation"
            ),
        })
}

pub(super) fn project_route_ownership(
    document: &ConfigDocument,
    environment: &str,
    stack: &str,
    service: &str,
) -> IngressRouteOwnership {
    IngressRouteOwnership::Project {
        project_id: document.config.project.id,
        environment: environment.to_owned(),
        stack: stack.to_owned(),
        service: service.to_owned(),
    }
}

pub(super) fn certificate_reference(host: &str, tls: bool) -> IngressCertificateReference {
    if !tls {
        IngressCertificateReference::None
    } else if crate::domain::ca::is_local_ca_host(host) {
        IngressCertificateReference::LocalCa {
            host: host.to_owned(),
        }
    } else {
        IngressCertificateReference::Acme {
            resolver: TRAEFIK_CERTIFICATE_RESOLVER.to_owned(),
        }
    }
}

pub(super) fn ingress_statuses(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    stacks: &[PreparedStackArtifact],
) -> Result<Vec<IngressServiceStatus>, MmError> {
    ingress_statuses_from_manifests(
        document,
        environment,
        target,
        &stacks
            .iter()
            .map(|stack| &stack.manifest)
            .collect::<Vec<_>>(),
    )
}

pub(super) fn ingress_statuses_from_manifests(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    stacks: &[&StackArtifactManifest],
) -> Result<Vec<IngressServiceStatus>, MmError> {
    let project_slug = Some(canonical_project_slug(document)?);
    let all_routes = stacks
        .iter()
        .flat_map(|stack| stack.routes.iter())
        .collect::<Vec<_>>();
    let mut statuses = Vec::new();
    for stack in stacks {
        for service in &stack.services {
            let slug = Some(
                ServiceSlug::normalize(&service.service)
                    .map_err(|message| MmError::Environment { message })?,
            );
            let matching = stack
                .routes
                .iter()
                .filter(|route| route.service == service.service)
                .collect::<Vec<_>>();
            if !matching.is_empty() {
                for route in matching {
                    statuses.push(IngressServiceStatus {
                        module: service.module.clone(),
                        stack: stack.stack.clone(),
                        service: service.service.clone(),
                        service_slug: slug.as_ref().map(ToString::to_string),
                        url: Some(route_url(&route.host, &route.path_prefix, route.tls)),
                        selected_port: Some(route.port),
                        tls: Some(route.tls),
                        provenance: Some(route.provenance),
                        certificate_reference: Some(route.certificate_reference.clone()),
                        ownership: Some(route.ownership.clone()),
                        reason: None,
                        warning: (!route.tls).then(|| {
                            "HTTP-only route explicitly opts out of trusted local HTTPS".to_owned()
                        }),
                    });
                }
                continue;
            }
            let config = service_config(document, &service.module, &service.service)?;
            let generated_host =
                slug.as_ref()
                    .zip(project_slug.as_ref())
                    .map(|(service_slug, project_slug)| {
                        format!("{}.{}.test", service_slug.as_str(), project_slug.as_str())
                    });
            statuses.push(IngressServiceStatus {
                module: service.module.clone(),
                stack: stack.stack.clone(),
                service: service.service.clone(),
                service_slug: slug.as_ref().map(ToString::to_string),
                url: None,
                selected_port: None,
                tls: None,
                provenance: None,
                certificate_reference: None,
                ownership: None,
                reason: Some(not_routed_reason(
                    document,
                    environment,
                    target,
                    config,
                    service,
                    generated_host.as_deref(),
                    &all_routes,
                )),
                warning: None,
            });
        }
    }
    statuses.sort_by(|left, right| {
        left.stack
            .cmp(&right.stack)
            .then_with(|| left.service.cmp(&right.service))
            .then_with(|| left.url.cmp(&right.url))
    });
    Ok(statuses)
}

pub(super) fn not_routed_reason(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    config: &ServiceConfig,
    service: &StackServiceManifest,
    generated_host: Option<&str>,
    routes: &[&StackRouteManifest],
) -> String {
    if document.config.project.development_environment.as_deref() != Some(environment) {
        return format!(
            "no route of stacks.{}.ingress names it in '{environment}'; mm adds routes by itself only in the development environment ({})",
            config.stack,
            document
                .config
                .project
                .development_environment
                .as_deref()
                .unwrap_or("not configured")
        );
    }
    if target.definition.transport != crate::domain::target::TargetTransport::Local {
        return "resolved target transport is remote; automatic production domains are never inferred".to_owned();
    }
    if document.config.ingress.automatic_local == Some(false) {
        return "automatic local ingress is disabled at project scope".to_owned();
    }
    if document.config.environments[environment].automatic_local_ingress == Some(false) {
        return "automatic local ingress is disabled for the selected environment".to_owned();
    }
    if config.automatic_local_ingress == Some(false) {
        return "automatic local ingress is disabled for this service".to_owned();
    }
    if let Some(role) = config.role.filter(|role| role.excludes_automatic_ingress()) {
        return format!("service role '{role:?}' is not HTTP-routable").to_ascii_lowercase();
    }
    if let Some(generated_host) = generated_host
        && routes
            .iter()
            .any(|route| route.host == generated_host && route.path_prefix == "/")
    {
        return format!(
            "generated host '{generated_host}' is already selected by an explicit route, which takes precedence"
        );
    }
    if generated_host.is_none() {
        return "project or service name cannot form a safe local ingress hostname".to_owned();
    }
    if let Some(problem) = &service.http_evidence.problem {
        return format!("HTTP port evidence is invalid: {problem}");
    }
    if !service.http_evidence.declared_in_compose {
        return "service is absent from the ordered Compose model".to_owned();
    }
    match service.http_evidence.candidates.as_slice() {
        [] => "service has no HTTP target-port evidence; configure role/http-port or one TCP expose/port".to_owned(),
        [candidate] if candidate.protocol != TransportProtocol::Tcp => format!(
            "only {}/{} is declared; HTTP requires one TCP target port",
            candidate.port,
            protocol_label(candidate.protocol)
        ),
        candidates => format!(
            "HTTP target port is ambiguous ({}); persist services.<name>.http-port to select one",
            candidates
                .iter()
                .map(|candidate| format!("{}/{}", candidate.port, protocol_label(candidate.protocol)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub(super) fn route_url(host: &str, path: &str, tls: bool) -> String {
    format!(
        "{}://{}{}",
        if tls { "https" } else { "http" },
        host,
        if path == "/" { "" } else { path }
    )
}

#[derive(Debug)]
pub(super) struct ResolvedIngressRoute {
    pub(super) name: String,
    pub(super) host: String,
    pub(super) path_prefix: String,
    pub(super) service: String,
    pub(super) port: Option<u16>,
    pub(super) tls: bool,
    pub(super) priority: i64,
    pub(super) balance: crate::domain::cluster_ingress::Balance,
    pub(super) headers: BTreeMap<String, String>,
    pub(super) allow: Vec<crate::domain::cluster_ingress::AllowRule>,
}

#[cfg(test)]
mod tests;
