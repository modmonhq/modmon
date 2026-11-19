use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::build::DockerBuildxBuilder;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{
    BuildOperationLock, BuildSourceInspector, BuildVariantRequest, CommandOutput, CommandSpec,
    ImageBuilder, ProcessRunner, SecretMaterialResolver, SecretMetadataInspector, StateStore,
    TargetOperationLockGuard, TargetStore,
};
use mm::app::{
    BuildExecutionPorts, BuildManager, BuildRequest, BuildService, ReceiptCheckout, StackManager,
    WorkspaceSelection,
};
use mm::domain::build::{
    BuildSource, BuiltImage, ImageBuildFilter, ImageBuildPlan, LocalImageEngine, LocalRebuildReason,
};
use mm::domain::error::MmError;
use mm::domain::readiness::SetupIssue;
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret::{SecretMetadata, SecretReference, SecretResolution, SecretScope};
use mm::domain::target::{TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use tempfile::TempDir;
use zeroize::Zeroizing;

mod temp_dirs;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

const CONFIG: &str = r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de5
  name: shop
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  base:
    path: base
    build:
      image: ghcr.io/example/base
  tools:
    path: tools
    build:
      image: ghcr.io/example/tools
  api:
    path: api
    services:
      api:
        stack: app
    build:
      image: ghcr.io/example/api
  devtool:
    path: devtool
    services:
      devtool:
        stack: app
        environments: [local]
    build:
      image: ghcr.io/example/devtool
      depends-on:
        tools: TOOLS_IMAGE
  worker:
    path: worker
    services:
      worker:
        stack: app
        environments: [staging]
    build:
      image: ghcr.io/example/worker
environments:
  local:
    bindings: {}
  staging:
    bindings: {}
"#;

#[derive(Debug)]
struct CleanSource;

#[async_trait]
impl BuildSourceInspector for CleanSource {
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

fn project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let root = project.path();
    fs::create_dir(root.join(".config"))?;
    fs::write(root.join(".config/mm.yaml"), CONFIG)?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  api: {expose: ['80']}\n  devtool: {expose: ['80']}\n  worker: {expose: ['80']}\n",
    )?;
    for (module, dockerfile) in [
        ("base", "FROM scratch\n"),
        ("tools", "FROM scratch\n"),
        ("api", "FROM ghcr.io/example/base:edge\nCOPY . /app\n"),
        ("devtool", "ARG TOOLS_IMAGE\nFROM ${TOOLS_IMAGE}\n"),
        ("worker", "FROM scratch\n"),
    ] {
        fs::create_dir(root.join(module))?;
        fs::write(root.join(module).join("Dockerfile"), dockerfile)?;
        fs::write(root.join(module).join("main.txt"), module)?;
    }
    Ok(project)
}

fn planner() -> BuildManager {
    BuildManager::new(Arc::new(ConfigLoader), Arc::new(CleanSource))
}

fn modules(plan: &ImageBuildPlan) -> Vec<&str> {
    let mut modules = plan
        .units
        .iter()
        .map(|unit| unit.module.as_str())
        .collect::<Vec<_>>();
    modules.sort_unstable();
    modules
}

#[tokio::test]
async fn a_build_plans_the_modules_its_environment_runs_and_their_bases()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let local = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    let plan = planner()
        .plan(&local, BuildRequest::new(false, false))
        .await?;
    assert_eq!(modules(&plan), ["api", "base", "devtool", "tools"]);
    let api = plan
        .units
        .iter()
        .find(|unit| unit.module == "api")
        .ok_or("api is not planned")?;
    assert_eq!(api.dependencies[0].producer_module, "base");

    let staging = WorkspaceSelection::new(project.path(), Some("staging".to_owned()));
    let published = planner()
        .plan(&staging, BuildRequest::new(true, false))
        .await?;
    assert_eq!(modules(&published), ["api", "base", "worker"]);

    let every = planner()
        .plan(&staging, BuildRequest::new(true, true))
        .await?;
    assert_eq!(
        modules(&every),
        ["api", "base", "devtool", "tools", "worker"]
    );
    Ok(())
}

#[tokio::test]
async fn a_module_outside_the_environment_cannot_break_its_build()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    fs::write(
        project.path().join("worker/Dockerfile"),
        "ARG UNDECLARED\nFROM ${UNDECLARED}\n",
    )?;
    let local = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    planner()
        .plan(&local, BuildRequest::new(false, false))
        .await?;
    let staging = WorkspaceSelection::new(project.path(), Some("staging".to_owned()));
    let error = planner()
        .plan(&staging, BuildRequest::new(true, false))
        .await
        .err()
        .ok_or("a broken module the environment runs was planned")?;
    assert!(error.to_string().contains("UNDECLARED"), "{error}");
    Ok(())
}

#[tokio::test]
async fn an_environment_without_built_modules_names_the_all_flag()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let config = project.path().join(".config/mm.yaml");
    fs::write(
        &config,
        CONFIG
            .replace(
                "  staging:\n    bindings: {}\n",
                "  staging:\n    bindings: {}\n  docs:\n    bindings: {}\n",
            )
            .replace(
                "      api:\n        stack: app\n",
                "      api:\n        stack: app\n        environments: [local, staging]\n",
            ),
    )?;
    let docs = WorkspaceSelection::new(project.path(), Some("docs".to_owned()));
    let error = planner()
        .plan(&docs, BuildRequest::new(false, false))
        .await
        .err()
        .ok_or("an environment with no built module was planned")?;
    assert!(
        error.to_string().contains("`mm -e docs build --all`"),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_receipt_of_every_module_promotes_to_an_environment_that_runs_fewer()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let machine = Machine::new()?;
    let builds = machine.builds(None, Arc::new(FakeEngine::default()));
    let staging = WorkspaceSelection::new(project.path(), Some("staging".to_owned()));
    let every = BuildRequest::new(true, true);
    let plan = builds.plan(&staging, every).await?;
    builds
        .apply_reviewed(&staging, every, &plan.plan_hash)
        .await?;
    let receipt = builds.receipt(&staging)?;
    assert_eq!(receipt.plan.units.len(), 5);

    let local = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    let promoted = planner()
        .receipt_for_environment(&local, &receipt, &ReceiptCheckout::Worktree)
        .await?;
    promoted.receipt.validate()?;
    assert_eq!(
        modules(&promoted.receipt.plan),
        ["api", "base", "devtool", "tools"]
    );
    assert_eq!(promoted.receipt.plan.environment, "local");
    assert_eq!(promoted.origin.receipt_hash, receipt.receipt_hash);
    Ok(())
}

const LOCAL_CONFIG: &str = r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de6
  name: shop
  development-environment: local
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  base:
    path: base
    build:
      image: ghcr.io/example/base
  web:
    path: web
    services:
      web:
        stack: app
    build:
      image: ghcr.io/example/web
      depends-on:
        base: BASE_IMAGE
environments:
  local:
    bindings: {}
"#;

fn local_project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let root = project.path();
    fs::create_dir(root.join(".config"))?;
    fs::write(root.join(".config/mm.yaml"), LOCAL_CONFIG)?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: ghcr.io/example/web:old\n    expose: ['80']\n",
    )?;
    fs::create_dir(root.join("base"))?;
    fs::write(root.join("base/Dockerfile"), "FROM scratch\n")?;
    fs::create_dir(root.join("web"))?;
    fs::write(
        root.join("web/Dockerfile"),
        "ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nCOPY . /app\n",
    )?;
    fs::write(root.join("web/index.html"), "hello\n")?;
    Ok(project)
}

/// What a build asked the engine: module, publish, engine, platforms.
type EngineRequest = (String, bool, Option<LocalImageEngine>, Vec<String>);

/// A container engine's image store: what a local build loads, by tag.
#[derive(Debug, Default)]
struct FakeEngine {
    images: Mutex<BTreeMap<String, String>>,
    requests: Mutex<Vec<EngineRequest>>,
    retagged: Mutex<usize>,
}

fn poisoned<T>(_: T) -> MmError {
    MmError::State {
        message: "fake engine lock was poisoned".to_owned(),
    }
}

impl FakeEngine {
    fn built(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|requests| requests.iter().map(|request| request.0.clone()).collect())
            .unwrap_or_default()
    }

    fn forget(&self, reference: &str) {
        if let Ok(mut images) = self.images.lock() {
            images.remove(reference);
        }
    }
}

#[async_trait]
impl ImageBuilder for FakeEngine {
    async fn build_variant(&self, request: BuildVariantRequest<'_>) -> Result<BuiltImage, MmError> {
        let mut requests = self.requests.lock().map_err(poisoned)?;
        let tags = request
            .variant
            .tags
            .iter()
            .map(|tag| format!("{}:{tag}", request.unit.image))
            .collect::<Vec<_>>();
        let identity = format!(
            "{}{:?}{}",
            request.unit.module,
            request.unit.args,
            requests.len()
        );
        let digest = format!("sha256:{}", blake3::hash(identity.as_bytes()).to_hex());
        requests.push((
            request.unit.module.clone(),
            request.publish,
            request.engine,
            request.platforms.to_vec(),
        ));
        let mut images = self.images.lock().map_err(poisoned)?;
        for tag in &tags {
            images.insert(tag.clone(), digest.clone());
        }
        Ok(BuiltImage {
            module: request.unit.module.clone(),
            target: request.variant.target.clone(),
            repository: request.unit.image.clone(),
            tags,
            digest,
            execution_fingerprint: String::new(),
            secret_revisions: BTreeMap::new(),
            archive: None,
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
        if let Ok(mut retagged) = self.retagged.lock() {
            *retagged += 1;
        }
        Ok(digest.to_owned())
    }

    async fn local_image_id(
        &self,
        _engine: LocalImageEngine,
        reference: &str,
    ) -> Result<Option<String>, MmError> {
        Ok(self
            .images
            .lock()
            .ok()
            .and_then(|images| images.get(reference).cloned()))
    }
}

#[derive(Debug)]
struct NoSecrets;

#[async_trait]
impl SecretMetadataInspector for NoSecrets {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Err(MmError::Secret {
            message: "this project has no build secrets".to_owned(),
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for NoSecrets {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Err(MmError::Secret {
            message: "this project has no build secrets".to_owned(),
        })
    }
}

#[derive(Debug)]
struct NoLock;

struct NoGuard;

impl TargetOperationLockGuard for NoGuard {}

impl BuildOperationLock for NoLock {
    fn acquire(
        &self,
        _project_id: ProjectId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(NoGuard))
    }
}

struct Machine {
    _home: TempDir,
    home: PathBuf,
    state: Arc<SqliteStateStore>,
}

impl Machine {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = temp_dirs::tempdir()?;
        let path = home.path().to_path_buf();
        let state = Arc::new(SqliteStateStore::open(&path.join("state.sqlite3"))?);
        Ok(Self {
            _home: home,
            home: path,
            state,
        })
    }

    fn builds(
        &self,
        engine: Option<LocalImageEngine>,
        builder: Arc<dyn ImageBuilder>,
    ) -> BuildService {
        let state: Arc<dyn StateStore> = self.state.clone();
        BuildService::new(
            planner().with_local_engine(engine),
            BuildExecutionPorts {
                documents: Arc::new(ConfigLoader),
                state,
                builder,
                secret_inspector: Arc::new(NoSecrets),
                secret_resolver: Arc::new(NoSecrets),
                locks: Arc::new(NoLock),
                clock: Arc::new(FixedClock { unix_ms: 1_000 }),
            },
        )
    }

    fn stacks(&self) -> Result<StackManager, MmError> {
        let targets = Arc::new(mm::adapters::target::FilesystemTargetStore::new(
            self.home.join("targets.yaml"),
        ));
        targets.insert_target(
            "local",
            TargetDefinition::local(TargetId(uuid::Uuid::new_v4())),
        )?;
        Ok(StackManager::new(
            Arc::new(ConfigLoader),
            Arc::new(mm::adapters::stack::FilesystemStackArtifactStore::new(
                self.home.join("compose"),
            )),
            targets,
        )
        .with_state(self.state.clone()))
    }
}

fn local_selection(project: &TempDir, runtime: RuntimeKind) -> WorkspaceSelection {
    let mut selection = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    selection.runtime = Some(runtime);
    selection
}

#[tokio::test]
async fn a_local_build_loads_into_the_engine_and_the_local_deployment_runs_it()
-> Result<(), Box<dyn std::error::Error>> {
    let project = local_project()?;
    let machine = Machine::new()?;
    let engine = Arc::new(FakeEngine::default());
    let builds = machine.builds(Some(LocalImageEngine::Docker), engine.clone());
    let selection = local_selection(&project, RuntimeKind::DockerCompose);

    let plan = builds.plan(&selection, false).await?;
    assert_eq!(plan.engine, Some(LocalImageEngine::Docker));
    assert!(!plan.publish);
    assert_eq!(plan.platforms.len(), 1);
    let web = plan
        .units
        .iter()
        .find(|unit| unit.module == "web")
        .ok_or("web is not planned")?;
    assert_eq!(web.image, "mm.invalid/shop/web");
    let tag = web.variants[0].tags.first().ok_or("web has no tag")?;
    assert!(tag.starts_with("local-"), "{tag}");
    assert_eq!(
        builds.plan(&selection, false).await?.plan_hash,
        plan.plan_hash
    );

    let result = builds
        .apply_reviewed(&selection, false, &plan.plan_hash)
        .await?;
    assert_eq!(engine.built(), ["base", "web"]);
    assert!(engine.requests.lock().map_err(|_| "lock")?.iter().all(
        |(_, publish, local, platforms)| !publish
            && *local == Some(LocalImageEngine::Docker)
            && platforms == &plan.platforms
    ));
    assert_eq!(*engine.retagged.lock().map_err(|_| "lock")?, 0);
    let dependency = result.dependencies.first().ok_or("no base dependency")?;
    assert!(
        dependency
            .immutable_reference
            .starts_with("mm.invalid/shop/base:local-")
    );
    assert!(result.images.iter().all(|image| image.archive.is_none()));

    let document = ConfigLoader.load_from(project.path())?;
    assert!(
        machine
            .state
            .latest_successful_image_build(document.config.project.id, &document.root, "local")?
            .is_none(),
        "a local build is never a published one"
    );
    assert!(
        machine
            .state
            .latest_image_build(
                document.config.project.id,
                &document.root,
                "local",
                ImageBuildFilter::LoadedInto(LocalImageEngine::Docker),
            )?
            .is_some()
    );

    let stacks = machine.stacks()?;
    let compiled = stacks.compile(&selection)?;
    let reference = format!("mm.invalid/shop/web:{tag}");
    assert_eq!(
        compiled.stacks[0].manifest.services[0].image.as_deref(),
        Some(reference.as_str())
    );
    let overlay = fs::read_to_string(&compiled.stacks[0].generated_overlay_path)?;
    assert!(
        overlay.contains(&format!("image: {reference}")),
        "{overlay}"
    );
    assert!(overlay.contains("pull_policy: never"), "{overlay}");
    assert!(!overlay.contains("ghcr.io"), "{overlay}");

    let podman = local_selection(&project, RuntimeKind::PodmanCompose);
    assert!(matches!(
        stacks.compile(&podman),
        Err(MmError::Setup {
            issue: SetupIssue::MissingImages { .. }
        })
    ));

    let mut host = selection.clone();
    host.host_values = true;
    assert!(
        matches!(
            stacks.compile(&host),
            Err(MmError::Setup {
                issue: SetupIssue::MissingImages { .. }
            })
        ),
        "a host runs published builds only"
    );
    let receipt = builds
        .receipt(&selection)
        .err()
        .ok_or("a local build was written as a receipt")?;
    assert!(
        receipt
            .to_string()
            .contains("mm --environment local build --publish"),
        "{receipt}"
    );
    Ok(())
}

#[tokio::test]
async fn a_local_build_keeps_the_images_whose_inputs_did_not_change()
-> Result<(), Box<dyn std::error::Error>> {
    let project = local_project()?;
    let machine = Machine::new()?;
    let engine = Arc::new(FakeEngine::default());
    let builds = machine.builds(Some(LocalImageEngine::Podman), engine.clone());
    let selection = local_selection(&project, RuntimeKind::PodmanCompose);

    let local = builds
        .local_build(&selection)
        .await?
        .ok_or("the environment has no local build")?;
    let review = local.review().ok_or("a first build was not planned")?;
    assert_eq!(review.engine, LocalImageEngine::Podman);
    assert!(
        review
            .modules
            .iter()
            .all(|rebuild| rebuild.reason == LocalRebuildReason::NotBuilt)
    );
    local
        .run(&selection)
        .await?
        .ok_or("the reviewed build did not run")?;
    assert!(
        local.run(&selection).await?.is_none(),
        "the build ran twice"
    );
    assert_eq!(engine.built(), ["base", "web"]);

    let unchanged = builds
        .local_build(&selection)
        .await?
        .ok_or("the environment has no local build")?;
    assert!(unchanged.review().is_none(), "{:?}", unchanged.review());

    fs::write(project.path().join("web/index.html"), "changed\n")?;
    let changed = builds
        .local_build(&selection)
        .await?
        .ok_or("the environment has no local build")?;
    let review = changed
        .review()
        .ok_or("a changed context planned no build")?;
    assert_eq!(review.modules.len(), 1);
    assert_eq!(review.modules[0].module, "web");
    assert_eq!(review.modules[0].reason, LocalRebuildReason::Context);
    changed.run(&selection).await?;
    assert_eq!(engine.built(), ["base", "web", "web"], "base was rebuilt");

    let current = builds.plan(&selection, false).await?;
    let base = current
        .units
        .iter()
        .find(|unit| unit.module == "base")
        .ok_or("base is not planned")?;
    engine.forget(&format!("{}:{}", base.image, base.variants[0].tags[0]));
    let pruned = builds
        .local_build(&selection)
        .await?
        .ok_or("the environment has no local build")?;
    let review = pruned.review().ok_or("a pruned image planned no build")?;
    assert_eq!(review.modules[0].module, "base");
    assert_eq!(review.modules[0].reason, LocalRebuildReason::ImageMissing);
    Ok(())
}

#[tokio::test]
async fn an_environment_on_remote_hosts_has_no_local_build()
-> Result<(), Box<dyn std::error::Error>> {
    let project = local_project()?;
    let machine = Machine::new()?;
    let builds = machine.builds(None, Arc::new(FakeEngine::default()));
    let selection = local_selection(&project, RuntimeKind::DockerCompose);
    assert!(builds.local_build(&selection).await?.is_none());
    assert!(builds.plan(&selection, false).await?.engine.is_none());
    Ok(())
}

/// An exit code, standard output and standard error, or why the program did
/// not start.
type Answer = Result<(i32, &'static str, &'static str), std::io::ErrorKind>;

/// A `docker` and `podman` that answer from a script: the first rule whose
/// prefix starts the command line answers it.
struct ScriptedEngine {
    rules: Vec<(&'static str, Answer)>,
    seen: Mutex<Vec<String>>,
}

impl ScriptedEngine {
    fn new(rules: Vec<(&'static str, Answer)>) -> Arc<Self> {
        Arc::new(Self {
            rules,
            seen: Mutex::new(Vec::new()),
        })
    }

    fn seen(&self) -> Vec<String> {
        self.seen
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl ProcessRunner for ScriptedEngine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let line = specification.redacted_display();
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(line.clone());
        }
        let answer = self
            .rules
            .iter()
            .find(|(prefix, _)| line.starts_with(prefix))
            .map_or(Ok((0, "", "")), |(_, answer)| *answer);
        match answer {
            Ok((code, stdout, stderr)) => Ok(CommandOutput {
                code: Some(code),
                stdout: Zeroizing::new(stdout.to_owned()),
                stderr: Zeroizing::new(stderr.to_owned()),
                truncated: false,
            }),
            Err(kind) => Err(MmError::ProcessSpawn {
                program: line,
                source: std::io::Error::from(kind),
            }),
        }
    }
}

const IMAGE_ID: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

async fn build_with(
    project: &TempDir,
    machine: &Machine,
    process: Arc<ScriptedEngine>,
    (engine, publish): (Option<LocalImageEngine>, bool),
) -> Result<Result<mm::domain::build::ImageBuildResult, MmError>, Box<dyn std::error::Error>> {
    let builder = Arc::new(DockerBuildxBuilder::new(
        process,
        machine.home.join("builds"),
    ));
    let builds = machine.builds(engine, builder);
    let selection = local_selection(project, RuntimeKind::DockerCompose);
    let plan = builds.plan(&selection, publish).await?;
    Ok(builds
        .apply_reviewed(&selection, publish, &plan.plan_hash)
        .await)
}

#[tokio::test]
async fn a_local_docker_build_runs_on_the_engine_and_records_its_image_ids()
-> Result<(), Box<dyn std::error::Error>> {
    let project = local_project()?;
    let machine = Machine::new()?;
    let process = ScriptedEngine::new(vec![("docker image inspect", Ok((0, IMAGE_ID, "")))]);
    let result = build_with(
        &project,
        &machine,
        process.clone(),
        (Some(LocalImageEngine::Docker), false),
    )
    .await??;
    assert!(result.images.iter().all(|image| image.digest == IMAGE_ID));
    let seen = process.seen();
    let builds = seen
        .iter()
        .filter(|line| line.starts_with("docker build "))
        .collect::<Vec<_>>();
    assert_eq!(builds.len(), 2, "{seen:#?}");
    for line in &builds {
        assert!(line.contains(" --tag mm.invalid/shop/"), "{line}");
        assert!(
            !line.contains("--push") && !line.contains("buildx"),
            "{line}"
        );
    }
    assert!(
        builds[1].contains("--build-arg BASE_IMAGE=mm.invalid/shop/base:local-"),
        "{}",
        builds[1]
    );
    assert!(
        seen.iter().all(|line| !line.contains("ghcr.io")),
        "{seen:#?}"
    );
    Ok(())
}
