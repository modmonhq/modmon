use super::*;
use crate::domain::config::{CastBinding, IngressConfig, ValueCast};

fn config() -> Result<MmConfig, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(
        "schema-version: 1\nproject:\n  id: 00000000-0000-0000-0000-000000000001\n  name: resolve\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\n    ingress:\n      - domain: web\n        route: /\n        https: true\n        environments: [local]\n      - domain: web\n        route: /api\n        https: true\n        environments: [local]\n      - domain: web\n        route: /inactive\n        https: false\n        environments: [production]\nenvironments:\n  local:\n    domains:\n      web: web.example.test\n    bindings: {}\n",
    )
}

#[test]
fn domain_casts_routes_and_fallbacks_have_exact_semantics() -> Result<(), String> {
    let mut config = config().map_err(|error| error.to_string())?;
    assert!(effective_bindings(&config, "absent").is_empty());
    assert!(effective_domains(&config, "absent").is_empty());
    assert_eq!(parse_domain_value_ref("domains.web")?, "web");
    assert!(parse_domain_value_ref("web").is_err());
    assert!(parse_domain_value_ref("domains.").is_err());
    assert!(parse_domain_value_ref("domains.web.extra").is_err());
    assert_eq!(resolve_url_cast("host", true), "https://host");
    assert_eq!(resolve_url_cast("host", false), "http://host");

    let mut binding = CastBinding {
        cast: ValueCast::Url,
        value_ref: "domains.web".to_owned(),
    };
    assert_eq!(
        resolve_cast_url(&config, "local", &binding)?,
        "https://web.example.test"
    );
    binding.value_ref = "domains.missing".to_owned();
    assert!(resolve_cast_url(&config, "local", &binding).is_err());
    binding.value_ref = "domains.web".to_owned();
    assert!(domain_https_policy(&config, "local", "unknown")?);
    assert_eq!(active_routes(&config, "local").len(), 2);

    let first_route = {
        let stack = config.stacks.get_mut("app").ok_or("stack missing")?;
        let IngressConfig::Routes(routes) = &mut stack.ingress else {
            return Err("expected route list".to_owned());
        };
        routes[1].https = false;
        routes[0].clone()
    };
    assert!(domain_https_policy(&config, "local", "web").is_err());
    let stack = config.stacks.get_mut("app").ok_or("stack missing")?;
    stack.ingress = IngressConfig::Route(first_route);
    assert_eq!(stack.ingress.routes().len(), 1);
    assert_eq!(route_priority("/"), 1);
    assert!(route_priority("/api/v1") > route_priority("/api"));
    assert!(route_paths_overlap("/api", "/api/v1"));
    assert!(!route_paths_overlap("/api", "/apix"));
    Ok(())
}
