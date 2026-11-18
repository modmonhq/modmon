use std::collections::BTreeMap;

use serde_json::Value;

use crate::app::ports::CommandOutput;
use crate::domain::error::MmError;
use crate::domain::runtime::{
    ContainerObservation, RenderedResourceSemantics, RenderedServiceSemantics, RuntimeTarget,
    ServiceIdentity, ServiceObservation,
};

pub fn parse_compose_observation(
    output: &CommandOutput,
    engine: &str,
) -> Result<Vec<ContainerObservation>, MmError> {
    if output.truncated {
        return Err(MmError::RuntimeResponse {
            message: format!("{engine} container list exceeded the output limit"),
        });
    }
    let records = parse_json_records(&output.stdout, engine, "container")?;
    let mut containers = Vec::with_capacity(records.len());
    for record in &records {
        containers.push(
            container_from_json(record).ok_or_else(|| MmError::RuntimeResponse {
                message: format!(
                    "{engine} returned a container record without a Compose service identity"
                ),
            })?,
        );
    }
    containers.sort_by(|left, right| {
        left.service
            .cmp(&right.service)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(containers)
}

pub fn parse_compose_service_names(config: &str) -> Result<Vec<String>, MmError> {
    let names = compose_service_names(config)?;
    if names.is_empty() {
        return Err(MmError::RuntimeResponse {
            message: "rendered Compose configuration has no services".to_owned(),
        });
    }
    Ok(names)
}

pub fn compose_service_names(config: &str) -> Result<Vec<String>, MmError> {
    let parsed: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(config).map_err(|error| invalid_rendered_yaml(config, &error))?;
    let mut names = parsed
        .get("services")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .map(|services| {
            services
                .keys()
                .filter_map(|key| key.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    names.dedup();
    Ok(names)
}

fn invalid_rendered_yaml(config: &str, error: &serde_yaml_ng::Error) -> MmError {
    let line = error
        .location()
        .and_then(|location| config.lines().nth(location.line().saturating_sub(1)))
        .map(|line| format!(" (the line, values masked: `{}`)", mask_yaml_line(line)))
        .unwrap_or_default();
    MmError::RuntimeResponse {
        message: format!("rendered Compose configuration is not valid YAML: {error}{line}"),
    }
}

fn mask_yaml_line(line: &str) -> String {
    let indent = line.len() - line.trim_start().len();
    let body = &line[indent..];
    let shown = match body.split_once(':') {
        Some((key, rest)) if !key.contains(' ') || key.starts_with("- ") => {
            if rest.trim().is_empty() {
                format!("{key}:")
            } else {
                format!("{key}: …")
            }
        }
        _ if body.is_empty() => String::new(),
        _ => "…".to_owned(),
    };
    format!("{}{shown}", &line[..indent])
}

#[cfg(test)]
pub fn hash_rendered_config(project_name: &str, rendered: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(project_name.as_bytes());
    hasher.update(&[0]);
    hasher.update(rendered.as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub struct NormalizedRenderedSemantics {
    pub services: BTreeMap<String, RenderedServiceSemantics>,
    pub resources: Vec<RenderedResourceSemantics>,
    pub config_hash: String,
}

pub fn normalized_rendered_semantics(
    project_name: &str,
    rendered: &str,
) -> Result<NormalizedRenderedSemantics, MmError> {
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(rendered)
        .map_err(|error| invalid_rendered_yaml(rendered, &error))?;
    let services = value
        .get("services")
        .and_then(serde_yaml_ng::Value::as_mapping)
        .map(|mapping| {
            mapping
                .iter()
                .filter_map(|(name, service)| {
                    Some((
                        name.as_str()?.to_owned(),
                        normalize_service_semantics(service),
                    ))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut resources = normalize_resources(
        &value,
        "networks",
        crate::domain::ownership::ResourceKind::Network,
    );
    resources.extend(normalize_resources(
        &value,
        "volumes",
        crate::domain::ownership::ResourceKind::Volume,
    ));
    resources.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.logical_name.cmp(&right.logical_name))
    });
    let canonical =
        serde_json::to_vec(&(project_name, &services, &resources)).map_err(|error| {
            MmError::RuntimeResponse {
                message: format!("could not canonicalize rendered runtime semantics: {error}"),
            }
        })?;
    Ok(NormalizedRenderedSemantics {
        services,
        resources,
        config_hash: blake3::hash(&canonical).to_hex().to_string(),
    })
}

fn normalize_service_semantics(value: &serde_yaml_ng::Value) -> RenderedServiceSemantics {
    let mut environment_names = Vec::new();
    if let Some(environment) = value.get("environment") {
        if let Some(mapping) = environment.as_mapping() {
            environment_names.extend(
                mapping
                    .keys()
                    .filter_map(|name| name.as_str().map(str::to_owned)),
            );
        } else if let Some(sequence) = environment.as_sequence() {
            environment_names.extend(sequence.iter().filter_map(|entry| {
                entry
                    .as_str()
                    .and_then(|entry| {
                        entry
                            .split_once('=')
                            .map_or(Some(entry), |(name, _)| Some(name))
                    })
                    .map(str::to_owned)
            }));
        }
    }
    environment_names.sort();
    environment_names.dedup();
    let mut networks = Vec::new();
    let mut aliases = BTreeMap::new();
    if let Some(network_value) = value.get("networks") {
        if let Some(mapping) = network_value.as_mapping() {
            for (network, settings) in mapping {
                let Some(network) = network.as_str() else {
                    continue;
                };
                networks.push(network.to_owned());
                let mut names = settings
                    .get("aliases")
                    .and_then(serde_yaml_ng::Value::as_sequence)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|alias| alias.as_str().map(str::to_owned))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                names.sort();
                names.dedup();
                if !names.is_empty() {
                    aliases.insert(network.to_owned(), names);
                }
            }
        } else if let Some(sequence) = network_value.as_sequence() {
            networks.extend(
                sequence
                    .iter()
                    .filter_map(|network| network.as_str().map(str::to_owned)),
            );
        }
    }
    networks.sort();
    networks.dedup();
    let deploy = value.get("deploy");
    RenderedServiceSemantics {
        image: value
            .get("image")
            .and_then(serde_yaml_ng::Value::as_str)
            .map(str::to_owned),
        environment_names,
        networks,
        aliases,
        healthcheck: value.get("healthcheck").is_some(),
        replicas: deploy
            .and_then(|deploy| deploy.get("replicas"))
            .and_then(serde_yaml_ng::Value::as_u64)
            .and_then(|replicas| match u32::try_from(replicas) {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::debug!(replicas, %error, "ignoring invalid observed replica count");
                    None
                }
            }),
        update_order: nested_string(deploy, &["update_config", "order"]),
        restart_condition: nested_string(deploy, &["restart_policy", "condition"]),
        placement_constraints: deploy
            .and_then(|deploy| deploy.get("placement"))
            .and_then(|placement| placement.get("constraints"))
            .and_then(serde_yaml_ng::Value::as_sequence)
            .map(|values| {
                let mut values = values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect::<Vec<_>>();
                values.sort();
                values.dedup();
                values
            })
            .unwrap_or_default(),
    }
}

fn nested_string(value: Option<&serde_yaml_ng::Value>, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value?, |current, key| current.get(*key))
        .and_then(serde_yaml_ng::Value::as_str)
        .map(str::to_owned)
}

fn normalize_resources(
    root: &serde_yaml_ng::Value,
    field: &str,
    kind: crate::domain::ownership::ResourceKind,
) -> Vec<RenderedResourceSemantics> {
    root.get(field)
        .and_then(serde_yaml_ng::Value::as_mapping)
        .map(|mapping| {
            mapping
                .iter()
                .filter_map(|(logical_name, value)| {
                    let logical_name = logical_name.as_str()?.to_owned();
                    let physical_name = value
                        .get("name")
                        .and_then(serde_yaml_ng::Value::as_str)
                        .unwrap_or(&logical_name)
                        .to_owned();
                    let external = value
                        .get("external")
                        .and_then(serde_yaml_ng::Value::as_bool)
                        .unwrap_or(false);
                    let ownership_labels = value
                        .get("labels")
                        .and_then(serde_yaml_ng::Value::as_mapping)
                        .map(|labels| {
                            labels
                                .iter()
                                .filter_map(|(key, value)| {
                                    let key = key.as_str()?;
                                    if key.starts_with("io.mm.") {
                                        Some((key.to_owned(), value.as_str()?.to_owned()))
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(RenderedResourceSemantics {
                        kind,
                        logical_name,
                        physical_name,
                        external,
                        ownership_labels,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn normalize_observation(
    target: &RuntimeTarget,
    expected_services: &[String],
    containers: &[ContainerObservation],
) -> Vec<ServiceObservation> {
    let mut names = expected_services
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    names.extend(
        containers
            .iter()
            .filter(|container| !container.is_one_off())
            .map(|container| container.service.clone()),
    );
    names
        .into_iter()
        .map(|service| {
            let all = containers
                .iter()
                .filter(|container| container.service == service)
                .collect::<Vec<_>>();
            // Containers that a newer one replaced say nothing about the
            // service as it is now.
            let tally = crate::domain::runtime::tally_service(&all);
            let members = crate::domain::runtime::current_containers(&all);
            ServiceObservation {
                identity: ServiceIdentity {
                    project_id: target.project_id,
                    environment: target.environment.clone(),
                    stack: target.stack_name.clone(),
                    module: None,
                    service,
                },
                desired_count: tally.desired_count,
                running_count: tally.running_count,
                healthy_count: tally.healthy_count,
                state: tally.state,
                exit_code: members.iter().find_map(|container| container.exit_code),
                created_at: members
                    .iter()
                    .filter_map(|container| container.created_at.clone())
                    .min(),
                started_at: members
                    .iter()
                    .filter_map(|container| container.started_at.clone())
                    .min(),
                finished_at: members
                    .iter()
                    .filter_map(|container| container.finished_at.clone())
                    .max(),
                revision: members
                    .iter()
                    .filter_map(|container| container.revision.clone())
                    .max(),
                digest: members
                    .iter()
                    .find_map(|container| container.digest.clone()),
                diagnostic_code: tally.diagnostic_code,
            }
        })
        .collect()
}

fn parse_json_records(stdout: &str, engine: &str, kind: &str) -> Result<Vec<Value>, MmError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).map_err(|error| MmError::RuntimeResponse {
            message: format!("{engine} returned invalid {kind} JSON: {error}"),
        });
    }
    let mut records = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        records.push(
            serde_json::from_str(line).map_err(|error| MmError::RuntimeResponse {
                message: format!("{engine} returned invalid {kind} JSON: {error}"),
            })?,
        );
    }
    Ok(records)
}

fn container_from_json(value: &Value) -> Option<ContainerObservation> {
    let all_labels = labels_from_value(value);
    let service = all_labels
        .get("com.docker.compose.service")
        .cloned()
        .or_else(|| string_for_keys(value, &["Service", "service"]))?;
    let labels = allowlisted_labels(all_labels);
    let status = string_for_keys(value, &["Status", "status"]).unwrap_or_default();
    let health = string_for_keys(value, &["Health", "health"])
        .filter(|health| !health.is_empty())
        .or_else(|| health_from_status(&status));
    let state = string_for_keys(value, &["State", "state"]).unwrap_or_else(|| "unknown".to_owned());
    let diagnostic_code = compose_diagnostic_code(&state, &status, health.as_deref());
    Some(ContainerObservation {
        id: string_for_keys(value, &["Id", "ID", "id"]).unwrap_or_default(),
        name: name_for_container(value),
        service,
        image: string_for_keys(value, &["Image", "image"]).unwrap_or_default(),
        state,
        status,
        health,
        // `podman ps` says `Restarts`.
        restart_count: unsigned_for_keys(value, &["RestartCount", "restart_count", "Restarts"]),
        exit_code: integer_for_keys(value, &["ExitCode", "exit_code"]),
        created_at: string_for_keys(value, &["CreatedAt", "created_at"]),
        started_at: string_for_keys(value, &["StartedAt", "started_at"]),
        finished_at: string_for_keys(value, &["FinishedAt", "finished_at"]),
        revision: string_for_keys(value, &["Version", "version"]),
        digest: digest_from_image(value, &["Image", "image"]),
        diagnostic_code,
        error: string_for_keys(value, &["Error", "error"]).filter(|value| !value.is_empty()),
        labels,
        node: None,
        history: false,
    })
}

fn labels_from_value(value: &Value) -> BTreeMap<String, String> {
    value
        .get("Labels")
        .or_else(|| value.get("labels"))
        .and_then(Value::as_object)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn allowlisted_labels(labels: BTreeMap<String, String>) -> BTreeMap<String, String> {
    labels
        .into_iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "com.docker.compose.project"
                    | "com.docker.compose.service"
                    | "com.docker.compose.container-number"
                    | "com.docker.compose.oneoff"
                    | "io.podman.compose.project"
                    | "io.podman.compose.service"
            )
        })
        .collect()
}

pub(crate) fn health_from_status(status: &str) -> Option<String> {
    let status = status.to_ascii_lowercase();
    if status.contains("unhealthy") {
        Some("unhealthy".to_owned())
    } else if status.contains("health: starting") || status.contains("(starting)") {
        Some("starting".to_owned())
    } else if status.contains("(healthy)") {
        Some("healthy".to_owned())
    } else {
        None
    }
}

fn name_for_container(value: &Value) -> String {
    let names = value.get("Names").or_else(|| value.get("names"));
    if let Some(name) = names.and_then(Value::as_str) {
        return name.to_owned();
    }
    names
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn string_for_keys(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn integer_for_keys(value: &Value, keys: &[&str]) -> Option<i32> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            if let Some(number) = value.as_i64() {
                return match i32::try_from(number) {
                    Ok(number) => Some(number),
                    Err(error) => {
                        tracing::debug!(key, number, %error, "ignoring invalid observed integer");
                        None
                    }
                };
            }
            value.as_str().and_then(|number| match number.parse() {
                Ok(number) => Some(number),
                Err(error) => {
                    tracing::debug!(key, number, %error, "ignoring invalid observed integer");
                    None
                }
            })
        })
    })
}

fn unsigned_for_keys(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            value.as_u64().or_else(|| {
                value.as_str().and_then(|number| match number.parse() {
                    Ok(number) => Some(number),
                    Err(error) => {
                        tracing::debug!(key, number, %error, "ignoring invalid observed unsigned integer");
                        None
                    }
                })
            })
        })
    })
}

fn digest_from_image(value: &Value, keys: &[&str]) -> Option<String> {
    string_for_keys(value, keys).and_then(|image| {
        image
            .rsplit_once('@')
            .map(|(_, digest)| digest.to_owned())
            .filter(|digest| digest.starts_with("sha256:"))
    })
}

pub(crate) fn compose_diagnostic_code(
    state: &str,
    status: &str,
    health: Option<&str>,
) -> Option<String> {
    if health.is_some_and(|health| health.eq_ignore_ascii_case("unhealthy"))
        || status.to_ascii_lowercase().contains("unhealthy")
    {
        Some("MM_RUNTIME_UNHEALTHY".to_owned())
    } else if state.eq_ignore_ascii_case("dead") || state.eq_ignore_ascii_case("removing") {
        Some("MM_RUNTIME_CONTAINER_FAILED".to_owned())
    } else {
        None
    }
}

#[cfg(test)]
#[path = "observe/tests.rs"]
mod tests;
