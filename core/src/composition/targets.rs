use super::*;

pub(crate) struct TargetServices {
    pub(crate) manager: TargetManager,
    documents: Arc<dyn ProjectDocumentStore>,
    targets: Arc<dyn TargetStore>,
    home: MmHome,
    process: Arc<dyn ProcessRunner>,
    clock: Arc<dyn Clock>,
}

impl TargetServices {
    /// The state store when it already exists, without creating it.
    pub(crate) fn existing_state(&self) -> Result<Option<Arc<dyn StateStore>>, MmError> {
        if !self.home.state_database.is_file() {
            return Ok(None);
        }
        let state: Arc<dyn StateStore> =
            Arc::new(SqliteStateStore::open(&self.home.state_database)?);
        Ok(Some(state))
    }

    pub(crate) fn bootstrap(self) -> Result<TargetBootstrapService, MmError> {
        let state: Arc<dyn StateStore> =
            Arc::new(SqliteStateStore::open(&self.home.state_database)?);
        let executor = Arc::new(AnsibleTargetBootstrapExecutor::new(
            Arc::clone(&self.process),
            self.home.run_directory.join("bootstrap"),
        )?);
        Ok(TargetBootstrapService::new(
            self.manager,
            TargetBootstrapExecutionPorts {
                documents: self.documents,
                state,
                locks: Arc::new(FilesystemTargetOperationLock::new(
                    self.home.target_locks_directory,
                )),
                executor,
                clock: self.clock,
            },
        ))
    }

    pub(crate) fn ingress(&self) -> TargetIngressManager {
        target_ingress_manager(&self.home, Arc::clone(&self.targets))
    }
}

pub(crate) fn target_services(
    explicit_home: Option<&Path>,
    process: Arc<dyn ProcessRunner>,
    clock: Arc<dyn Clock>,
) -> Result<TargetServices, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.targets_file.clone()));
    let documents = project_documents();
    Ok(TargetServices {
        manager: TargetManager::new(Arc::clone(&documents), Arc::clone(&targets))
            .with_host_releases(Arc::new(SshHostReleaseReader::new(Arc::clone(&process))))
            .with_host_binaries(super::host_binary_source(&home)?),
        documents,
        targets,
        home,
        process,
        clock,
    })
}

pub(crate) fn local_ca_manager(
    explicit_home: Option<&Path>,
    system_scope: bool,
    process: Arc<dyn ProcessRunner>,
) -> Result<LocalCaManager, MmError> {
    let home = MmHome::inspect(explicit_home)?;
    let scope = if system_scope || cfg!(target_os = "linux") {
        TrustScope::System
    } else {
        TrustScope::CurrentUser
    };
    Ok(LocalCaManager::new(
        Arc::new(FilesystemLocalCaStore::new(home.ca_directory)),
        Arc::new(PlatformTrustStore::new(
            home.root.join("trust"),
            process,
            scope,
        )),
    ))
}

pub fn ensure_local_target(explicit_home: Option<&Path>) -> Result<(), MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(home.targets_file));
    TargetManager::new(project_documents(), targets).ensure_local()?;
    Ok(())
}

pub(crate) async fn authorize_local_setup(
    explicit_home: Option<&Path>,
    process: Arc<dyn ProcessRunner>,
) -> Result<(), MmError> {
    let home = MmHome::inspect(explicit_home)?;
    let scope = if cfg!(target_os = "linux") {
        TrustScope::System
    } else {
        TrustScope::CurrentUser
    };
    PlatformTrustStore::new(home.root.join("trust"), process, scope)
        .authorize_interactive()
        .await
}

pub(crate) async fn prepare_public_ingress(
    process: Arc<dyn ProcessRunner>,
    allow_prompt: bool,
) -> Result<(), MmError> {
    crate::adapters::target::IngressPrerequisites::new(process)
        .ensure(allow_prompt)
        .await
}

pub fn install_ingress(
    artifact_root: &Path,
    target_id: TargetId,
    artifact_hash: &str,
) -> Result<(), MmError> {
    install_privileged_ingress_artifact(artifact_root, target_id, artifact_hash)
}

pub fn uninstall_ingress(target_id: TargetId, artifact_hash: &str) -> Result<(), MmError> {
    uninstall_privileged_ingress(target_id, artifact_hash)
}
