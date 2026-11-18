use super::*;

pub(in crate::app::stacks) fn resolve_network_membership(
    document: &ConfigDocument,
) -> Result<BTreeMap<(String, String), BTreeSet<String>>, MmError> {
    let mut membership = BTreeMap::<(String, String), BTreeSet<String>>::new();
    let compose_services = compose_services_by_stack(document)?;
    for (stack_name, stack) in &document.config.stacks {
        let services = compose_services
            .get(stack_name)
            .cloned()
            .unwrap_or_default();
        for network in &stack.networks {
            for service in &services {
                membership
                    .entry((stack_name.clone(), service.clone()))
                    .or_default()
                    .insert(network.clone());
            }
        }
    }
    for module in document.config.modules.values() {
        for network in &module.networks {
            for (service_name, service) in &module.services {
                membership
                    .entry((service.stack.clone(), service_name.clone()))
                    .or_default()
                    .insert(network.clone());
            }
        }
        for (service_name, service) in &module.services {
            for network in &service.networks {
                membership
                    .entry((service.stack.clone(), service_name.clone()))
                    .or_default()
                    .insert(network.clone());
            }
        }
    }
    Ok(membership)
}

fn compose_services_by_stack(
    document: &ConfigDocument,
) -> Result<BTreeMap<String, BTreeSet<String>>, MmError> {
    let mut by_stack = BTreeMap::new();
    for (stack_name, stack) in &document.config.stacks {
        let mut services = BTreeSet::new();
        for relative in &stack.files {
            let path = document.root.join(relative);
            let Some(bytes) = read_regular_file(&path, MAX_COMPOSE_FILE_BYTES)? else {
                continue;
            };
            let parsed =
                serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&bytes).map_err(|error| {
                    MmError::Environment {
                        message: format!(
                            "Compose file '{}' is invalid YAML: {error}",
                            path.display()
                        ),
                    }
                })?;
            if let Some(mapping) = parsed
                .as_mapping()
                .and_then(|root| mapping_value(root, "services"))
                .and_then(serde_yaml_ng::Value::as_mapping)
            {
                for key in mapping.keys() {
                    if let Some(name) = key.as_str() {
                        services.insert(name.to_owned());
                    }
                }
            }
        }
        by_stack.insert(stack_name.clone(), services);
    }
    Ok(by_stack)
}
