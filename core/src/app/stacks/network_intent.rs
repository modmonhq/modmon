//! The networks a deployment's services join, with the ingress proxy as a
//! member of the network it meets the routed services on. (The hash of
//! what the workloads ask for is `network_overlay/intent_hash.rs`.)

use super::network_overlay::managed_network_name;
use super::*;

pub(super) fn canonical_networks(
    document: &ConfigDocument,
    target: &NamedTarget,
    environment: &str,
    membership: &BTreeMap<(String, String), BTreeSet<String>>,
) -> Result<Vec<crate::domain::stack::CanonicalNetworkIntent>, MmError> {
    let mut by_logical =
        BTreeMap::<String, Vec<crate::domain::stack::CanonicalNetworkMember>>::new();
    for ((stack, service), networks) in membership {
        for logical in networks {
            by_logical.entry(logical.clone()).or_default().push(
                crate::domain::stack::CanonicalNetworkMember {
                    stack: stack.clone(),
                    service: service.clone(),
                },
            );
        }
    }
    Ok(document
        .config
        .networks
        .iter()
        .map(
            |(logical, network)| crate::domain::stack::CanonicalNetworkIntent {
                internal: network.internal,
                logical_name: logical.clone(),
                physical_name: managed_network_name(
                    target.definition.id,
                    document.config.project.id,
                    environment,
                    logical,
                ),
                members: by_logical
                    .remove(logical)
                    .unwrap_or_default()
                    .into_iter()
                    .chain(ingress_proxy_member(network))
                    .collect(),
            },
        )
        .collect())
}
