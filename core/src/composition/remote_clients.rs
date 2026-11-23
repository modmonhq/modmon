//! The connections a client opens to a remote environment's hosts.

use super::*;

/// The key an identity authenticates with, whether its connections are
/// kept, and what opens the key. The user's connections stay open between
/// requests for a few minutes; the agent's never do, so every call of the
/// MCP server connects anew. Each identity opens its own key and no other:
/// the user's commands the device key, the MCP server the agent key.
pub(super) fn client_identity(
    home: &MmHome,
    identity: RemoteIdentity,
) -> Result<
    (
        ClientKey,
        ConnectionReuse,
        crate::adapters::keys::ManagedKeys,
    ),
    MmError,
> {
    use crate::domain::key::ManagedKeyKind;

    match identity {
        RemoteIdentity::User => Ok((
            ClientKey::Default(Some(device_key_path(home))),
            ConnectionReuse::under(&home.run_directory),
            super::keys::managed_keys(home, ManagedKeyKind::Device)?,
        )),
        RemoteIdentity::Agent => {
            let key = agent_key_path(home);
            if !key.is_file() {
                return Err(MmError::Security {
                    message: "this device has no agent key; a user must run `mm key create --agent` and register it".to_owned(),
                });
            }
            Ok((
                ClientKey::Only(key),
                ConnectionReuse::off(),
                super::keys::managed_keys(home, ManagedKeyKind::Agent)?,
            ))
        }
    }
}

/// One connection per node of a cluster target, each addressed by the
/// node's name, with the services this checkout places on it.
pub(super) fn cluster_nodes(
    document: &ConfigDocument,
    target: &crate::domain::target::NamedTarget,
    (access, home): (ClientAccess<'_>, &MmHome),
    scope: &crate::domain::remote::RemoteScope,
) -> Result<Vec<crate::app::RemoteNode>, MmError> {
    let addresses = super::super::node_addresses::device_node_addresses(
        home,
        document,
        &scope.environment,
        &target.name,
    )?;
    let names = target
        .definition
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    let no_groups = std::collections::BTreeMap::new();
    let groups = document
        .config
        .targets
        .get(&target.name)
        .map_or(&no_groups, |repo| &repo.cluster.groups);
    target
        .definition
        .nodes
        .iter()
        .map(|node| {
            let address = addresses.get(&node.name);
            let transport: Result<Arc<dyn crate::app::ports::RemoteTransport>, _> =
                match SshRemoteTransport::for_node(target, node, address, access) {
                    Ok(transport) => Ok(Arc::new(transport)),
                    Err(MmError::Unreachable {
                        subject,
                        problem,
                        help,
                    }) => Err(crate::app::NodeUnavailable {
                        subject,
                        problem,
                        help,
                    }),
                    Err(error) => return Err(error),
                };
            let placed = crate::domain::placement::services_on_node(
                &document.config.modules,
                &scope.environment,
                crate::domain::placement::ClusterNodes {
                    nodes: &names,
                    groups,
                },
                &node.name,
            )
            .map_err(|message| MmError::Placement { message })?;
            let (host, port) = address.map_or_else(
                || (node.host.clone(), node.port),
                |address| address.dial(node),
            );
            Ok(crate::app::RemoteNode {
                name: Some(node.name.clone()),
                host,
                port,
                repo_address: address.map(|_| (node.host.clone(), node.port)),
                private_address: node.private_address,
                transport,
                scope: crate::domain::remote::RemoteScope {
                    node: Some(node.name.clone()),
                    ..scope.clone()
                },
                placed: Some(placed),
            })
        })
        .collect()
}
