use std::collections::BTreeMap;

use serde_json::Value;

use crate::app::ports::CommandOutput;
use crate::domain::error::MmError;
use crate::domain::ownership::ResourceOwnership;

pub fn inspect_labels(value: &Value) -> Option<BTreeMap<String, String>> {
    let labels = value
        .get("Labels")
        .or_else(|| value.get("labels"))
        .or_else(|| value.get("Config").and_then(|config| config.get("Labels")))
        .or_else(|| value.get("config").and_then(|config| config.get("labels")))
        .or_else(|| value.get("Spec").and_then(|spec| spec.get("Labels")))
        .or_else(|| value.get("spec").and_then(|spec| spec.get("labels")));
    labels.and_then(Value::as_object).map(|labels| {
        labels
            .iter()
            .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
            .collect()
    })
}

pub fn parse_inspected_resource(
    output: CommandOutput,
    ownership: &ResourceOwnership,
    engine: &str,
) -> Result<Value, MmError> {
    if output.truncated {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "{engine} {} inspect output was truncated",
                ownership.kind.as_str()
            ),
        });
    }
    let mut rows = serde_json::from_str::<Vec<Value>>(&output.stdout).map_err(|error| {
        MmError::RuntimeResponse {
            message: format!(
                "{engine} returned invalid {} inspect JSON: {error}",
                ownership.kind.as_str()
            ),
        }
    })?;
    if rows.len() != 1 {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "{engine} {} inspect returned {} resources for '{}'",
                ownership.kind.as_str(),
                rows.len(),
                ownership.logical_name
            ),
        });
    }
    let inspected = rows.pop().ok_or_else(|| MmError::RuntimeResponse {
        message: format!(
            "{engine} {} inspect returned no resource for '{}'",
            ownership.kind.as_str(),
            ownership.logical_name
        ),
    })?;
    let actual_name = inspected
        .get("Name")
        .or_else(|| inspected.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| MmError::RuntimeResponse {
            message: format!(
                "{engine} {} inspect omitted the resource name",
                ownership.kind.as_str()
            ),
        })?;
    let labels = inspect_labels(&inspected);
    ownership.require_owned(actual_name, labels.as_ref())?;
    Ok(inspected)
}

pub fn ownership_label_args(ownership: &ResourceOwnership) -> Vec<String> {
    let mut args = Vec::new();
    for (key, value) in ownership.labels() {
        args.push("--label".to_owned());
        args.push(format!("{key}={value}"));
    }
    args
}

pub fn parse_resource_ids(
    output: &CommandOutput,
    engine: &str,
    resource: &str,
) -> Result<Vec<String>, MmError> {
    if output.truncated {
        return Err(MmError::RuntimeResponse {
            message: format!("{engine} {resource} list output was truncated"),
        });
    }
    let trimmed = output.stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let rows = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<Value>>(trimmed).map_err(|error| MmError::RuntimeResponse {
            message: format!("{engine} returned invalid {resource} list JSON: {error}"),
        })?
    } else {
        trimmed
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<Value>(line).map_err(|error| MmError::RuntimeResponse {
                    message: format!("{engine} returned invalid {resource} list JSON: {error}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    rows.iter()
        .map(|row| {
            row.get("ID")
                .or_else(|| row.get("Id"))
                .or_else(|| row.get("id"))
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| MmError::RuntimeResponse {
                    message: format!("{engine} {resource} list omitted a stable ID"),
                })
        })
        .collect()
}

pub fn require_inspected_project_ownership(
    output: CommandOutput,
    ownership: &ResourceOwnership,
    engine: &str,
) -> Result<(), MmError> {
    if output.truncated {
        return Err(MmError::RuntimeResponse {
            message: format!("{engine} project resource inspect output was truncated"),
        });
    }
    let rows = serde_json::from_str::<Vec<Value>>(&output.stdout).map_err(|error| {
        MmError::RuntimeResponse {
            message: format!("{engine} returned invalid project resource JSON: {error}"),
        }
    })?;
    if rows.len() != 1 {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "{engine} inspect returned {} project resources for '{}'",
                rows.len(),
                ownership.logical_name
            ),
        });
    }
    let labels = rows.first().and_then(inspect_labels);
    ownership.require_owned(&ownership.logical_name, labels.as_ref())
}

#[cfg(test)]
#[path = "inspect/tests.rs"]
mod tests;
