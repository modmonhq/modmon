//! The containers on a node's networks, for the node's name resolver: who
//! they are, which addresses they hold, and whether they are healthy.
//!
//! Two engine commands list them (`ps` by network, then one `inspect`), two
//! more when one of them is also on a network of its own host, and the
//! engine's event stream says when to look again. Both engines print
//! the same inspect document for what is read here, take the same event
//! names (Podman calls `die` `died` and takes either), and print an
//! event's `Type`. Where the engine's API is read (`EngineApi`), the same
//! list and the same documents come from its socket and no process is
//! started for a look.

use std::net::Ipv4Addr;
use std::time::Duration;

use serde_json::Value;

use crate::adapters::engine_api::EngineApi;
use crate::adapters::runtime::engine_containers::inspected_containers;
use crate::adapters::runtime::project_services::ContainerEngine;
use crate::domain::discovery::{Attachment, Health, ObservedContainer};
use crate::domain::error::MmError;

const SERVICE_LABEL: &str = "com.docker.compose.service";
/// Set on the container of a `compose run`, which is a job and no service.
const ONE_OFF_LABEL: &str = "com.docker.compose.oneoff";
const TIMEOUT: Duration = Duration::from_secs(30);

/// The engine events after which the containers on a network, their
/// addresses, or their health may differ. A health check that merely ran is
/// not one of them.
pub(crate) const CHANGE_EVENTS: [&str; 10] = [
    "start",
    "die",
    "destroy",
    "pause",
    "unpause",
    "rename",
    "health_status",
    "connect",
    "disconnect",
    "restart",
];

/// The arguments of the engine's event stream, one line per change.
pub(crate) fn change_event_arguments() -> Vec<String> {
    let mut arguments = vec!["events".to_owned()];
    for event in CHANGE_EVENTS {
        arguments.extend(["--filter".to_owned(), format!("event={event}")]);
    }
    // Only that a line arrives matters. Docker calls the event `Action`,
    // Podman `Status`; both have `Type`.
    arguments.extend(["--format".to_owned(), "{{.Type}}".to_owned()]);
    arguments
}

/// How long one event stream runs before it is started again. The node
/// looks at the engine every few seconds anyway; this only keeps a stream
/// that went quiet from staying for ever.
const WATCH_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// Runs the engine's event stream and calls `changed` when events arrive,
/// until the stream ends: the engine stopped, the command failed, or the
/// stream had its day. The process is started where every child process
/// is, and ends when the caller stops watching.
pub(crate) async fn watch_changes(
    program: &str,
    inherited: &'static [&'static str],
    changed: &(dyn Fn() + Send + Sync),
) -> Result<(), MmError> {
    use crate::app::ports::{
        ProcessOutputSource, ProcessStreamEvent, StreamCommandSpec, StreamingProcessRunner,
    };

    let specification = StreamCommandSpec::new(program)
        .args(change_event_arguments())
        .inherit_environment(inherited)
        .timeout(WATCH_FOR);
    let cancellation = crate::domain::observability::OperationCancellation::default();
    let mut stream = crate::adapters::process::TokioProcessRunner
        .spawn(specification, cancellation)
        .await?;
    while let Some(event) = stream.recv().await {
        match event {
            ProcessStreamEvent::Output {
                source: ProcessOutputSource::Stdout,
                ..
            } => changed(),
            ProcessStreamEvent::Output { .. } => {}
            ProcessStreamEvent::Exited { .. }
            | ProcessStreamEvent::Failed(MmError::ProcessTimeout { .. }) => break,
            ProcessStreamEvent::Failed(error) => return Err(error),
        }
    }
    Ok(())
}

/// Every container attached to one of `networks`, running or not, and
/// every container that shares another network of the engine with one of
/// them: a container on a cluster network that is also on a network of its
/// own host looks up its neighbours there through the same resolver.
pub(crate) async fn network_containers(
    engine: &dyn ContainerEngine,
    api: Option<&EngineApi>,
    networks: &[String],
) -> Result<Vec<ObservedContainer>, MmError> {
    let mut containers = attached_to(engine, api, networks).await?;
    let neighbours = containers
        .iter()
        .flat_map(|container| &container.attachments)
        .map(|attachment| attachment.network.clone())
        .filter(|network| !networks.contains(network))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    for neighbour in attached_to(engine, api, &neighbours).await? {
        if !containers.iter().any(|known| known.id == neighbour.id) {
            containers.push(neighbour);
        }
    }
    Ok(containers)
}

async fn attached_to(
    engine: &dyn ContainerEngine,
    api: Option<&EngineApi>,
    networks: &[String],
) -> Result<Vec<ObservedContainer>, MmError> {
    if networks.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(api) = api {
        let filters = serde_json::json!({ "network": networks });
        return Ok(inspected_containers(api, &filters)
            .await?
            .iter()
            .filter_map(container)
            .collect());
    }
    let mut arguments = vec![
        "ps".to_owned(),
        "--all".to_owned(),
        "--quiet".to_owned(),
        "--no-trunc".to_owned(),
    ];
    for network in networks {
        arguments.extend(["--filter".to_owned(), format!("network={network}")]);
    }
    let listed = engine.engine(arguments, TIMEOUT).await?;
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
    parse_containers(inspected.trim(), engine.engine_name())
}

/// Reads the engine's inspect output: one JSON array, or one array per line.
pub(crate) fn parse_containers(
    output: &str,
    engine: &str,
) -> Result<Vec<ObservedContainer>, MmError> {
    let invalid = |error: serde_json::Error| MmError::RuntimeResponse {
        message: format!("{engine} returned invalid container inspect JSON: {error}"),
    };
    let mut containers = Vec::new();
    for document in serde_json::Deserializer::from_str(output).into_iter::<Value>() {
        match document.map_err(invalid)? {
            Value::Array(rows) => containers.extend(rows.iter().filter_map(container)),
            row => containers.extend(container(&row)),
        }
    }
    Ok(containers)
}

fn container(row: &Value) -> Option<ObservedContainer> {
    let id = row["Id"].as_str()?.to_owned();
    let name = row["Name"].as_str()?.trim_start_matches('/').to_owned();
    let labels = &row["Config"]["Labels"];
    let one_off = labels[ONE_OFF_LABEL]
        .as_str()
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let service = labels[SERVICE_LABEL]
        .as_str()
        .filter(|_| !one_off)
        .map(str::to_owned);
    let state = &row["State"];
    let health = Health::from_engine(
        state["Status"].as_str().unwrap_or_default(),
        state["Health"]["Status"].as_str(),
    );
    let attachments = row["NetworkSettings"]["Networks"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(network, settings)| {
            let address = settings["IPAddress"].as_str()?.parse::<Ipv4Addr>();
            let Ok(address) = address else {
                // A stopped container holds no address.
                return None;
            };
            let listed = |key: &str| {
                settings[key]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            };
            let mut names = listed("DNSNames");
            for alias in listed("Aliases") {
                if !names.contains(&alias) {
                    names.push(alias);
                }
            }
            Some(Attachment {
                network: network.clone(),
                address,
                names,
            })
        })
        .collect();
    Some(ObservedContainer {
        id,
        name,
        service,
        health,
        attachments,
    })
}
