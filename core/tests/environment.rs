use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod temp_dirs;
#[cfg(unix)]
use std::sync::Barrier;
#[cfg(unix)]
use std::thread;

use mm::MmError;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::{
    EnvironmentIndexer, FilesystemEnvironmentArtifactStore, FilesystemMaterializedEnvironmentWriter,
};
use mm::adapters::process::TokioProcessRunner;
use mm::adapters::secrets::{
    AgeSecretStore, EnvironmentSecretStore, SecretProviderRegistry, environment_storage_descriptor,
    load_or_create_environment_revision_key,
};
use mm::adapters::secrets::{compose_file_secret_descriptor, podman_native_secret_descriptor};
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{
    CommandSpec, EnvironmentArtifactRequest, EnvironmentArtifactStore, EnvironmentValueStore,
    ProcessRunner, SecretCandidateLifecycle, SecretCreator, SecretDeliveryCapabilityCatalog,
    SecretMaterialResolver, SecretMetadataInspector, SecretRemover, SecretReplacer,
    SecretStorageCapabilityCatalog, StateStore,
};
use mm::app::{EnvironmentDependencies, EnvironmentManager, WorkspaceSelection};
use mm::domain::environment::{
    CompiledEnvironmentBundle, CompiledEnvironmentManifest, CompiledServiceEnvironment,
    CompiledServiceManifest, ResolutionStatus, ValueSource,
};
use mm::domain::environment_delivery::{
    EnvironmentDeliveryMode, EnvironmentDeliveryProvenance, resolve_environment_delivery,
};
use mm::domain::operation::OperationId;
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretScope, SecretStatus,
};
use mm::domain::secret_contract::{
    DeliveryCapabilityDescriptor, DeliveryDriverId, StorageCapabilityDescriptor, StorageOperation,
    StorageProviderId,
};

#[tokio::test]
async fn process_environment_storage_and_revision_key_lifecycle_are_private()
-> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "MM_ENVIRONMENT_INTEGRATION_CHILD";
    const VALUE: &str = "MM_ENVIRONMENT_INTEGRATION_VALUE";
    if std::env::var_os(CHILD).is_some() {
        let store = EnvironmentSecretStore::new([17; 32]);
        let reference: SecretReference = format!("env://{VALUE}").parse()?;
        let scope = SecretScope::new("integration");
        assert_eq!(
            store.inspect(&scope, &reference).await?.status,
            SecretStatus::Set
        );
        assert!(store.resolve(&scope, &reference).await?.material.is_some());
        return Ok(());
    }

    let status = std::process::Command::new(std::env::current_exe()?)
        .args([
            "process_environment_storage_and_revision_key_lifecycle_are_private",
            "--exact",
        ])
        .env(CHILD, "1")
        .env(VALUE, "integration-secret")
        .status()?;
    assert!(status.success());

    let store = EnvironmentSecretStore::new([17; 32]);
    assert!(format!("{store:?}").contains("REDACTED"));
    let scope = SecretScope::new("integration");
    let missing: SecretReference = "env://MM_MODMON_INTEGRATION_DEFINITELY_UNSET".parse()?;
    assert_eq!(
        store.inspect(&scope, &missing).await?.status,
        SecretStatus::Unset
    );
    assert_eq!(
        store.resolve(&scope, &missing).await?.metadata.status,
        SecretStatus::Unset
    );
    assert_eq!(environment_storage_descriptor()?.id.as_str(), "env");

    let temporary = temp_dirs::tempdir()?;
    let key_path = temporary.path().join("keys/environment.key");
    let first = load_or_create_environment_revision_key(&key_path)?;
    assert_eq!(load_or_create_environment_revision_key(&key_path)?, first);
    let short = temporary.path().join("short.key");
    fs::write(&short, [0])?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&short, fs::Permissions::from_mode(0o600))?;
    }
    assert!(load_or_create_environment_revision_key(&short).is_err());
    Ok(())
}

#[allow(clippy::duplicate_mod)]
mod support;

const SENTINEL: &str = "TOP_SECRET_SENTINEL_DO_NOT_LEAK";

#[test]
fn environment_index_identity_is_independent_of_workspace_root()
-> Result<(), Box<dyn std::error::Error>> {
    let first = temp_dirs::tempdir()?;
    let second = temp_dirs::tempdir()?;
    copy_fixture(first.path())?;
    copy_fixture(second.path())?;
    let first = ConfigLoader.load_from(first.path())?;
    let second = ConfigLoader.load_from(second.path())?;
    let first = EnvironmentIndexer.index(&first, "local");
    let second = EnvironmentIndexer.index(&second, "local");
    assert_eq!(first.input_hash, second.input_hash);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn indexes_overrides_compiles_and_materializes_without_public_leaks()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identities/age.key"),
        home.path().join("secrets/project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let manager = environment_manager(
        state,
        values,
        Arc::clone(&router),
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    assert_eq!(format!("{manager:?}"), "EnvironmentManager { .. }");
    let selection = WorkspaceSelection::new(project.path(), None);
    let reference: SecretReference = "age://local/db-password".parse()?;
    router
        .create(
            &env_scope(),
            &reference,
            SecretMaterial::new(SENTINEL.to_owned())?,
            10_000,
        )
        .await?;
    manager.set_override(&selection, "DB.HOST", "postgres.local", None)?;

    let status = manager.status(&selection, true).await?;
    assert_eq!(status.delivery.mode, EnvironmentDeliveryMode::EnvFile);
    assert_eq!(
        status.delivery.provenance,
        EnvironmentDeliveryProvenance::BuiltInDefault
    );
    assert!(
        status
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != mm::domain::diagnostic::Severity::Error)
    );
    let host = &status.modules[0].services[0].variables[0];
    let password = &status.modules[0].services[0].variables[2];
    assert_eq!(host.source, ValueSource::SqliteOverride);
    assert_eq!(host.value.as_deref(), Some("postgres.local"));
    assert!(password.sensitive);
    assert_eq!(password.status, ResolutionStatus::Resolved);
    assert!(!serde_json::to_string(&status)?.contains(SENTINEL));

    let bundle = manager.compile(&selection).await?;
    assert!(!format!("{bundle:?}").contains(SENTINEL));
    assert!(
        bundle.services[0]
            .bytes()
            .windows(SENTINEL.len())
            .any(|window| window == SENTINEL.as_bytes())
    );
    let artifacts = FilesystemEnvironmentArtifactStore::new(home.path().join("run"))?;
    let prepared = artifacts.publish(EnvironmentArtifactRequest {
        operation_id: OperationId::new(),
        delivery_driver: mm::domain::secret_contract::DeliveryDriverId::new(
            EnvironmentDeliveryMode::EnvFile.driver_id(),
        )?,
        bundle: &bundle,
    })?;
    let path = prepared.services[0]
        .path
        .as_ref()
        .ok_or("env-file path is missing")?;
    assert!(path.is_file());
    assert!(fs::read_to_string(path)?.contains(SENTINEL));
    use std::os::unix::fs::PermissionsExt as _;
    assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    artifacts.dispose(&prepared)?;
    assert!(!prepared.root.exists());

    let direct = artifacts.publish(EnvironmentArtifactRequest {
        operation_id: OperationId::new(),
        delivery_driver: mm::domain::secret_contract::DeliveryDriverId::new(
            EnvironmentDeliveryMode::Direct.driver_id(),
        )?,
        bundle: &bundle,
    })?;
    assert!(direct.services.iter().all(|service| service.path.is_none()));
    let direct_files = regular_files(&direct.root)?;
    assert!(
        direct_files
            .iter()
            .all(|path| !path.to_string_lossy().ends_with(".env")
                && !path.to_string_lossy().ends_with(".secrets.env")
                && path.file_name().and_then(|name| name.to_str()) != Some("manifest.json"))
    );
    for file in &direct_files {
        let bytes = fs::read(file)?;
        assert!(
            !bytes
                .windows(SENTINEL.len())
                .any(|window| window == SENTINEL.as_bytes())
        );
        assert!(
            !bytes
                .windows("postgres.local".len())
                .any(|window| window == b"postgres.local")
        );
    }
    let overlay = fs::read_to_string(
        direct
            .root
            .join(format!("{}.compose.yaml", direct.services[0].stack)),
    )?;
    assert!(overlay.contains("MM_ENV_"));
    assert!(overlay.contains("DB_PASSWORD"));
    assert!(!overlay.contains(SENTINEL));
    assert_eq!(
        fs::metadata(
            direct
                .root
                .join(format!("{}.compose.yaml", direct.services[0].stack))
        )?
        .permissions()
        .mode()
            & 0o777,
        0o600
    );
    let specification = CommandSpec::new("/usr/bin/env")
        .interpolation_environment(&direct.interpolation_environment);
    let debug = format!("{specification:?}");
    let record = specification.redacted_record();
    let record_json = serde_json::to_string(&record)?;
    assert!(!debug.contains(SENTINEL));
    assert!(!record_json.contains(SENTINEL));
    assert_eq!(
        record.environment_count,
        direct.interpolation_environment.len()
    );
    assert!(
        record
            .environment_keys
            .iter()
            .all(|key| key.starts_with("MM_ENV_"))
    );
    let output = TokioProcessRunner.run(specification).await?;
    assert!(!output.stdout.contains(SENTINEL));
    assert!(output.stdout.contains("[REDACTED]"));
    artifacts.dispose(&direct)?;
    assert!(!direct.root.exists());
    let orphan_operation = OperationId::new();
    let orphan = artifacts.publish(EnvironmentArtifactRequest {
        operation_id: orphan_operation,
        delivery_driver: mm::domain::secret_contract::DeliveryDriverId::new(
            EnvironmentDeliveryMode::EnvFile.driver_id(),
        )?,
        bundle: &bundle,
    })?;
    artifacts.reconcile(&HashSet::from([orphan_operation]))?;
    assert!(orphan.root.exists());
    artifacts.reconcile(&HashSet::new())?;
    assert!(!orphan.root.exists());
    let materialized = manager.materialize(&selection, "api", None, false).await?;
    assert_eq!(materialized.service, "api");
    assert!(fs::read_to_string(&materialized.path)?.contains(SENTINEL));

    let ciphertext = fs::read(home.path().join("secrets/project.age"))?;
    assert!(
        !ciphertext
            .windows(SENTINEL.len())
            .any(|window| window == SENTINEL.as_bytes())
    );
    let database = fs::read(home.path().join("state.sqlite3"))?;
    assert!(
        !database
            .windows(SENTINEL.len())
            .any(|window| window == SENTINEL.as_bytes())
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        assert_eq!(
            fs::metadata(&materialized.path)?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(home.path().join("identities/age.key"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::set_permissions(&materialized.path, fs::Permissions::from_mode(0o644))?;
        manager.materialize(&selection, "api", None, true).await?;
        assert_eq!(
            fs::metadata(&materialized.path)?.permissions().mode() & 0o777,
            0o600
        );
        fs::remove_file(&materialized.path)?;
        let external_env = home.path().join("external.env");
        fs::write(&external_env, "EXTERNAL=value\n")?;
        symlink(&external_env, &materialized.path)?;
        assert!(manager.diff(&selection, "api", None).await.is_err());
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn age_status_is_metadata_only_and_env_provider_is_read_only()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    let router = support::secret_registry(Arc::clone(&age))?;
    let age_reference: SecretReference = "age://missing".parse()?;
    assert_eq!(
        router.inspect(&env_scope(), &age_reference).await?.status,
        SecretStatus::ProviderUnavailable
    );
    age.initialize()?;
    assert_eq!(
        router.inspect(&env_scope(), &age_reference).await?.status,
        SecretStatus::Unset
    );

    let env_reference: SecretReference = "env://MM_ENV_TEST_SECRET".parse()?;
    assert!(
        router
            .create(
                &env_scope(),
                &env_reference,
                SecretMaterial::new("value".to_owned())?,
                10_000,
            )
            .await
            .is_err()
    );
    assert!(SecretMaterial::new(String::new()).is_err());
    Ok(())
}

#[tokio::test]
async fn service_overrides_replace_and_append_without_silent_ignores()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    let secrets = support::secret_registry(age)?;
    let manager = environment_manager(
        state,
        values,
        secrets,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    let selection = WorkspaceSelection::new(service_override_fixture(), None);

    let bundle = manager.compile(&selection).await?;
    let api = bundle
        .services
        .iter()
        .find(|service| service.service == "api")
        .ok_or("missing api output")?;
    let worker = bundle
        .services
        .iter()
        .find(|service| service.service == "worker")
        .ok_or("missing worker output")?;
    assert_eq!(
        std::str::from_utf8(api.bytes())?,
        "ROLE=api\nDB_HOST=database\n"
    );
    assert_eq!(
        std::str::from_utf8(worker.bytes())?,
        "ROLE=worker\nDB_HOST=database\nEXTRA=enabled\n"
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn unset_optional_secrets_use_fallbacks_without_resolving_material()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    fs::write(
        project.path().join("api/.env.example"),
        "DB_HOST= # mm:from=DB.HOST required\nDB_PASSWORD=fallback # mm:from=DB.PASSWORD\nOPTIONAL= # mm:from=DB.OPTIONAL\n",
    )?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "environments:\n",
            "  database.optional:\n    storage:\n      provider: age\n      location: local/optional\nenvironments:\n",
        )
        .replace(
            "      DB.PASSWORD:\n        secret: database.password\n",
            "      DB.PASSWORD:\n        secret: database.password\n      DB.OPTIONAL:\n        secret: database.optional\n",
        );
    fs::write(&config_path, config)?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let secrets = support::secret_registry(age)?;
    let manager = environment_manager(
        state,
        values,
        secrets,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    let selection = WorkspaceSelection::new(project.path(), None);

    let status = manager.status(&selection, true).await?;
    assert!(
        status
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != mm::domain::diagnostic::Severity::Error)
    );
    let variables = &status.modules[0].services[0].variables;
    assert_eq!(variables[1].source, ValueSource::TemplateDefault);
    assert_eq!(variables[1].value.as_deref(), Some("fallback"));
    assert_eq!(variables[2].status, ResolutionStatus::Unset);
    let bundle = manager.compile(&selection).await?;
    assert_eq!(
        std::str::from_utf8(bundle.services[0].bytes())?,
        "DB_HOST=database\nDB_PASSWORD=fallback\nOPTIONAL=\n"
    );
    Ok(())
}

#[tokio::test]
async fn compose_sensitive_characters_are_emitted_without_interpolation()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    let secrets = support::secret_registry(age)?;
    let manager = environment_manager(
        state,
        values,
        secrets,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    let selection = WorkspaceSelection::new(service_override_fixture(), None);
    manager.set_override(&selection, "DB.HOST", "${HOME}", None)?;

    let bundle = manager.compile(&selection).await?;
    assert!(std::str::from_utf8(bundle.services[0].bytes())?.contains("DB_HOST=\"$${HOME}\""));
    assert!(
        manager
            .set_override(&selection, "DB.HOST", "unsafe\nvalue", None)
            .is_err()
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn environment_status_never_reads_a_symlinked_template()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let external = temp_dirs::tempdir()?;
    let external_template = external.path().join("stolen.env");
    fs::write(&external_template, format!("LEAK={SENTINEL}\n"))?;
    fs::remove_file(project.path().join("api/.env.example"))?;
    symlink(&external_template, project.path().join("api/.env.example"))?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    let secrets = support::secret_registry(age)?;
    let manager = environment_manager(
        state,
        values,
        secrets,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    let status = manager
        .status(&WorkspaceSelection::new(project.path(), None), false)
        .await?;

    assert!(!serde_json::to_string(&status)?.contains(SENTINEL));
    assert!(
        status
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_ENV_TEMPLATE_UNSAFE")
    );
    fs::remove_file(project.path().join("api/.env.example"))?;
    fs::create_dir(project.path().join("api/.env.example"))?;
    let status = manager
        .status(&WorkspaceSelection::new(project.path(), None), false)
        .await?;
    assert!(
        status
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_ENV_TEMPLATE_READ")
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn materialization_and_secret_initialization_refuse_symlink_paths()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let root = temp_dirs::tempdir()?;
    let external = temp_dirs::tempdir()?;
    symlink(external.path(), root.path().join("linked"))?;
    let materialized = root.path().join("linked/.env");
    let result = mm::adapters::environment::write_materialized_environment(
        &materialized,
        b"SECRET=not-written\n",
        false,
    );
    assert!(result.is_err());
    assert!(!external.path().join(".env").exists());

    let identity_target = external.path().join("identity.key");
    fs::write(&identity_target, "not-an-identity")?;
    symlink(&identity_target, root.path().join("identity.key"))?;
    let age = AgeSecretStore::new(
        root.path().join("identity.key"),
        root.path().join("project.age"),
    );
    assert!(age.initialize().is_err());
    assert!(!root.path().join("project.age").exists());

    use std::os::unix::fs::PermissionsExt;
    let unsafe_parent = root.path().join("unsafe-parent");
    fs::create_dir(&unsafe_parent)?;
    fs::set_permissions(&unsafe_parent, fs::Permissions::from_mode(0o777))?;
    let age = AgeSecretStore::new(
        unsafe_parent.join("identity.key"),
        root.path().join("unsafe-parent.age"),
    );
    assert!(age.initialize().is_err());
    assert_eq!(
        fs::metadata(&unsafe_parent)?.permissions().mode() & 0o777,
        0o777
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn artifact_store_rejects_path_components_from_untrusted_bundles()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store = FilesystemEnvironmentArtifactStore::new(home.path().join("run"))?;
    let bundle = CompiledEnvironmentBundle {
        manifest: CompiledEnvironmentManifest {
            project_name: "test".to_owned(),
            environment: "local".to_owned(),
            input_hash: "hash".to_owned(),
            services: vec![CompiledServiceManifest {
                module: "..".to_owned(),
                stack: "app".to_owned(),
                service: "api".to_owned(),
                variable_count: 1,
                secret_count: 1,
            }],
        },
        services: vec![CompiledServiceEnvironment::new(
            "..".to_owned(),
            "app".to_owned(),
            "api".to_owned(),
            b"SECRET=not-written\n".to_vec(),
        )],
    };

    assert!(
        store
            .publish(EnvironmentArtifactRequest {
                operation_id: OperationId::new(),
                delivery_driver: mm::domain::secret_contract::DeliveryDriverId::new(
                    EnvironmentDeliveryMode::Direct.driver_id(),
                )?,
                bundle: &bundle,
            })
            .is_err()
    );
    assert!(!home.path().join("run/api.env").exists());
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn concurrent_projects_share_one_age_identity_without_overwrite()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let identity = home.path().join("identity.key");
    let barrier = Arc::new(Barrier::new(2));
    let handles = ["first.age", "second.age"].map(|store_name| {
        let identity = identity.clone();
        let store = home.path().join(store_name);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            let age = AgeSecretStore::new(identity, store);
            barrier.wait();
            age.initialize()
        })
    });
    let [first_handle, second_handle] = handles;
    let first = first_handle
        .join()
        .map_err(|_| "first initializer panicked")??;
    let second = second_handle
        .join()
        .map_err(|_| "second initializer panicked")??;

    assert_eq!(first.recipient, second.recipient);
    let reference: SecretReference = "age://shared-check".parse()?;
    let first_store = AgeSecretStore::new(identity.clone(), home.path().join("first.age"));
    first_store
        .create(
            &env_scope(),
            &reference,
            SecretMaterial::new("first-value".to_owned())?,
            10_000,
        )
        .await?;
    let second_store = AgeSecretStore::new(identity, home.path().join("second.age"));
    second_store
        .create(
            &env_scope(),
            &reference,
            SecretMaterial::new("second-value".to_owned())?,
            10_000,
        )
        .await?;
    assert_eq!(
        first_store.inspect(&env_scope(), &reference).await?.status,
        SecretStatus::Set
    );
    assert_eq!(
        second_store.inspect(&env_scope(), &reference).await?.status,
        SecretStatus::Set
    );
    Ok(())
}

#[tokio::test]
async fn age_secret_lifecycle_uses_create_only_and_revision_guards()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store = AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    );
    store.initialize()?;
    let reference: SecretReference = "age://lifecycle".parse()?;

    let initial = store
        .create(
            &env_scope(),
            &reference,
            SecretMaterial::new("initial-value".to_owned())?,
            1_000,
        )
        .await?;
    assert!(initial.changed);
    let duplicate = store
        .create(
            &env_scope(),
            &reference,
            SecretMaterial::new("must-not-overwrite".to_owned())?,
            2_000,
        )
        .await?;
    assert!(!duplicate.changed);
    assert_eq!(duplicate.revision, initial.revision);
    assert!(
        store
            .resolve(&env_scope(), &reference)
            .await?
            .material
            .is_some_and(|material| material.expose(|value| value == "initial-value"))
    );

    assert!(
        store
            .stage(
                &env_scope(),
                &reference,
                SecretMaterial::new("candidate-value".to_owned())?,
                expected_state(Some("stale-revision"), None),
                3_000,
            )
            .await
            .is_err()
    );
    let candidate = store
        .stage(
            &env_scope(),
            &reference,
            SecretMaterial::new("candidate-value".to_owned())?,
            expected_state(Some(&initial.revision), None),
            3_000,
        )
        .await?;
    assert_eq!(
        store
            .inspect(&env_scope(), &reference)
            .await?
            .candidate_revision,
        Some(candidate.clone())
    );
    assert!(
        store
            .replace(
                &env_scope(),
                &reference,
                SecretMaterial::new("must-not-discard-candidate".to_owned())?,
                expected_state(Some(&initial.revision), None),
                3_500,
            )
            .await
            .is_err()
    );
    assert_eq!(
        store
            .inspect(&env_scope(), &reference)
            .await?
            .candidate_revision,
        Some(candidate.clone())
    );
    let promoted = store
        .promote(
            &env_scope(),
            &reference,
            expected_state(Some(&initial.revision), Some(&candidate)),
            4_000,
        )
        .await?;
    assert_eq!(promoted, candidate);
    assert!(
        store
            .resolve(&env_scope(), &reference)
            .await?
            .material
            .is_some_and(|material| material.expose(|value| value == "candidate-value"))
    );

    assert!(
        store
            .replace(
                &env_scope(),
                &reference,
                SecretMaterial::new("replacement".to_owned())?,
                expected_state(Some(&initial.revision), None),
                5_000,
            )
            .await
            .is_err()
    );
    let replacement = store
        .replace(
            &env_scope(),
            &reference,
            SecretMaterial::new("replacement".to_owned())?,
            expected_state(Some(&promoted), None),
            5_000,
        )
        .await?;
    assert!(
        store
            .unset(
                &env_scope(),
                &reference,
                expected_state(Some(&promoted), None)
            )
            .await
            .is_err()
    );
    assert!(
        store
            .unset(
                &env_scope(),
                &reference,
                expected_state(Some(&replacement), None)
            )
            .await?
    );
    assert_eq!(
        store.inspect(&env_scope(), &reference).await?.status,
        SecretStatus::Unset
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn concurrent_secret_creation_publishes_exactly_one_value() -> Result<(), Box<dyn std::error::Error>>
{
    let home = temp_dirs::tempdir()?;
    let identity = home.path().join("identity.key");
    let encrypted_store = home.path().join("project.age");
    AgeSecretStore::new(identity.clone(), encrypted_store.clone()).initialize()?;
    let barrier = Arc::new(Barrier::new(2));
    let handles = ["first-value", "second-value"].map(|value| {
        let barrier = Arc::clone(&barrier);
        let identity = identity.clone();
        let encrypted_store = encrypted_store.clone();
        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let store = AgeSecretStore::new(identity, encrypted_store);
            let reference: SecretReference =
                "age://create-race".parse().map_err(|error: String| error)?;
            barrier.wait();
            runtime
                .block_on(store.create(
                    &env_scope(),
                    &reference,
                    SecretMaterial::new(value.to_owned())?,
                    10_000,
                ))
                .map_err(|error| error.to_string())
        })
    });
    let [first_handle, second_handle] = handles;
    let first = first_handle
        .join()
        .map_err(|_| "first creator panicked")??;
    let second = second_handle
        .join()
        .map_err(|_| "second creator panicked")??;
    assert_eq!(usize::from(first.changed) + usize::from(second.changed), 1);
    assert_eq!(first.revision, second.revision);
    Ok(())
}

#[tokio::test]
async fn inherits_top_level_bindings_and_compiles_url_casts()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir_all(project.path().join("api"))?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  api:\n    image: example/api\n",
    )?;
    fs::write(
        project.path().join("api/.env.example"),
        "DB_HOST= # mm:from=DB.HOST required\nAPP_URL= # mm:from=APP.URL required\nMAIL_URL= # mm:from=MAIL.URL required\n",
    )?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: inherited
default-environment: preview
target: local
stacks:
  app:
    files: [compose.yaml]
    ingress:
      - domain: web
        service: api
      - domain: mail
        service: api
        https: false
modules:
  api:
    path: api
    env:
      template: .env.example
    services:
      api:
        stack: app
bindings:
  DB.HOST:
    value: database
environments:
  preview:
    domains:
      web: app.preview.test
      mail: mail.preview.test
    bindings:
      APP.URL:
        cast: url
        value-ref: domains.web
      MAIL.URL:
        cast: url
        value-ref: domains.mail
"#,
    )?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    let secrets = support::secret_registry(age)?;
    let manager = environment_manager(
        state,
        values,
        secrets,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    let selection = WorkspaceSelection::new(project.path(), None);
    let bundle = manager.compile(&selection).await?;
    assert_eq!(
        std::str::from_utf8(bundle.services[0].bytes())?,
        "DB_HOST=database\nAPP_URL=https://app.preview.test\nMAIL_URL=http://mail.preview.test\n"
    );
    Ok(())
}

#[tokio::test]
async fn an_env_template_reads_the_release_being_deployed() -> Result<(), Box<dyn std::error::Error>>
{
    use mm::domain::release_facts::{ReleaseFacts, ReleaseRequest};
    let project = temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir_all(project.path().join("api"))?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  api:\n    image: example/api\n",
    )?;
    fs::write(
        project.path().join("api/.env.example"),
        "COMMIT=dev # mm:from=MM.COMMIT_SHORT\nVERSION=dev # mm:from=APP.VERSION\nBUILD=0 # mm:from=RELEASE.BUILD\nIMAGE= # mm:from=MM.IMAGE.api\n",
    )?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: released
default-environment: production
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    env:
      template: .env.example
    services:
      api:
        stack: app
release-inputs:
  BUILD: {pattern: "[0-9]+"}
bindings:
  APP.VERSION: {value-ref: MM.RELEASE}
environments:
  production: {}
"#,
    )?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let state: Arc<dyn StateStore> = sqlite.clone();
    let values: Arc<dyn EnvironmentValueStore> = sqlite;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    let secrets = support::secret_registry(age)?;
    let manager = environment_manager(
        state,
        values,
        secrets,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;

    let selection = WorkspaceSelection::new(project.path(), None);
    let bundle = manager.compile(&selection).await?;
    assert_eq!(
        std::str::from_utf8(bundle.services[0].bytes())?,
        "COMMIT=dev\nVERSION=dev\nBUILD=0\nIMAGE=\n"
    );
    let status = manager.status(&selection, false).await?;
    let commit = &status.modules[0].services[0].variables[0];
    assert_eq!(
        (commit.source, commit.status),
        (ValueSource::Release, ResolutionStatus::NotChecked)
    );
    assert!(
        manager
            .set_override(&selection, "MM.COMMIT_SHORT", "x", None)
            .is_err_and(|error| error.to_string().contains("--release-input")),
        "a release fact is not this machine's to override"
    );

    let commit = "0123456789abcdef0123456789abcdef01234567";
    let request = ReleaseRequest {
        name: Some("v3".to_owned()),
        inputs: BTreeMap::from([("BUILD".to_owned(), "42".to_owned())]),
    };
    let without_image = selection.with_release(Some(ReleaseFacts::new(
        Some(commit.to_owned()),
        request.clone(),
    )));
    let refused = manager.review(&without_image).await;
    assert!(
        matches!(&refused, Err(MmError::ReleaseInput { message }) if message.contains("MM.IMAGE.api")),
        "{refused:?}"
    );
    let without_input = selection.with_release(Some(ReleaseFacts::new(
        Some(commit.to_owned()),
        ReleaseRequest::default(),
    )));
    let refused = manager.review(&without_input).await;
    assert!(
        matches!(&refused, Err(MmError::ReleaseInput { message })
            if message.starts_with("environment 'production' reads release inputs the plan was not given: BUILD (")
                && message.contains(".env.example line 3)")
                && message.ends_with("Plan with --release-input BUILD=VALUE")),
        "{refused:?}"
    );

    let released = selection.with_release(Some(
        ReleaseFacts::new(Some(commit.to_owned()), request).with_images(BTreeMap::from([(
            "api".to_owned(),
            format!("registry.example.com/api@sha256:{}", "c".repeat(64)),
        )])),
    ));
    manager.review(&released).await?;
    let bundle = manager.compile(&released).await?;
    assert_eq!(
        std::str::from_utf8(bundle.services[0].bytes())?,
        format!(
            "COMMIT=0123456789ab\nVERSION=v3\nBUILD=42\nIMAGE=registry.example.com/api@sha256:{}\n",
            "c".repeat(64)
        )
    );
    Ok(())
}

#[tokio::test]
async fn review_includes_resolved_delivery_without_secret_values()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: delivery
  development-environment: workstation
default-environment: workstation
target: local
stacks:
  app:
    files: [compose.yaml]
environments:
  workstation:
    bindings: {}
  production:
    bindings: {}
"#,
    )?;
    let document = ConfigLoader.load_from(project.path())?;
    let workstation = resolve_environment_delivery(&document.config, "workstation");
    let production = resolve_environment_delivery(&document.config, "production");
    assert_eq!(workstation.mode, EnvironmentDeliveryMode::EnvFile);
    assert_eq!(production.mode, EnvironmentDeliveryMode::Direct);
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let manager = environment_manager(
        sqlite.clone(),
        sqlite,
        support::secret_registry(Arc::new(AgeSecretStore::new(
            home.path().join("identity.key"),
            home.path().join("project.age"),
        )))?,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    let review = manager
        .review(&WorkspaceSelection::new(
            project.path(),
            Some("production".to_owned()),
        ))
        .await?;
    assert_eq!(review.delivery.mode, EnvironmentDeliveryMode::Direct);
    assert!(!serde_json::to_string(&review)?.contains("TOP_SECRET"));
    Ok(())
}

#[tokio::test]
async fn manager_override_default_delivery_and_diff_contracts_are_explicit()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let secrets = support::secret_registry(age)?;
    secrets
        .create(
            &env_scope(),
            &"age://local/db-password".parse()?,
            SecretMaterial::new(SENTINEL.to_owned())?,
            10_000,
        )
        .await?;
    let manager = environment_manager(
        sqlite.clone(),
        sqlite,
        secrets,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    let selection = WorkspaceSelection::new(project.path(), None);

    assert_eq!(
        manager.default_environment(project.path())?.as_deref(),
        Some("local")
    );
    assert_eq!(manager.set_default_environment(project.path(), None)?, None);
    assert_eq!(
        manager
            .set_default_environment(project.path(), Some("local"))?
            .as_deref(),
        Some("local")
    );
    assert_eq!(
        manager.delivery(&selection)?.mode,
        EnvironmentDeliveryMode::EnvFile
    );
    assert_eq!(
        manager
            .set_delivery(project.path(), "local", EnvironmentDeliveryMode::Direct)?
            .mode,
        EnvironmentDeliveryMode::Direct
    );
    assert!(
        manager
            .set_override(&selection, "DB.HOST", "line\nbreak", None)
            .is_err()
    );
    assert!(
        manager
            .set_override(&selection, "UNKNOWN", "value", None)
            .is_err()
    );
    assert!(
        manager
            .set_override(&selection, "DB.PASSWORD", "value", None)
            .is_err()
    );
    let value = manager.set_override(&selection, "DB.HOST", "database.internal", None)?;
    assert_eq!(value.revision, 1);
    assert!(manager.unset_override(&selection, "DB.HOST")?);
    assert!(!manager.unset_override(&selection, "DB.HOST")?);

    let before = manager.diff(&selection, "api", None).await?;
    assert!(!before.exists && before.changed);
    let materialized = manager.materialize(&selection, "api", None, false).await?;
    let unchanged = manager.diff(&selection, "api", Some("api")).await?;
    assert!(unchanged.exists && !unchanged.changed);
    fs::write(&materialized.path, "DB_HOST=changed\n")?;
    assert!(manager.diff(&selection, "api", None).await?.changed);
    assert!(
        manager
            .materialize(&selection, "missing", None, false)
            .await
            .is_err()
    );
    assert!(
        manager
            .diff(&selection, "api", Some("missing"))
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn secret_status_maps_each_runtime_and_filters_requested_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        config_path,
        config.replace(
            "      api:\n        stack: app\n",
            "      api:\n        stack: app\n        secret-files:\n          database-password:\n            secret: database.password\n",
        ) + "targets:\n  production:\n    nodes:\n      - {name: app-1, host: 203.0.113.11}\n",
    )?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let manager = environment_manager(
        sqlite.clone(),
        sqlite,
        support::secret_registry(Arc::new(AgeSecretStore::new(
            home.path().join("identity.key"),
            home.path().join("project.age"),
        )))?,
        Arc::new(FixedClock { unix_ms: 10_000 }),
    )?;
    // Rootful Podman on a remote host, where `mmd` knows each service's
    // user ids, takes a file per revision given to the container's user.
    let user_ids: Arc<dyn mm::app::ports::ServiceUserIds> =
        Arc::new(mm::adapters::user_namespace::FilesystemServiceUserIds::at(
            home.path().join("service-user-ids.json"),
            home.path().join("subuid"),
            home.path().join("subgid"),
        ));
    // On a workstation, an environment of hosts setup prepared names what
    // those hosts use.
    for (runtime, user_ids, target, delivery) in [
        (
            RuntimeKind::PodmanCompose,
            None,
            None,
            "podman-native-secret",
        ),
        (
            RuntimeKind::PodmanCompose,
            Some(user_ids),
            None,
            "compose-file-secret",
        ),
        (
            RuntimeKind::PodmanCompose,
            None,
            Some("production"),
            "compose-file-secret",
        ),
        (
            RuntimeKind::DockerCompose,
            None,
            None,
            "compose-file-secret",
        ),
    ] {
        let mut selection = WorkspaceSelection::new(project.path(), None);
        selection.runtime = Some(runtime);
        selection.user_ids = user_ids;
        selection.target = target.map(str::to_owned);
        let status = manager
            .secret_status(&selection, Some("database.password"))
            .await?;
        assert_eq!(status.secrets.len(), 1);
        assert_eq!(status.secrets[0].key, "database.password");
        assert_eq!(status.runtime_delivery.len(), 1);
        assert_eq!(status.runtime_delivery[0].delivery, delivery);
    }
    assert!(
        manager
            .secret_status(
                &WorkspaceSelection::new(project.path(), None),
                Some("missing")
            )
            .await
            .is_err()
    );
    let unchecked = manager
        .status(&WorkspaceSelection::new(project.path(), None), false)
        .await?;
    assert!(
        unchecked.modules[0].services[0]
            .variables
            .iter()
            .any(|variable| {
                variable.name == "DB_PASSWORD" && variable.status == ResolutionStatus::NotChecked
            })
    );
    Ok(())
}

enum InspectionOutcome {
    Invalid,
    Failure,
}

struct ControlledInspector(InspectionOutcome);

#[async_trait::async_trait]
impl SecretMetadataInspector for ControlledInspector {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        match self.0 {
            InspectionOutcome::Invalid => Ok(SecretMetadata {
                status: SecretStatus::Invalid,
                revision: None,
                created_at_unix_ms: None,
                candidate_revision: None,
            }),
            InspectionOutcome::Failure => Err(MmError::Secret {
                message: "injected provider inspection failure".to_owned(),
            }),
        }
    }
}

struct FixedStorageCapability(StorageCapabilityDescriptor);

impl SecretStorageCapabilityCatalog for FixedStorageCapability {
    fn storage_capabilities(&self) -> Vec<StorageCapabilityDescriptor> {
        vec![self.0.clone()]
    }

    fn storage_capability(
        &self,
        provider: &StorageProviderId,
    ) -> Result<StorageCapabilityDescriptor, MmError> {
        if self.0.id == *provider {
            Ok(self.0.clone())
        } else {
            Err(MmError::Secret {
                message: format!("test storage provider '{provider}' is unavailable"),
            })
        }
    }
}

#[tokio::test]
async fn validation_and_provider_failures_preserve_secret_inspection_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let valid_config = fs::read_to_string(&config_path)?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let registry = support::secret_registry(Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    )))?;
    let selection = WorkspaceSelection::new(project.path(), None);

    fs::write(
        &config_path,
        valid_config.replace("name: minimal", "name: Invalid Name"),
    )?;
    let unchecked = environment_manager_with_inspector(
        sqlite.clone(),
        registry.clone(),
        Arc::new(ControlledInspector(InspectionOutcome::Failure)),
    )?
    .secret_status(&selection, Some("database.password"))
    .await?;
    assert_eq!(unchecked.secrets[0].status, SecretStatus::NotChecked);

    fs::write(&config_path, valid_config)?;
    let unavailable = environment_manager_with_inspector(
        sqlite.clone(),
        registry.clone(),
        Arc::new(ControlledInspector(InspectionOutcome::Failure)),
    )?;
    let status = unavailable.status(&selection, true).await?;
    assert!(
        status
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_SECRET_PROVIDER_UNAVAILABLE")
    );
    let secret_status = unavailable
        .secret_status(&selection, Some("database.password"))
        .await?;
    assert_eq!(
        secret_status.secrets[0].status,
        SecretStatus::ProviderUnavailable
    );

    let invalid = environment_manager_with_inspector(
        sqlite,
        registry,
        Arc::new(ControlledInspector(InspectionOutcome::Invalid)),
    )?;
    assert!(invalid.review(&selection).await.is_err());
    Ok(())
}

#[tokio::test]
async fn runtime_delivery_rejects_backend_resolution_and_location_capability_mismatches()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let valid_config = fs::read_to_string(&config_path)?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let registry = support::secret_registry(Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    )))?;
    let provider = ConfigLoader.load_from(project.path())?.config.secrets["database.password"]
        .storage
        .provider
        .clone();
    let selection = WorkspaceSelection::new(project.path(), None);
    let revisions = BTreeMap::from([(
        "age/local/db-password".to_owned(),
        Some("revision-1".to_owned()),
    )]);

    let mut short_location = registry.storage_capability(&provider)?;
    short_location.max_location_bytes = 1;
    let manager = environment_manager_with_storage(
        sqlite.clone(),
        registry.clone(),
        Arc::new(FixedStorageCapability(short_location)),
    )?;
    assert!(
        manager
            .validate_secret_delivery(&selection, RuntimeKind::DockerCompose, &revisions)
            .is_err()
    );

    fs::write(
        &config_path,
        valid_config
            .replace(
                "      api:\n        stack: app\n",
                "      api:\n        stack: app\n        secret-files:\n          database-password:\n            secret: database.password\n",
            )
            .replace(
                "      DB.PASSWORD:\n        secret: database.password",
                "      DB.PASSWORD:\n        value: configured",
            ),
    )?;
    let mut no_resolution = registry.storage_capability(&provider)?;
    no_resolution
        .operations
        .retain(|operation| *operation != StorageOperation::ResolveMaterial);
    let manager = environment_manager_with_storage(
        sqlite,
        registry,
        Arc::new(FixedStorageCapability(no_resolution)),
    )?;
    assert!(
        manager
            .validate_secret_delivery(&selection, RuntimeKind::DockerCompose, &revisions)
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn runtime_secret_plans_bind_revisions_consumers_and_delivery_capabilities()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace(
            "      api:\n        stack: app\n",
            "      api:\n        stack: app\n        secret-files:\n          database-password:\n            secret: database.password\n",
        ),
    )?;
    let home = temp_dirs::tempdir()?;
    let sqlite = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let registry = support::secret_registry(Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    )))?;
    let manager = environment_manager_with_delivery(
        sqlite.clone(),
        sqlite,
        registry,
        Arc::new(RuntimeDeliveryCatalog::new()?),
        Arc::new(FixedClock { unix_ms: 10_000 }),
    );
    let selection = WorkspaceSelection::new(project.path(), None);
    let revisions = BTreeMap::from([(
        "age/local/db-password".to_owned(),
        Some("revision-1".to_owned()),
    )]);
    for runtime in RuntimeKind::ALL {
        let items = manager.validate_secret_delivery(&selection, runtime, &revisions)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].logical_name, "database.password");
        assert_eq!(items[0].source_revision, "revision-1");
        assert_eq!(items[0].consumers[0].target, "database-password");
    }
    assert!(
        manager
            .validate_secret_delivery(&selection, RuntimeKind::DockerCompose, &BTreeMap::new())
            .is_err()
    );
    Ok(())
}

struct RuntimeDeliveryCatalog(Vec<DeliveryCapabilityDescriptor>);

impl RuntimeDeliveryCatalog {
    fn new() -> Result<Self, MmError> {
        let environment = FilesystemEnvironmentArtifactStore::new(PathBuf::from(
            "/tmp/mm-test-secret-delivery-unused",
        ))?;
        let mut descriptors = environment.delivery_capabilities();
        descriptors.extend([
            podman_native_secret_descriptor()?,
            compose_file_secret_descriptor()?,
        ]);
        Ok(Self(descriptors))
    }
}

impl SecretDeliveryCapabilityCatalog for RuntimeDeliveryCatalog {
    fn delivery_capabilities(&self) -> Vec<DeliveryCapabilityDescriptor> {
        self.0.clone()
    }

    fn delivery_capability(
        &self,
        driver: &DeliveryDriverId,
    ) -> Result<DeliveryCapabilityDescriptor, MmError> {
        self.0
            .iter()
            .find(|descriptor| descriptor.id == *driver)
            .cloned()
            .ok_or_else(|| MmError::Secret {
                message: format!("test delivery driver '{driver}' is unavailable"),
            })
    }
}

fn env_scope() -> SecretScope {
    SecretScope::new("local")
}

fn environment_manager(
    state: Arc<dyn StateStore>,
    values: Arc<dyn EnvironmentValueStore>,
    secrets: Arc<SecretProviderRegistry>,
    clock: Arc<dyn mm::app::ports::Clock>,
) -> Result<EnvironmentManager, mm::domain::error::MmError> {
    let delivery = Arc::new(FilesystemEnvironmentArtifactStore::new(PathBuf::from(
        "/tmp/mm-test-secret-delivery-unused",
    ))?);
    let delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog> = delivery;
    Ok(environment_manager_with_delivery(
        state,
        values,
        secrets,
        delivery_capabilities,
        clock,
    ))
}

fn environment_manager_with_inspector(
    state: Arc<SqliteStateStore>,
    secrets: Arc<SecretProviderRegistry>,
    inspector: Arc<dyn SecretMetadataInspector>,
) -> Result<EnvironmentManager, MmError> {
    let delivery = Arc::new(FilesystemEnvironmentArtifactStore::new(PathBuf::from(
        "/tmp/mm-test-secret-delivery-unused",
    ))?);
    let resolver: Arc<dyn SecretMaterialResolver> = secrets.clone();
    let storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog> = secrets;
    Ok(EnvironmentManager::new(EnvironmentDependencies {
        documents: Arc::new(ConfigLoader),
        sources: Arc::new(EnvironmentIndexer),
        writer: Arc::new(FilesystemMaterializedEnvironmentWriter),
        state: state.clone(),
        values: state,
        inspector,
        resolver,
        storage_capabilities,
        delivery_capabilities: delivery,
        clock: Arc::new(FixedClock { unix_ms: 10_000 }),
    }))
}

fn environment_manager_with_storage(
    state: Arc<SqliteStateStore>,
    secrets: Arc<SecretProviderRegistry>,
    storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog>,
) -> Result<EnvironmentManager, MmError> {
    let inspector: Arc<dyn SecretMetadataInspector> = secrets.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = secrets;
    Ok(EnvironmentManager::new(EnvironmentDependencies {
        documents: Arc::new(ConfigLoader),
        sources: Arc::new(EnvironmentIndexer),
        writer: Arc::new(FilesystemMaterializedEnvironmentWriter),
        state: state.clone(),
        values: state,
        inspector,
        resolver,
        storage_capabilities,
        delivery_capabilities: Arc::new(RuntimeDeliveryCatalog::new()?),
        clock: Arc::new(FixedClock { unix_ms: 10_000 }),
    }))
}

fn environment_manager_with_delivery(
    state: Arc<dyn StateStore>,
    values: Arc<dyn EnvironmentValueStore>,
    secrets: Arc<SecretProviderRegistry>,
    delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog>,
    clock: Arc<dyn mm::app::ports::Clock>,
) -> EnvironmentManager {
    let inspector: Arc<dyn SecretMetadataInspector> = secrets.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = secrets.clone();
    let storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog> = secrets;
    EnvironmentManager::new(EnvironmentDependencies {
        documents: Arc::new(ConfigLoader),
        sources: Arc::new(EnvironmentIndexer),
        writer: Arc::new(FilesystemMaterializedEnvironmentWriter),
        state,
        values,
        inspector,
        resolver,
        storage_capabilities,
        delivery_capabilities,
        clock,
    })
}

fn copy_fixture(destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = fixture_root();
    fs::create_dir_all(destination.join(".config"))?;
    fs::create_dir_all(destination.join("api"))?;
    fs::copy(
        source.join(".config/mm.yaml"),
        destination.join(".config/mm.yaml"),
    )?;
    fs::copy(
        source.join("compose.yaml"),
        destination.join("compose.yaml"),
    )?;
    fs::copy(
        source.join("api/.env.example"),
        destination.join("api/.env.example"),
    )?;
    Ok(())
}

fn regular_files(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

fn expected_state(active: Option<&str>, candidate: Option<&str>) -> SecretExpectedState {
    SecretExpectedState {
        active_revision: active.map(str::to_owned),
        candidate_revision: candidate.map(str::to_owned),
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
}

fn service_override_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/service-overrides")
}
