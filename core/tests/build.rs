use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{
    BuildOperationGuard, BuildOperationLock, BuildSourceInspector, ImageBuilder,
    ProjectDocumentStore, SecretMaterialResolver, SecretMetadataInspector, StateStore,
    TargetOperationLockGuard,
};
use mm::app::{BuildExecutionPorts, BuildManager, BuildService, WorkspaceSelection};
use mm::domain::build::{BuildSource, BuiltImage};
use mm::domain::error::{MmError, MutationStatus};
use mm::domain::operation::{NewOperation, OperationId, OperationKind, OperationState};
use mm::domain::secret::{
    SecretMaterial, SecretMetadata, SecretReference, SecretResolution, SecretStatus,
};
use mm::domain::workspace::{ProjectId, WorkspaceRegistration};
use tempfile::TempDir;

mod temp_dirs;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

#[derive(Debug)]
struct FakeSource;

#[async_trait]
impl BuildSourceInspector for FakeSource {
    async fn inspect(&self, _project_root: &Path) -> Result<BuildSource, MmError> {
        Ok(BuildSource {
            commit: COMMIT.to_owned(),
            branch: Some("main".to_owned()),
            tags_at_head: Vec::new(),
            repository_tags: Vec::new(),
            tags_complete: true,
            default_branch: true,
            dirty: false,
        })
    }
}

#[derive(Debug)]
struct MutatingSource {
    inspections: AtomicUsize,
    config_path: PathBuf,
    replacement: String,
}

#[async_trait]
impl BuildSourceInspector for MutatingSource {
    async fn inspect(&self, project_root: &Path) -> Result<BuildSource, MmError> {
        if self.inspections.fetch_add(1, Ordering::SeqCst) == 2 {
            fs::write(&self.config_path, &self.replacement).map_err(|source| MmError::Io {
                context: "mutating configuration after the second build plan".to_owned(),
                source,
            })?;
        }
        FakeSource.inspect(project_root).await
    }
}

#[derive(Debug)]
struct FakeSecrets;

#[derive(Debug)]
struct FailingSecretResolver;

#[derive(Debug)]
struct FailingPortDocuments;

impl ProjectDocumentStore for FailingPortDocuments {
    fn discover(&self, start: &Path) -> Result<PathBuf, MmError> {
        ConfigLoader.discover(start)
    }

    fn load_from(&self, _start: &Path) -> Result<mm::domain::config::ConfigDocument, MmError> {
        Err(MmError::State {
            message: "injected execution document load failure".to_owned(),
        })
    }

    fn load(&self, path: &Path) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ConfigLoader.load(path)
    }

    fn initialize(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<Box<dyn mm::app::ports::ProjectInitialization>, MmError> {
        <ConfigLoader as ProjectDocumentStore>::initialize(&ConfigLoader, root, requested_name)
    }

    fn schema(&self) -> schemars::Schema {
        ConfigLoader.schema()
    }

    fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ConfigLoader.set_default_environment(start, environment)
    }

    fn set_environment_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: mm::domain::environment_delivery::EnvironmentDeliveryMode,
    ) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ConfigLoader.set_environment_delivery(start, environment, mode)
    }

    fn set_development_mount(
        &self,
        start: &Path,
        module: &str,
        mount: Option<&mm::domain::development_mount::DevelopmentMountConfig>,
        expected_source_hash: &str,
    ) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ConfigLoader.set_development_mount(start, module, mount, expected_source_hash)
    }
}

#[async_trait]
impl SecretMetadataInspector for FakeSecrets {
    async fn inspect(
        &self,
        _scope: &mm::domain::secret::SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Ok(secret_metadata())
    }
}

#[async_trait]
impl SecretMaterialResolver for FakeSecrets {
    async fn resolve(
        &self,
        _scope: &mm::domain::secret::SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Ok(SecretResolution {
            metadata: secret_metadata(),
            material: Some(
                SecretMaterial::new("never-store-this-value".to_owned())
                    .map_err(|message| MmError::Secret { message })?,
            ),
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for FailingSecretResolver {
    async fn resolve(
        &self,
        _scope: &mm::domain::secret::SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Err(MmError::Secret {
            message: "injected build secret resolution failure".to_owned(),
        })
    }
}

fn secret_metadata() -> SecretMetadata {
    SecretMetadata {
        status: SecretStatus::Set,
        revision: Some("revision-1".to_owned()),
        created_at_unix_ms: Some(1),
        candidate_revision: None,
    }
}

#[derive(Debug, Default)]
struct FakeBuilder {
    secret_seen: Mutex<bool>,
    fail_begin: bool,
    fail_cleanup: bool,
    fail_retag: bool,
    fail: bool,
    fail_module: Option<String>,
    built: Mutex<Vec<String>>,
    retagged: Mutex<Vec<String>>,
    received_args: Mutex<BTreeMap<String, BTreeMap<String, String>>>,
    received_platforms: Mutex<Vec<String>>,
    received_dependencies: Mutex<BTreeMap<String, Vec<mm::domain::build::ResolvedBuildDependency>>>,
    upstream_digest: Option<String>,
    retag_digest: Option<String>,
    begun: Arc<AtomicUsize>,
    preserved: Arc<AtomicUsize>,
    rolled_back: Arc<AtomicUsize>,
    cleaned: Arc<AtomicUsize>,
}

struct TrackingBuildGuard {
    preserved: bool,
    preserve_count: Arc<AtomicUsize>,
    rollback_count: Arc<AtomicUsize>,
}

impl BuildOperationGuard for TrackingBuildGuard {
    fn preserve(mut self: Box<Self>) {
        self.preserved = true;
        self.preserve_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for TrackingBuildGuard {
    fn drop(&mut self) {
        if !self.preserved {
            self.rollback_count.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[async_trait]
impl ImageBuilder for FakeBuilder {
    fn begin_operation(
        &self,
        _operation_id: OperationId,
    ) -> Result<Box<dyn BuildOperationGuard>, MmError> {
        self.begun.fetch_add(1, Ordering::SeqCst);
        if self.fail_begin {
            return Err(MmError::Build {
                message: "injected build operation start failure".to_owned(),
            });
        }
        Ok(Box::new(TrackingBuildGuard {
            preserved: false,
            preserve_count: self.preserved.clone(),
            rollback_count: self.rolled_back.clone(),
        }))
    }

    fn cleanup_operation(&self, _operation_id: OperationId) -> Result<(), MmError> {
        self.cleaned.fetch_add(1, Ordering::SeqCst);
        if self.fail_cleanup {
            return Err(MmError::Build {
                message: "injected build cleanup failure".to_owned(),
            });
        }
        Ok(())
    }

    async fn build_variant(
        &self,
        request: mm::app::ports::BuildVariantRequest<'_>,
    ) -> Result<BuiltImage, MmError> {
        let mm::app::ports::BuildVariantRequest {
            publish,
            unit,
            variant,
            secrets,
            dependencies,
            platforms,
            ..
        } = request;
        if self.fail || self.fail_module.as_deref() == Some(&unit.module) {
            return Err(MmError::Build {
                message: "injected build failure".to_owned(),
            });
        }
        let seen = secrets
            .get("gh_pat")
            .is_some_and(|secret| secret.expose(|value| value == "never-store-this-value"));
        *self.secret_seen.lock().map_err(|_| MmError::State {
            message: "fake builder lock was poisoned".to_owned(),
        })? = seen;
        self.built
            .lock()
            .map_err(|_| MmError::State {
                message: "fake builder lock was poisoned".to_owned(),
            })?
            .push(unit.module.clone());
        self.received_args
            .lock()
            .map_err(|_| MmError::State {
                message: "fake builder lock was poisoned".to_owned(),
            })?
            .insert(unit.module.clone(), unit.args.clone());
        *self.received_platforms.lock().map_err(|_| MmError::State {
            message: "fake builder lock was poisoned".to_owned(),
        })? = platforms.to_vec();
        self.received_dependencies
            .lock()
            .map_err(|_| MmError::State {
                message: "fake builder lock was poisoned".to_owned(),
            })?
            .insert(unit.module.clone(), dependencies.to_vec());
        let digest = if unit.module == "php" {
            self.upstream_digest
                .clone()
                .unwrap_or_else(|| digest_for("php"))
        } else {
            let canonical = serde_json::to_vec(&unit.args).map_err(|error| MmError::Build {
                message: format!("serializing fake build arguments: {error}"),
            })?;
            format!("sha256:{}", blake3::hash(&canonical).to_hex())
        };
        Ok(BuiltImage {
            module: unit.module.clone(),
            target: variant.target.clone(),
            repository: unit.image.clone(),
            tags: variant
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest,
            execution_fingerprint: String::new(),
            secret_revisions: BTreeMap::new(),
            archive: (!publish).then(|| std::path::PathBuf::from("/tmp/fake-image.oci.tar")),
            reused: false,
        })
    }

    async fn retag_variant(
        &self,
        _source_repository: &str,
        digest: &str,
        destination_repository: &str,
        _tags: &[String],
    ) -> Result<String, MmError> {
        if self.fail_retag {
            return Err(MmError::Build {
                message: "injected image retag failure".to_owned(),
            });
        }
        self.retagged
            .lock()
            .map_err(|_| MmError::State {
                message: "fake builder lock was poisoned".to_owned(),
            })?
            .push(destination_repository.to_owned());
        Ok(self
            .retag_digest
            .clone()
            .unwrap_or_else(|| digest.to_owned()))
    }
}

fn digest_for(module: &str) -> String {
    let suffix = if module == "php" { "aa" } else { "bb" };
    format!("sha256:{suffix}{}", "0".repeat(62))
}

#[derive(Debug)]
struct FakeLock;

#[derive(Debug)]
struct FailingLock;

#[derive(Debug)]
struct FakeGuard;

impl TargetOperationLockGuard for FakeGuard {}

impl BuildOperationLock for FakeLock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(FakeGuard))
    }
}

impl BuildOperationLock for FailingLock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Err(MmError::State {
            message: "injected build lock failure".to_owned(),
        })
    }
}

#[derive(Debug)]
struct MutatingLock {
    path: PathBuf,
    contents: Mutex<Option<String>>,
}

impl BuildOperationLock for MutatingLock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        let contents = self
            .contents
            .lock()
            .map_err(|_| MmError::State {
                message: "mutating lock was poisoned".to_owned(),
            })?
            .take()
            .ok_or_else(|| MmError::State {
                message: "mutating lock was acquired more than once".to_owned(),
            })?;
        fs::write(&self.path, contents).map_err(|source| MmError::Io {
            context: "mutating source after build lock acquisition".to_owned(),
            source,
        })?;
        Ok(Box::new(FakeGuard))
    }
}

struct Fixture {
    project: TempDir,
    state_directory: TempDir,
    state: Arc<SqliteStateStore>,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let project = temp_dirs::tempdir()?;
        fs::create_dir(project.path().join(".config"))?;
        fs::create_dir(project.path().join("app"))?;
        fs::write(
            project.path().join(".config/mm.yaml"),
            r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-service
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web:
        stack: app
    build:
      image: ghcr.io/example/app
      secrets:
        gh_pat: build.token
secrets:
  build.token:
    storage:
      provider: env
      location: BUILD_TEST_TOKEN
environments:
  local:
    bindings: {}
"#,
        )?;
        fs::write(
            project.path().join("compose.yaml"),
            "services:\n  web:\n    image: ghcr.io/example/app:old\n",
        )?;
        fs::write(project.path().join("app/Dockerfile"), "FROM scratch\n")?;
        fs::write(project.path().join("app/content.txt"), "content\n")?;
        let state_directory = temp_dirs::tempdir()?;
        let state = Arc::new(SqliteStateStore::open(
            &state_directory.path().join("state.sqlite3"),
        )?);
        Ok(Self {
            project,
            state_directory,
            state,
        })
    }

    fn service(&self, builder: Arc<FakeBuilder>) -> BuildService {
        self.service_with_lock(builder, Arc::new(FakeLock))
    }

    fn service_with_lock(
        &self,
        builder: Arc<FakeBuilder>,
        locks: Arc<dyn BuildOperationLock>,
    ) -> BuildService {
        self.service_with_source_and_lock(builder, Arc::new(FakeSource), locks)
    }

    fn service_with_source_and_lock(
        &self,
        builder: Arc<FakeBuilder>,
        source: Arc<dyn BuildSourceInspector>,
        locks: Arc<dyn BuildOperationLock>,
    ) -> BuildService {
        self.service_with_source_lock_and_resolver(builder, source, locks, Arc::new(FakeSecrets))
    }

    fn service_with_source_lock_and_resolver(
        &self,
        builder: Arc<FakeBuilder>,
        source: Arc<dyn BuildSourceInspector>,
        locks: Arc<dyn BuildOperationLock>,
        secret_resolver: Arc<dyn SecretMaterialResolver>,
    ) -> BuildService {
        let state: Arc<dyn StateStore> = self.state.clone();
        BuildService::new(
            BuildManager::new(Arc::new(ConfigLoader), source),
            BuildExecutionPorts {
                documents: Arc::new(ConfigLoader),
                state,
                builder,
                secret_inspector: Arc::new(FakeSecrets),
                secret_resolver,
                locks,
                clock: Arc::new(FixedClock { unix_ms: 1000 }),
            },
        )
    }

    fn service_with_port_documents(
        &self,
        builder: Arc<FakeBuilder>,
        documents: Arc<dyn ProjectDocumentStore>,
    ) -> BuildService {
        let state: Arc<dyn StateStore> = self.state.clone();
        BuildService::new(
            BuildManager::new(Arc::new(ConfigLoader), Arc::new(FakeSource)),
            BuildExecutionPorts {
                documents,
                state,
                builder,
                secret_inspector: Arc::new(FakeSecrets),
                secret_resolver: Arc::new(FakeSecrets),
                locks: Arc::new(FakeLock),
                clock: Arc::new(FixedClock { unix_ms: 1000 }),
            },
        )
    }

    fn workspace_id(&self) -> Result<mm::domain::workspace::WorkspaceId, MmError> {
        let document = ConfigLoader.load_from(self.project.path())?;
        Ok(self
            .state
            .register_workspace(
                &WorkspaceRegistration {
                    project_id: document.config.project.id,
                    root: document.root,
                    config_path: document.path,
                },
                1000,
            )?
            .id)
    }

    fn stacks(&self) -> Result<mm::app::StackManager, MmError> {
        use mm::app::ports::TargetStore;
        use mm::domain::target::{TargetDefinition, TargetId};

        let targets = Arc::new(mm::adapters::target::FilesystemTargetStore::new(
            self.state_directory.path().join("targets.yaml"),
        ));
        targets.insert_target(
            "local",
            TargetDefinition::local(TargetId(uuid::Uuid::new_v4())),
        )?;
        Ok(mm::app::StackManager::new(
            Arc::new(ConfigLoader),
            Arc::new(mm::adapters::stack::FilesystemStackArtifactStore::new(
                self.state_directory.path().join("compose"),
            )),
            targets,
        )
        .with_state(self.state.clone()))
    }
}

#[tokio::test]
async fn local_override_changes_invalidate_review_and_reach_the_builder()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let base = service.plan(&selection, true).await?;
    let path = fixture.project.path().join(".config/mm.overrides.yaml");
    fs::write(&path, "build: {platforms: [linux/arm64]}\n")?;
    assert!(
        service
            .apply_reviewed(&selection, true, &base.plan_hash)
            .await
            .is_err()
    );
    let arm = service.plan(&selection, true).await?;
    assert_eq!(arm.platforms, ["linux/arm64"]);
    service
        .apply_reviewed(&selection, true, &arm.plan_hash)
        .await?;
    assert_eq!(
        *builder
            .received_platforms
            .lock()
            .map_err(|_| "fake builder lock")?,
        ["linux/arm64"]
    );
    fs::write(&path, "build: {platforms: [linux/amd64]}\n")?;
    assert!(
        service
            .apply_reviewed(&selection, true, &arm.plan_hash)
            .await
            .is_err()
    );
    let overridden = service.plan(&selection, true).await?;
    fs::remove_file(path)?;
    assert!(
        service
            .apply_reviewed(&selection, true, &overridden.plan_hash)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn runtime_configuration_edits_keep_published_digests_and_refresh_deployment_artifacts()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let mut selection = WorkspaceSelection::new(fixture.project.path(), None);
    selection.runtime = Some(mm::domain::runtime::RuntimeKind::DockerCompose);
    let plan = service.plan(&selection, true).await?;
    let result = service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let stacks = fixture.stacks()?;
    let before = stacks.compile(&selection)?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        fixture.project.path().join("app/.env.example"),
        "LOG_LEVEL=info # mm:from=LOG.LEVEL\n",
    )?;
    fs::write(
        &config_path,
        format!(
            "# A runtime-only edit must not require publication.\n{}",
            config
                .replace("    path: app", "    path: app\n    env: {template: .env.example}")
                .replace("        stack: app", "        stack: app\n        env: {LOG_LEVEL: debug}")
                .replace("    files: [compose.yaml]", "    files: [compose.yaml]\n    ingress: {domain: web, service: web, port: 80, https: false}")
                .replace("    bindings: {}", "    bindings: {LOG.LEVEL: {value: debug}}\n    domains: {web: changed.example.test}")
        ),
    )?;
    let document = ConfigLoader.load_from(fixture.project.path())?;
    assert_ne!(document.source_hash, plan.config_hash);
    let validation = mm::adapters::config::validate(&document, "local");
    assert!(validation.is_valid(), "{:?}", validation.diagnostics);

    let after = stacks.compile(&selection)?;
    assert_eq!(
        after.stacks[0].manifest.services[0].image,
        Some(format!(
            "{}@{}",
            result.images[0].repository, result.images[0].digest
        ))
    );
    assert_eq!(
        before.stacks[0].manifest.services[0].image,
        after.stacks[0].manifest.services[0].image
    );
    assert_ne!(
        before.stacks[0].manifest.artifact_hash,
        after.stacks[0].manifest.artifact_hash
    );
    // Routes reach Traefik through dynamic configuration, not Compose labels:
    // the edited host lands in the artifact's routes, and the overlay attaches
    // the routed service to the ingress network under its backend alias.
    let route = after.stacks[0]
        .manifest
        .routes
        .iter()
        .find(|route| route.service == "web")
        .ok_or("edited route missing from the artifact")?;
    assert_eq!(route.host, "changed.example.test");
    let overlay = fs::read_to_string(&after.stacks[0].generated_overlay_path)?;
    assert!(overlay.contains(&route.backend_alias), "{overlay}");

    let readiness = mm::app::ReadinessService::new(
        Arc::new(ConfigLoader),
        Arc::new(mm::adapters::state::SqliteStateInspector {
            path: fixture.state_directory.path().join("state.sqlite3"),
        }),
        Arc::new(mm::adapters::target::FilesystemTargetStore::new(
            fixture.state_directory.path().join("targets.yaml"),
        )),
        Vec::new(),
    )
    .inspect(&selection)
    .await;
    let checks = readiness.readiness.ok_or("missing readiness")?.checks;
    for name in ["images", "stacks"] {
        let check = checks
            .iter()
            .find(|check| check.name == name)
            .ok_or("missing check")?;
        assert_eq!(
            check.status,
            mm::domain::readiness::CheckStatus::Passed,
            "{check:?}"
        );
    }
    assert_eq!(builder.built.lock().map_err(|_| "lock poisoned")?.len(), 1);
    assert_eq!(
        fixture
            .state
            .recent_operations(fixture.workspace_id()?, 10)?
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn changed_build_settings_require_images_only_for_affected_services()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::readiness::SetupIssue;

    let fixture = graph_fixture()?;
    let service = fixture.service(Arc::new(FakeBuilder::default()));
    let mut selection = WorkspaceSelection::new(fixture.project.path(), None);
    selection.runtime = Some(mm::domain::runtime::RuntimeKind::DockerCompose);
    let plan = service.plan(&selection, true).await?;
    service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let stacks = fixture.stacks()?;
    stacks.compile(&selection)?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        fixture.project.path().join("build-trigger.txt"),
        "trigger\n",
    )?;
    let changes = [
        ("image: ghcr.io/example/web", "image: ghcr.io/example/other"),
        (
            "image: ghcr.io/example/web",
            "image: ghcr.io/example/web\n      args: {FEATURE: enabled}",
        ),
        ("path: web", "path: php"),
        (
            "image: ghcr.io/example/web",
            "image: ghcr.io/example/web\n      context: src",
        ),
        (
            "image: ghcr.io/example/web",
            "image: ghcr.io/example/web\n      dockerfile: Other.Dockerfile",
        ),
        (
            "image: ghcr.io/example/web",
            "image: ghcr.io/example/web\n      extra-inputs: [build-trigger.txt]",
        ),
        ("php: MM_PHP_IMAGE", "php: OTHER_IMAGE"),
        (
            "image: ghcr.io/example/web",
            "image: ghcr.io/example/web\n      targets: {production: prod}",
        ),
        // A base with no services must invalidate its dependent application.
        (
            "image: ghcr.io/example/php",
            "image: ghcr.io/example/php-v2",
        ),
        (
            "image: ghcr.io/example/php",
            "image: ghcr.io/example/php\n      args: {FEATURE: enabled}",
        ),
        (
            "default-environment: local",
            "build: {platforms: [linux/amd64]}\ndefault-environment: local",
        ),
        // A new module has no result in the existing published build.
        (
            "  php:\n",
            "  new:\n    path: php\n    build: {image: ghcr.io/example/new}\n    services: {worker: {stack: app}}\n  php:\n",
        ),
    ];
    for (old, new) in changes {
        fs::write(&config_path, config.replace(old, new))?;
        let error = stacks
            .compile(&selection)
            .err()
            .ok_or("changed build was accepted")?;
        let expected = if new.contains("services: {worker:") {
            "new/worker"
        } else {
            "web/web"
        };
        assert!(
            matches!(error,
                MmError::Setup { issue: SetupIssue::MissingImages { ref services } }
                    if services == &[expected.to_owned()]
            ),
            "{old} -> {new}: {error}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn published_image_lookup_preserves_scope_and_skips_failed_and_archive_builds()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let service = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = service.plan(&selection, true).await?;
    let published = service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let archive = service.plan(&selection, false).await?;
    service
        .apply_reviewed(&selection, false, &archive.plan_hash)
        .await?;
    let failing = fixture.service(Arc::new(FakeBuilder {
        fail: true,
        ..FakeBuilder::default()
    }));
    // Secret-backed builds cannot reuse the earlier image, so this attempt fails.
    assert!(
        failing
            .apply_reviewed(&selection, true, &plan.plan_hash)
            .await
            .is_err()
    );
    let document = ConfigLoader.load_from(fixture.project.path())?;
    let (_, result) = fixture
        .state
        .latest_successful_image_build(document.config.project.id, &document.root, "local")?
        .ok_or("missing published build")?;
    assert_eq!(result.operation_id, published.operation_id);
    for (project_id, root, environment) in [
        (
            document.config.project.id,
            document.root.as_path(),
            "production",
        ),
        (
            document.config.project.id,
            fixture.state_directory.path(),
            "local",
        ),
        (
            ProjectId(uuid::Uuid::new_v4()),
            document.root.as_path(),
            "local",
        ),
    ] {
        assert!(
            fixture
                .state
                .latest_successful_image_build(project_id, root, environment)?
                .is_none()
        );
    }
    Ok(())
}

#[tokio::test]
async fn reviewed_build_is_journaled_without_secret_material()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;

    let result = service
        .apply_reviewed(&selection, false, &plan.plan_hash)
        .await?;

    assert_eq!(result.images.len(), 1);
    assert!(*builder.secret_seen.lock().map_err(|_| "lock poisoned")?);
    assert_eq!(builder.begun.load(Ordering::SeqCst), 1);
    assert_eq!(builder.preserved.load(Ordering::SeqCst), 1);
    assert_eq!(builder.rolled_back.load(Ordering::SeqCst), 0);
    let operations = fixture
        .state
        .recent_operations(fixture.workspace_id()?, 10)?;
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].kind, OperationKind::Build);
    assert_eq!(operations[0].state, OperationState::Succeeded);

    let connection =
        rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
    let (plan_json, result_json): (String, String) = connection.query_row(
        "SELECT plan_json, result_json FROM image_build_snapshots",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert!(!plan_json.contains("never-store-this-value"));
    assert!(!result_json.contains("never-store-this-value"));
    assert!(!plan_json.contains("BUILD_TEST_TOKEN"));
    Ok(())
}

#[tokio::test]
async fn changed_secret_provider_binding_requires_a_new_review()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let service = fixture.service(Arc::new(FakeBuilder::default()));
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace("BUILD_TEST_TOKEN", "OTHER_BUILD_TEST_TOKEN"),
    )?;

    let current = service.plan(&selection, false).await?;
    assert_ne!(current.config_hash, plan.config_hash);
    assert_ne!(current.plan_hash, plan.plan_hash);
    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));
    assert!(
        fixture
            .state
            .recent_operations(fixture.workspace_id()?, 10)?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn changed_context_and_builder_failure_cannot_succeed()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let service = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = service.plan(&selection, false).await?;
    fs::write(fixture.project.path().join("app/content.txt"), "changed\n")?;

    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));
    assert!(
        fixture
            .state
            .recent_operations(fixture.workspace_id()?, 10)?
            .is_empty()
    );

    let failing_builder = Arc::new(FakeBuilder {
        fail: true,
        ..FakeBuilder::default()
    });
    let failing = fixture.service(failing_builder.clone());
    let new_plan = failing.plan(&selection, false).await?;
    assert!(matches!(
        failing
            .apply_reviewed(&selection, false, &new_plan.plan_hash)
            .await,
        Err(MmError::Build { .. })
    ));
    let operations = fixture
        .state
        .recent_operations(fixture.workspace_id()?, 10)?;
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].state, OperationState::Failed);
    assert_eq!(failing_builder.begun.load(Ordering::SeqCst), 1);
    assert_eq!(failing_builder.preserved.load(Ordering::SeqCst), 0);
    assert_eq!(failing_builder.rolled_back.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn source_changed_while_acquiring_the_build_lock_requires_a_new_review()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service_with_lock(
        builder.clone(),
        Arc::new(MutatingLock {
            path: fixture.project.path().join("app/content.txt"),
            contents: Mutex::new(Some("changed after preview\n".to_owned())),
        }),
    );
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;

    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));
    assert_eq!(builder.begun.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn configuration_changed_after_the_second_plan_fails_before_execution()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let replacement =
        fs::read_to_string(&config_path)?.replace("BUILD_TEST_TOKEN", "CHANGED_BUILD_TEST_TOKEN");
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service_with_source_and_lock(
        builder.clone(),
        Arc::new(MutatingSource {
            inspections: AtomicUsize::new(0),
            config_path,
            replacement,
        }),
        Arc::new(FakeLock),
    );
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;

    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::Security { .. })
    ));
    assert_eq!(builder.begun.load(Ordering::SeqCst), 0);
    Ok(())
}

fn graph_fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    fs::create_dir(project.path().join(".config"))?;
    fs::create_dir(project.path().join("php"))?;
    fs::create_dir(project.path().join("web"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-graph
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  php:
    path: php
    build:
      image: ghcr.io/example/php
  web:
    path: web
    services:
      web:
        stack: app
    build:
      image: ghcr.io/example/web
      depends-on:
        php: MM_PHP_IMAGE
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    expose: ['80']\n",
    )?;
    fs::write(project.path().join("php/Dockerfile"), "FROM scratch\n")?;
    fs::write(
        project.path().join("web/Dockerfile"),
        "ARG MM_PHP_IMAGE\nFROM ${MM_PHP_IMAGE}\n",
    )?;
    let state_directory = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(
        &state_directory.path().join("state.sqlite3"),
    )?);
    Ok(Fixture {
        project,
        state_directory,
        state,
    })
}

fn inferred_graph_fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        config_path,
        config.replace("      depends-on:\n        php: MM_PHP_IMAGE\n", ""),
    )?;
    fs::write(
        fixture.project.path().join("web/Dockerfile"),
        "FrOm --platform=$TARGETPLATFORM ghcr.io/example/php:mutable AS runtime\n",
    )?;
    Ok(fixture)
}

#[tokio::test]
async fn apply_injects_upstream_digests_in_dependency_order()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, true).await?;
    assert_eq!(
        plan.units
            .iter()
            .map(|unit| unit.module.as_str())
            .collect::<Vec<_>>(),
        vec!["php", "web"]
    );

    let result = service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;

    assert_eq!(
        builder
            .built
            .lock()
            .map_err(|_| "lock poisoned")?
            .as_slice(),
        ["php", "web"]
    );
    let web_args = builder
        .received_args
        .lock()
        .map_err(|_| "lock poisoned")?
        .get("web")
        .cloned()
        .ok_or("web args missing")?;
    let expected = format!("ghcr.io/example/php@{}", digest_for("php"));
    assert_eq!(web_args.get("MM_PHP_IMAGE"), Some(&expected));
    assert_eq!(result.dependencies.len(), 1);
    assert_eq!(result.dependencies[0].edge.producer_module, "php");
    assert_eq!(result.dependencies[0].edge.dependent_module, "web");
    assert_eq!(result.dependencies[0].digest, digest_for("php"));
    Ok(())
}

#[tokio::test]
async fn inferred_from_is_audited_and_rewritten_to_the_new_digest()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = inferred_graph_fixture()?;
    let dockerfile = fixture.project.path().join("web/Dockerfile");
    let source_before = fs::read(&dockerfile)?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, true).await?;

    let result = service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;

    let edge = &plan.units[1].dependencies[0];
    assert!(matches!(
        edge.reference,
        mm::domain::build::BuildDependencyReference::Dockerfile { .. }
    ));
    let received = builder
        .received_dependencies
        .lock()
        .map_err(|_| "lock poisoned")?;
    let web = received.get("web").ok_or("web dependencies missing")?;
    assert_eq!(web.len(), 1);
    assert_eq!(
        web[0].immutable_reference,
        format!("ghcr.io/example/php@{}", digest_for("php"))
    );
    assert_eq!(result.dependencies, *web);
    assert_eq!(fs::read(&dockerfile)?, source_before);
    Ok(())
}

#[tokio::test]
async fn dependent_failure_never_modifies_the_source_dockerfile()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = inferred_graph_fixture()?;
    let dockerfile = fixture.project.path().join("web/Dockerfile");
    let before = fs::read(&dockerfile)?;
    let service = fixture.service(Arc::new(FakeBuilder {
        fail_module: Some("web".to_owned()),
        ..FakeBuilder::default()
    }));
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, true).await?;

    assert!(matches!(
        service
            .apply_reviewed(&selection, true, &plan.plan_hash)
            .await,
        Err(MmError::Build { .. })
    ));
    assert_eq!(fs::read(&dockerfile)?, before);
    Ok(())
}

#[tokio::test]
async fn upstream_digest_changes_downstream_execution_identity_and_output()
-> Result<(), Box<dyn std::error::Error>> {
    async fn execute(
        upstream_digest: String,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let fixture = graph_fixture()?;
        let builder = Arc::new(FakeBuilder {
            upstream_digest: Some(upstream_digest),
            ..FakeBuilder::default()
        });
        let service = fixture.service(builder);
        let selection = WorkspaceSelection::new(fixture.project.path(), None);
        let plan = service.plan(&selection, true).await?;
        let result = service
            .apply_reviewed(&selection, true, &plan.plan_hash)
            .await?;
        let web = result
            .images
            .iter()
            .find(|image| image.module == "web")
            .ok_or("web image missing")?;
        Ok((web.execution_fingerprint.clone(), web.digest.clone()))
    }

    let first = execute(format!("sha256:{}", "a".repeat(64))).await?;
    let second = execute(format!("sha256:{}", "c".repeat(64))).await?;
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
    Ok(())
}

#[tokio::test]
async fn canonical_local_builds_execute_once_without_cross_repository_retagging()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config
            .replace("    path: web\n", "    path: php\n")
            .replace("      depends-on:\n        php: MM_PHP_IMAGE\n", ""),
    )?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;
    assert_eq!(
        plan.units[0].input_fingerprint,
        plan.units[1].input_fingerprint
    );

    let result = service
        .apply_reviewed(&selection, false, &plan.plan_hash)
        .await?;

    assert_eq!(builder.built.lock().map_err(|_| "lock poisoned")?.len(), 1);
    assert!(
        builder
            .retagged
            .lock()
            .map_err(|_| "lock poisoned")?
            .is_empty()
    );
    assert_eq!(result.images.len(), 2);
    assert_eq!(result.images.iter().filter(|image| image.reused).count(), 1);
    Ok(())
}

#[tokio::test]
async fn canonical_published_builds_execute_once_and_retag_the_reviewed_repository()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config
            .replace("    path: web\n", "    path: php\n")
            .replace("      depends-on:\n        php: MM_PHP_IMAGE\n", ""),
    )?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, true).await?;

    let result = service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;

    assert_eq!(builder.built.lock().map_err(|_| "lock poisoned")?.len(), 1);
    assert_eq!(
        builder
            .retagged
            .lock()
            .map_err(|_| "lock poisoned")?
            .as_slice(),
        ["ghcr.io/example/web"]
    );
    assert_eq!(result.images.iter().filter(|image| image.reused).count(), 1);
    Ok(())
}

#[tokio::test]
async fn secret_backed_published_builds_are_never_reused() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let first = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = first.plan(&selection, true).await?;
    first
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;

    let second_builder = Arc::new(FakeBuilder::default());
    let second = fixture.service(second_builder.clone());
    let result = second
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;

    assert_eq!(
        second_builder
            .built
            .lock()
            .map_err(|_| "lock poisoned")?
            .len(),
        1
    );
    assert!(
        second_builder
            .retagged
            .lock()
            .map_err(|_| "lock poisoned")?
            .is_empty()
    );
    assert!(!result.images[0].reused);
    Ok(())
}

#[tokio::test]
async fn credential_only_secrets_are_mounted_but_keep_published_builds_reusable()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let secret_backed = fixture
        .service(Arc::new(FakeBuilder::default()))
        .plan(&selection, true)
        .await?;
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace(
            "        gh_pat: build.token\n",
            "        gh_pat: build.token\n      credential-only-secrets: [gh_pat]\n",
        ),
    )?;

    let first_builder = Arc::new(FakeBuilder::default());
    let first = fixture.service(first_builder.clone());
    let plan = first.plan(&selection, true).await?;
    let unit = &plan.units[0];
    assert_eq!(unit.secret_ids, ["gh_pat"]);
    assert_eq!(unit.credential_secret_ids, ["gh_pat"]);
    assert_eq!(
        unit.reuse_policy,
        mm::domain::build::BuildReusePolicy::Allowed
    );
    assert_ne!(
        unit.input_fingerprint,
        secret_backed.units[0].input_fingerprint
    );
    first
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    // The token still reaches BuildKit.
    assert!(
        *first_builder
            .secret_seen
            .lock()
            .map_err(|_| "lock poisoned")?
    );

    let second_builder = Arc::new(FakeBuilder::default());
    let again = fixture
        .service(second_builder.clone())
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    assert!(
        second_builder
            .built
            .lock()
            .map_err(|_| "lock poisoned")?
            .is_empty()
    );
    assert!(
        again
            .images
            .iter()
            .all(|image| image.reused && image.secret_revisions.is_empty())
    );
    Ok(())
}

/// A build service for the same checkout with its own empty state, like a
/// fresh CI runner or a deployment host.
fn fresh_service(
    builder: Arc<FakeBuilder>,
) -> Result<(BuildService, Arc<SqliteStateStore>, TempDir), Box<dyn std::error::Error>> {
    let directory = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(
        &directory.path().join("state.sqlite3"),
    )?);
    let port: Arc<dyn StateStore> = state.clone();
    let service = BuildService::new(
        BuildManager::new(Arc::new(ConfigLoader), Arc::new(FakeSource)),
        BuildExecutionPorts {
            documents: Arc::new(ConfigLoader),
            state: port,
            builder,
            secret_inspector: Arc::new(FakeSecrets),
            secret_resolver: Arc::new(FakeSecrets),
            locks: Arc::new(FakeLock),
            clock: Arc::new(FixedClock { unix_ms: 2000 }),
        },
    );
    Ok((service, state, directory))
}

#[tokio::test]
async fn cache_only_import_lets_a_fresh_runner_reuse_unchanged_images()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let first = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = first.plan(&selection, true).await?;
    let published = first
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let receipt = first.receipt(&selection)?;

    let builder = Arc::new(FakeBuilder::default());
    let (runner, state, _directory) = fresh_service(builder.clone())?;
    let review = runner.review_import(&selection, &receipt, true).await?;
    assert!(review.cache_only);
    assert_eq!(review.build_plan_hash, None);
    assert_eq!(review.source_environment, "local");
    assert!(
        review
            .images
            .iter()
            .all(|image| image.reuse_fingerprint.is_some())
    );
    assert!(
        runner
            .apply_import(&selection, &receipt, true, &"0".repeat(64))
            .await
            .is_err()
    );
    let applied = runner
        .apply_import(&selection, &receipt, true, &review.plan_hash)
        .await?;
    assert_eq!(applied, review);

    // Nothing is deployable from a cache-only import.
    let document = ConfigLoader.load_from(fixture.project.path())?;
    assert!(
        state
            .latest_successful_image_build(document.config.project.id, &document.root, "local")?
            .is_none()
    );
    assert!(runner.receipt(&selection).is_err());

    // The fresh runner retags the unchanged images instead of building them.
    let rebuilt = runner
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    assert!(
        builder
            .built
            .lock()
            .map_err(|_| "lock poisoned")?
            .is_empty()
    );
    assert_eq!(
        builder.retagged.lock().map_err(|_| "lock poisoned")?.len(),
        2
    );
    assert!(rebuilt.images.iter().all(|image| image.reused));
    assert_eq!(
        rebuilt
            .images
            .iter()
            .map(|image| &image.digest)
            .collect::<Vec<_>>(),
        published
            .images
            .iter()
            .map(|image| &image.digest)
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
async fn import_records_a_promoted_receipt_as_the_environments_published_build()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    fs::write(
        &config_path,
        format!(
            "{}  production:\n    bindings: {{}}\n",
            fs::read_to_string(&config_path)?
        ),
    )?;
    let local = WorkspaceSelection::new(fixture.project.path(), None);
    let production = WorkspaceSelection::new(fixture.project.path(), Some("production".to_owned()));
    let first = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = first.plan(&local, true).await?;
    let published = first.apply_reviewed(&local, true, &plan.plan_hash).await?;
    let receipt = first.receipt(&local)?;

    let (host, state, _directory) = fresh_service(Arc::new(FakeBuilder::default()))?;
    let review = host.review_import(&production, &receipt, false).await?;
    assert!(!review.cache_only);
    assert_eq!(review.source_environment, "local");
    assert_eq!(review.environment, "production");
    assert!(review.build_plan_hash.is_some());
    host.apply_import(&production, &receipt, false, &review.plan_hash)
        .await?;

    // Deploy plans, status, and `mm build receipt` now resolve the receipt's
    // digests for production.
    let document = ConfigLoader.load_from(fixture.project.path())?;
    let (recorded_plan, recorded) = state
        .latest_successful_image_build(document.config.project.id, &document.root, "production")?
        .ok_or("the import was not recorded")?;
    assert_eq!(
        Some(&recorded_plan.plan_hash),
        review.build_plan_hash.as_ref()
    );
    assert_eq!(recorded_plan.environment, "production");
    assert_eq!(recorded.images.len(), published.images.len());
    for (recorded, published) in recorded.images.iter().zip(&published.images) {
        assert_eq!(recorded.digest, published.digest);
    }
    let rewritten = host.receipt(&production)?;
    assert_eq!(rewritten.plan.environment, "production");

    // A module that builds differently here cannot take the receipt's image.
    fs::write(
        &config_path,
        fs::read_to_string(&config_path)?.replace(
            "      image: ghcr.io/example/web\n",
            "      image: ghcr.io/example/web\n      args: {FEATURE: on}\n",
        ),
    )?;
    let refused = host.review_import(&production, &receipt, false).await;
    assert!(
        matches!(&refused, Err(MmError::Build { message })
            if message.contains("module 'web'") && message.contains("build argument FEATURE")),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn secret_backed_receipts_neither_promote_nor_seed_reuse()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    fs::write(
        &config_path,
        format!(
            "{}  production:\n    bindings: {{}}\n",
            fs::read_to_string(&config_path)?
        ),
    )?;
    let local = WorkspaceSelection::new(fixture.project.path(), None);
    let production = WorkspaceSelection::new(fixture.project.path(), Some("production".to_owned()));
    let first = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = first.plan(&local, true).await?;
    first.apply_reviewed(&local, true, &plan.plan_hash).await?;
    let receipt = first.receipt(&local)?;

    let (host, _state, _directory) = fresh_service(Arc::new(FakeBuilder::default()))?;
    let refused = host.review_import(&production, &receipt, false).await;
    assert!(
        matches!(&refused, Err(MmError::Build { message })
            if message.contains("module 'app'") && message.contains("gh_pat")),
        "{refused:?}"
    );
    // Its own environment still records it, but never for reuse.
    let review = host.review_import(&local, &receipt, false).await?;
    assert!(
        review
            .images
            .iter()
            .all(|image| image.reuse_fingerprint.is_none())
    );
    let cache = host.review_import(&production, &receipt, true).await?;
    assert!(
        cache
            .images
            .iter()
            .all(|image| image.reuse_fingerprint.is_none())
    );
    Ok(())
}

#[tokio::test]
async fn unchanged_publish_retags_instead_of_rebuilding() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = graph_fixture()?;
    let first_builder = Arc::new(FakeBuilder::default());
    let first = fixture.service(first_builder);
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = first.plan(&selection, true).await?;
    first
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;

    let second_builder = Arc::new(FakeBuilder::default());
    let second = fixture.service(second_builder.clone());
    let again = second
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;

    assert!(
        second_builder
            .built
            .lock()
            .map_err(|_| "lock poisoned")?
            .is_empty()
    );
    assert_eq!(
        second_builder
            .retagged
            .lock()
            .map_err(|_| "lock poisoned")?
            .len(),
        2
    );
    assert!(again.images.iter().all(|image| image.reused));
    Ok(())
}

#[tokio::test]
async fn pre_release_v2_snapshots_are_rejected_instead_of_reused()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, true).await?;
    service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let connection =
        rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
    connection.execute(
        "UPDATE image_build_snapshots
         SET plan_json = replace(plan_json, 'mm-image-build-plan-v1', 'mm-image-build-plan-v2')",
        [],
    )?;
    let document = ConfigLoader.load_from(fixture.project.path())?;

    assert!(
        fixture
            .state
            .latest_successful_image_build(document.config.project.id, &document.root, "local",)
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn interrupted_builds_are_cleaned_and_journaled_before_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let workspace_id = fixture.workspace_id()?;
    let interrupted = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id: interrupted,
        workspace_id,
        kind: OperationKind::Build,
        environment: "local".to_owned(),
        requested_at_unix_ms: 1,
        plan_hash: Some("abandoned".to_owned()),
    })?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;

    service
        .apply_reviewed(&selection, false, &plan.plan_hash)
        .await?;

    let operations = fixture.state.recent_operations(workspace_id, 10)?;
    assert_eq!(operations.len(), 2);
    assert_eq!(operations[0].state, OperationState::Succeeded);
    assert_eq!(operations[1].id, interrupted);
    assert_eq!(operations[1].state, OperationState::Interrupted);
    assert_eq!(builder.cleaned.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn non_build_operations_are_not_interrupted_by_builds()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let workspace_id = fixture.workspace_id()?;
    let deployment = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id: deployment,
        workspace_id,
        kind: OperationKind::Deploy,
        environment: "local".to_owned(),
        requested_at_unix_ms: 1,
        plan_hash: Some("deployment".to_owned()),
    })?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;
    service
        .apply_reviewed(&selection, false, &plan.plan_hash)
        .await?;

    let operations = fixture.state.recent_operations(workspace_id, 10)?;
    let deployment = operations
        .iter()
        .find(|operation| operation.id == deployment)
        .ok_or("deployment operation missing")?;
    assert_eq!(deployment.state, OperationState::Running);
    assert_eq!(builder.cleaned.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn reusable_image_repository_and_digest_tampering_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let first = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = first.plan(&selection, true).await?;
    first
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let connection =
        rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
    connection.execute(
        "UPDATE image_build_unit_cache SET repository = 'attacker.example/foreign'",
        [],
    )?;

    let repository_error = fixture
        .service(Arc::new(FakeBuilder::default()))
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await;
    assert!(matches!(repository_error, Err(MmError::Security { .. })));

    connection.execute(
        "UPDATE image_build_unit_cache SET repository = CASE module
         WHEN 'php' THEN 'ghcr.io/example/php' ELSE 'ghcr.io/example/web' END",
        [],
    )?;
    let digest_error = fixture
        .service(Arc::new(FakeBuilder {
            retag_digest: Some(format!("sha256:{}", "f".repeat(64))),
            ..FakeBuilder::default()
        }))
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await;
    assert!(matches!(digest_error, Err(MmError::Security { .. })));
    Ok(())
}

#[tokio::test]
async fn reusable_cache_rejournal_failure_rolls_back_the_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let first = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = first.plan(&selection, true).await?;
    first
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let connection =
        rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
    connection.execute_batch(
        "CREATE TRIGGER fail_reusable_record BEFORE INSERT ON image_build_unit_cache
         BEGIN SELECT RAISE(ABORT, 'injected reusable cache failure'); END;",
    )?;
    let builder = Arc::new(FakeBuilder::default());
    let retry = fixture.service(builder.clone());

    assert!(matches!(
        retry
            .apply_reviewed(&selection, true, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));
    assert!(
        builder
            .built
            .lock()
            .map_err(|_| "lock poisoned")?
            .is_empty()
    );
    assert_eq!(
        builder.retagged.lock().map_err(|_| "lock poisoned")?.len(),
        1
    );
    assert_eq!(builder.rolled_back.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn operation_start_verify_and_success_finish_failures_roll_back_exactly_once()
-> Result<(), Box<dyn std::error::Error>> {
    async fn run(
        builder: Arc<FakeBuilder>,
        trigger: Option<&str>,
    ) -> Result<MmError, Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let service = fixture.service(builder);
        let selection = WorkspaceSelection::new(fixture.project.path(), None);
        let plan = service.plan(&selection, false).await?;
        if let Some(trigger) = trigger {
            let connection =
                rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
            connection.execute_batch(trigger)?;
        }
        match service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await
        {
            Err(error) => Ok(error),
            Ok(_) => Err("injected execution boundary failure unexpectedly succeeded".into()),
        }
    }

    let start = Arc::new(FakeBuilder {
        fail_begin: true,
        ..FakeBuilder::default()
    });
    assert!(matches!(
        run(start.clone(), None).await?,
        MmError::Build { .. }
    ));
    assert_eq!(start.begun.load(Ordering::SeqCst), 1);
    assert_eq!(start.rolled_back.load(Ordering::SeqCst), 0);

    let verify = Arc::new(FakeBuilder::default());
    let verify_error = run(
        verify.clone(),
        Some(
            "CREATE TRIGGER fail_build_verify BEFORE INSERT ON operation_events
             WHEN NEW.payload_json LIKE '%\"phase\":\"verify\"%'
             BEGIN SELECT RAISE(ABORT, 'injected verify journal failure'); END;",
        ),
    )
    .await?;
    assert!(matches!(verify_error, MmError::State { .. }));
    assert_eq!(verify.rolled_back.load(Ordering::SeqCst), 1);

    let finish = Arc::new(FakeBuilder::default());
    let finish_error = run(
        finish.clone(),
        Some(
            "CREATE TRIGGER fail_success_finish BEFORE UPDATE ON operations
             WHEN NEW.state = 'succeeded'
             BEGIN SELECT RAISE(ABORT, 'injected success finish failure'); END;",
        ),
    )
    .await?;
    assert!(matches!(finish_error, MmError::State { .. }));
    assert_eq!(finish.preserved.load(Ordering::SeqCst), 0);
    assert_eq!(finish.rolled_back.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn plan_and_result_journal_failures_never_report_success()
-> Result<(), Box<dyn std::error::Error>> {
    async fn execute_with_trigger(
        trigger: &str,
    ) -> Result<(MmError, OperationState, usize, usize), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let builder = Arc::new(FakeBuilder::default());
        let service = fixture.service(builder.clone());
        let selection = WorkspaceSelection::new(fixture.project.path(), None);
        let plan = service.plan(&selection, false).await?;
        let connection =
            rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
        connection.execute_batch(trigger)?;
        let error = match service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await
        {
            Err(error) => error,
            Ok(_) => return Err("injected journal failure must fail the build".into()),
        };
        let state = fixture
            .state
            .recent_operations(fixture.workspace_id()?, 1)?
            .first()
            .ok_or("operation missing")?
            .state;
        Ok((
            error,
            state,
            builder.begun.load(Ordering::SeqCst),
            builder.rolled_back.load(Ordering::SeqCst),
        ))
    }

    let (plan_error, plan_state, plan_begun, plan_rollback) = execute_with_trigger(
        "CREATE TRIGGER fail_build_plan BEFORE INSERT ON image_build_snapshots
         BEGIN SELECT RAISE(ABORT, 'injected plan journal failure'); END;",
    )
    .await?;
    assert!(matches!(plan_error, MmError::State { .. }));
    assert_eq!(plan_state, OperationState::Failed);
    assert_eq!(plan_begun, 0);
    assert_eq!(plan_rollback, 0);

    let (result_error, result_state, result_begun, result_rollback) = execute_with_trigger(
        "CREATE TRIGGER fail_build_result BEFORE UPDATE ON image_build_snapshots
         BEGIN SELECT RAISE(ABORT, 'injected result journal failure'); END;",
    )
    .await?;
    assert!(matches!(result_error, MmError::State { .. }));
    assert_eq!(result_state, OperationState::Failed);
    assert_eq!(result_begun, 1);
    assert_eq!(result_rollback, 1);
    Ok(())
}

#[tokio::test]
async fn primary_and_finish_journal_failures_preserve_both_causes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let service = fixture.service(Arc::new(FakeBuilder::default()));
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;
    let connection =
        rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
    connection.execute_batch(
        "CREATE TRIGGER fail_build_plan BEFORE INSERT ON image_build_snapshots
         BEGIN SELECT RAISE(ABORT, 'injected plan journal failure'); END;
         CREATE TRIGGER fail_build_finish BEFORE UPDATE ON operations
         BEGIN SELECT RAISE(ABORT, 'injected finish journal failure'); END;",
    )?;

    let error = match service
        .apply_reviewed(&selection, false, &plan.plan_hash)
        .await
    {
        Err(error) => error,
        Ok(_) => return Err("both journal failures must be surfaced".into()),
    };
    assert!(matches!(error, MmError::OperationFailure { .. }));
    assert!(error.to_string().contains("injected plan journal failure"));
    assert!(
        error
            .to_string()
            .contains("injected finish journal failure")
    );
    Ok(())
}

#[tokio::test]
async fn execution_or_result_failure_with_finish_failure_rolls_back_and_preserves_both_causes()
-> Result<(), Box<dyn std::error::Error>> {
    async fn run(
        builder: Arc<FakeBuilder>,
        triggers: &str,
        expected_status: MutationStatus,
        primary_message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let service = fixture.service(builder.clone());
        let selection = WorkspaceSelection::new(fixture.project.path(), None);
        let plan = service.plan(&selection, false).await?;
        let connection =
            rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
        connection.execute_batch(triggers)?;

        let error = match service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await
        {
            Err(error) => error,
            Ok(_) => return Err("combined build and finish failures unexpectedly succeeded".into()),
        };
        assert!(error.to_string().contains(primary_message), "{error}");
        assert!(
            error.to_string().contains("injected finish failure"),
            "{error}"
        );
        assert!(matches!(
            error,
            MmError::OperationFailure {
                mutation_status,
                ..
            } if mutation_status == expected_status
        ));
        assert_eq!(builder.begun.load(Ordering::SeqCst), 1);
        assert_eq!(builder.preserved.load(Ordering::SeqCst), 0);
        assert_eq!(builder.rolled_back.load(Ordering::SeqCst), 1);
        Ok(())
    }

    run(
        Arc::new(FakeBuilder {
            fail: true,
            ..FakeBuilder::default()
        }),
        "CREATE TRIGGER fail_build_finish BEFORE UPDATE ON operations
         BEGIN SELECT RAISE(ABORT, 'injected finish failure'); END;",
        MutationStatus::Unknown,
        "injected build failure",
    )
    .await?;
    run(
        Arc::new(FakeBuilder::default()),
        "CREATE TRIGGER fail_build_result BEFORE UPDATE ON image_build_snapshots
         BEGIN SELECT RAISE(ABORT, 'injected result failure'); END;
         CREATE TRIGGER fail_build_finish BEFORE UPDATE ON operations
         BEGIN SELECT RAISE(ABORT, 'injected finish failure'); END;",
        MutationStatus::Applied,
        "injected result failure",
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn preview_lock_replan_and_secret_adapter_failures_propagate()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let service = fixture.service(Arc::new(FakeBuilder::default()));
    let missing = temp_dirs::tempdir()?;
    assert!(
        service
            .apply_reviewed(
                &WorkspaceSelection::new(missing.path(), None),
                false,
                "unused"
            )
            .await
            .is_err()
    );

    let locked = fixture.service_with_lock(Arc::new(FakeBuilder::default()), Arc::new(FailingLock));
    let plan = locked.plan(&selection, false).await?;
    assert!(matches!(
        locked
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));

    let config_path = fixture.project.path().join(".config/mm.yaml");
    let replanning = fixture.service_with_lock(
        Arc::new(FakeBuilder::default()),
        Arc::new(MutatingLock {
            path: config_path,
            contents: Mutex::new(Some("not: [valid".to_owned())),
        }),
    );
    let plan = replanning.plan(&selection, false).await?;
    assert!(
        replanning
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await
            .is_err()
    );

    let fixture = Fixture::new()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let service = fixture.service_with_source_lock_and_resolver(
        Arc::new(FakeBuilder::default()),
        Arc::new(FakeSource),
        Arc::new(FakeLock),
        Arc::new(FailingSecretResolver),
    );
    let plan = service.plan(&selection, false).await?;
    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::Secret { .. })
    ));

    let fixture = Fixture::new()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let service = fixture.service_with_port_documents(
        Arc::new(FakeBuilder::default()),
        Arc::new(FailingPortDocuments),
    );
    let plan = service.plan(&selection, false).await?;
    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn state_adapter_failures_propagate_at_each_build_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    async fn run(
        fixture: &Fixture,
        publish: bool,
        sql: &str,
    ) -> Result<MmError, Box<dyn std::error::Error>> {
        let service = fixture.service(Arc::new(FakeBuilder::default()));
        let selection = WorkspaceSelection::new(fixture.project.path(), None);
        let plan = service.plan(&selection, publish).await?;
        rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?
            .execute_batch(sql)?;
        service
            .apply_reviewed(&selection, publish, &plan.plan_hash)
            .await
            .err()
            .ok_or_else(|| "injected state failure unexpectedly succeeded".into())
    }

    let fixture = Fixture::new()?;
    assert!(matches!(
        run(
            &fixture,
            false,
            "CREATE TRIGGER fail_workspace BEFORE INSERT ON workspaces
             BEGIN SELECT RAISE(ABORT, 'injected workspace registration failure'); END;"
        )
        .await?,
        MmError::State { .. }
    ));

    let fixture = Fixture::new()?;
    assert!(matches!(
        run(&fixture, false, "DROP TABLE operations;").await?,
        MmError::State { .. }
    ));

    let fixture = Fixture::new()?;
    assert!(matches!(
        run(
            &fixture,
            false,
            "CREATE TRIGGER fail_begin BEFORE INSERT ON operations
             BEGIN SELECT RAISE(ABORT, 'injected operation begin failure'); END;"
        )
        .await?,
        MmError::State { .. }
    ));

    let fixture = Fixture::new()?;
    assert!(matches!(
        run(
            &fixture,
            false,
            "CREATE TRIGGER fail_build_phase BEFORE INSERT ON operation_events
             WHEN NEW.payload_json LIKE '%\"phase\":\"build\"%'
             BEGIN SELECT RAISE(ABORT, 'injected build phase failure'); END;"
        )
        .await?,
        MmError::State { .. }
    ));

    let fixture = graph_fixture()?;
    assert!(matches!(
        run(&fixture, true, "DROP TABLE image_build_unit_cache;").await?,
        MmError::State { .. }
    ));

    let fixture = graph_fixture()?;
    assert!(matches!(
        run(
            &fixture,
            true,
            "CREATE TRIGGER fail_cache_record BEFORE INSERT ON image_build_unit_cache
             BEGIN SELECT RAISE(ABORT, 'injected initial cache record failure'); END;"
        )
        .await?,
        MmError::State { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn interrupted_cleanup_and_journal_adapter_failures_propagate()
-> Result<(), Box<dyn std::error::Error>> {
    async fn abandoned(fixture: &Fixture) -> Result<OperationId, Box<dyn std::error::Error>> {
        let operation = OperationId::new();
        fixture.state.begin_operation(&NewOperation {
            id: operation,
            workspace_id: fixture.workspace_id()?,
            kind: OperationKind::Build,
            environment: "local".to_owned(),
            requested_at_unix_ms: 1,
            plan_hash: Some("abandoned".to_owned()),
        })?;
        Ok(operation)
    }

    let fixture = Fixture::new()?;
    abandoned(&fixture).await?;
    let builder = Arc::new(FakeBuilder {
        fail_cleanup: true,
        ..FakeBuilder::default()
    });
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;
    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::Build { .. })
    ));
    assert_eq!(builder.cleaned.load(Ordering::SeqCst), 1);

    let fixture = Fixture::new()?;
    abandoned(&fixture).await?;
    rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?
        .execute_batch(
            "CREATE TRIGGER fail_interrupt BEFORE UPDATE ON operations
             WHEN NEW.state = 'interrupted'
             BEGIN SELECT RAISE(ABORT, 'injected interrupt journal failure'); END;",
        )?;
    let builder = Arc::new(FakeBuilder::default());
    let service = fixture.service(builder.clone());
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, false).await?;
    assert!(matches!(
        service
            .apply_reviewed(&selection, false, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));
    assert_eq!(builder.cleaned.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn current_and_cached_retag_adapter_failures_propagate()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = graph_fixture()?;
    let config_path = fixture.project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config
            .replace("    path: web\n", "    path: php\n")
            .replace("      depends-on:\n        php: MM_PHP_IMAGE\n", ""),
    )?;
    let service = fixture.service(Arc::new(FakeBuilder {
        fail_retag: true,
        ..FakeBuilder::default()
    }));
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let plan = service.plan(&selection, true).await?;
    assert!(matches!(
        service
            .apply_reviewed(&selection, true, &plan.plan_hash)
            .await,
        Err(MmError::Build { .. })
    ));

    let fixture = graph_fixture()?;
    let selection = WorkspaceSelection::new(fixture.project.path(), None);
    let first = fixture.service(Arc::new(FakeBuilder::default()));
    let plan = first.plan(&selection, true).await?;
    first
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let second = fixture.service(Arc::new(FakeBuilder {
        fail_retag: true,
        ..FakeBuilder::default()
    }));
    assert!(matches!(
        second
            .apply_reviewed(&selection, true, &plan.plan_hash)
            .await,
        Err(MmError::Build { .. })
    ));
    Ok(())
}
