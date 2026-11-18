use super::*;

pub(crate) type RemoteTransportContext = (
    Arc<dyn crate::app::ports::RemoteTransport>,
    crate::domain::remote::RemoteScope,
);

pub async fn serve_stdio_relay(
    socket_path: &Path,
    expected_daemon_uid: u32,
    idle_timeout: std::time::Duration,
) -> Result<(), MmError> {
    crate::adapters::remote::serve_stdio_relay(socket_path, expected_daemon_uid, idle_timeout).await
}

pub(crate) fn remote_transport_context(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
) -> Result<Option<RemoteTransportContext>, MmError> {
    remote_transport_context_as(selection, explicit_home, runtime, RemoteIdentity::User)
}

/// Whose key a remote connection authenticates with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteIdentity {
    /// `MM_SSH_KEY`, the target's key file, or this device's key.
    User,
    /// Only this device's agent key, so the host applies the agent's limits.
    Agent,
}

pub(crate) fn remote_transport_context_as(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    identity: RemoteIdentity,
) -> Result<Option<RemoteTransportContext>, MmError> {
    remote_environment_as(selection, explicit_home, runtime, identity)?
        .map(|environment| environment.first_connection())
        .transpose()
}

/// The selected environment's remote hosts: the one host of a single-host
/// target, or every node of a cluster target. `None` for a local target.
pub(crate) fn remote_environment(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
) -> Result<Option<crate::app::RemoteEnvironment>, MmError> {
    remote_environment_as(selection, explicit_home, runtime, RemoteIdentity::User)
}

pub(crate) fn remote_environment_as(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    identity: RemoteIdentity,
) -> Result<Option<crate::app::RemoteEnvironment>, MmError> {
    let document = ConfigLoader.load_from(&selection.start)?;
    let environment = resolve_environment(&document, selection.environment.as_deref())?;
    let home = MmHome::resolve(explicit_home)?;
    let state = SqliteStateStore::open(&home.state_database)?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.targets_file.clone()));
    let (environment_pref, project_pref) =
        crate::app::load_selection_preferences(Some(&state), &document, &environment)?;
    let context = crate::app::resolve_request_context(
        &document,
        &environment,
        (selection.runtime, runtime),
        selection.target.as_deref(),
        (environment_pref.as_ref(), project_pref.as_ref()),
        targets.as_ref(),
    )?;
    let crate::domain::target::TargetTransport::Ssh { host, port, .. } =
        &context.target.definition.transport
    else {
        return Ok(None);
    };
    // A repo-declared target (no private catalog entry): pin its keys on first
    // use so a later commit cannot silently redirect this client.
    if let Some(repo) = document.config.targets.get(&context.target.name)
        && targets.resolve_target(&context.target.name).is_err()
    {
        super::target_trust::repo_target_pins(&home).check_or_record(
            &super::target_trust::pin_key(&document, &context.target.name),
            &context.target.name,
            &repo
                .pin(&context.target.name)
                .map_err(|message| MmError::Target { message })?,
        )?;
    }
    let (key, reuse, keys) = client_identity(&home, identity)?;
    let access = ClientAccess {
        key: &key,
        reuse: &reuse,
        keys: &keys,
    };
    let scope = crate::domain::remote::RemoteScope {
        project_id: document.config.project.id,
        environment: environment.clone(),
        target: context.target.name.clone(),
        target_fingerprint: context.target.fingerprint.clone(),
        runtime,
        node: None,
    };
    let nodes = if context.target.definition.is_cluster() {
        cluster_nodes(&document, &context.target, access, &scope)?
    } else {
        vec![crate::app::RemoteNode {
            name: None,
            host: host.clone(),
            port: *port,
            private_address: None,
            transport: Ok(Arc::new(SshRemoteTransport::for_target(
                &context.target,
                access,
            )?)),
            scope,
            placed: None,
        }]
    };
    let existing_interface =
        super::cluster_networks::existing_interface(&document, &context.target.name);
    let cluster_networks = super::cluster_networks::settings(&document, &context.target.name);
    let storage_node = super::storage::storage_node(&document, &context.target.name);
    let pushed = super::delivery::pushed_commits(&document, &context.target.name);
    // A value validation refuses falls back to the default here.
    let progress_timeout = crate::domain::config::DeployConfig::progress_timeout(
        document
            .config
            .environments
            .get(&environment)
            .and_then(|environment| environment.deploy.as_ref()),
    )
    .unwrap_or(crate::domain::config::DEFAULT_PROGRESS_TIMEOUT);
    Ok(Some(
        crate::app::RemoteEnvironment::new(context.target.name, nodes)?
            .on_existing_interface(existing_interface)
            .with_cluster_networks(cluster_networks)
            .with_environments(document.config.environments.keys().cloned().collect())
            .with_storage_node(storage_node)
            .with_pushed_commits(pushed)
            .with_progress_timeout(progress_timeout)
            .with_version_pin(crate::domain::version_pin::VersionPin::of(
                &document.config,
                &environment,
            )),
    ))
}

pub struct DaemonUseCaseAdapter {
    mm_home: PathBuf,
    mesh: Option<Arc<dyn crate::app::ports::MeshHost>>,
    networks: Option<Arc<dyn crate::app::ports::ClusterNetworkHost>>,
    names: Option<Arc<dyn crate::app::ports::ServiceNames>>,
    storage: Option<crate::app::SharedStorageNode>,
    host_policy: Option<PathBuf>,
    engine: super::engine_facts::HostEngine,
    node_host: Option<daemon_host::NodeHost>,
}

#[path = "remote_clients.rs"]
mod clients;
#[path = "daemon_host.rs"]
mod daemon_host;
use clients::{client_identity, cluster_nodes};

impl DaemonUseCaseAdapter {
    /// Remote hosts never build: a deployment's images come from the build
    /// receipt it was planned with, or from the receipt recorded when this
    /// checkout was last deployed. A receipt built for another environment
    /// is promoted when this environment builds the same units.
    async fn deployment(
        &self,
        selection: &WorkspaceSelection,
        receipt: Option<&crate::domain::build::BuildReceipt>,
    ) -> Result<crate::app::DeploymentService, MmError> {
        let composition = RuntimeComposition::new();
        let mut service = composition.deployment_service(selection, Some(&self.mm_home))?;
        if let (Some(networks), Some(_)) = (&self.networks, &selection.node) {
            service = service.with_cluster_networks(Arc::clone(networks));
        }
        if let (Some(names), Some(_)) = (&self.names, &selection.node) {
            service = service.with_names(Arc::clone(names));
        }
        if let (Some(host), Some(_)) = (&self.node_host, &selection.node) {
            service = service.with_node_host(Arc::clone(&host.leaving), Arc::clone(&host.ingress));
            if let Some(scale) = &host.scale {
                service = service.with_scale_state(Arc::clone(scale));
            }
        }
        let Some(receipt) = receipt else {
            return Ok(service);
        };
        // The checkout is a `git archive` export of the planned commit plus
        // its marker file, which no build context of the commit contains.
        let checkout = crate::app::ReceiptCheckout::Export {
            added: vec![
                selection
                    .start
                    .join(crate::adapters::remote::CHECKOUT_MARKER),
            ],
        };
        let promoted = composition
            .build_manager()
            .receipt_for_environment(selection, receipt, &checkout)
            .await?;
        Ok(service.with_receipt(promoted))
    }
}

#[async_trait::async_trait]
impl crate::app::ports::RemoteUseCases for DaemonUseCaseAdapter {
    async fn engine(&self, runtime: RuntimeKind) -> Option<crate::domain::remote::EngineFacts> {
        Some(self.engine.facts(runtime).await)
    }

    async fn secret_store(
        &self,
        scope: &crate::domain::remote::RemoteScope,
    ) -> Option<crate::domain::secret_store::StoreStatus> {
        super::secret_store::store_here(&self.mm_home, scope.project_id, &scope.environment).await
    }

    async fn store_snapshot(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        recipient: &str,
    ) -> Result<String, MmError> {
        super::secret_store::snapshot_here(&self.mm_home, &context.scope, recipient).await
    }

    async fn store_restore(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        step: crate::domain::secret_store::StoreRestoreStep,
    ) -> Result<crate::domain::secret_store::StoreRestored, MmError> {
        super::secret_store::restore_here(&self.mm_home, &context.scope, step).await
    }

    async fn mesh_status(
        &self,
        _context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<crate::domain::mesh::MeshNodeStatus, MmError> {
        match &self.mesh {
            Some(mesh) => mesh.status().await,
            None => Ok(crate::domain::mesh::MeshNodeStatus::default()),
        }
    }

    async fn networks(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<crate::domain::cluster_network::NodeNetworkStatus, MmError> {
        match &self.networks {
            Some(host) => {
                let runtime = RuntimeComposition::new().driver(context.scope.runtime);
                let mut status =
                    super::cluster_networks::status(host.as_ref(), runtime.as_ref()).await?;
                if let Some(mesh) = &self.mesh {
                    status.host_ranges = mesh.host_ranges().await?;
                }
                Ok(status)
            }
            None => Ok(crate::domain::cluster_network::NodeNetworkStatus::default()),
        }
    }

    async fn store_networks(
        &self,
        node: &str,
        networks: &crate::domain::cluster_network::SealedNetworks,
        converge: bool,
    ) -> Result<(), MmError> {
        let host = self.networks.as_ref().ok_or_else(|| MmError::Environment {
            message: "this daemon does not hold cluster networks".to_owned(),
        })?;
        super::cluster_networks::store(host.as_ref(), node, networks, converge).await
    }

    async fn retire_networks(&self, retired: &[String]) -> Result<Vec<String>, MmError> {
        match self.networks.as_ref() {
            Some(host) => super::cluster_networks::retire(host.as_ref(), retired).await,
            None => Ok(Vec::new()),
        }
    }

    async fn prepare_storage(
        &self,
        node: &str,
        storage: &crate::domain::shared_storage::SealedStorage,
    ) -> Result<Vec<String>, MmError> {
        super::storage::node(self.storage.as_ref())?
            .prepare(node, storage)
            .await
    }

    async fn runtimes(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<crate::domain::hardening::HostRuntimeStatus, MmError> {
        // The host's policy and its engine: no project file is read.
        super::runtimes::check(context.scope.runtime, self.host_policy.as_deref()).await
    }

    async fn storage_status(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        storage_node: &str,
        sizes: bool,
    ) -> Result<crate::domain::shared_storage::StorageNodeStatus, MmError> {
        super::storage::node(self.storage.as_ref())?
            .status(super::storage::scope(context), storage_node, sizes)
            .await
    }

    async fn plan_volume_delete(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        volume: &crate::domain::shared_storage::VolumeName,
    ) -> Result<crate::domain::shared_storage::VolumeDeletePlan, MmError> {
        super::storage::node(self.storage.as_ref())?
            .plan_delete(super::storage::scope(context), volume)
            .await
    }

    async fn delete_volume(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        volume: &crate::domain::shared_storage::VolumeName,
        plan_hash: &str,
    ) -> Result<crate::domain::shared_storage::VolumeDeleted, MmError> {
        super::storage::node(self.storage.as_ref())?
            .delete(super::storage::scope(context), volume, plan_hash)
            .await
    }

    async fn status(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
        self.status_here(context).await
    }

    async fn logs(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        request: crate::domain::observability::LogRequest,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<crate::app::ports::LogStream, MmError> {
        let selection = self.selection(context)?;
        let services = RuntimeComposition::new();
        runtime_operations_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &services,
        )?
        .logs(&selection, request, cancellation)
        .await
    }

    async fn exec(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        request: crate::domain::observability::ExecRequest,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<crate::app::ports::ExecStream, MmError> {
        let selection = self.selection(context)?;
        let services = RuntimeComposition::new();
        runtime_operations_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &services,
        )?
        .exec(&selection, request, cancellation)
        .await
    }

    async fn forward_address(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        service: &str,
        port: u16,
    ) -> Result<std::net::SocketAddr, MmError> {
        let selection = self.selection(context)?;
        let services = RuntimeComposition::new();
        runtime_operations_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &services,
        )?
        .forward_address(&selection, service, port)
        .await
    }

    async fn secret_recipient(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<String, MmError> {
        super::host_values::secret_recipient(&self.mm_home, context)
    }

    async fn stage_values(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        stage: crate::domain::staged::ValueStage,
    ) -> Result<crate::domain::staged::StageAnswer, MmError> {
        let mut selections = vec![self.selection(context)?];
        if let Some(environment) = context.project_root.parent() {
            for root in crate::adapters::remote::checkouts_in(environment)
                .into_iter()
                .take(8)
            {
                if root != context.project_root
                    && let Ok(selection) = self.selection_at(&context.scope, &root)
                {
                    selections.push(selection);
                }
            }
        }
        super::host_values::stage_values(&selections, &self.mm_home, context, stage).await
    }

    async fn host_values(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<crate::domain::staged::NodeValues, MmError> {
        let selection = self.selection(context)?;
        super::host_values::host_values(&selection, &self.mm_home, context).await
    }

    async fn store_migrate(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        provider: &str,
    ) -> Result<crate::domain::secret_store::StoreMigrated, MmError> {
        let selection = self.selection(context)?;
        super::host_values::migrate_to_store(&selection, &self.mm_home, context, provider).await
    }

    async fn staged_plan(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<Vec<crate::domain::staged::StagedPlanItem>, MmError> {
        let selection = self.selection(context)?;
        super::host_values::staged_plan(&selection, &self.mm_home, context).await
    }

    async fn go_live(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        staged: &[crate::domain::staged::StagedPlanItem],
    ) -> Result<(), MmError> {
        let selection = self.selection(context)?;
        super::host_values::go_live(&selection, &self.mm_home, context, staged).await
    }

    async fn plan_deployment(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        receipt: Option<&crate::domain::build::BuildReceipt>,
    ) -> Result<crate::domain::deployment::DeploymentPlan, MmError> {
        let selection = self.selection(context)?;
        let unset = super::host_values::unset_required(&selection, &self.mm_home, context).await?;
        if !unset.is_empty() {
            return Err(unset_on_host(&context.scope.environment, &unset));
        }
        let planned = self
            .deployment(&selection, receipt)
            .await?
            .plan(&selection)
            .await;
        match planned {
            Err(MmError::Setup {
                issue: crate::domain::readiness::SetupIssue::MissingImages { services },
            }) if receipt.is_none() => Err(MmError::Build {
                message: format!(
                    "{} need published images, and remote hosts never build. Publish them, write `mm build receipt FILE`, and plan with `mm deploy plan --receipt FILE`",
                    services.join(", ")
                ),
            }),
            Err(MmError::Validation { errors }) => Err(super::daemon_checkout::invalid_at_commit(
                &selection, errors,
            )),
            planned => planned,
        }
    }

    async fn apply_deployment(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
        receipt: Option<&crate::domain::build::BuildReceipt>,
    ) -> Result<crate::domain::deployment::DeploymentResult, MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, receipt)
            .await?
            .apply_reviewed(&selection, deployment_plan_hash)
            .await
    }

    async fn names(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<crate::domain::discovery::NameReport, MmError> {
        let ours = super::cluster_networks::environment_networks(context, &context.project_root);
        Ok(self.names.as_ref().map_or_else(Default::default, |names| {
            names
                .report(&crate::domain::cluster_network::network_key(
                    context.scope.project_id,
                    &context.scope.environment,
                    "",
                ))
                .only_networks(ours)
        }))
    }

    async fn begin_rollout(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
        receipt: Option<&crate::domain::build::BuildReceipt>,
    ) -> Result<(), MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, receipt)
            .await?
            .begin_rollout(&selection, deployment_plan_hash)
            .await
    }

    async fn roll_stack(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
        stack: &str,
        step: crate::app::StackStep,
    ) -> Result<crate::app::StackOutcome, MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, None)
            .await?
            .roll_stack(&selection, deployment_plan_hash, stack, step)
            .await
    }

    async fn roll_job(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
        stack: &str,
        job: &str,
    ) -> Result<crate::domain::scheduler::JobRun, MmError> {
        let selection = self.selection(context)?;
        // A rollout step's reviewed plan is the cluster plan.
        let label = crate::domain::scheduler::deployment_label(
            context.request_plan_hash.as_deref(),
            deployment_plan_hash,
        );
        (self.deployment(&selection, None).await?)
            .roll_job(&selection, (deployment_plan_hash, label), stack, job)
            .await
    }

    async fn finish_rollout(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
    ) -> Result<crate::domain::deployment::DeploymentResult, MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, None)
            .await?
            .finish_rollout(&selection, deployment_plan_hash)
            .await
    }

    async fn rollback_rollout(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
        changes: Option<&crate::domain::rollout::RolloutChanges>,
    ) -> Result<bool, MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, None)
            .await?
            .rollback_rollout(&selection, deployment_plan_hash, changes)
            .await
    }

    async fn take_back_leaves(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
        stack: &str,
    ) -> Result<Vec<String>, MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, None)
            .await?
            .take_back_leaves(&selection, deployment_plan_hash, stack)
            .await
    }

    async fn restart(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        request: crate::domain::observability::RestartRequest,
    ) -> Result<crate::domain::observability::RestartOutcome, MmError> {
        self.restart_here(&context.scope, &context.project_root, request)
            .await
    }

    async fn restart_at(
        &self,
        scope: &crate::domain::remote::RemoteScope,
        root: &std::path::Path,
        request: crate::domain::observability::RestartRequest,
    ) -> Result<crate::domain::observability::RestartOutcome, MmError> {
        self.restart_here(scope, root, request).await
    }

    async fn run_job(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        request: crate::domain::scheduler::JobRunRequest,
    ) -> Result<crate::domain::scheduler::JobRun, MmError> {
        self.run_job_here(context, request).await
    }

    async fn job_runs(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        request: crate::domain::scheduler::JobRunsRequest,
    ) -> Result<Vec<crate::domain::scheduler::JobRun>, MmError> {
        self.job_runs_here(context, request).await
    }

    async fn cancel_job_runs(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        started: &[crate::domain::scheduler::StartedRuns],
    ) -> Result<Vec<crate::domain::scheduler::JobRun>, MmError> {
        self.cancel_job_runs_here(context, started).await
    }

    async fn running_job_runs(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
    ) -> Result<Vec<crate::domain::rollout::OperationInFlight>, MmError> {
        self.running_job_runs_here(context)
    }

    async fn apply_ingress(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        node: &str,
        ingress: &crate::domain::cluster_ingress::SealedIngress,
    ) -> Result<(), MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, None)
            .await?
            .apply_cluster_ingress(&selection, node, ingress)
            .await
    }

    async fn finish_moves(
        &self,
        context: &crate::app::ports::RemoteRequestContext,
        deployment_plan_hash: &str,
    ) -> Result<Vec<crate::domain::placement::PlacedService>, MmError> {
        let selection = self.selection(context)?;
        self.deployment(&selection, None)
            .await?
            .finish_moves(&selection, deployment_plan_hash)
            .await
    }

    async fn scale_service(
        &self,
        scope: &crate::domain::remote::RemoteScope,
        root: &std::path::Path,
        deployment_plan_hash: &str,
        service: (&str, &str),
        count: u32,
    ) -> Result<(), MmError> {
        let selection = self.selection_at(scope, root)?;
        self.deployment(&selection, None)
            .await?
            .scale_service(&selection, deployment_plan_hash, service, count)
            .await
    }
}

#[path = "remote_operations.rs"]
mod operations;
#[path = "remote_status.rs"]
mod status;

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;

/// A plan refused because the node would deploy without a required value.
fn unset_on_host(
    environment: &str,
    unset: &[(crate::domain::staged::ValueKind, String, Vec<String>)],
) -> MmError {
    let listed = unset
        .iter()
        .map(|(kind, key, services)| {
            let command = match kind {
                crate::domain::staged::ValueKind::Secret => "secret",
                crate::domain::staged::ValueKind::Variable => "env",
            };
            format!(
                "{} '{key}' (used by {}): run `mm -e {environment} {command} set {key}`",
                kind.as_str(),
                services.join(", ")
            )
        })
        .collect::<Vec<_>>();
    MmError::Environment {
        message: format!(
            "this host has no value for what the deployment needs: {}. Then plan again",
            listed.join("; ")
        ),
    }
}
