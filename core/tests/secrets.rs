#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::FilesystemEnvironmentArtifactStore;
use mm::adapters::secrets::{AgeSecretStore, SecretProviderRegistry};
use mm::app::ports::{
    Clock, SecretBatchReplacer, SecretCreator, SecretDeliveryCapabilityCatalog,
    SecretMaterialResolver, SecretMetadataInspector, SecretRemover, SecretReplacer,
    SecretStorageCapabilityCatalog,
};
use mm::app::{SecretManager, SecretManagerDependencies};
use mm::domain::secret::{
    SecretExpectedState, SecretGenerationSelection, SecretGenerationStatus, SecretMaterial,
    SecretReference, SecretReplacement, SecretRotationPlanStatus, SecretScope,
};

mod temp_dirs;

#[allow(clippy::duplicate_mod)]
mod support;

const DAY_MILLISECONDS: i64 = 24 * 60 * 60 * 1_000;

#[tokio::test]
async fn scheduled_rotation_requires_the_current_reviewed_plan()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "      length: 32\n",
        "      length: 32\n    rotation:\n      interval-days: 1\n",
    );
    fs::write(config_path, config)?;

    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let initial_time = 1_000;
    let initial = secret_manager(
        Arc::clone(&router),
        Arc::new(FixedClock {
            unix_ms: initial_time,
        }),
    )?;

    let generated = initial
        .generate(project.path(), None, &named("database.password"))
        .await?;
    assert_eq!(generated.secrets.len(), 1);
    assert_eq!(
        generated.secrets[0].status,
        SecretGenerationStatus::Generated
    );
    let original_revision = generated.secrets[0]
        .revision
        .clone()
        .ok_or("a generated secret has a revision")?;
    let early_plan = initial.rotation_plan(project.path(), None).await?;
    assert_eq!(
        early_plan.secrets[0].status,
        SecretRotationPlanStatus::NotDue
    );

    let due = secret_manager(
        Arc::clone(&router),
        Arc::new(FixedClock {
            unix_ms: initial_time + DAY_MILLISECONDS,
        }),
    )?;
    let due_plan = due.rotation_plan(project.path(), None).await?;
    assert_eq!(due_plan.secrets[0].status, SecretRotationPlanStatus::Ready);
    assert_ne!(early_plan.plan_hash, due_plan.plan_hash);
    assert!(
        due.apply_rotation(project.path(), None, &early_plan.plan_hash)
            .await
            .is_err()
    );

    let applied = due
        .apply_rotation(project.path(), None, &due_plan.plan_hash)
        .await?;
    assert_eq!(applied.rotated.len(), 1);
    assert_ne!(applied.rotated[0].revision, original_revision);
    assert!(
        due.apply_rotation(project.path(), None, &due_plan.plan_hash)
            .await
            .is_err()
    );

    let reference: SecretReference = "age://local/db-password".parse()?;
    let metadata = router.inspect(&scope(), &reference).await?;
    assert_eq!(metadata.revision, Some(applied.rotated[0].revision.clone()));
    assert_eq!(
        metadata.created_at_unix_ms,
        Some(initial_time + DAY_MILLISECONDS)
    );
    Ok(())
}

#[tokio::test]
async fn replacement_batches_validate_every_revision_before_publishing()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store = AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    );
    store.initialize()?;
    let first: SecretReference = "age://first".parse()?;
    let second: SecretReference = "age://second".parse()?;
    let first_created = store
        .create(
            &scope(),
            &first,
            SecretMaterial::new("first".to_owned())?,
            1_000,
        )
        .await?;
    let second_created = store
        .create(
            &scope(),
            &second,
            SecretMaterial::new("second".to_owned())?,
            1_000,
        )
        .await?;

    let failed = store
        .replace_batch(
            vec![
                SecretReplacement {
                    scope: scope(),
                    reference: first.clone(),
                    material: SecretMaterial::new("new-first".to_owned())?,
                    expected: expected_state(&first_created.revision),
                },
                SecretReplacement {
                    scope: scope(),
                    reference: second.clone(),
                    material: SecretMaterial::new("new-second".to_owned())?,
                    expected: expected_state("stale-revision"),
                },
            ],
            2_000,
        )
        .await;
    assert!(failed.is_err());
    assert_eq!(
        store.inspect(&scope(), &first).await?.revision,
        Some(first_created.revision.clone())
    );
    assert_eq!(
        store.inspect(&scope(), &second).await?.revision,
        Some(second_created.revision.clone())
    );

    let revisions = store
        .replace_batch(
            vec![
                SecretReplacement {
                    scope: scope(),
                    reference: first.clone(),
                    material: SecretMaterial::new("new-first".to_owned())?,
                    expected: expected_state(&first_created.revision),
                },
                SecretReplacement {
                    scope: scope(),
                    reference: second.clone(),
                    material: SecretMaterial::new("new-second".to_owned())?,
                    expected: expected_state(&second_created.revision),
                },
            ],
            2_000,
        )
        .await?;
    assert_eq!(revisions.len(), 2);
    assert_eq!(
        store.inspect(&scope(), &first).await?.revision,
        Some(revisions[0].clone())
    );
    assert_eq!(
        store.inspect(&scope(), &second).await?.revision,
        Some(revisions[1].clone())
    );
    Ok(())
}

#[tokio::test]
async fn mixed_deployment_coupled_rotation_plan_fails_before_writing()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "      length: 32\n",
            "      length: 32\n    rotation:\n      interval-days: 1\n",
        )
        .replace(
            "environments:\n",
            "  deployment.token:\n    storage:\n      provider: age\n      location: local/deployment-token\n    generate:\n      preset: token\n      length: 32\n    rotation:\n      strategy: coordinated\n      interval-days: 1\nenvironments:\n",
        );
    fs::write(config_path, config)?;

    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let initial = secret_manager(Arc::clone(&router), Arc::new(FixedClock { unix_ms: 1_000 }))?;
    initial
        .generate(project.path(), None, &SecretGenerationSelection::All)
        .await?;
    let reference: SecretReference = "age://local/db-password".parse()?;
    let original_revision = router.inspect(&scope(), &reference).await?.revision;

    let due = secret_manager(
        Arc::clone(&router),
        Arc::new(FixedClock {
            unix_ms: 1_000 + DAY_MILLISECONDS,
        }),
    )?;
    let plan = due.rotation_plan(project.path(), None).await?;
    assert!(
        plan.secrets
            .iter()
            .any(|item| item.status == SecretRotationPlanStatus::Ready)
    );
    assert!(
        plan.secrets
            .iter()
            .any(|item| item.status == SecretRotationPlanStatus::UnsupportedStrategy)
    );
    assert!(
        due.apply_rotation(project.path(), None, &plan.plan_hash)
            .await
            .is_err()
    );
    assert_eq!(
        router.inspect(&scope(), &reference).await?.revision,
        original_revision
    );
    Ok(())
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

#[tokio::test]
async fn environment_scopes_isolate_secret_values() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store = AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    );
    store.initialize()?;
    let reference: SecretReference = "age://db-password".parse()?;
    let integration = SecretScope::new("integration");
    let uat = SecretScope::new("uat");
    store
        .create(
            &integration,
            &reference,
            SecretMaterial::new("integration-secret".to_owned())?,
            1_000,
        )
        .await?;
    store
        .create(
            &uat,
            &reference,
            SecretMaterial::new("uat-secret".to_owned())?,
            1_000,
        )
        .await?;

    assert!(
        store
            .resolve(&integration, &reference)
            .await?
            .material
            .is_some_and(|material| material.expose(|value| value == "integration-secret"))
    );
    assert!(
        store
            .resolve(&uat, &reference)
            .await?
            .material
            .is_some_and(|material| material.expose(|value| value == "uat-secret"))
    );
    assert!(
        store
            .unset(
                &integration,
                &reference,
                SecretExpectedState {
                    active_revision: store.inspect(&uat, &reference).await?.revision,
                    candidate_revision: None,
                },
            )
            .await
            .is_err()
    );
    assert_eq!(
        store.inspect(&uat, &reference).await?.status,
        mm::domain::secret::SecretStatus::Set
    );
    Ok(())
}

#[tokio::test]
async fn generate_rejects_operator_supplied_secrets() -> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "  database.password:\n    storage:\n      provider: age\n      location: local/db-password\n    generate:\n      preset: password\n      length: 32\n",
        "  stripe.secret:\n    storage:\n      provider: age\n      location: stripe-secret\n    input:\n      prompt: Stripe secret key\n",
    );
    fs::write(config_path, config)?;
    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let manager = secret_manager(router, Arc::new(FixedClock { unix_ms: 1_000 }))?;
    let error = manager
        .generate(project.path(), None, &named("stripe.secret"))
        .await
        .err()
        .ok_or("operator-supplied secrets cannot be generated")?;
    assert!(error.to_string().contains("operator-supplied"));
    Ok(())
}

#[tokio::test]
async fn operator_secret_prompt_set_rotate_and_unset_are_reviewed()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "    generate:\n      preset: password\n      length: 32\n",
        "    input:\n      prompt: Database password\n",
    );
    fs::write(config_path, config)?;

    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let manager = secret_manager(router.clone(), Arc::new(FixedClock { unix_ms: 1_000 }))?;

    assert_eq!(
        manager.input_prompt(project.path(), "database.password")?,
        "Database password"
    );
    assert_eq!(
        manager.rotation_prompt(project.path(), "database.password")?,
        Some("Database password".to_owned())
    );
    assert!(format!("{manager:?}").starts_with("SecretManager"));
    assert!(!manager.capabilities().storage.is_empty());
    assert!(!manager.capabilities().delivery.is_empty());

    let created = manager
        .set(
            project.path(),
            None,
            "database.password",
            SecretMaterial::new("first-value".to_owned())?,
        )
        .await?;
    let replaced = manager
        .set(
            project.path(),
            None,
            "database.password",
            SecretMaterial::new("second-value".to_owned())?,
        )
        .await?;
    assert_ne!(created.revision, replaced.revision);

    let plan = manager
        .plan_direct_rotation(project.path(), None, "database.password")
        .await?;
    assert!(plan.requires_hidden_input);
    assert!(
        manager
            .apply_direct_rotation(
                project.path(),
                None,
                "database.password",
                "stale-plan",
                Some(SecretMaterial::new("third-value".to_owned())?),
            )
            .await
            .is_err()
    );
    let rotated = manager
        .apply_direct_rotation(
            project.path(),
            None,
            "database.password",
            &plan.plan_hash,
            Some(SecretMaterial::new("third-value".to_owned())?),
        )
        .await?;
    assert_ne!(rotated.revision, replaced.revision);

    assert_eq!(
        manager
            .unset(project.path(), None, "database.password")
            .await?,
        ("age".to_owned(), true)
    );
    assert_eq!(
        manager
            .unset(project.path(), None, "database.password")
            .await?,
        ("age".to_owned(), false)
    );
    assert!(
        manager
            .rotate(project.path(), None, "database.password", None)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn generated_secret_direct_rotation_rejects_input_and_regenerates()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let manager = secret_manager(router, Arc::new(FixedClock { unix_ms: 1_000 }))?;

    assert_eq!(
        manager.input_prompt(project.path(), "database.password")?,
        "Secret value"
    );
    assert_eq!(
        manager.rotation_prompt(project.path(), "database.password")?,
        None
    );
    assert!(manager.input_prompt(project.path(), "missing").is_err());

    let generated = manager
        .generate(project.path(), None, &named("database.password"))
        .await?;
    assert_eq!(
        generated.secrets[0].status,
        SecretGenerationStatus::Generated
    );
    let repeated = manager
        .generate(project.path(), None, &SecretGenerationSelection::Missing)
        .await?;
    assert_eq!(
        repeated.secrets[0].status,
        SecretGenerationStatus::AlreadySet
    );

    let plan = manager
        .plan_direct_rotation(project.path(), None, "database.password")
        .await?;
    assert!(!plan.requires_hidden_input);
    assert!(
        manager
            .apply_direct_rotation(
                project.path(),
                None,
                "database.password",
                &plan.plan_hash,
                Some(SecretMaterial::new("operator-value".to_owned())?),
            )
            .await
            .is_err()
    );
    let rotated = manager
        .apply_direct_rotation(
            project.path(),
            None,
            "database.password",
            &plan.plan_hash,
            None,
        )
        .await?;
    assert_ne!(Some(rotated.revision), generated.secrets[0].revision);
    Ok(())
}

#[tokio::test]
async fn declaration_sources_prompts_and_rotation_strategy_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let original = fs::read_to_string(&config_path)?;
    let generator = "    generate:\n      preset: password\n      length: 32\n";

    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let manager = secret_manager(router, Arc::new(FixedClock { unix_ms: 1_000 }))?;

    fs::write(
        &config_path,
        original.replace(
            generator,
            &format!("{generator}    input:\n      prompt: Database password\n"),
        ),
    )?;
    assert!(
        manager
            .input_prompt(project.path(), "database.password")
            .is_err()
    );
    assert!(
        manager
            .generate(project.path(), None, &named("database.password"))
            .await
            .is_err()
    );

    fs::write(&config_path, original.replace(generator, ""))?;
    assert_eq!(
        manager.input_prompt(project.path(), "database.password")?,
        "Secret value"
    );
    assert_eq!(
        manager.rotation_prompt(project.path(), "database.password")?,
        Some("Secret value".to_owned())
    );

    fs::write(
        &config_path,
        original.replace(generator, "    input:\n      prompt: \"\"\n"),
    )?;
    assert!(
        manager
            .input_prompt(project.path(), "database.password")
            .is_err()
    );
    assert!(
        manager
            .rotation_prompt(project.path(), "database.password")
            .is_err()
    );

    fs::write(
        &config_path,
        original.replace(
            generator,
            &format!("{generator}    rotation:\n      strategy: replace-and-redeploy\n"),
        ),
    )?;
    assert!(
        manager
            .rotation_prompt(project.path(), "database.password")
            .is_err()
    );
    assert!(
        manager
            .plan_direct_rotation(project.path(), None, "database.password")
            .await
            .is_err()
    );
    assert!(
        manager
            .rotate(project.path(), None, "database.password", None)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn read_only_storage_rejects_mutation_and_scheduled_rotation()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let original = fs::read_to_string(&config_path)?;
    let environment_storage = original
        .replace("provider: age", "provider: env")
        .replace("location: local/db-password", "location: DATABASE_PASSWORD")
        .replace(
            "    generate:\n      preset: password\n      length: 32\n",
            "    input:\n      prompt: Database password\n",
        );

    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let router = support::secret_registry(age)?;
    let manager = secret_manager(router, Arc::new(FixedClock { unix_ms: 1_000 }))?;

    fs::write(&config_path, &environment_storage)?;
    assert!(
        manager
            .input_prompt(project.path(), "database.password")
            .is_err()
    );
    let plan = manager.rotation_plan(project.path(), None).await?;
    assert_eq!(
        plan.secrets[0].status,
        SecretRotationPlanStatus::NotScheduled
    );
    assert!(
        manager
            .apply_rotation(project.path(), None, &plan.plan_hash)
            .await
            .is_err()
    );

    fs::write(
        &config_path,
        environment_storage.replace(
            "    input:\n      prompt: Database password\n",
            "    input:\n      prompt: Database password\n    rotation:\n      interval-days: 1\n",
        ),
    )?;
    assert!(manager.rotation_plan(project.path(), None).await.is_err());
    Ok(())
}

fn named(name: &str) -> SecretGenerationSelection {
    SecretGenerationSelection::Named(name.to_owned())
}

fn scope() -> SecretScope {
    SecretScope::new("local")
}

fn expected_state(active_revision: &str) -> SecretExpectedState {
    SecretExpectedState {
        active_revision: Some(active_revision.to_owned()),
        candidate_revision: None,
    }
}

fn secret_manager(
    registry: Arc<SecretProviderRegistry>,
    clock: Arc<dyn Clock>,
) -> Result<SecretManager, mm::domain::error::MmError> {
    let inspector: Arc<dyn SecretMetadataInspector> = registry.clone();
    let creator: Arc<dyn SecretCreator> = registry.clone();
    let replacer: Arc<dyn SecretReplacer> = registry.clone();
    let batch_replacer: Arc<dyn SecretBatchReplacer> = registry.clone();
    let remover: Arc<dyn SecretRemover> = registry.clone();
    let storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog> = registry;
    let delivery = Arc::new(FilesystemEnvironmentArtifactStore::new(PathBuf::from(
        "/tmp/mm-test-secret-manager-unused",
    ))?);
    let delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog> = delivery;
    Ok(SecretManager::new(SecretManagerDependencies {
        documents: Arc::new(ConfigLoader),
        inspector,
        creator,
        replacer,
        batch_replacer,
        remover,
        storage_capabilities,
        delivery_capabilities,
        clock,
    }))
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
}
