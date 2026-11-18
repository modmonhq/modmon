//! The ingress of a cluster plan, put together from what the nodes planned:
//! each node's plan carries the routes of the services placed on it, and
//! says whether it runs the proxy.

use std::collections::BTreeMap;

use super::RemoteEnvironment;
use crate::domain::cluster_ingress::{INGRESS_NETWORK, IngressRoute, SealedIngress, SealedTunnel};
use crate::domain::error::MmError;
use crate::domain::remote::NodeDeploymentPlan;

impl RemoteEnvironment {
    /// The routes of every node's plan and the nodes that serve them. `None`
    /// for one host, and when the plan leaves nodes out: the routes of a
    /// node that does not plan are not known, so every ingress keeps what
    /// it serves until a plan covers every node.
    pub(super) fn plan_ingress(
        &self,
        nodes: &[NodeDeploymentPlan],
        skipped: &[String],
    ) -> Result<Option<SealedIngress>, MmError> {
        if !self.is_cluster() {
            return refuse_route_rules(nodes).map(|()| None);
        }
        if !skipped.is_empty() {
            return Ok(None);
        }
        let scope = &self.first().scope;
        let mut routes = BTreeMap::new();
        let mut serving = Vec::new();
        let mut certificate = None;
        let mut tunnel: Option<SealedTunnel> = None;
        for node in nodes {
            let Some(plan) = &node.plan.plan else {
                continue;
            };
            for stack in &plan.stacks {
                for route in &stack.artifact.manifest.routes {
                    if let Some(middleware) = route.middlewares.first() {
                        return Err(MmError::Environment {
                            message: format!(
                                "ingress middleware '{middleware}' would apply to {}, and a cluster target does not apply middleware files; limit ingress.middlewares.{middleware} to other environments with `environments:`, and use the route's `headers` and `allow` on clusters",
                                route.host
                            ),
                        });
                    }
                    routes
                        .entry(route.router_name.clone())
                        .or_insert_with(|| IngressRoute {
                            router: route.router_name.clone(),
                            host: route.host.clone(),
                            path: route.path_prefix.clone(),
                            priority: route.priority,
                            stack: stack.stack.clone(),
                            service: route.service.clone(),
                            port: route.port,
                            tls: route.tls,
                            balance: route.balance,
                            headers: route.headers.clone(),
                            allow: route.allow.clone(),
                        });
                }
            }
            let ingress = plan.node.as_ref().and_then(|node| node.ingress.as_ref());
            if let (Some(name), Some(ingress)) = (&node.node, ingress) {
                serving.push(name.clone());
                certificate.clone_from(&ingress.certificate);
                if let Some(held) = &ingress.tunnel {
                    seal_tunnel(&mut tunnel, name, held)?;
                }
            }
        }
        if serving.is_empty() && !routes.is_empty() {
            return Err(MmError::Placement {
                message: format!(
                    "no node of target '{}' plans to run the ingress proxy for the routes of '{}'; check targets.{}.cluster.ingress and plan again",
                    self.target, scope.environment, self.target
                ),
            });
        }
        Ok(Some(SealedIngress {
            nodes: serving,
            network: crate::domain::cluster_network::network_key(
                scope.project_id,
                &scope.environment,
                INGRESS_NETWORK,
            ),
            routes: routes.into_values().collect(),
            certificate,
            tunnel,
        }))
    }
}

/// Adds what `node` says about the environment's tunnel; the nodes must
/// hold tokens of one tunnel.
fn seal_tunnel(
    sealed: &mut Option<SealedTunnel>,
    node: &str,
    held: &crate::domain::cluster_ingress::NodeTunnel,
) -> Result<(), MmError> {
    let tunnel = sealed.get_or_insert_with(|| SealedTunnel {
        token: held.token.clone(),
        image: held.image.clone(),
        id: held.id.clone(),
        open_ports: BTreeMap::new(),
    });
    if tunnel.id != held.id {
        return Err(MmError::Environment {
            message: format!(
                "the nodes hold tokens of different tunnels in secret {}: {} on node '{node}', {} on another; store the same token on every node with `mm secret set {} --stdin` and plan again",
                held.token, held.id, tunnel.id, held.token
            ),
        });
    }
    if !held.open_ports_for.is_empty() {
        tunnel
            .open_ports
            .insert(node.to_owned(), held.open_ports_for.clone());
    }
    Ok(())
}

/// A target of one host serves its routes through its own proxy, which
/// sets no headers and has no allowlist.
fn refuse_route_rules(nodes: &[NodeDeploymentPlan]) -> Result<(), MmError> {
    let ruled = nodes
        .iter()
        .filter_map(|node| node.plan.plan.as_ref())
        .flat_map(|plan| &plan.stacks)
        .flat_map(|stack| &stack.artifact.manifest.routes)
        .find(|route| !route.headers.is_empty() || !route.allow.is_empty());
    match ruled {
        Some(route) => Err(MmError::Environment {
            message: format!(
                "the ingress route to {} sets `headers` (its own or the environment's `ingress.headers`) or `allow`, which apply on a cluster target (one that lists `nodes:`); a target of one host does not apply them, so remove them from this environment's routes or limit the route with `environments:`",
                route.host
            ),
        }),
        None => Ok(()),
    }
}
