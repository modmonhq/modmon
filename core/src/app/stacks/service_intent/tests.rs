use crate::domain::config::ServiceRole;

use super::*;
use crate::app::stacks::tests::support::{StackFixture, candidate, prepared_stack, route, service};

#[test]
fn automatic_routes_and_ingress_statuses_report_routed_and_skipped_services()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let web = service("web", true, vec![candidate(8080, TransportProtocol::Tcp)]);
    let project_slug = canonical_project_slug(&fixture.document)?;
    let automatic = automatic_local_routes(
        &fixture.document,
        "development",
        "app",
        &project_slug,
        std::slice::from_ref(&web),
        &[],
        &["auth".to_owned()],
    )?;
    assert_eq!(automatic.len(), 1);
    assert_eq!(automatic[0].host, "web.stack-fixture.test");
    assert_eq!(automatic[0].middlewares, ["auth"]);

    let disabled = automatic_local_routes(
        &fixture.document,
        "production",
        "app",
        &project_slug,
        std::slice::from_ref(&web),
        &[],
        &[],
    )?;
    assert!(disabled.is_empty());

    let duplicate = ResolvedIngressRoute {
        name: "duplicate".to_owned(),
        host: "web.stack-fixture.test".to_owned(),
        path_prefix: "/".to_owned(),
        service: "other".to_owned(),
        port: Some(8080),
        tls: true,
        priority: 1,
        balance: crate::domain::cluster_ingress::Balance::LocalFirst,
        headers: std::collections::BTreeMap::new(),
        allow: Vec::new(),
    };
    assert!(
        automatic_local_routes(
            &fixture.document,
            "development",
            "app",
            &project_slug,
            std::slice::from_ref(&web),
            &[duplicate],
            &[],
        )?
        .is_empty()
    );

    let invalid = service("---", true, vec![candidate(8080, TransportProtocol::Tcp)]);
    assert!(
        automatic_local_routes(
            &fixture.document,
            "development",
            "app",
            &project_slug,
            &[invalid],
            &[],
            &[],
        )
        .is_err()
    );

    let routed = prepared_stack(
        &fixture.document,
        vec![web.clone()],
        vec![route(
            &fixture.document,
            "web",
            "web.stack-fixture.test",
            "/api",
            false,
        )],
    );
    let statuses = ingress_statuses(&fixture.document, "development", &fixture.target, &[routed])?;
    assert_eq!(
        statuses[0].url.as_deref(),
        Some("http://web.stack-fixture.test/api")
    );
    assert_eq!(statuses[0].selected_port, Some(8080));
    assert!(statuses[0].warning.is_some());

    let unrouted = prepared_stack(&fixture.document, vec![web], Vec::new());
    let statuses = ingress_statuses(
        &fixture.document,
        "development",
        &fixture.target,
        &[unrouted],
    )?;
    assert!(
        statuses[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("ambiguous"))
    );
    assert_eq!(route_url("example.test", "/", true), "https://example.test");
    assert_eq!(
        route_url("example.test", "/api", false),
        "http://example.test/api"
    );
    Ok(())
}

#[test]
fn not_routed_reasons_cover_every_operator_action() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let base_config = fixture.document.config.modules["app"].services["web"].clone();
    let base_service = service("web", false, Vec::new());
    let mut cases = Vec::new();

    cases.push((
        fixture.document.clone(),
        "production".to_owned(),
        fixture.target.clone(),
        base_config.clone(),
        base_service.clone(),
        Some("web.stack-fixture.test".to_owned()),
        Vec::new(),
        "mm adds routes by itself only in the development environment",
    ));
    cases.push((
        fixture.document.clone(),
        "development".to_owned(),
        fixture.remote_target(),
        base_config.clone(),
        base_service.clone(),
        Some("web.stack-fixture.test".to_owned()),
        Vec::new(),
        "remote",
    ));

    let mut project_disabled = fixture.document.clone();
    project_disabled.config.ingress.automatic_local = Some(false);
    cases.push((
        project_disabled,
        "development".to_owned(),
        fixture.target.clone(),
        base_config.clone(),
        base_service.clone(),
        Some("web.stack-fixture.test".to_owned()),
        Vec::new(),
        "project scope",
    ));

    let mut environment_disabled = fixture.document.clone();
    environment_disabled
        .config
        .environments
        .get_mut("development")
        .ok_or("missing environment")?
        .automatic_local_ingress = Some(false);
    cases.push((
        environment_disabled,
        "development".to_owned(),
        fixture.target.clone(),
        base_config.clone(),
        base_service.clone(),
        Some("web.stack-fixture.test".to_owned()),
        Vec::new(),
        "selected environment",
    ));

    let mut service_disabled = base_config.clone();
    service_disabled.automatic_local_ingress = Some(false);
    cases.push((
        fixture.document.clone(),
        "development".to_owned(),
        fixture.target.clone(),
        service_disabled,
        base_service.clone(),
        Some("web.stack-fixture.test".to_owned()),
        Vec::new(),
        "this service",
    ));

    let mut worker = base_config.clone();
    worker.role = Some(ServiceRole::Worker);
    cases.push((
        fixture.document.clone(),
        "development".to_owned(),
        fixture.target.clone(),
        worker,
        base_service.clone(),
        Some("web.stack-fixture.test".to_owned()),
        Vec::new(),
        "not http-routable",
    ));

    let explicit = route(
        &fixture.document,
        "other",
        "web.stack-fixture.test",
        "/",
        true,
    );
    cases.push((
        fixture.document.clone(),
        "development".to_owned(),
        fixture.target.clone(),
        base_config.clone(),
        base_service.clone(),
        Some("web.stack-fixture.test".to_owned()),
        vec![explicit],
        "takes precedence",
    ));

    let mut problem = base_service.clone();
    problem.http_evidence.problem = Some("bad merge".to_owned());
    cases.push((
        fixture.document.clone(),
        "development".to_owned(),
        fixture.target.clone(),
        base_config.clone(),
        problem,
        Some("web.stack-fixture.test".to_owned()),
        Vec::new(),
        "invalid: bad merge",
    ));

    let evidence_cases = [
        (
            base_service.clone(),
            "absent from the ordered Compose model",
        ),
        (
            service("web", true, Vec::new()),
            "no HTTP target-port evidence",
        ),
        (
            service("web", true, vec![candidate(53, TransportProtocol::Udp)]),
            "HTTP requires one TCP",
        ),
        (
            service(
                "web",
                true,
                vec![
                    candidate(80, TransportProtocol::Tcp),
                    candidate(443, TransportProtocol::Tcp),
                ],
            ),
            "ambiguous",
        ),
    ];
    for (service, expected) in evidence_cases {
        cases.push((
            fixture.document.clone(),
            "development".to_owned(),
            fixture.target.clone(),
            base_config.clone(),
            service,
            Some("web.stack-fixture.test".to_owned()),
            Vec::new(),
            expected,
        ));
    }
    cases.push((
        fixture.document.clone(),
        "development".to_owned(),
        fixture.target.clone(),
        base_config,
        base_service,
        None,
        Vec::new(),
        "cannot form a safe",
    ));

    for (document, environment, target, config, service, host, routes, expected) in cases {
        let routes = routes.iter().collect::<Vec<_>>();
        let reason = not_routed_reason(
            &document,
            &environment,
            &target,
            &config,
            &service,
            host.as_deref(),
            &routes,
        );
        assert!(
            reason.contains(expected),
            "expected '{expected}' in '{reason}'"
        );
    }
    Ok(())
}

#[test]
fn mail_binding_and_certificate_policies_are_explicit() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let config = &fixture.document.config.modules["app"].services["web"];
    let bindings = development_mail_bindings(
        &fixture.document,
        "development",
        &fixture.target,
        "app",
        config,
    );
    assert_eq!(bindings.len(), 4);
    let mut configured_document = fixture.document.clone();
    configured_document.config.development_mail.bindings = BTreeMap::from([
        (
            "MM_MAIL_SMTP_HOST".to_owned(),
            crate::domain::development_mail::DevelopmentMailValue::SmtpHost,
        ),
        (
            "MM_MAIL_SMTP_PORT".to_owned(),
            crate::domain::development_mail::DevelopmentMailValue::SmtpPort,
        ),
    ]);
    let mut configured_service = config.clone();
    configured_service.env.insert(
        "MM_MAIL_SMTP_HOST".to_owned(),
        crate::domain::config::EnvScalar::String("preserved".to_owned()),
    );
    let configured = development_mail_bindings(
        &configured_document,
        "development",
        &fixture.target,
        "app",
        &configured_service,
    );
    assert!(configured.iter().any(|binding| matches!(
        binding.provenance,
        crate::domain::development_mail::MailBindingProvenance::PreservedProjectValue
    )));
    assert!(configured.iter().any(|binding| matches!(
        binding.provenance,
        crate::domain::development_mail::MailBindingProvenance::InjectedConfigured
    )));
    assert!(matches!(
        certificate_reference("web.stack-fixture.test", true),
        IngressCertificateReference::LocalCa { .. }
    ));
    assert!(matches!(
        certificate_reference("public.example.com", true),
        IngressCertificateReference::Acme { .. }
    ));
    assert_eq!(
        certificate_reference("public.example.com", false),
        IngressCertificateReference::None
    );

    let mut opted_out = config.clone();
    opted_out.development_mail = Some(false);
    assert!(
        development_mail_bindings(
            &fixture.document,
            "development",
            &fixture.target,
            "app",
            &opted_out,
        )
        .is_empty()
    );
    assert!(service_config(&fixture.document, "missing", "web").is_err());
    Ok(())
}

#[test]
fn ingress_statuses_reject_unsafe_slugs_and_sort_canonical_service_output()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let invalid = prepared_stack(
        &fixture.document,
        vec![service(
            "---",
            true,
            vec![candidate(8080, TransportProtocol::Tcp)],
        )],
        Vec::new(),
    );
    assert!(
        ingress_statuses(
            &fixture.document,
            "development",
            &fixture.target,
            &[invalid],
        )
        .is_err()
    );

    let services = vec![
        service(
            "worker",
            true,
            vec![candidate(8081, TransportProtocol::Tcp)],
        ),
        service("web", true, vec![candidate(8080, TransportProtocol::Tcp)]),
    ];
    let routes = vec![
        route(
            &fixture.document,
            "worker",
            "worker.stack-fixture.test",
            "/",
            true,
        ),
        route(
            &fixture.document,
            "web",
            "web.stack-fixture.test",
            "/",
            true,
        ),
        route(
            &fixture.document,
            "web",
            "web.stack-fixture.test",
            "/api",
            true,
        ),
    ];
    let statuses = ingress_statuses(
        &fixture.document,
        "development",
        &fixture.target,
        &[prepared_stack(&fixture.document, services, routes)],
    )?;
    assert_eq!(
        statuses
            .iter()
            .map(|status| status.service.as_str())
            .collect::<Vec<_>>(),
        ["web", "web", "worker"]
    );
    assert!(statuses[0].url < statuses[1].url);
    Ok(())
}
