//! The containers of a Compose project by the labels Compose gives them.
//!
//! A cluster node removes the services that left it itself, and finds them
//! from what runs, instead of relying on `--remove-orphans`: Docker Compose
//! honours that flag on `up`, Podman Compose 1.6 accepts it and leaves the
//! containers running. Both label every container with its project and
//! service, so the same three engine commands work for both.

use std::collections::BTreeSet;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::adapters::runtime::inspect::{inspect_labels, parse_resource_ids};
use crate::app::ports::CommandOutput;
use crate::domain::error::MmError;

const PROJECT_LABEL: &str = "com.docker.compose.project";
const SERVICE_LABEL: &str = "com.docker.compose.service";
const CONFIG_HASH_LABEL: &str = "com.docker.compose.config-hash";
/// Set on the container of a `compose run`, which is a job and no service.
const ONE_OFF_LABEL: &str = "com.docker.compose.oneoff";

/// The engine command line of a runtime adapter.
#[async_trait]
pub(crate) trait ContainerEngine: Send + Sync {
    /// `Docker` or `Podman`, for messages.
    fn engine_name(&self) -> &'static str;
    /// Runs the engine with `args` and returns its output once it succeeded.
    async fn engine(&self, args: Vec<String>, timeout: Duration) -> Result<CommandOutput, MmError>;
}

async fn container_ids(
    engine: &dyn ContainerEngine,
    project: &str,
    service: Option<&str>,
) -> Result<Vec<String>, MmError> {
    let mut args = vec![
        "ps".to_owned(),
        "--all".to_owned(),
        "--filter".to_owned(),
        format!("label={PROJECT_LABEL}={project}"),
    ];
    if let Some(service) = service {
        args.extend([
            "--filter".to_owned(),
            format!("label={SERVICE_LABEL}={service}"),
        ]);
    }
    args.extend(["--format".to_owned(), "json".to_owned()]);
    let output = engine.engine(args, Duration::from_secs(30)).await?;
    parse_resource_ids(&output, engine.engine_name(), "container")
}

/// The services that have a container in `project`, running or not.
pub(crate) async fn project_services(
    engine: &dyn ContainerEngine,
    project: &str,
) -> Result<BTreeSet<String>, MmError> {
    let mut services = BTreeSet::new();
    for id in container_ids(engine, project, None).await? {
        let output = engine
            .engine(
                vec![
                    "container".to_owned(),
                    "inspect".to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                    id,
                ],
                Duration::from_secs(30),
            )
            .await?;
        let rows = serde_json::from_str::<Vec<Value>>(output.stdout.trim()).map_err(|error| {
            MmError::RuntimeResponse {
                message: format!(
                    "{} returned invalid container inspect JSON: {error}",
                    engine.engine_name()
                ),
            }
        })?;
        for labels in rows.iter().filter_map(inspect_labels) {
            let one_off = labels
                .get(ONE_OFF_LABEL)
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
            if let Some(service) = labels.get(SERVICE_LABEL).filter(|_| !one_off) {
                services.insert(service.clone());
            }
        }
    }
    Ok(services)
}

/// One container of a Compose service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServiceContainer {
    pub id: String,
    /// When the engine created it, as the engine prints it (RFC 3339).
    pub created: String,
    /// Compose's hash of the service configuration the container was
    /// created from.
    pub config_hash: Option<String>,
    pub running: bool,
}

/// The containers of `service` in `project`, newest first.
pub(crate) async fn service_containers(
    engine: &dyn ContainerEngine,
    project: &str,
    service: &str,
) -> Result<Vec<ServiceContainer>, MmError> {
    let ids = container_ids(engine, project, Some(service)).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec![
        "container".to_owned(),
        "inspect".to_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ];
    args.extend(ids);
    let output = engine.engine(args, Duration::from_secs(30)).await?;
    let rows = serde_json::from_str::<Vec<Value>>(output.stdout.trim()).map_err(|error| {
        MmError::RuntimeResponse {
            message: format!(
                "{} returned invalid container inspect JSON: {error}",
                engine.engine_name()
            ),
        }
    })?;
    let mut containers = rows
        .iter()
        .filter_map(|row| {
            let labels = inspect_labels(row).unwrap_or_default();
            let one_off = labels
                .get(ONE_OFF_LABEL)
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
            (!one_off).then(|| ServiceContainer {
                id: row["Id"].as_str().unwrap_or_default().to_owned(),
                created: row["Created"].as_str().unwrap_or_default().to_owned(),
                config_hash: labels.get(CONFIG_HASH_LABEL).cloned(),
                running: row["State"]["Running"].as_bool().unwrap_or(false),
            })
        })
        .filter(|container| !container.id.is_empty())
        .collect::<Vec<_>>();
    containers.sort_by(|left, right| right.created.cmp(&left.created));
    Ok(containers)
}

/// The ids of the containers of `services` in `project`, running or not;
/// jobs' one-off containers are none of them.
pub(crate) async fn service_container_ids(
    engine: &dyn ContainerEngine,
    project: &str,
    services: &[String],
) -> Result<Vec<String>, MmError> {
    let mut ids = Vec::new();
    for service in services {
        ids.extend(
            service_containers(engine, project, service)
                .await?
                .into_iter()
                .map(|container| container.id),
        );
    }
    Ok(ids)
}

/// The running containers of `services` whose configuration differs from
/// `hashes`, Compose's hash of each service in the files about to be
/// applied: the ones applying them would replace. A service the files do
/// not hash has none.
pub(crate) async fn outdated_containers(
    engine: &dyn ContainerEngine,
    project: &str,
    services: &[String],
    hashes: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<String>, MmError> {
    let mut outdated = Vec::new();
    for service in services {
        let Some(wanted) = hashes.get(service) else {
            continue;
        };
        outdated.extend(
            service_containers(engine, project, service)
                .await?
                .into_iter()
                .filter(|container| {
                    container.running && container.config_hash.as_ref() != Some(wanted)
                })
                .map(|container| container.id),
        );
    }
    Ok(outdated)
}

/// Stops, then removes, the containers `containers` names by id.
pub(crate) async fn remove_containers(
    engine: &dyn ContainerEngine,
    containers: &[String],
) -> Result<(), MmError> {
    if containers.is_empty() {
        return Ok(());
    }
    let mut stop = vec!["stop".to_owned()];
    stop.extend(containers.iter().cloned());
    engine.engine(stop, Duration::from_secs(300)).await?;
    let mut remove = vec!["rm".to_owned(), "--force".to_owned()];
    remove.extend(containers.iter().cloned());
    engine.engine(remove, Duration::from_secs(120)).await?;
    Ok(())
}

/// The last `lines` lines container `container` logged, oldest first, the
/// output and error streams in the order they were written.
pub(crate) async fn log_tail(
    engine: &dyn ContainerEngine,
    container: &str,
    lines: usize,
) -> Result<Vec<String>, MmError> {
    let output = engine
        .engine(
            vec![
                "logs".to_owned(),
                "--timestamps".to_owned(),
                "--tail".to_owned(),
                lines.to_string(),
                container.to_owned(),
            ],
            Duration::from_secs(30),
        )
        .await?;
    // Each line starts with the time it was written, which sorts as text.
    let mut stamped = output
        .stdout
        .lines()
        .chain(output.stderr.lines())
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    stamped.sort_by_key(|line| line.split(' ').next().unwrap_or_default().to_owned());
    let skip = stamped.len().saturating_sub(lines);
    Ok(stamped
        .into_iter()
        .skip(skip)
        .map(|line| {
            let text = line.split_once(' ').map_or(line, |(_, text)| text);
            text.chars()
                .filter(|character| !character.is_control())
                .collect()
        })
        .collect())
}

/// Stops, then removes, the containers of `services` in `project`. With
/// `whole_project` and no container left, the networks Compose created for
/// the project are removed as well. Volumes stay.
pub(crate) async fn remove_services(
    engine: &dyn ContainerEngine,
    project: &str,
    services: &[String],
    whole_project: bool,
) -> Result<(), MmError> {
    let mut containers = Vec::new();
    for service in services {
        containers.extend(container_ids(engine, project, Some(service)).await?);
    }
    if !containers.is_empty() {
        let mut stop = vec!["stop".to_owned()];
        stop.extend(containers.iter().cloned());
        engine.engine(stop, Duration::from_secs(300)).await?;
        let mut remove = vec!["rm".to_owned(), "--force".to_owned()];
        remove.extend(containers);
        engine.engine(remove, Duration::from_secs(120)).await?;
    }
    if !whole_project || !container_ids(engine, project, None).await?.is_empty() {
        return Ok(());
    }
    let listed = engine
        .engine(
            vec![
                "network".to_owned(),
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label={PROJECT_LABEL}={project}"),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            Duration::from_secs(30),
        )
        .await?;
    let networks = parse_resource_ids(&listed, engine.engine_name(), "network")?;
    if networks.is_empty() {
        return Ok(());
    }
    let mut remove = vec!["network".to_owned(), "rm".to_owned()];
    remove.extend(networks);
    engine.engine(remove, Duration::from_secs(60)).await?;
    Ok(())
}

#[cfg(test)]
#[path = "project_services_tests.rs"]
mod tests;
