use super::*;
use crate::domain::secret_backend::{
    ManagedSecretBackendAction, ProviderConfiguration, ProviderModeDescriptor, ProviderScope,
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct FakeManaged {
    applies: AtomicUsize,
    data_applies: AtomicUsize,
}

#[async_trait]
impl ManagedSecretBackendProvisioner for FakeManaged {
    fn plan(
        &self,
        request: ManagedSecretBackendPlanRequest,
    ) -> Result<ManagedSecretBackendPlan, MmError> {
        Ok(ManagedSecretBackendPlan {
            instance: request.instance,
            action: request.action,
            installation: request.installation,
            immutable_artifact: format!("provider@{}", request.release),
            previous_artifact: None,
            ingress_url: "https://provider.mm.test".to_owned(),
            management_access: "loopback".to_owned(),
            data_resource: "provider-data".to_owned(),
            purge_data: request.purge_data,
            plan_hash: "driver-lifecycle-hash".to_owned(),
        })
    }

    async fn apply(
        &self,
        request: ManagedSecretBackendPlanRequest,
        reviewed_plan_hash: &str,
    ) -> Result<ManagedSecretBackendResult, MmError> {
        assert_eq!(reviewed_plan_hash, "driver-lifecycle-hash");
        self.applies.fetch_add(1, Ordering::SeqCst);
        Ok(ManagedSecretBackendResult {
            instance: request.instance,
            action: request.action,
            plan_hash: reviewed_plan_hash.to_owned(),
            rolled_back: false,
            data_purged: request.purge_data,
        })
    }

    async fn status(
        &self,
        instance: &ProviderInstanceName,
    ) -> Result<ManagedSecretBackendStatus, MmError> {
        Ok(ManagedSecretBackendStatus {
            instance: instance.clone(),
            driver: ProviderDriverId::new("managed-fake")
                .map_err(|message| MmError::State { message })?,
            driver_display_name: "Managed Fake".to_owned(),
            mode: ProviderMode::Managed,
            scope: ProviderScope::Installation,
            capabilities: vec![ProviderOperation::Status],
            installed: true,
            installation: Some("local-secrets".to_owned()),
            connection: None,
            immutable_artifact: Some("provider@1".to_owned()),
            data_resource: Some("provider-data".to_owned()),
            ingress_url: "https://provider.mm.test".to_owned(),
            management_url: Some("https://127.0.0.1:18200".to_owned()),
            management_access: "loopback".to_owned(),
            health: None,
            recovery_available_off_instance: true,
            journal: None,
            diagnostics: Vec::new(),
        })
    }

    fn plan_data(
        &self,
        instance: &ProviderInstanceName,
        action: ManagedSecretDataAction,
        path: &Path,
    ) -> Result<ManagedSecretDataPlan, MmError> {
        Ok(ManagedSecretDataPlan {
            instance: instance.clone(),
            action,
            path: path.to_owned(),
            state_digest: "state-digest".to_owned(),
            source_digest: (action == ManagedSecretDataAction::Restore)
                .then(|| "source-digest".to_owned()),
            target: String::new(),
            config_hash: String::new(),
            consequences: vec![format!("{} encrypted provider data", action.as_str())],
            plan_hash: "driver-data-hash".to_owned(),
        })
    }

    async fn apply_data(
        &self,
        plan: &ManagedSecretDataPlan,
    ) -> Result<ManagedSecretDataResult, MmError> {
        assert_eq!(plan.plan_hash, "driver-data-hash");
        self.data_applies.fetch_add(1, Ordering::SeqCst);
        Ok(ManagedSecretDataResult {
            instance: ProviderInstanceName::new("driver-result")
                .map_err(|message| MmError::State { message })?,
            action: plan.action,
            path: plan.path.clone(),
            snapshot_digest: "snapshot-digest".to_owned(),
            encrypted: true,
            integrity_checked: true,
        })
    }
}

fn request(
    instance: ProviderInstanceName,
    action: ManagedSecretBackendAction,
) -> ManagedSecretBackendPlanRequest {
    ManagedSecretBackendPlanRequest {
        instance,
        action,
        installation: "local-secrets".to_owned(),
        project_id: crate::domain::workspace::ProjectId(uuid::Uuid::nil()),
        environment: "local".to_owned(),
        locations: vec!["database/password".to_owned()],
        release: "1.0".to_owned(),
        management_port: 18200,
        purge_data: false,
        target: "local".to_owned(),
        config_hash: "a".repeat(64),
    }
}

#[tokio::test]
async fn managed_lifecycle_and_data_plans_are_exact_reviewed_and_effect_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Arc::new(FakeManaged {
        applies: AtomicUsize::new(0),
        data_applies: AtomicUsize::new(0),
    });
    let driver = ProviderDriverId::new("managed-fake")?;
    let name = ProviderInstanceName::new("local-provider")?;
    let operations = vec![
        ProviderOperation::Install,
        ProviderOperation::Restart,
        ProviderOperation::Upgrade,
        ProviderOperation::Seal,
        ProviderOperation::Unseal,
        ProviderOperation::Remove,
        ProviderOperation::Status,
        ProviderOperation::Backup,
        ProviderOperation::Restore,
    ];
    let manager = SecretBackendManager::new(
        vec![ProviderDriverRegistration {
            descriptor: ProviderDescriptor {
                driver: driver.clone(),
                display_name: "Managed Fake".to_owned(),
                modes: vec![ProviderModeDescriptor {
                    mode: ProviderMode::Managed,
                    scopes: vec![ProviderScope::Installation],
                    operations,
                    configuration: Vec::new(),
                }],
            },
            remote: None,
            managed: Some(fake.clone()),
        }],
        [(
            name.clone(),
            ProviderInstanceConfig {
                driver,
                scope: ProviderScope::Installation,
                configuration: ProviderConfiguration::Managed {
                    installation: Some("local-secrets".to_owned()),
                    release: "1.0".to_owned(),
                    management_port: Some(18200),
                    nodes: None,
                },
            },
        )]
        .into_iter()
        .collect(),
    )?;

    for action in [
        ManagedSecretBackendAction::Install,
        ManagedSecretBackendAction::Restart,
        ManagedSecretBackendAction::Upgrade,
        ManagedSecretBackendAction::Seal,
        ManagedSecretBackendAction::Unseal,
        ManagedSecretBackendAction::Remove,
    ] {
        assert_eq!(
            manager.plan_managed(request(name.clone(), action))?.action,
            action
        );
    }
    let install = request(name.clone(), ManagedSecretBackendAction::Install);
    let plan = manager.plan_managed(install.clone())?;
    assert!(
        manager
            .apply_managed(install.clone(), "stale")
            .await
            .is_err()
    );
    assert_eq!(fake.applies.load(Ordering::SeqCst), 0);
    let result = manager.apply_managed(install, &plan.plan_hash).await?;
    assert_eq!(result.plan_hash, plan.plan_hash);
    assert_eq!(fake.applies.load(Ordering::SeqCst), 1);
    assert!(manager.managed_status(&name).await?.installed);

    for action in [
        ManagedSecretDataAction::Backup,
        ManagedSecretDataAction::Restore,
    ] {
        let plan = manager.plan_managed_data(
            &name,
            action,
            &PathBuf::from("/private/backup.age"),
            "local",
            &"b".repeat(64),
        )?;
        let mut stale = plan.clone();
        stale.config_hash.push('x');
        assert!(manager.apply_managed_data(&stale).await.is_err());
        let result = manager.apply_managed_data(&plan).await?;
        assert_eq!(result.instance, name);
        assert!(result.encrypted && result.integrity_checked);
    }
    assert_eq!(fake.data_applies.load(Ordering::SeqCst), 2);
    Ok(())
}
