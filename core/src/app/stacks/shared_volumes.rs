//! A stack's shared volumes as one node of a cluster target runs them.
//!
//! The Compose files keep saying `uploads:/data`. The node's overlay gives
//! the top-level volume `uploads` an engine volume that is the volume's
//! directory on the storage node: the directory itself there, a mount over
//! the mesh on every other node. A service that mounts one is labelled
//! with where its storage is, so the node can start it again when the
//! storage node was away while the engine tried.

use serde_yaml_ng::Value;

use super::placement::service_resources;
use super::*;
use crate::domain::shared_storage::{
    EngineVolume, STORAGE_LABEL, SharedVolumeUse, VOLUME_LABEL, VolumeName,
};

/// One shared volume of a stack on this node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NodeVolume {
    /// The volume's key in the Compose files.
    pub key: String,
    pub engine: EngineVolume,
    /// The services of this node that mount it.
    pub services: Vec<String>,
}

/// The shared volumes one stack uses on this node.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct NodeVolumes {
    pub volumes: Vec<NodeVolume>,
    /// The storage node's name; empty when the stack shares nothing here.
    pub storage_node: String,
    pub local: bool,
}

impl NodeVolumes {
    /// The labels of an engine volume: what it holds, and whose it is.
    pub(super) fn volume_labels(
        document: &ConfigDocument,
        environment: &str,
        stack: &str,
        key: &str,
    ) -> BTreeMap<String, String> {
        BTreeMap::from([
            (VOLUME_LABEL.to_owned(), format!("{stack}/{key}")),
            (
                crate::domain::ownership::LABEL_PROJECT_ID.to_owned(),
                document.config.project.id.0.to_string(),
            ),
            (
                crate::domain::ownership::LABEL_ENVIRONMENT.to_owned(),
                environment.to_owned(),
            ),
        ])
    }

    /// The label each service that mounts a shared volume carries.
    pub(super) fn service_labels(&self) -> BTreeMap<String, (String, String)> {
        let mut labels = BTreeMap::new();
        for volume in &self.volumes {
            for service in &volume.services {
                labels.insert(
                    service.clone(),
                    (
                        STORAGE_LABEL.to_owned(),
                        volume.engine.storage_label.clone(),
                    ),
                );
            }
        }
        labels
    }

    /// What the node's plan says about them.
    pub(super) fn uses(&self, stack: &str) -> Vec<SharedVolumeUse> {
        self.volumes
            .iter()
            .map(|volume| SharedVolumeUse {
                stack: stack.to_owned(),
                volume: volume.key.clone(),
                storage_node: self.storage_node.clone(),
                local: self.local,
                directory: volume.engine.directory.clone(),
                engine_volume: volume.engine.name.clone(),
                services: volume.services.clone(),
            })
            .collect()
    }
}

/// The services of each Compose file that mount a named volume, by volume
/// key.
pub(crate) fn volume_users(models: &[Value]) -> BTreeMap<String, BTreeSet<String>> {
    let mut users = BTreeMap::<String, BTreeSet<String>>::new();
    for model in models {
        let Some(services) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in services {
            let Some(name) = name.as_str() else {
                continue;
            };
            for volume in service_resources(service, "volumes") {
                users.entry(volume).or_default().insert(name.to_owned());
            }
        }
    }
    users
}

/// The shared volumes of `stack` that a service of this node mounts. Empty
/// anywhere but on a node of a cluster target, where the configuration is
/// the node's view and `sources` hold only the services placed there.
pub(super) fn node_volumes(
    document: &ConfigDocument,
    environment: &str,
    stack_name: &str,
    stack: &StackConfig,
    sources: &[CompiledComposeSource],
) -> Result<NodeVolumes, MmError> {
    let Some(storage) = stack.storage.as_ref() else {
        return Ok(NodeVolumes::default());
    };
    let mut models = Vec::with_capacity(sources.len());
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        models.push(model);
    }
    let users = volume_users(&models);
    let mut volumes = Vec::new();
    let mut listed = BTreeSet::new();
    for key in &stack.shared_volumes {
        let Some(services) = users.get(key).filter(|_| listed.insert(key.as_str())) else {
            continue;
        };
        let engine = storage
            .engine_volume(
                document.config.project.id,
                environment,
                &VolumeName::new(stack_name, key),
            )
            .map_err(|problem| MmError::Environment {
                message: format!(
                    "shared volume '{key}' of stack '{stack_name}' cannot be used on this node: {problem}"
                ),
            })?;
        volumes.push(NodeVolume {
            key: key.clone(),
            engine,
            services: services.iter().cloned().collect(),
        });
    }
    Ok(NodeVolumes {
        volumes,
        storage_node: storage.node().to_owned(),
        local: storage.is_local(),
    })
}
