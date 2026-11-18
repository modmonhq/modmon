//! What a route lets through and adds on a cluster target: response
//! headers, a method and path allowlist that answers 404 to the rest, and
//! the order of several routes on one host.

use std::collections::BTreeMap;

use mm::domain::cluster_ingress::{
    AllowRule, Balance, IngressRoute, IngressTable, ProxyNeeds, SealedIngress, proxy_compose,
    render_dynamic, route_lines, rule_problems,
};
use mm::domain::config::{EnvironmentConfig, IngressRouteConfig};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn route(router: &str, path: &str, priority: i64) -> IngressRoute {
    IngressRoute {
        router: router.to_owned(),
        host: "hooks.example.com".to_owned(),
        path: path.to_owned(),
        priority,
        stack: "app".to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls: true,
        balance: Balance::LocalFirst,
        headers: BTreeMap::new(),
        allow: Vec::new(),
    }
}

fn allow(methods: &[&str], path: Option<&str>) -> AllowRule {
    AllowRule {
        methods: methods.iter().map(|method| (*method).to_owned()).collect(),
        path: path.map(str::to_owned),
    }
}

fn hooks() -> IngressRoute {
    IngressRoute {
        headers: BTreeMap::from([("X-Robots-Tag".to_owned(), "noindex".to_owned())]),
        allow: vec![
            allow(&["GET", "HEAD"], Some("^/hooks/status$")),
            allow(&["POST"], Some("^/hooks/[^/]+/events$")),
        ],
        ..route("mm-hooks", "/", 1)
    }
}

fn rendered(routes: Vec<IngressRoute>) -> Result<serde_yaml_ng::Value, Box<dyn std::error::Error>> {
    let table = IngressTable {
        network: "0123456789ab-production-ingress".to_owned(),
        routes,
        certificate: None,
        engine_network: None,
        tunnel: None,
    };
    let text = render_dynamic(&[("shop".to_owned(), table)], &BTreeMap::new(), 1)?;
    Ok(serde_yaml_ng::from_str(&text)?)
}

#[test]
fn an_allowlist_lets_its_requests_through_and_sends_the_rest_to_404() -> TestResult {
    let file = rendered(vec![hooks()])?;
    let routers = &file["http"]["routers"];
    assert_eq!(
        routers["mm-hooks"]["rule"],
        "Host(`hooks.example.com`) && (((Method(`GET`) || Method(`HEAD`)) && PathRegexp(`^/hooks/status$`)) || ((Method(`POST`)) && PathRegexp(`^/hooks/[^/]+/events$`)))"
    );
    assert_eq!(
        routers["mm-hooks-denied"]["rule"],
        "Host(`hooks.example.com`)"
    );
    assert_eq!(routers["mm-hooks-denied"]["service"], "mm-denied");
    assert_eq!(routers["mm-hooks-denied"]["entryPoints"][0], "websecure");
    assert_eq!(
        routers["mm-hooks-denied"]["tls"]["certResolver"], "mm-http",
        "a refused request still gets the host's certificate"
    );
    let allowed = routers["mm-hooks"]["priority"]
        .as_i64()
        .ok_or("no priority")?;
    let denied = routers["mm-hooks-denied"]["priority"]
        .as_i64()
        .ok_or("no priority")?;
    assert_eq!(allowed, denied + 1);
    assert_eq!(
        file["http"]["services"]["mm-denied"]["loadBalancer"]["servers"][0]["url"],
        "http://127.0.0.1:8083"
    );
    assert!(
        file["http"]["services"]["mm-hooks"]["loadBalancer"]["servers"].is_sequence(),
        "the route keeps its own service"
    );
    Ok(())
}

#[test]
fn headers_are_set_on_the_responses_of_the_route() -> TestResult {
    let file = rendered(vec![hooks(), route("mm-plain", "/plain", 1)])?;
    assert_eq!(
        file["http"]["routers"]["mm-hooks"]["middlewares"][0],
        "mm-hooks-headers"
    );
    assert_eq!(
        file["http"]["middlewares"]["mm-hooks-headers"]["headers"]["customResponseHeaders"]["X-Robots-Tag"],
        "noindex"
    );
    assert!(file["http"]["routers"]["mm-plain"]["middlewares"].is_null());
    assert!(
        file["http"]["routers"]["mm-plain-denied"].is_null(),
        "a route without an allowlist lets everything through"
    );
    Ok(())
}

#[test]
fn the_longer_path_goes_first_unless_a_route_sets_its_priority() -> TestResult {
    use mm::domain::config_resolve::route_priority;
    let file = rendered(vec![
        route("mm-root", "/", route_priority("/")),
        route("mm-app", "/app", route_priority("/app")),
        route("mm-api", "/api", route_priority("/api")),
        route("mm-oidc", "/.well-known/openid-configuration", 5),
        IngressRoute {
            allow: vec![allow(&["GET"], None)],
            ..route("mm-guarded", "/app/admin", route_priority("/app/admin"))
        },
    ])?;
    let routers = &file["http"]["routers"];
    let priority = |name: &str| routers[name]["priority"].as_i64().unwrap_or_default();
    assert!(priority("mm-app") > priority("mm-root"));
    assert!(priority("mm-guarded-denied") > priority("mm-app"));
    assert!(
        priority("mm-oidc") > priority("mm-root") && priority("mm-oidc") < priority("mm-app"),
        "an explicit priority places a route among the others as given"
    );
    assert!(priority("mm-root") >= 1);
    Ok(())
}

#[test]
fn the_proxy_opens_what_its_tables_need_and_nothing_more() -> TestResult {
    let public = IngressTable {
        network: "n".to_owned(),
        routes: vec![route("mm-web", "/", 1)],
        certificate: None,
        engine_network: None,
        tunnel: None,
    };
    let guarded = IngressTable {
        routes: vec![hooks()],
        ..public.clone()
    };
    assert_eq!(
        ProxyNeeds::of([&public]),
        ProxyNeeds {
            public: true,
            ..ProxyNeeds::default()
        }
    );
    assert!(ProxyNeeds::of([&public, &guarded]).denied);
    let text = proxy_compose("/var/lib/mm/ingress", "traefik:v3", &[("shop", &guarded)])?;
    let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
    let command = compose["services"]["proxy"]["command"]
        .as_sequence()
        .ok_or("no command")?
        .iter()
        .filter_map(serde_yaml_ng::Value::as_str)
        .collect::<Vec<_>>();
    assert!(command.contains(&"--entrypoints.mm-denied.address=127.0.0.1:8083"));
    let ports = compose["services"]["proxy"]["ports"]
        .as_sequence()
        .ok_or("no ports")?
        .iter()
        .filter_map(serde_yaml_ng::Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(ports, ["80:8080", "443:8443", "127.0.0.1:7954:8082"]);
    let quiet = proxy_compose("/var/lib/mm/ingress", "traefik:v3", &[])?;
    assert!(!quiet.contains("80:8080") && !quiet.contains("mm-denied"));
    assert!(quiet.contains("127.0.0.1:7954:8082"));
    Ok(())
}

#[test]
fn a_rule_mm_cannot_render_safely_is_refused() {
    let none = BTreeMap::new();
    assert!(rule_problems(&none, &[allow(&["GET"], Some("^/a$"))], Some(10)).is_empty());
    for (rules, expected) in [
        (vec![allow(&[], None)], "neither methods nor a path"),
        (vec![allow(&["get"], None)], "HTTP methods in capitals"),
        (vec![allow(&[], Some("^/a`) || Host(`x"))], "backtick"),
        (vec![allow(&[], Some("^/(a"))], "not a regular expression"),
    ] {
        let problems = rule_problems(&none, &rules, None);
        assert!(
            problems.iter().any(|problem| problem.contains(expected)),
            "{rules:?}: {problems:?}"
        );
    }
    let bad_headers = BTreeMap::from([
        ("X Robots".to_owned(), "noindex".to_owned()),
        ("X-Ok".to_owned(), "line\r\nInjected: yes".to_owned()),
    ]);
    assert_eq!(rule_problems(&bad_headers, &[], None).len(), 2);
    for priority in [0, -1, 1_000_001] {
        assert_eq!(rule_problems(&none, &[], Some(priority)).len(), 1);
    }
    let sealed = SealedIngress {
        nodes: vec!["app-1".to_owned()],
        network: "n".to_owned(),
        routes: vec![IngressRoute {
            allow: vec![allow(&[], Some("^/`) || Host(`other"))],
            ..route("mm-hooks", "/", 1)
        }],
        certificate: None,
        tunnel: None,
    };
    assert!(
        sealed.validate().is_err(),
        "a node refuses a route it cannot write safely"
    );
}

#[test]
fn the_plan_lists_routes_in_the_order_the_proxy_tries_them() {
    let sealed = SealedIngress {
        nodes: vec!["app-1".to_owned()],
        network: "n".to_owned(),
        routes: vec![route("mm-root", "/", 1), {
            let mut hooks = hooks();
            hooks.path = "/hooks".to_owned();
            hooks.priority = 5000;
            hooks
        }],
        certificate: None,
        tunnel: None,
    };
    assert_eq!(
        route_lines(&sealed),
        [
            "https://hooks.example.com/hooks -> app/web port 8080, lets through GET|HEAD ^/hooks/status$, POST ^/hooks/[^/]+/events$ and answers 404 to the rest, sets X-Robots-Tag",
            "https://hooks.example.com -> app/web port 8080",
        ]
    );
}

#[test]
fn routes_and_environments_declare_their_rules_in_mm_yaml() -> TestResult {
    let route: IngressRouteConfig = serde_yaml_ng::from_str(
        "domain: hooks
service: hooks
priority: 200
headers: {X-Robots-Tag: noindex}
allow:
  - {methods: [GET, HEAD], path: '^/hooks/status$'}
  - {methods: [POST], path: '^/hooks/[^/]+/events$'}
",
    )?;
    assert_eq!(route.priority, Some(200));
    assert_eq!(route.allow.len(), 2);
    assert_eq!(route.headers["X-Robots-Tag"], "noindex");
    assert!(
        serde_yaml_ng::from_str::<IngressRouteConfig>(
            "domain: hooks\nallow: [{methods: [GET], paths: ['^/']}]\n"
        )
        .is_err(),
        "a mistyped key is refused"
    );
    let environment: EnvironmentConfig =
        serde_yaml_ng::from_str("target: live\ningress: {headers: {X-Robots-Tag: noindex}}\n")?;
    assert_eq!(
        environment.ingress.ok_or("no ingress")?.headers["X-Robots-Tag"],
        "noindex"
    );
    Ok(())
}
