use super::*;

pub(crate) fn project_documents() -> Arc<dyn ProjectDocumentStore> {
    Arc::new(ConfigLoader)
}

/// The document store for a request: on a remote host, one that reads the
/// configuration under the host's runtime policy, and on a cluster node
/// as that node sees it.
pub(crate) fn documents_on(scope: crate::app::DocumentScope<'_>) -> Arc<dyn ProjectDocumentStore> {
    let mut documents = project_documents();
    if !scope.development_with.is_empty() {
        documents = Arc::new(crate::app::DevelopmentDocuments::new(
            documents,
            scope.development_with.to_vec(),
        ));
    }
    if let Some(runtimes) = scope.host_runtimes {
        documents = Arc::new(crate::app::HostDocuments::new(documents, runtimes.clone()));
    }
    match scope.node {
        Some(node) => Arc::new(crate::app::NodeDocuments::new(documents, node.clone())),
        None => documents,
    }
}

pub(crate) fn onboarding_service() -> OnboardingService {
    OnboardingService::new(Arc::new(ConfigLoader))
}

pub(crate) fn environment_sources() -> Arc<dyn EnvironmentSourceDiscovery> {
    Arc::new(EnvironmentIndexer)
}

pub(crate) fn materialized_environment_writer() -> Arc<dyn MaterializedEnvironmentWriter> {
    Arc::new(FilesystemMaterializedEnvironmentWriter)
}

pub(crate) fn persistent_application(
    explicit_home: Option<&Path>,
    runtime: Arc<dyn RuntimeDriver>,
    clock: Arc<dyn Clock>,
) -> Result<Application, MmError> {
    persistent_application_on(Default::default(), explicit_home, runtime, clock)
}

/// The application for one request; `scope` says which host it is for.
pub(crate) fn persistent_application_on(
    scope: crate::app::DocumentScope<'_>,
    explicit_home: Option<&Path>,
    runtime: Arc<dyn RuntimeDriver>,
    clock: Arc<dyn Clock>,
) -> Result<Application, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let state = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let route_registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(home.targets_file));
    let documents = documents_on(scope);
    let stacks = StackManager::with_route_registry(
        Arc::clone(&documents),
        Arc::new(FilesystemStackArtifactStore::new(home.compose_directory)),
        route_registry,
        targets,
    )
    .with_state(state.clone())
    .with_user_ids(scope.user_ids.cloned());
    Ok(Application::new(ApplicationDependencies {
        documents,
        environment_sources: environment_sources(),
        runtime,
        state,
        stacks,
        clock,
    }))
}

#[cfg(test)]
pub(crate) fn inspection_application(
    explicit_home: Option<&Path>,
    runtime: Arc<dyn RuntimeDriver>,
    clock: Arc<dyn Clock>,
) -> Result<Application, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let route_registry: Arc<dyn IngressRouteRegistry> =
        Arc::new(SqliteStateStore::open(&home.state_database)?);
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(home.targets_file));
    let documents = project_documents();
    let stacks = StackManager::with_route_registry(
        Arc::clone(&documents),
        Arc::new(FilesystemStackArtifactStore::new(home.compose_directory)),
        route_registry,
        targets,
    );
    Ok(Application::runtime_inspection(
        runtime,
        clock,
        documents,
        environment_sources(),
        stacks,
    ))
}

pub(crate) fn stack_manager(explicit_home: Option<&Path>) -> Result<StackManager, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let registry: Arc<dyn IngressRouteRegistry> = sqlite.clone();
    let state: Arc<dyn StateStore> = sqlite;
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(home.targets_file));
    Ok(StackManager::with_route_registry(
        project_documents(),
        Arc::new(FilesystemStackArtifactStore::new(home.compose_directory)),
        registry,
        targets,
    )
    .with_state(state))
}

/// Lists and turns on or off the optional units of local development.
pub(crate) fn development_units() -> crate::app::DevelopmentUnitService {
    crate::app::DevelopmentUnitService::new(project_documents())
}

pub(crate) fn development_mount_manager(
    explicit_home: Option<&Path>,
    runtime: Arc<dyn RuntimeDriver>,
) -> Result<DevelopmentMountManager, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(home.targets_file));
    Ok(DevelopmentMountManager::new(
        project_documents(),
        targets,
        runtime,
    ))
}

pub(crate) fn runtime_operations_service(
    start: &Path,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<RuntimeOperationsService, MmError> {
    runtime_operations_service_on(Default::default(), start, explicit_home, runtime, services)
}

/// Status, logs, and commands for one request; `scope` says which host it
/// is for: a cluster node sees only the services placed on it.
pub(crate) fn runtime_operations_service_on(
    scope: crate::app::DocumentScope<'_>,
    start: &Path,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<RuntimeOperationsService, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let audit: Arc<dyn ExecAuditJournal> = Arc::new(SqliteStateStore::open(&home.state_database)?);
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
    Ok(RuntimeOperationsService::new(
        RuntimeOperationsDependencies {
            application: persistent_application_on(
                scope,
                explicit_home,
                services.driver(runtime),
                services.clock(),
            )?,
            driver: services.factory.operations(runtime)?,
            secrets,
            authorizer,
            audit,
            clock: services.clock(),
        },
    ))
}

pub(crate) fn preference_state(
    start: &Path,
    explicit_home: Option<&Path>,
) -> Result<(ConfigDocument, Arc<dyn StateStore>), MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let state: Arc<dyn StateStore> = Arc::new(SqliteStateStore::open(&home.state_database)?);
    Ok((document, state))
}
