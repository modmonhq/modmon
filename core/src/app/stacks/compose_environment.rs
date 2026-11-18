//! Environment values belong to `mm.yaml`, not to a Compose file.
//!
//! A service's variables come from its module's template and the service's
//! `env` in `mm.yaml`, so a Compose file that sets `environment` or
//! `env_file` is refused. `mm config validate` and the compilation of a
//! stack ask the same function, so a file validation accepts is not
//! refused later by the host that plans the deployment.

use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use super::*;

/// Why `service` of the Compose file `path` is refused, when it sets its
/// own environment.
pub(crate) fn compose_environment_refusal(
    path: &Path,
    service_name: &str,
    service: &Mapping,
) -> Option<String> {
    let key = ["environment", "env_file"]
        .into_iter()
        .find(|key| mapping_value(service, key).is_some())?;
    Some(format!(
        "Compose file '{}' declares environment or env_file: service '{service_name}' sets `{key}`; module templates and service env in mm.yaml are authoritative. Move the values to the service's `env:` in mm.yaml or to its module's environment template",
        path.display()
    ))
}

/// Every refusal in the Compose files of the environment's stacks, with
/// the file. A file that cannot be read or parsed is skipped: the stack
/// checks report it. Services that are not part of `environment` are left
/// out, as the compilation leaves them out before it looks.
pub(crate) fn compose_environment_refusals(
    document: &ConfigDocument,
    environment: &str,
) -> Vec<(PathBuf, String)> {
    let mut refusals = Vec::new();
    for (stack_name, stack) in &document.config.stacks {
        let excluded = excluded_service_names(document, environment, stack_name);
        for file in &stack.files {
            let path = document.root.join(file);
            let Ok(Some(bytes)) = read_regular_file(&path, MAX_COMPOSE_FILE_BYTES) else {
                continue;
            };
            let Ok(mut model) = serde_yaml_ng::from_slice::<Value>(&bytes) else {
                continue;
            };
            if model.apply_merge().is_err() {
                continue;
            }
            let Some(services) = model.get("services").and_then(Value::as_mapping) else {
                continue;
            };
            for (name, service) in services {
                let (Some(name), Some(service)) = (name.as_str(), service.as_mapping()) else {
                    continue;
                };
                if excluded.contains(name) {
                    continue;
                }
                if let Some(refusal) = compose_environment_refusal(&path, name, service) {
                    refusals.push((path.clone(), refusal));
                }
            }
        }
    }
    refusals
}
