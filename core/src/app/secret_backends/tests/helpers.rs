use super::*;
use async_trait::async_trait;

#[async_trait]
impl RemoteSecretBackendProvisioner for FakeRemote {
    async fn setup(
        &self,
        request: RemoteSecretBackendSetupRequest,
        _bootstrap_credential: SecretMaterial,
    ) -> Result<RemoteSecretBackendStatus, MmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result(&request.instance, &request.environment)
    }

    async fn status(
        &self,
        request: &RemoteSecretBackendStatusRequest,
    ) -> Result<RemoteSecretBackendStatus, MmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result(&request.instance, &request.environment)
    }
}

impl FakeRemote {
    fn result(
        &self,
        instance: &ProviderInstanceName,
        environment: &str,
    ) -> Result<RemoteSecretBackendStatus, MmError> {
        if !self.succeed {
            return Err(MmError::State {
                message: "unexpected fake effect".to_owned(),
            });
        }
        Ok(RemoteSecretBackendStatus {
            instance: instance.clone(),
            driver: ProviderDriverId::new("future")
                .map_err(|message| MmError::State { message })?,
            driver_display_name: "Future Safe".to_owned(),
            mode: ProviderMode::External,
            scope: ProviderScope::Project,
            capabilities: vec![ProviderOperation::Add, ProviderOperation::Test],
            environment: environment.to_owned(),
            health: crate::domain::secret_backend::SecretBackendHealth {
                initialized: true,
                sealed: false,
                standby: false,
                healthy: true,
                version: "1".to_owned(),
            },
            authenticated: true,
            isolated_scope: "project/local".to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

pub(super) fn external_descriptor(
    operations: Vec<ProviderOperation>,
    configuration: Vec<crate::domain::secret_backend::ProviderConfigurationField>,
) -> Result<ProviderDescriptor, String> {
    Ok(ProviderDescriptor {
        driver: ProviderDriverId::new("future")?,
        display_name: "Future Safe".to_owned(),
        modes: vec![ProviderModeDescriptor {
            mode: ProviderMode::External,
            scopes: vec![ProviderScope::Project],
            operations,
            configuration,
        }],
    })
}

pub(super) fn external_instance() -> Result<(ProviderInstanceName, ProviderInstanceConfig), String>
{
    let name = ProviderInstanceName::new("team-secrets")?;
    Ok((
        name,
        ProviderInstanceConfig {
            driver: ProviderDriverId::new("future")?,
            scope: ProviderScope::Project,
            configuration: crate::domain::secret_backend::ProviderConfiguration::External {
                address: "https://future.example.test".to_owned(),
                namespace: None,
                storage_mount: "secrets".to_owned(),
                automation_auth_mount: "automation".to_owned(),
                trust_bundle: "ca.pem".into(),
                expected_trust_sha256: format!("sha256:{}", "a".repeat(64)),
            },
        },
    ))
}

pub(super) fn external_request(
    instance: ProviderInstanceName,
    operation: ProviderOperation,
) -> RemoteSecretBackendSetupRequest {
    RemoteSecretBackendSetupRequest {
        instance,
        operation,
        address: "https://future.example.test".to_owned(),
        namespace: None,
        storage_mount: "secrets".to_owned(),
        automation_auth_mount: "automation".to_owned(),
        trust_bundle: "ca.pem".into(),
        expected_trust_sha256: format!("sha256:{}", "a".repeat(64)),
        project_id: crate::domain::workspace::ProjectId(uuid::Uuid::nil()),
        environment: "local".to_owned(),
        locations: vec!["database/password".to_owned()],
        allow_bootstrap: false,
        target: "local".to_owned(),
        config_hash: "b".repeat(64),
    }
}
