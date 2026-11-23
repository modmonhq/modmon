//! `mm target preference set-address`: where this device connects for a
//! cluster node, private to the device.

use std::collections::BTreeMap;

use super::*;
use crate::adapters::target::NodeAddressStore;
use crate::domain::target::{DeviceNodeAddress, NodeAddress};

fn store(home: &MmHome) -> NodeAddressStore {
    NodeAddressStore::new(home.root.join("node-addresses.json"))
}

fn key(document: &ConfigDocument, environment: &str) -> String {
    format!("{}/{environment}", document.config.project.id.0)
}

/// This device's addresses for the nodes of `environment` on `target`, by
/// node name. An address set while the environment used another target is
/// left out, with a warning.
pub(super) fn device_node_addresses(
    home: &MmHome,
    document: &ConfigDocument,
    environment: &str,
    target: &str,
) -> Result<BTreeMap<String, NodeAddress>, MmError> {
    Ok(store(home)
        .environment(&key(document, environment))?
        .into_iter()
        .filter_map(|(node, entry)| {
            if entry.applies_to(target) {
                return Some((node, entry.address));
            }
            tracing::warn!(
                "this device's address {} for node '{node}' of environment '{environment}' was set for target '{}', and the environment now uses target '{target}', so mm connects to the node's host in targets.yaml. Run `mm target preference set-address {environment} {node} ADDRESS` to set it for '{target}', or `mm target preference unset-address {environment} {node}`",
                entry.address,
                entry.target.as_deref().unwrap_or_default()
            );
            None
        })
        .collect())
}

/// This project's node addresses on this device, by environment.
pub(crate) fn project_node_addresses(
    document: &ConfigDocument,
    explicit_home: Option<&Path>,
) -> Result<BTreeMap<String, BTreeMap<String, DeviceNodeAddress>>, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let prefix = key(document, "");
    Ok(store(&home)
        .all()?
        .into_iter()
        .filter_map(|(key, nodes)| {
            key.strip_prefix(&prefix)
                .map(|environment| (environment.to_owned(), nodes))
        })
        .collect())
}

/// Connects this device to `node` of `environment` at `address`. Refuses
/// an environment the project does not declare and a node its target does
/// not list.
pub(crate) fn set_node_address(
    document: &ConfigDocument,
    state: &dyn StateStore,
    explicit_home: Option<&Path>,
    (environment, node): (&str, &str),
    address: &str,
) -> Result<NodeAddress, MmError> {
    let address = address
        .parse::<NodeAddress>()
        .map_err(|message| MmError::Target { message })?;
    if !document.config.environments.contains_key(environment) {
        return Err(MmError::Environment {
            message: format!(
                "the project declares no environment '{environment}'; use one of: {}",
                document
                    .config
                    .environments
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    let home = MmHome::resolve(explicit_home)?;
    let targets = FilesystemTargetStore::new(home.targets_file.clone());
    let (environment_pref, project_pref) =
        crate::app::load_selection_preferences(Some(state), document, environment)?;
    let (name, _) = crate::app::resolve_target_name(
        document,
        environment,
        None,
        environment_pref.as_ref(),
        project_pref.as_ref(),
    )?;
    let target = crate::app::resolve_named_target(document, &name, &targets)?;
    if !target.definition.is_cluster() {
        return Err(MmError::Target {
            message: format!(
                "environment '{environment}' deploys to target '{name}', which names one host and no nodes; a device address is for a node of a cluster target"
            ),
        });
    }
    if !target
        .definition
        .nodes
        .iter()
        .any(|known| known.name == node)
    {
        return Err(MmError::Target {
            message: format!(
                "target '{name}' of environment '{environment}' has no node '{node}'; its nodes are {}. `mm -e {environment} target nodes` lists them",
                target
                    .definition
                    .nodes
                    .iter()
                    .map(|known| known.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    store(&home).set(
        &key(document, environment),
        node,
        DeviceNodeAddress {
            address: address.clone(),
            target: Some(name),
        },
    )?;
    Ok(address)
}

/// Goes back to the repo's address for `node` of `environment`. Whether
/// this device had an address of its own for it.
pub(crate) fn unset_node_address(
    document: &ConfigDocument,
    explicit_home: Option<&Path>,
    (environment, node): (&str, &str),
) -> Result<bool, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    store(&home).unset(&key(document, environment), node)
}
