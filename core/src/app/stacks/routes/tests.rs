use crate::domain::config::{IngressConfig, IngressRouteConfig};

use super::*;
use crate::app::stacks::tests::support::{StackFixture, candidate, service};

fn ingress(service: Option<&str>, domain: &str, port: Option<u16>) -> IngressConfig {
    IngressConfig::Route(IngressRouteConfig {
        domain: domain.to_owned(),
        route: "/api".to_owned(),
        service: service.map(str::to_owned),
        port,
        https: true,
        environments: Vec::new(),
        balance: None,
        off_locally: None,
        priority: None,
        headers: std::collections::BTreeMap::new(),
        allow: Vec::new(),
    })
}

#[test]
fn ingress_service_resolution_covers_selection_and_configuration_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let mut document = fixture.document.clone();
    document
        .config
        .stacks
        .get_mut("app")
        .ok_or("missing stack")?
        .ingress = ingress(None, "public", None);
    let stack = &document.config.stacks["app"];
    let resolved = resolve_ingress_services(&document, "development", "app", stack)?;
    assert_eq!(resolved[0].service, "web");
    assert_eq!(resolved[0].host, "public.example.test");

    let mut cases = Vec::new();
    let mut unknown_domain = document.clone();
    unknown_domain
        .config
        .stacks
        .get_mut("app")
        .ok_or("missing stack")?
        .ingress = ingress(None, "missing", None);
    cases.push((unknown_domain, "unknown domain"));

    let mut unknown_service = document.clone();
    unknown_service
        .config
        .stacks
        .get_mut("app")
        .ok_or("missing stack")?
        .ingress = ingress(Some("missing"), "public", None);
    cases.push((unknown_service, "not configured"));

    let mut no_services = document.clone();
    no_services
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .services
        .clear();
    cases.push((no_services, "no configured service"));

    let mut multiple = document;
    let worker = multiple.config.modules["app"].services["web"].clone();
    multiple
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .services
        .insert("worker".to_owned(), worker);
    cases.push((multiple, "multiple configured services"));

    for (document, expected) in cases {
        let stack = &document.config.stacks["app"];
        let error = resolve_ingress_services(&document, "development", "app", stack)
            .err()
            .ok_or("invalid ingress unexpectedly resolved")?;
        assert!(error.to_string().contains(expected), "{error}");
    }
    Ok(())
}

#[test]
fn ingress_port_resolution_and_http_selection_are_table_driven()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let route = ResolvedIngressRoute {
        name: "public-api".to_owned(),
        host: "public.example.com".to_owned(),
        path_prefix: "/api".to_owned(),
        service: "web".to_owned(),
        port: None,
        tls: true,
        priority: 10,
        balance: crate::domain::cluster_ingress::Balance::LocalFirst,
        headers: std::collections::BTreeMap::new(),
        allow: Vec::new(),
    };
    let web = service("web", true, vec![candidate(8080, TransportProtocol::Tcp)]);
    let routes = resolve_ingress_ports(
        &fixture.document,
        "development",
        "app",
        &[route],
        std::slice::from_ref(&web),
        &["auth".to_owned()],
    )?;
    assert_eq!(routes[0].port, 8080);
    assert_eq!(routes[0].middlewares, ["auth"]);
    assert!(matches!(
        routes[0].certificate_reference,
        IngressCertificateReference::Acme { .. }
    ));

    let cases = [
        (
            ServiceHttpEvidence {
                problem: Some("invalid evidence".to_owned()),
                ..ServiceHttpEvidence::default()
            },
            "invalid evidence",
        ),
        (ServiceHttpEvidence::default(), "no target port"),
        (
            ServiceHttpEvidence {
                candidates: vec![candidate(53, TransportProtocol::Udp)],
                ..ServiceHttpEvidence::default()
            },
            "HTTP requires one TCP",
        ),
        (
            ServiceHttpEvidence {
                candidates: vec![
                    candidate(80, TransportProtocol::Tcp),
                    candidate(443, TransportProtocol::Tcp),
                ],
                ..ServiceHttpEvidence::default()
            },
            "multiple target ports",
        ),
        (
            ServiceHttpEvidence {
                candidates: vec![
                    candidate(80, TransportProtocol::Tcp),
                    candidate(53, TransportProtocol::Udp),
                ],
                ..ServiceHttpEvidence::default()
            },
            "multiple port/protocol",
        ),
    ];
    for (evidence, expected) in cases {
        let mut service = web.clone();
        service.http_evidence = evidence;
        let error = selected_http_port(&service)
            .err()
            .ok_or("ambiguous HTTP evidence unexpectedly selected")?;
        assert!(error.contains(expected), "{error}");
    }

    let missing = ResolvedIngressRoute {
        service: "missing".to_owned(),
        port: Some(9000),
        name: "missing".to_owned(),
        host: "missing.example.test".to_owned(),
        path_prefix: "/".to_owned(),
        tls: false,
        priority: 1,
        balance: crate::domain::cluster_ingress::Balance::LocalFirst,
        headers: std::collections::BTreeMap::new(),
        allow: Vec::new(),
    };
    assert!(
        resolve_ingress_ports(
            &fixture.document,
            "development",
            "app",
            &[missing],
            &[web],
            &[],
        )
        .is_err()
    );
    Ok(())
}
