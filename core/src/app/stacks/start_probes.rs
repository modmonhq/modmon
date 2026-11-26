//! The generated overlay's quick health probes while containers start
//! (`domain::compose_healthcheck::start_probes`), on a Docker Engine that
//! honours them.

use serde_yaml_ng::{Mapping, Value};

use crate::domain::compose_healthcheck::start_probes;
use crate::domain::error::MmError;
use crate::domain::stack::CompiledComposeSource;

/// `overlay` with `start_interval` (and a `start_period` where the service
/// sets none) for each service of `sources` that has a health check. The
/// keys are only ever added; what the overlay sets stays, and a check the
/// overlay turns off stays off.
pub(super) fn with_start_probes(
    overlay: Vec<u8>,
    sources: &[CompiledComposeSource],
) -> Result<Vec<u8>, MmError> {
    let models = sources
        .iter()
        .filter_map(
            |source| match serde_yaml_ng::from_slice::<Value>(&source.bytes) {
                Ok(model) => Some(model),
                Err(error) => {
                    tracing::debug!(%error, "no start probes for a Compose file mm cannot read");
                    None
                }
            },
        )
        .collect::<Vec<_>>();
    let probes = start_probes(&models);
    if probes.is_empty() {
        return Ok(overlay);
    }
    let invalid = |error: serde_yaml_ng::Error| MmError::Environment {
        message: format!("mm's generated Compose overlay is not valid YAML: {error}"),
    };
    let mut document = serde_yaml_ng::from_slice::<Value>(&overlay).map_err(invalid)?;
    let Some(root) = document.as_mapping_mut() else {
        return Ok(overlay);
    };
    let services = root
        .entry(Value::String("services".to_owned()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(services) = services.as_mapping_mut() else {
        return Ok(overlay);
    };
    for (service, keys) in probes {
        let entry = services
            .entry(Value::String(service))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        let Some(entry) = entry.as_mapping_mut() else {
            continue;
        };
        let check = entry
            .entry(Value::String("healthcheck".to_owned()))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        let Some(check) = check.as_mapping_mut() else {
            continue;
        };
        if check.get("disable").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        for (key, value) in keys {
            check.entry(key).or_insert(value);
        }
    }
    serde_yaml_ng::to_string(&document)
        .map(String::into_bytes)
        .map_err(invalid)
}
