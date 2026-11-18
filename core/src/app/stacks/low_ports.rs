//! Which services may open ports below 1024 as any user.

use std::collections::BTreeSet;

use serde_yaml_ng::Value;

use crate::domain::error::MmError;
use crate::domain::stack::CompiledComposeSource;

pub(super) const UNPRIVILEGED_PORT_START: &str = "net.ipv4.ip_unprivileged_port_start";

/// The services of the stack's Compose files that have a network namespace
/// of their own and do not set the sysctl.
pub(super) fn services_opening_low_ports(
    sources: &[CompiledComposeSource],
) -> Result<BTreeSet<String>, MmError> {
    let mut services = std::collections::BTreeMap::<String, bool>::new();
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        let Some(listed) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in listed {
            let Some(name) = name.as_str() else {
                continue;
            };
            let shares_network = service
                .get("network_mode")
                .and_then(Value::as_str)
                .is_some_and(|mode| {
                    mode == "host" || mode.starts_with("container:") || mode.starts_with("service:")
                });
            let sets_it = service.get("sysctls").is_some_and(|sysctls| {
                super::guardrails::sysctl_names(sysctls)
                    .iter()
                    .any(|sysctl| sysctl == UNPRIVILEGED_PORT_START)
            });
            *services.entry(name.to_owned()).or_default() |= shares_network || sets_it;
        }
    }
    Ok(services
        .into_iter()
        .filter(|(_, left_alone)| !left_alone)
        .map(|(name, _)| name)
        .collect())
}
