use super::*;

pub(crate) struct EnvironmentServices {
    pub(crate) manager: EnvironmentManager,
    pub(crate) secrets: SecretManager,
    age: Arc<AgeSecretStore>,
}

impl EnvironmentServices {
    pub(crate) fn initialize_age(&self) -> Result<InitializedAgeStore, MmError> {
        self.age.initialize()
    }

    pub(crate) fn age_initialized(&self) -> bool {
        self.age.initialized()
    }
}

pub(crate) struct GateServices {
    pub(crate) manager: GateManager,
    age: Arc<AgeSecretStore>,
}

impl GateServices {
    pub(crate) fn initialize_selected_backend(&self, provider: &str) -> Result<(), MmError> {
        if provider != "age" {
            return Ok(());
        }
        if !self.age.initialized() {
            self.age.initialize()?;
        }
        Ok(())
    }
}

pub(crate) fn environment_services(
    start: &Path,
    explicit_home: Option<&Path>,
    clock: Arc<dyn Clock>,
) -> Result<EnvironmentServices, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        identity_path(&home),
        home.secrets_directory
            .join(format!("{}.age", document.config.project.id.0)),
    ));
    let revision_key = crate::adapters::secrets::load_or_create_environment_revision_key(
        &home.identities_directory.join("environment-revisions.key"),
    )?;
    let registry = secret_provider_registry(
        Arc::clone(&age),
        revision_key,
        &home,
        document.config.project.id,
        &document.config,
    )?;
    let inspector: Arc<dyn SecretMetadataInspector> = registry.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = registry.clone();
    let creator: Arc<dyn SecretCreator> = registry.clone();
    let replacer: Arc<dyn SecretReplacer> = registry.clone();
    let batch_replacer: Arc<dyn SecretBatchReplacer> = registry.clone();
    let remover: Arc<dyn SecretRemover> = registry.clone();
    let storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog> = registry;
    let delivery_registry = Arc::new(FilesystemEnvironmentArtifactStore::new(
        home.run_directory.join("env"),
    )?);
    let delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog> = delivery_registry;
    let documents = project_documents();
    let manager = EnvironmentManager::new(EnvironmentDependencies {
        documents: Arc::clone(&documents),
        sources: environment_sources(),
        writer: materialized_environment_writer(),
        state,
        values,
        inspector: Arc::clone(&inspector),
        resolver,
        storage_capabilities: Arc::clone(&storage_capabilities),
        delivery_capabilities: Arc::clone(&delivery_capabilities),
        clock: Arc::clone(&clock),
    });
    let secrets = SecretManager::new(SecretManagerDependencies {
        documents,
        inspector,
        creator,
        replacer,
        batch_replacer,
        remover,
        storage_capabilities,
        delivery_capabilities,
        clock,
    });
    Ok(EnvironmentServices {
        manager,
        secrets,
        age,
    })
}

pub(crate) fn gate_services(
    start: &Path,
    explicit_home: Option<&Path>,
    clock: Arc<dyn Clock>,
) -> Result<GateServices, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let age = Arc::new(AgeSecretStore::new(
        identity_path(&home),
        home.secrets_directory
            .join(format!("{}.age", document.config.project.id.0)),
    ));
    let revision_key = crate::adapters::secrets::load_or_create_environment_revision_key(
        &home.identities_directory.join("environment-revisions.key"),
    )?;
    let secrets = secret_provider_registry(
        age.clone(),
        revision_key,
        &home,
        document.config.project.id,
        &document.config,
    )?;
    let secret_inspector: Arc<dyn SecretMetadataInspector> = secrets.clone();
    let secret_resolver: Arc<dyn SecretMaterialResolver> = secrets.clone();
    let secret_creator: Arc<dyn SecretCreator> = secrets.clone();
    let secret_remover: Arc<dyn SecretRemover> = secrets;
    Ok(GateServices {
        manager: gate_manager_with_secrets(
            home,
            sqlite,
            GateSecretPorts {
                inspector: secret_inspector,
                resolver: secret_resolver,
                creator: secret_creator,
                remover: secret_remover,
            },
            clock,
        )?,
        age,
    })
}

pub(super) struct GateSecretPorts {
    pub inspector: Arc<dyn SecretMetadataInspector>,
    pub resolver: Arc<dyn SecretMaterialResolver>,
    pub creator: Arc<dyn SecretCreator>,
    pub remover: Arc<dyn SecretRemover>,
}

pub(super) fn gate_manager_with_secrets(
    home: MmHome,
    sqlite: Arc<SqliteStateStore>,
    secrets: GateSecretPorts,
    clock: Arc<dyn Clock>,
) -> Result<GateManager, MmError> {
    let repository: Arc<dyn GateRepository> = sqlite.clone();
    let routes: Arc<dyn IngressRouteRegistry> = sqlite;
    let GateSecretPorts {
        inspector: secret_inspector,
        resolver: secret_resolver,
        creator: secret_creator,
        remover: secret_remover,
    } = secrets;
    let targets = FilesystemTargetStore::new(home.targets_file.clone());
    let local = targets
        .resolve_target("local")
        .map_err(|_| MmError::Target {
            message:
                "the local target is missing; run `mm dev setup` before managing access policies"
                    .to_owned(),
        })?;
    let managed_cap_endpoint = format!(
        "http://127.0.0.1:{}",
        ingress_verify_port(local.definition.id)
    );
    let (cap_endpoint, cap_host) = match std::env::var("MM_CAP_INTERNAL_ENDPOINT") {
        Ok(endpoint) => (endpoint, None),
        Err(_) => (managed_cap_endpoint.clone(), Some(CAP_HOST)),
    };
    let mut cap = ReqwestCapTokenVerifier::new(cap_endpoint)?;
    if let Some(host) = cap_host {
        cap = cap.with_host_header(host);
    }
    let service_store = Arc::new(FilesystemServiceStackStore::new(
        home.services_directory.clone(),
    ));
    let process: Arc<dyn ProcessRunner> = Arc::new(TokioProcessRunner);
    let runtimes = RuntimeFactory::new(process);
    let dynamic_configs = dynamic_config_store(&home);
    let cap_admin_endpoint = std::env::var("MM_CAP_ADMIN_ENDPOINT").unwrap_or(managed_cap_endpoint);
    let password: Arc<dyn crate::app::ports::PasswordVerifier> = Arc::new(Argon2idPasswordVerifier);
    let cap: Arc<dyn crate::app::ports::CapTokenVerifier> = Arc::new(cap);
    let password_cap: Arc<dyn crate::app::ports::GateDriver> =
        Arc::new(PasswordCapGateDriver::new(
            Arc::clone(&repository),
            Arc::clone(&password),
            Arc::clone(&cap),
            Arc::clone(&secret_resolver),
            Arc::clone(&clock),
        )?);
    Ok(GateManager::new(GateManagerDependencies {
        repository,
        routes,
        ingress: Arc::new(RuntimeGateIngressPublisher::new(
            local,
            service_store.clone(),
            runtimes,
            dynamic_configs,
        )),
        target_locks: Arc::new(FilesystemTargetOperationLock::new(
            home.target_locks_directory,
        )),
        password,
        sessions: Arc::new(XChaChaSessionCipher),
        drivers: GateDriverRegistry::new(vec![password_cap])?,
        cap_sites: Arc::new(ReqwestCapSiteProvisioner::new(
            cap_admin_endpoint,
            CAP_HOST,
            service_store.cap_admin_key_file(),
        )?),
        secret_inspector,
        secret_resolver,
        secret_creator,
        secret_remover,
        clock,
    }))
}
