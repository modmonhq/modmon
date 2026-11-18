//! What `mm forward` needs besides the connection: the forwards saved on
//! this machine, and the ports a service declares.

use super::*;

/// Saved forwards, private to this machine.
pub(crate) fn forward_shortcuts(
    explicit_home: Option<&Path>,
) -> Result<crate::adapters::target::ForwardShortcutStore, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    Ok(crate::adapters::target::ForwardShortcutStore::new(
        home.root.join("forwards.json"),
    ))
}

/// Container ports a service declares: its `http-port`, then the container
/// side of Compose `ports` and `expose`, in that order.
pub(crate) fn declared_service_ports(start: &Path, service: &str) -> Result<Vec<u16>, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let mut ports = Vec::new();
    for module in document.config.modules.values() {
        if let Some(port) = module
            .services
            .get(service)
            .and_then(|config| config.http_port)
        {
            ports.push(port);
        }
    }
    for stack in document.config.stacks.values() {
        for file in &stack.files {
            let Ok(text) = std::fs::read_to_string(document.root.join(file)) else {
                continue;
            };
            let Ok(compose) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text) else {
                continue;
            };
            let Some(config) = compose
                .get("services")
                .and_then(|services| services.get(service))
            else {
                continue;
            };
            for entry in config
                .get("ports")
                .and_then(serde_yaml_ng::Value::as_sequence)
                .into_iter()
                .flatten()
            {
                let container = match entry {
                    serde_yaml_ng::Value::Mapping(long) => long
                        .get("target")
                        .and_then(serde_yaml_ng::Value::as_u64)
                        .map(|port| port.to_string()),
                    serde_yaml_ng::Value::Number(port) => Some(port.to_string()),
                    serde_yaml_ng::Value::String(short) => short
                        .split('/')
                        .next()
                        .and_then(|mapping| mapping.rsplit(':').next())
                        .map(str::to_owned),
                    _ => None,
                };
                if let Some(port) = container.and_then(|port| port.parse::<u16>().ok()) {
                    ports.push(port);
                }
            }
            for entry in config
                .get("expose")
                .and_then(serde_yaml_ng::Value::as_sequence)
                .into_iter()
                .flatten()
            {
                let port = match entry {
                    serde_yaml_ng::Value::Number(port) => {
                        port.as_u64().and_then(|port| u16::try_from(port).ok())
                    }
                    serde_yaml_ng::Value::String(port) => {
                        port.split('/').next().and_then(|port| port.parse().ok())
                    }
                    _ => None,
                };
                ports.extend(port);
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    ports.retain(|port| seen.insert(*port));
    Ok(ports)
}
