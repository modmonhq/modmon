use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::domain::ca::is_local_ca_host;
use crate::domain::ingress::{CanonicalMiddleware, IngressRouteClaim};
use crate::domain::target::TargetId;
use crate::domain::target_ingress::{
    TRAEFIK_CERTIFICATE_RESOLVER, TRAEFIK_DYNAMIC_CONFIG_DIRECTORY,
};
use crate::domain::traefik_dynamic::{
    RenderedTraefikDynamicConfig, TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE, TRAEFIK_VERIFY_ENTRYPOINT,
    TRAEFIK_VERIFY_HEADER, TRAEFIK_VERIFY_HEADER_VALUE, TRAEFIK_VERIFY_MIDDLEWARE,
    TraefikDynamicConfigError, TraefikDynamicConfigManifest,
};

const HTTP_ENTRYPOINT: &str = "web";
const HTTPS_ENTRYPOINT: &str = "websecure";
const GATE_INGRESS_AUTH_MIDDLEWARE: &str = "mm-gate-ingress-auth@file";
const GATE_INGRESS_CLEAR_MIDDLEWARE: &str = "mm-gate-ingress-clear@file";

struct RenderedClaims {
    routers: BTreeMap<String, TraefikRouter>,
    services: BTreeMap<String, TraefikService>,
}

pub fn render_traefik_dynamic_routes(
    target_id: TargetId,
    claims: &[IngressRouteClaim],
    project_middlewares: &BTreeMap<String, CanonicalMiddleware>,
    local_ca_hosts: &BTreeSet<String>,
) -> Result<RenderedTraefikDynamicConfig, TraefikDynamicConfigError> {
    let mut claims = claims.iter().collect::<Vec<_>>();
    claims.sort_by(|left, right| canonical_claim_order(left, right));

    for claim in &claims {
        validate_claim(target_id, claim)?;
    }
    reject_overlapping_claims(&claims)?;
    reject_router_name_collisions(&claims, project_middlewares)?;

    let rendered_claims = render_claims(&claims, project_middlewares, local_ca_hosts)?;
    let middlewares = render_middlewares(&claims, project_middlewares)?;
    render_dynamic_config(
        target_id,
        &claims,
        rendered_claims.routers,
        rendered_claims.services,
        middlewares,
        local_ca_hosts,
    )
}

fn render_claims(
    claims: &[&IngressRouteClaim],
    project_middlewares: &BTreeMap<String, CanonicalMiddleware>,
    local_ca_hosts: &BTreeSet<String>,
) -> Result<RenderedClaims, TraefikDynamicConfigError> {
    let mut routers = BTreeMap::new();
    let mut services = BTreeMap::new();
    for claim in claims {
        let rule = route_rule(&claim.host, &claim.path_prefix);
        let backend_alias = &claim.backend_alias;
        let port = claim.port;
        let (entrypoint, tls) = if claim.tls {
            (
                HTTPS_ENTRYPOINT,
                Some(route_tls(&claim.host, local_ca_hosts)?),
            )
        } else {
            (HTTP_ENTRYPOINT, None)
        };
        for name in &claim.middlewares {
            if !project_middlewares.contains_key(name) {
                return Err(TraefikDynamicConfigError::UnknownMiddleware {
                    router_name: claim.router_name.clone(),
                    middleware: name.clone(),
                });
            }
        }
        let has_gate = claim.middlewares.iter().any(|name| {
            matches!(
                project_middlewares.get(name),
                Some(CanonicalMiddleware::ForwardAuth { address, .. })
                    if address.contains("/_mm/gate/forward/")
            )
        });
        let mut route_middlewares = claim.middlewares.clone();
        if has_gate {
            route_middlewares.insert(0, GATE_INGRESS_AUTH_MIDDLEWARE.to_owned());
            route_middlewares.push(GATE_INGRESS_CLEAR_MIDDLEWARE.to_owned());
        }
        routers.insert(
            claim.router_name.clone(),
            TraefikRouter {
                entry_points: vec![entrypoint],
                rule: rule.clone(),
                service: claim.router_name.clone(),
                middlewares: route_middlewares,
                priority: claim.priority,
                tls: tls.clone(),
            },
        );
        if claim.tls {
            routers.insert(
                redirect_router_name(&claim.router_name),
                TraefikRouter {
                    entry_points: vec![HTTP_ENTRYPOINT],
                    rule: rule.clone(),
                    service: claim.router_name.clone(),
                    middlewares: vec![TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE.to_owned()],
                    priority: claim.priority,
                    tls: None,
                },
            );
        }
        routers.insert(
            verification_router_name(&claim.router_name),
            TraefikRouter {
                entry_points: vec![TRAEFIK_VERIFY_ENTRYPOINT],
                rule,
                service: claim.router_name.clone(),
                middlewares: vec![TRAEFIK_VERIFY_MIDDLEWARE.to_owned()],
                priority: claim.priority,
                tls: None,
            },
        );
        services.insert(
            claim.router_name.clone(),
            TraefikService {
                load_balancer: TraefikLoadBalancer {
                    servers: vec![TraefikServer {
                        url: format!("http://{backend_alias}:{port}"),
                    }],
                },
            },
        );
        if has_gate {
            add_gate_route(claim, entrypoint, tls, &mut routers, &mut services);
        }
    }
    Ok(RenderedClaims { routers, services })
}

fn add_gate_route(
    claim: &IngressRouteClaim,
    entrypoint: &'static str,
    tls: Option<TraefikTls>,
    routers: &mut BTreeMap<String, TraefikRouter>,
    services: &mut BTreeMap<String, TraefikService>,
) {
    routers.insert(
        format!("{}-gate-login", claim.router_name),
        TraefikRouter {
            entry_points: vec![entrypoint],
            rule: format!("Host(`{}`) && PathPrefix(`/_mm/gate`)", claim.host),
            service: "mm-gate-control".to_owned(),
            middlewares: vec![GATE_INGRESS_AUTH_MIDDLEWARE.to_owned()],
            priority: claim.priority.saturating_add(1_000_000),
            tls,
        },
    );
    services
        .entry("mm-gate-control".to_owned())
        .or_insert_with(|| TraefikService {
            load_balancer: TraefikLoadBalancer {
                servers: vec![TraefikServer {
                    url: "http://mm-control-host:42831".to_owned(),
                }],
            },
        });
}

fn render_middlewares(
    claims: &[&IngressRouteClaim],
    project_middlewares: &BTreeMap<String, CanonicalMiddleware>,
) -> Result<BTreeMap<String, TraefikMiddleware>, TraefikDynamicConfigError> {
    let mut middlewares = BTreeMap::new();
    let has_tls = claims.iter().any(|claim| claim.tls);
    if has_tls {
        middlewares.insert(
            TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE.to_owned(),
            TraefikMiddleware {
                redirect_scheme: Some(TraefikRedirectScheme {
                    scheme: "https",
                    permanent: true,
                }),
                headers: None,
                replace_path_regex: None,
                forward_auth: None,
            },
        );
    }
    if !claims.is_empty() {
        middlewares.insert(
            TRAEFIK_VERIFY_MIDDLEWARE.to_owned(),
            TraefikMiddleware {
                redirect_scheme: None,
                headers: Some(TraefikHeaders {
                    custom_response_headers: [(TRAEFIK_VERIFY_HEADER, TRAEFIK_VERIFY_HEADER_VALUE)]
                        .into_iter()
                        .collect(),
                }),
                replace_path_regex: None,
                forward_auth: None,
            },
        );
    }
    for (name, definition) in project_middlewares {
        if !is_resource_name(name) {
            return Err(TraefikDynamicConfigError::InvalidMiddlewareName {
                middleware: name.clone(),
            });
        }
        let rendered = match definition {
            CanonicalMiddleware::ReplacePathRegex { regex, replacement } => TraefikMiddleware {
                redirect_scheme: None,
                headers: None,
                replace_path_regex: Some(TraefikReplacePathRegex {
                    regex: regex.clone(),
                    replacement: replacement.clone(),
                }),
                forward_auth: None,
            },
            CanonicalMiddleware::ForwardAuth {
                address,
                trust_forward_header,
                auth_response_headers,
                add_auth_cookies_to_response,
            } => TraefikMiddleware {
                redirect_scheme: None,
                headers: None,
                replace_path_regex: None,
                forward_auth: Some(TraefikForwardAuth {
                    address: address.clone(),
                    trust_forward_header: *trust_forward_header,
                    auth_response_headers: auth_response_headers.clone(),
                    add_auth_cookies_to_response: add_auth_cookies_to_response.clone(),
                }),
            },
        };
        if let Some(existing) = middlewares.get(name)
            && existing != &rendered
        {
            return Err(TraefikDynamicConfigError::ConflictingMiddleware {
                middleware: name.clone(),
            });
        }
        middlewares.insert(name.clone(), rendered);
    }
    Ok(middlewares)
}

fn render_dynamic_config(
    target_id: TargetId,
    claims: &[&IngressRouteClaim],
    routers: BTreeMap<String, TraefikRouter>,
    services: BTreeMap<String, TraefikService>,
    middlewares: BTreeMap<String, TraefikMiddleware>,
    local_ca_hosts: &BTreeSet<String>,
) -> Result<RenderedTraefikDynamicConfig, TraefikDynamicConfigError> {
    let router_count = routers.len();
    let service_count = services.len();
    let tls_certificates = local_ca_hosts
        .iter()
        .filter(|host| claims.iter().any(|claim| claim.tls && claim.host == **host))
        .map(|host| TraefikTlsCertificate {
            cert_file: format!("{TRAEFIK_DYNAMIC_CONFIG_DIRECTORY}/{host}.crt"),
            key_file: format!("{TRAEFIK_DYNAMIC_CONFIG_DIRECTORY}/{host}.key"),
        })
        .collect::<Vec<_>>();
    let has_http = !routers.is_empty() || !services.is_empty() || !middlewares.is_empty();
    let bytes = serde_yaml_ng::to_string(&TraefikDynamicConfig {
        http: has_http.then_some(TraefikHttp {
            routers,
            services,
            middlewares,
        }),
        tls: (!tls_certificates.is_empty()).then_some(TraefikTlsStore {
            certificates: tls_certificates,
        }),
    })
    .map(String::into_bytes)
    .map_err(|error| TraefikDynamicConfigError::Serialization {
        message: error.to_string(),
    })?;
    let digest = blake3::hash(&bytes).to_hex().to_string();

    Ok(RenderedTraefikDynamicConfig {
        manifest: TraefikDynamicConfigManifest {
            target_id,
            claim_count: claims.len(),
            router_count,
            service_count,
            digest,
        },
        bytes,
    })
}

fn canonical_claim_order(left: &IngressRouteClaim, right: &IngressRouteClaim) -> Ordering {
    left.host
        .cmp(&right.host)
        .then_with(|| left.path_prefix.cmp(&right.path_prefix))
        .then_with(|| left.priority.cmp(&right.priority))
        .then_with(|| left.router_name.cmp(&right.router_name))
        .then_with(|| left.backend_alias.cmp(&right.backend_alias))
        .then_with(|| left.port.cmp(&right.port))
        .then_with(|| left.tls.cmp(&right.tls))
        .then_with(|| left.target_id.cmp(&right.target_id))
}

fn validate_claim(
    expected_target_id: TargetId,
    claim: &IngressRouteClaim,
) -> Result<(), TraefikDynamicConfigError> {
    if claim.target_id != expected_target_id {
        return Err(TraefikDynamicConfigError::TargetMismatch {
            expected_target_id,
            actual_target_id: claim.target_id,
            router_name: claim.router_name.clone(),
        });
    }
    if !is_resource_name(&claim.router_name) {
        return Err(TraefikDynamicConfigError::InvalidRouterName {
            router_name: claim.router_name.clone(),
        });
    }
    if !is_dns_name(&claim.host) {
        return Err(TraefikDynamicConfigError::InvalidHost {
            router_name: claim.router_name.clone(),
            host: claim.host.clone(),
        });
    }
    if !is_route_path(&claim.path_prefix) {
        return Err(TraefikDynamicConfigError::InvalidPath {
            router_name: claim.router_name.clone(),
            path: claim.path_prefix.clone(),
        });
    }
    if claim.port == 0 {
        return Err(TraefikDynamicConfigError::InvalidPort {
            router_name: claim.router_name.clone(),
        });
    }
    if !is_dns_name(&claim.backend_alias) {
        return Err(TraefikDynamicConfigError::InvalidBackendAlias {
            router_name: claim.router_name.clone(),
            backend_alias: claim.backend_alias.clone(),
        });
    }
    Ok(())
}

fn reject_overlapping_claims(
    claims: &[&IngressRouteClaim],
) -> Result<(), TraefikDynamicConfigError> {
    for (index, left) in claims.iter().enumerate() {
        for right in claims.iter().skip(index + 1) {
            if left.host != right.host {
                break;
            }
            let same_owner =
                left.project_id == right.project_id && left.environment == right.environment;
            let conflict = if same_owner {
                left.path_prefix == right.path_prefix
            } else {
                route_paths_overlap(&left.path_prefix, &right.path_prefix)
            };
            if conflict {
                return Err(TraefikDynamicConfigError::OverlappingClaims {
                    host: left.host.clone(),
                    left_path: left.path_prefix.clone(),
                    right_path: right.path_prefix.clone(),
                    left_router_name: left.router_name.clone(),
                    right_router_name: right.router_name.clone(),
                });
            }
        }
    }
    Ok(())
}

fn reject_router_name_collisions(
    claims: &[&IngressRouteClaim],
    project_middlewares: &BTreeMap<String, CanonicalMiddleware>,
) -> Result<(), TraefikDynamicConfigError> {
    let mut names = BTreeMap::<String, &str>::new();
    for claim in claims {
        let generated_names = [
            Some(claim.router_name.clone()),
            claim.tls.then(|| redirect_router_name(&claim.router_name)),
            Some(verification_router_name(&claim.router_name)),
            claim
                .middlewares
                .iter()
                .any(|name| {
                    matches!(
                        project_middlewares.get(name),
                        Some(CanonicalMiddleware::ForwardAuth { .. })
                    )
                })
                .then(|| format!("{}-gate-login", claim.router_name)),
        ];
        for name in generated_names.into_iter().flatten() {
            if let Some(owner) = names.insert(name.clone(), &claim.router_name) {
                return Err(TraefikDynamicConfigError::RouterNameCollision {
                    generated_name: name,
                    left_router_name: owner.to_owned(),
                    right_router_name: claim.router_name.clone(),
                });
            }
        }
    }
    Ok(())
}

fn route_rule(host: &str, path: &str) -> String {
    if path == "/" {
        format!("Host(`{host}`)")
    } else {
        format!("Host(`{host}`) && (Path(`{path}`) || PathPrefix(`{path}/`))")
    }
}

fn redirect_router_name(router_name: &str) -> String {
    format!("{router_name}-redirect")
}

fn verification_router_name(router_name: &str) -> String {
    format!("{router_name}-verify")
}

fn route_paths_overlap(left: &str, right: &str) -> bool {
    route_path_contains(left, right) || route_path_contains(right, left)
}

fn route_path_contains(prefix: &str, candidate: &str) -> bool {
    prefix == "/"
        || candidate == prefix
        || candidate
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn is_resource_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_dns_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
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

fn is_route_path(path: &str) -> bool {
    path.starts_with('/')
        && (path == "/" || !path.ends_with('/'))
        && !path.contains("//")
        && !path.split('/').any(|segment| matches!(segment, "." | ".."))
        && path.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~')
        })
}

fn route_tls(
    host: &str,
    local_ca_hosts: &BTreeSet<String>,
) -> Result<TraefikTls, TraefikDynamicConfigError> {
    if local_ca_hosts.contains(host) {
        return Ok(TraefikTls {
            cert_resolver: None,
        });
    }
    if is_local_ca_host(host) {
        return Err(TraefikDynamicConfigError::LocalHttpsRequiresCa {
            host: host.to_owned(),
        });
    }
    Ok(TraefikTls {
        cert_resolver: Some(TRAEFIK_CERTIFICATE_RESOLVER),
    })
}

#[derive(Serialize)]
struct TraefikDynamicConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    http: Option<TraefikHttp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<TraefikTlsStore>,
}

#[derive(Serialize)]
struct TraefikTlsStore {
    certificates: Vec<TraefikTlsCertificate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TraefikTlsCertificate {
    cert_file: String,
    key_file: String,
}

#[derive(Serialize)]
struct TraefikHttp {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    routers: BTreeMap<String, TraefikRouter>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    services: BTreeMap<String, TraefikService>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    middlewares: BTreeMap<String, TraefikMiddleware>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TraefikRouter {
    entry_points: Vec<&'static str>,
    rule: String,
    service: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    middlewares: Vec<String>,
    priority: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<TraefikTls>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TraefikTls {
    #[serde(skip_serializing_if = "Option::is_none")]
    cert_resolver: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TraefikService {
    load_balancer: TraefikLoadBalancer,
}

#[derive(Serialize)]
struct TraefikLoadBalancer {
    servers: Vec<TraefikServer>,
}

#[derive(Serialize)]
struct TraefikServer {
    url: String,
}

#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TraefikMiddleware {
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_scheme: Option<TraefikRedirectScheme>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<TraefikHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replace_path_regex: Option<TraefikReplacePathRegex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forward_auth: Option<TraefikForwardAuth>,
}

#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TraefikForwardAuth {
    address: String,
    trust_forward_header: bool,
    auth_response_headers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    add_auth_cookies_to_response: Vec<String>,
}

#[derive(Serialize, PartialEq, Eq)]
struct TraefikReplacePathRegex {
    regex: String,
    replacement: String,
}

#[derive(Serialize, PartialEq, Eq)]
struct TraefikRedirectScheme {
    scheme: &'static str,
    permanent: bool,
}

#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TraefikHeaders {
    custom_response_headers: BTreeMap<&'static str, &'static str>,
}

#[cfg(test)]
mod tests;
