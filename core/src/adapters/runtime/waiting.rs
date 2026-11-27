//! The containers of a node that are not running although they should be,
//! for the node's storage keeper: which ones carry a label, what state
//! they are in, and what the engine said when they last failed to start.
//!
//! Two engine commands (`ps` by label and state, then one `inspect`), and
//! only the first when nothing waits. Both engines print the same inspect
//! document for what is read here. Where the engine's API is read
//! (`EngineApi`), the list and the documents come from its socket and no
//! process is started for a look.

use std::time::Duration;

use serde_json::Value;

use crate::adapters::engine_api::EngineApi;
use crate::adapters::runtime::engine_containers::inspected_containers;
use crate::adapters::runtime::project_services::ContainerEngine;
use crate::domain::error::MmError;
use crate::domain::shared_storage::WaitingContainer;

const TIMEOUT: Duration = Duration::from_secs(30);
/// Mounting a volume from a host that does not answer takes the engine
/// about half a minute to give up; starting a container may take longer.
const START_TIMEOUT: Duration = Duration::from_secs(120);

/// Every container that carries `label` and is created or exited.
pub(crate) async fn waiting_containers(
    engine: &dyn ContainerEngine,
    api: Option<&EngineApi>,
    label: &str,
) -> Result<Vec<WaitingContainer>, MmError> {
    if let Some(api) = api {
        let filters = serde_json::json!({ "label": [label], "status": ["created", "exited"] });
        return Ok(inspected_containers(api, &filters)
            .await?
            .iter()
            .filter_map(|row| container(row, label))
            .collect());
    }
    let listed = engine
        .engine(
            [
                "ps",
                "--all",
                "--quiet",
                "--no-trunc",
                "--filter",
                &format!("label={label}"),
                "--filter",
                "status=created",
                "--filter",
                "status=exited",
            ]
            .map(str::to_owned)
            .to_vec(),
            TIMEOUT,
        )
        .await?;
    let ids = listed
        .stdout
        .lines()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let inspected =
        crate::adapters::runtime::project_services::inspect_containers(engine, &ids, TIMEOUT)
            .await?;
    parse_waiting(inspected.trim(), label, engine.engine_name())
}

/// Reads the engine's inspect output: one JSON array, or one array per line.
pub(crate) fn parse_waiting(
    output: &str,
    label: &str,
    engine: &str,
) -> Result<Vec<WaitingContainer>, MmError> {
    let invalid = |error: serde_json::Error| MmError::RuntimeResponse {
        message: format!("{engine} returned invalid container inspect JSON: {error}"),
    };
    let mut containers = Vec::new();
    for document in serde_json::Deserializer::from_str(output).into_iter::<Value>() {
        match document.map_err(invalid)? {
            Value::Array(rows) => {
                containers.extend(rows.iter().filter_map(|row| container(row, label)));
            }
            row => containers.extend(container(&row, label)),
        }
    }
    Ok(containers)
}

fn container(row: &Value, label: &str) -> Option<WaitingContainer> {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    Some(WaitingContainer {
        id: row["Id"].as_str()?.to_owned(),
        name: row["Name"].as_str()?.trim_start_matches('/').to_owned(),
        storage: row["Config"]["Labels"][label].as_str()?.to_owned(),
        state: text(&row["State"]["Status"]),
        error: text(&row["State"]["Error"]),
        restart_policy: text(&row["HostConfig"]["RestartPolicy"]["Name"]),
    })
}

/// Starts the container `id`.
pub(crate) async fn start_container(engine: &dyn ContainerEngine, id: &str) -> Result<(), MmError> {
    engine
        .engine(vec!["start".to_owned(), id.to_owned()], START_TIMEOUT)
        .await
        .map(drop)
}
