use super::*;
use crate::adapters::secrets::{EnvironmentSecretStore, environment_storage_descriptor};

fn registration(descriptor: StorageCapabilityDescriptor) -> StorageProviderRegistration {
    let store = Arc::new(EnvironmentSecretStore::new([7; 32]));
    StorageProviderRegistration {
        descriptor,
        inspector: store.clone(),
        resolver: store,
        creator: None,
        replacer: None,
        batch_replacer: None,
        candidates: None,
        remover: None,
    }
}

fn material(value: &str) -> Result<SecretMaterial, MmError> {
    SecretMaterial::new(value.to_owned()).map_err(|message| MmError::Secret { message })
}

fn expected() -> SecretExpectedState {
    SecretExpectedState {
        active_revision: None,
        candidate_revision: None,
    }
}

#[tokio::test]
async fn registry_routes_reads_and_rejects_unadvertised_mutations_and_bounds()
-> Result<(), Box<dyn std::error::Error>> {
    let descriptor = environment_storage_descriptor()?;
    let provider = descriptor.id.clone();
    let registry = SecretProviderRegistry::new(vec![registration(descriptor.clone())])?;
    assert!(format!("{registry:?}").contains("env"));
    assert_eq!(registry.storage_capabilities(), vec![descriptor.clone()]);
    assert_eq!(registry.storage_capability(&provider)?, descriptor);

    let scope = SecretScope::new("local");
    // `cargo test` sets CARGO_PKG_NAME for every test process.
    let reference = SecretReference::new(provider.clone(), "CARGO_PKG_NAME")?;
    assert!(
        registry
            .inspect(&scope, &reference)
            .await?
            .revision
            .is_some()
    );
    assert!(
        registry
            .resolve(&scope, &reference)
            .await?
            .material
            .is_some()
    );
    assert!(
        registry
            .create(&scope, &reference, material("new")?, 1, None)
            .await
            .is_err()
    );
    assert!(
        registry
            .replace(&scope, &reference, material("new")?, expected(), 1, None)
            .await
            .is_err()
    );
    assert!(
        registry
            .stage(&scope, &reference, material("new")?, expected(), 1, None)
            .await
            .is_err()
    );
    assert!(
        registry
            .promote(&scope, &reference, expected(), 1)
            .await
            .is_err()
    );
    assert!(
        registry
            .unset(&scope, &reference, expected())
            .await
            .is_err()
    );
    assert!(
        registry
            .replace_batch(Vec::new(), 1, None)
            .await?
            .is_empty()
    );

    let long = SecretReference::new(provider, "x".repeat(256))?;
    assert!(registry.inspect(&scope, &long).await.is_err());
    let unknown_provider = StorageProviderId::new("unknown")?;
    let unknown = SecretReference::new(unknown_provider.clone(), "value")?;
    assert!(registry.storage_capability(&unknown_provider).is_err());
    assert!(registry.resolve(&scope, &unknown).await.is_err());
    assert!(
        registry
            .replace_batch(
                vec![
                    SecretReplacement {
                        scope: scope.clone(),
                        reference,
                        material: material("one")?,
                        expected: expected(),
                    },
                    SecretReplacement {
                        scope,
                        reference: unknown,
                        material: material("two")?,
                        expected: expected(),
                    },
                ],
                1,
                None,
            )
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn registration_validation_rejects_duplicates_limits_capability_mismatches_and_rotation_lies()
-> Result<(), Box<dyn std::error::Error>> {
    let descriptor = environment_storage_descriptor()?;
    assert!(
        SecretProviderRegistry::new(vec![
            registration(descriptor.clone()),
            registration(descriptor.clone()),
        ])
        .is_err()
    );

    let mut empty_limit = descriptor.clone();
    empty_limit.max_location_bytes = 0;
    assert!(SecretProviderRegistry::new(vec![registration(empty_limit)]).is_err());

    let mut implementation_mismatch = descriptor.clone();
    implementation_mismatch
        .operations
        .push(StorageOperation::Create);
    assert!(SecretProviderRegistry::new(vec![registration(implementation_mismatch)]).is_err());

    let mut missing_required = descriptor.clone();
    missing_required
        .operations
        .retain(|operation| *operation != StorageOperation::InspectMetadata);
    assert!(SecretProviderRegistry::new(vec![registration(missing_required)]).is_err());

    let mut invalid_rotation = descriptor;
    invalid_rotation.rotation = crate::domain::secret_contract::RotationCapability::Replace;
    assert!(SecretProviderRegistry::new(vec![registration(invalid_rotation)]).is_err());
    Ok(())
}
