//! A cluster node's part of a deployment plan: what it runs afterwards,
//! what starts, and what leaves.
//!
//! What leaves is decided from what the node runs now, not from a record of
//! earlier plans: any container of this environment's projects whose
//! service is not placed here. A move that was interrupted, or a container
//! left by hand, therefore shows up in the next plan and is removed by it.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::app::stacks::PlacementNotes;
use crate::domain::deployment::{LeavingService, NodeProjectPlan};
use crate::domain::placement::{NodeScope, PlacedService};

impl DeploymentManager {
    pub(super) async fn node_project(
        &self,
        scope: &NodeScope,
        plan: &DeploymentPlan,
        notes: PlacementNotes,
        certificate: Option<crate::domain::config::EnvironmentTlsConfig>,
    ) -> Result<NodeProjectPlan, MmError> {
        let mut rendered = BTreeMap::<&str, BTreeSet<&str>>::new();
        for stack in &plan.stacks {
            rendered.entry(stack.stack.as_str()).or_default().extend(
                stack
                    .artifact
                    .manifest
                    .services
                    .iter()
                    .map(|service| service.service.as_str()),
            );
        }
        // Every stack of the configuration, and the stacks the last
        // deployment ran here that the configuration no longer has.
        let mut projects = notes.projects.clone();
        for removed in &plan.removed_stacks {
            projects
                .entry(removed.stack.clone())
                .or_insert_with(|| removed.compose_project_name.clone());
        }
        let mut starting = Vec::new();
        let mut leaving = Vec::new();
        for (stack, project) in &projects {
            let observed = self.runtime.project_services(project).await?;
            let placed = rendered.get(stack.as_str());
            starting.extend(
                placed
                    .into_iter()
                    .flatten()
                    .filter(|service| !observed.contains(**service))
                    .map(|service| PlacedService {
                        stack: stack.clone(),
                        service: (*service).to_owned(),
                    }),
            );
            for service in observed {
                if placed.is_some_and(|placed| placed.contains(service.as_str())) {
                    continue;
                }
                leaving.push(LeavingService {
                    moves_to: notes
                        .elsewhere
                        .get(&(stack.clone(), service.clone()))
                        .cloned()
                        .unwrap_or_default(),
                    stack: stack.clone(),
                    compose_project_name: project.clone(),
                    service,
                });
            }
        }
        let mut services = rendered
            .iter()
            .flat_map(|(stack, services)| {
                services.iter().map(|service| PlacedService {
                    stack: (*stack).to_owned(),
                    service: (*service).to_owned(),
                })
            })
            .collect::<Vec<_>>();
        services.sort();
        starting.sort();
        leaving.sort();
        let mut dropped_dependencies = notes.dropped_dependencies;
        dropped_dependencies.sort();
        let mut restart_defaults = notes
            .restart_defaults
            .into_iter()
            .map(|(stack, service)| PlacedService { stack, service })
            .collect::<Vec<_>>();
        restart_defaults.sort();
        let mut shared_volumes = notes.shared_volumes;
        shared_volumes.sort();
        let scaled = self.scaled_services(&notes.scaled, &projects, plan)?;
        let health = health_settings(&notes.health)?;
        // A scaled service the node runs none of now does not start here.
        starting.retain(|placed| !idle_here(&scaled, &scope.node, placed));
        Ok(NodeProjectPlan {
            node: scope.node.clone(),
            services,
            starting,
            leaving,
            dropped_dependencies,
            networks: self.node_networks(plan).await?,
            restart_defaults,
            shared_volumes,
            ingress: ingress_role(plan, certificate),
            scaled,
            health,
        })
    }

    /// What this ingress node says about the environment's tunnel: the
    /// tunnel its token belongs to, refused when the token is missing, and
    /// the other environments that keep ports 80 and 443 open here.
    pub(super) async fn node_tunnel(
        &self,
        selection: &WorkspaceSelection,
        (token, image): (String, String),
    ) -> Result<crate::domain::cluster_ingress::NodeTunnel, MmError> {
        let held = self.environments.tunnel_token(selection, &token).await?;
        let open_ports_for = match &self.ingress {
            Some(host) => host.public_tables(&self.environments.ingress_table(selection)?)?,
            None => Vec::new(),
        };
        Ok(crate::domain::cluster_ingress::NodeTunnel {
            token,
            image,
            id: held.tunnel,
            open_ports_for,
        })
    }

    /// The node's services that set `replicas` or `autoscale`, with every
    /// default filled in, and the count the nodes go by now.
    fn scaled_services(
        &self,
        notes: &[crate::app::stacks::ScaleNote],
        projects: &BTreeMap<String, String>,
        plan: &DeploymentPlan,
    ) -> Result<Vec<crate::domain::autoscale::ScaledService>, MmError> {
        use crate::domain::autoscale::{ScalePolicy, ScaledService, replicas};
        let invalid = |service: &str, problem: String| MmError::Environment {
            message: format!(
                "service '{service}': {problem}; `mm config validate` lists every problem"
            ),
        };
        let mut scaled = Vec::with_capacity(notes.len());
        for note in notes {
            let count = replicas(note.replicas, note.nodes.len())
                .map_err(|problem| invalid(&note.service, problem.to_string()))?;
            let autoscale = note
                .autoscale
                .as_ref()
                .map(|config| ScalePolicy::resolve(config, note.replicas, note.nodes.len()))
                .transpose()
                .map_err(|problems| {
                    let problems = problems.iter().map(ToString::to_string).collect::<Vec<_>>();
                    invalid(&note.service, problems.join("; "))
                })?;
            let mut service = ScaledService {
                key: crate::domain::autoscale::service_key(
                    plan.project_id,
                    &plan.environment,
                    &note.stack,
                    &note.service,
                ),
                stack: note.stack.clone(),
                service: note.service.clone(),
                compose_project: projects.get(&note.stack).cloned().unwrap_or_default(),
                nodes: note.nodes.clone(),
                replicas: count,
                autoscale,
                drain_ms: u64::try_from(plan.rollout.drain(&note.stack, &note.service).as_millis())
                    .unwrap_or(u64::MAX),
                now: None,
            };
            if service.autoscale.is_some() {
                let wanted = self
                    .scale_state()
                    .and_then(|state| state.wanted(&service.key()));
                service.now = Some(service.count(wanted));
            }
            scaled.push(service);
        }
        scaled.sort_by(|left, right| {
            (&left.stack, &left.service).cmp(&(&right.stack, &right.service))
        });
        Ok(scaled)
    }

    /// The mm networks the node's services are on, with the engine's
    /// network of each name as it is now. Empty on a host that holds no
    /// cluster networks, where they stay local to the host.
    async fn node_networks(
        &self,
        plan: &DeploymentPlan,
    ) -> Result<Vec<crate::domain::cluster_network::NetworkUse>, MmError> {
        let Some(host) = &self.cluster_networks else {
            return Ok(Vec::new());
        };
        let stored = host.stored()?;
        let mut uses = Vec::new();
        for network in &plan.networks {
            if network.members.is_empty() {
                continue;
            }
            let key = crate::domain::cluster_network::network_key(
                plan.project_id,
                &plan.environment,
                &network.logical_name,
            );
            let wanted = stored.as_ref().and_then(|stored| stored.kept_network(&key));
            let mut services = network
                .members
                .iter()
                .map(|member| member.service.clone())
                .collect::<Vec<_>>();
            services.sort();
            services.dedup();
            uses.push(crate::domain::cluster_network::NetworkUse {
                name: network.logical_name.clone(),
                key,
                network: network.physical_name.clone(),
                internal: network.internal,
                services,
                observed: self
                    .runtime
                    .observe_cluster_network(&network.physical_name, wanted.as_ref())
                    .await?,
            });
        }
        Ok(uses)
    }
}

/// Whether `placed` is a scaled service of which `node` runs no container
/// at the count of the moment.
fn idle_here(
    scaled: &[crate::domain::autoscale::ScaledService],
    node: &str,
    placed: &PlacedService,
) -> bool {
    scaled
        .iter()
        .find(|scaled| scaled.stack == placed.stack && scaled.service == placed.service)
        .is_some_and(|scaled| scaled.share(node, scaled.now) == 0)
}

/// Set when the node runs the ingress proxy: the proxy is a member of the
/// network it shares with the routed services (`view_from_node`).
fn ingress_role(
    plan: &DeploymentPlan,
    certificate: Option<crate::domain::config::EnvironmentTlsConfig>,
) -> Option<crate::domain::cluster_ingress::NodeIngress> {
    use crate::domain::cluster_ingress::{INGRESS_NETWORK, NodeIngress, PROXY_MEMBER};
    plan.networks
        .iter()
        .any(|network| {
            network.logical_name == INGRESS_NETWORK
                && network
                    .members
                    .iter()
                    .any(|member| member.service == PROXY_MEMBER)
        })
        .then_some(NodeIngress {
            certificate,
            tunnel: None,
        })
}

/// The node's services that set `health:`, with every default filled in.
fn health_settings(
    notes: &[(String, String, crate::domain::healing::HealthConfig)],
) -> Result<Vec<crate::domain::healing::ServiceHealth>, MmError> {
    let mut health = notes
        .iter()
        .map(|(stack, service, config)| {
            crate::domain::healing::RestartPolicy::resolve(
                Some(config),
                &format!("service '{service}'"),
            )
            .map(|restart| crate::domain::healing::ServiceHealth {
                stack: stack.clone(),
                service: service.clone(),
                restart,
            })
            .map_err(|problems| MmError::Environment {
                message: format!(
                    "{}; `mm config validate` lists every problem",
                    problems.join("; ")
                ),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    health.sort_by(|left, right| (&left.stack, &left.service).cmp(&(&right.stack, &right.service)));
    Ok(health)
}
