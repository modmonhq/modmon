//! The containers of a Compose project, read from the engine's API.
//!
//! One list request by the project's label and one inspect request per
//! container, all at once on the engine's socket: a few milliseconds where
//! `compose ps` takes a hundred of CPU time, and no Compose file is
//! rendered to find them.

use futures_util::StreamExt as _;
use serde_json::Value;

use crate::adapters::engine_api::{EngineApi, query_value};
use crate::adapters::runtime::observe::{allowlisted_labels, compose_diagnostic_code};
use crate::domain::error::MmError;
use crate::domain::runtime::ContainerObservation;

/// How many containers are inspected at once.
const INSPECTED_AT_ONCE: usize = 16;
/// The time the engine reports for something that never happened.
const NEVER: &str = "0001-01-01T00:00:00Z";

/// A container as the engine's API describes it.
#[derive(Debug, Clone)]
pub struct EngineContainer {
    pub observation: ContainerObservation,
    /// Whether the container has a terminal: its log is then one raw
    /// stream instead of framed standard output and standard error.
    pub tty: bool,
}

/// Every container of the Compose project `project`, running or not, in
/// the order they were created. With `service`, only that service's.
pub async fn project_containers(
    api: &EngineApi,
    project: &str,
    service: Option<&str>,
) -> Result<Vec<EngineContainer>, MmError> {
    let mut labels = vec![format!("com.docker.compose.project={project}")];
    if let Some(service) = service {
        labels.push(format!("com.docker.compose.service={service}"));
    }
    let filters = serde_json::json!({ "label": labels }).to_string();
    let listed = api
        .json(&format!(
            "/containers/json?all=true&filters={}",
            query_value(&filters)
        ))
        .await?;
    let rows = listed.as_array().cloned().unwrap_or_default();
    let mut containers = futures_util::stream::iter(rows.into_iter().map(|row| async move {
        let Some(id) = row["Id"].as_str() else {
            return Ok(None);
        };
        // A container removed since it was listed is simply not there.
        let inspected = api.json_if_found(&format!("/containers/{id}/json")).await?;
        Ok::<_, MmError>(inspected.and_then(|inspected| container(&row, &inspected)))
    }))
    .buffered(INSPECTED_AT_ONCE)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    containers.sort_by(|left, right| {
        (&left.observation.created_at, &left.observation.name)
            .cmp(&(&right.observation.created_at, &right.observation.name))
    });
    Ok(containers)
}

/// The inspect document of every container the engine's `filters` select,
/// running or not: what `docker container inspect` prints for the
/// containers `docker ps --all --filter …` lists, with no process started.
pub async fn inspected_containers(api: &EngineApi, filters: &Value) -> Result<Vec<Value>, MmError> {
    let listed = api
        .json(&format!(
            "/containers/json?all=true&filters={}",
            query_value(&filters.to_string())
        ))
        .await?;
    let ids = listed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["Id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    Ok(
        futures_util::stream::iter(ids.into_iter().map(|id| async move {
            // A container removed since it was listed is simply not there.
            api.json_if_found(&format!("/containers/{id}/json")).await
        }))
        .buffered(INSPECTED_AT_ONCE)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect(),
    )
}

/// One container from its row in the list and its inspect document.
fn container(row: &Value, inspected: &Value) -> Option<EngineContainer> {
    let text = |value: &Value| value.as_str().map(str::to_owned);
    let happened = |value: &Value| text(value).filter(|time| time != NEVER && !time.is_empty());
    let labels = inspected["Config"]["Labels"]
        .as_object()
        .map(|labels| {
            labels
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_owned()))
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let service = labels.get("com.docker.compose.service")?.clone();
    let state = &inspected["State"];
    let status = text(&row["Status"]).unwrap_or_default();
    let state_name = text(&state["Status"]).unwrap_or_else(|| "unknown".to_owned());
    let health =
        text(&state["Health"]["Status"]).filter(|health| !health.is_empty() && health != "none");
    let running = state["Running"].as_bool().unwrap_or(false);
    let image = text(&inspected["Config"]["Image"]).unwrap_or_default();
    let digest = image
        .rsplit_once('@')
        .map(|(_, digest)| digest.to_owned())
        .filter(|digest| digest.starts_with("sha256:"));
    Some(EngineContainer {
        tty: inspected["Config"]["Tty"].as_bool().unwrap_or(false),
        observation: ContainerObservation {
            id: text(&inspected["Id"])?,
            name: text(&inspected["Name"])?.trim_start_matches('/').to_owned(),
            service,
            image,
            diagnostic_code: compose_diagnostic_code(&state_name, &status, health.as_deref()),
            state: state_name,
            status,
            health,
            restart_count: inspected["RestartCount"].as_u64(),
            exit_code: if running {
                None
            } else {
                state["ExitCode"]
                    .as_i64()
                    .and_then(|code| match i32::try_from(code) {
                        Ok(code) => Some(code),
                        Err(error) => {
                            tracing::debug!(code, %error, "ignoring invalid observed exit code");
                            None
                        }
                    })
            },
            created_at: happened(&inspected["Created"]),
            started_at: happened(&state["StartedAt"]),
            finished_at: if running {
                None
            } else {
                happened(&state["FinishedAt"])
            },
            revision: None,
            digest,
            error: text(&state["Error"]).filter(|error| !error.is_empty()),
            labels: allowlisted_labels(labels),
            node: None,
            history: false,
        },
    })
}

/// The engine's version, as `docker --version` would name it.
/// The engine's API version and the kernel it runs on, as it says.
pub async fn engine_versions(api: &EngineApi) -> Result<(String, String), MmError> {
    let version = api.json("/version").await?;
    let text = |key: &str| version[key].as_str().unwrap_or_default().to_owned();
    Ok((text("ApiVersion"), text("KernelVersion")))
}

pub async fn engine_version(api: &EngineApi) -> Result<String, MmError> {
    let version = api.json("/version").await?;
    Ok(format!(
        "Docker version {}, build {}",
        version["Version"].as_str().unwrap_or("unknown"),
        version["GitCommit"].as_str().unwrap_or("unknown"),
    ))
}
