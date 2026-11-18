#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use mm::adapters::environment::{
    DirectEnvironmentDelivery, FilesystemEnvironmentArtifactStore, OperationEnvironmentFileDelivery,
};
use mm::adapters::secrets::{AgeSecretStore, SecretProviderRegistry, StorageProviderRegistration};
use mm::app::ports::{
    EnvironmentArtifactRequest, EnvironmentArtifactStore, SecretBatchReplacer, SecretCreator,
    SecretDeliveryDriver, SecretMaterialResolver, SecretMetadataInspector, SecretRemover,
    SecretReplacer, SecretStorageCapabilityCatalog,
};
use mm::domain::environment::{
    CompiledEnvironmentBundle, CompiledEnvironmentManifest, CompiledServiceEnvironment,
    CompiledServiceManifest,
};
use mm::domain::error::MmError;
use mm::domain::operation::OperationId;
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretReplacement,
    SecretResolution, SecretScope, SecretStatus,
};
use mm::domain::secret_contract::{
    AuthenticationKind, DeliveryDriverId, LogicalSecretId, MaterializationName,
    PlaintextPersistence, RevisionSemantics, RotationCapability, SecretCompatibilityRequest,
    SecretMaterialKind, SecretWorkloadKind, StorageCapabilityDescriptor, StorageEnvironmentScoping,
    StorageOperation, StorageProviderId, plan_secret_compatibility,
};

mod support;
mod temp_dirs;

const SENTINEL: &str = "mm-secret-contract-sentinel-4cb91e";

#[tokio::test]
async fn age_and_environment_pass_storage_conformance() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let registry = support::secret_registry(age)?;

    let age_id = StorageProviderId::new("age")?;
    let age_descriptor = registry.storage_capability(&age_id)?;
    assert_storage_descriptor(&age_descriptor)?;
    assert_eq!(
        age_descriptor.revision_semantics,
        RevisionSemantics::CompareAndSwap
    );
    assert_eq!(age_descriptor.rotation, RotationCapability::Staged);
    assert!(!age_descriptor.metadata_without_material_resolution);
    let age_reference: SecretReference = "age://conformance/value".parse()?;
    let scope = SecretScope::new("integration");
    assert_eq!(
        registry.inspect(&scope, &age_reference).await?.status,
        SecretStatus::Unset
    );
    let created = registry
        .create(
            &scope,
            &age_reference,
            SecretMaterial::new(SENTINEL.to_owned())?,
            1_000,
        )
        .await?;
    let inspected = registry.inspect(&scope, &age_reference).await?;
    assert_eq!(
        inspected.revision.as_deref(),
        Some(created.revision.as_str())
    );
    let resolved = registry.resolve(&scope, &age_reference).await?;
    assert!(
        resolved
            .material
            .is_some_and(|material| material.expose(|value| value == SENTINEL))
    );
    assert!(
        registry
            .replace(
                &scope,
                &age_reference,
                SecretMaterial::new("replacement".to_owned())?,
                SecretExpectedState {
                    active_revision: Some("stale".to_owned()),
                    candidate_revision: None,
                },
                2_000,
            )
            .await
            .is_err()
    );
    assert!(
        registry
            .unset(&scope, &age_reference, inspected.expected_state())
            .await?
    );

    let env_id = StorageProviderId::new("env")?;
    let env_descriptor = registry.storage_capability(&env_id)?;
    assert_storage_descriptor(&env_descriptor)?;
    assert_eq!(
        env_descriptor.revision_semantics,
        RevisionSemantics::Observed
    );
    assert_eq!(env_descriptor.rotation, RotationCapability::None);
    assert!(env_descriptor.metadata_without_material_resolution);
    let env_reference: SecretReference = "env://MM_MISSING_SECRET_TEST_VARIABLE".parse()?;
    assert_eq!(
        registry.inspect(&scope, &env_reference).await?.status,
        SecretStatus::Unset
    );
    assert!(
        registry
            .resolve(&scope, &env_reference)
            .await?
            .material
            .is_none()
    );
    assert!(
        registry
            .create(
                &scope,
                &env_reference,
                SecretMaterial::new("not-written".to_owned())?,
                1_000,
            )
            .await
            .is_err()
    );
    let oversized_env_location = SecretReference::new(env_id, "A".repeat(256))?;
    assert!(
        registry
            .inspect(&scope, &oversized_env_location)
            .await
            .is_err()
    );
    let cross_provider = registry
        .replace_batch(
            vec![
                SecretReplacement {
                    scope: scope.clone(),
                    reference: age_reference.clone(),
                    material: SecretMaterial::new("age-value".to_owned())?,
                    expected: SecretExpectedState {
                        active_revision: None,
                        candidate_revision: None,
                    },
                },
                SecretReplacement {
                    scope: scope.clone(),
                    reference: env_reference.clone(),
                    material: SecretMaterial::new("env-value".to_owned())?,
                    expected: SecretExpectedState {
                        active_revision: None,
                        candidate_revision: None,
                    },
                },
            ],
            2_000,
        )
        .await
        .err()
        .ok_or("cross-provider atomic batch was accepted")?;
    assert!(
        cross_provider
            .to_string()
            .contains("may not span storage providers")
    );
    Ok(())
}

#[test]
fn direct_and_env_file_pass_delivery_conformance() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let direct = Arc::new(DirectEnvironmentDelivery::new(run.clone())?);
    let env_file = Arc::new(OperationEnvironmentFileDelivery::new(run.clone())?);
    assert_delivery_descriptor(direct.as_ref())?;
    assert_delivery_descriptor(env_file.as_ref())?;
    let store = FilesystemEnvironmentArtifactStore::from_drivers(
        run,
        vec![
            direct as Arc<dyn SecretDeliveryDriver>,
            env_file as Arc<dyn SecretDeliveryDriver>,
        ],
    )?;
    let bundle = delivery_bundle();

    for (id, persistence) in [
        ("direct-environment", PlaintextPersistence::None),
        (
            "operation-env-file",
            PlaintextPersistence::OperationScopedPrivateFile,
        ),
    ] {
        let operation_id = OperationId::new();
        let prepared = store.publish(EnvironmentArtifactRequest {
            operation_id,
            delivery_driver: DeliveryDriverId::new(id)?,
            bundle: &bundle,
        })?;
        match persistence {
            PlaintextPersistence::None => {
                assert!(
                    prepared
                        .services
                        .iter()
                        .all(|service| service.path.is_none())
                );
                for file in regular_files(&prepared.root)? {
                    assert!(
                        !fs::read(file)?
                            .windows(SENTINEL.len())
                            .any(|bytes| bytes == SENTINEL.as_bytes())
                    );
                }
            }
            PlaintextPersistence::OperationScopedPrivateFile => {
                let path = prepared.services[0]
                    .path
                    .as_ref()
                    .ok_or("missing env file")?;
                assert!(fs::read_to_string(path)?.contains(SENTINEL));
                assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
            }
            PlaintextPersistence::RevisionScopedPrivateFile
            | PlaintextPersistence::ProviderManaged => {
                return Err("unexpected persistence".into());
            }
        }
        store.dispose(&prepared)?;
        assert!(!prepared.root.exists());
    }
    Ok(())
}

#[tokio::test]
async fn fake_provider_is_registered_without_business_logic_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Arc::new(FakeProvider);
    let descriptor = fake_descriptor()?;
    let registry = SecretProviderRegistry::new(vec![StorageProviderRegistration {
        descriptor: descriptor.clone(),
        inspector: fake.clone(),
        resolver: fake,
        creator: None,
        replacer: None,
        batch_replacer: None,
        candidates: None,
        remover: None,
    }])?;
    let reference = SecretReference::new(descriptor.id, "logical-location")?;
    assert_eq!(
        registry
            .inspect(&SecretScope::new("test"), &reference)
            .await?
            .status,
        SecretStatus::Set
    );
    let mut dishonest_descriptor = fake_descriptor()?;
    dishonest_descriptor
        .operations
        .push(StorageOperation::Create);
    assert!(
        SecretProviderRegistry::new(vec![StorageProviderRegistration {
            descriptor: dishonest_descriptor,
            inspector: Arc::new(FakeProvider),
            resolver: Arc::new(FakeProvider),
            creator: None,
            replacer: None,
            batch_replacer: None,
            candidates: None,
            remover: None,
        }])
        .is_err()
    );
    Ok(())
}

#[test]
fn compatibility_fails_closed_with_actionable_context() -> Result<(), Box<dyn std::error::Error>> {
    let backend = fake_descriptor()?;
    let delivery = mm::adapters::environment::direct_environment_descriptor()?;
    let logical = LogicalSecretId::new("database.password")?;
    let materialization = MaterializationName::environment("DB_PASSWORD")?;
    let error = plan_secret_compatibility(SecretCompatibilityRequest {
        logical_secret: &logical,
        backend: &backend,
        delivery: &delivery,
        runtime: RuntimeKind::PodmanCompose,
        workload: SecretWorkloadKind::Build,
        material_kind: SecretMaterialKind::Binary,
        materialization_name: &materialization,
    })
    .err()
    .ok_or("incompatible plan was accepted")?;
    let message = error.to_string();
    for expected in [
        "database.password",
        "fake",
        "direct-environment",
        "PodmanCompose",
        "Build",
        "supports runtimes",
    ] {
        assert!(message.contains(expected), "missing {expected}: {message}");
    }
    Ok(())
}

#[test]
fn material_is_bounded_and_redacted() -> Result<(), Box<dyn std::error::Error>> {
    let material = SecretMaterial::new(SENTINEL.to_owned())?;
    assert_eq!(format!("{material:?}"), "SecretMaterial([REDACTED])");
    assert!(!format!("{material:?}").contains(SENTINEL));
    assert!(
        SecretMaterial::new("x".repeat(mm::domain::secret_contract::MAX_SECRET_MATERIAL_BYTES + 1))
            .is_err()
    );
    Ok(())
}

fn assert_storage_descriptor(
    descriptor: &StorageCapabilityDescriptor,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(descriptor.supports(StorageOperation::InspectMetadata));
    assert!(descriptor.supports(StorageOperation::ResolveMaterial));
    assert!(descriptor.max_material_bytes > 0);
    assert!(descriptor.max_location_bytes > 0);
    assert!(!descriptor.material_kinds.is_empty());
    if descriptor.revision_semantics == RevisionSemantics::CompareAndSwap {
        assert!(descriptor.supports(StorageOperation::Replace));
    }
    if descriptor.rotation == RotationCapability::Staged {
        assert!(descriptor.supports(StorageOperation::StageCandidate));
        assert!(descriptor.supports(StorageOperation::PromoteCandidate));
    }
    Ok(())
}

fn assert_delivery_descriptor(
    driver: &dyn SecretDeliveryDriver,
) -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = driver.descriptor();
    assert!(descriptor.runtimes.contains(&RuntimeKind::PodmanCompose));
    assert!(
        descriptor
            .workload_kinds
            .contains(&SecretWorkloadKind::Service)
    );
    assert_eq!(descriptor.material_kinds, vec![SecretMaterialKind::Text]);
    assert!(descriptor.max_material_bytes > 0);
    assert!(descriptor.max_materialization_name_bytes > 0);
    Ok(())
}

fn delivery_bundle() -> CompiledEnvironmentBundle {
    CompiledEnvironmentBundle {
        manifest: CompiledEnvironmentManifest {
            project_name: "conformance".to_owned(),
            environment: "integration".to_owned(),
            input_hash: "input".to_owned(),
            services: vec![CompiledServiceManifest {
                module: "api".to_owned(),
                stack: "app".to_owned(),
                service: "api".to_owned(),
                variable_count: 1,
                secret_count: 1,
            }],
        },
        services: vec![
            CompiledServiceEnvironment::new(
                "api".to_owned(),
                "app".to_owned(),
                "api".to_owned(),
                format!("TOKEN={SENTINEL}\n").into_bytes(),
            )
            .with_intent(
                mm::domain::runtime::RuntimeEnvironment::from_sensitive_values(
                    [("TOKEN".to_owned(), SENTINEL.to_owned())]
                        .into_iter()
                        .collect(),
                ),
            ),
        ],
    }
}

fn regular_files(root: &Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            files.extend(regular_files(&entry.path())?);
        } else {
            files.push(entry.path());
        }
    }
    Ok(files)
}

#[derive(Debug)]
struct FakeProvider;

#[async_trait]
impl SecretMetadataInspector for FakeProvider {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Ok(SecretMetadata {
            status: SecretStatus::Set,
            revision: None,
            created_at_unix_ms: None,
            candidate_revision: None,
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for FakeProvider {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Ok(SecretResolution {
            metadata: self.inspect(scope, reference).await?,
            material: Some(
                SecretMaterial::new("fake".to_owned())
                    .map_err(|message| MmError::Secret { message })?,
            ),
        })
    }
}

fn fake_descriptor() -> Result<StorageCapabilityDescriptor, MmError> {
    Ok(StorageCapabilityDescriptor {
        id: StorageProviderId::new("fake").map_err(|message| MmError::Secret { message })?,
        operations: vec![
            StorageOperation::InspectMetadata,
            StorageOperation::ResolveMaterial,
        ],
        revision_semantics: RevisionSemantics::None,
        max_material_bytes: 64,
        max_location_bytes: 64,
        environment_scoping: StorageEnvironmentScoping::Isolated,
        material_kinds: vec![SecretMaterialKind::Text],
        rotation: RotationCapability::None,
        authentication: AuthenticationKind::LocalIdentity,
        metadata_without_material_resolution: true,
    })
}
