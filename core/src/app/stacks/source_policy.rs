use super::*;

pub(super) fn reject_transitive_compose_files(
    path: &std::path::Path,
    bytes: &[u8],
) -> Result<(), MmError> {
    let mut document =
        serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(bytes).map_err(|error| {
            MmError::Environment {
                message: format!("Compose file '{}' is invalid YAML: {error}", path.display()),
            }
        })?;
    reject_compose_yaml_tags(path, &document)?;
    document
        .apply_merge()
        .map_err(|error| MmError::Environment {
            message: format!(
                "Compose file '{}' has invalid YAML merge keys: {error}",
                path.display()
            ),
        })?;
    let Some(root) = document.as_mapping() else {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' must contain a YAML mapping",
                path.display()
            ),
        });
    };
    if mapping_value(root, "include").is_some() {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' uses include; list every Compose file explicitly in stack.files",
                path.display()
            ),
        });
    }
    if let Some(services) =
        mapping_value(root, "services").and_then(serde_yaml_ng::Value::as_mapping)
    {
        for service in services.values() {
            let external_extends = service
                .as_mapping()
                .and_then(|service| mapping_value(service, "extends"))
                .and_then(serde_yaml_ng::Value::as_mapping)
                .is_some_and(|extends| mapping_value(extends, "file").is_some());
            if external_extends {
                return Err(MmError::Environment {
                    message: format!(
                        "Compose file '{}' uses extends.file; list and merge Compose files explicitly in stack.files",
                        path.display()
                    ),
                });
            }
        }
    }
    Ok(())
}

pub(super) fn reject_reserved_ingress_configuration(
    path: &Path,
    bytes: &[u8],
    routed_services: &std::collections::BTreeSet<&str>,
    proxy_ports: Option<(u16, u16, u16)>,
    project_root: &Path,
    compose_directory: &Path,
    host_network: &super::host_network::HostNetwork<'_>,
) -> Result<(), MmError> {
    let mut document =
        serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(bytes).map_err(|error| {
            MmError::Environment {
                message: format!("Compose file '{}' is invalid YAML: {error}", path.display()),
            }
        })?;
    reject_compose_yaml_tags(path, &document)?;
    let written = document.clone();
    document
        .apply_merge()
        .map_err(|error| MmError::Environment {
            message: format!(
                "Compose file '{}' has invalid YAML merge keys: {error}",
                path.display()
            ),
        })?;
    if let Some(root) = document.as_mapping_mut() {
        super::guardrails::resolve_extends(root);
    }
    let Some(root) = document.as_mapping() else {
        return Ok(());
    };
    reject_reserved_resources(path, root)?;
    if let Some(services) =
        mapping_value(root, "services").and_then(serde_yaml_ng::Value::as_mapping)
    {
        for (service_name, service) in services {
            let service_name = service_name.as_str().unwrap_or_default();
            let Some(service) = service.as_mapping() else {
                continue;
            };
            reject_reserved_service_identity(path, service_name, service)?;
            let as_written = written
                .get("services")
                .and_then(|services| services.get(service_name))
                .and_then(serde_yaml_ng::Value::as_mapping);
            if let Some(message) = host_network.refusal(
                path,
                service_name,
                service,
                as_written,
                routed_services.contains(service_name),
            ) {
                return Err(MmError::Environment { message });
            }
            reject_reserved_service_networking(path, service_name, service)?;
            reject_unsafe_service_mounts(
                path,
                service,
                proxy_ports,
                project_root,
                compose_directory,
            )?;
        }
    }
    Ok(())
}

fn reject_reserved_resources(path: &Path, root: &serde_yaml_ng::Mapping) -> Result<(), MmError> {
    for resource_kind in ["networks", "volumes"] {
        if let Some(resources) =
            mapping_value(root, resource_kind).and_then(serde_yaml_ng::Value::as_mapping)
        {
            for (key, value) in resources {
                let key = key.as_str().unwrap_or_default();
                let explicit_name = compose_resource_name(value).unwrap_or_default();
                if is_reserved_ingress_name(key)
                    || explicit_name.to_ascii_lowercase().contains("mm-ingress-")
                    || explicit_name.contains('$')
                {
                    return Err(MmError::Environment {
                        message: format!(
                            "Compose file '{}' uses an mm-reserved ingress {resource_kind} name",
                            path.display()
                        ),
                    });
                }
                if resource_kind == "volumes"
                    && value.as_mapping().is_some_and(|volume| {
                        mapping_value(volume, "driver_opts").is_some()
                            || mapping_value(volume, "driver")
                                .and_then(serde_yaml_ng::Value::as_str)
                                .is_some_and(|driver| !driver.eq_ignore_ascii_case("local"))
                    })
                {
                    return Err(MmError::Environment {
                        message: format!(
                            "Compose file '{}' uses a custom volume driver, which can bypass private ingress volume ownership",
                            path.display()
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

fn reject_reserved_service_identity(
    path: &Path,
    service_name: &str,
    service: &serde_yaml_ng::Mapping,
) -> Result<(), MmError> {
    if is_reserved_ingress_name(service_name) {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' uses an mm-reserved ingress service name",
                path.display()
            ),
        });
    }
    for field in ["container_name", "hostname"] {
        if mapping_value(service, field)
            .and_then(serde_yaml_ng::Value::as_str)
            .is_some_and(|identity| is_reserved_ingress_name(identity) || identity.contains('$'))
        {
            return Err(MmError::Environment {
                message: format!(
                    "Compose file '{}' uses an mm-reserved or dynamic service identity",
                    path.display()
                ),
            });
        }
    }
    if mapping_value(service, "links").is_some_and(uses_reserved_ingress_link) {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' uses an mm-reserved or dynamic service link alias",
                path.display()
            ),
        });
    }
    if let Some(labels) = mapping_value(service, "labels") {
        let reserved = labels.as_mapping().is_some_and(|labels| {
            labels
                .keys()
                .filter_map(serde_yaml_ng::Value::as_str)
                .any(is_reserved_label)
        }) || labels.as_sequence().is_some_and(|labels| {
            labels
                .iter()
                .filter_map(serde_yaml_ng::Value::as_str)
                .any(|label| is_reserved_label(label.split_once('=').map_or(label, |(key, _)| key)))
        });
        if reserved {
            return Err(MmError::Environment {
                message: format!(
                    "Compose file '{}' sets an mm-reserved ingress label",
                    path.display()
                ),
            });
        }
    }
    Ok(())
}

fn reject_reserved_service_networking(
    path: &Path,
    service_name: &str,
    service: &serde_yaml_ng::Mapping,
) -> Result<(), MmError> {
    if mapping_value(service, "networks").is_some_and(|networks| {
        networks.as_mapping().is_some_and(|networks| {
            networks
                .keys()
                .filter_map(serde_yaml_ng::Value::as_str)
                .any(is_reserved_ingress_name)
        }) || networks.as_sequence().is_some_and(|networks| {
            networks
                .iter()
                .filter_map(serde_yaml_ng::Value::as_str)
                .any(is_reserved_ingress_name)
        })
    }) {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' attaches a service to an mm-reserved ingress network",
                path.display()
            ),
        });
    }
    match super::compose_environment::compose_environment_refusal(path, service_name, service) {
        Some(message) => Err(MmError::Environment { message }),
        None => Ok(()),
    }
}

fn reject_unsafe_service_mounts(
    path: &Path,
    service: &serde_yaml_ng::Mapping,
    proxy_ports: Option<(u16, u16, u16)>,
    project_root: &Path,
    compose_directory: &Path,
) -> Result<(), MmError> {
    if mapping_value(service, "volumes_from").is_some() {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' uses volumes_from, which can bypass private ingress volume ownership",
                path.display()
            ),
        });
    }
    if let Some(volumes) = mapping_value(service, "volumes")
        && uses_unsafe_ingress_volume(volumes, project_root, compose_directory)?
    {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' mounts an mm-reserved, dynamic, symlinked, or out-of-project volume source",
                path.display()
            ),
        });
    }
    if let Some(proxy_ports) = proxy_ports
        && mapping_value(service, "ports").is_some_and(|ports| uses_proxy_port(ports, proxy_ports))
    {
        return Err(MmError::Environment {
            message: format!(
                "Compose file '{}' publishes an mm-reserved ingress proxy port",
                path.display()
            ),
        });
    }
    Ok(())
}

pub(super) fn reject_nonportable_compose(
    path: &Path,
    bytes: &[u8],
    managed_networks: &BTreeSet<String>,
) -> Result<(), MmError> {
    let document = serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(bytes).map_err(|error| {
        MmError::Environment {
            message: format!("Compose file '{}' is invalid YAML: {error}", path.display()),
        }
    })?;
    let Some(root) = document.as_mapping() else {
        return Ok(());
    };
    if let Some(networks) =
        mapping_value(root, "networks").and_then(serde_yaml_ng::Value::as_mapping)
    {
        for key in networks.keys() {
            if key
                .as_str()
                .is_some_and(|name| managed_networks.contains(name))
            {
                return Err(MmError::Environment {
                    message: format!(
                        "Compose file '{}' declares mm-managed network '{}'; declare it in mm.yaml",
                        path.display(),
                        key.as_str().unwrap_or_default()
                    ),
                });
            }
        }
    }
    if let Some(services) =
        mapping_value(root, "services").and_then(serde_yaml_ng::Value::as_mapping)
    {
        for service in services.values() {
            let Some(service) = service.as_mapping() else {
                continue;
            };
            if mapping_value(service, "networks").is_some_and(|networks| {
                networks.as_mapping().is_some_and(|networks| {
                    networks
                        .keys()
                        .filter_map(serde_yaml_ng::Value::as_str)
                        .any(|name| managed_networks.contains(name))
                }) || networks.as_sequence().is_some_and(|networks| {
                    networks
                        .iter()
                        .filter_map(serde_yaml_ng::Value::as_str)
                        .any(|name| managed_networks.contains(name))
                })
            }) {
                return Err(MmError::Environment {
                    message: format!(
                        "Compose file '{}' attaches a service to an mm-managed network; declare attachments in mm.yaml",
                        path.display()
                    ),
                });
            }
        }
    }
    Ok(())
}

pub(super) fn compose_resource_name(value: &serde_yaml_ng::Value) -> Option<&str> {
    let mapping = value.as_mapping()?;
    mapping_value(mapping, "name")
        .and_then(serde_yaml_ng::Value::as_str)
        .or_else(|| {
            mapping_value(mapping, "external")
                .and_then(serde_yaml_ng::Value::as_mapping)
                .and_then(|external| mapping_value(external, "name"))
                .and_then(serde_yaml_ng::Value::as_str)
        })
}

pub(super) fn uses_unsafe_ingress_volume(
    value: &serde_yaml_ng::Value,
    project_root: &Path,
    compose_directory: &Path,
) -> Result<bool, MmError> {
    let Some(volumes) = value.as_sequence() else {
        return Ok(false);
    };
    for volume in volumes {
        if let Some(volume) = volume.as_str() {
            let source = volume.split_once(':').map_or("", |(source, _)| source);
            if unsafe_volume_source(
                source,
                is_bind_source(source),
                project_root,
                compose_directory,
            )? {
                return Ok(true);
            }
            continue;
        }
        let Some(volume) = volume.as_mapping() else {
            continue;
        };
        let source = mapping_value(volume, "source")
            .and_then(serde_yaml_ng::Value::as_str)
            .unwrap_or_default();
        let kind = mapping_value(volume, "type")
            .and_then(serde_yaml_ng::Value::as_str)
            .unwrap_or("volume");
        if kind.contains('$')
            || unsafe_volume_source(
                source,
                kind.eq_ignore_ascii_case("bind"),
                project_root,
                compose_directory,
            )?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn unsafe_volume_source(
    source: &str,
    bind: bool,
    project_root: &Path,
    compose_directory: &Path,
) -> Result<bool, MmError> {
    if source.contains('$') || is_reserved_ingress_name(source) {
        return Ok(true);
    }
    if !bind || source.is_empty() {
        return Ok(false);
    }
    if source.starts_with('~') {
        return Ok(true);
    }
    let source = Path::new(source);
    let absolute = if source.is_absolute() {
        source.to_path_buf()
    } else {
        compose_directory.join(source)
    };
    let Some(normalized) = normalize_absolute_path(&absolute) else {
        return Ok(true);
    };
    if !normalized.starts_with(project_root) {
        return Ok(true);
    }
    ensure_no_symlink_components(&normalized)?;
    Ok(false)
}

pub(super) fn is_bind_source(source: &str) -> bool {
    source.starts_with('/') || source.starts_with('.') || source.starts_with('~')
}

pub(super) fn normalize_absolute_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    Some(normalized)
}

pub(super) fn uses_proxy_port(value: &serde_yaml_ng::Value, proxy_ports: (u16, u16, u16)) -> bool {
    let proxy_ports = [proxy_ports.0, proxy_ports.1, proxy_ports.2];
    value.as_sequence().is_some_and(|ports| {
        ports.iter().any(|port| {
            if let Some(port) = port.as_str() {
                if port.contains('$') {
                    return true;
                }
                return port.split(':').any(|part| {
                    proxy_ports
                        .iter()
                        .any(|expected| port_expression_contains(part, *expected))
                });
            }
            let published = port
                .as_mapping()
                .and_then(|port| mapping_value(port, "published"));
            published.is_some_and(|published| {
                published.as_u64().is_some_and(|port| {
                    u16::try_from(port).is_ok_and(|port| proxy_ports.contains(&port))
                }) || published.as_str().is_some_and(|port| {
                    port.contains('$')
                        || proxy_ports
                            .iter()
                            .any(|expected| port_expression_contains(port, *expected))
                })
            })
        })
    })
}

pub(super) fn port_expression_contains(value: &str, expected: u16) -> bool {
    let value = value
        .split_once('/')
        .map_or(value, |(value, _)| value)
        .trim_matches(['[', ']']);
    if let Some((start, end)) = value.split_once('-') {
        return match (start.parse::<u16>(), end.parse::<u16>()) {
            (Ok(start), Ok(end)) => start <= expected && expected <= end,
            (Err(_), _) | (_, Err(_)) => false,
        };
    }
    value.parse::<u16>() == Ok(expected)
}

pub(super) fn is_reserved_label(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    label.starts_with("traefik.") || label.starts_with("io.mm.")
}

pub(super) fn is_reserved_ingress_name(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("mm-ingress")
}

pub(super) fn uses_reserved_ingress_link(value: &serde_yaml_ng::Value) -> bool {
    value.as_sequence().is_some_and(|links| {
        links
            .iter()
            .filter_map(serde_yaml_ng::Value::as_str)
            .any(|link| {
                let alias = link.rsplit_once(':').map_or(link, |(_, alias)| alias);
                alias.contains('$') || is_reserved_ingress_name(alias)
            })
    })
}

pub(super) fn reject_compose_yaml_tags(
    path: &Path,
    value: &serde_yaml_ng::Value,
) -> Result<(), MmError> {
    match value {
        serde_yaml_ng::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                reject_compose_yaml_tags(path, key)?;
                reject_compose_yaml_tags(path, value)?;
            }
        }
        serde_yaml_ng::Value::Sequence(sequence) => {
            for value in sequence {
                reject_compose_yaml_tags(path, value)?;
            }
        }
        serde_yaml_ng::Value::Tagged(_) => {
            return Err(MmError::Environment {
                message: format!(
                    "Compose file '{}' uses a YAML tag, which is outside the immutable compiler model",
                    path.display()
                ),
            });
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn mapping_value<'a>(
    mapping: &'a serde_yaml_ng::Mapping,
    key: &str,
) -> Option<&'a serde_yaml_ng::Value> {
    mapping.get(serde_yaml_ng::Value::String(key.to_owned()))
}

#[cfg(test)]
mod tests;
