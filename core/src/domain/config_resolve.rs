use std::collections::BTreeMap;

use crate::domain::config::{CastBinding, EnvironmentBinding, IngressRouteConfig, MmConfig};

#[must_use]
pub fn effective_bindings<'a>(
    config: &'a MmConfig,
    environment: &str,
) -> BTreeMap<&'a str, &'a EnvironmentBinding> {
    let mut bindings = BTreeMap::new();
    for (key, binding) in &config.bindings {
        bindings.insert(key.as_str(), binding);
    }
    if let Some(environment_config) = config.environments.get(environment) {
        for (key, binding) in &environment_config.bindings {
            bindings.insert(key.as_str(), binding);
        }
    }
    bindings
}

#[must_use]
pub fn effective_binding<'a>(
    config: &'a MmConfig,
    environment: &str,
    key: &str,
) -> Option<&'a EnvironmentBinding> {
    config
        .environments
        .get(environment)
        .and_then(|environment_config| environment_config.bindings.get(key))
        .or_else(|| config.bindings.get(key))
}

#[must_use]
pub fn effective_domains<'a>(
    config: &'a MmConfig,
    environment: &str,
) -> BTreeMap<&'a str, &'a str> {
    config
        .environments
        .get(environment)
        .map(|environment_config| {
            environment_config
                .domains
                .iter()
                .map(|(key, host)| (key.as_str(), host.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

#[must_use]
pub fn route_is_active(route: &IngressRouteConfig, environment: &str) -> bool {
    (route.environments.is_empty() || route.environments.iter().any(|name| name == environment))
        && route.off_locally.as_deref() != Some(environment)
}

pub fn parse_domain_value_ref(value_ref: &str) -> Result<&str, String> {
    let Some(name) = value_ref.strip_prefix("domains.") else {
        return Err(format!(
            "value-ref '{value_ref}' is not a supported domain reference; use domains.<name>"
        ));
    };
    if name.is_empty() || name.contains('.') {
        return Err(format!(
            "value-ref '{value_ref}' is not a supported domain reference; use domains.<name>"
        ));
    }
    Ok(name)
}

#[must_use]
pub fn resolve_url_cast(host: &str, https: bool) -> String {
    if https {
        format!("https://{host}")
    } else {
        format!("http://{host}")
    }
}

pub fn resolve_cast_url(
    config: &MmConfig,
    environment: &str,
    binding: &CastBinding,
) -> Result<String, String> {
    let domain = parse_domain_value_ref(&binding.value_ref)?;
    let host = effective_domains(config, environment)
        .get(domain)
        .copied()
        .ok_or_else(|| {
            format!(
                "value-ref '{}' does not match a declared domain",
                binding.value_ref
            )
        })?;
    Ok(resolve_url_cast(
        host,
        domain_https_policy(config, environment, domain)?,
    ))
}

pub fn domain_https_policy(
    config: &MmConfig,
    environment: &str,
    domain: &str,
) -> Result<bool, String> {
    let mut policy = None;
    for route in active_routes(config, environment) {
        if route.domain != domain {
            continue;
        }
        match policy {
            None => policy = Some(route.https),
            Some(existing) if existing != route.https => {
                return Err(format!("domain '{domain}' has conflicting https policies"));
            }
            Some(_) => {}
        }
    }
    Ok(policy.unwrap_or(true))
}

#[must_use]
pub fn active_routes<'a>(config: &'a MmConfig, environment: &str) -> Vec<&'a IngressRouteConfig> {
    config
        .stacks
        .values()
        .flat_map(|stack| stack.ingress.routes())
        .filter(|route| route_is_active(route, environment))
        .collect()
}

#[must_use]
pub fn route_priority(path: &str) -> i64 {
    if path == "/" {
        1
    } else {
        let segments = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .count();
        i64::try_from(segments)
            .unwrap_or(i64::MAX)
            .saturating_mul(1000)
            + i64::try_from(path.len()).unwrap_or(i64::MAX)
    }
}

#[must_use]
pub fn route_paths_overlap(left: &str, right: &str) -> bool {
    route_path_contains(left, right) || route_path_contains(right, left)
}

#[must_use]
pub fn route_path_contains(prefix: &str, candidate: &str) -> bool {
    prefix == "/"
        || candidate == prefix
        || candidate
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

#[cfg(test)]
mod tests;
