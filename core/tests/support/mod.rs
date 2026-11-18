use std::sync::Arc;

use mm::adapters::secrets::{
    AgeSecretStore, EnvironmentSecretStore, SecretProviderRegistry, StorageProviderRegistration,
    age_storage_descriptor, environment_storage_descriptor,
};
use mm::app::ports::{
    SecretBatchReplacer, SecretCandidateLifecycle, SecretCreator, SecretMaterialResolver,
    SecretMetadataInspector, SecretRemover, SecretReplacer,
};
use mm::domain::error::MmError;

pub fn secret_registry(age: Arc<AgeSecretStore>) -> Result<Arc<SecretProviderRegistry>, MmError> {
    let environment = Arc::new(EnvironmentSecretStore::new([17_u8; 32]));
    let age_inspector: Arc<dyn SecretMetadataInspector> = age.clone();
    let age_resolver: Arc<dyn SecretMaterialResolver> = age.clone();
    let age_creator: Arc<dyn SecretCreator> = age.clone();
    let age_replacer: Arc<dyn SecretReplacer> = age.clone();
    let age_batch_replacer: Arc<dyn SecretBatchReplacer> = age.clone();
    let age_candidates: Arc<dyn SecretCandidateLifecycle> = age.clone();
    let age_remover: Arc<dyn SecretRemover> = age;
    let environment_inspector: Arc<dyn SecretMetadataInspector> = environment.clone();
    let environment_resolver: Arc<dyn SecretMaterialResolver> = environment;
    Ok(Arc::new(SecretProviderRegistry::new(vec![
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
    ])?))
}
