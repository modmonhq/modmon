//! Whether a stack on a node already runs what a deployment would start.
//!
//! Such a stack is left as it is: no container leaves, Compose is not run,
//! and nothing waits for health. It counts only when every container of the
//! stack is the one Compose would create from the new files: the
//! configuration hash Compose gives the service, the image the tag names on
//! the node now, healthy, and as many as the node runs of the service.

use std::collections::BTreeMap;

use serde_yaml_ng::Value;

/// What the new files ask of one service on this node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WantedService {
    pub service: String,
    /// Compose's hash of the service in the new files
    /// (`compose config --hash`).
    pub config_hash: String,
    /// How many containers of it the node runs.
    pub count: u32,
    /// The engine's id of the image the service's tag names now; `None`
    /// when the node does not have it.
    pub image_id: Option<String>,
    /// Its `pull_policy` pulls the image even when the node has it.
    pub always_pulled: bool,
}

/// One container of the stack's Compose project as it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningContainer {
    pub service: String,
    pub config_hash: Option<String>,
    pub image_id: String,
    pub running: bool,
    /// The engine's health status; `None` without a health check.
    pub health: Option<String>,
    /// An earlier step had it leave: it drains, then it is removed.
    pub leaving: bool,
}

/// Whether the stack runs exactly what `wanted` asks for, so a deployment
/// leaves it alone. Containers that are leaving do not count.
#[must_use]
pub fn stack_unchanged(wanted: &[WantedService], running: &[RunningContainer]) -> bool {
    if wanted.is_empty() || wanted.iter().any(|service| service.always_pulled) {
        return false;
    }
    let running = running
        .iter()
        .filter(|container| !container.leaving)
        .collect::<Vec<_>>();
    if running.iter().any(|container| {
        !wanted
            .iter()
            .any(|service| service.service == container.service)
    }) {
        return false;
    }
    wanted.iter().all(|service| {
        let containers = running
            .iter()
            .filter(|container| container.service == service.service)
            .collect::<Vec<_>>();
        u32::try_from(containers.len()).is_ok_and(|count| count == service.count)
            && containers.iter().all(|container| {
                container.running
                    && container.config_hash.as_deref() == Some(service.config_hash.as_str())
                    && service.image_id.as_deref() == Some(container.image_id.as_str())
                    && container
                        .health
                        .as_deref()
                        .is_none_or(|health| health == "healthy")
            })
    })
}

/// What starting a service's new container next to its old one takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Beside {
    /// Nothing runs, or what runs is what the files say.
    Nothing,
    /// A start that was interrupted left the new container already.
    Started,
    /// One more container, so that the service counts `scale`.
    Start { scale: usize },
}

/// What a `start-first` step does for a service whose containers on the
/// node are `containers` and whose files hash to `wanted`. A container that
/// is leaving is neither the old one nor the new one; Compose still counts
/// it.
#[must_use]
pub fn beside(wanted: &str, containers: &[RunningContainer]) -> Beside {
    let staying = containers
        .iter()
        .filter(|container| !container.leaving)
        .collect::<Vec<_>>();
    let current = |container: &&RunningContainer| container.config_hash.as_deref() == Some(wanted);
    if !staying.iter().any(|container| container.running) || staying.iter().all(current) {
        return Beside::Nothing;
    }
    if staying.iter().any(current) {
        return Beside::Started;
    }
    Beside::Start {
        scale: containers.len() + 1,
    }
}

/// Each service's image and whether its `pull_policy` pulls it always, from
/// a rendered Compose configuration (`compose config`).
pub fn service_images(config: &str) -> Result<BTreeMap<String, (Option<String>, bool)>, String> {
    let parsed: Value = serde_yaml_ng::from_str(config)
        .map_err(|error| format!("rendered Compose configuration is not YAML: {error}"))?;
    let mut images = BTreeMap::new();
    let Some(services) = parsed.get("services").and_then(Value::as_mapping) else {
        return Ok(images);
    };
    for (name, service) in services {
        let Some(name) = name.as_str() else {
            continue;
        };
        let policy = service
            .get("pull_policy")
            .and_then(Value::as_str)
            .unwrap_or("missing");
        let always = !matches!(policy, "missing" | "if_not_present" | "never" | "build");
        let image = service
            .get("image")
            .and_then(Value::as_str)
            .map(str::to_owned);
        images.insert(name.to_owned(), (image, always));
    }
    Ok(images)
}
