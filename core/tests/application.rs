use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::EnvironmentIndexer;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{IngressRouteRegistry, TargetStore};
use mm::app::{Application, ApplicationDependencies, StackManager, WorkspaceSelection};
use mm::domain::runtime::{
    ComposeProvider, ContainerObservation, ObservedProject, RenderedProject, RuntimeCapability,
    RuntimeKind, RuntimeProbe, ServiceState,
};
use mm::domain::target::{TargetBootstrap, TargetDefinition, TargetId, TargetTransport};
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

#[tokio::test]
async fn creates_one_snapshot_for_every_presentation() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(
        &temporary.path().join("state.sqlite3"),
    )?);
    let runtime = Arc::new(FakeRuntime {
        probe: RuntimeProbe {
            kind: RuntimeKind::PodmanCompose,
            available: true,
            engine_version: Some("test".to_owned()),
            compose_provider: Some(ComposeProvider {
                name: "test-compose".to_owned(),
                version: Some("1".to_owned()),
                executable: None,
            }),
            rootless: Some(true),
            socket_path: None,
            capabilities: vec![
                RuntimeCapability::RenderCompose,
                RuntimeCapability::ObserveServices,
            ],
            diagnostics: Vec::new(),
        },
        user_rendered: None,
        rendered: [(
            "app".to_owned(),
            RenderedProject {
                stack: "app".to_owned(),
                project_name: "minimal-local-app".to_owned(),
                service_names: vec!["api".to_owned(), "sidecar".to_owned()],
                services: BTreeMap::new(),
                resources: Vec::new(),
                config_hash: "rendered-hash".to_owned(),
            },
        )]
        .into_iter()
        .collect(),
        observed: [(
            "app".to_owned(),
            ObservedProject {
                stack: "app".to_owned(),
                services: Vec::new(),
                containers: vec![
                    ContainerObservation {
                        id: "container-id".to_owned(),
                        name: "minimal-api-1".to_owned(),
                        service: "api".to_owned(),
                        image: "alpine:3.22".to_owned(),
                        state: "running".to_owned(),
                        status: "Up".to_owned(),
                        health: Some("healthy".to_owned()),
                        restart_count: Some(2),
                        exit_code: None,
                        created_at: None,
                        started_at: None,
                        finished_at: None,
                        revision: None,
                        digest: None,
                        diagnostic_code: None,
                        error: None,
                        labels: BTreeMap::new(),
                        node: None,
                        history: false,
                    },
                    ContainerObservation {
                        id: "sidecar-id".to_owned(),
                        name: "minimal-sidecar-1".to_owned(),
                        service: "sidecar".to_owned(),
                        image: "alpine:3.22".to_owned(),
                        state: "running".to_owned(),
                        status: "Up".to_owned(),
                        health: None,
                        restart_count: Some(0),
                        exit_code: None,
                        created_at: None,
                        started_at: None,
                        finished_at: None,
                        revision: None,
                        digest: None,
                        diagnostic_code: None,
                        error: None,
                        labels: BTreeMap::new(),
                        node: None,
                        history: false,
                    },
                ],
            },
        )]
        .into_iter()
        .collect(),
    });
    let artifacts = Arc::new(FilesystemStackArtifactStore::new(
        temporary.path().join("compose"),
    ));
    let route_registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let targets = target_store(temporary.path())?;
    let documents = Arc::new(ConfigLoader);
    let stacks =
        StackManager::with_route_registry(documents.clone(), artifacts, route_registry, targets);
    let application = Application::new(ApplicationDependencies {
        documents,
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime,
        state,
        stacks,
        clock: Arc::new(FixedClock { unix_ms: 1_000 }),
    });

    let snapshot = application
        .status(&WorkspaceSelection::new(fixture_root(), None))
        .await?;

    assert!(snapshot.validation.is_valid());
    assert_eq!(snapshot.target, "local");
    assert_eq!(snapshot.stacks.len(), 1);
    assert_eq!(snapshot.stacks[0].name, "app");
    assert!(!snapshot.stacks[0].artifact_hash.is_empty());
    assert_eq!(snapshot.stacks[0].services.len(), 2);
    assert_eq!(
        snapshot.stacks[0].services[0].module.as_deref(),
        Some("api")
    );
    assert_eq!(snapshot.stacks[0].services[0].state, ServiceState::Running);
    assert_eq!(snapshot.stacks[0].services[1].name, "sidecar");
    assert_eq!(snapshot.stacks[0].services[1].module, None);
    assert_eq!(snapshot.stacks[0].services[1].state, ServiceState::Running);
    assert_eq!(snapshot.stacks[0].rendered_config_hash, "rendered-hash");
    Ok(())
}

#[tokio::test]
async fn generated_overlay_cannot_hide_a_service_missing_from_user_files()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(
        &temporary.path().join("state.sqlite3"),
    )?);
    let runtime = Arc::new(FakeRuntime {
        probe: RuntimeProbe {
            kind: RuntimeKind::PodmanCompose,
            available: true,
            engine_version: Some("test".to_owned()),
            compose_provider: Some(ComposeProvider {
                name: "test-compose".to_owned(),
                version: Some("1".to_owned()),
                executable: None,
            }),
            rootless: Some(true),
            socket_path: None,
            capabilities: vec![
                RuntimeCapability::RenderCompose,
                RuntimeCapability::ObserveServices,
            ],
            diagnostics: Vec::new(),
        },
        user_rendered: Some(
            [(
                "app".to_owned(),
                RenderedProject {
                    stack: "app".to_owned(),
                    project_name: "minimal-local-app".to_owned(),
                    service_names: Vec::new(),
                    services: BTreeMap::new(),
                    resources: Vec::new(),
                    config_hash: "user-hash".to_owned(),
                },
            )]
            .into_iter()
            .collect(),
        ),
        rendered: [(
            "app".to_owned(),
            RenderedProject {
                stack: "app".to_owned(),
                project_name: "minimal-local-app".to_owned(),
                service_names: vec!["api".to_owned()],
                services: BTreeMap::new(),
                resources: Vec::new(),
                config_hash: "full-hash".to_owned(),
            },
        )]
        .into_iter()
        .collect(),
        observed: [(
            "app".to_owned(),
            ObservedProject {
                stack: "app".to_owned(),
                services: Vec::new(),
                containers: Vec::new(),
            },
        )]
        .into_iter()
        .collect(),
    });
    let artifacts = Arc::new(FilesystemStackArtifactStore::new(
        temporary.path().join("compose"),
    ));
    let route_registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let targets = target_store(temporary.path())?;
    let documents = Arc::new(ConfigLoader);
    let stacks =
        StackManager::with_route_registry(documents.clone(), artifacts, route_registry, targets);
    let application = Application::new(ApplicationDependencies {
        documents,
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime,
        state,
        stacks,
        clock: Arc::new(FixedClock { unix_ms: 1_000 }),
    });
    let snapshot = application
        .status(&WorkspaceSelection::new(fixture_root(), None))
        .await?;
    assert!(
        snapshot
            .validation
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "MM_SERVICE_NOT_IN_COMPOSE" })
    );
    Ok(())
}

#[tokio::test]
async fn ssh_targets_never_fall_back_to_local_runtime_inspection()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(
        &temporary.path().join("state.sqlite3"),
    )?);
    let runtime = Arc::new(FakeRuntime {
        probe: RuntimeProbe {
            kind: RuntimeKind::PodmanCompose,
            available: true,
            engine_version: Some("test".to_owned()),
            compose_provider: Some(ComposeProvider {
                name: "test-compose".to_owned(),
                version: Some("1".to_owned()),
                executable: None,
            }),
            rootless: Some(true),
            socket_path: None,
            capabilities: vec![
                RuntimeCapability::RenderCompose,
                RuntimeCapability::ObserveServices,
            ],
            diagnostics: Vec::new(),
        },
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let artifacts = Arc::new(FilesystemStackArtifactStore::new(
        temporary.path().join("compose"),
    ));
    let route_registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(
        temporary.path().join("targets.yaml"),
    ));
    targets.insert_target(
        "local",
        TargetDefinition {
            id: TargetId(Uuid::parse_str("319c527b-6242-7b43-bfd4-bd60e27c1de3")?),
            transport: TargetTransport::Ssh {
                host: "host.example.test".to_owned(),
                port: 22,
                user: "mm".to_owned(),
                host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"
                    .to_owned(),
                host_key: None,
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: None,
            },
            bootstrap: TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: None,
            available_runtimes: Vec::new(),
            runtime: None,
            nodes: Vec::new(),
        },
    )?;
    let documents = Arc::new(ConfigLoader);
    let stacks =
        StackManager::with_route_registry(documents.clone(), artifacts, route_registry, targets);
    let application = Application::new(ApplicationDependencies {
        documents,
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime,
        state,
        stacks,
        clock: Arc::new(FixedClock { unix_ms: 1_000 }),
    });

    let snapshot = application
        .status(&WorkspaceSelection::new(fixture_root(), None))
        .await?;
    assert!(
        snapshot
            .validation
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "MM_RUNTIME_TARGET_TRANSPORT_UNSUPPORTED" })
    );
    assert!(!snapshot.runtime.available);
    assert!(
        snapshot
            .runtime
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_RUNTIME_PROBE_SKIPPED")
    );
    assert!(
        snapshot
            .stacks
            .iter()
            .all(|stack| stack.artifact_hash.is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn read_only_validation_and_host_doctor_do_not_require_state()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let runtime = Arc::new(FakeRuntime {
        probe: runtime_probe(Vec::new(), false),
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let application = Application::read_only(
        Arc::new(ConfigLoader),
        Arc::new(EnvironmentIndexer),
        runtime,
        Arc::new(FixedClock { unix_ms: 1_000 }),
    );
    assert!(format!("{application:?}").contains("PodmanCompose"));
    assert!(application.schema().as_value().is_object());

    let mut selection = WorkspaceSelection::new(fixture_root(), Some("local".to_owned()));
    selection.runtime = Some(RuntimeKind::DockerCompose);
    selection.target = Some("explicit-target".to_owned());
    let validation = application.validate(&selection)?;
    assert_eq!(validation.environment, "local");
    assert_eq!(validation.target, "local");
    assert_eq!(validation.runtime_kind, Some(RuntimeKind::DockerCompose));
    assert_eq!(validation.target_source, None);

    let report = application
        .doctor(&WorkspaceSelection::new(temporary.path(), None))
        .await?;
    assert!(!report.project_selected);
    assert_eq!(report.runtime_kind, Some(RuntimeKind::PodmanCompose));
    assert!(
        report
            .validation
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_CONFIG_NOT_FOUND")
    );
    assert!(
        application
            .initialize(temporary.path(), Some("unavailable"))
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn initialization_registers_the_created_workspace_and_starter_files()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let runtime = Arc::new(FakeRuntime {
        probe: runtime_probe(Vec::new(), false),
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let application = full_application(&temporary, runtime)?;
    let root = temporary.path().join("created-project");
    std::fs::create_dir(&root)?;
    let initialized = application.initialize(&root, Some("created"))?;
    assert_eq!(initialized.project_name, "created");
    assert_eq!(initialized.root, root);
    assert!(initialized.config_path.is_file());
    assert!(!initialized.starter_files.is_empty());
    Ok(())
}

#[tokio::test]
async fn failed_workspace_registration_restores_the_reviewed_gitignore()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let runtime = Arc::new(FakeRuntime {
        probe: runtime_probe(Vec::new(), false),
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let application = full_application(&temporary, runtime)?;
    rusqlite::Connection::open(temporary.path().join("state.sqlite3"))?.execute_batch(
        "CREATE TRIGGER fail_registration BEFORE INSERT ON workspaces BEGIN SELECT RAISE(FAIL, 'injected registration failure'); END;",
    )?;
    let root = temporary.path().join("project");
    std::fs::create_dir(&root)?;
    std::fs::write(
        root.join(".gitignore"),
        "# existing operator rules\nnode_modules\n",
    )?;
    let plan =
        mm::app::OnboardingService::new(Arc::new(ConfigLoader)).plan_initialization(&root, None)?;
    assert!(
        application
            .initialize_reviewed(&root, None, &plan.plan_hash)
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".gitignore"))?,
        "# existing operator rules\nnode_modules\n"
    );
    assert!(!root.join(".config").exists());
    Ok(())
}

#[tokio::test]
async fn project_doctor_renders_each_valid_stack_and_reports_render_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let rendered = RenderedProject {
        stack: "app".to_owned(),
        project_name: "minimal-local-app".to_owned(),
        service_names: vec!["api".to_owned()],
        services: BTreeMap::new(),
        resources: Vec::new(),
        config_hash: "doctor-render".to_owned(),
    };
    let runtime = Arc::new(FakeRuntime {
        probe: runtime_probe(vec![RuntimeCapability::RenderCompose], true),
        user_rendered: None,
        rendered: [("app".to_owned(), rendered)].into_iter().collect(),
        observed: BTreeMap::new(),
    });
    let report = full_application(&temporary, runtime)?
        .doctor(&WorkspaceSelection::new(fixture_root(), None))
        .await?;
    assert!(report.project_selected);
    assert!(report.validation.is_valid());
    assert!(report.runtime.diagnostics.is_empty());

    let failed_runtime = Arc::new(FakeRuntime {
        probe: runtime_probe(vec![RuntimeCapability::RenderCompose], true),
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let failed = full_application(&temp_dirs::tempdir()?, failed_runtime)?
        .doctor(&WorkspaceSelection::new(fixture_root(), None))
        .await?;
    assert!(
        failed
            .runtime
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_COMPOSE_RENDER_FAILED")
    );
    Ok(())
}

fn runtime_probe(capabilities: Vec<RuntimeCapability>, available: bool) -> RuntimeProbe {
    RuntimeProbe {
        kind: RuntimeKind::PodmanCompose,
        available,
        engine_version: available.then(|| "test".to_owned()),
        compose_provider: None,
        rootless: Some(true),
        socket_path: None,
        capabilities,
        diagnostics: Vec::new(),
    }
}

fn full_application(
    temporary: &TempDir,
    runtime: Arc<FakeRuntime>,
) -> Result<Application, Box<dyn std::error::Error>> {
    let state = Arc::new(SqliteStateStore::open(
        &temporary.path().join("state.sqlite3"),
    )?);
    let documents = Arc::new(ConfigLoader);
    let route_registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let stacks = StackManager::with_route_registry(
        documents.clone(),
        Arc::new(FilesystemStackArtifactStore::new(
            temporary.path().join("compose"),
        )),
        route_registry,
        target_store(temporary.path())?,
    );
    Ok(Application::new(ApplicationDependencies {
        documents,
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime,
        state,
        stacks,
        clock: Arc::new(FixedClock { unix_ms: 1_000 }),
    }))
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
}

fn target_store(root: &Path) -> Result<Arc<dyn TargetStore>, Box<dyn std::error::Error>> {
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(root.join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    Ok(store)
}
