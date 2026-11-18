use super::*;

pub(super) fn installation_store(home: &MmHome) -> Arc<FilesystemServiceStackStore> {
    Arc::new(FilesystemServiceStackStore::new(
        home.services_directory.clone(),
    ))
}

pub(crate) async fn local_services_runtime(
    home: Option<&Path>,
    explicit: Option<RuntimeKind>,
    services: &RuntimeComposition,
) -> Result<RuntimeKind, MmError> {
    use crate::app::ports::ServiceStackStore;
    let home = MmHome::inspect(home)?;
    if let Some(state) = FilesystemServiceStackStore::new(home.services_directory).load_state()? {
        if explicit.is_some_and(|runtime| runtime != state.runtime) {
            return Err(MmError::Target {
                message: format!(
                    "shared services are owned by {:?}; select that runtime rather than switching the installation",
                    state.runtime
                ),
            });
        }
        return Ok(state.runtime);
    }
    if let Some(runtime) = explicit {
        return Ok(runtime);
    }
    let mut available = Vec::new();
    for kind in [RuntimeKind::DockerCompose, RuntimeKind::PodmanCompose] {
        let probe = services.driver(kind).probe().await;
        if probe.available
            && !probe
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == crate::domain::diagnostic::Severity::Error)
        {
            available.push(kind);
        }
    }
    match available.as_slice() {
        [runtime] => Ok(*runtime),
        [] => Err(MmError::Target { message: "no local Compose runtime is available; start Docker Compose or Podman Compose, then retry `mm ingress start`".to_owned() }),
        _ => Err(MmError::Target { message: "multiple local runtimes are available; choose `mm --runtime docker-compose ingress start` or `mm --runtime podman-compose ingress start`".to_owned() }),
    }
}

pub(crate) fn job_scheduler_service(
    start: &Path,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<JobSchedulerService, MmError> {
    job_scheduler_service_on(Default::default(), start, explicit_home, runtime, services)
}

/// Jobs as a host runs them: on a cluster node, `scope` makes the node's
/// view of the configuration, under the host's runtime policy, the one that
/// decides which jobs run here.
pub(crate) fn job_scheduler_service_on(
    scope: crate::app::DocumentScope<'_>,
    start: &Path,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<JobSchedulerService, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let repository: Arc<dyn SchedulerRepository> =
        Arc::new(SqliteStateStore::open(&home.state_database)?);
    let age = Arc::new(AgeSecretStore::new(
        identity_path(&home),
        home.secrets_directory
            .join(format!("{}.age", document.config.project.id.0)),
    ));
    let revision_key = crate::adapters::secrets::load_or_create_environment_revision_key(
        &home.identities_directory.join("environment-revisions.key"),
    )?;
    let registry = secret_provider_registry(
        age,
        revision_key,
        &home,
        document.config.project.id,
        &document.config,
    )?;
    let secrets: Arc<dyn SecretMaterialResolver> = registry;
    let authorizer: Arc<dyn RuntimeOperationAuthorizer> = Arc::new(LocalRuntimeOperationAuthorizer);
    Ok(JobSchedulerService::new(JobSchedulerDependencies {
        application: persistent_application_on(
            scope,
            explicit_home,
            services.driver(runtime),
            services.clock(),
        )?,
        repository,
        runtime: services.factory.jobs(runtime)?,
        secrets,
        authorizer,
        clock: services.clock(),
    })
    .with_launch_targets(Arc::new(DeployedJobTargets {
        home: explicit_home.map(Path::to_path_buf),
    })))
}

/// A job's container gets what the environment's last deployment on this
/// host gave its service.
struct DeployedJobTargets {
    home: Option<std::path::PathBuf>,
}

#[async_trait::async_trait]
impl crate::app::ports::JobLaunchTargets for DeployedJobTargets {
    async fn prepare(
        &self,
        selection: &WorkspaceSelection,
        stack: &str,
        run: uuid::Uuid,
    ) -> Result<Option<crate::app::ports::PreparedJobTarget>, MmError> {
        RuntimeComposition::new()
            .deployment_service(selection, self.home.as_deref())?
            .job_target(selection, stack, run)
            .await
    }
}

pub(crate) fn service_stack_manager(
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<ServiceStackManager, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.targets_file.clone()));
    let ingress = target_ingress_manager(&home, Arc::clone(&targets));
    let executor = Arc::new(local_service_executor(&home, runtime, services)?);
    let store = installation_store(&home);
    let dashboard_ingress_token = store.load_or_create_dashboard_token()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let gate_policies: Arc<dyn crate::app::ports::GatePolicySource> = sqlite;
    let documents = project_documents();
    Ok(ServiceStackManager::new(ServiceStackManagerConfig {
        runtime,
        targets,
        ingress,
        store,
        executor,
        locks: Arc::new(FilesystemTargetOperationLock::new(
            home.target_locks_directory,
        )),
        clock: services.clock(),
        dashboard_ingress_token,
        dashboard_service: DashboardServiceConfig {
            executable: std::env::current_exe()
                .and_then(std::fs::canonicalize)
                .map_err(|source| MmError::Io {
                    context: "resolving the installed mm executable for dashboard service"
                        .to_owned(),
                    source,
                })?,
            mm_home: home.root,
        },
    })
    .with_dashboard_hosts(Arc::new(RegisteredDashboardHostSource::new(
        documents, state,
    )))
    .with_gate_policies(gate_policies))
}

pub(crate) async fn inspect_service_stack(
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<crate::domain::service_stack::ServiceStackStatus, MmError> {
    let home = MmHome::inspect(explicit_home)?;
    let executor = local_service_executor(&home, runtime, services)?;
    let store = FilesystemServiceStackStore::new(home.services_directory);
    ServiceStackManager::inspect_status(runtime, &store, &executor).await
}

fn local_service_executor(
    home: &MmHome,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<LocalServiceStackExecutor, MmError> {
    Ok(LocalServiceStackExecutor::new(
        LocalServiceStackExecutorConfig {
            routes: Arc::new(crate::adapters::state::SqliteIngressRouteReader {
                path: home.state_database.clone(),
            }),
            dynamic_configs: dynamic_config_store(home),
            targets: Arc::new(FilesystemTargetStore::new(home.targets_file.clone())),
            runtime: services.factory.deployment(runtime)?,
            inspector: services.factory.driver(runtime),
            ca: Arc::new(FilesystemLocalCaStore::new(home.ca_directory.clone())),
            verifier: Arc::new(TcpIngressVerifier::default()),
            rootless: rootless_ingress_installer(services.process())?,
            privileged: privileged_ingress_installer(services.process())?,
            dashboard: dashboard_service_installer(services.process())?,
        },
    ))
}

pub(super) fn dynamic_config_store(
    home: &MmHome,
) -> Arc<dyn crate::app::ports::TraefikDynamicConfigStore> {
    Arc::new(FilesystemTraefikDynamicConfigStore::new(
        home.ingress_directory.join("dynamic"),
    ))
}

pub(crate) fn dashboard_services(
    _start: &Path,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<
    (
        DashboardQueryService,
        Arc<dyn crate::app::ports::DashboardSubscription>,
        String,
        GateManager,
    ),
    MmError,
> {
    let home = MmHome::resolve(explicit_home)?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let scheduler: Arc<dyn SchedulerRepository> = sqlite.clone();
    let documents = project_documents();
    let secrets = Arc::new(super::dashboard_secrets::DashboardGateSecrets::new(
        home.clone(),
        state.clone(),
    ));
    let operations = RuntimeOperationsService::new(RuntimeOperationsDependencies {
        application: persistent_application(
            explicit_home,
            services.driver(runtime),
            services.clock(),
        )?,
        driver: services.factory.operations(runtime)?,
        secrets: secrets.clone(),
        authorizer: Arc::new(LocalRuntimeOperationAuthorizer),
        audit: sqlite.clone(),
        clock: services.clock(),
    });
    let gates = gate_manager_with_secrets(
        home.clone(),
        sqlite,
        GateSecretPorts {
            inspector: secrets.clone(),
            resolver: secrets.clone(),
            creator: secrets.clone(),
            remover: secrets,
        },
        services.clock(),
    )?;
    let installation: Arc<dyn crate::app::ports::InstallationStatusQuery> =
        Arc::new(service_stack_manager(explicit_home, runtime, services)?);
    let events: Arc<dyn crate::app::ports::DashboardSubscription> =
        Arc::new(DashboardEventHub::start(services.clock()));
    let dashboard_ingress_token = FilesystemServiceStackStore::new(home.services_directory)
        .load_or_create_dashboard_token()?;
    Ok((
        DashboardQueryService::new(
            operations,
            documents,
            state,
            scheduler,
            installation,
            services.clock(),
        ),
        events,
        dashboard_ingress_token,
        gates,
    ))
}

/// The jobs of a deployment, run by the scheduler of the same home as the
/// node the selection is.
pub(super) struct SchedulerJobs {
    pub(super) home: Option<std::path::PathBuf>,
    pub(super) runtime: RuntimeKind,
}

impl SchedulerJobs {
    fn scheduler(&self, selection: &WorkspaceSelection) -> Result<JobSchedulerService, MmError> {
        job_scheduler_service_on(
            selection.document_scope(),
            &selection.start,
            self.home.as_deref(),
            self.runtime,
            &RuntimeComposition::new(),
        )
    }
}

#[async_trait::async_trait]
impl crate::app::ports::DeploymentJobs for SchedulerJobs {
    async fn run(
        &self,
        selection: &WorkspaceSelection,
        job: &str,
        target: crate::domain::runtime::RuntimeTarget,
        plan_hash: &str,
    ) -> Result<crate::domain::scheduler::JobRun, MmError> {
        self.scheduler(selection)?
            .run(selection, job, target, plan_hash)
            .await
    }

    async fn sweep(
        &self,
        selection: &WorkspaceSelection,
        projects: &[crate::domain::runtime::RuntimeTarget],
    ) -> Result<Vec<String>, MmError> {
        self.scheduler(selection)?.sweep(selection, projects).await
    }
}
