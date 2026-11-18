use super::*;
use serde_yaml_ng::Value;

/// The services the environment leaves out by its own configuration. On a
/// cluster node this does not count the services placed on other nodes:
/// those belong to the environment and the plan reports where they run.
pub(super) fn environment_excluded_services(
    document: &ConfigDocument,
    environment: &str,
) -> Vec<crate::domain::stack::ExcludedService> {
    document
        .config
        .modules
        .iter()
        .flat_map(|(module, config)| {
            config
                .services
                .iter()
                .filter(|(_, service)| !service.is_in_environment(environment))
                .map(|(name, service)| crate::domain::stack::ExcludedService {
                    module: module.clone(),
                    stack: service.stack.clone(),
                    service: name.clone(),
                    off_locally: service.off_locally_in(environment).cloned(),
                })
        })
        .collect()
}

/// Every service that does not run here in `environment`: left out by the
/// environment, or, on a cluster node, placed on other nodes.
pub(super) fn excluded_services(
    document: &ConfigDocument,
    environment: &str,
) -> Vec<crate::domain::stack::ExcludedService> {
    document
        .config
        .modules
        .iter()
        .flat_map(|(module, config)| {
            config
                .services
                .iter()
                .filter(|(_, service)| !service.is_active(environment))
                .map(|(name, service)| crate::domain::stack::ExcludedService {
                    module: module.clone(),
                    stack: service.stack.clone(),
                    service: name.clone(),
                    off_locally: service.off_locally_in(environment).cloned(),
                })
        })
        .collect()
}

pub(super) fn excluded_service_names(
    document: &ConfigDocument,
    environment: &str,
    stack: &str,
) -> BTreeSet<String> {
    excluded_services(document, environment)
        .into_iter()
        .filter(|service| service.stack == stack)
        .map(|service| service.service)
        .collect()
}

pub(super) fn active_network_membership(
    document: &ConfigDocument,
    environment: &str,
) -> Result<BTreeMap<(String, String), BTreeSet<String>>, MmError> {
    let mut membership = resolve_network_membership(document)?;
    for service in excluded_services(document, environment) {
        membership.remove(&(service.stack, service.service));
    }
    Ok(membership)
}

pub(super) fn filter_compose_services(
    path: &Path,
    bytes: Vec<u8>,
    excluded: &BTreeSet<String>,
) -> Result<Vec<u8>, MmError> {
    if excluded.is_empty() {
        return Ok(bytes);
    }
    let error = |message| MmError::Environment {
        message: format!("Compose file '{}': {message}", path.display()),
    };
    let mut model: Value =
        serde_yaml_ng::from_slice(&bytes).map_err(|cause| error(cause.to_string()))?;
    model
        .apply_merge()
        .map_err(|cause| error(cause.to_string()))?;
    if let Some(services) = model.get_mut("services").and_then(Value::as_mapping_mut) {
        for name in excluded {
            services.remove(Value::String(name.clone()));
        }
        for (name, service) in services {
            let Some(config) = service.as_mapping_mut() else {
                continue;
            };
            if let Some(Value::Mapping(dependencies)) =
                config.get_mut(Value::String("depends_on".to_owned()))
            {
                dependencies.retain(|name, options| {
                    !(name.as_str().is_some_and(|name| excluded.contains(name))
                        && options.get("required").and_then(Value::as_bool) == Some(false))
                });
            }
            for field in [
                "depends_on",
                "links",
                "volumes_from",
                "network_mode",
                "ipc",
                "pid",
            ] {
                let Some(value) = config.get(Value::String(field.to_owned())) else {
                    continue;
                };
                let references: Vec<&str> = match value {
                    Value::Mapping(map) => map.keys().filter_map(Value::as_str).collect(),
                    Value::Sequence(items) => items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|item| item.split(':').next().unwrap_or(item))
                        .collect(),
                    Value::String(value) => value.strip_prefix("service:").into_iter().collect(),
                    _ => Vec::new(),
                };
                if let Some(dependency) =
                    references.into_iter().find(|name| excluded.contains(*name))
                {
                    return Err(error(format!(
                        "service '{}' references excluded service '{dependency}' through {field}; align their environments or remove the dependency",
                        name.as_str().unwrap_or("unknown")
                    )));
                }
            }
        }
    }
    serde_yaml_ng::to_string(&model)
        .map(String::into_bytes)
        .map_err(|cause| error(cause.to_string()))
}

pub(super) fn sources_have_services(sources: &[CompiledComposeSource]) -> Result<bool, MmError> {
    for source in sources {
        let model: Value =
            serde_yaml_ng::from_slice(&source.bytes).map_err(|error| MmError::Environment {
                message: error.to_string(),
            })?;
        if model
            .get("services")
            .and_then(Value::as_mapping)
            .is_some_and(|services| !services.is_empty())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests;
