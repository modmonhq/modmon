use super::*;

pub(super) fn infer_compose_target_ports(
    document: &ConfigDocument,
    stack_name: &str,
    services: &BTreeSet<String>,
    source_files: &[CompiledComposeSource],
) -> Result<BTreeMap<String, ServiceHttpEvidence>, MmError> {
    if services.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut effective = services
        .iter()
        .cloned()
        .map(|service| (service, EffectiveServicePorts::default()))
        .collect::<BTreeMap<_, _>>();

    // Compose appends expose and ports entries across ordered files. YAML reset/override
    // tags are rejected earlier, so the target-port projection is their ordered union.
    for source in source_files {
        merge_source_ports(document, stack_name, services, source, &mut effective)?;
    }

    effective
        .into_iter()
        .map(|(service, state)| {
            let candidates = state
                .targets
                .into_iter()
                .map(|((port, protocol), sources)| HttpPortCandidate {
                    port,
                    protocol,
                    sources: sources.into_iter().collect(),
                })
                .collect();
            Ok((
                service,
                ServiceHttpEvidence {
                    declared_in_compose: state.declared,
                    candidates,
                    problem: None,
                },
            ))
        })
        .collect()
}

fn merge_source_ports(
    document: &ConfigDocument,
    stack_name: &str,
    services: &BTreeSet<String>,
    source: &CompiledComposeSource,
    effective: &mut BTreeMap<String, EffectiveServicePorts>,
) -> Result<(), MmError> {
    let path = document.root.join(&source.manifest.path);
    let mut compose =
        serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&source.bytes).map_err(|error| {
            MmError::Environment {
                message: format!("Compose file '{}' is invalid YAML: {error}", path.display()),
            }
        })?;
    compose
        .apply_merge()
        .map_err(|error| MmError::Environment {
            message: format!(
                "Compose file '{}' has invalid YAML merge keys: {error}",
                path.display()
            ),
        })?;
    let Some(root) = compose.as_mapping() else {
        return Err(port_inference_error(
            stack_name,
            &services.iter().cloned().collect::<Vec<_>>().join(", "),
            &format!("Compose file '{}' is not a mapping", path.display()),
        ));
    };
    let Some(service_value) = mapping_value(root, "services") else {
        return Ok(());
    };
    let Some(compose_services) = service_value.as_mapping() else {
        return Err(port_inference_error(
            stack_name,
            &services.iter().cloned().collect::<Vec<_>>().join(", "),
            &format!(
                "Compose file '{}' does not define services as a mapping",
                path.display()
            ),
        ));
    };
    for service_name in services {
        let Some(service_value) =
            compose_services.get(serde_yaml_ng::Value::String(service_name.clone()))
        else {
            continue;
        };
        let Some(service) = service_value.as_mapping() else {
            return Err(port_inference_error(
                stack_name,
                service_name,
                &format!(
                    "Compose file '{}' does not define the service as a mapping",
                    path.display()
                ),
            ));
        };
        let state = effective.entry(service_name.clone()).or_default();
        state.declared = true;
        if mapping_value(service, "extends").is_some() {
            return Err(port_inference_error(
                stack_name,
                service_name,
                &format!(
                    "Compose file '{}' uses extends, so inherited target ports cannot be established conservatively",
                    path.display()
                ),
            ));
        }
        if let Some(expose) = mapping_value(service, "expose") {
            collect_exposed_ports(expose, &path, service_name, &mut state.targets)
                .map_err(|detail| port_inference_error(stack_name, service_name, &detail))?;
        }
        if let Some(ports) = mapping_value(service, "ports") {
            collect_published_target_ports(ports, &path, service_name, &mut state.targets)
                .map_err(|detail| port_inference_error(stack_name, service_name, &detail))?;
        }
    }
    Ok(())
}

pub(super) fn collect_exposed_ports(
    value: &serde_yaml_ng::Value,
    path: &Path,
    service: &str,
    targets: &mut BTreeMap<(u16, TransportProtocol), BTreeSet<HttpEvidenceSource>>,
) -> Result<(), String> {
    let entries = value.as_sequence().ok_or_else(|| {
        format!(
            "Compose file '{}' service '{service}' expose must be a sequence",
            path.display()
        )
    })?;
    for (index, entry) in entries.iter().enumerate() {
        let (ports, protocol) = if let Some(port) = entry.as_u64() {
            (
                vec![valid_target_port(port).map_err(|detail| {
                    format!(
                        "Compose file '{}' service '{service}' expose entry {} {detail}",
                        path.display(),
                        index + 1
                    )
                })?],
                TransportProtocol::Tcp,
            )
        } else if let Some(port) = entry.as_str() {
            let (port, protocol) = split_port_protocol(port).map_err(|detail| {
                format!(
                    "Compose file '{}' service '{service}' expose entry {} {detail}",
                    path.display(),
                    index + 1
                )
            })?;
            (
                parse_port_range(port).map_err(|detail| {
                    format!(
                        "Compose file '{}' service '{service}' expose entry {} {detail}",
                        path.display(),
                        index + 1
                    )
                })?,
                protocol.unwrap_or(TransportProtocol::Tcp),
            )
        } else {
            return Err(format!(
                "Compose file '{}' service '{service}' expose entry {} is not a numeric short-syntax target port",
                path.display(),
                index + 1
            ));
        };
        for port in ports {
            targets
                .entry((port, protocol))
                .or_default()
                .insert(HttpEvidenceSource::ComposeExpose);
        }
    }
    Ok(())
}

pub(super) fn collect_published_target_ports(
    value: &serde_yaml_ng::Value,
    path: &Path,
    service: &str,
    targets: &mut BTreeMap<(u16, TransportProtocol), BTreeSet<HttpEvidenceSource>>,
) -> Result<(), String> {
    let entries = value.as_sequence().ok_or_else(|| {
        format!(
            "Compose file '{}' service '{service}' ports must be a sequence",
            path.display()
        )
    })?;
    for (index, entry) in entries.iter().enumerate() {
        let (parsed, protocol) = if let Some(port) = entry.as_u64() {
            (
                vec![valid_target_port(port).map_err(|detail| {
                    format!(
                        "Compose file '{}' service '{service}' ports entry {} {detail}",
                        path.display(),
                        index + 1
                    )
                })?],
                TransportProtocol::Tcp,
            )
        } else if let Some(port) = entry.as_str() {
            let (port, protocol) = split_port_protocol(port).map_err(|detail| {
                format!(
                    "Compose file '{}' service '{service}' ports entry {} {detail}",
                    path.display(),
                    index + 1
                )
            })?;
            (
                parse_short_port_without_protocol(port).map_err(|detail| {
                    format!(
                        "Compose file '{}' service '{service}' ports entry {} {detail}",
                        path.display(),
                        index + 1
                    )
                })?,
                protocol.unwrap_or(TransportProtocol::Tcp),
            )
        } else if let Some(port) = entry.as_mapping() {
            let target = mapping_value(port, "target").ok_or_else(|| {
                format!(
                    "Compose file '{}' service '{service}' ports entry {} omits long-syntax target",
                    path.display(),
                    index + 1
                )
            })?;
            let protocol = mapping_value(port, "protocol")
                .and_then(serde_yaml_ng::Value::as_str)
                .map(parse_protocol)
                .transpose()
                .map_err(|detail| {
                    format!(
                        "Compose file '{}' service '{service}' ports entry {} {detail}",
                        path.display(),
                        index + 1
                    )
                })?
                .unwrap_or(TransportProtocol::Tcp);
            (
                vec![parse_long_target_port(target).map_err(|detail| {
                    format!(
                        "Compose file '{}' service '{service}' ports entry {} {detail}",
                        path.display(),
                        index + 1
                    )
                })?],
                protocol,
            )
        } else {
            return Err(format!(
                "Compose file '{}' service '{service}' ports entry {} is not supported short or long syntax",
                path.display(),
                index + 1
            ));
        };
        for port in parsed {
            targets
                .entry((port, protocol))
                .or_default()
                .insert(HttpEvidenceSource::ComposePublishedPort);
        }
    }
    Ok(())
}

pub(super) fn parse_short_port_without_protocol(value: &str) -> Result<Vec<u16>, String> {
    let (prefix, target) = value
        .rsplit_once(':')
        .map_or((None, value), |(prefix, target)| (Some(prefix), target));
    if let Some(prefix) = prefix {
        if prefix.is_empty() {
            return Err("has an empty published-port prefix".to_owned());
        }
        if let Some((host, published)) = prefix.rsplit_once(':') {
            if host.is_empty() || published.is_empty() {
                return Err("has an incomplete host-IP/published-port prefix".to_owned());
            }
            validate_short_host_ip(host)?;
            parse_port_range(published)?;
        } else {
            parse_port_range(prefix)?;
        }
    }
    parse_port_range(target)
}

pub(super) fn parse_long_target_port(value: &serde_yaml_ng::Value) -> Result<u16, String> {
    if let Some(port) = value.as_u64() {
        return valid_target_port(port);
    }
    let port = value
        .as_str()
        .ok_or_else(|| "has a non-numeric long-syntax target".to_owned())?;
    if port.contains(['/', '-']) {
        return Err("has a non-scalar long-syntax target".to_owned());
    }
    parse_port_number(port)
}

pub(super) fn split_port_protocol(
    value: &str,
) -> Result<(&str, Option<TransportProtocol>), String> {
    if value.is_empty()
        || value.contains('$')
        || value.trim() != value
        || value.chars().any(char::is_whitespace)
    {
        return Err("contains an empty, dynamic, or whitespace-bearing port expression".to_owned());
    }
    let mut parts = value.split('/');
    let port = parts.next().unwrap_or_default();
    let protocol = parts.next();
    if parts.next().is_some() {
        return Err("uses an unsupported port protocol".to_owned());
    }
    Ok((port, protocol.map(parse_protocol).transpose()?))
}

pub(super) fn parse_protocol(value: &str) -> Result<TransportProtocol, String> {
    match value {
        "tcp" => Ok(TransportProtocol::Tcp),
        "udp" => Ok(TransportProtocol::Udp),
        "sctp" => Ok(TransportProtocol::Sctp),
        _ => Err("uses an unsupported port protocol".to_owned()),
    }
}

pub(super) const fn protocol_label(protocol: TransportProtocol) -> &'static str {
    match protocol {
        TransportProtocol::Tcp => "tcp",
        TransportProtocol::Udp => "udp",
        TransportProtocol::Sctp => "sctp",
    }
}

pub(super) fn parse_port_range(value: &str) -> Result<Vec<u16>, String> {
    if let Some((start, end)) = value.split_once('-') {
        if end.contains('-') {
            return Err("has an invalid port range".to_owned());
        }
        let start = parse_port_number(start)?;
        let end = parse_port_number(end)?;
        if start > end {
            return Err("has a descending port range".to_owned());
        }
        return Ok((start..=end).collect());
    }
    parse_port_number(value).map(|port| vec![port])
}

pub(super) fn parse_port_number(value: &str) -> Result<u16, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("has a non-numeric port".to_owned());
    }
    let port = value
        .parse::<u16>()
        .map_err(|_| "has a port outside 1..=65535".to_owned())?;
    if port == 0 {
        return Err("has a zero target port".to_owned());
    }
    Ok(port)
}

pub(super) fn valid_target_port(value: u64) -> Result<u16, String> {
    let port = u16::try_from(value).map_err(|_| "has a port outside 1..=65535".to_owned())?;
    if port == 0 {
        return Err("has a zero target port".to_owned());
    }
    Ok(port)
}

pub(super) fn validate_short_host_ip(value: &str) -> Result<(), String> {
    let value = if let Some(value) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        value
    } else {
        if value.contains(':') {
            return Err("uses an unbracketed IPv6 host IP".to_owned());
        }
        value
    };
    value
        .parse::<std::net::IpAddr>()
        .map(|_| ())
        .map_err(|_| "has an invalid host IP".to_owned())
}

pub(super) fn port_inference_error(stack: &str, service: &str, detail: &str) -> MmError {
    MmError::Environment {
        message: format!(
            "cannot infer ingress port for service '{service}' in stack '{stack}': {detail}; declare exactly one target port with service.expose or service.ports, or set ingress.port explicitly"
        ),
    }
}

#[cfg(test)]
mod tests;
