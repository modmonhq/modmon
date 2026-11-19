use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use mm::adapters::config::ConfigLoader;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{ProjectDocumentStore, TargetStore};
use mm::app::{
    DevelopmentMountManager, DevelopmentMountPlanRequest, StackManager, WorkspaceSelection,
};
use mm::domain::runtime::{RuntimeCapability, RuntimeKind, RuntimeProbe};
use mm::domain::target::{TargetBootstrap, TargetDefinition, TargetId, TargetTransport};
use tempfile::TempDir;

mod temp_dirs;

struct Fixture {
    project: TempDir,
    home: TempDir,
    targets: Arc<FilesystemTargetStore>,
    runtime: Arc<FakeRuntime>,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let project = temp_dirs::tempdir()?;
        fs::create_dir_all(project.path().join(".config"))?;
        fs::create_dir(project.path().join("app"))?;
        fs::write(
            project.path().join("compose.yaml"),
            "services:\n  web:\n    image: example/web\n    working_dir: /app\n",
        )?;
        fs::write(project.path().join("app/package.json"), "{}\n")?;
        fs::write(
            project.path().join("app/Dockerfile"),
            "FROM scratch AS build\nWORKDIR /build\nFROM scratch\nWORKDIR /app\n",
        )?;
        fs::write(
            project.path().join(".config/mm.yaml"),
            r#"schema-version: 1
project:
  id: 219c527b-6242-7b43-bfd4-bd60e27c1de3
  name: development-mounts
  development-environment: local
default-environment: local
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web:
        stack: app
environments:
  local: {}
  production: {}
"#,
        )?;
        let home = temp_dirs::tempdir()?;
        let targets = Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
        targets.insert_target(
            "local",
            TargetDefinition::local(TargetId(uuid::Uuid::parse_str(
                "319c527b-6242-7b43-bfd4-bd60e27c1de3",
            )?)),
        )?;
        targets.insert_target(
            "remote",
            TargetDefinition {
                id: TargetId(uuid::Uuid::parse_str(
                    "419c527b-6242-7b43-bfd4-bd60e27c1de3",
                )?),
                transport: TargetTransport::Ssh {
                    host: "remote.example.test".to_owned(),
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
                default_runtime: Some(RuntimeKind::DockerCompose),
                available_runtimes: vec![RuntimeKind::DockerCompose],
                runtime: None,
                nodes: Vec::new(),
            },
        )?;
        let runtime = Arc::new(FakeRuntime {
            probe: RuntimeProbe {
                kind: RuntimeKind::DockerCompose,
                available: true,
                engine_version: Some("test".to_owned()),
                compose_provider: None,
                rootless: None,
                socket_path: None,
                capabilities: vec![
                    RuntimeCapability::BindMounts,
                    RuntimeCapability::ManageVolumes,
                ],
                diagnostics: Vec::new(),
            },
            user_rendered: None,
            rendered: BTreeMap::new(),
            observed: BTreeMap::new(),
        });
        Ok(Self {
            project,
            home,
            targets,
            runtime,
        })
    }

    fn manager(&self) -> DevelopmentMountManager {
        let documents: Arc<dyn ProjectDocumentStore> = Arc::new(ConfigLoader);
        let targets: Arc<dyn TargetStore> = self.targets.clone();
        DevelopmentMountManager::new(documents, targets, self.runtime.clone())
    }

    fn selection(&self) -> WorkspaceSelection {
        WorkspaceSelection::new(self.project.path(), None)
    }

    fn request(&self) -> DevelopmentMountPlanRequest {
        DevelopmentMountPlanRequest {
            module: "app".to_owned(),
            ..Default::default()
        }
    }

    fn stack_manager(&self) -> StackManager {
        let documents: Arc<dyn ProjectDocumentStore> = Arc::new(ConfigLoader);
        let targets: Arc<dyn TargetStore> = self.targets.clone();
        StackManager::new(
            documents,
            Arc::new(FilesystemStackArtifactStore::new(
                self.home.path().join("compose"),
            )),
            targets,
        )
    }
}

#[tokio::test]
async fn compose_dockerfile_and_missing_target_discovery_are_distinct()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.project.path().join("app/package.json"))?;
    fs::remove_file(fixture.project.path().join("app/Dockerfile"))?;
    let manager = fixture.manager();
    let compose = manager
        .plan(&fixture.selection(), &fixture.request())
        .await?;
    assert_eq!(compose.proposal.evidence.len(), 1);
    assert_eq!(
        compose.proposal.evidence[0].source,
        mm::domain::development_mount::MountEvidenceSource::ComposeWorkingDirectory
    );

    fs::write(
        fixture.project.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    fs::write(
        fixture.project.path().join("app/Dockerfile"),
        "FROM scratch AS build\nWORKDIR /build\nFROM scratch\nWORKDIR /workspace\n",
    )?;
    let dockerfile = manager
        .plan(&fixture.selection(), &fixture.request())
        .await?;
    assert_eq!(
        dockerfile
            .proposal
            .intent
            .as_ref()
            .map(|intent| intent.target.as_str()),
        Some("/workspace")
    );
    assert!(dockerfile.proposal.evidence.iter().any(|evidence| {
        evidence.source == mm::domain::development_mount::MountEvidenceSource::DockerfileWorkdir
            && evidence.target == "/workspace"
    }));
    assert!(
        dockerfile
            .proposal
            .confirmations
            .iter()
            .any(|reason| reason.contains("stages"))
    );

    fs::remove_file(fixture.project.path().join("app/Dockerfile"))?;
    let missing = manager
        .plan(&fixture.selection(), &fixture.request())
        .await?;
    assert!(missing.proposal.intent.is_none());
    assert!(
        missing
            .proposal
            .confirmations
            .iter()
            .any(|reason| reason.contains("no Compose working_dir"))
    );
    Ok(())
}

#[tokio::test]
async fn proposal_is_deterministic_stale_checked_and_persisted_portably()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let manager = fixture.manager();
    let selection = fixture.selection();
    let request = fixture.request();
    let first = manager.plan(&selection, &request).await?;
    let second = manager.plan(&selection, &request).await?;
    assert_eq!(first.plan_hash, second.plan_hash);
    assert_eq!(
        first
            .proposal
            .intent
            .as_ref()
            .map(|mount| mount.target.as_str()),
        Some("/app")
    );
    assert!(
        first
            .proposal
            .confirmations
            .iter()
            .any(|reason| reason.contains("stages"))
    );
    assert_eq!(
        first
            .proposal
            .intent
            .as_ref()
            .and_then(|mount| mount.preset),
        Some(mm::domain::development_mount::DevelopmentMountPreset::Node)
    );

    fs::write(
        fixture.project.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n    working_dir: /workspace\n",
    )?;
    let stale = manager
        .configure_reviewed(&selection, &request, &first.plan_hash)
        .await;
    assert!(matches!(stale, Err(mm::MmError::State { .. })));

    let config_path = fixture.project.path().join(".config/mm.yaml");
    fs::write(
        &config_path,
        format!(
            "# retained project comment\n{}",
            fs::read_to_string(&config_path)?
        ),
    )?;
    let current = manager.plan(&selection, &request).await?;
    manager
        .configure_reviewed(&selection, &request, &current.plan_hash)
        .await?;
    let source = fs::read_to_string(fixture.project.path().join(".config/mm.yaml"))?;
    assert!(source.starts_with("# retained project comment\n"));
    assert!(source.contains("development-mount:"));
    assert!(source.contains("source: ."));
    assert!(!source.contains(&fixture.project.path().display().to_string()));
    let document = ConfigLoader.load_from(fixture.project.path())?;
    assert_eq!(
        document.config.modules["app"]
            .development_mount
            .as_ref()
            .map(|mount| mount.services.clone()),
        Some(vec!["web".to_owned()])
    );
    Ok(())
}

#[tokio::test]
async fn differing_compose_services_require_confirmation_and_explicit_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fs::write(
        fixture.project.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n    working_dir: /app\n  worker:\n    image: example/worker\n    working_dir: /workspace\n",
    )?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "      web:\n        stack: app\n",
        "      web:\n        stack: app\n      worker:\n        stack: app\n",
    );
    fs::write(config_path, config)?;
    let plan = fixture
        .manager()
        .plan(&fixture.selection(), &fixture.request())
        .await?;
    assert!(plan.proposal.confirmations.iter().any(|reason| {
        reason.contains("different working_dir") || reason.contains("evidence disagrees")
    }));
    assert_eq!(
        plan.proposal
            .intent
            .as_ref()
            .map(|mount| mount.services.clone()),
        Some(vec!["web".to_owned(), "worker".to_owned()])
    );
    Ok(())
}

#[tokio::test]
async fn local_development_gating_refuses_remote_nondevelopment_and_missing_capability()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let manager = fixture.manager();
    let request = fixture.request();
    let mut selection = fixture.selection();
    selection.target = Some("remote".to_owned());
    assert!(matches!(
        manager.plan(&selection, &request).await,
        Err(mm::MmError::Security { .. })
    ));
    selection.target = Some("local".to_owned());
    selection.environment = Some("production".to_owned());
    assert!(matches!(
        manager.plan(&selection, &request).await,
        Err(mm::MmError::Security { .. })
    ));

    let unavailable = Arc::new(FakeRuntime {
        probe: RuntimeProbe {
            capabilities: Vec::new(),
            ..fixture.runtime.probe.clone()
        },
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let documents: Arc<dyn ProjectDocumentStore> = Arc::new(ConfigLoader);
    let targets: Arc<dyn TargetStore> = fixture.targets.clone();
    let manager = DevelopmentMountManager::new(documents, targets, unavailable);
    assert!(matches!(
        manager.plan(&fixture.selection(), &request).await,
        Err(error) if error.code() == "MM_RUNTIME_UNSUPPORTED"
    ));
    Ok(())
}

#[tokio::test]
async fn overlay_is_deterministic_owned_and_disappears_outside_local_development()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let manager = fixture.manager();
    let selection = fixture.selection();
    let request = fixture.request();
    let plan = manager.plan(&selection, &request).await?;
    manager
        .configure_reviewed(&selection, &request, &plan.plan_hash)
        .await?;

    let stacks = fixture.stack_manager();
    let first = stacks.compile(&selection)?;
    let second = stacks.compile(&selection)?;
    assert_eq!(
        first.stacks[0].manifest.artifact_hash,
        second.stacks[0].manifest.artifact_hash
    );
    let manifest = &first.stacks[0].manifest;
    assert!(manifest.services[0].development_mount.is_some());
    assert_eq!(manifest.preserved_volumes.len(), 1);
    assert!(
        manifest.preserved_volumes[0]
            .resource
            .starts_with("development-mount-")
    );
    let overlay = fs::read_to_string(&first.stacks[0].generated_overlay_path)?;
    assert!(overlay.contains(":/app:z"));
    assert!(overlay.contains("/app/node_modules"));
    assert!(overlay.contains("io.mm.target-id"));
    assert!(overlay.contains("io.mm.project-id"));

    let mut production = selection.clone();
    production.environment = Some("production".to_owned());
    let production = stacks.compile(&production)?;
    let overlay = fs::read_to_string(&production.stacks[0].generated_overlay_path)?;
    assert!(!overlay.contains(":/app:z"));
    assert!(production.stacks[0].manifest.preserved_volumes.is_empty());

    let mut remote = selection;
    remote.target = Some("remote".to_owned());
    let remote = stacks.compile(&remote)?;
    let overlay = fs::read_to_string(&remote.stacks[0].generated_overlay_path)?;
    assert!(!overlay.contains(":/app:z"));
    assert!(remote.stacks[0].manifest.preserved_volumes.is_empty());

    let collision_key = manifest.preserved_volumes[0].logical_name.clone();
    fs::write(
        fixture.project.path().join("compose.yaml"),
        format!(
            "services:\n  web:\n    image: example/web\n    working_dir: /app\nvolumes:\n  {collision_key}: {{}}\n"
        ),
    )?;
    assert!(matches!(
        stacks.compile(&fixture.selection()),
        Err(mm::MmError::Security { .. })
    ));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn escaping_symlink_and_sensitive_content_fail_before_config_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new()?;
    let manager = fixture.manager();
    let request = fixture.request();
    fs::write(fixture.project.path().join("app/.env"), "SECRET=value\n")?;
    assert!(matches!(
        manager.plan(&fixture.selection(), &request).await,
        Err(mm::MmError::Security { .. })
    ));
    let mut exception = request.clone();
    exception.safe_exceptions = vec![".env".to_owned()];
    assert!(manager.plan(&fixture.selection(), &exception).await.is_ok());
    fs::remove_file(fixture.project.path().join("app/.env"))?;
    let outside = temp_dirs::tempdir()?;
    symlink(outside.path(), fixture.project.path().join("app/escape"))?;
    assert!(matches!(
        manager.plan(&fixture.selection(), &request).await,
        Err(mm::MmError::Security { .. })
    ));
    let document = ConfigLoader.load_from(fixture.project.path())?;
    assert!(document.config.modules["app"].development_mount.is_none());
    let traversal = DevelopmentMountPlanRequest {
        source: Some("../outside".to_owned()),
        ..request
    };
    assert!(matches!(
        manager.plan(&fixture.selection(), &traversal).await,
        Err(mm::MmError::Security { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn disable_removes_shared_intent_and_leaves_cleanup_in_the_next_owned_plan()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let manager = fixture.manager();
    let selection = fixture.selection();
    let request = fixture.request();
    let plan = manager.plan(&selection, &request).await?;
    let configured = manager
        .configure_reviewed(&selection, &request, &plan.plan_hash)
        .await?;
    assert!(configured.proposal.configured);
    let disabled = manager
        .disable_reviewed(&selection, &request, &configured.plan_hash)
        .await?;
    assert!(!disabled.proposal.configured);
    let document = ConfigLoader.load_from(fixture.project.path())?;
    assert!(document.config.modules["app"].development_mount.is_none());
    Ok(())
}

#[tokio::test]
async fn monorepo_sources_and_masked_dependencies_compile_together()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fs::create_dir(fixture.project.path().join("shared"))?;
    fs::write(
        fixture.project.path().join("shared/code.js"),
        "export default 1;",
    )?;
    fs::create_dir_all(fixture.project.path().join("app/node_modules/dependency"))?;
    fs::write(
        fixture
            .project
            .path()
            .join("app/node_modules/dependency/.env"),
        "masked",
    )?;
    fs::create_dir(fixture.project.path().join("app/.ai"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "../../shared",
        fixture.project.path().join("app/.ai/shared"),
    )?;

    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "    path: app",
        r#"    path: .
    development-mount:
      source: app
      target: /app
      additional:
        - source: shared
          target: /shared
      services: [web]
      preserve:
        - path: /app/node_modules
      excludes: [.ai]"#,
    );
    fs::write(&config_path, config)?;

    let plan = fixture
        .manager()
        .plan(&fixture.selection(), &fixture.request())
        .await?;
    assert_eq!(
        plan.proposal
            .intent
            .as_ref()
            .ok_or("missing mount")?
            .additional
            .len(),
        1
    );
    let compiled = fixture.stack_manager().compile(&fixture.selection())?;
    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(
        &compiled.stacks[0].generated_overlay_path,
    )?)?;
    let mounts = overlay["services"]["web"]["volumes"]
        .as_sequence()
        .ok_or("missing mounts")?;
    assert!(
        mounts
            .iter()
            .any(|m| m.as_str().is_some_and(|m| m.ends_with("/shared:/shared:z")))
    );
    let deps = mounts
        .iter()
        .find(|m| m["target"] == "/app/node_modules")
        .ok_or("missing dependency volume")?;
    assert_eq!(deps["volume"]["nocopy"], false);
    let mask = mounts
        .iter()
        .find(|m| m["target"] == "/app/.ai")
        .ok_or("missing mask")?;
    assert_eq!(mask["volume"]["nocopy"], true);

    let mut production = fixture.selection();
    production.environment = Some("production".to_owned());
    let production = fixture.stack_manager().compile(&production)?;
    let overlay = fs::read_to_string(&production.stacks[0].generated_overlay_path)?;
    assert!(!overlay.contains(":/shared:z"));
    assert!(!overlay.contains(":/app:z"));
    assert!(production.stacks[0].manifest.preserved_volumes.is_empty());

    // Additional sources receive the same containment and sensitive-file checks.
    fs::write(fixture.project.path().join("shared/.env"), "secret")?;
    assert!(
        fixture
            .manager()
            .plan(&fixture.selection(), &fixture.request())
            .await
            .is_err()
    );
    fs::remove_file(fixture.project.path().join("shared/.env"))?;
    let config = fs::read_to_string(&config_path)?.replace("source: shared", "source: ../outside");
    fs::write(&config_path, config)?;
    assert!(
        fixture
            .manager()
            .plan(&fixture.selection(), &fixture.request())
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn protected_and_excluded_paths_are_covered_by_what_the_source_has_there()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::development_mount::MaskedPathKind;

    let fixture = Fixture::new()?;
    let app = fixture.project.path().join("app");
    fs::write(app.join(".env"), "LOCAL_ONLY=1\n")?;
    fs::create_dir(app.join("private"))?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "    path: app",
        r#"    path: app
    development-mount:
      source: .
      target: /app
      services: [web]
      excludes: [logs]
      protected: [.env, private, config/local.env]"#,
    );
    fs::write(&config_path, config)?;

    let compiled = fixture.stack_manager().compile(&fixture.selection())?;
    let manifest = &compiled.stacks[0].manifest;
    assert_eq!(
        manifest
            .masked_paths
            .iter()
            .map(|path| (path.container_path.as_str(), path.kind))
            .collect::<Vec<_>>(),
        [
            ("/app/.env", MaskedPathKind::File),
            ("/app/config/local.env", MaskedPathKind::Missing),
            ("/app/logs", MaskedPathKind::Missing),
            ("/app/private", MaskedPathKind::Directory),
        ]
    );
    assert_eq!(
        manifest.masked_paths[0].line(),
        "masked: /app/.env (module app) is a file; an empty read-only file covers it"
    );
    assert_eq!(
        manifest.masked_paths[1].line(),
        "not masked: /app/config/local.env (module app) is not in the source, so nothing is mounted there; if it is created, the containers see it until the next deployment"
    );
    assert_eq!(
        manifest.masked_paths[3].line(),
        "masked: /app/private (module app) is a directory; an empty volume covers it"
    );
    assert_eq!(
        manifest
            .preserved_volumes
            .iter()
            .map(|volume| volume.container_path.as_str())
            .collect::<Vec<_>>(),
        ["/app/private"]
    );

    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(
        &compiled.stacks[0].generated_overlay_path,
    )?)?;
    let mounts = overlay["services"]["web"]["volumes"]
        .as_sequence()
        .ok_or("missing mounts")?;
    let env = mounts
        .iter()
        .filter_map(serde_yaml_ng::Value::as_str)
        .find_map(|mount| mount.strip_suffix(":/app/.env:ro,z"))
        .ok_or("no read-only file over /app/.env")?;
    let env = std::path::Path::new(env);
    assert!(env.starts_with(fixture.home.path()), "{}", env.display());
    assert!(env.is_file());
    assert_eq!(fs::read(env)?, b"");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(env)?.permissions().mode() & 0o777, 0o444);
    }
    let private = mounts
        .iter()
        .find(|mount| mount["target"] == "/app/private")
        .ok_or("no volume over /app/private")?;
    assert_eq!(private["type"], "volume");
    assert_eq!(private["volume"]["nocopy"], true);
    for missing in ["/app/logs", "/app/config"] {
        assert!(
            !mounts.iter().any(|mount| mount["target"] == missing
                || mount.as_str().is_some_and(|mount| mount.contains(missing))),
            "{missing} is mounted"
        );
    }
    assert!(!app.join("logs").exists());
    assert!(!app.join("config").exists());
    assert_eq!(fs::read_to_string(app.join(".env"))?, "LOCAL_ONLY=1\n");

    let mut changed = manifest.clone();
    changed.masked_paths[2].kind = MaskedPathKind::File;
    assert_ne!(changed.canonical_hash()?, manifest.canonical_hash()?);

    fs::remove_file(app.join(".env"))?;
    let without = fixture.stack_manager().compile(&fixture.selection())?;
    let without = &without.stacks[0].manifest;
    assert_ne!(without.artifact_hash, manifest.artifact_hash);
    assert_eq!(without.masked_paths[0].kind, MaskedPathKind::Missing);
    assert!(!without.masks_a_file());
    assert!(!app.join(".env").exists());
    Ok(())
}
