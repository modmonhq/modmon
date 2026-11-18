use mm::app::render_traefik_dynamic_routes;
use mm::domain::ingress::IngressRouteClaim;
use mm::domain::target::TargetId;
use mm::domain::traefik_dynamic::{
    TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE, TRAEFIK_VERIFY_ENTRYPOINT, TRAEFIK_VERIFY_HEADER,
    TRAEFIK_VERIFY_HEADER_VALUE, TRAEFIK_VERIFY_MIDDLEWARE, TraefikDynamicConfigError,
};
use mm::domain::workspace::ProjectId;
use serde_yaml_ng::Value;
use uuid::Uuid;

const TARGET_ID: u128 = 0x319c527b62427b43bfd4bd60e27c1de3;

#[test]
fn renders_canonical_tls_routes_independent_of_claim_order()
-> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let api = claim(target_id, "api", "app.example.test", "/api", 8080, true);
    let assets = claim(
        target_id,
        "assets",
        "static.example.test",
        "/assets",
        3000,
        true,
    );

    let first = render(target_id, &[api.clone(), assets.clone()])?;
    let second = render(target_id, &[assets, api])?;

    assert_eq!(first, second);
    assert_eq!(first.manifest.target_id, target_id);
    assert_eq!(first.manifest.claim_count, 2);
    assert_eq!(first.manifest.router_count, 6);
    assert_eq!(first.manifest.service_count, 2);
    assert_eq!(
        first.manifest.digest,
        blake3::hash(&first.bytes).to_hex().to_string()
    );

    let config: Value = serde_yaml_ng::from_slice(&first.bytes)?;
    assert_eq!(
        config["http"]["routers"]["mm-api"]["entryPoints"][0],
        "websecure"
    );
    assert!(config["http"]["routers"]["mm-api"]["tls"]["certResolver"].is_null());
    assert_eq!(
        config["http"]["routers"]["mm-api-redirect"]["entryPoints"][0],
        "web"
    );
    assert_eq!(
        config["http"]["routers"]["mm-api-redirect"]["middlewares"][0],
        TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE
    );
    assert_eq!(
        config["http"]["middlewares"]
            .as_mapping()
            .map(serde_yaml_ng::Mapping::len),
        Some(2)
    );
    assert_eq!(
        config["http"]["middlewares"][TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE]["redirectScheme"]["scheme"],
        "https"
    );
    assert_eq!(
        config["http"]["routers"]["mm-api-verify"]["entryPoints"][0],
        TRAEFIK_VERIFY_ENTRYPOINT
    );
    assert!(config["http"]["routers"]["mm-api-verify"]["tls"].is_null());
    assert_eq!(
        config["http"]["routers"]["mm-api-verify"]["middlewares"][0],
        TRAEFIK_VERIFY_MIDDLEWARE
    );
    assert_eq!(
        config["http"]["middlewares"][TRAEFIK_VERIFY_MIDDLEWARE]["headers"]["customResponseHeaders"]
            [TRAEFIK_VERIFY_HEADER],
        TRAEFIK_VERIFY_HEADER_VALUE
    );
    assert_eq!(
        config["http"]["services"]["mm-api"]["loadBalancer"]["servers"][0]["url"],
        "http://mm-ingress-backend-mm-api:8080"
    );
    Ok(())
}

#[test]
fn renders_exact_path_boundaries_and_host_only_root() -> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let rendered = render(
        target_id,
        &[
            claim(target_id, "api", "api.example.test", "/api", 8080, true),
            claim(target_id, "root", "www.example.test", "/", 3000, true),
        ],
    )?;
    let config: Value = serde_yaml_ng::from_slice(&rendered.bytes)?;

    assert_eq!(
        config["http"]["routers"]["mm-root"]["rule"],
        "Host(`www.example.test`)"
    );
    assert_eq!(
        config["http"]["routers"]["mm-api"]["rule"],
        "Host(`api.example.test`) && (Path(`/api`) || PathPrefix(`/api/`))"
    );
    let rule = config["http"]["routers"]["mm-api"]["rule"]
        .as_str()
        .ok_or("API router rule is not a string")?;
    assert!(!rule.contains("PathPrefix(`/api`)"));
    let verification_rule = config["http"]["routers"]["mm-api-verify"]["rule"]
        .as_str()
        .ok_or("verification router rule is not a string")?;
    assert!(verification_rule.contains(rule));
    Ok(())
}

#[test]
fn renders_plain_http_directly_without_redirect_middleware()
-> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let rendered = render(
        target_id,
        &[claim(
            target_id,
            "plain",
            "plain.example.test",
            "/",
            8080,
            false,
        )],
    )?;
    let config: Value = serde_yaml_ng::from_slice(&rendered.bytes)?;

    assert_eq!(rendered.manifest.router_count, 2);
    assert_eq!(
        config["http"]["routers"]["mm-plain"]["entryPoints"][0],
        "web"
    );
    assert_eq!(config["http"]["routers"]["mm-plain"]["service"], "mm-plain");
    assert!(config["http"]["routers"]["mm-plain"]["tls"].is_null());
    assert!(config["http"]["routers"]["mm-plain"]["middlewares"].is_null());
    assert!(config["http"]["routers"]["mm-plain-redirect"].is_null());
    assert!(config["http"]["middlewares"][TRAEFIK_VERIFY_MIDDLEWARE].is_mapping());
    Ok(())
}

#[test]
fn rejects_duplicate_and_cross_owner_segment_overlaps() {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let api = claim(target_id, "api", "app.example.test", "/api", 8080, true);
    let duplicate = claim(
        target_id,
        "duplicate",
        "app.example.test",
        "/api",
        8081,
        true,
    );
    let nested = claim(
        target_id,
        "nested",
        "app.example.test",
        "/api/v1",
        8082,
        true,
    );
    let adjacent = claim(
        target_id,
        "adjacent",
        "app.example.test",
        "/apix",
        8083,
        true,
    );
    let mut foreign_nested = nested.clone();
    foreign_nested.project_id = ProjectId(Uuid::from_u128(0x229c527b62427b43bfd4bd60e27c1de3));
    foreign_nested.environment = "other".to_owned();

    assert!(matches!(
        render(target_id, &[api.clone(), duplicate]),
        Err(TraefikDynamicConfigError::OverlappingClaims { .. })
    ));
    assert!(render(target_id, &[api.clone(), nested]).is_ok());
    assert!(matches!(
        render(target_id, &[api.clone(), foreign_nested]),
        Err(TraefikDynamicConfigError::OverlappingClaims { .. })
    ));
    assert!(render(target_id, &[api, adjacent]).is_ok());
}

#[test]
fn assigns_longest_prefix_priority_for_same_owner_nested_routes()
-> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let rendered = render(
        target_id,
        &[
            claim(target_id, "root", "app.example.test", "/", 3000, true),
            claim(target_id, "cp", "app.example.test", "/cp", 3001, true),
        ],
    )?;
    let config: Value = serde_yaml_ng::from_slice(&rendered.bytes)?;
    assert_eq!(config["http"]["routers"]["mm-root"]["priority"], 1);
    assert_eq!(config["http"]["routers"]["mm-cp"]["priority"], 1003);
    assert!(
        config["http"]["routers"]["mm-cp"]["priority"]
            .as_i64()
            .unwrap_or_default()
            > config["http"]["routers"]["mm-root"]["priority"]
                .as_i64()
                .unwrap_or_default()
    );
    Ok(())
}

#[test]
fn rejects_claims_from_another_target() {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let other_target_id = TargetId(Uuid::from_u128(TARGET_ID + 1));
    let claims = [
        claim(target_id, "first", "first.example.test", "/", 8080, true),
        claim(
            other_target_id,
            "second",
            "second.example.test",
            "/",
            8081,
            true,
        ),
    ];

    assert!(matches!(
        render(target_id, &claims),
        Err(TraefikDynamicConfigError::TargetMismatch {
            expected_target_id,
            actual_target_id,
            ..
        }) if expected_target_id == target_id && actual_target_id == other_target_id
    ));
}

#[test]
fn emits_no_claim_metadata_secrets_or_filesystem_paths_and_rejects_expressions()
-> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let mut safe = claim(target_id, "safe", "safe.example.test", "/api", 8080, true);
    safe.project_name = "database-password=super-secret".to_owned();
    safe.environment = "/srv/mm/private".to_owned();
    safe.stack = "/run/secrets/backend-token".to_owned();
    safe.service = "secret-service-name".to_owned();
    safe.artifact_hash = "private-artifact-material".to_owned();

    let rendered = render(target_id, &[safe.clone()])?;
    let output = String::from_utf8(rendered.bytes)?;
    for excluded in [
        "super-secret",
        "/srv/mm/private",
        "/run/secrets/backend-token",
        "secret-service-name",
        "private-artifact-material",
    ] {
        assert!(!output.contains(excluded));
    }

    let mut expression = safe.clone();
    expression.host = "safe.example.test`) || Host(`attacker.example.test".to_owned();
    assert!(matches!(
        render(target_id, &[expression]),
        Err(TraefikDynamicConfigError::InvalidHost { .. })
    ));
    safe.backend_alias = "/run/secrets/backend".to_owned();
    assert!(matches!(
        render(target_id, &[safe]),
        Err(TraefikDynamicConfigError::InvalidBackendAlias { .. })
    ));
    Ok(())
}

#[test]
fn attaches_project_middleware_only_to_primary_routers() -> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::ingress::CanonicalMiddleware;
    use std::collections::BTreeMap;

    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let mut api = claim(target_id, "api", "app.example.test", "/api", 8080, true);
    let name = "mm-119c527b62427b43bfd4bd60e27c1de3-block-metrics-block-metrics";
    api.middlewares = vec![name.to_owned()];
    let mut definitions = BTreeMap::new();
    definitions.insert(
        name.to_owned(),
        CanonicalMiddleware::ReplacePathRegex {
            regex: "^/(prometheus|metrics).*".to_owned(),
            replacement: "/heartbeat".to_owned(),
        },
    );

    let rendered = render_traefik_dynamic_routes(
        target_id,
        &[api],
        &definitions,
        &["app.example.test".to_owned()].into_iter().collect(),
    )?;
    let config: Value = serde_yaml_ng::from_slice(&rendered.bytes)?;
    assert_eq!(config["http"]["routers"]["mm-api"]["middlewares"][0], name);
    assert_eq!(
        config["http"]["routers"]["mm-api-redirect"]["middlewares"][0],
        TRAEFIK_HTTPS_REDIRECT_MIDDLEWARE
    );
    assert_eq!(
        config["http"]["routers"]["mm-api-verify"]["middlewares"][0],
        TRAEFIK_VERIFY_MIDDLEWARE
    );
    assert_eq!(
        config["http"]["middlewares"][name]["replacePathRegex"]["replacement"],
        "/heartbeat"
    );
    Ok(())
}

#[test]
fn renders_forward_auth_and_a_high_priority_gate_login_router_only_when_attached()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::ingress::CanonicalMiddleware;
    use std::collections::BTreeMap;

    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let mut gated = claim(target_id, "gated", "app.example.test", "/", 8080, true);
    let name = "mm-gate-example";
    gated.middlewares = vec![name.to_owned()];
    let definitions = BTreeMap::from([(
        name.to_owned(),
        CanonicalMiddleware::ForwardAuth {
            address:
                "http://mm-control-host:42831/_mm/gate/forward/00000000-0000-0000-0000-000000000001"
                    .to_owned(),
            trust_forward_header: false,
            auth_response_headers: vec!["X-MM-Gate-Principal".to_owned()],
            add_auth_cookies_to_response: vec![
                "__Host-mm_gate_00000000000000000000000000000001".to_owned(),
            ],
        },
    )]);

    let rendered = render_traefik_dynamic_routes(
        target_id,
        &[gated],
        &definitions,
        &["app.example.test".to_owned()].into_iter().collect(),
    )?;
    let config: Value = serde_yaml_ng::from_slice(&rendered.bytes)?;
    assert_eq!(
        config["http"]["middlewares"][name]["forwardAuth"]["trustForwardHeader"],
        false
    );
    assert_eq!(
        config["http"]["middlewares"][name]["forwardAuth"]["addAuthCookiesToResponse"][0],
        "__Host-mm_gate_00000000000000000000000000000001"
    );
    assert_eq!(
        config["http"]["routers"]["mm-gated"]["middlewares"],
        serde_yaml_ng::from_str::<Value>(
            "[mm-gate-ingress-auth@file, mm-gate-example, mm-gate-ingress-clear@file]"
        )?
    );
    assert_eq!(
        config["http"]["routers"]["mm-gated-gate-login"]["priority"],
        1_000_001
    );
    assert_eq!(
        config["http"]["routers"]["mm-gated-gate-login"]["service"],
        "mm-gate-control"
    );
    assert_eq!(
        config["http"]["routers"]["mm-gated-gate-login"]["middlewares"][0],
        "mm-gate-ingress-auth@file"
    );
    assert_eq!(
        config["http"]["services"]["mm-gate-control"]["loadBalancer"]["servers"][0]["url"],
        "http://mm-control-host:42831"
    );

    let ungated = render(
        target_id,
        &[claim(
            target_id,
            "plain",
            "plain.example.test",
            "/",
            8080,
            true,
        )],
    )?;
    let ungated: Value = serde_yaml_ng::from_slice(&ungated.bytes)?;
    assert!(ungated["http"]["routers"]["mm-plain-gate-login"].is_null());
    assert!(ungated["http"]["services"]["mm-gate-control"].is_null());
    Ok(())
}

#[test]
fn renders_an_empty_complete_target_slice() -> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(Uuid::from_u128(TARGET_ID));
    let rendered = render(target_id, &[])?;
    let config: Value = serde_yaml_ng::from_slice(&rendered.bytes)?;

    assert_eq!(rendered.manifest.claim_count, 0);
    assert_eq!(rendered.manifest.router_count, 0);
    assert_eq!(rendered.manifest.service_count, 0);
    assert_eq!(
        config.as_mapping().map(serde_yaml_ng::Mapping::len),
        Some(0)
    );
    assert_eq!(
        rendered.bytes,
        mm::domain::traefik_dynamic::EMPTY_DYNAMIC_CONFIG.as_bytes()
    );
    Ok(())
}

#[test]
fn local_ca_hosts_omit_acme_and_reference_issued_files() -> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(Uuid::from_u128(0x019c527b62427b43bfd4bd60e27c1de3));
    let rendered = render_traefik_dynamic_routes(
        target_id,
        &[claim(target_id, "web", "web.example.test", "/", 80, true)],
        &std::collections::BTreeMap::new(),
        &["web.example.test".to_owned()].into_iter().collect(),
    )?;
    let config: serde_yaml_ng::Value = serde_yaml_ng::from_slice(&rendered.bytes)?;
    assert!(
        config["http"]["routers"]["mm-web"]["tls"]
            .as_mapping()
            .is_some_and(|tls| !tls.contains_key(serde_yaml_ng::Value::from("certResolver")))
    );
    assert_eq!(
        config["tls"]["certificates"][0]["certFile"],
        "/etc/traefik/dynamic/web.example.test.crt"
    );
    Ok(())
}

fn claim(
    target_id: TargetId,
    name: &str,
    host: &str,
    path_prefix: &str,
    port: u16,
    tls: bool,
) -> IngressRouteClaim {
    let router_name = format!("mm-{name}");
    IngressRouteClaim {
        target: "production".to_owned(),
        target_id,
        project_id: ProjectId(Uuid::from_u128(0x119c527b62427b43bfd4bd60e27c1de3)),
        project_name: "example".to_owned(),
        environment: "production".to_owned(),
        stack: "app".to_owned(),
        route: name.to_owned(),
        host: host.to_owned(),
        path_prefix: path_prefix.to_owned(),
        service: "backend".to_owned(),
        port,
        tls,
        provenance: mm::domain::ingress::IngressRouteProvenance::ExplicitConfig,
        certificate_reference: if tls {
            mm::domain::ingress::IngressCertificateReference::LocalCa {
                host: host.to_owned(),
            }
        } else {
            mm::domain::ingress::IngressCertificateReference::None
        },
        ownership: mm::domain::ingress::IngressRouteOwnership::Project {
            project_id: ProjectId(Uuid::from_u128(0x119c527b62427b43bfd4bd60e27c1de3)),
            environment: "production".to_owned(),
            stack: "app".to_owned(),
            service: "backend".to_owned(),
        },
        priority: if path_prefix == "/" {
            1
        } else {
            i64::try_from(path_prefix.split('/').filter(|s| !s.is_empty()).count()).unwrap_or(1)
                * 1000
                + i64::try_from(path_prefix.len()).unwrap_or(1)
        },
        backend_alias: format!("mm-ingress-backend-{router_name}"),
        router_name,
        artifact_hash: "artifact-hash".to_owned(),
        middlewares: Vec::new(),
    }
}

fn render(
    target_id: TargetId,
    claims: &[IngressRouteClaim],
) -> Result<mm::domain::traefik_dynamic::RenderedTraefikDynamicConfig, TraefikDynamicConfigError> {
    let local_hosts = claims
        .iter()
        .filter(|claim| claim.tls && mm::domain::ca::is_local_ca_host(&claim.host))
        .map(|claim| claim.host.clone())
        .collect();
    render_traefik_dynamic_routes(
        target_id,
        claims,
        &std::collections::BTreeMap::new(),
        &local_hosts,
    )
}
