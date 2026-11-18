use std::collections::HashSet;
use std::fs;
use std::sync::Arc;

use super::*;
use crate::app::ports::MaterializedEnvironmentWriter as _;
use crate::domain::environment::{
    CompiledEnvironmentManifest, CompiledServiceEnvironment, CompiledServiceManifest,
};

struct DescriptorDriver(DeliveryCapabilityDescriptor);

impl SecretDeliveryDriver for DescriptorDriver {
    fn descriptor(&self) -> &DeliveryCapabilityDescriptor {
        &self.0
    }

    fn publish(
        &self,
        _operation_id: OperationId,
        _bundle: &CompiledEnvironmentBundle,
    ) -> Result<PreparedEnvironmentBundle, MmError> {
        Err(MmError::State {
            message: "unused test driver".to_owned(),
        })
    }
}

fn bundle() -> CompiledEnvironmentBundle {
    CompiledEnvironmentBundle {
        manifest: CompiledEnvironmentManifest {
            project_name: "artifact-tests".to_owned(),
            environment: "development".to_owned(),
            input_hash: "input".to_owned(),
            services: vec![CompiledServiceManifest {
                module: "api".to_owned(),
                stack: "app".to_owned(),
                service: "web".to_owned(),
                variable_count: 1,
                secret_count: 1,
            }],
        },
        services: vec![
            CompiledServiceEnvironment::new(
                "api".to_owned(),
                "app".to_owned(),
                "web".to_owned(),
                b"TOKEN=secret\n".to_vec(),
            )
            .with_intent(
                crate::domain::runtime::RuntimeEnvironment::from_sensitive_values(
                    [("TOKEN".to_owned(), "secret".to_owned())]
                        .into_iter()
                        .collect(),
                ),
            ),
        ],
    }
}

#[test]
fn store_publishes_direct_and_file_bundles_then_disposes_idempotently()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let run = temp.path().join("run");
    let store = FilesystemEnvironmentArtifactStore::new(run.clone())?;
    assert!(format!("{store:?}").contains("FilesystemEnvironmentArtifactStore"));
    assert_eq!(store.capabilities().len(), 4);
    assert_eq!(store.delivery_capabilities().len(), 4);
    assert!(
        store.capabilities().iter().all(|descriptor| descriptor
            .runtimes
            .iter()
            .all(|runtime| { crate::domain::runtime::RuntimeKind::ALL.contains(runtime) })),
        "every delivery driver runs on a runtime mm still has"
    );
    let direct = direct_environment_descriptor()?.id;
    let env_file = operation_environment_file_descriptor()?.id;
    assert_eq!(store.delivery_capability(&direct)?.id, direct);
    assert_eq!(store.delivery_capability(&env_file)?.id, env_file);
    assert!(
        store
            .delivery_capability(&DeliveryDriverId::from_static("missing"))
            .is_err()
    );

    let compiled = bundle();
    let direct_id = OperationId(uuid::Uuid::from_u128(111));
    let direct_bundle = store.publish(EnvironmentArtifactRequest {
        operation_id: direct_id,
        delivery_driver: direct.clone(),
        bundle: &compiled,
    })?;
    assert_eq!(direct_bundle.services.len(), 1);
    assert!(direct_bundle.services[0].path.is_none());
    assert_eq!(direct_bundle.interpolation_environment.len(), 1);
    assert!(direct_bundle.root.join("app.compose.yaml").is_file());
    assert!(
        store
            .publish(EnvironmentArtifactRequest {
                operation_id: direct_id,
                delivery_driver: direct,
                bundle: &compiled,
            })
            .is_err()
    );

    let file_id = OperationId(uuid::Uuid::from_u128(112));
    let file_bundle = store.publish(EnvironmentArtifactRequest {
        operation_id: file_id,
        delivery_driver: env_file,
        bundle: &compiled,
    })?;
    let path = file_bundle.services[0]
        .path
        .as_ref()
        .ok_or("env file path missing")?;
    assert_eq!(fs::read(path)?, b"TOKEN=secret\n");
    assert!(file_bundle.interpolation_environment.is_empty());

    store.reconcile(&HashSet::from([file_id]))?;
    assert!(!direct_bundle.root.exists());
    assert!(file_bundle.root.exists());
    store.dispose(&file_bundle)?;
    store.dispose(&file_bundle)?;
    let mut escaped = file_bundle;
    escaped.root = temp.path().join("elsewhere");
    assert!(store.dispose(&escaped).is_err());
    Ok(())
}

#[test]
fn registration_reconciliation_names_and_materialization_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    let temp = crate::test_support::tempdir()?;
    let run = temp.path().join("run");
    let first: Arc<dyn SecretDeliveryDriver> =
        Arc::new(DirectEnvironmentDelivery::new(run.clone())?);
    let second: Arc<dyn SecretDeliveryDriver> =
        Arc::new(DirectEnvironmentDelivery::new(run.clone())?);
    assert!(
        FilesystemEnvironmentArtifactStore::from_drivers(run.clone(), vec![first, second]).is_err()
    );

    validate_artifact_component("safe-name_1", "service")?;
    for value in ["", "../escape", "bad.name", "bad name"] {
        assert!(validate_artifact_component(value, "service").is_err());
    }
    let operation_id = OperationId(uuid::Uuid::from_u128(113));
    assert!(placeholder_name(operation_id, 4).starts_with("MM_ENV_"));
    assert_eq!(
        direct_environment_descriptor()?.plaintext_persistence,
        PlaintextPersistence::None
    );
    assert_eq!(
        operation_environment_file_descriptor()?.plaintext_persistence,
        PlaintextPersistence::OperationScopedPrivateFile
    );

    reconcile_bundles(&run, &HashSet::new())?;
    create_private_directory(&run)?;
    fs::write(run.join("unexpected"), b"file")?;
    assert!(reconcile_bundles(&run, &HashSet::new()).is_err());
    fs::remove_file(run.join("unexpected"))?;
    fs::create_dir(run.join("not-an-operation"))?;
    assert!(reconcile_bundles(&run, &HashSet::new()).is_err());

    let output = temp.path().join("materialized.env");
    FilesystemMaterializedEnvironmentWriter.write(&output, b"A=1\n", false)?;
    assert_eq!(fs::metadata(&output)?.permissions().mode() & 0o777, 0o600);
    assert!(write_materialized_environment(&output, b"A=2\n", false).is_err());
    write_materialized_environment(&output, b"A=2\n", true)?;
    assert_eq!(fs::read(&output)?, b"A=2\n");
    let target = temp.path().join("target");
    fs::write(&target, b"safe")?;
    let link = temp.path().join("link.env");
    symlink(&target, &link)?;
    assert!(write_materialized_environment(&link, b"unsafe", true).is_err());
    assert_eq!(fs::read(&target)?, b"safe");
    Ok(())
}

#[test]
fn malformed_driver_descriptors_duplicate_values_and_overlay_invariants_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let valid = direct_environment_descriptor()?;
    for mutate in [
        |value: &mut DeliveryCapabilityDescriptor| value.max_material_bytes = 0,
        |value: &mut DeliveryCapabilityDescriptor| value.max_materialization_name_bytes = 0,
        |value: &mut DeliveryCapabilityDescriptor| value.material_kinds.clear(),
        |value: &mut DeliveryCapabilityDescriptor| value.runtimes.clear(),
        |value: &mut DeliveryCapabilityDescriptor| value.workload_kinds.clear(),
    ] {
        let mut descriptor = valid.clone();
        mutate(&mut descriptor);
        assert!(
            FilesystemEnvironmentArtifactStore::from_drivers(
                temp.path().join(uuid::Uuid::new_v4().to_string()),
                vec![Arc::new(DescriptorDriver(descriptor))],
            )
            .is_err()
        );
    }
    let collision = crate::adapters::secrets::podman_native_secret_descriptor()?;
    assert!(
        FilesystemEnvironmentArtifactStore::from_drivers(
            temp.path().join("collision"),
            vec![Arc::new(DescriptorDriver(collision))],
        )
        .is_err()
    );

    let store = FilesystemEnvironmentArtifactStore::new(temp.path().join("run"))?;
    let compiled = bundle();
    assert!(
        store
            .publish(EnvironmentArtifactRequest {
                operation_id: OperationId::new(),
                delivery_driver: DeliveryDriverId::from_static("missing"),
                bundle: &compiled,
            })
            .is_err()
    );
    let mut duplicate = bundle();
    let duplicate_service = bundle().services.pop().ok_or("test service missing")?;
    duplicate.services.push(duplicate_service);
    assert!(
        store
            .publish(EnvironmentArtifactRequest {
                operation_id: OperationId::new(),
                delivery_driver: direct_environment_descriptor()?.id,
                bundle: &duplicate,
            })
            .is_err()
    );
    let mut unsafe_bundle = bundle();
    unsafe_bundle.services[0].service = "../escape".to_owned();
    assert!(
        store
            .publish(EnvironmentArtifactRequest {
                operation_id: OperationId::new(),
                delivery_driver: operation_environment_file_descriptor()?.id,
                bundle: &unsafe_bundle,
            })
            .is_err()
    );

    let service = PreparedServiceEnvironment {
        module: "api".to_owned(),
        stack: "app".to_owned(),
        service: "web".to_owned(),
        path: None,
    };
    assert!(
        write_runtime_overlays(
            temp.path(),
            temp.path(),
            EnvironmentDeliveryMode::EnvFile,
            std::slice::from_ref(&service),
            &BTreeMap::new(),
        )
        .is_err()
    );
    assert!(
        write_runtime_overlays(
            temp.path(),
            temp.path(),
            EnvironmentDeliveryMode::Direct,
            &[service],
            &BTreeMap::new(),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn cleanup_and_private_filesystem_helpers_cover_failure_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let temp = crate::test_support::tempdir()?;
    let missing = temp.path().join("missing-staging");
    let primary = MmError::State {
        message: "publication failed".to_owned(),
    };
    assert!(matches!(
        environment_publication_failure(&missing, OperationId::new(), primary),
        MmError::State { .. }
    ));

    let staging_file = temp.path().join("staging-file");
    fs::write(&staging_file, b"not a directory")?;
    let primary = MmError::State {
        message: "publication failed".to_owned(),
    };
    assert!(matches!(
        environment_publication_failure(&staging_file, OperationId::new(), primary),
        MmError::OperationFailure { .. }
    ));

    let temporary = temp.path().join("temporary");
    fs::write(&temporary, b"remove")?;
    assert!(matches!(
        with_temporary_cleanup(
            MmError::State {
                message: "primary".to_owned(),
            },
            &temporary,
        ),
        MmError::State { .. }
    ));
    assert!(!temporary.exists());
    assert!(matches!(
        with_temporary_cleanup(
            MmError::State {
                message: "primary".to_owned(),
            },
            &temporary,
        ),
        MmError::State { .. }
    ));
    fs::create_dir(&temporary)?;
    assert!(matches!(
        with_temporary_cleanup(
            MmError::State {
                message: "primary".to_owned(),
            },
            &temporary,
        ),
        MmError::OperationFailure { .. }
    ));

    let blocked = temp.path().join("blocked");
    fs::write(&blocked, b"file")?;
    assert!(create_private_directory(&blocked.join("child")).is_err());
    assert!(write_new_private_file(&blocked.join("child"), b"value").is_err());
    assert!(set_private_permissions(&temp.path().join("absent")).is_err());

    let invalid_name = temp.path().join(OsString::from_vec(vec![0xff]));
    assert!(write_materialized_environment(&invalid_name, b"A=1\n", false).is_err());
    assert!(write_materialized_environment(&blocked.join("child/value"), b"A=1\n", true).is_err());
    assert!(write_materialized_environment(Path::new("/"), b"A=1\n", false).is_err());
    assert!(write_materialized_environment(Path::new("/proc/mm-env"), b"A=1\n", true).is_err());
    let directory_destination = temp.path().join("directory-destination");
    fs::create_dir(&directory_destination)?;
    assert!(write_materialized_environment(&directory_destination, b"A=1\n", true).is_err());
    assert!(create_private_directory(Path::new("/proc/mm-environment-directory")).is_err());
    assert!(write_new_private_file(&temp.path().join("x".repeat(300)), b"value").is_err());
    Ok(())
}

#[test]
fn reconciliation_and_disposal_reject_non_unicode_and_non_directory_entries()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let temp = crate::test_support::tempdir()?;
    let run = temp.path().join("run");
    create_private_directory(&run)?;
    fs::create_dir(run.join(OsString::from_vec(vec![0xff])))?;
    assert!(reconcile_bundles(&run, &HashSet::new()).is_err());

    let operation_id = OperationId::new();
    let root = run.join(operation_id.0.to_string());
    fs::write(&root, b"not a directory")?;
    let prepared = PreparedEnvironmentBundle {
        operation_id,
        root,
        manifest: bundle().manifest,
        services: Vec::new(),
        interpolation_environment: Default::default(),
        runtime_secret_overlays: Vec::new(),
        runtime_secret_rollback_overlays: Vec::new(),
        registry_credentials: Vec::new(),
    };
    assert!(dispose_bundle(&run, &prepared).is_err());
    Ok(())
}
