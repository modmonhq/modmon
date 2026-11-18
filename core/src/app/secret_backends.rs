use std::collections::BTreeMap;
use std::sync::Arc;

use crate::app::ports::{ManagedSecretBackendProvisioner, RemoteSecretBackendProvisioner};
use crate::domain::error::MmError;
use crate::domain::secret::SecretMaterial;
use crate::domain::secret_backend::{
    ManagedSecretBackendPlan, ManagedSecretBackendPlanRequest, ManagedSecretBackendResult,
    ManagedSecretBackendStatus, ManagedSecretDataAction, ManagedSecretDataPlan,
    ManagedSecretDataResult, ProviderDescriptor, ProviderInstanceConfig,
    ProviderInstanceDescriptor, ProviderInstanceName, ProviderMode, ProviderOperation,
    RemoteSecretBackendSetupPlan, RemoteSecretBackendSetupRequest, RemoteSecretBackendStatus,
    RemoteSecretBackendStatusRequest,
};

pub struct ProviderDriverRegistration {
    pub descriptor: ProviderDescriptor,
    pub remote: Option<Arc<dyn RemoteSecretBackendProvisioner>>,
    pub managed: Option<Arc<dyn ManagedSecretBackendProvisioner>>,
}

impl std::fmt::Debug for ProviderDriverRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderDriverRegistration")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct SecretBackendManager {
    drivers:
        BTreeMap<crate::domain::secret_backend::ProviderDriverId, Arc<ProviderDriverRegistration>>,
    instances: BTreeMap<ProviderInstanceName, ProviderInstanceConfig>,
}

impl std::fmt::Debug for SecretBackendManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretBackendManager")
            .field("drivers", &self.drivers.keys().collect::<Vec<_>>())
            .field("instances", &self.instances.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl SecretBackendManager {
    pub fn new(
        registrations: Vec<ProviderDriverRegistration>,
        instances: BTreeMap<ProviderInstanceName, ProviderInstanceConfig>,
    ) -> Result<Self, MmError> {
        let mut drivers = BTreeMap::new();
        for registration in registrations {
            validate_driver_registration(&registration)?;
            let id = registration.descriptor.driver.clone();
            if drivers.insert(id.clone(), Arc::new(registration)).is_some() {
                return Err(MmError::Security {
                    message: format!("provider driver '{id}' is registered more than once"),
                });
            }
        }
        let manager = Self { drivers, instances };
        for (name, instance) in &manager.instances {
            manager.registration(name, instance.configuration.mode())?;
        }
        Ok(manager)
    }

    #[must_use]
    pub fn drivers(&self) -> Vec<ProviderDescriptor> {
        self.drivers
            .values()
            .map(|driver| driver.descriptor.clone())
            .collect()
    }

    pub fn instances(&self) -> Result<Vec<ProviderInstanceDescriptor>, MmError> {
        self.instances
            .iter()
            .map(|(name, instance)| self.describe(name, instance))
            .collect()
    }

    pub fn instance(
        &self,
        name: &ProviderInstanceName,
    ) -> Result<ProviderInstanceDescriptor, MmError> {
        let instance = self.config(name)?;
        self.describe(name, instance)
    }

    pub fn require_operation(
        &self,
        name: &ProviderInstanceName,
        operation: ProviderOperation,
    ) -> Result<(), MmError> {
        let mode = self.config(name)?.configuration.mode();
        self.require(name, mode, operation).map(|_| ())
    }

    pub fn plan_remote_setup(
        &self,
        request: RemoteSecretBackendSetupRequest,
    ) -> Result<RemoteSecretBackendSetupPlan, MmError> {
        validate_external_setup_operation(request.operation)?;
        self.require(&request.instance, ProviderMode::External, request.operation)?;
        plan_remote_secret_backend_setup(request)
    }

    pub async fn apply_remote_setup(
        &self,
        request: RemoteSecretBackendSetupRequest,
        reviewed_plan_hash: &str,
        bootstrap_credential: SecretMaterial,
    ) -> Result<RemoteSecretBackendStatus, MmError> {
        validate_external_setup_operation(request.operation)?;
        let registration =
            self.require(&request.instance, ProviderMode::External, request.operation)?;
        let plan = self.plan_remote_setup(request.clone())?;
        if plan.plan_hash != reviewed_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "provider setup plan changed (expected {reviewed_plan_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        registration
            .remote
            .as_ref()
            .ok_or_else(|| inconsistent_driver(&registration.descriptor, ProviderMode::External))?
            .setup(request, bootstrap_credential)
            .await
    }

    pub async fn remote_status(
        &self,
        request: &RemoteSecretBackendStatusRequest,
    ) -> Result<RemoteSecretBackendStatus, MmError> {
        let registration = self.require(
            &request.instance,
            ProviderMode::External,
            ProviderOperation::Test,
        )?;
        registration
            .remote
            .as_ref()
            .ok_or_else(|| inconsistent_driver(&registration.descriptor, ProviderMode::External))?
            .status(request)
            .await
    }

    pub fn plan_managed(
        &self,
        request: ManagedSecretBackendPlanRequest,
    ) -> Result<ManagedSecretBackendPlan, MmError> {
        let operation = lifecycle_operation(request.action);
        let registration = self.require(&request.instance, ProviderMode::Managed, operation)?;
        let mut plan = registration
            .managed
            .as_ref()
            .ok_or_else(|| inconsistent_driver(&registration.descriptor, ProviderMode::Managed))?
            .plan(request.clone())?;
        plan.plan_hash = bound_hash(&(request, &plan), "provider lifecycle plan")?;
        Ok(plan)
    }

    pub async fn apply_managed(
        &self,
        request: ManagedSecretBackendPlanRequest,
        reviewed_plan_hash: &str,
    ) -> Result<ManagedSecretBackendResult, MmError> {
        let operation = lifecycle_operation(request.action);
        let registration = self.require(&request.instance, ProviderMode::Managed, operation)?;
        let managed = registration
            .managed
            .as_ref()
            .ok_or_else(|| inconsistent_driver(&registration.descriptor, ProviderMode::Managed))?;
        let driver_plan = managed.plan(request.clone())?;
        let current = bound_hash(&(request.clone(), &driver_plan), "provider lifecycle plan")?;
        if current != reviewed_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "provider lifecycle plan changed (expected {reviewed_plan_hash}, current {current}); review the new plan"
                ),
            });
        }
        let mut result = managed.apply(request, &driver_plan.plan_hash).await?;
        result.plan_hash = reviewed_plan_hash.to_owned();
        Ok(result)
    }

    pub async fn managed_status(
        &self,
        instance: &ProviderInstanceName,
    ) -> Result<ManagedSecretBackendStatus, MmError> {
        let registration =
            self.require(instance, ProviderMode::Managed, ProviderOperation::Status)?;
        registration
            .managed
            .as_ref()
            .ok_or_else(|| inconsistent_driver(&registration.descriptor, ProviderMode::Managed))?
            .status(instance)
            .await
    }

    pub fn plan_managed_data(
        &self,
        instance: &ProviderInstanceName,
        action: ManagedSecretDataAction,
        path: &std::path::Path,
        target: &str,
        config_hash: &str,
    ) -> Result<ManagedSecretDataPlan, MmError> {
        let operation = match action {
            ManagedSecretDataAction::Backup => ProviderOperation::Backup,
            ManagedSecretDataAction::Restore => ProviderOperation::Restore,
        };
        let registration = self.require(instance, ProviderMode::Managed, operation)?;
        let mut plan = registration
            .managed
            .as_ref()
            .ok_or_else(|| inconsistent_driver(&registration.descriptor, ProviderMode::Managed))?
            .plan_data(instance, action, path)?;
        plan.target = target.to_owned();
        plan.config_hash = config_hash.to_owned();
        plan.plan_hash = bound_hash(&plan, "provider data plan")?;
        Ok(plan)
    }

    pub async fn apply_managed_data(
        &self,
        plan: &ManagedSecretDataPlan,
    ) -> Result<ManagedSecretDataResult, MmError> {
        let operation = match plan.action {
            ManagedSecretDataAction::Backup => ProviderOperation::Backup,
            ManagedSecretDataAction::Restore => ProviderOperation::Restore,
        };
        let registration = self.require(&plan.instance, ProviderMode::Managed, operation)?;
        let managed = registration
            .managed
            .as_ref()
            .ok_or_else(|| inconsistent_driver(&registration.descriptor, ProviderMode::Managed))?;
        let mut driver_plan = managed.plan_data(&plan.instance, plan.action, &plan.path)?;
        driver_plan.target = plan.target.clone();
        driver_plan.config_hash = plan.config_hash.clone();
        let current = bound_hash(&driver_plan, "provider data plan")?;
        if current != plan.plan_hash {
            return Err(MmError::State {
                message: format!(
                    "provider data plan changed (expected {}, current {current}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        driver_plan.plan_hash = managed
            .plan_data(&plan.instance, plan.action, &plan.path)?
            .plan_hash;
        let mut result = managed.apply_data(&driver_plan).await?;
        result.instance = plan.instance.clone();
        Ok(result)
    }

    fn config(&self, name: &ProviderInstanceName) -> Result<&ProviderInstanceConfig, MmError> {
        self.instances.get(name).ok_or_else(|| MmError::Secret {
            message: format!("provider instance '{name}' is not configured"),
        })
    }

    fn registration(
        &self,
        name: &ProviderInstanceName,
        mode: ProviderMode,
    ) -> Result<&Arc<ProviderDriverRegistration>, MmError> {
        let instance = self.config(name)?;
        if instance.configuration.mode() != mode {
            return Err(MmError::Secret {
                message: format!(
                    "provider instance '{name}' uses {} mode, not {}",
                    instance.configuration.mode().as_str(),
                    mode.as_str()
                ),
            });
        }
        let registration = self
            .drivers
            .get(&instance.driver)
            .ok_or_else(|| MmError::Secret {
                message: format!(
                    "provider instance '{name}' uses unregistered driver '{}'",
                    instance.driver
                ),
            })?;
        let mode_descriptor =
            registration
                .descriptor
                .mode(mode)
                .ok_or_else(|| MmError::Secret {
                    message: format!(
                        "provider driver '{}' does not support {} mode",
                        instance.driver,
                        mode.as_str()
                    ),
                })?;
        if !mode_descriptor.scopes.contains(&instance.scope) {
            return Err(MmError::Secret {
                message: format!(
                    "provider instance '{name}' uses unsupported {:?} scope for driver '{}' in {} mode; supported scopes are {:?}",
                    instance.scope,
                    instance.driver,
                    mode.as_str(),
                    mode_descriptor.scopes
                ),
            });
        }
        validate_instance_configuration(name, instance, mode_descriptor)?;
        Ok(registration)
    }

    fn require(
        &self,
        name: &ProviderInstanceName,
        mode: ProviderMode,
        operation: ProviderOperation,
    ) -> Result<&Arc<ProviderDriverRegistration>, MmError> {
        let registration = self.registration(name, mode)?;
        registration.descriptor.require(mode, operation)?;
        Ok(registration)
    }

    fn describe(
        &self,
        name: &ProviderInstanceName,
        instance: &ProviderInstanceConfig,
    ) -> Result<ProviderInstanceDescriptor, MmError> {
        let registration = self.registration(name, instance.configuration.mode())?;
        let mode = registration
            .descriptor
            .mode(instance.configuration.mode())
            .ok_or_else(|| {
                inconsistent_driver(&registration.descriptor, instance.configuration.mode())
            })?;
        Ok(ProviderInstanceDescriptor {
            name: name.clone(),
            driver: instance.driver.clone(),
            driver_display_name: registration.descriptor.display_name.clone(),
            mode: mode.mode,
            scope: instance.scope,
            operations: mode.operations.clone(),
            configuration: serde_json::to_value(&instance.configuration).map_err(|error| {
                MmError::State {
                    message: format!("could not render provider configuration: {error}"),
                }
            })?,
        })
    }
}

pub(crate) fn plan_remote_secret_backend_setup(
    request: RemoteSecretBackendSetupRequest,
) -> Result<RemoteSecretBackendSetupPlan, MmError> {
    if request.locations.is_empty() {
        return Err(MmError::Security {
            message: "provider setup requires declared logical locations".to_owned(),
        });
    }
    let consequences = vec![
        "Validate the exact HTTPS trust bundle and supported server release.".to_owned(),
        "Create or update an environment-scoped automation role and exact-location policy."
            .to_owned(),
        "Encrypt the resulting non-interactive credential outside project configuration."
            .to_owned(),
    ];
    let canonical =
        serde_json::to_vec(&(&request, &consequences)).map_err(|error| MmError::State {
            message: format!("could not canonicalize provider setup plan: {error}"),
        })?;
    Ok(RemoteSecretBackendSetupPlan {
        request,
        consequences,
        plan_hash: blake3::hash(&canonical).to_hex().to_string(),
    })
}

fn lifecycle_operation(
    action: crate::domain::secret_backend::ManagedSecretBackendAction,
) -> ProviderOperation {
    use crate::domain::secret_backend::ManagedSecretBackendAction as Action;
    match action {
        Action::Install => ProviderOperation::Install,
        Action::Restart => ProviderOperation::Restart,
        Action::Upgrade => ProviderOperation::Upgrade,
        Action::Seal => ProviderOperation::Seal,
        Action::Unseal => ProviderOperation::Unseal,
        Action::Remove => ProviderOperation::Remove,
    }
}

fn validate_external_setup_operation(operation: ProviderOperation) -> Result<(), MmError> {
    if matches!(operation, ProviderOperation::Add | ProviderOperation::Edit) {
        return Ok(());
    }
    Err(MmError::Security {
        message: format!(
            "external provider setup accepts only add or edit, not {}",
            operation.as_str()
        ),
    })
}

fn bound_hash(value: &impl serde::Serialize, context: &str) -> Result<String, MmError> {
    let canonical = serde_json::to_vec(value).map_err(|error| MmError::State {
        message: format!("could not canonicalize {context}: {error}"),
    })?;
    Ok(blake3::hash(&canonical).to_hex().to_string())
}

fn validate_driver_registration(registration: &ProviderDriverRegistration) -> Result<(), MmError> {
    if registration.descriptor.display_name.is_empty() || registration.descriptor.modes.is_empty() {
        return Err(MmError::Security {
            message: format!(
                "provider driver '{}' has incomplete display or mode metadata",
                registration.descriptor.driver
            ),
        });
    }
    for mode in &registration.descriptor.modes {
        let implementation = match mode.mode {
            ProviderMode::External => registration.remote.is_some(),
            ProviderMode::Managed => registration.managed.is_some(),
        };
        if !implementation {
            return Err(inconsistent_driver(&registration.descriptor, mode.mode));
        }
        if mode.configuration.iter().any(|field| field.secret) {
            return Err(MmError::Security {
                message: format!(
                    "provider driver '{}' descriptor may not contain secret configuration fields",
                    registration.descriptor.driver
                ),
            });
        }
    }
    Ok(())
}

fn validate_instance_configuration(
    name: &ProviderInstanceName,
    instance: &ProviderInstanceConfig,
    descriptor: &crate::domain::secret_backend::ProviderModeDescriptor,
) -> Result<(), MmError> {
    let values = serde_json::to_value(&instance.configuration).map_err(|error| MmError::State {
        message: format!("could not validate provider instance '{name}': {error}"),
    })?;
    for field in &descriptor.configuration {
        let value = values.get(&field.key);
        if field.required && value.is_none_or(serde_json::Value::is_null) {
            return Err(MmError::Secret {
                message: format!(
                    "provider instance '{name}' requires configuration field '{}'",
                    field.key
                ),
            });
        }
        if !field.allowed_values.is_empty()
            && let Some(value) = value.and_then(serde_json::Value::as_str)
            && !field.allowed_values.iter().any(|allowed| allowed == value)
        {
            return Err(MmError::Secret {
                message: format!(
                    "provider instance '{name}' field '{}' value is unsupported; allowed values are {:?}",
                    field.key, field.allowed_values
                ),
            });
        }
    }
    Ok(())
}

fn inconsistent_driver(descriptor: &ProviderDescriptor, mode: ProviderMode) -> MmError {
    MmError::Security {
        message: format!(
            "provider driver '{}' has a descriptor/implementation mismatch for {} mode",
            descriptor.driver,
            mode.as_str()
        ),
    }
}

#[cfg(test)]
mod tests;
