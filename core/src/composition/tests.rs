use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::adapters::clock::FixedClock;
use crate::app::ports::GateIngressPublisher as _;
use crate::domain::target::{NamedTarget, TargetDefinition, TargetId};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock {
        unix_ms: 1_700_000_000_000,
    })
}

fn local_home(root: &Path) -> Result<MmHome, MmError> {
    let home = MmHome::resolve(Some(root))?;
    FilesystemTargetStore::new(home.targets_file.clone()).insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(44))),
    )?;
    Ok(home)
}

#[tokio::test]
async fn installation_dashboard_starts_without_a_project_checkout()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = crate::test_support::tempdir()?;
    let home_path = directory.path().join("home");
    local_home(&home_path)?;
    let empty_root = directory.path().join("empty");
    std::fs::create_dir(&empty_root)?;
    let services = RuntimeComposition::new();
    let (dashboard, _, _, _) = dashboard_services(
        &empty_root,
        Some(&home_path),
        RuntimeKind::DockerCompose,
        &services,
    )?;
    dashboard
        .query(crate::domain::dashboard::DashboardScope::Installation)
        .await?;
    assert!(!empty_root.join(".config/mm.yaml").exists());
    Ok(())
}

#[tokio::test]
async fn global_gate_secret_router_rejects_unscoped_or_unregistered_references()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::secret::{SecretReference, SecretScope};
    let directory = crate::test_support::tempdir()?;
    let home = local_home(directory.path())?;
    let state = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let secrets = super::dashboard_secrets::DashboardGateSecrets::new(home, state);
    for locator in [
        "other/password",
        "gate/not-a-uuid/dev/policy/key",
        "gate/019c527b-6242-7b43-bfd4-bd60e27c1de3/dev/policy/key",
    ] {
        let reference = SecretReference::new(
            crate::domain::secret_contract::StorageProviderId::new("age")?,
            locator,
        )?;
        assert!(matches!(
            secrets.inspect(&SecretScope::new("dev"), &reference).await,
            Err(MmError::Security { .. })
        ));
    }
    Ok(())
}

#[test]
fn application_and_provider_helpers_assemble_local_adapters()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let home_path = temp.path().join("home");
    let home = local_home(&home_path)?;
    let services = RuntimeComposition::new();
    let runtime = services.driver(RuntimeKind::PodmanCompose);

    let _documents = project_documents();
    let _onboarding = onboarding_service();
    let _sources = environment_sources();
    let _writer = materialized_environment_writer();
    let _persistent = persistent_application(Some(&home_path), runtime.clone(), clock())?;
    let _inspection = inspection_application(Some(&home_path), runtime.clone(), clock())?;
    let _stacks = stack_manager(Some(&home_path))?;
    let _mounts = development_mount_manager(Some(&home_path), runtime)?;
    let (document, _state) = preference_state(&fixture(), Some(&home_path))?;
    assert_eq!(document.config.project.name, "minimal");
    assert_eq!(provider_driver_descriptors()?.len(), 2);
    assert_eq!(
        identity_path(&home),
        home.identities_directory.join("age.key")
    );
    let _update = update_check_service()?;
    Ok(())
}

#[test]
fn environment_and_gate_composition_initialize_selected_backends()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let home_path = temp.path().join("home");
    local_home(&home_path)?;

    let environment = environment_services(&fixture(), Some(&home_path), clock())?;
    let initialized = environment.initialize_age()?;
    assert!(initialized.identity_path.is_file());
    let _manager = environment.manager;
    let _secrets = environment.secrets;

    let gate = gate_services(&fixture(), Some(&home_path), clock())?;
    gate.initialize_selected_backend("environment")?;
    gate.initialize_selected_backend("age")?;
    gate.initialize_selected_backend("age")?;
    let _manager = gate.manager;
    Ok(())
}

#[tokio::test]
async fn runtime_and_service_composition_constructs_execution_services()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let home_path = temp.path().join("home");
    local_home(&home_path)?;
    let services = RuntimeComposition::new();
    assert!(Arc::strong_count(&services.process()) >= 1);
    assert!(services.clock().now_unix_ms() > 0);

    let selection = WorkspaceSelection::new(fixture(), Some("local".to_owned()));
    let planner = services.build_manager();
    let _build = services.build_service(planner, &selection, Some(&home_path))?;
    let _deployment = services.deployment_service(&selection, Some(&home_path))?;
    let _operations = runtime_operations_service(
        &fixture(),
        Some(&home_path),
        RuntimeKind::PodmanCompose,
        &services,
    )?;
    let _scheduler = job_scheduler_service(
        &fixture(),
        Some(&home_path),
        RuntimeKind::PodmanCompose,
        &services,
    )?;
    let _service_stack =
        service_stack_manager(Some(&home_path), RuntimeKind::PodmanCompose, &services)?;
    let (_dashboard, _events, token, _gate) = dashboard_services(
        &fixture(),
        Some(&home_path),
        RuntimeKind::PodmanCompose,
        &services,
    )?;
    assert!(!token.is_empty());
    Ok(())
}

#[test]
fn config_composition_resolves_explicit_default_and_persisted_runtime_contexts()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let home_path = temp.path().join("home");
    local_home(&home_path)?;

    assert_eq!(
        resolve_runtime(
            temp.path(),
            Some(&home_path),
            Some(RuntimeKind::DockerCompose),
            None,
            None,
            false,
            true,
        )?,
        RuntimeKind::DockerCompose
    );
    assert_eq!(
        resolve_runtime(
            &temp.path().join("missing"),
            Some(&home_path),
            None,
            None,
            None,
            true,
            false,
        )?,
        RuntimeKind::PodmanCompose
    );
    assert!(
        resolve_runtime(
            &temp.path().join("missing"),
            Some(&home_path),
            None,
            None,
            None,
            false,
            false,
        )
        .is_err()
    );
    assert_eq!(
        resolve_runtime(&fixture(), Some(&home_path), None, None, None, false, false,)?,
        RuntimeKind::PodmanCompose
    );
    assert_eq!(
        resolve_runtime(
            &fixture(),
            Some(&home_path),
            None,
            Some("local"),
            None,
            false,
            true,
        )?,
        RuntimeKind::PodmanCompose
    );

    assert!(schema().get("$comment").is_some());
    let capabilities = secret_capabilities()?;
    assert_eq!(capabilities.storage.len(), 2);
    assert_eq!(capabilities.delivery.len(), 4);
    assert_eq!(
        discover_project(&fixture())?,
        fixture().join(crate::adapters::config::CONFIG_RELATIVE_PATH)
    );
    Ok(())
}

#[test]
fn target_composition_constructs_managers_and_rejects_unattributed_privileged_calls()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let home_path = temp.path().join("home");
    let runtime = RuntimeComposition::new();

    ensure_local_target(Some(&home_path))?;
    let home = MmHome::resolve(Some(&home_path))?;
    let targets = FilesystemTargetStore::new(home.targets_file.clone());
    assert_eq!(targets.resolve_target("local")?.name, "local");

    let services = target_services(Some(&home_path), runtime.process(), clock())?;
    let _manager = services.manager;
    let services = target_services(Some(&home_path), runtime.process(), clock())?;
    let _ingress = services.ingress();
    let services = target_services(Some(&home_path), runtime.process(), clock())?;
    assert!(services.bootstrap().is_err());
    let _ca = local_ca_manager(Some(&home_path), false, runtime.process())?;
    let _system_ca = local_ca_manager(Some(&home_path), true, runtime.process())?;

    let target_id = TargetId(uuid::Uuid::from_u128(45));
    let hash = "a".repeat(64);
    assert!(install_ingress(&temp.path().join("missing"), target_id, &hash).is_err());
    assert!(uninstall_ingress(target_id, &hash).is_err());
    Ok(())
}

#[tokio::test]
async fn gate_ingress_composition_rejects_wrong_targets_and_missing_service_state()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let target_id = TargetId(uuid::Uuid::from_u128(46));
    let target = NamedTarget::new("local".to_owned(), TargetDefinition::local(target_id));
    let runtime = RuntimeComposition::new();
    let publisher = RuntimeGateIngressPublisher::new(
        target,
        Arc::new(FilesystemServiceStackStore::new(
            temp.path().join("services"),
        )),
        RuntimeFactory::new(runtime.process()),
        Arc::new(FilesystemTraefikDynamicConfigStore::new(
            temp.path().join("routes"),
        )),
    );

    assert!(
        publisher
            .publish(
                TargetId(uuid::Uuid::from_u128(47)),
                &[],
                &Default::default()
            )
            .await
            .is_err()
    );
    assert!(
        publisher
            .publish(target_id, &[], &Default::default())
            .await
            .is_err()
    );
    Ok(())
}
