use super::*;

fn openbao_administrator(
    explicit_home: Option<&Path>,
) -> Result<crate::adapters::secrets::OpenBaoConnectionAdministrator, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let connections = Arc::new(OpenBaoConnectionStore::new(
        home.identities_directory.join("openbao-connections.key"),
        home.secrets_directory.join("openbao-connections.age"),
    ));
    let transport: Arc<dyn crate::adapters::secrets::OpenBaoHttpTransport> =
        Arc::new(ReqwestOpenBaoTransport);
    Ok(crate::adapters::secrets::OpenBaoConnectionAdministrator::new(connections, transport))
}

fn managed_openbao_manager(
    home: &MmHome,
    runtime: Arc<dyn crate::app::ports::ManagedServiceRuntime>,
) -> crate::adapters::secrets::ManagedOpenBaoManager {
    let connections = Arc::new(OpenBaoConnectionStore::new(
        home.identities_directory.join("openbao-connections.key"),
        home.secrets_directory.join("openbao-connections.age"),
    ));
    let transport: Arc<dyn crate::adapters::secrets::OpenBaoHttpTransport> =
        Arc::new(ReqwestOpenBaoTransport);
    crate::adapters::secrets::ManagedOpenBaoManager::new(
        home.services_directory.join("openbao"),
        connections,
        transport,
        runtime,
    )
}

fn vault_administrator(
    explicit_home: Option<&Path>,
) -> Result<crate::adapters::secrets::VaultConnectionAdministrator, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let connections = Arc::new(VaultConnectionStore::new(
        home.identities_directory.join("vault-connections.key"),
        home.secrets_directory.join("vault-connections.age"),
    ));
    let transport: Arc<dyn crate::adapters::secrets::VaultHttpTransport> =
        Arc::new(ReqwestVaultTransport);
    Ok(crate::adapters::secrets::VaultConnectionAdministrator::new(
        connections,
        transport,
    ))
}

fn managed_vault_manager(
    home: &MmHome,
    runtime: Arc<dyn crate::app::ports::ManagedServiceRuntime>,
) -> crate::adapters::secrets::ManagedVaultManager {
    let connections = Arc::new(VaultConnectionStore::new(
        home.identities_directory.join("vault-connections.key"),
        home.secrets_directory.join("vault-connections.age"),
    ));
    let transport: Arc<dyn crate::adapters::secrets::VaultHttpTransport> =
        Arc::new(ReqwestVaultTransport);
    crate::adapters::secrets::ManagedVaultManager::new(
        home.services_directory.join("vault"),
        connections,
        transport,
        runtime,
    )
}

pub(crate) fn interactive_secret_backend_manager(
    explicit_home: Option<&Path>,
    services: &RuntimeComposition,
    runtime_kind: RuntimeKind,
    instances: std::collections::BTreeMap<
        crate::domain::secret_backend::ProviderInstanceName,
        crate::domain::secret_backend::ProviderInstanceConfig,
    >,
) -> Result<crate::app::SecretBackendManager, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.targets_file.clone()));
    let runtime: Arc<dyn crate::app::ports::ManagedServiceRuntime> =
        Arc::new(SelectedTargetManagedServiceRuntime::new(
            home.services_directory.join("managed-secret-runtime"),
            targets,
            runtime_kind,
            services.factory.deployment(runtime_kind)?,
            services.factory.driver(runtime_kind),
        ));
    let openbao = Arc::new(OpenBaoBackendUseCases::new(
        openbao_administrator(explicit_home)?,
        Some(managed_openbao_manager(&home, Arc::clone(&runtime))),
    ));
    let vault = Arc::new(VaultBackendUseCases::new(
        vault_administrator(explicit_home)?,
        Some(managed_vault_manager(&home, runtime)),
    ));
    backend_registry(openbao, vault, instances)
}

fn backend_registry(
    openbao: Arc<OpenBaoBackendUseCases>,
    vault: Arc<VaultBackendUseCases>,
    instances: std::collections::BTreeMap<
        crate::domain::secret_backend::ProviderInstanceName,
        crate::domain::secret_backend::ProviderInstanceConfig,
    >,
) -> Result<crate::app::SecretBackendManager, MmError> {
    let openbao_remote: Arc<dyn crate::app::ports::RemoteSecretBackendProvisioner> =
        openbao.clone();
    let openbao_managed: Arc<dyn crate::app::ports::ManagedSecretBackendProvisioner> = openbao;
    let vault_remote: Arc<dyn crate::app::ports::RemoteSecretBackendProvisioner> = vault.clone();
    let vault_managed: Arc<dyn crate::app::ports::ManagedSecretBackendProvisioner> = vault;
    crate::app::SecretBackendManager::new(
        vec![
            crate::app::ProviderDriverRegistration {
                descriptor: openbao_provider_descriptor()?,
                remote: Some(openbao_remote),
                managed: Some(openbao_managed),
            },
            crate::app::ProviderDriverRegistration {
                descriptor: vault_provider_descriptor()?,
                remote: Some(vault_remote),
                managed: Some(vault_managed),
            },
        ],
        instances,
    )
}

pub(crate) fn provider_driver_descriptors()
-> Result<Vec<crate::domain::secret_backend::ProviderDescriptor>, MmError> {
    Ok(vec![
        openbao_provider_descriptor()?,
        vault_provider_descriptor()?,
    ])
}

pub(super) fn identity_path(home: &MmHome) -> PathBuf {
    std::env::var_os("MM_AGE_IDENTITY_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.identities_directory.join("age.key"))
}

/// The managed store of a cluster environment as this host reads it,
/// without what is staged here.
pub(super) fn cluster_store(
    home: &MmHome,
    project_id: crate::domain::workspace::ProjectId,
    provider: &str,
) -> Result<OpenBaoSecretStore, MmError> {
    let name = crate::domain::secret_backend::ProviderInstanceName::new(provider)
        .map_err(|message| MmError::Secret { message })?;
    Ok(OpenBaoSecretStore::new(
        name,
        crate::adapters::secrets::OpenBaoMode::Managed,
        project_id,
        Arc::new(OpenBaoConnectionStore::new(
            home.identities_directory.join("openbao-connections.key"),
            home.secrets_directory.join("openbao-connections.age"),
        )),
        Arc::new(ReqwestOpenBaoTransport),
    ))
}

pub(super) fn secret_provider_registry(
    age: Arc<AgeSecretStore>,
    environment_revision_key: [u8; 32],
    home: &MmHome,
    project_id: crate::domain::workspace::ProjectId,
    config: &crate::domain::config::MmConfig,
) -> Result<Arc<SecretProviderRegistry>, MmError> {
    // A deployment host reads a store only where the commit declares it.
    registry_of(
        age,
        environment_revision_key,
        home,
        project_id,
        &config.providers,
        &config.declared_provider_addresses(),
    )
}

fn registry_of(
    age: Arc<AgeSecretStore>,
    environment_revision_key: [u8; 32],
    home: &MmHome,
    project_id: crate::domain::workspace::ProjectId,
    instances: &std::collections::BTreeMap<
        crate::domain::secret_backend::ProviderInstanceName,
        crate::domain::secret_backend::ProviderInstanceConfig,
    >,
    declared: &std::collections::BTreeMap<
        crate::domain::secret_backend::ProviderInstanceName,
        crate::domain::repo_target::DeclaredProviderAddress,
    >,
) -> Result<Arc<SecretProviderRegistry>, MmError> {
    let environment = Arc::new(EnvironmentSecretStore::new(environment_revision_key));
    let age_inspector: Arc<dyn SecretMetadataInspector> = age.clone();
    let age_resolver: Arc<dyn SecretMaterialResolver> = age.clone();
    let age_creator: Arc<dyn SecretCreator> = age.clone();
    let age_replacer: Arc<dyn SecretReplacer> = age.clone();
    let age_batch_replacer: Arc<dyn SecretBatchReplacer> = age.clone();
    let age_candidates: Arc<dyn crate::app::ports::SecretCandidateLifecycle> = age.clone();
    let staged = Arc::clone(&age);
    let age_remover: Arc<dyn SecretRemover> = age;
    let environment_inspector: Arc<dyn SecretMetadataInspector> = environment.clone();
    let environment_resolver: Arc<dyn SecretMaterialResolver> = environment;
    let connection_store = Arc::new(OpenBaoConnectionStore::new(
        home.identities_directory.join("openbao-connections.key"),
        home.secrets_directory.join("openbao-connections.age"),
    ));
    let transport: Arc<dyn crate::adapters::secrets::OpenBaoHttpTransport> =
        Arc::new(ReqwestOpenBaoTransport);
    let vault_connections = Arc::new(VaultConnectionStore::new(
        home.identities_directory.join("vault-connections.key"),
        home.secrets_directory.join("vault-connections.age"),
    ));
    let vault_transport: Arc<dyn crate::adapters::secrets::VaultHttpTransport> =
        Arc::new(ReqwestVaultTransport);
    let mut registrations = vec![
        StorageProviderRegistration {
            descriptor: age_storage_descriptor()?,
            inspector: age_inspector,
            resolver: age_resolver,
            creator: Some(age_creator),
            replacer: Some(age_replacer),
            batch_replacer: Some(age_batch_replacer),
            candidates: Some(age_candidates),
            remover: Some(age_remover),
        },
        StorageProviderRegistration {
            descriptor: environment_storage_descriptor()?,
            inspector: environment_inspector,
            resolver: environment_resolver,
            creator: None,
            replacer: None,
            batch_replacer: None,
            candidates: None,
            remover: None,
        },
    ];
    for (name, instance) in instances {
        let mode = instance.configuration.mode();
        match instance.driver.as_str() {
            "openbao" => {
                let mut store = OpenBaoSecretStore::new(
                    name.clone(),
                    mode.into(),
                    project_id,
                    Arc::clone(&connection_store),
                    Arc::clone(&transport),
                )
                .declared_at(declared.get(name).cloned());
                if crate::domain::secret_store::is_cluster_store(instance) {
                    store = store.staged_in(Arc::clone(&staged));
                }
                registrations.push(openbao_registration(name, Arc::new(store))?);
            }
            "vault" => {
                let store = Arc::new(
                    VaultSecretStore::new(
                        name.clone(),
                        mode.into(),
                        project_id,
                        Arc::clone(&vault_connections),
                        Arc::clone(&vault_transport),
                    )
                    .declared_at(declared.get(name).cloned()),
                );
                registrations.push(vault_registration(name, store)?);
            }
            _ => {}
        }
    }
    Ok(Arc::new(SecretProviderRegistry::new(registrations)?))
}

fn vault_registration(
    name: &crate::domain::secret_backend::ProviderInstanceName,
    store: Arc<VaultSecretStore>,
) -> Result<StorageProviderRegistration, MmError> {
    let inspector: Arc<dyn SecretMetadataInspector> = store.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = store.clone();
    let creator: Arc<dyn SecretCreator> = store.clone();
    let replacer: Arc<dyn SecretReplacer> = store.clone();
    let remover: Arc<dyn SecretRemover> = store;
    Ok(StorageProviderRegistration {
        descriptor: vault_storage_descriptor(name)?,
        inspector,
        resolver,
        creator: Some(creator),
        replacer: Some(replacer),
        batch_replacer: None,
        candidates: None,
        remover: Some(remover),
    })
}

fn openbao_registration(
    name: &crate::domain::secret_backend::ProviderInstanceName,
    store: Arc<OpenBaoSecretStore>,
) -> Result<StorageProviderRegistration, MmError> {
    let inspector: Arc<dyn SecretMetadataInspector> = store.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = store.clone();
    let creator: Arc<dyn SecretCreator> = store.clone();
    let replacer: Arc<dyn SecretReplacer> = store.clone();
    let remover: Arc<dyn SecretRemover> = store;
    Ok(StorageProviderRegistration {
        descriptor: openbao_storage_descriptor(name)?,
        inspector,
        resolver,
        creator: Some(creator),
        replacer: Some(replacer),
        batch_replacer: None,
        candidates: None,
        remover: Some(remover),
    })
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
