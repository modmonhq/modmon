use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::{ConfigLoader, validate};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    BuildOperationLock, BuildSourceInspector, ImageBuilder, IngressRouteRegistry,
    SecretMaterialResolver, SecretMetadataInspector, StateStore, TargetOperationLockGuard,
    TargetStore,
};
use mm::app::{BuildExecutionPorts, BuildManager, BuildService, StackManager, WorkspaceSelection};
use mm::domain::build::{BuildSource, BuiltImage};
use mm::domain::error::MmError;
use mm::domain::secret::{
    SecretMaterial, SecretMetadata, SecretReference, SecretResolution, SecretStatus,
};
use mm::domain::target::{TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;

mod temp_dirs;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const TARGET_ID: &str = "319c527b-6242-7b43-bfd4-bd60e27c1de3";

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
struct FakeSecrets;

#[async_trait]
impl SecretMetadataInspector for FakeSecrets {
    async fn inspect(
        &self,
        _scope: &mm::domain::secret::SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Ok(SecretMetadata {
            status: SecretStatus::Set,
            revision: Some("revision-1".to_owned()),
            created_at_unix_ms: Some(1),
            candidate_revision: None,
            set_by: None,
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for FakeSecrets {
    async fn resolve(
        &self,
        scope: &mm::domain::secret::SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Ok(SecretResolution {
            metadata: self.inspect(scope, reference).await?,
            material: Some(
                SecretMaterial::new("unused".to_owned())
                    .map_err(|message| MmError::Secret { message })?,
            ),
        })
    }
}

#[derive(Debug, Default)]
struct FakeBuilder {
    built: Mutex<Vec<String>>,
    args: Mutex<BTreeMap<String, BTreeMap<String, String>>>,
}

#[async_trait]
impl ImageBuilder for FakeBuilder {
    async fn build_variant(
        &self,
        request: mm::app::ports::BuildVariantRequest<'_>,
    ) -> Result<BuiltImage, MmError> {
        let unit = request.unit;
        let variant = request.variant;
        let publish = request.publish;
        self.built
            .lock()
            .map_err(|_| MmError::State {
                message: "lock".to_owned(),
            })?
            .push(unit.module.clone());
        self.args
            .lock()
            .map_err(|_| MmError::State {
                message: "lock".to_owned(),
            })?
            .insert(unit.module.clone(), unit.args.clone());
        let digest = if unit.module == "base-runtime" {
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        } else {
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
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
            digest: digest.to_owned(),
            execution_fingerprint: String::new(),
            secret_revisions: BTreeMap::new(),
            archive: (!publish).then(|| std::path::PathBuf::from("/tmp/acme.oci.tar")),
            reused: false,
        })
    }

    async fn retag_variant(
        &self,
        _source_repository: &str,
        digest: &str,
        _destination_repository: &str,
        _tags: &[String],
    ) -> Result<String, MmError> {
        Ok(digest.to_owned())
    }
}

#[derive(Debug)]
struct FakeLock;

impl BuildOperationLock for FakeLock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(FakeGuard))
    }
}

#[derive(Debug)]
struct FakeGuard;

impl TargetOperationLockGuard for FakeGuard {}

#[tokio::test]
async fn portable_project_builds_dependencies_then_compiles_scoped_network_overlays()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_portable_fixture(project.path())?;
    let document = ConfigLoader.load_from(project.path())?;
    let report = validate(&document, "local");
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == mm::domain::diagnostic::Severity::Error)
            .count(),
        0
    );
    assert!(document.config.modules["base-runtime"].services.is_empty());
    assert_eq!(
        document.config.modules["web"]
            .build
            .as_ref()
            .and_then(|build| build.depends_on.get("base-runtime"))
            .map(String::as_str),
        Some("MM_BASE_RUNTIME_IMAGE")
    );

    let home = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let builder = Arc::new(FakeBuilder::default());
    let service = BuildService::new(
        BuildManager::new(Arc::new(ConfigLoader), Arc::new(FakeSource)),
        BuildExecutionPorts {
            documents: Arc::new(ConfigLoader),
            state: state.clone(),
            builder: builder.clone(),
            secret_inspector: Arc::new(FakeSecrets),
            secret_resolver: Arc::new(FakeSecrets),
            locks: Arc::new(FakeLock),
            clock: Arc::new(FixedClock { unix_ms: 1000 }),
        },
    );
    let selection = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    let plan = service.plan(&selection, true).await?;
    let modules = plan
        .units
        .iter()
        .map(|unit| unit.module.as_str())
        .collect::<Vec<_>>();
    assert!(modules.contains(&"base-runtime"));
    assert!(modules.contains(&"web"));
    assert!(
        modules.iter().position(|name| *name == "base-runtime")
            < modules.iter().position(|name| *name == "web")
    );
    let result = service
        .apply_reviewed(&selection, true, &plan.plan_hash)
        .await?;
    let built = builder.built.lock().map_err(|_| "lock")?.clone();
    assert_eq!(
        plan.units
            .iter()
            .find(|unit| unit.module == "base-runtime")
            .map(|unit| unit.input_fingerprint.as_str()),
        plan.units
            .iter()
            .find(|unit| unit.module == "docs")
            .map(|unit| unit.input_fingerprint.as_str()),
        "byte-identical first-party scratch builds share one canonical node"
    );
    assert_eq!(
        built
            .iter()
            .filter(|module| *module == "base-runtime" || *module == "docs")
            .count(),
        1
    );
    assert!(built.contains(&"web".to_owned()));
    let base_digest = result
        .images
        .iter()
        .find(|image| image.module == "base-runtime")
        .map(|image| image.digest.clone())
        .ok_or("base runtime image missing")?;
    assert_eq!(
        builder
            .args
            .lock()
            .map_err(|_| "lock")?
            .get("web")
            .and_then(|args| args.get("MM_BASE_RUNTIME_IMAGE"))
            .cloned(),
        Some(format!("ghcr.io/acme/base-runtime@{base_digest}"))
    );
    assert_eq!(result.dependencies[0].edge.producer_module, "base-runtime");

    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(TARGET_ID)?)),
    )?;
    let registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let store: Arc<dyn StateStore> = state;
    let stacks = StackManager::with_route_registry(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        registry,
        targets,
    )
    .with_state(store);
    let compiled = stacks.compile(&selection)?;
    assert_eq!(compiled.stacks.len(), 2);
    let app = compiled
        .stacks
        .iter()
        .find(|stack| stack.manifest.stack == "app")
        .ok_or("app stack missing")?;
    let overlay = fs::read_to_string(&app.generated_overlay_path)?;
    assert!(overlay.contains(
        "image: ghcr.io/acme/web@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    ));
    assert!(overlay.contains("internal:"));
    assert!(overlay.contains("web:"));
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&overlay)?;
    assert!(parsed["services"]["web"]["networks"]["internal"].is_mapping());
    assert!(parsed["services"]["docs"]["networks"]["internal"].is_null());
    let database = compiled
        .stacks
        .iter()
        .find(|stack| stack.manifest.stack == "database")
        .ok_or("database stack missing")?;
    let database_overlay = fs::read_to_string(&database.generated_overlay_path)?;
    assert!(database_overlay.contains("postgres:"));
    assert!(database_overlay.contains("internal:"));
    Ok(())
}

fn write_portable_fixture(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("base-runtime"))?;
    fs::create_dir_all(root.join("web"))?;
    fs::create_dir_all(root.join("docs"))?;
    fs::write(
        root.join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 5c421beb-ac02-4b2d-a16d-9ad7e8b5d832
  name: acme
default-environment: local
target: local
runtime:
  driver: podman-compose
networks:
  internal: {}
stacks:
  database:
    files: [compose.database.yaml]
    networks: [internal]
  app:
    files: [compose.app.yaml]
    ingress:
      - domain: web
        service: web
modules:
  postgres:
    path: postgres
    services:
      postgres:
        stack: database
  web:
    path: web
    networks: [internal]
    env:
      template: .env.example
    build:
      image: ghcr.io/acme/web
      depends-on:
        base-runtime: MM_BASE_RUNTIME_IMAGE
    services:
      web:
        stack: app
        env:
          CONTAINER_ROLE: app
  docs:
    path: docs
    build:
      image: ghcr.io/acme/docs
    services:
      docs:
        stack: app
  base-runtime:
    path: base-runtime
    build:
      image: ghcr.io/acme/base-runtime
environments:
  local:
    domains:
      web: web.acme.test
    bindings:
      APP.URL:
        cast: url
        value-ref: domains.web
  integration:
    domains:
      web: web.example.com
    bindings: {}
"#,
    )?;
    fs::create_dir_all(root.join("postgres"))?;
    fs::write(
        root.join("compose.app.yaml"),
        "services:\n  web:\n    expose: ['80']\n  docs:\n    expose: ['80']\n",
    )?;
    fs::write(
        root.join("compose.database.yaml"),
        "services:\n  postgres:\n    image: docker.io/library/postgres@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
    )?;
    fs::write(root.join("base-runtime/Dockerfile"), "FROM scratch\n")?;
    fs::write(
        root.join("web/Dockerfile"),
        "ARG VERSION=dev\nARG MM_BASE_RUNTIME_IMAGE\nFROM ${MM_BASE_RUNTIME_IMAGE}\nARG VERSION\n",
    )?;
    fs::write(
        root.join("web/.env.example"),
        "APP_URL=\n# mm:from=APP.URL\n",
    )?;
    fs::write(root.join("web/.dockerignore"), ".env\n.env.*\n")?;
    fs::write(root.join("docs/Dockerfile"), "FROM scratch\n")?;
    Ok(())
}
