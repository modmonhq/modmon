use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::app::ports::{
    SecretBatchReplacer, SecretCandidateLifecycle, SecretCreator, SecretMaterialResolver,
    SecretMetadataInspector, SecretRemover, SecretReplacer, SecretStorageCapabilityCatalog,
};
use crate::domain::error::MmError;
use crate::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretReplacement,
    SecretResolution, SecretScope, SecretWriteResult,
};
use crate::domain::secret_contract::{
    StorageCapabilityDescriptor, StorageOperation, StorageProviderId,
};

pub struct StorageProviderRegistration {
    pub descriptor: StorageCapabilityDescriptor,
    pub inspector: Arc<dyn SecretMetadataInspector>,
    pub resolver: Arc<dyn SecretMaterialResolver>,
    pub creator: Option<Arc<dyn SecretCreator>>,
    pub replacer: Option<Arc<dyn SecretReplacer>>,
    pub batch_replacer: Option<Arc<dyn SecretBatchReplacer>>,
    pub candidates: Option<Arc<dyn SecretCandidateLifecycle>>,
    pub remover: Option<Arc<dyn SecretRemover>>,
}

impl std::fmt::Debug for StorageProviderRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageProviderRegistration")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct SecretProviderRegistry {
    providers: BTreeMap<StorageProviderId, StorageProviderRegistration>,
}

impl SecretProviderRegistry {
    pub fn new(registrations: Vec<StorageProviderRegistration>) -> Result<Self, MmError> {
        let mut providers = BTreeMap::new();
        for registration in registrations {
            validate_registration(&registration)?;
            let id = registration.descriptor.id.clone();
            if providers.insert(id.clone(), registration).is_some() {
                return Err(MmError::Secret {
                    message: format!("secret storage provider '{id}' is registered more than once"),
                });
            }
        }
        Ok(Self { providers })
    }

    fn provider(
        &self,
        reference: &SecretReference,
    ) -> Result<&StorageProviderRegistration, MmError> {
        let provider = self
            .providers
            .get(reference.provider())
            .ok_or_else(|| unsupported_provider(reference.provider()))?;
        if reference.locator().len() > provider.descriptor.max_location_bytes {
            return Err(MmError::Secret {
                message: format!(
                    "secret storage provider '{}' locations may not exceed {} bytes",
                    provider.descriptor.id, provider.descriptor.max_location_bytes
                ),
            });
        }
        Ok(provider)
    }
}

impl SecretStorageCapabilityCatalog for SecretProviderRegistry {
    fn storage_capabilities(&self) -> Vec<StorageCapabilityDescriptor> {
        self.providers
            .values()
            .map(|registration| registration.descriptor.clone())
            .collect()
    }

    fn storage_capability(
        &self,
        provider: &StorageProviderId,
    ) -> Result<StorageCapabilityDescriptor, MmError> {
        self.providers
            .get(provider)
            .map(|registration| registration.descriptor.clone())
            .ok_or_else(|| unsupported_provider(provider))
    }
}

#[async_trait]
impl SecretMetadataInspector for SecretProviderRegistry {
    async fn inspect(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        self.provider(reference)?
            .inspector
            .inspect(scope, reference)
            .await
    }

    /// Each provider is asked once, for all of its references.
    async fn inspect_many(
        &self,
        scope: &SecretScope,
        references: &[SecretReference],
    ) -> Result<Vec<SecretMetadata>, MmError> {
        let mut by_provider: BTreeMap<&StorageProviderId, Vec<usize>> = BTreeMap::new();
        for (index, reference) in references.iter().enumerate() {
            self.provider(reference)?;
            by_provider
                .entry(reference.provider())
                .or_default()
                .push(index);
        }
        let mut found = vec![None; references.len()];
        for (provider, indices) in by_provider {
            let asked = indices
                .iter()
                .map(|index| references[*index].clone())
                .collect::<Vec<_>>();
            let answers = self.providers[provider]
                .inspector
                .inspect_many(scope, &asked)
                .await?;
            for (index, answer) in indices.into_iter().zip(answers) {
                found[index] = Some(answer);
            }
        }
        found
            .into_iter()
            .map(|answer| {
                answer.ok_or_else(|| MmError::Secret {
                    message: "a secret store left a secret unanswered".to_owned(),
                })
            })
            .collect()
    }

    async fn invalid_reason(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Option<String> {
        let Ok(provider) = self.provider(reference) else {
            return None;
        };
        provider.inspector.invalid_reason(scope, reference).await
    }
}

#[async_trait]
impl SecretMaterialResolver for SecretProviderRegistry {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        self.provider(reference)?
            .resolver
            .resolve(scope, reference)
            .await
    }
}

#[async_trait]
impl SecretCreator for SecretProviderRegistry {
    async fn create(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        now_unix_ms: i64,
    ) -> Result<SecretWriteResult, MmError> {
        let provider = self.provider(reference)?;
        validate_material(provider, &material)?;
        operation(
            provider,
            StorageOperation::Create,
            provider.creator.as_ref(),
        )?
        .create(scope, reference, material, now_unix_ms)
        .await
    }
}

#[async_trait]
impl SecretReplacer for SecretProviderRegistry {
    async fn replace(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError> {
        let provider = self.provider(reference)?;
        validate_material(provider, &material)?;
        operation(
            provider,
            StorageOperation::Replace,
            provider.replacer.as_ref(),
        )?
        .replace(scope, reference, material, expected, now_unix_ms)
        .await
    }
}

#[async_trait]
impl SecretBatchReplacer for SecretProviderRegistry {
    async fn replace_batch(
        &self,
        replacements: Vec<SecretReplacement>,
        now_unix_ms: i64,
    ) -> Result<Vec<String>, MmError> {
        let Some(first) = replacements.first() else {
            return Ok(Vec::new());
        };
        if replacements
            .iter()
            .any(|replacement| replacement.reference.provider() != first.reference.provider())
        {
            return Err(MmError::Secret {
                message: "atomic secret replacement batches may not span storage providers"
                    .to_owned(),
            });
        }
        let provider = self.provider(&first.reference)?;
        for replacement in &replacements {
            validate_material(
                self.provider(&replacement.reference)?,
                &replacement.material,
            )?;
        }
        operation(
            provider,
            StorageOperation::ReplaceBatch,
            provider.batch_replacer.as_ref(),
        )?
        .replace_batch(replacements, now_unix_ms)
        .await
    }
}

#[async_trait]
impl SecretCandidateLifecycle for SecretProviderRegistry {
    async fn stage(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError> {
        let provider = self.provider(reference)?;
        validate_material(provider, &material)?;
        operation(
            provider,
            StorageOperation::StageCandidate,
            provider.candidates.as_ref(),
        )?
        .stage(scope, reference, material, expected, now_unix_ms)
        .await
    }

    async fn promote(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError> {
        let provider = self.provider(reference)?;
        operation(
            provider,
            StorageOperation::PromoteCandidate,
            provider.candidates.as_ref(),
        )?
        .promote(scope, reference, expected, now_unix_ms)
        .await
    }
}

#[async_trait]
impl SecretRemover for SecretProviderRegistry {
    async fn unset(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        let provider = self.provider(reference)?;
        operation(
            provider,
            StorageOperation::Delete,
            provider.remover.as_ref(),
        )?
        .unset(scope, reference, expected)
        .await
    }
}

fn operation<'a, T: ?Sized>(
    registration: &'a StorageProviderRegistration,
    required: StorageOperation,
    implementation: Option<&'a Arc<T>>,
) -> Result<&'a T, MmError> {
    if !registration.descriptor.supports(required) {
        return Err(unsupported_operation(&registration.descriptor, required));
    }
    implementation
        .map(Arc::as_ref)
        .ok_or_else(|| inconsistent_registration(&registration.descriptor, required))
}

fn validate_material(
    registration: &StorageProviderRegistration,
    material: &SecretMaterial,
) -> Result<(), MmError> {
    let within_limit =
        material.expose(|value| value.len() <= registration.descriptor.max_material_bytes);
    if within_limit {
        Ok(())
    } else {
        Err(MmError::Secret {
            message: format!(
                "secret storage provider '{}' material may not exceed {} bytes",
                registration.descriptor.id, registration.descriptor.max_material_bytes
            ),
        })
    }
}

fn validate_registration(registration: &StorageProviderRegistration) -> Result<(), MmError> {
    if registration.descriptor.max_material_bytes == 0
        || registration.descriptor.max_location_bytes == 0
        || registration.descriptor.material_kinds.is_empty()
    {
        return Err(MmError::Security {
            message: format!(
                "secret storage provider '{}' has empty material or location limits",
                registration.descriptor.id
            ),
        });
    }
    let checks = [
        (StorageOperation::Create, registration.creator.is_some()),
        (StorageOperation::Replace, registration.replacer.is_some()),
        (
            StorageOperation::ReplaceBatch,
            registration.batch_replacer.is_some(),
        ),
        (
            StorageOperation::StageCandidate,
            registration.candidates.is_some(),
        ),
        (
            StorageOperation::PromoteCandidate,
            registration.candidates.is_some(),
        ),
        (StorageOperation::Delete, registration.remover.is_some()),
    ];
    for (operation, present) in checks {
        if registration.descriptor.supports(operation) != present {
            return Err(inconsistent_registration(
                &registration.descriptor,
                operation,
            ));
        }
    }
    if !registration
        .descriptor
        .supports(StorageOperation::InspectMetadata)
        || !registration
            .descriptor
            .supports(StorageOperation::ResolveMaterial)
    {
        return Err(MmError::Secret {
            message: format!(
                "storage provider '{}' must truthfully advertise metadata inspection and material resolution",
                registration.descriptor.id
            ),
        });
    }
    let rotation_is_consistent = match registration.descriptor.rotation {
        crate::domain::secret_contract::RotationCapability::None => true,
        crate::domain::secret_contract::RotationCapability::Replace => {
            registration.descriptor.supports(StorageOperation::Replace)
        }
        crate::domain::secret_contract::RotationCapability::Staged => {
            registration.descriptor.supports(StorageOperation::Replace)
                && registration
                    .descriptor
                    .supports(StorageOperation::StageCandidate)
                && registration
                    .descriptor
                    .supports(StorageOperation::PromoteCandidate)
        }
    };
    if !rotation_is_consistent {
        return Err(MmError::Security {
            message: format!(
                "secret storage provider '{}' has inconsistent rotation capabilities",
                registration.descriptor.id
            ),
        });
    }
    Ok(())
}

fn unsupported_provider(provider: &StorageProviderId) -> MmError {
    MmError::Secret {
        message: format!("secret storage provider '{provider}' is not registered"),
    }
}

fn unsupported_operation(
    descriptor: &StorageCapabilityDescriptor,
    operation: StorageOperation,
) -> MmError {
    MmError::Secret {
        message: format!(
            "secret storage provider '{}' does not support {operation:?}; supported operations are {:?}",
            descriptor.id, descriptor.operations
        ),
    }
}

fn inconsistent_registration(
    descriptor: &StorageCapabilityDescriptor,
    operation: StorageOperation,
) -> MmError {
    MmError::Security {
        message: format!(
            "secret storage provider '{}' has an implementation/capability mismatch for {operation:?}",
            descriptor.id
        ),
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
