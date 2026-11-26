use std::path::Path;

use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::runtime::{
    ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy, ObservedProject,
    RenderedProject, ResourceEnsureResult, RuntimeKind, RuntimeProbe, RuntimeResource,
    RuntimeTarget,
};
use crate::domain::target::NamedTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeOwnership {
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnedVolumeFile<'a> {
    pub name: &'a str,
    pub ownership: VolumeOwnership,
}

#[async_trait]
pub trait RuntimeDriver: Send + Sync {
    fn kind(&self) -> RuntimeKind;
    async fn probe(&self) -> RuntimeProbe;
    /// What a status says about the engine. A status only reads
    /// containers, so a driver may answer with less than `probe` finds
    /// out, as long as it says whether the engine answers.
    async fn status_probe(&self) -> RuntimeProbe {
        self.probe().await
    }

    /// The project's containers as they are now, found by the project's
    /// name without rendering its Compose files, with every service of
    /// `expected_services` listed whether it has a container or not.
    /// `None` when this driver finds a project's containers only through
    /// a rendering (`render_and_observe`).
    async fn observe_unrendered(
        &self,
        _target: &RuntimeTarget,
        _expected_services: &[String],
    ) -> Option<Result<ObservedProject, MmError>> {
        None
    }

    async fn render(&self, target: &RuntimeTarget) -> Result<RenderedProject, MmError>;
    async fn observe(&self, target: &RuntimeTarget) -> Result<ObservedProject, MmError>;
    /// `render` and `observe` in one: the project as the engine renders it
    /// and as it runs it. An engine renders once for both and can ask for
    /// the containers while it renders.
    async fn render_and_observe(
        &self,
        target: &RuntimeTarget,
    ) -> Result<(RenderedProject, ObservedProject), MmError> {
        Ok((self.render(target).await?, self.observe(target).await?))
    }
    async fn wait_for_convergence(
        &self,
        target: &RuntimeTarget,
        policy: ConvergencePolicy,
        cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError>;
    /// Like `wait_for_convergence`, for some services of the project: the
    /// others may be in any state.
    async fn wait_for_services(
        &self,
        target: &RuntimeTarget,
        _services: &[String],
        policy: ConvergencePolicy,
        cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        self.wait_for_convergence(target, policy, cancellation)
            .await
    }
    /// The last `lines` lines container `container` logged, oldest first.
    /// A failed deployment reports them. Empty where the runtime cannot
    /// read a container's log.
    async fn container_log_tail(
        &self,
        _container: &str,
        _lines: usize,
    ) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }
    /// The Compose services that have a container in `project`, running or
    /// not. A cluster node plans from it which services leave the node.
    async fn project_services(
        &self,
        _project: &str,
    ) -> Result<std::collections::BTreeSet<String>, MmError> {
        Err(MmError::RuntimeResponse {
            message: "this runtime cannot list the services of a Compose project".to_owned(),
        })
    }
    /// The engine's network `name` against the bridge network `wanted`
    /// calls for; `None` when the engine has no network of that name. Also
    /// refuses an engine whose firewall mm's rules cannot work with.
    async fn observe_cluster_network(
        &self,
        _name: &str,
        _wanted: Option<&crate::domain::cluster_network::LocalNetwork>,
    ) -> Result<Option<crate::domain::cluster_network::ObservedNetwork>, MmError> {
        Err(MmError::Target {
            message: "this runtime cannot hold networks that span cluster nodes; use docker-compose on the nodes of a cluster target".to_owned(),
        })
    }
    /// `observe_cluster_network` for every network a node holds, in the
    /// order given. An engine can check its firewall once for all of them
    /// and look at several networks at a time.
    async fn observe_cluster_networks(
        &self,
        wanted: &[crate::domain::cluster_network::LocalNetwork],
    ) -> Result<Vec<Option<crate::domain::cluster_network::ObservedNetwork>>, MmError> {
        let mut observed = Vec::with_capacity(wanted.len());
        for network in wanted {
            observed.push(
                self.observe_cluster_network(&network.network, Some(network))
                    .await?,
            );
        }
        Ok(observed)
    }
    /// Every container attached to one of the engine's networks `networks`,
    /// running or not, with its addresses, names, and health. A node's name
    /// resolver answers from it.
    async fn network_containers(
        &self,
        _networks: &[String],
    ) -> Result<Vec<crate::domain::discovery::ObservedContainer>, MmError> {
        Err(MmError::Target {
            message: "this runtime cannot list the containers on a network; use docker-compose on the nodes of a cluster target".to_owned(),
        })
    }
    /// The containers that carry the label `label` and are not running,
    /// with what the engine said when each last failed to start. A node
    /// finds the ones that wait for shared storage among them.
    async fn waiting_containers(
        &self,
        _label: &str,
    ) -> Result<Vec<crate::domain::shared_storage::WaitingContainer>, MmError> {
        Ok(Vec::new())
    }
    /// What each of `containers` (ids, or their first twelve characters)
    /// uses now, as the engine itself measures it. It costs engine
    /// commands and more than a second, so a node asks only about a
    /// container whose counters its host does not show. An engine without
    /// such numbers reports none.
    async fn container_usage(
        &self,
        _containers: &[String],
    ) -> Result<Vec<crate::domain::metrics::ContainerSample>, MmError> {
        Ok(Vec::new())
    }
    /// Runs a container that does nothing under the OCI runtime `name`.
    /// `Err` says why the engine could not, and what to do on the host.
    async fn check_oci_runtime(&self, name: &str) -> Result<(), String> {
        Err(format!(
            "this engine cannot be asked to run a container under '{name}'"
        ))
    }
    /// Starts an existing container by its id.
    async fn start_container(&self, _id: &str) -> Result<(), MmError> {
        Err(MmError::Target {
            message: "this runtime cannot start a container by its id".to_owned(),
        })
    }
    /// Calls `changed` each time a container starts, stops, changes health,
    /// or joins or leaves a network, until the engine's event stream ends.
    /// An engine without such a stream never calls it and never returns;
    /// the caller looks again on a timer either way.
    async fn watch_containers(&self, _changed: &(dyn Fn() + Send + Sync)) -> Result<(), MmError> {
        std::future::pending::<Result<(), MmError>>().await
    }
}

#[async_trait]
pub trait LocalDeploymentRuntime: Send + Sync {
    async fn preflight(
        &self,
        target: &NamedTarget,
        resources: &[RuntimeResource],
        projects: &[RuntimeTarget],
    ) -> Result<(), MmError>;
    async fn ensure_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError>;
    /// Ensures a network whose containers reach its members and nothing
    /// else (`internal: true`).
    async fn ensure_internal_network(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        Err(MmError::Target {
            message: format!("this runtime cannot create the internal network '{name}'"),
        })
    }
    /// Ensures the bridge network of a cluster network on this node: the
    /// network's block as subnet, the node's slice as the range containers
    /// get addresses from. A network of that name with other addressing is
    /// removed, with the containers on it, and created again.
    async fn ensure_cluster_network(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
        _network: &crate::domain::cluster_network::LocalNetwork,
    ) -> Result<ResourceEnsureResult, MmError> {
        Err(MmError::Target {
            message: format!(
                "this runtime cannot create '{name}' as a network that spans cluster nodes; use docker-compose on the nodes of a cluster target"
            ),
        })
    }
    /// Ensures a network with fixed IPv4 addressing. Runtimes that cannot pin
    /// a subnet refuse instead of creating a network with dynamic addressing.
    async fn ensure_network_with_subnet(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
        _subnet: &str,
        _gateway: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        Err(MmError::Target {
            message: format!("this runtime cannot create network '{name}' with a fixed subnet"),
        })
    }
    async fn inspect_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<bool, MmError>;
    async fn remove_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<(), MmError>;
    async fn ensure_volume(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError>;
    async fn remove_volume(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<(), MmError>;
    async fn set_volume_owner(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _ownership: VolumeOwnership,
    ) -> Result<(), MmError> {
        Err(MmError::Security {
            message: "runtime adapter does not implement owned volume permissions".to_owned(),
        })
    }
    async fn create_volume_file_if_absent(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
        mode: u32,
    ) -> Result<(), MmError>;
    async fn replace_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
        source: &Path,
        mode: u32,
    ) -> Result<(), MmError>;
    async fn replace_volume_file_owned(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file: OwnedVolumeFile<'_>,
        source: &Path,
    ) -> Result<(), MmError> {
        self.replace_volume_file(
            target,
            volume_name,
            volume_resource,
            file.name,
            source,
            file.ownership.mode,
        )
        .await
    }
    async fn remove_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
    ) -> Result<(), MmError> {
        Err(MmError::Security {
            message: "runtime adapter does not implement owned volume-file removal".to_owned(),
        })
    }
    /// Pulls the images of `services` (every service when `None`) that
    /// the engine does not have yet, within `limit` for all of them, so
    /// that starting them pulls nothing.
    async fn pull_images(
        &self,
        _target: &RuntimeTarget,
        _services: Option<&[String]>,
        _limit: std::time::Duration,
    ) -> Result<(), MmError> {
        Ok(())
    }
    async fn apply(&self, target: &RuntimeTarget) -> Result<(), MmError>;
    async fn remove(&self, target: &RuntimeTarget) -> Result<(), MmError>;
    /// Like `apply`, but containers of services the project no longer lists
    /// keep running. A cluster node applies this way: mm removes what left
    /// the node itself, a moved service only after it runs elsewhere.
    async fn apply_keeping_other_services(&self, _target: &RuntimeTarget) -> Result<(), MmError> {
        Err(MmError::RuntimeResponse {
            message: "this runtime cannot apply a project and keep its other services".to_owned(),
        })
    }
    /// Starts, or replaces, only `services` of the target's project, and
    /// none of what they depend on; the other services stay as they are.
    /// `counts` as in `apply_scaled`, for the services it names.
    async fn apply_services(
        &self,
        _target: &RuntimeTarget,
        _services: &[String],
        _counts: &std::collections::BTreeMap<String, u32>,
    ) -> Result<(), MmError> {
        Err(MmError::RuntimeResponse {
            message: "this runtime cannot start some services of a project alone".to_owned(),
        })
    }
    /// `apply` (`apply_keeping_other_services` with `keep_others`) that
    /// runs `counts[service]` containers of each service `counts` names,
    /// and one of every other service.
    async fn apply_scaled(
        &self,
        target: &RuntimeTarget,
        counts: &std::collections::BTreeMap<String, u32>,
        keep_others: bool,
    ) -> Result<(), MmError> {
        if let Some((service, count)) = counts.iter().find(|(_, count)| **count != 1) {
            return Err(MmError::RuntimeResponse {
                message: format!(
                    "this runtime cannot run {count} containers of service '{service}' on one node; remove its `replicas` and `autoscale`, or deploy with the docker-compose or podman-compose runtime"
                ),
            });
        }
        if keep_others {
            self.apply_keeping_other_services(target).await
        } else {
            self.apply(target).await
        }
    }
    /// Starts containers of `service` from the target's files until the
    /// engine has `count` of it, the containers it has counted: none of
    /// those is recreated.
    async fn scale_service(
        &self,
        _target: &RuntimeTarget,
        service: &str,
        _count: u32,
    ) -> Result<(), MmError> {
        Err(MmError::RuntimeResponse {
            message: format!(
                "this runtime cannot start another container of service '{service}'; remove its `autoscale`, or deploy with the docker-compose or podman-compose runtime"
            ),
        })
    }
    /// The ids of the containers of `services` in the target's project,
    /// running or not.
    async fn service_container_ids(
        &self,
        _target: &RuntimeTarget,
        _services: &[String],
    ) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }
    /// For each of `services` that runs a container the target's files
    /// would replace: starts one more container from those files next to
    /// it and leaves every other container alone. Returns the services it
    /// did that for (also those a start that was interrupted left with two
    /// containers).
    async fn start_beside(
        &self,
        _target: &RuntimeTarget,
        _services: &[String],
    ) -> Result<Vec<String>, MmError> {
        Err(MmError::RuntimeResponse {
            message: "this runtime cannot start a service's new container next to the old one; use `update.order: stop-first`".to_owned(),
        })
    }
    /// The containers of `services` that a newer container of the same
    /// service replaces: every container but the newest of each.
    async fn replaced_containers(
        &self,
        _target: &RuntimeTarget,
        _services: &[String],
    ) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }
    /// Whether the target's project runs exactly what applying its files
    /// would start, `counts[service]` containers of each service it names
    /// and one of every other, so a deployment leaves it as it is
    /// (`domain::rollout::stack_unchanged`). A runtime that cannot tell
    /// says no.
    async fn stack_unchanged(
        &self,
        _target: &RuntimeTarget,
        _counts: &std::collections::BTreeMap<String, u32>,
    ) -> Result<bool, MmError> {
        Ok(false)
    }
    /// The running containers of `services` that applying the target's
    /// files would replace: their configuration differs from the files'.
    /// A runtime that cannot tell says none.
    async fn outdated_containers(
        &self,
        _target: &RuntimeTarget,
        _services: &[String],
    ) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }
    /// Stops, then removes, the containers `containers` names by id.
    async fn remove_containers(
        &self,
        _target: &RuntimeTarget,
        containers: &[String],
    ) -> Result<(), MmError> {
        if containers.is_empty() {
            return Ok(());
        }
        Err(MmError::RuntimeResponse {
            message: "this runtime cannot remove single containers".to_owned(),
        })
    }
    /// Stops and removes the containers of `services` in the target's
    /// project, whether or not its Compose files still list them. With
    /// `whole_project`, the project has no service left on this host, so
    /// the networks Compose created for it go too. Volumes always stay.
    async fn remove_services(
        &self,
        _target: &RuntimeTarget,
        _services: &[String],
        _whole_project: bool,
    ) -> Result<(), MmError> {
        Err(MmError::RuntimeResponse {
            message: "this runtime cannot remove single services of a project".to_owned(),
        })
    }
}

#[async_trait]
pub trait IngressVerifier: Send + Sync {
    async fn verify_listener(&self, port: u16) -> Result<(), MmError>;
    async fn verify_route(&self, port: u16, host: &str, path: &str) -> Result<(), MmError>;
}

#[async_trait]
pub trait RootlessIngressInstaller: Send + Sync {
    async fn install(
        &self,
        plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError>;
    async fn uninstall(
        &self,
        plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError>;
}

#[async_trait]
pub trait PrivilegedIngressInstaller: Send + Sync {
    async fn install(
        &self,
        plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError>;
    async fn uninstall(
        &self,
        plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError>;
}

#[cfg(test)]
mod tests;
