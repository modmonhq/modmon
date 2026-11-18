//! A service node by node: which of its containers count now, and the rows
//! the Services view lists beneath it on a cluster target.

use std::collections::BTreeSet;

use crate::domain::runtime::{ContainerObservation, ServiceSnapshot};
use crate::presentation::tui::ops::ServiceTarget;

/// The containers that say how the service is now. An earlier container
/// that a newer one replaced and one that ran once for a job are history.
pub(super) fn current(service: &ServiceSnapshot) -> impl Iterator<Item = &ContainerObservation> {
    service
        .containers
        .iter()
        .filter(|container| !container.history)
}

/// One row beneath a service on a cluster target: a node the service is
/// placed on and one of its containers there, or that it has none.
#[derive(Debug, Clone, Copy)]
pub struct NodeRow<'a> {
    pub node: &'a str,
    pub container: Option<&'a ContainerObservation>,
    /// The last row of the service.
    pub last: bool,
}

/// The rows of a service on a cluster target, in the target's node order:
/// every current container of each node, and one row for a node that has
/// none. Empty on a single host.
pub(super) fn node_rows(service: &ServiceSnapshot) -> Vec<NodeRow<'_>> {
    let mut rows = Vec::new();
    for share in &service.nodes {
        let before = rows.len();
        rows.extend(
            current(service)
                .filter(|container| container.node.as_deref() == Some(share.node.as_str()))
                .map(|container| NodeRow {
                    node: &share.node,
                    container: Some(container),
                    last: false,
                }),
        );
        if rows.len() == before {
            rows.push(NodeRow {
                node: &share.node,
                container: None,
                last: false,
            });
        }
    }
    if let Some(row) = rows.last_mut() {
        row.last = true;
    }
    rows
}

/// The node of a service that runs as one container on one node. Its row
/// names the node after the service: a row beneath would add nothing.
pub(super) fn only_node(service: &ServiceSnapshot) -> Option<&str> {
    let rows = node_rows(service);
    match rows.as_slice() {
        [row] if row.container.is_some() => Some(row.node),
        _ => None,
    }
}

/// Whether the Services view lists rows beneath the service that can be
/// folded away: its replicas on a single host, its nodes on a cluster.
pub(super) fn foldable(service: &ServiceSnapshot) -> bool {
    if service.nodes.is_empty() {
        current(service).count() > 1
    } else {
        only_node(service).is_none()
    }
}

/// One selectable row of the Services view.
#[derive(Debug, Clone, Copy)]
pub enum ServiceRow<'a> {
    Service {
        service: &'a ServiceSnapshot,
        /// Whether its containers or nodes are listed beneath it.
        expanded: bool,
    },
    /// One of a service's containers on a single host.
    Container {
        service: &'a ServiceSnapshot,
        container: &'a ContainerObservation,
        last: bool,
    },
    /// On a cluster target: a node the service is placed on, with its
    /// container there when it has one.
    Node {
        service: &'a ServiceSnapshot,
        row: NodeRow<'a>,
    },
}

impl<'a> ServiceRow<'a> {
    #[must_use]
    pub fn service(&self) -> &'a ServiceSnapshot {
        match self {
            Self::Service { service, .. }
            | Self::Container { service, .. }
            | Self::Node { service, .. } => service,
        }
    }

    /// What an action on the row applies to: the service, one container,
    /// or the service on one node.
    #[must_use]
    pub fn target(&self) -> ServiceTarget {
        let service = self.service();
        let mut target = ServiceTarget {
            stack: service.stack.clone(),
            service: service.name.clone(),
            container: None,
            label: service.name.clone(),
            node: None,
        };
        match self {
            Self::Service { .. } => {}
            Self::Container { container, .. } => {
                target.container = Some(container.id.clone());
                target.label = container.name.clone();
            }
            Self::Node { row, .. } => {
                target.node = Some(row.node.to_owned());
                if let Some(container) = row.container {
                    target.container = Some(container.id.clone());
                    target.label = crate::presentation::output::container_name(service, container);
                }
            }
        }
        target
    }
}

/// The rows of the Services view: each service, then beneath one that is
/// unfolded its containers on a single host, or its nodes on a cluster.
pub(super) fn service_rows<'a>(
    services: Vec<&'a ServiceSnapshot>,
    collapsed: &BTreeSet<(String, String)>,
) -> Vec<ServiceRow<'a>> {
    let mut rows = Vec::new();
    for service in services {
        let expanded = foldable(service)
            && !collapsed.contains(&(service.stack.clone(), service.name.clone()));
        rows.push(ServiceRow::Service { service, expanded });
        if !expanded {
            continue;
        }
        if service.nodes.is_empty() {
            let containers = current(service).collect::<Vec<_>>();
            let count = containers.len();
            rows.extend(
                containers
                    .into_iter()
                    .enumerate()
                    .map(|(index, container)| ServiceRow::Container {
                        service,
                        container,
                        last: index + 1 == count,
                    }),
            );
        } else {
            rows.extend(
                node_rows(service)
                    .into_iter()
                    .map(|row| ServiceRow::Node { service, row }),
            );
        }
    }
    rows
}
