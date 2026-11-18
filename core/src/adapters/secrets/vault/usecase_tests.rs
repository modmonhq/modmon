use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Method;
use tempfile::TempDir;

use crate::app::ports::{ManagedSecretBackendProvisioner, RemoteSecretBackendProvisioner};
use crate::domain::error::MmError;
use crate::domain::secret::SecretMaterial;
use crate::domain::secret_backend::{
    ManagedSecretBackendAction, ManagedSecretBackendPlanRequest, ManagedSecretDataAction,
    ManagedSecretDataPlan, ProviderInstanceName, ProviderMode, ProviderOperation,
    RemoteSecretBackendSetupRequest, RemoteSecretBackendStatusRequest,
};
use crate::domain::workspace::ProjectId;

use super::super::admin::{VaultConnectionAdministrator, VaultConnectionStatus};
use super::super::connection_store::{VaultConnection, VaultConnectionStore};
use super::super::http::{VaultHttpResponse, VaultHttpTransport};
use super::super::provider::VaultHealth;
use super::{
    VaultBackendUseCases, health, managed_request, remote_status, vault_provider_descriptor,
};

struct UnreachableTransport;

#[async_trait]
impl VaultHttpTransport for UnreachableTransport {
    async fn request(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        _path: &str,
        _token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        Err(MmError::State {
            message: "transport must not be reached".to_owned(),
        })
    }

    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        Err(MmError::State {
            message: "transport must not be reached".to_owned(),
        })
    }
}

fn backend() -> Result<(TempDir, VaultBackendUseCases), std::io::Error> {
    let temp = crate::test_support::tempdir()?;
    let connections = Arc::new(VaultConnectionStore::new(
        temp.path().join("identity.key"),
        temp.path().join("connections.age"),
    ));
    let administrator =
        VaultConnectionAdministrator::new(connections, Arc::new(UnreachableTransport));
    Ok((temp, VaultBackendUseCases::new(administrator, None)))
}

fn remote_request(
    path: &std::path::Path,
) -> Result<RemoteSecretBackendSetupRequest, std::io::Error> {
    Ok(RemoteSecretBackendSetupRequest {
        instance: ProviderInstanceName::new("primary").map_err(std::io::Error::other)?,
        operation: ProviderOperation::Add,
        address: "https://vault.example.test".to_owned(),
        namespace: None,
        storage_mount: "kv".to_owned(),
        automation_auth_mount: "approle".to_owned(),
        trust_bundle: path.to_path_buf(),
        expected_trust_sha256: "a".repeat(64),
        project_id: ProjectId(uuid::Uuid::nil()),
        environment: "development".to_owned(),
        locations: vec!["app/key".to_owned()],
        allow_bootstrap: false,
        target: "remote".to_owned(),
        config_hash: "b".repeat(64),
    })
}

fn request(release: &str) -> Result<ManagedSecretBackendPlanRequest, std::io::Error> {
    Ok(ManagedSecretBackendPlanRequest {
        instance: ProviderInstanceName::new("primary").map_err(std::io::Error::other)?,
        action: ManagedSecretBackendAction::Install,
        installation: "local".to_owned(),
        release: release.to_owned(),
        management_port: 18_210,
        purge_data: false,
        project_id: ProjectId(uuid::Uuid::nil()),
        environment: "development".to_owned(),
        locations: vec!["app".to_owned()],
        target: "local".to_owned(),
        config_hash: "a".repeat(64),
    })
}

fn vault_health(healthy: bool) -> VaultHealth {
    VaultHealth {
        initialized: true,
        sealed: false,
        standby: false,
        healthy,
        version: "1.19.0".to_owned(),
    }
}

#[test]
fn descriptor_and_managed_request_pin_supported_releases() -> Result<(), Box<dyn std::error::Error>>
{
    let descriptor = vault_provider_descriptor()?;
    assert_eq!(descriptor.driver.as_str(), "vault");
    assert!(
        descriptor
            .require(ProviderMode::Managed, ProviderOperation::Backup)
            .is_ok()
    );
    assert!(
        descriptor
            .require(ProviderMode::External, ProviderOperation::Install)
            .is_err()
    );
    assert!(managed_request(&request("1.18")?).is_ok());
    assert!(managed_request(&request("1.19")?).is_ok());
    assert!(managed_request(&request("latest")?).is_err());
    Ok(())
}

#[test]
fn status_mapping_preserves_health_and_emits_actionable_diagnostics()
-> Result<(), Box<dyn std::error::Error>> {
    let instance = ProviderInstanceName::new("primary").map_err(std::io::Error::other)?;
    let healthy = remote_status(
        instance.clone(),
        VaultConnectionStatus {
            environment: "development".to_owned(),
            health: vault_health(true),
            authenticated: true,
            isolated_path: "kv/data/project/development".to_owned(),
        },
    )?;
    assert!(healthy.diagnostics.is_empty());
    assert!(health(vault_health(true)).healthy);

    let unhealthy = remote_status(
        instance,
        VaultConnectionStatus {
            environment: "development".to_owned(),
            health: vault_health(false),
            authenticated: false,
            isolated_path: "kv/data/project/development".to_owned(),
        },
    )?;
    assert_eq!(unhealthy.diagnostics.len(), 1);
    Ok(())
}

#[tokio::test]
async fn use_case_facade_fails_closed_without_managed_backend_or_trust_material()
-> Result<(), Box<dyn std::error::Error>> {
    let (temp, backend) = backend()?;
    assert!(format!("{backend:?}").contains("VaultBackendUseCases"));
    assert!(backend.managed().is_err());

    let missing = temp.path().join("missing.pem");
    let credential = SecretMaterial::new("bootstrap".to_owned()).map_err(std::io::Error::other)?;
    assert!(
        backend
            .setup(remote_request(&missing)?, credential)
            .await
            .is_err()
    );
    let invalid = temp.path().join("invalid.pem");
    std::fs::write(&invalid, [0xff])?;
    let credential = SecretMaterial::new("bootstrap".to_owned()).map_err(std::io::Error::other)?;
    assert!(
        backend
            .setup(remote_request(&invalid)?, credential)
            .await
            .is_err()
    );

    let instance = ProviderInstanceName::new("primary").map_err(std::io::Error::other)?;
    let status = RemoteSecretBackendStatusRequest {
        instance: instance.clone(),
        project_id: ProjectId(uuid::Uuid::nil()),
        environment: "development".to_owned(),
    };
    assert!(
        RemoteSecretBackendProvisioner::status(&backend, &status)
            .await
            .is_err()
    );

    let managed = request("1.19")?;
    assert!(ManagedSecretBackendProvisioner::plan(&backend, managed.clone()).is_err());
    assert!(
        ManagedSecretBackendProvisioner::apply(&backend, managed, &"c".repeat(64))
            .await
            .is_err()
    );
    assert!(
        ManagedSecretBackendProvisioner::status(&backend, &instance)
            .await
            .is_err()
    );
    assert!(
        ManagedSecretBackendProvisioner::plan_data(
            &backend,
            &instance,
            ManagedSecretDataAction::Backup,
            &temp.path().join("backup"),
        )
        .is_err()
    );
    let plan = ManagedSecretDataPlan {
        instance,
        action: ManagedSecretDataAction::Restore,
        path: temp.path().join("backup"),
        state_digest: "e".repeat(64),
        source_digest: None,
        target: String::new(),
        config_hash: String::new(),
        consequences: Vec::new(),
        plan_hash: "d".repeat(64),
    };
    assert!(
        ManagedSecretBackendProvisioner::apply_data(&backend, &plan)
            .await
            .is_err()
    );
    Ok(())
}
