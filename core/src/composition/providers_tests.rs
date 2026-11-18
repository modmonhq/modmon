use std::collections::BTreeMap;

use super::*;
use crate::app::ports::{ManagedSecretBackendProvisioner, ManagedServiceRuntime};
use crate::domain::managed_service::{
    ManagedServicePlan, ManagedServicePlanRequest, ManagedServiceStatus,
};
use crate::domain::secret_backend::{
    ManagedSecretBackendAction, ManagedSecretBackendPlanRequest, ProviderConfiguration,
    ProviderDriverId, ProviderInstanceConfig, ProviderInstanceName, ProviderScope,
};
use crate::domain::workspace::ProjectId;

struct InertRuntime;

#[async_trait::async_trait]
impl ManagedServiceRuntime for InertRuntime {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::PodmanCompose
    }

    fn plan(&self, _request: ManagedServicePlanRequest) -> Result<ManagedServicePlan, MmError> {
        Err(MmError::State {
            message: "inert runtime".to_owned(),
        })
    }

    async fn apply(&self, _plan: &ManagedServicePlan) -> Result<(), MmError> {
        Err(MmError::State {
            message: "inert runtime".to_owned(),
        })
    }

    async fn observe(&self, _plan: &ManagedServicePlan) -> Result<ManagedServiceStatus, MmError> {
        Err(MmError::State {
            message: "inert runtime".to_owned(),
        })
    }

    async fn remove(&self, _plan: &ManagedServicePlan, _purge_data: bool) -> Result<(), MmError> {
        Err(MmError::State {
            message: "inert runtime".to_owned(),
        })
    }
}

fn instance(driver: &str, trust_bundle: PathBuf) -> Result<ProviderInstanceConfig, String> {
    Ok(ProviderInstanceConfig {
        driver: ProviderDriverId::new(driver)?,
        scope: ProviderScope::Project,
        configuration: ProviderConfiguration::External {
            address: "https://127.0.0.1:8200".to_owned(),
            namespace: None,
            storage_mount: "secret".to_owned(),
            automation_auth_mount: "approle".to_owned(),
            trust_bundle,
            expected_trust_sha256: "a".repeat(64),
        },
    })
}

#[tokio::test]
async fn provider_composition_builds_administrators_managers_and_instance_stores()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let home_path = temp.path().join("home");
    let home = MmHome::resolve(Some(&home_path))?;
    let openbao_admin = openbao_administrator(Some(&home_path))?;
    let vault_admin = vault_administrator(Some(&home_path))?;
    let runtime: Arc<dyn ManagedServiceRuntime> = Arc::new(InertRuntime);
    let openbao_managed = managed_openbao_manager(&home, Arc::clone(&runtime));
    let vault_managed = managed_vault_manager(&home, runtime);
    let openbao = Arc::new(OpenBaoBackendUseCases::new(
        openbao_admin,
        Some(openbao_managed),
    ));
    let vault = Arc::new(VaultBackendUseCases::new(vault_admin, Some(vault_managed)));

    let openbao_name = ProviderInstanceName::new("openbao-managed")?;
    let vault_name = ProviderInstanceName::new("vault-managed")?;
    let openbao_status =
        ManagedSecretBackendProvisioner::status(openbao.as_ref(), &openbao_name).await?;
    assert_eq!(openbao_status.diagnostics.len(), 1);
    let vault_status = ManagedSecretBackendProvisioner::status(vault.as_ref(), &vault_name).await?;
    assert_eq!(vault_status.diagnostics.len(), 1);
    for (backend, instance_name, release, port) in [
        (
            openbao.as_ref() as &dyn ManagedSecretBackendProvisioner,
            openbao_name,
            "2.4",
            18_200,
        ),
        (
            vault.as_ref() as &dyn ManagedSecretBackendProvisioner,
            vault_name,
            "1.19",
            18_210,
        ),
    ] {
        assert!(
            backend
                .plan(ManagedSecretBackendPlanRequest {
                    instance: instance_name,
                    action: ManagedSecretBackendAction::Install,
                    installation: "local".to_owned(),
                    release: release.to_owned(),
                    management_port: port,
                    purge_data: false,
                    project_id: ProjectId(uuid::Uuid::from_u128(46)),
                    environment: "development".to_owned(),
                    locations: vec!["application/key".to_owned()],
                    target: "local".to_owned(),
                    config_hash: "a".repeat(64),
                })
                .is_err()
        );
    }
    let _backends = backend_registry(openbao, vault, BTreeMap::new())?;

    let mut instances = BTreeMap::new();
    instances.insert(
        ProviderInstanceName::new("openbao-main")?,
        instance("openbao", temp.path().join("openbao.pem"))?,
    );
    instances.insert(
        ProviderInstanceName::new("vault-main")?,
        instance("vault", temp.path().join("vault.pem"))?,
    );
    instances.insert(
        ProviderInstanceName::new("unknown-main")?,
        instance("unknown", temp.path().join("unknown.pem"))?,
    );
    let age = Arc::new(AgeSecretStore::new(
        home.identities_directory.join("age.key"),
        home.secrets_directory.join("project.age"),
    ));
    let registry = registry_of(
        age,
        [7; 32],
        &home,
        ProjectId(uuid::Uuid::from_u128(45)),
        &instances,
        &BTreeMap::new(),
    )?;
    assert!(Arc::strong_count(&registry) >= 1);

    let services = RuntimeComposition::new();
    let _interactive = interactive_secret_backend_manager(
        Some(&home_path),
        &services,
        RuntimeKind::PodmanCompose,
        BTreeMap::new(),
    )?;
    Ok(())
}
