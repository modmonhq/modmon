use async_trait::async_trait;
use std::path::Path;

use crate::app::ports::{ManagedSecretBackendProvisioner, RemoteSecretBackendProvisioner};
use crate::domain::error::MmError;
use crate::domain::secret::SecretMaterial;
use crate::domain::secret_backend::{
    ManagedSecretBackendAction, ManagedSecretBackendPlan, ManagedSecretBackendPlanRequest,
    ManagedSecretBackendResult, ManagedSecretBackendStatus, ManagedSecretDataAction,
    ManagedSecretDataPlan, ManagedSecretDataResult, ProviderConfigurationField, ProviderDescriptor,
    ProviderDriverId, ProviderFieldKind, ProviderInstanceName, ProviderMode,
    ProviderModeDescriptor, ProviderOperation, ProviderScope, RemoteSecretBackendSetupRequest,
    RemoteSecretBackendStatus, RemoteSecretBackendStatusRequest, SecretBackendHealth,
};

use super::admin::{OpenBaoConnectionAdministrator, OpenBaoSetupRequest};
use super::connection_store::OpenBaoMode;
use super::managed::{
    ManagedOpenBaoAction, ManagedOpenBaoManager, ManagedOpenBaoPlanRequest, OPENBAO_2_3_IMAGE,
    OPENBAO_2_4_IMAGE,
};

#[derive(Clone)]
pub struct OpenBaoBackendUseCases {
    administrator: OpenBaoConnectionAdministrator,
    managed: Option<ManagedOpenBaoManager>,
}

impl std::fmt::Debug for OpenBaoBackendUseCases {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenBaoBackendUseCases")
            .finish_non_exhaustive()
    }
}

impl OpenBaoBackendUseCases {
    #[must_use]
    pub fn new(
        administrator: OpenBaoConnectionAdministrator,
        managed: Option<ManagedOpenBaoManager>,
    ) -> Self {
        Self {
            administrator,
            managed,
        }
    }

    fn managed(&self) -> Result<&ManagedOpenBaoManager, MmError> {
        self.managed.as_ref().ok_or_else(|| MmError::Target {
            message:
                "managed secret backend operations require the local target; run `mm dev setup` first"
                    .to_owned(),
        })
    }
}

#[async_trait]
impl RemoteSecretBackendProvisioner for OpenBaoBackendUseCases {
    async fn setup(
        &self,
        request: RemoteSecretBackendSetupRequest,
        bootstrap_credential: SecretMaterial,
    ) -> Result<RemoteSecretBackendStatus, MmError> {
        let instance = request.instance.clone();
        let ca_bytes =
            crate::domain::path_policy::read_regular_file(&request.trust_bundle, 1024 * 1024)?
                .ok_or_else(|| MmError::Security {
                    message: "remote provider trust bundle does not exist".to_owned(),
                })?;
        let ca_pem = String::from_utf8(ca_bytes).map_err(|_| MmError::Security {
            message: "remote provider trust bundle is not UTF-8 PEM".to_owned(),
        })?;
        let status = self
            .administrator
            .setup(OpenBaoSetupRequest {
                name: request.instance.to_string(),
                mode: OpenBaoMode::External,
                address: request.address,
                namespace: request.namespace,
                mount: request.storage_mount,
                auth_mount: request.automation_auth_mount,
                ca_pem,
                expected_ca_sha256: request.expected_trust_sha256,
                project_id: request.project_id,
                environment: request.environment,
                locations: request.locations,
                developer_token: bootstrap_credential.into_zeroizing(),
                allow_mount_bootstrap: request.allow_bootstrap,
                managed_installation: None,
            })
            .await?;
        remote_status(instance, status)
    }

    async fn status(
        &self,
        request: &RemoteSecretBackendStatusRequest,
    ) -> Result<RemoteSecretBackendStatus, MmError> {
        self.administrator
            .status(
                request.instance.as_str(),
                OpenBaoMode::External,
                request.project_id,
                &request.environment,
            )
            .await
            .and_then(|status| remote_status(request.instance.clone(), status))
    }
}

#[async_trait]
impl ManagedSecretBackendProvisioner for OpenBaoBackendUseCases {
    fn plan(
        &self,
        request: ManagedSecretBackendPlanRequest,
    ) -> Result<ManagedSecretBackendPlan, MmError> {
        let plan = self.managed()?.plan(managed_request(&request)?)?;
        Ok(ManagedSecretBackendPlan {
            instance: request.instance,
            action: request.action,
            installation: plan.installation,
            immutable_artifact: plan.image,
            previous_artifact: plan.previous_image,
            ingress_url: format!("https://{}", plan.ingress_host),
            management_access: plan.management_access,
            data_resource: plan.volume,
            purge_data: plan.purge_data,
            plan_hash: plan.plan_hash,
        })
    }

    async fn apply(
        &self,
        request: ManagedSecretBackendPlanRequest,
        reviewed_plan_hash: &str,
    ) -> Result<ManagedSecretBackendResult, MmError> {
        let instance = request.instance.clone();
        let result = self
            .managed()?
            .execute_reviewed(managed_request(&request)?, reviewed_plan_hash)
            .await?;
        Ok(ManagedSecretBackendResult {
            instance,
            action: request.action,
            plan_hash: result.plan_hash,
            rolled_back: result.rolled_back,
            data_purged: result.data_purged,
        })
    }

    async fn status(
        &self,
        instance: &ProviderInstanceName,
    ) -> Result<ManagedSecretBackendStatus, MmError> {
        let status = self.managed()?.status().await?;
        Ok(ManagedSecretBackendStatus {
            instance: instance.clone(),
            driver: driver_id()?,
            driver_display_name: "OpenBao".to_owned(),
            mode: ProviderMode::Managed,
            scope: ProviderScope::Installation,
            capabilities: managed_operations(),
            installed: status.installed,
            installation: status.installation,
            connection: status.connection,
            immutable_artifact: status.image,
            data_resource: status.volume,
            ingress_url: status.ingress_url,
            management_url: status.management_url,
            management_access: status.management_access,
            health: status.health.map(health),
            recovery_available_off_instance: status.recovery_available_off_instance,
            journal: status.journal,
            diagnostics: status.diagnostics,
        })
    }

    fn plan_data(
        &self,
        instance: &ProviderInstanceName,
        action: ManagedSecretDataAction,
        path: &Path,
    ) -> Result<ManagedSecretDataPlan, MmError> {
        let state_digest = self.managed()?.reviewed_state_digest()?;
        let source_digest = self.managed()?.reviewed_path_digest(path)?;
        if action == ManagedSecretDataAction::Restore && source_digest.is_none() {
            return Err(MmError::State {
                message: "managed provider backup does not exist".to_owned(),
            });
        }
        let consequences = match action {
            ManagedSecretDataAction::Backup => vec![
                "Write an encrypted, integrity-checked snapshot envelope to the selected path."
                    .to_owned(),
                "Replace an existing same-path backup only after this exact review.".to_owned(),
            ],
            ManagedSecretDataAction::Restore => vec![
                "Replace the installed Raft state from the authenticated backup envelope."
                    .to_owned(),
                "The server will seal during restore and use off-instance recovery to unseal."
                    .to_owned(),
            ],
        };
        let canonical =
            serde_json::to_vec(&(action, path, &state_digest, &source_digest, &consequences))
                .map_err(|error| MmError::State {
                    message: format!("could not canonicalize managed data plan: {error}"),
                })?;
        Ok(ManagedSecretDataPlan {
            instance: instance.clone(),
            action,
            path: path.to_path_buf(),
            state_digest,
            source_digest,
            target: String::new(),
            config_hash: String::new(),
            consequences,
            plan_hash: blake3::hash(&canonical).to_hex().to_string(),
        })
    }

    async fn apply_data(
        &self,
        plan: &ManagedSecretDataPlan,
    ) -> Result<ManagedSecretDataResult, MmError> {
        let current = self.plan_data(&plan.instance, plan.action, &plan.path)?;
        if current.plan_hash != plan.plan_hash {
            return Err(MmError::State {
                message: format!(
                    "managed data plan changed (expected {}, current {}); review the new plan",
                    plan.plan_hash, current.plan_hash
                ),
            });
        }
        let snapshot_digest = match plan.action {
            ManagedSecretDataAction::Backup => self.managed()?.backup(&plan.path).await?,
            ManagedSecretDataAction::Restore => self.managed()?.restore(&plan.path).await?,
        };
        Ok(ManagedSecretDataResult {
            instance: plan.instance.clone(),
            action: plan.action,
            path: plan.path.clone(),
            snapshot_digest,
            encrypted: true,
            integrity_checked: true,
        })
    }
}

fn managed_request(
    request: &ManagedSecretBackendPlanRequest,
) -> Result<ManagedOpenBaoPlanRequest, MmError> {
    Ok(ManagedOpenBaoPlanRequest {
        action: match request.action {
            ManagedSecretBackendAction::Install => ManagedOpenBaoAction::Install,
            ManagedSecretBackendAction::Restart => ManagedOpenBaoAction::Restart,
            ManagedSecretBackendAction::Upgrade => ManagedOpenBaoAction::Upgrade,
            ManagedSecretBackendAction::Seal => ManagedOpenBaoAction::Seal,
            ManagedSecretBackendAction::Unseal => ManagedOpenBaoAction::Unseal,
            ManagedSecretBackendAction::Remove => ManagedOpenBaoAction::Remove,
        },
        installation: request.installation.clone(),
        connection: request.instance.to_string(),
        project_id: request.project_id,
        environment: request.environment.clone(),
        locations: request.locations.clone(),
        image: match request.release.as_str() {
            "2.3" => OPENBAO_2_3_IMAGE,
            "2.4" => OPENBAO_2_4_IMAGE,
            other => {
                return Err(MmError::Security {
                    message: format!("managed secret backend release '{other}' is unsupported"),
                });
            }
        }
        .to_owned(),
        host_port: request.management_port,
        purge_data: request.purge_data,
        target: request.target.clone(),
    })
}

fn remote_status(
    instance: ProviderInstanceName,
    status: super::admin::OpenBaoConnectionStatus,
) -> Result<RemoteSecretBackendStatus, MmError> {
    let diagnostics = if status.health.healthy && status.authenticated {
        Vec::new()
    } else {
        vec!["backend health or environment-scoped authentication requires attention".to_owned()]
    };
    Ok(RemoteSecretBackendStatus {
        instance,
        driver: driver_id()?,
        driver_display_name: "OpenBao".to_owned(),
        mode: ProviderMode::External,
        scope: ProviderScope::Project,
        capabilities: external_operations(),
        environment: status.environment,
        health: health(status.health),
        authenticated: status.authenticated,
        isolated_scope: status.isolated_path,
        diagnostics,
    })
}

fn health(value: super::provider::OpenBaoHealth) -> SecretBackendHealth {
    SecretBackendHealth {
        initialized: value.initialized,
        sealed: value.sealed,
        standby: value.standby,
        healthy: value.healthy,
        version: value.version,
    }
}

fn driver_id() -> Result<ProviderDriverId, MmError> {
    ProviderDriverId::new("openbao").map_err(|message| MmError::Security { message })
}

fn external_operations() -> Vec<ProviderOperation> {
    vec![
        ProviderOperation::Add,
        ProviderOperation::Edit,
        ProviderOperation::Test,
        ProviderOperation::Status,
    ]
}

fn managed_operations() -> Vec<ProviderOperation> {
    vec![
        ProviderOperation::Add,
        ProviderOperation::Status,
        ProviderOperation::Install,
        ProviderOperation::Restart,
        ProviderOperation::Upgrade,
        ProviderOperation::Seal,
        ProviderOperation::Unseal,
        ProviderOperation::Backup,
        ProviderOperation::Restore,
        ProviderOperation::Remove,
    ]
}

pub fn openbao_provider_descriptor() -> Result<ProviderDescriptor, MmError> {
    Ok(ProviderDescriptor {
        driver: driver_id()?,
        display_name: "OpenBao".to_owned(),
        modes: vec![
            ProviderModeDescriptor {
                mode: ProviderMode::External,
                scopes: vec![ProviderScope::Project],
                operations: external_operations(),
                configuration: external_fields(),
            },
            ProviderModeDescriptor {
                mode: ProviderMode::Managed,
                scopes: vec![ProviderScope::Installation, ProviderScope::Environment],
                operations: managed_operations(),
                configuration: managed_fields(["2.3", "2.4", "2.5"], "2.4", "18200"),
            },
        ],
    })
}

fn external_fields() -> Vec<ProviderConfigurationField> {
    [
        (
            "address",
            "HTTPS address",
            ProviderFieldKind::HttpsUrl,
            None,
        ),
        (
            "namespace",
            "Namespace (optional)",
            ProviderFieldKind::Text,
            None,
        ),
        (
            "storage-mount",
            "KV v2 mount",
            ProviderFieldKind::Text,
            Some("mm-kv"),
        ),
        (
            "automation-auth-mount",
            "AppRole mount",
            ProviderFieldKind::Text,
            Some("mm-approle"),
        ),
        ("trust-bundle", "CA PEM path", ProviderFieldKind::Path, None),
        (
            "expected-trust-sha256",
            "CA sha256 pin",
            ProviderFieldKind::Sha256,
            None,
        ),
    ]
    .into_iter()
    .map(|(key, label, kind, default)| ProviderConfigurationField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind,
        required: key != "namespace",
        secret: false,
        default: default.map(str::to_owned),
        allowed_values: Vec::new(),
    })
    .collect()
}

fn managed_fields<const N: usize>(
    releases: [&str; N],
    default: &str,
    port: &str,
) -> Vec<ProviderConfigurationField> {
    vec![
        ProviderConfigurationField {
            key: "installation".to_owned(),
            label: "Installation".to_owned(),
            kind: ProviderFieldKind::Text,
            required: false,
            secret: false,
            default: None,
            allowed_values: Vec::new(),
        },
        ProviderConfigurationField {
            key: "release".to_owned(),
            label: "Server release".to_owned(),
            kind: ProviderFieldKind::Release,
            required: true,
            secret: false,
            default: Some(default.to_owned()),
            allowed_values: releases.into_iter().map(str::to_owned).collect(),
        },
        ProviderConfigurationField {
            key: "management-port".to_owned(),
            label: "Loopback management port".to_owned(),
            kind: ProviderFieldKind::U16,
            required: false,
            secret: false,
            default: Some(port.to_owned()),
            allowed_values: Vec::new(),
        },
    ]
}

#[cfg(test)]
#[path = "usecase_tests.rs"]
mod tests;
