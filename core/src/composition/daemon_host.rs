//! What `mmd` owns on its own host that a daemon's use cases reach: the
//! mesh, the cluster networks, the name resolver, shared storage, and the
//! keepers that let containers leave and run the ingress proxy.

use super::*;

/// The keepers of a cluster node that run beside the daemon.
pub(super) struct NodeHost {
    pub(super) leaving: Arc<dyn crate::app::ports::ContainerLeaving>,
    pub(super) ingress: Arc<dyn crate::app::ports::IngressHost>,
    /// The count of each scaled service, from the node's autoscaler.
    pub(super) scale: Option<Arc<dyn crate::app::ports::ScaleState>>,
}

impl DaemonUseCaseAdapter {
    #[must_use]
    pub fn new(mm_home: PathBuf) -> Self {
        Self {
            mm_home,
            mesh: None,
            networks: None,
            names: None,
            storage: None,
            host_policy: None,
            engine: super::super::engine_facts::HostEngine::default(),
            node_host: None,
        }
    }

    /// The host's policy file: plans keep to the OCI runtimes it offers
    /// and asks for.
    #[must_use]
    pub fn with_host_policy(mut self, path: PathBuf) -> Self {
        self.host_policy = Some(path);
        self
    }

    /// The host's shared storage: a node of a cluster target makes a
    /// plan's shared volumes ready, reports them, and deletes one on
    /// request.
    #[must_use]
    pub fn with_storage(mut self, storage: crate::app::SharedStorageNode) -> Self {
        self.storage = Some(storage);
        self
    }

    /// The host's name resolver: a node's status says what each service
    /// name resolves to.
    #[must_use]
    pub fn with_names(mut self, names: Arc<dyn crate::app::ports::ServiceNames>) -> Self {
        self.names = Some(names);
        self
    }

    /// The host's cluster networks: a node of a cluster target creates the
    /// networks of `mm.yaml` on the addresses it stores here.
    #[must_use]
    pub fn with_networks(
        mut self,
        networks: Arc<dyn crate::app::ports::ClusterNetworkHost>,
    ) -> Self {
        self.networks = Some(networks);
        self
    }

    /// The host's own mesh interface, for `mm target mesh status`.
    #[must_use]
    pub fn with_mesh(mut self, mesh: Arc<dyn crate::app::ports::MeshHost>) -> Self {
        self.mesh = Some(mesh);
        self
    }

    /// The keepers beside the daemon: a container a rollout replaces
    /// leaves through `leaving`, and the node serves the cluster's routes
    /// through `ingress`.
    #[must_use]
    pub fn with_node_host(
        mut self,
        leaving: Arc<dyn crate::app::ports::ContainerLeaving>,
        ingress: Arc<dyn crate::app::ports::IngressHost>,
    ) -> Self {
        self.node_host = Some(NodeHost {
            leaving,
            ingress,
            scale: None,
        });
        self
    }

    /// The keeper of the containers that leave this node, on a node that
    /// has one.
    pub(super) fn node_leaving(&self) -> Option<Arc<dyn crate::app::ports::ContainerLeaving>> {
        self.node_host
            .as_ref()
            .map(|host| Arc::clone(&host.leaving))
    }

    /// The node's autoscaler: a deployment runs each scaled service's
    /// share of the count the nodes agreed on. Needs `with_node_host`.
    #[must_use]
    pub fn with_scale_state(mut self, scale: Arc<dyn crate::app::ports::ScaleState>) -> Self {
        if let Some(host) = self.node_host.as_mut() {
            host.scale = Some(scale);
        }
        self
    }

    pub(super) fn selection(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<WorkspaceSelection, MmError> {
        Ok(self
            .selection_at(&context.scope, &context.project_root)?
            .with_release(context.release.clone()))
    }

    pub(super) fn selection_at(
        &self,
        scope: &crate::domain::remote::RemoteScope,
        project_root: &Path,
    ) -> Result<WorkspaceSelection, MmError> {
        let document = ConfigLoader.load_from(project_root)?;
        if document.config.project.id != scope.project_id {
            return Err(MmError::Security {
                message: "authorized remote root contains another project identity".to_owned(),
            });
        }
        let environment = resolve_environment(&document, Some(&scope.environment))?;
        if environment != scope.environment {
            return Err(MmError::Security {
                message: "remote environment scope changed during resolution".to_owned(),
            });
        }
        let mut selection = WorkspaceSelection::new(project_root, Some(scope.environment.clone()));
        selection.runtime = Some(scope.runtime);
        selection.target = Some("local".to_owned());
        selection.host_values = true;
        selection.host_runtimes =
            super::super::runtimes::host_runtimes(self.host_policy.as_deref())?.map(Arc::new);
        // Rootful Podman gives each service user ids of its own that mm
        // chooses, so that files mm hands over are the container's.
        if scope.runtime == RuntimeKind::PodmanCompose {
            selection.user_ids = Some(Arc::new(
                crate::adapters::user_namespace::FilesystemServiceUserIds::on_host(&self.mm_home),
            ));
        }
        // A cluster node plans, applies, and reports what is placed on it.
        if let Some(node) = &scope.node {
            selection.node = Some(crate::domain::placement::NodeScope {
                target: scope.target.clone(),
                node: node.clone(),
                environment: scope.environment.clone(),
                resolver_files: self.networks.as_ref().map(|host| host.resolver_files()),
                mesh_addresses: super::super::storage::mesh_addresses(self.mesh.as_deref(), node)?,
            });
        }
        Ok(selection)
    }
}
