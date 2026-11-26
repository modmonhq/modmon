//! `environments.<name>.healthchecks: false`: the overlay turns off every
//! service's health check, the image's included, and a `depends_on` that
//! waits for `service_healthy` waits for `service_started` instead.

use serde::Serialize;
use serde_yaml_ng::Value;

use super::*;
use crate::domain::runtime::HEALTHCHECKS_LABEL;
use crate::domain::target::TargetTransport;

#[derive(Debug, Default, Serialize)]
pub(super) struct GeneratedHealth {
    #[serde(skip_serializing_if = "Option::is_none")]
    healthcheck: Option<DisabledCheck>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    depends_on: BTreeMap<String, StartedCondition>,
}

#[derive(Debug, Serialize)]
struct DisabledCheck {
    disable: bool,
}

#[derive(Debug, Serialize)]
struct StartedCondition {
    condition: &'static str,
}

pub(super) fn healthchecks_on(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
) -> bool {
    document.config.healthchecks_on(
        environment,
        target.definition.transport == TargetTransport::Local,
    )
}

/// The services of `sources` with their health checks off, and the label
/// that says so, when the environment turns them off.
pub(super) fn turn_off(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    sources: &[CompiledComposeSource],
) -> Result<BTreeMap<String, GeneratedHealth>, MmError> {
    let mut services = BTreeMap::<String, GeneratedHealth>::new();
    if healthchecks_on(document, environment, target) {
        return Ok(services);
    }
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        if let Some(root) = model.as_mapping_mut() {
            crate::domain::compose_extends::resolve_extends(root);
        }
        let Some(defined) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, definition) in defined {
            let Some(name) = name.as_str() else {
                continue;
            };
            let generated = services.entry(name.to_owned()).or_default();
            generated.healthcheck = Some(DisabledCheck { disable: true });
            let Some(Value::Mapping(depends)) = definition.get("depends_on") else {
                continue;
            };
            for (dependency, entry) in depends {
                if let Some(dependency) = dependency.as_str()
                    && entry.get("condition").and_then(Value::as_str) == Some("service_healthy")
                {
                    generated.depends_on.insert(
                        dependency.to_owned(),
                        StartedCondition {
                            condition: "service_started",
                        },
                    );
                }
            }
        }
    }
    Ok(services)
}

pub(super) const OFF: (&str, &str) = (HEALTHCHECKS_LABEL, "off");
