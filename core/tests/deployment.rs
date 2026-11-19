use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::ca::FilesystemLocalCaStore;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::{
    EnvironmentIndexer, FilesystemEnvironmentArtifactStore, FilesystemMaterializedEnvironmentWriter,
};
use mm::adapters::secrets::{AgeSecretStore, compose_file_secret_descriptor};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::{MmHome, SqliteStateStore};
use mm::adapters::target::{
    FilesystemTargetIngressArtifactStore, FilesystemTargetOperationLock, FilesystemTargetStore,
    FilesystemTraefikDynamicConfigStore,
};
use mm::app::ports::{
    BuildSourceInspector, EnvironmentArtifactRequest, EnvironmentArtifactStore,
    EnvironmentValueStore, IngressClaimsReplacement, IngressRouteRegistry, IngressVerifier,
    LocalDeploymentRuntime, PrivilegedIngressInstaller, ProjectDocumentStore,
    RootlessIngressInstaller, RuntimeDriver, RuntimeSecretDelivery, SchedulerRepository,
    SecretCreator, SecretDeliveryCapabilityCatalog, SecretMaterialResolver,
    SecretMetadataInspector, SecretReplacer, SecretStorageCapabilityCatalog, StateStore,
    TargetOperationLock, TargetStore,
};
use mm::app::{
    BuildManager, DeploymentExecutionPorts, DeploymentManager, DeploymentManagerDependencies,
    DeploymentService, EnvironmentDependencies, ReceiptCheckout, StackManager,
    TargetIngressManager, WorkspaceSelection,
};
use mm::domain::build::{BuildReceipt, BuildSource, BuiltImage, ImageBuildPlan, ImageBuildResult};
use mm::domain::environment::PreparedEnvironmentBundle;
use mm::domain::error::MmError;
use mm::domain::ingress::{
    CanonicalMiddleware, IngressRouteClaim, IngressRouteConflict, IngressRouteSet,
};
use mm::domain::operation::{NewOperation, OperationId, OperationKind, OperationState};
use mm::domain::runtime::{
    ComposeProvider, ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy,
    ConvergenceState, ObservedProject, RenderedProject, ResourceEnsureResult, RuntimeCapability,
    RuntimeKind, RuntimeProbe, RuntimeResource, RuntimeTarget,
};
use mm::domain::secret::{SecretExpectedState, SecretMaterial, SecretReference, SecretScope};
use mm::domain::secret_contract::{
    DeliveryCapabilityDescriptor, DeliveryDriverId, StorageProviderId,
};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId, TargetIngress, TargetTransport};
use mm::domain::target_ingress::TargetIngressPlan;
use mm::domain::workspace::{ProjectId, WorkspaceRegistration};
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

#[allow(clippy::duplicate_mod)]
mod support;

#[path = "deployment/route_adoption.rs"]
mod route_adoption;

const PROJECT_ID: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";
const TARGET_ID: &str = "319c527b-6242-7b43-bfd4-bd60e27c1de3";
const BUILD_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const IMAGE_DIGEST: &str =
    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[derive(Debug)]
struct StaticBuildSource;

#[async_trait]
impl BuildSourceInspector for StaticBuildSource {
    async fn inspect(&self, _project_root: &Path) -> Result<BuildSource, MmError> {
        Ok(BuildSource {
            commit: BUILD_COMMIT.to_owned(),
            branch: Some("main".to_owned()),
            tags_at_head: Vec::new(),
            repository_tags: Vec::new(),
            tags_complete: true,
            default_branch: true,
            dirty: false,
        })
    }
}

#[tokio::test]
async fn service_exclusions_are_reviewed_remove_empty_stacks_and_skip_unused_secrets()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fs::write(
        fixture.project.join(".config/mm.overrides.yaml"),
        r#"
environments:
  production: {}
stacks:
  app:
    ingress:
      environments: [production]
modules:
  app:
    services:
      web:
        environments: [production]
        secret-files:
          worker-token: {secret: worker.token}
secrets:
  worker.token:
    storage: {provider: age, location: local/worker-token}
"#,
    )?;
    let excluded = fixture.service.plan(&fixture.selection).await?;
    assert!(excluded.stacks.is_empty());
    assert_eq!(excluded.removed_stacks.len(), 1);
    assert_eq!(excluded.excluded_services.len(), 1);
    assert_eq!(excluded.excluded_services[0].service, "web");
    assert!(excluded.environment_review.services.is_empty());
    assert!(
        excluded
            .runtime_secret_plan
            .as_ref()
            .is_none_or(|plan| plan.items.is_empty())
    );
    assert!(
        fixture
            .service
            .apply_reviewed(&fixture.selection, &initial.plan_hash)
            .await
            .is_err()
    );
    let mut changed = excluded.clone();
    changed.excluded_services.clear();
    assert_ne!(changed.canonical_hash()?, excluded.plan_hash);
    fixture
        .service
        .apply_reviewed(&fixture.selection, &excluded.plan_hash)
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_plan_lists_the_keys_its_planner_ignored_outside_its_hash()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    fs::write(
        fixture.project.join(".config/mm.overrides.yaml"),
        "modules:\n  app:\n    services:\n      web:\n        replicaz: 2\nnewer-key: {}\n",
    )?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let keys = plan
        .unknown_config_keys
        .iter()
        .map(|key| {
            (
                key.file.as_str(),
                key.key.as_str(),
                key.suggestion.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            (
                ".config/mm.overrides.yaml",
                "modules.app.services.web.replicaz",
                Some("replicas")
            ),
            (".config/mm.overrides.yaml", "newer-key", None),
        ]
    );
    let json = serde_json::to_value(&plan)?;
    assert_eq!(json["unknown_config_keys"][1]["key"], "newer-key");
    let mut without = plan.clone();
    without.unknown_config_keys.clear();
    assert_eq!(without.canonical_hash()?, plan.plan_hash);
    Ok(())
}

#[tokio::test]
async fn deployment_requires_a_matching_published_build_and_pins_its_digest()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    make_buildable(&fixture)?;
    assert!(matches!(
        fixture.service.plan(&fixture.selection).await,
        Err(MmError::Setup {
            issue: mm::domain::readiness::SetupIssue::MissingImages { .. }
        })
    ));

    let build = BuildManager::new(Arc::new(ConfigLoader), Arc::new(StaticBuildSource));
    let build_plan = build.plan(&fixture.selection, true).await?;
    let workspace = fixture.workspace()?;
    let operation_id = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id: operation_id,
        workspace_id: workspace.id,
        kind: OperationKind::Build,
        environment: build_plan.environment.clone(),
        requested_at_unix_ms: 700,
        plan_hash: Some(build_plan.plan_hash.clone()),
    })?;
    fixture
        .state
        .save_image_build_plan(operation_id, &build_plan)?;
    let images = published_images(&build_plan)?;
    let build_result = ImageBuildResult {
        operation_id,
        plan_hash: build_plan.plan_hash.clone(),
        images,
        dependencies: Vec::new(),
    };
    fixture
        .state
        .save_image_build_result(operation_id, &build_result)?;
    fixture
        .state
        .finish_operation(operation_id, OperationState::Succeeded, 800, None)?;

    let deployment = fixture.service.plan(&fixture.selection).await?;
    let image = format!("example/web@{IMAGE_DIGEST}");
    assert_eq!(
        deployment.stacks[0].artifact.manifest.services[0]
            .image
            .as_deref(),
        Some(image.as_str())
    );
    let overlay = fs::read_to_string(&deployment.stacks[0].artifact.generated_overlay_path)?;
    assert!(overlay.contains(&format!("image: {image}")));
    assert!(!overlay.contains("image: example/web\n"));
    Ok(())
}

fn make_buildable(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(fixture.project.join("app/Dockerfile"), "FROM scratch\n")?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace(
            "    path: app\n",
            "    path: app\n    build:\n      image: example/web\n",
        ),
    )?;
    Ok(())
}

/// What a publishing build of `plan` reports: every variant at `IMAGE_DIGEST`.
fn published_images(plan: &ImageBuildPlan) -> Result<Vec<BuiltImage>, Box<dyn std::error::Error>> {
    Ok(plan
        .units
        .iter()
        .flat_map(|unit| {
            unit.variants.iter().map(|variant| {
                Ok(BuiltImage {
                    module: unit.module.clone(),
                    target: variant.target.clone(),
                    repository: unit.image.clone(),
                    tags: variant
                        .tags
                        .iter()
                        .map(|tag| format!("{}:{tag}", unit.image))
                        .collect(),
                    digest: IMAGE_DIGEST.to_owned(),
                    execution_fingerprint: mm::domain::build::unit_execution_fingerprint(
                        &unit.input_fingerprint,
                        &plan.environment,
                        Some(variant),
                        &BTreeMap::new(),
                        &BTreeMap::new(),
                    )
                    .map_err(std::io::Error::other)?,
                    secret_revisions: BTreeMap::new(),
                    archive: None,
                    reused: false,
                })
            })
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?)
}

/// A container engine that keeps what a local build loads into it.
#[derive(Default)]
struct LocalEngine {
    images: Mutex<BTreeMap<String, String>>,
    builds: AtomicUsize,
}

#[async_trait]
impl mm::app::ports::ImageBuilder for LocalEngine {
    async fn build_variant(
        &self,
        request: mm::app::ports::BuildVariantRequest<'_>,
    ) -> Result<BuiltImage, MmError> {
        assert!(!request.publish && request.engine.is_some());
        let build = self.builds.fetch_add(1, Ordering::SeqCst);
        let digest = format!(
            "sha256:{}",
            blake3::hash(format!("{}-{build}", request.unit.module).as_bytes()).to_hex()
        );
        let tags = request
            .variant
            .tags
            .iter()
            .map(|tag| format!("{}:{tag}", request.unit.image))
            .collect::<Vec<_>>();
        if let Ok(mut images) = self.images.lock() {
            for tag in &tags {
                images.insert(tag.clone(), digest.clone());
            }
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
        _digest: &str,
        _destination_repository: &str,
        _tags: &[String],
    ) -> Result<String, MmError> {
        Err(MmError::Build {
            message: "a local build never talks to a registry".to_owned(),
        })
    }

    async fn local_image_id(
        &self,
        _engine: mm::domain::build::LocalImageEngine,
        reference: &str,
    ) -> Result<Option<String>, MmError> {
        Ok(self
            .images
            .lock()
            .ok()
            .and_then(|images| images.get(reference).cloned()))
    }
}

#[tokio::test]
async fn dev_up_builds_stale_local_images_first_and_runs_them_from_the_engine()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    make_buildable(&fixture)?;
    let engine = Arc::new(LocalEngine::default());
    let registry = support::secret_registry(fixture.age.clone())?;
    let state: Arc<dyn StateStore> = fixture.state.clone();
    let builds = mm::app::BuildService::new(
        BuildManager::new(Arc::new(ConfigLoader), Arc::new(StaticBuildSource))
            .with_local_engine(Some(mm::domain::build::LocalImageEngine::Podman)),
        mm::app::BuildExecutionPorts {
            documents: Arc::new(ConfigLoader),
            state,
            builder: engine.clone(),
            secret_inspector: registry.clone(),
            secret_resolver: registry,
            locks: Arc::new(FilesystemTargetOperationLock::new(
                fixture.state_path.with_file_name("build-locks"),
            )),
            clock: Arc::new(FixedClock { unix_ms: 1_000 }),
        },
    );
    let local = builds
        .local_build(&fixture.selection)
        .await?
        .ok_or("the environment has no local build")?;
    let service = fixture.service.clone().with_local_build(local);
    let plan = service.plan(&fixture.selection).await?;
    let review = plan
        .local_build
        .as_ref()
        .ok_or("the plan builds nothing first")?;
    assert_eq!(review.engine, mm::domain::build::LocalImageEngine::Podman);
    assert_eq!(review.modules.len(), 1);
    assert_eq!(review.modules[0].module, "app");
    let image = plan.stacks[0].artifact.manifest.services[0]
        .image
        .clone()
        .ok_or("the service has no image")?;
    assert!(
        image.starts_with("mm.invalid/deployment-test/app:local-"),
        "{image}"
    );
    let overlay = fs::read_to_string(&plan.stacks[0].artifact.generated_overlay_path)?;
    assert!(overlay.contains(&format!("image: {image}")), "{overlay}");
    assert!(overlay.contains("pull_policy: never"), "{overlay}");
    assert!(plan.registries.is_empty());

    service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    assert_eq!(engine.builds.load(Ordering::SeqCst), 1);
    assert!(fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    let workspace = fixture.workspace()?;
    assert!(
        fixture
            .state
            .latest_successful_image_build(workspace.project_id, &workspace.root, "local")?
            .is_none(),
        "a local build is never a published one"
    );

    let current = builds
        .local_build(&fixture.selection)
        .await?
        .ok_or("the environment has no local build")?;
    assert!(current.review().is_none(), "{:?}", current.review());
    Ok(())
}

#[tokio::test]
async fn a_build_receipt_deploys_its_digests_and_becomes_the_checkouts_build()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    make_buildable(&fixture)?;
    // Built elsewhere, as CI does: this machine's state has no build.
    let build = BuildManager::new(Arc::new(ConfigLoader), Arc::new(StaticBuildSource));
    let build_plan = build.plan(&fixture.selection, true).await?;
    let images = published_images(&build_plan)?;
    let receipt = BuildReceipt::new(
        build_plan.clone(),
        ImageBuildResult {
            operation_id: OperationId::new(),
            plan_hash: build_plan.plan_hash.clone(),
            images,
            dependencies: Vec::new(),
        },
    )?;
    let mut tampered = receipt.clone();
    tampered.result.images[0].digest = format!("sha256:{}", "e".repeat(64));
    assert!(tampered.validate().is_err());

    let workspace = fixture.workspace()?;
    let service = fixture.service.with_receipt(receipt.unpromoted()?);
    let plan = service.plan(&fixture.selection).await?;
    let image = format!("example/web@{IMAGE_DIGEST}");
    assert_eq!(
        plan.stacks[0].artifact.manifest.services[0]
            .image
            .as_deref(),
        Some(image.as_str())
    );
    service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    // Status, logs, and later plans of this checkout now find the same build.
    let (recorded_plan, recorded_result) = fixture
        .state
        .latest_successful_image_build(workspace.project_id, &workspace.root, &plan.environment)?
        .ok_or("the receipt was not recorded")?;
    assert_eq!(recorded_plan, receipt.plan);
    assert_eq!(recorded_result.images, receipt.result.images);

    // Deploying the same receipt again records nothing new.
    let again = service.plan(&fixture.selection).await?;
    service
        .apply_reviewed(&fixture.selection, &again.plan_hash)
        .await?;
    let builds = fixture
        .state
        .recent_operations(workspace.id, 20)?
        .into_iter()
        .filter(|operation| operation.kind == OperationKind::Build)
        .collect::<Vec<_>>();
    assert_eq!(builds.len(), 1);
    assert_eq!(builds[0].state, OperationState::Succeeded);
    Ok(())
}

/// A published build of the fixture for `environment`, as CI writes it.
async fn ci_receipt(
    fixture: &Fixture,
    environment: &str,
) -> Result<BuildReceipt, Box<dyn std::error::Error>> {
    let build = BuildManager::new(Arc::new(ConfigLoader), Arc::new(StaticBuildSource));
    let selection = WorkspaceSelection::new(&fixture.project, Some(environment.to_owned()));
    let build_plan = build.plan(&selection, true).await?;
    Ok(BuildReceipt::new(
        build_plan.clone(),
        ImageBuildResult {
            operation_id: OperationId::new(),
            plan_hash: build_plan.plan_hash.clone(),
            images: published_images(&build_plan)?,
            dependencies: Vec::new(),
        },
    )?)
}

#[tokio::test]
async fn a_receipt_built_for_another_environment_deploys_when_it_builds_the_same_units()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    make_buildable(&fixture)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        format!(
            "{config}  ci:\n    domains:\n      app: app.ci.example.test\n    bindings: {{}}\n"
        ),
    )?;
    let receipt = ci_receipt(&fixture, "ci").await?;
    let build = BuildManager::new(Arc::new(ConfigLoader), Arc::new(StaticBuildSource));
    let promoted = build
        .receipt_for_environment(&fixture.selection, &receipt, &ReceiptCheckout::Worktree)
        .await?;
    assert_eq!(promoted.origin, receipt.origin());
    assert_eq!(promoted.receipt.plan.environment, "local");
    assert_eq!(promoted.receipt.result.images[0].digest, IMAGE_DIGEST);

    // A build argument that only this checkout's configuration sets changes
    // the module's inputs, so the receipt no longer stands in for it.
    fs::write(
        &config_path,
        fs::read_to_string(&config_path)?.replace(
            "      image: example/web\n",
            "      image: example/web\n      args: {FEATURE: on}\n",
        ),
    )?;
    let refused = build
        .receipt_for_environment(&fixture.selection, &receipt, &ReceiptCheckout::Worktree)
        .await;
    assert!(
        matches!(&refused, Err(MmError::Build { message })
            if message.contains("module 'app'") && message.contains("build argument FEATURE")
                && message.contains("from environment 'ci'")),
        "{refused:?}"
    );
    fs::write(
        &config_path,
        fs::read_to_string(&config_path)?.replace("      args: {FEATURE: on}\n", ""),
    )?;

    let workspace = fixture.workspace()?;
    let service = fixture.service.with_receipt(promoted.clone());
    let plan = service.plan(&fixture.selection).await?;
    assert_eq!(plan.build_receipt.as_ref(), Some(&receipt.origin()));
    assert_eq!(
        plan.stacks[0].artifact.manifest.services[0]
            .image
            .as_deref(),
        Some(format!("example/web@{IMAGE_DIGEST}").as_str())
    );
    service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    // The promoted build is now this checkout's build for the environment.
    let (recorded_plan, _) = fixture
        .state
        .latest_successful_image_build(workspace.project_id, &workspace.root, "local")?
        .ok_or("the promoted receipt was not recorded")?;
    assert_eq!(recorded_plan, promoted.receipt.plan);
    Ok(())
}

/// The calls that start, stop, or remove workloads, in order.
fn workload_calls(fixture: &Fixture) -> Result<Vec<String>, MmError> {
    Ok(fixture
        .runtime
        .calls()?
        .into_iter()
        .filter(|call| {
            ["up:", "stop:", "remove:", "apply:", "converge:"]
                .iter()
                .any(|prefix| call.starts_with(prefix))
        })
        .collect())
}

#[tokio::test]
async fn a_shared_volume_is_the_storage_nodes_directory_and_a_mount_on_every_other_node()
-> Result<(), Box<dyn std::error::Error>> {
    for (node, local) in [("app-1", true), ("app-2", false)] {
        let fixture = Fixture::on_node(node)?;
        fs::write(
            fixture.project.join("compose.yaml"),
            "services:\n  web:\n    image: example/web\n    volumes: [\"uploads:/data\", \"cache:/cache\"]\n  worker: {image: example/worker}\nvolumes:\n  uploads: {}\n  cache: {}\n",
        )?;
        fs::write(
            fixture.project.join(".config/mm.yaml"),
            format!(
                "schema-version: 1
project:
  id: {PROJECT_ID}
  name: deployment-test
default-environment: production
target: local
stacks:
  app:
    project-name: deployment-test-{{environment}}-{{stack}}
    files: [compose.yaml]
    shared-volumes: [uploads]
modules:
  app:
    path: app
    services:
      web: {{stack: app, placement: all}}
      worker: {{stack: app}}
targets:
  cluster:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
environments:
  production:
    target: cluster
    bindings: {{}}
"
            ),
        )?;
        let plan = fixture.service.plan(&fixture.selection).await?;
        let shared = plan
            .node
            .clone()
            .ok_or("a node plan has a node section")?
            .shared_volumes;
        let directory = format!("{PROJECT_ID}/production/app/uploads");
        assert_eq!(
            shared.len(),
            1,
            "only what `shared-volumes` lists: {shared:?}"
        );
        let used = &shared[0];
        assert_eq!(
            (used.stack.as_str(), used.volume.as_str(), used.local),
            ("app", "uploads", local)
        );
        assert_eq!(used.storage_node, "app-1", "the first node keeps the files");
        assert_eq!(used.directory, directory);
        assert_eq!(used.services, ["web"]);

        // The overlay places the volume and leaves the Compose file alone.
        let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(
            &plan.stacks[0].artifact.generated_overlay_path,
        )?)?;
        let volumes = overlay["volumes"]
            .as_mapping()
            .ok_or("the overlay places the shared volume")?;
        assert_eq!(
            volumes.len(),
            1,
            "an ordinary volume stays the engine's own"
        );
        let volume = &overlay["volumes"]["uploads"];
        assert_eq!(volume["name"].as_str(), Some(used.engine_volume.as_str()));
        assert_eq!(volume["driver"].as_str(), Some("local"));
        let options = &volume["driver_opts"];
        if local {
            assert_eq!(options["type"].as_str(), Some("none"));
            assert_eq!(options["o"].as_str(), Some("bind"));
            assert_eq!(
                options["device"].as_str(),
                Some(format!("/var/lib/mm/storage/{directory}").as_str())
            );
        } else {
            assert_eq!(options["type"].as_str(), Some("nfs"));
            assert!(
                options["o"]
                    .as_str()
                    .is_some_and(|mount| mount.starts_with("addr=10.90.0.1,nfsvers=4.2,hard,")),
                "{options:?}"
            );
            assert_eq!(
                options["device"].as_str(),
                Some(format!("10.90.0.1:/{directory}").as_str())
            );
        }
        assert_eq!(
            volume["labels"]["io.mm.shared-volume"].as_str(),
            Some("app/uploads")
        );
        assert_eq!(
            overlay["services"]["web"]["labels"]["io.mm.shared-storage"].as_str(),
            Some(if local { "local" } else { "10.90.0.1" }),
            "the node's keeper finds the service by where its storage is"
        );
        assert!(
            overlay["services"]["worker"]["labels"]["io.mm.shared-storage"].is_null(),
            "a service that mounts no shared volume carries no such label"
        );

        // The plan says the same every time it is made: no step of a
        // rollout changes what a node answers.
        let again = fixture.service.plan(&fixture.selection).await?;
        assert_eq!(again.plan_hash, plan.plan_hash);
    }
    Ok(())
}

/// A service's part of an overlay, and its config file mounts as `(file on
/// the host, path in the container)`.
type OverlayService = (serde_yaml_ng::Value, Vec<(PathBuf, String)>);

/// What the plan's overlay for stack `app` says about `service`.
fn overlay_service(
    plan: &mm::domain::deployment::DeploymentPlan,
    service: &str,
) -> Result<OverlayService, Box<dyn std::error::Error>> {
    let stack = plan
        .stacks
        .iter()
        .find(|stack| stack.stack == "app")
        .ok_or("the plan has the stack `app`")?;
    let overlay: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(&stack.artifact.generated_overlay_path)?)?;
    let service = overlay["services"][service].clone();
    let mounts = service["volumes"]
        .as_sequence()
        .into_iter()
        .flatten()
        .filter_map(serde_yaml_ng::Value::as_str)
        .filter_map(|mount| {
            let mut parts = mount.split(':');
            let source = PathBuf::from(parts.next()?);
            let target = parts.next()?.to_owned();
            (parts.next() == Some("ro,z")).then_some((source, target))
        })
        .collect();
    Ok((service, mounts))
}

fn file_mode(path: &Path) -> Result<u32, std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::metadata(path)?.permissions().mode() & 0o7777)
}

#[tokio::test]
async fn a_changed_template_replaces_only_the_service_that_mounts_it_on_every_node()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::StackStep;
    use mm::domain::config_file::ConfigFileChange;
    for node in ["app-1", "app-2"] {
        let fixture = Fixture::on_node(node)?;
        let project = &fixture.project;
        fs::write(
            project.join("compose.yaml"),
            "services:\n  web: {image: example/web}\n  worker: {image: example/worker}\n",
        )?;
        fs::write(
            project.join("app/site.conf.tmpl"),
            "greeting {{ APP.GREETING }};\n",
        )?;
        fs::write(project.join("app/static.txt"), "static\n")?;
        let configure = |greeting: &str| {
            fs::write(
                project.join(".config/mm.yaml"),
                format!(
                    "schema-version: 1
project:
  id: {PROJECT_ID}
  name: deployment-test
default-environment: production
target: local
stacks:
  app:
    project-name: deployment-test-{{environment}}-{{stack}}
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web:
        stack: app
        placement: all
        config-files:
          /etc/web/site.conf: {{template: site.conf.tmpl}}
          /etc/web/static.txt: {{file: static.txt, mode: 0o640}}
      worker: {{stack: app, placement: all}}
targets:
  cluster:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
environments:
  production:
    target: cluster
    bindings:
      APP.GREETING: {{value: {greeting}}}
"
                ),
            )
        };
        let (service, selection) = (&fixture.service, &fixture.selection);
        let roll = |plan_hash: String| async move {
            service.begin_rollout(selection, &plan_hash).await?;
            service
                .roll_stack(selection, &plan_hash, "app", StackStep::Apply)
                .await?;
            service.finish_rollout(selection, &plan_hash).await
        };

        // Every node that runs the service renders the file for itself.
        configure("hello")?;
        let first = fixture.service.plan(&fixture.selection).await?;
        let listed = first
            .config_files
            .iter()
            .map(|file| {
                (
                    file.service.as_str(),
                    file.target.as_str(),
                    file.source.as_str(),
                    file.template,
                    file.change(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            listed,
            [
                (
                    "web",
                    "/etc/web/site.conf",
                    "app/site.conf.tmpl",
                    true,
                    ConfigFileChange::New
                ),
                (
                    "web",
                    "/etc/web/static.txt",
                    "app/static.txt",
                    false,
                    ConfigFileChange::New
                ),
            ],
            "{node}"
        );
        let (web, mounts) = overlay_service(&first, "web")?;
        let (worker, worker_mounts) = overlay_service(&first, "worker")?;
        assert!(worker_mounts.is_empty(), "the worker ships no file");
        assert_eq!(
            mounts
                .iter()
                .map(|(_, target)| target.as_str())
                .collect::<Vec<_>>(),
            ["/etc/web/site.conf", "/etc/web/static.txt"]
        );
        let (site, fixed) = (&mounts[0].0, &mounts[1].0);
        assert_eq!(fs::read_to_string(site)?, "greeting hello;\n");
        assert_eq!(fs::read_to_string(fixed)?, "static\n");
        assert_eq!(file_mode(site)?, 0o444, "readable by any user, by default");
        assert_eq!(file_mode(fixed)?, 0o640);
        assert_eq!(
            site.file_name().and_then(|name| name.to_str()),
            Some(first.config_files[0].version.as_str()),
            "the file on the host is named after its version"
        );
        roll(first.plan_hash.clone()).await?;

        // A deployment that changes no file mounts the same files.
        let same = fixture.service.plan(&fixture.selection).await?;
        assert!(
            same.config_files
                .iter()
                .all(|file| file.change() == ConfigFileChange::Unchanged),
            "{:?}",
            same.config_files
        );
        assert_eq!(overlay_service(&same, "web")?.0, web);

        // A binding the template uses changes: the plan names the service
        // that is replaced, and nothing about the other service changes.
        configure("hola")?;
        let second = fixture.service.plan(&fixture.selection).await?;
        let changes = second
            .config_files
            .iter()
            .map(|file| (file.target.as_str(), file.change()))
            .collect::<Vec<_>>();
        assert_eq!(
            changes,
            [
                ("/etc/web/site.conf", ConfigFileChange::Changed),
                ("/etc/web/static.txt", ConfigFileChange::Unchanged),
            ]
        );
        assert_eq!(
            second.config_files[0].previous.as_deref(),
            Some(first.config_files[0].version.as_str())
        );
        let (web_after, mounts_after) = overlay_service(&second, "web")?;
        assert_ne!(web_after, web, "the web service mounts another file");
        assert_eq!(
            overlay_service(&second, "worker")?.0,
            worker,
            "the worker's definition is what it was, so the engine keeps its container"
        );
        assert_eq!(fs::read_to_string(&mounts_after[0].0)?, "greeting hola;\n");
        assert_eq!(
            &mounts_after[1].0, fixed,
            "an unchanged file keeps its name"
        );
        assert!(
            site.is_file(),
            "the version the running container mounts stays until the rollout converged"
        );

        // Once the rollout is finished the superseded version is gone.
        roll(second.plan_hash.clone()).await?;
        assert!(!site.exists(), "the superseded version is pruned");
        assert!(mounts_after[0].0.is_file() && fixed.is_file());
        let directory = site.parent().ok_or("a version has a directory")?;
        assert_eq!(fs::read_dir(directory)?.count(), 2, "{node}");
    }
    Ok(())
}

/// A node's project where `web` runs three containers over two nodes
/// (two here), hardened, with a config file and a file secret, and
/// `worker` is what web's `before-deploy` job depends on.
fn write_scaled_project(project: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        project.join("compose.yaml"),
        "services:\n  web: {image: example/web, depends_on: [worker]}\n  worker: {image: example/worker}\n",
    )?;
    fs::write(project.join("app/site.conf"), "listen 8080;\n")?;
    fs::write(
        project.join(".config/mm.yaml"),
        format!(
            "schema-version: 1
project:
  id: {PROJECT_ID}
  name: deployment-test
default-environment: production
target: local
hardening: strict
stacks:
  app:
    project-name: deployment-test-{{environment}}-{{stack}}
    files: [compose.yaml]
secrets:
  database.password:
    storage: {{provider: age, location: local/db-password}}
modules:
  app:
    path: app
    env:
      template: .env.example
    services:
      web:
        stack: app
        placement: all
        replicas: 3
        config-files:
          /etc/web/site.conf: {{file: site.conf}}
        secret-files:
          database-password:
            secret: database.password
      worker: {{stack: app, placement: all, replicas: 4}}
jobs:
  migrate: {{module: app, service: web, command: [migrate], run: before-deploy}}
targets:
  cluster:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
environments:
  production:
    target: cluster
    bindings: {{}}
"
        ),
    )?;
    Ok(())
}

/// Fails with every error `mm config validate` finds in the project.
fn assert_valid(project: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let document = ConfigLoader.load_from(project)?;
    let errors = mm::adapters::config::validate(&document, "production")
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == mm::domain::diagnostic::Severity::Error)
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{errors:#?}");
    Ok(())
}

impl Fixture {
    /// The age secret `write_scaled_project` mounts into web.
    async fn create_scaled_secret(&self) -> Result<(), Box<dyn std::error::Error>> {
        let reference = SecretReference::new(
            StorageProviderId::new("age".to_owned())?,
            "local/db-password",
        )?;
        self.age.initialize()?;
        self.age
            .create(
                &SecretScope::new("production"),
                &reference,
                SecretMaterial::new("runtime-secret-material".to_owned())?,
                900,
            )
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn a_container_the_node_adds_is_made_from_what_the_deployment_ran()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::StackStep;
    let fixture = Fixture::on_node("app-1")?;
    write_scaled_project(&fixture.project)?;
    assert_valid(&fixture.project)?;
    fixture.create_scaled_secret().await?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let (service, selection) = (&fixture.service, &fixture.selection);
    service.begin_rollout(selection, &plan.plan_hash).await?;
    service
        .roll_stack(selection, &plan.plan_hash, "app", StackStep::Apply)
        .await?;
    service.finish_rollout(selection, &plan.plan_hash).await?;

    // The deployment ran this node's share: 2 of web's 3, 2 of worker's 4.
    let applied = fixture.runtime.scaled_calls()?;
    let [apply] = applied.as_slice() else {
        return Err(format!("one scaled apply: {applied:?}").into());
    };
    assert_eq!(
        apply.counts,
        BTreeMap::from([("web".to_owned(), 2), ("worker".to_owned(), 2)])
    );
    // What the deployment gave web: its config file, its secret file, and
    // `hardening: strict`.
    let overlay = apply
        .files
        .iter()
        .find(|text| text.contains("/etc/web/site.conf"))
        .ok_or("the generated overlay mounts the config file")?;
    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(overlay)?;
    assert_eq!(
        overlay["services"]["web"]["read_only"],
        serde_yaml_ng::Value::Bool(true)
    );
    assert!(
        apply
            .files
            .iter()
            .any(|text| text.contains("database-password")),
        "the runtime secret overlay gives web its file: {:?}",
        apply.files
    );

    // The node's autoscaler adds a container: Compose is given the same
    // files, with the same environment, as the deployment.
    fixture
        .service
        .scale_service(&fixture.selection, &plan.plan_hash, ("app", "web"), 3)
        .await?;
    let calls = fixture.runtime.scaled_calls()?;
    let added = calls.last().ok_or("a scaled call")?;
    assert_eq!(added.call, "scale:web");
    assert_eq!(added.counts, BTreeMap::from([("web".to_owned(), 3)]));
    assert_eq!(added.files, apply.files);
    assert_eq!(added.environment, apply.environment);
    assert!(
        apply
            .environment
            .iter()
            .any(|(_, value)| value == "runtime-public-marker"),
        "{:?}",
        apply.environment
    );

    // Only for the deployment that runs here now.
    let stale = fixture
        .service
        .scale_service(&fixture.selection, &"0".repeat(64), ("app", "web"), 3)
        .await;
    assert!(
        matches!(&stale, Err(MmError::State { message }) if message.contains("not the last one applied here")),
        "{stale:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_ahead_step_and_its_rollback_keep_each_scaled_services_count()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::StackStep;
    use mm::domain::rollout::RolloutChanges;
    let fixture = Fixture::on_node("app-1")?;
    write_scaled_project(&fixture.project)?;
    assert_valid(&fixture.project)?;
    fixture.create_scaled_secret().await?;
    let (service, selection) = (&fixture.service, &fixture.selection);
    let first = service.plan(selection).await?;
    assert_eq!(
        first
            .rollout
            .ahead
            .iter()
            .map(|placed| placed.service.as_str())
            .collect::<Vec<_>>(),
        ["worker"],
        "web's migration needs the worker first"
    );
    service.begin_rollout(selection, &first.plan_hash).await?;
    service
        .roll_stack(selection, &first.plan_hash, "app", StackStep::Apply)
        .await?;
    service.finish_rollout(selection, &first.plan_hash).await?;

    // The worker grew to three containers here meanwhile; the next
    // deployment's ahead step starts the worker alone and keeps them.
    fixture
        .runtime
        .containers
        .lock()
        .map_err(|_| "lock")?
        .insert("worker".to_owned(), 3);
    fs::write(
        fixture.project.join("compose.yaml"),
        "services:\n  web: {image: example/web, depends_on: [worker]}\n  worker: {image: example/worker:2}\n",
    )?;
    let second = service.plan(selection).await?;
    service.begin_rollout(selection, &second.plan_hash).await?;
    let ahead_outcome = service
        .roll_stack(selection, &second.plan_hash, "app", StackStep::Ahead)
        .await;
    let calls = fixture.runtime.scaled_calls()?;
    let ahead = calls.last().ok_or("the ahead step")?;
    assert_eq!(ahead.call, "ahead:worker", "{ahead_outcome:?}");
    assert_eq!(
        ahead.counts,
        BTreeMap::from([("web".to_owned(), 2), ("worker".to_owned(), 3)]),
        "the node's share, or what runs when that is more"
    );

    // The job failed: the rollback puts back only the worker, at its count.
    let mut changes = RolloutChanges::default();
    changes.note("app", true);
    assert!(
        service
            .rollback_rollout(selection, &second.plan_hash, Some(&changes))
            .await?
    );
    let calls = fixture.runtime.scaled_calls()?;
    let restored = calls.last().ok_or("the rollback")?;
    assert_eq!(restored.call, "ahead:worker");
    assert_eq!(restored.counts.get("worker"), Some(&3));
    Ok(())
}

#[tokio::test]
async fn a_single_host_keeps_a_config_file_version_until_the_deployment_that_replaces_it_converged()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::config_file::ConfigFileChange;
    let fixture = Fixture::new(false)?;
    let config = fixture.project.join(".config/mm.yaml");
    let service = "      web:\n        stack: app\n";
    let written = fs::read_to_string(&config)?;
    assert!(written.contains(service), "the fixture has the web service");
    fs::write(
        &config,
        written.replacen(
            service,
            &format!("{service}        config-files:\n          /etc/web/site.conf: {{file: site.conf}}\n"),
            1,
        ),
    )?;
    let source = fixture.project.join("app/site.conf");
    fs::write(&source, "listen 8080;\n")?;
    let first = fixture.service.plan(&fixture.selection).await?;
    assert!(first.node.is_none(), "a single host, not a cluster node");
    assert_eq!(first.config_files.len(), 1);
    assert_eq!(first.config_files[0].change(), ConfigFileChange::New);
    let old = overlay_service(&first, "web")?.1.remove(0).0;
    assert_eq!(fs::read_to_string(&old)?, "listen 8080;\n");
    fixture
        .service
        .apply_reviewed(&fixture.selection, &first.plan_hash)
        .await?;
    assert!(old.is_file());

    // The file changes and the deployment fails: the host is returned to
    // the containers that mount the earlier version, which is still there.
    fs::write(&source, "listen 8081;\n")?;
    fixture.verifier.fail_route.store(true, Ordering::SeqCst);
    let failing = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(failing.config_files[0].change(), ConfigFileChange::Changed);
    let new = overlay_service(&failing, "web")?.1.remove(0).0;
    assert_ne!(new, old);
    let failed = fixture
        .service
        .apply_reviewed(&fixture.selection, &failing.plan_hash)
        .await;
    assert!(failed.is_err(), "the route check fails");
    assert!(old.is_file(), "a failed deployment prunes nothing");

    // The same change converges: now nothing mounts the earlier version.
    fixture.verifier.fail_route.store(false, Ordering::SeqCst);
    let second = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(
        second.config_files[0].previous.as_deref(),
        Some(first.config_files[0].version.as_str()),
        "the plan compares with the last deployment that finished"
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &second.plan_hash)
        .await?;
    assert!(!old.exists(), "the superseded version is pruned");
    assert_eq!(fs::read_to_string(&new)?, "listen 8081;\n");

    // A source that is not in the checkout is refused when planning.
    fs::remove_file(&source)?;
    let missing = fixture.service.plan(&fixture.selection).await;
    assert!(
        matches!(missing, Err(MmError::Validation { errors: 1, .. })),
        "{missing:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_protected_file_is_covered_by_an_empty_file_kept_while_a_deployment_mounts_it()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::development_mount::MaskedPathKind;
    let fixture = Fixture::new(false)?;
    let config = fixture.project.join(".config/mm.yaml");
    let module = "    path: app\n";
    let written = fs::read_to_string(&config)?;
    assert!(written.contains(module), "the fixture has the app module");
    fs::write(
        &config,
        written.replacen(
            module,
            &format!("{module}    development-mount:\n      source: .\n      target: /app\n      services: [web]\n      protected: [.env, private]\n"),
            1,
        ),
    )?;
    let app = fixture.project.join("app");
    fs::write(app.join(".env"), "LOCAL_ONLY=1\n")?;

    let first = fixture.service.plan(&fixture.selection).await?;
    let masked = |plan: &mm::domain::deployment::DeploymentPlan| {
        plan.stacks[0]
            .artifact
            .manifest
            .masked_paths
            .iter()
            .map(|path| (path.container_path.clone(), path.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        masked(&first),
        [
            ("/app/.env".to_owned(), MaskedPathKind::File),
            ("/app/private".to_owned(), MaskedPathKind::Missing),
        ]
    );
    assert!(first.preserved_volumes.is_empty(), "no volume over a file");
    let mut mounts = overlay_service(&first, "web")?.1;
    assert_eq!(mounts.len(), 1, "{mounts:?}");
    let (empty, target) = mounts.remove(0);
    assert_eq!(target, "/app/.env");
    assert_eq!(fs::read(&empty)?, b"");
    assert_eq!(file_mode(&empty)?, 0o444);
    fixture
        .service
        .apply_reviewed(&fixture.selection, &first.plan_hash)
        .await?;
    assert!(empty.is_file(), "the deployment that mounts it keeps it");
    assert!(!app.join("private").exists());
    assert_eq!(fs::read_to_string(app.join(".env"))?, "LOCAL_ONLY=1\n");

    fs::remove_file(app.join(".env"))?;
    let second = fixture.service.plan(&fixture.selection).await?;
    assert_ne!(
        second.stacks[0].artifact_hash,
        first.stacks[0].artifact_hash
    );
    assert_eq!(masked(&second)[0].1, MaskedPathKind::Missing);
    assert!(overlay_service(&second, "web")?.1.is_empty());
    fixture
        .service
        .apply_reviewed(&fixture.selection, &second.plan_hash)
        .await?;
    assert!(!empty.exists(), "nothing mounts it any more");
    assert!(!app.join(".env").exists() && !app.join("private").exists());
    Ok(())
}

#[tokio::test]
async fn a_cluster_node_runs_what_is_placed_on_it_and_stops_a_moved_service_last()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::on_node("app-1")?;
    let selection = &fixture.selection;

    // Everything is placed on app-1, the first node.
    let first = fixture.service.plan(selection).await?;
    let node = first.node.clone().ok_or("a node plan has a node section")?;
    assert_eq!(node.node, "app-1");
    let names = |services: &[mm::domain::placement::PlacedService]| {
        services
            .iter()
            .map(|service| service.service.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&node.services), ["legacy", "web", "worker"]);
    assert_eq!(names(&node.starting), ["legacy", "web", "worker"]);
    assert!(node.leaving.is_empty() && node.dropped_dependencies.is_empty());
    fixture
        .service
        .apply_reviewed(selection, &first.plan_hash)
        .await?;
    assert_eq!(
        workload_calls(&fixture)?,
        ["up:app", "converge:app"],
        "a node starts its project without removing what the project does not list"
    );
    assert_eq!(fixture.runtime.runs()?, ["legacy", "web", "worker"]);
    assert_eq!(
        fixture
            .service
            .plan(selection)
            .await?
            .node
            .map(|node| node.starting),
        Some(Vec::new()),
        "what runs is not started again"
    );

    // worker moves to app-2 and legacy leaves the environment.
    write_cluster_project(
        &fixture.project,
        "      web: {stack: app}\n      worker: {stack: app, placement: app-2}",
        "services:\n  web: {image: example/web, depends_on: [worker]}\n  worker: {image: example/worker}\n",
    )?;
    fixture.runtime.clear()?;
    let moved = fixture.service.plan(selection).await?;
    assert_ne!(
        moved.plan_hash, first.plan_hash,
        "placement is part of the hash"
    );
    let node = moved.node.clone().ok_or("a node plan has a node section")?;
    assert_eq!(names(&node.services), ["web"]);
    assert_eq!(
        node.leaving
            .iter()
            .map(|leaving| (leaving.service.as_str(), leaving.moves_to.join(",")))
            .collect::<Vec<_>>(),
        [("legacy", String::new()), ("worker", "app-2".to_owned())]
    );
    assert_eq!(
        node.dropped_dependencies
            .iter()
            .map(|dropped| (dropped.service.as_str(), dropped.depends_on.as_str()))
            .collect::<Vec<_>>(),
        [("web", "worker")]
    );
    let result = fixture
        .service
        .apply_reviewed(selection, &moved.plan_hash)
        .await?;
    assert_eq!(
        workload_calls(&fixture)?,
        ["up:app", "stop:app:legacy", "converge:app"],
        "the service that is gone is removed at once; the moved one keeps running"
    );
    assert_eq!(fixture.runtime.runs()?, ["web", "worker"]);
    assert_eq!(
        result
            .moved_away
            .iter()
            .map(|service| service.service.as_str())
            .collect::<Vec<_>>(),
        ["worker"]
    );

    // Finishing another plan stops nothing; finishing this one stops worker.
    fixture.runtime.clear()?;
    let stale = fixture
        .service
        .finish_moves(selection, &first.plan_hash)
        .await
        .err()
        .ok_or("an older plan cannot be finished")?;
    assert_eq!(stale.code(), "MM_STATE");
    assert!(
        stale
            .to_string()
            .contains("is not the last one applied here"),
        "{stale}"
    );
    assert_eq!(fixture.runtime.runs()?, ["web", "worker"]);
    let stopped = fixture
        .service
        .finish_moves(selection, &moved.plan_hash)
        .await?;
    assert_eq!(names(&stopped), ["worker"]);
    assert_eq!(workload_calls(&fixture)?, ["stop:app:worker"]);
    assert_eq!(fixture.runtime.runs()?, ["web"]);
    Ok(())
}

#[tokio::test]
async fn a_rollback_leaves_a_stack_no_step_reached_as_it_runs()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::StackStep;
    let fixture = Fixture::on_node("app-1")?;
    let selection = &fixture.selection;
    let service = &fixture.service;
    let first = service.plan(selection).await?;
    service.begin_rollout(selection, &first.plan_hash).await?;
    service
        .roll_stack(selection, &first.plan_hash, "app", StackStep::Apply)
        .await?;
    service.finish_rollout(selection, &first.plan_hash).await?;
    write_cluster_project(
        &fixture.project,
        "      web: {stack: app}\n      worker: {stack: app}",
        "services:\n  web: {image: example/web:2}\n  worker: {image: example/worker}\n",
    )?;
    let second = service.plan(selection).await?;
    service.begin_rollout(selection, &second.plan_hash).await?;
    fixture.runtime.clear()?;
    // A job before the stack failed: no step changed app here.
    let nothing = mm::domain::rollout::RolloutChanges::default();
    assert!(
        service
            .rollback_rollout(selection, &second.plan_hash, Some(&nothing))
            .await?
    );
    assert!(
        workload_calls(&fixture)?.is_empty(),
        "app keeps running as it is: {:?}",
        workload_calls(&fixture)?
    );
    let latest = mm::app::ports::StateStore::latest_successful_deployment_for_owner(
        fixture.state.as_ref(),
        first.project_id,
        "production",
        fixture.target_id,
    )?;
    assert_eq!(latest.map(|plan| plan.plan_hash), Some(first.plan_hash));
    assert!(
        !service
            .rollback_rollout(selection, &second.plan_hash, None)
            .await?,
        "the rollout is closed"
    );
    Ok(())
}

/// On the VMs a rollback stopped the web its step had started while the
/// ingress proxy and the names still sent it requests (502, refused). What
/// a rollback replaces now leaves first where another node serves it, as on
/// a stop-first update, and Compose puts the earlier container back after.
#[tokio::test]
async fn a_rollback_lets_what_it_replaces_leave_before_compose_puts_the_old_back()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::StackStep;
    let fixture = Fixture::on_node("app-1")?;
    let leaving = Arc::new(RecordingLeaving(Arc::clone(&fixture.runtime)));
    let service = fixture
        .service
        .with_names(Arc::new(WebOnAppTwo))
        .with_node_host(leaving, Arc::new(NoProxy));
    let selection = &fixture.selection;
    // The names report web on `backend`, which the rollout reads only as a
    // network of the plan.
    on_backend(&fixture.project)?;
    let first = service.plan(selection).await?;
    service.begin_rollout(selection, &first.plan_hash).await?;
    service
        .roll_stack(selection, &first.plan_hash, "app", StackStep::Apply)
        .await?;
    service.finish_rollout(selection, &first.plan_hash).await?;
    write_cluster_project(
        &fixture.project,
        "      web: {stack: app, drain: 15s}\n      worker: {stack: app}",
        "services:\n  web: {image: example/web:2}\n  worker: {image: example/worker}\n",
    )?;
    on_backend(&fixture.project)?;
    let second = service.plan(selection).await?;
    service.begin_rollout(selection, &second.plan_hash).await?;
    service
        .roll_stack(selection, &second.plan_hash, "app", StackStep::Apply)
        .await?;
    *fixture.runtime.outdated.lock().map_err(|_| "poisoned")? = vec![
        ("web".to_owned(), "web-2-id".to_owned()),
        ("worker".to_owned(), "worker-2-id".to_owned()),
    ];
    fixture.runtime.clear()?;
    assert!(
        service
            .rollback_rollout(selection, &second.plan_hash, None)
            .await?
    );
    let calls = fixture
        .runtime
        .calls()?
        .into_iter()
        .filter(|call| {
            ["leave:", "recall:", "apply:"]
                .iter()
                .any(|kind| call.starts_with(kind))
        })
        // The recall names the node's Compose project of the stack.
        .map(|call| match call.strip_prefix("recall:") {
            Some(project) if project.starts_with("deployment-test-production-app") => {
                "recall:app".to_owned()
            }
            _ => call,
        })
        .collect::<Vec<_>>();
    // Only web runs on another node too; worker's only container is here,
    // and a drain would only make its gap longer. The drain is the one the
    // earlier deployment gave web: none, so the default minute.
    assert_eq!(calls, ["recall:app", "leave:web-2-id:60s", "apply:app"]);
    Ok(())
}

#[tokio::test]
async fn a_node_rolls_out_in_steps_and_returns_to_what_it_ran_before_when_asked()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::{StackOutcome, StackStep};
    let fixture = Fixture::on_node("app-1")?;
    let selection = &fixture.selection;
    let service = &fixture.service;
    // Begin opens the deployment and starts nothing.
    let first = service.plan(selection).await?;
    let project_id = first.project_id;
    let finished = |fixture: &Fixture| {
        mm::app::ports::StateStore::latest_successful_deployment_for_owner(
            fixture.state.as_ref(),
            project_id,
            "production",
            fixture.target_id,
        )
        .map(|plan| plan.map(|plan| plan.plan_hash))
    };
    assert_eq!(first.rollout.stack_order, ["app"]);
    assert_eq!(first.rollout.convergence_timeout().as_secs(), 600);
    let unbegun = service
        .roll_stack(selection, &first.plan_hash, "app", StackStep::Apply)
        .await
        .err()
        .ok_or("a stack rolled without a begin")?;
    assert!(
        unbegun.to_string().contains("has no rollout of plan"),
        "{unbegun}"
    );
    service.begin_rollout(selection, &first.plan_hash).await?;
    assert!(
        workload_calls(&fixture)?.is_empty(),
        "begin changes no workload"
    );
    let rolled = service
        .roll_stack(selection, &first.plan_hash, "app", StackStep::Apply)
        .await?;
    assert_eq!(
        rolled,
        StackOutcome::Done {
            started: Vec::new(),
            addresses: Vec::new()
        }
    );
    assert_eq!(workload_calls(&fixture)?, ["up:app", "converge:app"]);
    assert_eq!(finished(&fixture)?, None, "nothing is final before finish");
    let result = service.finish_rollout(selection, &first.plan_hash).await?;
    assert_eq!(result.applied_stacks, ["app"]);
    assert_eq!(finished(&fixture)?, Some(first.plan_hash.clone()));
    let again = service
        .finish_rollout(selection, &first.plan_hash)
        .await
        .err()
        .ok_or("a finished rollout finished twice")?;
    assert!(again.to_string().contains("nothing to finish"), "{again}");

    // The next deployment does not become healthy: the step says so and
    // leaves the node as it is; the rollback is a step of its own.
    write_cluster_project(
        &fixture.project,
        "      web: {stack: app}\n      worker: {stack: app}",
        "services:\n  web: {image: example/web:2}\n  worker: {image: example/worker}\n",
    )?;
    fixture.runtime.clear()?;
    let second = service.plan(selection).await?;
    service.begin_rollout(selection, &second.plan_hash).await?;
    fixture
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    let failed = service
        .roll_stack(selection, &second.plan_hash, "app", StackStep::Apply)
        .await?;
    let StackOutcome::Failed(failure) = failed else {
        return Err("an unhealthy stack was reported healthy".into());
    };
    assert_eq!(failure.stack, "app");
    assert_eq!(failure.reason, "injected convergence failure");
    assert_eq!(
        workload_calls(&fixture)?,
        ["up:app", "stop:app:legacy", "converge:app"],
        "nothing is undone by the step that failed"
    );
    fixture.runtime.clear()?;
    assert!(
        service
            .rollback_rollout(selection, &second.plan_hash, None)
            .await?
    );
    assert_eq!(
        workload_calls(&fixture)?,
        ["apply:app"],
        "the node applies what it ran before"
    );
    assert!(
        !fixture
            .runtime
            .calls()?
            .iter()
            .any(|call| call.starts_with("replace:")),
        "a host without routes has no route configuration to put back"
    );
    assert_eq!(finished(&fixture)?, Some(first.plan_hash.clone()));
    assert!(
        !service
            .rollback_rollout(selection, &second.plan_hash, None)
            .await?,
        "a rollout that is closed has nothing left to undo"
    );

    // A rollout nobody finished is undone when the next one begins.
    let third = service.plan(selection).await?;
    service.begin_rollout(selection, &third.plan_hash).await?;
    service
        .roll_stack(selection, &third.plan_hash, "app", StackStep::Apply)
        .await?;
    write_cluster_project(
        &fixture.project,
        "      web: {stack: app}\n      worker: {stack: app}",
        "services:\n  web: {image: example/web:3}\n  worker: {image: example/worker}\n",
    )?;
    fixture.runtime.clear()?;
    let fourth = service.plan(selection).await?;
    assert_ne!(fourth.plan_hash, third.plan_hash);
    service.begin_rollout(selection, &fourth.plan_hash).await?;
    assert_eq!(
        workload_calls(&fixture)?,
        ["apply:app"],
        "beginning undoes the rollout that was left open"
    );
    let gone = service
        .roll_stack(selection, &third.plan_hash, "app", StackStep::Apply)
        .await
        .err()
        .ok_or("the abandoned rollout went on")?;
    assert!(
        gone.to_string().contains("has no rollout of plan"),
        "{gone}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_keeps_its_last_service_until_it_runs_elsewhere_then_drops_the_project()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::on_node("app-1")?;
    let selection = &fixture.selection;
    let compose = "services:\n  web: {image: example/web}\n";
    write_cluster_project(&fixture.project, "      web: {stack: app}", compose)?;
    let first = fixture.service.plan(selection).await?;
    fixture
        .service
        .apply_reviewed(selection, &first.plan_hash)
        .await?;

    write_cluster_project(
        &fixture.project,
        "      web: {stack: app, placement: app-2}",
        compose,
    )?;
    fixture.runtime.clear()?;
    let emptied = fixture.service.plan(selection).await?;
    assert!(
        emptied.stacks.is_empty(),
        "nothing is placed on app-1 any more"
    );
    assert_eq!(emptied.removed_stacks.len(), 1);
    let result = fixture
        .service
        .apply_reviewed(selection, &emptied.plan_hash)
        .await?;
    assert!(
        workload_calls(&fixture)?.is_empty(),
        "the stack is not removed while its service has not been stopped: {:?}",
        workload_calls(&fixture)?
    );
    assert_eq!(fixture.runtime.runs()?, ["web"]);
    assert_eq!(result.moved_away.len(), 1);

    // An interrupted move is planned again from what still runs.
    let again = fixture.service.plan(selection).await?;
    assert_eq!(
        again.node.map(|node| node.leaving.len()),
        Some(1),
        "the service still runs here, so the next plan lists it"
    );

    fixture
        .service
        .finish_moves(selection, &emptied.plan_hash)
        .await?;
    assert_eq!(workload_calls(&fixture)?, ["stop:app:web:project"]);
    assert!(fixture.runtime.runs()?.is_empty());
    let settled = fixture.service.plan(selection).await?;
    assert_eq!(settled.node.map(|node| node.leaving.len()), Some(0));
    Ok(())
}

#[tokio::test]
async fn applies_a_reviewed_local_deployment_and_journals_success()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let stale_id = fixture.insert_running_operation()?;
    let plan = fixture.service.plan(&fixture.selection).await?;

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    assert_eq!(result.applied_stacks, ["app"]);
    assert_eq!(result.route_count, 1);
    assert_eq!(result.route_generation, 1);
    let routes = fixture.routes.route_set(fixture.target_id)?;
    assert_eq!(routes.claims.len(), 1);
    assert_eq!(routes.generation, 1);
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"apply:ingress".to_owned()));
    assert!(calls.contains(&"apply:app".to_owned()));
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.as_str() == "replace:dynamic.yaml")
            .count(),
        1
    );
    assert_eq!(fixture.rootless.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.privileged.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.verifier.route_calls.load(Ordering::SeqCst), 1);

    let workspace = fixture.workspace()?;
    let operations = fixture.state.recent_operations(workspace.id, 10)?;
    assert!(operations.iter().any(|operation| {
        operation.id == stale_id && operation.state == OperationState::Interrupted
    }));
    assert!(operations.iter().any(|operation| {
        operation.kind == OperationKind::Deploy && operation.state == OperationState::Succeeded
    }));
    assert!(fixture.state.running_operations(workspace.id)?.is_empty());
    assert_environment_directory_empty(&fixture.environment_directory)?;
    assert!(
        fixture
            .runtime
            .environment_file_observed
            .load(Ordering::SeqCst)
            > 0
    );
    Ok(())
}

#[tokio::test]
async fn reviewed_deployment_reconciles_scheduler_definitions_transactionally()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let base = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        format!(
            "{base}jobs:\n  cleanup:\n    module: app\n    service: web\n    command: ['true']\n    timeout: 1m\nschedules:\n  cleanup-hourly:\n    job: cleanup\n    cron: '0 * * * *'\n    timezone: UTC\n"
        ),
    )?;

    let create = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(
        create
            .scheduler_reconciliation
            .as_ref()
            .ok_or("scheduler plan was not embedded")?
            .changes
            .len(),
        2
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &create.plan_hash)
        .await?;
    let project_id = ProjectId(Uuid::parse_str(PROJECT_ID)?);
    assert!(
        fixture
            .state
            .list_jobs(project_id, "local")?
            .jobs
            .contains_key("cleanup")
    );
    assert!(
        fixture
            .state
            .list_schedules(project_id, "local")?
            .iter()
            .any(|schedule| schedule.name == "cleanup-hourly" && !schedule.retired)
    );

    fs::write(&config_path, base)?;
    let retire = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &retire.plan_hash)
        .await?;
    assert!(
        fixture
            .state
            .list_jobs(project_id, "local")?
            .jobs
            .is_empty()
    );
    assert!(
        fixture
            .state
            .list_schedules(project_id, "local")?
            .iter()
            .any(|schedule| schedule.name == "cleanup-hourly" && schedule.retired)
    );
    Ok(())
}

#[tokio::test]
async fn deployment_plans_preserve_gates_and_reject_gated_route_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;

    let mut routes = fixture.routes.route_set(fixture.target_id)?;
    let claim = &mut routes.claims[0];
    let project_id = claim.project_id;
    let environment = claim.environment.clone();
    let gate_name = "mm-gate-internal-0123456789abcdef".to_owned();
    claim.middlewares.push(gate_name.clone());
    let gate = CanonicalMiddleware::ForwardAuth {
        address:
            "http://mm-control-host:42831/_mm/gate/forward/019c527b-6242-7b43-bfd4-bd60e27c1de4"
                .to_owned(),
        trust_forward_header: false,
        auth_response_headers: vec!["X-MM-Gate-Principal".to_owned()],
        add_auth_cookies_to_response: vec!["mm_gate_session".to_owned()],
    };
    let middlewares = BTreeMap::from([(gate_name.clone(), gate.clone())]);
    fixture.routes.replace_owner_claims(
        fixture.target_id,
        project_id,
        &environment,
        &routes.claims,
        &middlewares,
        1_001,
    )?;

    let update = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(update.desired_middlewares.get(&gate_name), Some(&gate));
    assert!(
        update.desired_route_claims[0]
            .middlewares
            .contains(&gate_name)
    );
    let rendered = fs::read_to_string(&update.dynamic_config.path)?;
    assert!(rendered.contains(&gate_name));
    assert!(rendered.contains("forwardAuth"));

    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace("app.example.test", "changed.example.test"),
    )?;
    assert!(matches!(
        fixture.service.plan(&fixture.selection).await,
        Err(MmError::Security { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn pre_release_v2_through_v6_plans_are_rejected_before_snapshot_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let current = fixture.service.plan(&fixture.selection).await?;
    let workspace = fixture.workspace()?;

    let mut incomplete = serde_json::to_value(&current)?;
    incomplete
        .as_object_mut()
        .ok_or("deployment plan was not an object")?
        .remove("networks");
    assert!(serde_json::from_value::<mm::domain::deployment::DeploymentPlan>(incomplete).is_err());

    for field in [
        "provenance",
        "certificate_reference",
        "ownership",
        "middlewares",
    ] {
        let mut incomplete = serde_json::to_value(&current)?;
        incomplete["desired_route_claims"][0]
            .as_object_mut()
            .ok_or("deployment route claim was not an object")?
            .remove(field);
        assert!(
            serde_json::from_value::<mm::domain::deployment::DeploymentPlan>(incomplete).is_err()
        );
    }

    for version in 2..=6 {
        let mut plan = current.clone();
        plan.schema = format!("mm-local-deployment-plan-v{version}");
        let operation_id = OperationId::new();
        fixture.state.begin_operation(&NewOperation {
            id: operation_id,
            workspace_id: workspace.id,
            kind: OperationKind::Deploy,
            environment: plan.environment.clone(),
            requested_at_unix_ms: 750,
            plan_hash: Some(plan.plan_hash.clone()),
        })?;
        assert!(
            fixture
                .state
                .save_deployment_plan(operation_id, &plan)
                .is_err()
        );
        assert!(fixture.state.deployment_plan(operation_id)?.is_none());
    }
    Ok(())
}

#[tokio::test]
async fn deployment_plan_canonical_contract_rejects_cross_owner_and_inconsistent_state()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(plan.canonical_hash()?, plan.plan_hash);
    let foreign_target = TargetId(Uuid::parse_str("419c527b-6242-7b43-bfd4-bd60e27c1de3")?);
    let foreign_project = ProjectId(Uuid::parse_str("519c527b-6242-7b43-bfd4-bd60e27c1de3")?);
    let runtime_secret_plan = mm::domain::runtime_secret::RuntimeSecretPlan::new(
        plan.project_id.0.to_string(),
        plan.target_id.0.to_string(),
        plan.environment.clone(),
        plan.runtime.ok_or("deployment has no runtime")?,
        Vec::new(),
    )?;
    let mut remote_binding = mm::domain::remote::RemotePlanBinding {
        protocol_version: mm::domain::remote::PROTOCOL_VERSION,
        daemon_version: "1.0.0".to_owned(),
        daemon_fingerprint: "a".repeat(64),
        target_fingerprint: plan.target_fingerprint.clone(),
        target: plan.target_name.clone(),
        node: None,
        kind: mm::domain::remote::RemotePlanKind::Deployment,
        project_id: plan.project_id,
        environment: plan.environment.clone(),
        runtime: plan.runtime.ok_or("deployment has no runtime")?,
        commit: "e".repeat(40),
        deployment_plan_hash: "f".repeat(64),
        access_change_hash: "f".repeat(64),
        principal_id: mm::domain::access::PrincipalId(Uuid::from_u128(88)),
        authorization_revision: 1,
        authorization_hash: "b".repeat(64),
        plan_nonce: Uuid::from_u128(89),
        expires_at_unix_ms: 10_000,
        bundle_hash: None,
        plan_hash: String::new(),
    };
    remote_binding.plan_hash = remote_binding.canonical_hash()?;
    assert!(
        mm::domain::remote::RemoteDeploymentPlan {
            binding: remote_binding,
            plan: Some(plan.clone()),
            access: mm::domain::users::AccessChange::between(
                None,
                mm::domain::users::reconcile_users(
                    (&Default::default(), &Default::default()),
                    None,
                    plan.project_id,
                    &plan.environment,
                    &"e".repeat(40),
                    1,
                    "test",
                )?,
            ),
            receipt: None,
            unfinished: None,
            once: Default::default(),
            staged: Vec::new(),
            release: None,
        }
        .validate()
        .is_err()
    );

    macro_rules! rejects {
        ($change:expr) => {{
            let mut changed = plan.clone();
            $change(&mut changed);
            assert!(changed.canonical_hash().is_err());
        }};
    }
    rejects!(
        |plan: &mut mm::domain::deployment::DeploymentPlan| plan.schema =
            "mm-local-deployment-plan-v2".to_owned()
    );
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| plan
        .dynamic_config
        .manifest
        .target_id = foreign_target);
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| plan
        .previous_dynamic_config
        .manifest
        .target_id = foreign_target);
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| plan
        .expected_route_config_digest =
        Some("stale".to_owned()));
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| plan
        .dynamic_config
        .manifest
        .claim_count += 1);
    rejects!(
        |plan: &mut mm::domain::deployment::DeploymentPlan| plan.complete_route_claims[0]
            .target_id = foreign_target
    );
    rejects!(
        |plan: &mut mm::domain::deployment::DeploymentPlan| plan.desired_route_claims[0]
            .project_id = foreign_project
    );
    rejects!(
        |plan: &mut mm::domain::deployment::DeploymentPlan| plan.desired_route_claims[0]
            .environment = "foreign".to_owned()
    );
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        let mut claim = plan.desired_route_claims[0].clone();
        claim.target_id = foreign_target;
        plan.previous_owner_claims.push(claim);
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| plan
        .stacks
        .push(plan.stacks[0].clone()));
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        let mut duplicate = plan.stacks[0].clone();
        duplicate.stack = "other".to_owned();
        duplicate.artifact.manifest.stack = "other".to_owned();
        plan.stacks.push(duplicate);
    });
    rejects!(
        |plan: &mut mm::domain::deployment::DeploymentPlan| plan.stacks[0]
            .artifact
            .manifest
            .project_name = "other".to_owned()
    );
    rejects!(
        |plan: &mut mm::domain::deployment::DeploymentPlan| plan.stacks[0].artifact_hash =
            "f".repeat(64)
    );
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.stacks[0].artifact_hash = "f".repeat(64);
        plan.stacks[0].artifact.manifest.artifact_hash = "f".repeat(64);
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.removed_stacks.push(plan.stacks[0].clone());
        plan.stacks.clear();
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| plan
        .static_ingress
        .as_mut()
        .map(|ingress| ingress.target_id = foreign_target));

    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.runtime_secret_plan = Some(runtime_secret_plan.clone());
        if let Some(runtime) = plan.runtime_secret_plan.as_mut() {
            runtime.project_id = foreign_project.0.to_string();
        }
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.runtime_secret_plan = Some(runtime_secret_plan.clone());
        if let Some(runtime) = plan.runtime_secret_plan.as_mut() {
            runtime.target_id = foreign_target.0.to_string();
        }
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.runtime_secret_plan = Some(runtime_secret_plan.clone());
        if let Some(runtime) = plan.runtime_secret_plan.as_mut() {
            runtime.environment = "foreign".to_owned();
        }
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.runtime_secret_plan = Some(runtime_secret_plan.clone());
        if let Some(runtime) = plan.runtime_secret_plan.as_mut() {
            runtime.plan_hash = "stale".to_owned();
        }
    });

    let mut with_runtime_secrets = plan.clone();
    with_runtime_secrets.runtime_secret_plan = Some(runtime_secret_plan);
    assert_ne!(with_runtime_secrets.canonical_hash()?, plan.plan_hash);
    if plan.scheduler_reconciliation.is_some() {
        rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
            if let Some(reconciliation) = plan.scheduler_reconciliation.as_mut() {
                reconciliation.project_id = foreign_project;
            }
        });
    }
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.removed_stacks = vec![plan.stacks[0].clone(), plan.stacks[0].clone()];
        plan.stacks.clear();
        plan.expected_previous_plan_hash = Some("previous".to_owned());
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.removed_stacks.push(plan.stacks[0].clone());
        plan.expected_previous_plan_hash = Some("previous".to_owned());
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        let mut removed = plan.stacks.remove(0);
        removed.artifact.manifest.environment = "foreign".to_owned();
        plan.removed_stacks.push(removed);
        plan.expected_previous_plan_hash = Some("previous".to_owned());
    });

    let volume = mm::domain::development_mount::PreservedVolumeManifest {
        module: "web".to_owned(),
        container_path: "/workspace/node_modules".to_owned(),
        logical_name: "node-modules".to_owned(),
        physical_name: "mm-example-node-modules".to_owned(),
        resource: "volume".to_owned(),
    };
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.preserved_volumes = vec![volume.clone(), volume.clone()];
    });
    rejects!(|plan: &mut mm::domain::deployment::DeploymentPlan| {
        plan.preserved_volumes.push(volume.clone());
        plan.removed_preserved_volumes.push(volume.clone());
        plan.expected_previous_plan_hash = Some("previous".to_owned());
    });

    let mut with_network_and_volume = plan.clone();
    with_network_and_volume
        .networks
        .push(mm::domain::stack::CanonicalNetworkIntent {
            internal: false,
            logical_name: "frontend".to_owned(),
            physical_name: "mm-example-frontend".to_owned(),
            members: vec![mm::domain::stack::CanonicalNetworkMember {
                stack: "app".to_owned(),
                service: "web".to_owned(),
            }],
        });
    with_network_and_volume.preserved_volumes.push(volume);
    assert_ne!(with_network_and_volume.canonical_hash()?, plan.plan_hash);

    let mut with_removed_volume = plan.clone();
    with_removed_volume
        .removed_preserved_volumes
        .push(with_network_and_volume.preserved_volumes[0].clone());
    with_removed_volume.expected_previous_plan_hash = Some(plan.plan_hash.clone());
    assert_ne!(with_removed_volume.canonical_hash()?, plan.plan_hash);

    let mut with_ordered_claims = plan.clone();
    let mut second_claim = with_ordered_claims.desired_route_claims[0].clone();
    second_claim.artifact_hash = "f".repeat(64);
    with_ordered_claims.desired_route_claims.push(second_claim);
    assert_ne!(with_ordered_claims.canonical_hash()?, plan.plan_hash);
    for provenance in [
        mm::domain::ingress::IngressRouteProvenance::ExplicitConfig,
        mm::domain::ingress::IngressRouteProvenance::AutomaticConvention,
        mm::domain::ingress::IngressRouteProvenance::MmManagedService,
    ] {
        let mut changed = plan.clone();
        changed.desired_route_claims[0].provenance = provenance;
        changed.canonical_hash()?;
    }

    let mut valid_removal = plan.clone();
    valid_removal
        .removed_stacks
        .push(valid_removal.stacks.remove(0));
    valid_removal.expected_previous_plan_hash = Some(plan.plan_hash.clone());
    assert_ne!(valid_removal.canonical_hash()?, plan.plan_hash);

    let mut duplicate_removed_project = valid_removal.clone();
    let mut second_removed = duplicate_removed_project.removed_stacks[0].clone();
    second_removed.stack = "other".to_owned();
    second_removed.artifact.manifest.stack = "other".to_owned();
    second_removed.artifact.manifest.artifact_hash =
        second_removed.artifact.manifest.canonical_hash()?;
    second_removed.artifact_hash = second_removed.artifact.manifest.artifact_hash.clone();
    duplicate_removed_project
        .removed_stacks
        .push(second_removed);
    assert!(duplicate_removed_project.canonical_hash().is_err());

    if let Some(ingress) = &plan.static_ingress {
        let mut invalid_removal = plan.clone();
        invalid_removal.removed_ingress = Some(ingress.clone());
        assert!(invalid_removal.canonical_hash().is_err());

        let mut valid_shape = plan.clone();
        valid_shape.static_ingress = None;
        valid_shape.complete_route_claims.clear();
        valid_shape.removed_ingress = Some(ingress.clone());
        valid_shape.expected_previous_plan_hash = Some(plan.plan_hash.clone());
        valid_shape
            .removed_ingress
            .as_mut()
            .ok_or("removed ingress missing")?
            .target_id = foreign_target;
        assert!(valid_shape.canonical_hash().is_err());

        let mut valid_shape = plan.clone();
        valid_shape.static_ingress = None;
        valid_shape.complete_route_claims.clear();
        valid_shape.dynamic_config.manifest.claim_count = 0;
        valid_shape.removed_ingress = Some(ingress.clone());
        valid_shape.expected_previous_plan_hash = Some(plan.plan_hash.clone());
        assert_ne!(valid_shape.canonical_hash()?, plan.plan_hash);

        let mut mismatched_removal = valid_shape;
        if let Some(removed) = mismatched_removal.removed_ingress.as_mut() {
            removed.target_id = foreign_target;
        }
        assert_eq!(
            mismatched_removal
                .canonical_hash()
                .as_ref()
                .map(String::as_str)
                .map_err(String::as_str),
            Err("removed deployment ingress does not match an empty target route set")
        );
    }

    let baseline = plan.canonical_hash()?;
    for runtime in [
        None,
        Some(RuntimeKind::PodmanCompose),
        Some(RuntimeKind::DockerCompose),
    ] {
        let mut changed = plan.clone();
        changed.runtime = runtime;
        if changed.runtime_secret_plan.is_none() && runtime != plan.runtime {
            assert_ne!(changed.canonical_hash()?, baseline);
        }
    }
    Ok(())
}

#[tokio::test]
async fn restores_previous_dynamic_routes_and_journals_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(true)?;
    let plan = fixture.service.plan(&fixture.selection).await?;

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await;

    assert!(matches!(result, Err(MmError::RuntimeResponse { .. })));
    let routes = fixture.routes.route_set(fixture.target_id)?;
    assert!(routes.claims.is_empty());
    assert_eq!(routes.generation, 0);
    assert_eq!(
        fixture
            .runtime
            .calls()?
            .iter()
            .filter(|call| call.as_str() == "replace:dynamic.yaml")
            .count(),
        2,
        "the second volume write must restore the previous route file"
    );
    assert_eq!(fixture.rootless.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.privileged.calls.load(Ordering::SeqCst), 0);
    assert!(fixture.runtime.calls()?.contains(&"remove:app".to_owned()));
    let operations = fixture
        .state
        .recent_operations(fixture.workspace()?.id, 10)?;
    assert!(operations.iter().any(|operation| {
        operation.state == OperationState::Failed
            && operation.error_code.as_deref() == Some("MM_RUNTIME_RESPONSE")
    }));
    assert_environment_directory_empty(&fixture.environment_directory)?;
    Ok(())
}

#[tokio::test]
async fn direct_delivery_has_only_placeholders_during_apply_and_cleans_afterward()
-> Result<(), Box<dyn std::error::Error>> {
    const VALUE: &str = "runtime-public-marker";
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace("  local:\n", "  local:\n    environment-delivery: direct\n"),
    )?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let repeated = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(plan.plan_hash, repeated.plan_hash);
    assert_eq!(
        plan.environment_review.delivery.mode,
        mm::domain::environment_delivery::EnvironmentDeliveryMode::Direct
    );

    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    assert!(fixture.runtime.direct_overlay_safe.load(Ordering::SeqCst));
    assert!(
        fixture
            .runtime
            .direct_environment_count
            .load(Ordering::SeqCst)
            > 0
    );
    assert_environment_directory_empty(&fixture.environment_directory)?;
    assert!(
        !fs::read(&fixture.state_path)?
            .windows(VALUE.len())
            .any(|window| window == VALUE.as_bytes())
    );
    assert!(
        fixture
            .runtime
            .calls()?
            .iter()
            .all(|call| !call.contains(VALUE))
    );
    Ok(())
}

#[tokio::test]
async fn direct_delivery_capability_mismatch_fails_during_planning_before_runtime_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        config_path,
        config.replace("  local:\n", "  local:\n    environment-delivery: direct\n"),
    )?;
    fixture
        .runtime
        .support_direct
        .store(false, Ordering::SeqCst);

    let result = fixture.service.plan(&fixture.selection).await;

    assert!(matches!(
        result,
        Err(MmError::UnsupportedCapability { operation, required, .. })
            if operation == "direct_environment_delivery"
                && required == [RuntimeCapability::DirectEnvironment]
    ));
    assert!(fixture.runtime.calls()?.is_empty());
    assert_environment_directory_empty(&fixture.environment_directory)?;
    Ok(())
}

#[tokio::test]
async fn cancellation_drops_private_environment_artifacts() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new(false)?;
    fixture
        .runtime
        .block_app_apply
        .store(true, Ordering::SeqCst);
    let plan = fixture.service.plan(&fixture.selection).await?;
    let service = fixture.service.clone();
    let selection = fixture.selection.clone();
    let plan_hash = plan.plan_hash;
    let handle = tokio::spawn(async move { service.apply_reviewed(&selection, &plan_hash).await });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !fixture.runtime.app_apply_entered.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await?;
    assert!(fixture.environment_directory.is_dir());
    assert!(
        fixture
            .runtime
            .environment_file_observed
            .load(Ordering::SeqCst)
            > 0
    );
    handle.abort();
    let _ = handle.await;
    assert_environment_directory_empty(&fixture.environment_directory)?;
    Ok(())
}

#[tokio::test]
async fn reapplies_the_previous_workload_after_a_failed_update()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fixture.runtime.clear()?;
    fixture
        .runtime
        .environment_file_observed
        .store(0, Ordering::SeqCst);
    let compose = fixture.project.join("compose.yaml");
    fs::write(
        compose,
        "services:\n  web:\n    image: example/web-v2\n    expose: [8080]\n",
    )?;
    fixture.verifier.fail_route.store(true, Ordering::SeqCst);
    let update = fixture.service.plan(&fixture.selection).await?;

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &update.plan_hash)
        .await;

    assert!(matches!(result, Err(MmError::RuntimeResponse { .. })));
    let calls = fixture.runtime.calls()?;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.as_str() == "apply:app")
            .count(),
        2,
        "the update and then the previous workload must both be applied"
    );
    assert_eq!(
        fixture
            .runtime
            .environment_file_observed
            .load(Ordering::SeqCst),
        2,
        "the update and rollback must both observe the guarded environment file"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.as_str() == "replace:dynamic.yaml")
            .count(),
        3
    );
    assert_eq!(fixture.routes.route_set(fixture.target_id)?.generation, 1);
    assert_eq!(fixture.rootless.calls.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.privileged.calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn removes_a_replacement_compose_project_after_a_failed_update()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fixture.runtime.clear()?;
    let config = fixture.project.join(".config/mm.yaml");
    let source = fs::read_to_string(&config)?;
    fs::write(
        config,
        source.replace(
            "deployment-test-{environment}-{stack}",
            "replacement-{environment}-{stack}",
        ),
    )?;
    fixture.verifier.fail_route.store(true, Ordering::SeqCst);
    let update = fixture.service.plan(&fixture.selection).await?;

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &update.plan_hash)
        .await;

    assert!(matches!(result, Err(MmError::RuntimeResponse { .. })));
    assert!(fixture.runtime.calls()?.contains(&"remove:app".to_owned()));
    Ok(())
}

#[tokio::test]
async fn removes_deleted_stacks_and_uninstalls_final_route_ingress()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fixture.runtime.clear()?;
    write_empty_project_config(&fixture.project)?;

    let removal = fixture.service.plan(&fixture.selection).await?;

    assert_eq!(removal.removed_stacks.len(), 1);
    assert_eq!(removal.removed_stacks[0].stack, "app");
    assert!(removal.stacks.is_empty());
    assert!(removal.removed_ingress.is_some());
    let mut invalid_removal = removal.clone();
    invalid_removal
        .complete_route_claims
        .push(initial.complete_route_claims[0].clone());
    assert!(invalid_removal.canonical_hash().is_err());
    assert_eq!(
        removal.expected_previous_plan_hash.as_deref(),
        Some(initial.plan_hash.as_str())
    );
    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &removal.plan_hash)
        .await?;

    assert_eq!(result.removed_stacks, ["app"]);
    assert!(result.applied_stacks.is_empty());
    assert_eq!(result.route_count, 0);
    assert!(fixture.runtime.calls()?.contains(&"remove:app".to_owned()));
    assert_eq!(fixture.rootless.uninstall_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.privileged.uninstall_calls.load(Ordering::SeqCst), 1);
    assert!(
        fixture
            .routes
            .route_set(fixture.target_id)?
            .claims
            .is_empty()
    );

    let settled = fixture.service.plan(&fixture.selection).await?;
    assert!(settled.removed_stacks.is_empty());
    assert!(settled.removed_ingress.is_none());
    Ok(())
}

#[tokio::test]
async fn restores_a_deleted_stack_when_removal_fails() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fixture.runtime.clear()?;
    write_empty_project_config(&fixture.project)?;
    fixture.runtime.fail_remove.store(true, Ordering::SeqCst);
    let removal = fixture.service.plan(&fixture.selection).await?;

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &removal.plan_hash)
        .await;

    assert!(matches!(result, Err(MmError::RuntimeResponse { .. })));
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"remove:app".to_owned()));
    assert!(calls.contains(&"apply:app".to_owned()));
    assert_eq!(fixture.routes.route_set(fixture.target_id)?.claims.len(), 1);
    assert_eq!(fixture.rootless.uninstall_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.privileged.uninstall_calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn preserves_shared_ingress_when_another_project_still_owns_a_route()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    let mut other = fixture.routes.route_set(fixture.target_id)?.claims[0].clone();
    let other_project = ProjectId(Uuid::from_u128(0x119c527b62427b43bfd4bd60e27c1de3));
    other.project_id = other_project;
    other.project_name = "other-project".to_owned();
    other.stack = "other".to_owned();
    other.route = "other-route".to_owned();
    other.ownership = mm::domain::ingress::IngressRouteOwnership::Project {
        project_id: other_project,
        environment: other.environment.clone(),
        stack: other.stack.clone(),
        service: other.service.clone(),
    };
    other.host = "other.example.test".to_owned();
    other.router_name = "other-router".to_owned();
    other.backend_alias = "other-backend".to_owned();
    fixture.routes.replace_owner_claims(
        fixture.target_id,
        other_project,
        "local",
        &[other],
        &std::collections::BTreeMap::new(),
        1_001,
    )?;
    write_empty_project_config(&fixture.project)?;

    let removal = fixture.service.plan(&fixture.selection).await?;

    assert!(removal.removed_ingress.is_none());
    assert!(removal.static_ingress.is_some());
    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &removal.plan_hash)
        .await?;
    assert_eq!(result.route_count, 1);
    assert_eq!(fixture.rootless.uninstall_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.privileged.uninstall_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.rootless.calls.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.privileged.calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn renaming_an_automatic_service_replaces_its_owned_route_and_certificate()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace("    ingress:\n      domain: app\n", "")
        .replace(
            "  local:\n    domains:\n      app: app.example.test\n    bindings: {}\n",
            "  local:\n    bindings: {}\n",
        );
    fs::write(&config_path, config)?;

    let initial = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(
        initial.desired_route_claims[0].host,
        "web.deployment-test.test"
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    let old_certificate = fixture.ca.certificate_path("web.deployment-test.test")?;
    assert!(old_certificate.is_file());

    fs::write(
        fixture.project.join("compose.yaml"),
        "services:\n  api:\n    image: example/web\n    expose: [8080]\n",
    )?;
    let config = fs::read_to_string(&config_path)?.replace(
        "      web:\n        stack: app\n",
        "      api:\n        stack: app\n",
    );
    fs::write(&config_path, config)?;

    let rename = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(
        rename.previous_owner_claims[0].host,
        "web.deployment-test.test"
    );
    assert_eq!(
        rename.desired_route_claims[0].host,
        "api.deployment-test.test"
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &rename.plan_hash)
        .await?;

    let claims = fixture.routes.route_set(fixture.target_id)?.claims;
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].host, "api.deployment-test.test");
    assert!(!old_certificate.exists());
    assert!(
        fixture
            .ca
            .certificate_path("api.deployment-test.test")?
            .is_file()
    );
    Ok(())
}

#[tokio::test]
async fn stop_removes_stack_containers_and_keeps_routes_for_the_next_apply()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    fixture.runtime.clear()?;

    let stopped = fixture.service.stop(&fixture.selection).await?;

    assert_eq!(stopped, ["app"]);
    assert_eq!(fixture.runtime.calls()?, ["remove:app"]);
    assert_eq!(fixture.rootless.uninstall_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.routes.route_set(fixture.target_id)?.claims.len(), 1);

    let restart = fixture.service.plan(&fixture.selection).await?;
    assert!(restart.removed_stacks.is_empty());
    fixture
        .service
        .apply_reviewed(&fixture.selection, &restart.plan_hash)
        .await?;
    assert!(fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    Ok(())
}

#[tokio::test]
async fn rejects_a_stale_plan_before_runtime_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let config = fixture.project.join(".config/mm.yaml");
    let source = fs::read_to_string(&config)?;
    fs::write(
        config,
        source.replace("app.example.test", "replacement.example.test"),
    )?;

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await;

    assert!(matches!(result, Err(MmError::State { .. })));
    assert!(fixture.runtime.calls()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn recovers_a_crash_after_the_route_commit_as_success()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let first = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &first.plan_hash)
        .await?;
    let workspace = fixture.workspace()?;
    let completed = fixture
        .state
        .recent_operations(workspace.id, 1)?
        .into_iter()
        .next()
        .ok_or_else(|| std::io::Error::other("deployment operation is missing"))?;
    let connection = rusqlite::Connection::open(&fixture.state_path)?;
    connection.execute(
        "UPDATE operations SET state = 'running', finished_at_ms = NULL WHERE id = ?1",
        [completed.id.0.to_string()],
    )?;
    let retry = fixture.service.plan(&fixture.selection).await?;

    fixture
        .service
        .apply_reviewed(&fixture.selection, &retry.plan_hash)
        .await?;

    let recovered = fixture
        .state
        .recent_operations(workspace.id, 10)?
        .into_iter()
        .find(|operation| operation.id == completed.id)
        .ok_or_else(|| std::io::Error::other("recovered operation is missing"))?;
    assert_eq!(recovered.state, OperationState::Succeeded);
    assert_eq!(recovered.error_code, None);
    Ok(())
}

#[tokio::test]
async fn failure_injection_preflight_first_mutation_mid_apply_and_verification_is_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let preflight = Fixture::new(false)?;
    let plan = preflight.service.plan(&preflight.selection).await?;
    preflight
        .runtime
        .fail_preflight
        .store(true, Ordering::SeqCst);
    assert!(matches!(
        preflight
            .service
            .apply_reviewed(&preflight.selection, &plan.plan_hash)
            .await,
        Err(MmError::Security { .. })
    ));
    assert_eq!(preflight.runtime.calls()?.len(), 1);

    for mutation in [1, 6] {
        let fixture = Fixture::new(false)?;
        let plan = fixture.service.plan(&fixture.selection).await?;
        fixture
            .runtime
            .fail_mutation_at
            .store(mutation, Ordering::SeqCst);
        assert!(matches!(
            fixture
                .service
                .apply_reviewed(&fixture.selection, &plan.plan_hash)
                .await,
            Err(MmError::RuntimeResponse { .. })
        ));
        let calls = fixture.runtime.calls()?;
        assert!(calls.contains(&"remove:app".to_owned()));
        assert!(calls.contains(&"remove:ingress".to_owned()));
        assert!(
            fixture
                .routes
                .route_set(fixture.target_id)?
                .claims
                .is_empty()
        );
        assert!(
            fixture
                .state
                .running_operations(fixture.workspace()?.id)?
                .is_empty()
        );
    }

    let verification = Fixture::new(false)?;
    let plan = verification.service.plan(&verification.selection).await?;
    verification
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    assert!(matches!(
        verification
            .service
            .apply_reviewed(&verification.selection, &plan.plan_hash)
            .await,
        Err(MmError::RuntimeResponse { .. })
    ));
    assert!(
        verification
            .runtime
            .calls()?
            .contains(&"remove:app".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn route_commit_and_rollback_failures_are_deterministic_and_journalled()
-> Result<(), Box<dyn std::error::Error>> {
    let commit = Fixture::new(false)?;
    let plan = commit.service.plan(&commit.selection).await?;
    commit
        .verifier
        .fail_route_commit
        .store(true, Ordering::SeqCst);
    assert!(matches!(
        commit
            .service
            .apply_reviewed(&commit.selection, &plan.plan_hash)
            .await,
        Err(MmError::State { .. })
    ));
    assert!(commit.runtime.calls()?.contains(&"remove:app".to_owned()));

    let rollback = Fixture::new(true)?;
    rollback.runtime.fail_remove.store(true, Ordering::SeqCst);
    let plan = rollback.service.plan(&rollback.selection).await?;
    let rollback_result = rollback
        .service
        .apply_reviewed(&rollback.selection, &plan.plan_hash)
        .await;
    assert!(matches!(
        rollback_result,
        Err(MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Unknown,
            cleanup_failures,
            primary,
            ..
        }) if cleanup_failures.iter().any(|failure| failure.contains("simulated Compose removal failure"))
            && primary.code() == "MM_RUNTIME_RESPONSE"
    ));
    let operations = rollback
        .state
        .recent_operations(rollback.workspace()?.id, 10)?;
    assert!(operations.iter().any(|operation| {
        operation.state == OperationState::Failed
            && operation.error_code.as_deref() == Some("MM_RUNTIME_RESPONSE")
    }));
    Ok(())
}

#[tokio::test]
async fn interrupted_precommit_update_restores_the_baseline_before_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fs::write(
        fixture.project.join("compose.yaml"),
        "services:\n  web:\n    image: example/web-v2\n    expose: [8080]\n",
    )?;
    let update = fixture.service.plan(&fixture.selection).await?;
    let workspace = fixture.workspace()?;
    let interrupted = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id: interrupted,
        workspace_id: workspace.id,
        kind: OperationKind::Deploy,
        environment: "local".to_owned(),
        requested_at_unix_ms: 900,
        plan_hash: Some(update.plan_hash.clone()),
    })?;
    fixture.state.save_deployment_plan(interrupted, &update)?;
    fixture.runtime.clear()?;
    fixture
        .runtime
        .environment_file_observed
        .store(0, Ordering::SeqCst);

    let config_path = fixture.project.join(".config/mm.yaml");
    fs::write(
        &config_path,
        format!(
            "{}\n# configuration changed after the interruption\n",
            fs::read_to_string(&config_path)?
        ),
    )?;
    fixture.installation.0.store(true, Ordering::SeqCst);
    let retry = fixture.service.plan(&fixture.selection).await?;
    assert_ne!(
        retry.environment_review.digest(),
        update.environment_review.digest()
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &retry.plan_hash)
        .await?;

    let calls = fixture.runtime.calls()?;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.as_str() == "apply:ingress")
            .count(),
        1
    );
    assert_eq!(fixture.privileged.calls.load(Ordering::SeqCst), 2);
    assert!(
        calls
            .iter()
            .filter(|call| call.as_str() == "apply:app")
            .count()
            >= 2,
        "recovery must reapply the baseline before the reviewed retry"
    );
    assert!(
        fixture
            .runtime
            .environment_file_observed
            .load(Ordering::SeqCst)
            >= 2,
        "recovery and retry must both observe guarded environment files"
    );
    let recovered = fixture
        .state
        .recent_operations(workspace.id, 10)?
        .into_iter()
        .find(|operation| operation.id == interrupted)
        .ok_or_else(|| std::io::Error::other("interrupted operation is missing"))?;
    assert_eq!(recovered.state, OperationState::Interrupted);
    assert_eq!(
        recovered.error_code.as_deref(),
        Some("MM_INTERRUPTED_RECOVERED")
    );
    Ok(())
}

#[tokio::test]
async fn empty_reviewed_deployment_performs_preflight_without_runtime_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    write_empty_project_config(&fixture.project)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    assert!(plan.stacks.is_empty());
    assert!(plan.static_ingress.is_none());
    assert!(!plan.route_state_changed);

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    assert!(result.applied_stacks.is_empty());
    assert!(result.removed_stacks.is_empty());
    assert_eq!(result.route_count, 0);
    assert_eq!(fixture.runtime.calls()?, ["preflight:0:0"]);
    assert_eq!(fixture.rootless.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.privileged.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn rootful_podman_applies_like_rootless_podman() -> Result<(), Box<dyn std::error::Error>> {
    // `mmd` drives rootful Podman on a remote host. Which mode the engine
    // may be in for the process is the adapter's to say (its probe is
    // unavailable for a rootful engine driven by another user), not the
    // deployment's.
    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime
        .report_non_rootless
        .store(true, Ordering::SeqCst);

    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    assert!(!fixture.runtime.calls()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn ingress_installer_failures_restore_routes_and_skip_later_installers()
-> Result<(), Box<dyn std::error::Error>> {
    let rootless = Fixture::new(false)?;
    rootless.rootless.fail_install.store(true, Ordering::SeqCst);
    let plan = rootless.service.plan(&rootless.selection).await?;
    assert!(matches!(
        rootless
            .service
            .apply_reviewed(&rootless.selection, &plan.plan_hash)
            .await,
        Err(MmError::RuntimeResponse { .. })
    ));
    assert_eq!(rootless.rootless.calls.load(Ordering::SeqCst), 1);
    assert_eq!(rootless.privileged.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        rootless
            .runtime
            .dynamic_replace_count
            .load(Ordering::SeqCst),
        2
    );
    assert!(rootless.runtime.calls()?.contains(&"remove:app".to_owned()));

    let privileged = Fixture::new(false)?;
    privileged
        .privileged
        .fail_install
        .store(true, Ordering::SeqCst);
    let plan = privileged.service.plan(&privileged.selection).await?;
    assert!(matches!(
        privileged
            .service
            .apply_reviewed(&privileged.selection, &plan.plan_hash)
            .await,
        Err(MmError::RuntimeResponse { .. })
    ));
    assert_eq!(privileged.rootless.calls.load(Ordering::SeqCst), 1);
    assert_eq!(privileged.privileged.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        privileged
            .runtime
            .dynamic_replace_count
            .load(Ordering::SeqCst),
        2
    );
    Ok(())
}

#[tokio::test]
async fn failed_route_restore_reports_unknown_ingress_state_and_continues_runtime_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(true)?;
    fixture
        .runtime
        .fail_dynamic_restore
        .store(true, Ordering::SeqCst);
    let plan = fixture.service.plan(&fixture.selection).await?;

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await;

    assert!(matches!(
        result,
        Err(MmError::Security { message })
            if message.contains("previous ingress routes could not be restored")
    ));
    let calls = fixture.runtime.calls()?;
    let failed_restore = calls
        .iter()
        .rposition(|call| call == "replace:dynamic.yaml")
        .ok_or("dynamic restore was not attempted")?;
    let workload_rollback = calls
        .iter()
        .position(|call| call == "remove:app")
        .ok_or("workload rollback was not attempted")?;
    assert!(failed_restore < workload_rollback);
    assert!(
        fixture
            .routes
            .route_set(fixture.target_id)?
            .claims
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn managed_network_is_removed_when_initial_deployment_rolls_back()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(true)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "runtime:\n  driver: podman-compose\n",
            "runtime:\n  driver: podman-compose\nnetworks:\n  internal: {}\n",
        )
        .replace(
            "    files: [compose.yaml]\n",
            "    files: [compose.yaml]\n    networks: [internal]\n",
        )
        .replace(
            "  app:\n    path: app\n",
            "  app:\n    path: app\n    networks: [internal]\n",
        );
    fs::write(config_path, config)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let network = plan
        .networks
        .first()
        .ok_or("managed network was not planned")?;
    assert!(!network.members.is_empty());

    assert!(matches!(
        fixture
            .service
            .apply_reviewed(&fixture.selection, &plan.plan_hash)
            .await,
        Err(MmError::RuntimeResponse { .. })
    ));

    let calls = fixture.runtime.calls()?;
    let ensure = format!("network:{}", network.physical_name);
    let remove = format!("remove-network:{}", network.physical_name);
    let ensure_index = calls
        .iter()
        .position(|call| call == &ensure)
        .ok_or("managed network was not ensured")?;
    let remove_index = calls
        .iter()
        .position(|call| call == &remove)
        .ok_or("managed network was not rolled back")?;
    assert!(ensure_index < remove_index);
    Ok(())
}

#[tokio::test]
async fn preserved_development_volumes_and_unused_networks_follow_the_deployment_lifecycle()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "runtime:\n  driver: podman-compose\n",
            "runtime:\n  driver: podman-compose\nnetworks:\n  unused: {}\n",
        )
        .replace(
            "  app:\n    path: app\n",
            "  app:\n    path: app\n    development-mount:\n      source: .\n      target: /app\n      preserve:\n        - path: /app/node_modules\n      services: [web]\n      preset: node\n",
        );
    fs::write(&config_path, &config)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    let preserved = initial
        .preserved_volumes
        .first()
        .ok_or("development mount did not plan a preserved volume")?;
    assert_eq!(initial.preserved_volumes.len(), 1);
    assert!(
        initial
            .networks
            .iter()
            .any(|network| { network.logical_name == "unused" && network.members.is_empty() })
    );

    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&format!("volume:{}", preserved.physical_name)));
    assert!(calls.iter().any(|call| call.starts_with("remove-network:")));

    fixture.runtime.clear()?;
    let config = config.replace(
        "    development-mount:\n      source: .\n      target: /app\n      preserve:\n        - path: /app/node_modules\n      services: [web]\n      preset: node\n",
        "",
    );
    fs::write(config_path, config)?;
    let removal = fixture.service.plan(&fixture.selection).await?;
    assert!(removal.preserved_volumes.is_empty());
    assert_eq!(removal.removed_preserved_volumes.len(), 1);
    assert_eq!(
        removal.removed_preserved_volumes[0].physical_name,
        preserved.physical_name
    );

    fixture
        .service
        .apply_reviewed(&fixture.selection, &removal.plan_hash)
        .await?;
    assert!(
        fixture
            .runtime
            .calls()?
            .contains(&format!("remove-volume:{}", preserved.physical_name))
    );
    Ok(())
}

#[tokio::test]
async fn deployment_plan_journal_failures_never_start_runtime_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let connection = rusqlite::Connection::open(&fixture.state_path)?;
    connection.execute_batch(
        "CREATE TRIGGER fail_deployment_plan BEFORE INSERT ON deployment_snapshots
         BEGIN SELECT RAISE(ABORT, 'injected deployment plan journal failure'); END;",
    )?;
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("plan journal failure unexpectedly succeeded")?;
    assert!(matches!(error, MmError::State { .. }));
    let calls = fixture.runtime.calls()?;
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with("preflight:"));
    assert_eq!(
        fixture
            .state
            .recent_operations(fixture.workspace()?.id, 1)?[0]
            .state,
        OperationState::Failed
    );

    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let connection = rusqlite::Connection::open(&fixture.state_path)?;
    connection.execute_batch(
        "CREATE TRIGGER fail_deployment_plan BEFORE INSERT ON deployment_snapshots
         BEGIN SELECT RAISE(ABORT, 'injected deployment plan journal failure'); END;
         CREATE TRIGGER fail_deployment_finish BEFORE UPDATE ON operations
         BEGIN SELECT RAISE(ABORT, 'injected deployment finish journal failure'); END;",
    )?;
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("combined journal failure unexpectedly succeeded")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::NotStarted,
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("injected deployment plan journal failure")
    );
    assert!(
        error
            .to_string()
            .contains("injected deployment finish journal failure")
    );
    let calls = fixture.runtime.calls()?;
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with("preflight:"));
    Ok(())
}

#[tokio::test]
async fn deployment_finish_journal_failures_preserve_applied_or_unknown_status()
-> Result<(), Box<dyn std::error::Error>> {
    let applied = Fixture::new(false)?;
    let plan = applied.service.plan(&applied.selection).await?;
    let connection = rusqlite::Connection::open(&applied.state_path)?;
    connection.execute_batch(
        "CREATE TRIGGER fail_deployment_success BEFORE UPDATE ON operations
         WHEN NEW.state = 'succeeded'
         BEGIN SELECT RAISE(ABORT, 'injected deployment success journal failure'); END;",
    )?;
    let error = applied
        .service
        .apply_reviewed(&applied.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("success journal failure unexpectedly succeeded")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Applied,
            ..
        }
    ));
    assert_eq!(applied.routes.route_set(applied.target_id)?.claims.len(), 1);

    let unknown = Fixture::new(false)?;
    let plan = unknown.service.plan(&unknown.selection).await?;
    unknown
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    let connection = rusqlite::Connection::open(&unknown.state_path)?;
    connection.execute_batch(
        "CREATE TRIGGER fail_deployment_failure BEFORE UPDATE ON operations
         WHEN NEW.state = 'failed'
         BEGIN SELECT RAISE(ABORT, 'injected deployment failure journal failure'); END;",
    )?;
    let error = unknown
        .service
        .apply_reviewed(&unknown.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("failure journal failure unexpectedly succeeded")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Unknown,
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("injected deployment failure journal failure")
    );
    assert!(unknown.runtime.calls()?.contains(&"remove:app".to_owned()));
    Ok(())
}

#[tokio::test]
async fn rollback_phase_journal_failure_does_not_skip_runtime_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    let connection = rusqlite::Connection::open(&fixture.state_path)?;
    connection.execute_batch(
        "CREATE TRIGGER fail_rollback_phase BEFORE INSERT ON operation_events
         WHEN NEW.payload_json LIKE '%\"phase\":\"rollback\"%'
         BEGIN SELECT RAISE(ABORT, 'injected rollback phase journal failure'); END;",
    )?;

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("combined convergence and rollback journal failure unexpectedly succeeded")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Unknown,
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("injected rollback phase journal failure")
    );
    assert!(fixture.runtime.calls()?.contains(&"remove:app".to_owned()));
    Ok(())
}

#[tokio::test]
async fn environment_publication_and_cleanup_failures_preserve_mutation_status()
-> Result<(), Box<dyn std::error::Error>> {
    let unpublished = Fixture::new(false)?;
    let plan = unpublished.service.plan(&unpublished.selection).await?;
    unpublished
        .environment_artifacts
        .fail_publish
        .store(true, Ordering::SeqCst);
    let error = unpublished
        .service
        .apply_reviewed(&unpublished.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("environment publication failure unexpectedly succeeded")?;
    assert!(matches!(error, MmError::Environment { .. }));
    let calls = unpublished.runtime.calls()?;
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with("preflight:"));

    let applied = Fixture::new(false)?;
    let plan = applied.service.plan(&applied.selection).await?;
    applied
        .environment_artifacts
        .fail_dispose
        .store(true, Ordering::SeqCst);
    let error = applied
        .service
        .apply_reviewed(&applied.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("environment cleanup failure unexpectedly succeeded")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Applied,
            ..
        }
    ));
    assert_eq!(applied.routes.route_set(applied.target_id)?.claims.len(), 1);
    assert_eq!(
        applied
            .environment_artifacts
            .dispose_calls
            .load(Ordering::SeqCst),
        1
    );

    let unknown = Fixture::new(false)?;
    let plan = unknown.service.plan(&unknown.selection).await?;
    unknown
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    unknown
        .environment_artifacts
        .fail_dispose
        .store(true, Ordering::SeqCst);
    let error = unknown
        .service
        .apply_reviewed(&unknown.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("combined apply and cleanup failure unexpectedly succeeded")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Unknown,
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("injected environment disposal failure")
    );
    assert!(unknown.runtime.calls()?.contains(&"remove:app".to_owned()));
    Ok(())
}

#[tokio::test]
async fn docker_compose_runtime_secrets_are_staged_applied_reconciled_and_disposed()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;

    let plan = fixture.service.plan(&fixture.selection).await?;
    let runtime_plan = plan
        .runtime_secret_plan
        .as_ref()
        .ok_or("runtime secret plan was not generated")?;
    assert_eq!(runtime_plan.items.len(), 1);
    assert_eq!(runtime_plan.items[0].logical_name, "database.password");

    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    assert!(fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    assert_eq!(
        fixture
            .runtime_secrets
            .prepared_materials
            .lock()
            .map_err(|_| "runtime secret material recording lock was poisoned")?
            .as_slice(),
        [vec!["database.password".to_owned()]]
    );
    assert_environment_directory_empty(&fixture.environment_directory)?;
    assert_environment_directory_empty(&fixture.runtime_secret_directory)?;
    Ok(())
}

/// The user these tests run as, whom a workstation's secret files belong to.
fn this_user() -> mm::domain::runtime_secret::HostIds {
    mm::domain::runtime_secret::HostIds::unprivileged()
        .unwrap_or(mm::domain::runtime_secret::HostIds { uid: 0, gid: 0 })
}

#[cfg(unix)]
#[tokio::test]
async fn a_local_plan_names_the_secret_files_that_belong_to_you_instead_of_their_ids()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "            secret: database.password\n",
        "            secret: database.password\n            uid: \"999\"\n            gid: \"999\"\n            mode: 0o400\n",
    );
    fs::write(&config_path, config)?;

    let plan = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(
        plan.secret_file_owners,
        [mm::domain::runtime_secret::SecretFileOwnerNote {
            service: "web".to_owned(),
            file: "database-password".to_owned(),
            uid: "999".to_owned(),
            gid: "999".to_owned(),
            mode: 0o400,
            kept_for: this_user(),
        }]
    );
    assert_eq!(
        serde_json::to_value(&plan)?["secret_file_owners"][0]["service"],
        "web"
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    // The file the service mounts has the entry's mode and is the user's.
    let root = fixture
        .runtime_secret_directory
        .with_file_name("runtime-secrets-files");
    let mut pending = vec![root];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_name() != "name.key" {
                files.push(fs::metadata(entry.path())?);
            }
        }
    }
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].permissions().mode() & 0o777, 0o400);
    assert_eq!(
        (files[0].uid(), files[0].gid()),
        (this_user().uid, this_user().gid)
    );
    Ok(())
}

#[tokio::test]
async fn runtime_secret_failures_clean_artifacts_and_preserve_mutation_truth()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime_secrets
        .fail_prepare
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("runtime secret preparation failure must reject deployment")?;
    assert!(error.to_string().contains("runtime secret prepare failed"));
    assert!(!fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    assert_environment_directory_empty(&fixture.environment_directory)?;
    assert_environment_directory_empty(&fixture.runtime_secret_directory)?;

    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime_secrets
        .fail_prepare
        .store(true, Ordering::SeqCst);
    fixture
        .environment_artifacts
        .fail_dispose
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("combined runtime preparation and environment cleanup failure succeeded")?;
    assert!(
        error
            .to_string()
            .contains("injected environment disposal failure")
    );
    assert!(!fixture.runtime.calls()?.contains(&"apply:app".to_owned()));

    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime_secrets
        .fail_reconcile_at
        .store(2, Ordering::SeqCst);
    // The second reconcile removes superseded revisions after the operation
    // is journalled as succeeded: its failure is not the deployment's.
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    assert_eq!(
        fixture
            .runtime_secrets
            .reconcile_calls
            .load(Ordering::SeqCst),
        2
    );
    assert!(fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    assert!(!fixture.runtime.calls()?.contains(&"remove:app".to_owned()));
    assert_environment_directory_empty(&fixture.environment_directory)?;
    assert_environment_directory_empty(&fixture.runtime_secret_directory)?;

    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime_secrets
        .fail_dispose
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("runtime secret disposal failure must not report clean success")?;
    assert!(error.to_string().contains("runtime secret dispose failed"));
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Applied,
            ..
        }
    ));
    assert_environment_directory_empty(&fixture.environment_directory)?;
    assert!(
        fs::read_dir(&fixture.runtime_secret_directory)?
            .next()
            .is_some()
    );
    Ok(())
}

/// The files the Docker Compose driver keeps for the fixture's secrets,
/// one per revision, with what each holds.
fn kept_secret_revisions(fixture: &Fixture) -> Result<Vec<String>, std::io::Error> {
    let root = fixture
        .runtime_secret_directory
        .with_file_name("runtime-secrets-files");
    let mut contents = Vec::new();
    let mut pending = vec![root];
    while let Some(directory) = pending.pop() {
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_name() != "name.key" {
                contents.push(fs::read_to_string(entry.path())?);
            }
        }
    }
    contents.sort();
    Ok(contents)
}

#[tokio::test]
async fn a_file_secret_plan_names_the_first_host_id_of_each_consumer_with_ids_of_its_own()
-> Result<(), Box<dyn std::error::Error>> {
    // A host's record with mm's range, as on rootful Podman.
    let host = temp_dirs::tempdir()?;
    let lines = "mm-services:3221225472:1073676288\n";
    fs::write(host.path().join("subuid"), lines)?;
    fs::write(host.path().join("subgid"), lines)?;
    let user_ids: Arc<dyn mm::app::ports::ServiceUserIds> =
        Arc::new(mm::adapters::user_namespace::FilesystemServiceUserIds::at(
            host.path().join("service-user-ids.json"),
            host.path().join("subuid"),
            host.path().join("subgid"),
        ));
    let fixture = Fixture::with_user_ids(user_ids)?;
    fixture.configure_runtime_secret().await?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let secrets = plan
        .runtime_secret_plan
        .as_ref()
        .ok_or("the plan has no runtime secrets")?;
    let consumer = secrets
        .items
        .first()
        .and_then(|item| item.consumers.first())
        .ok_or("the secret has no consumer")?;
    let first = secrets
        .first_host_id(consumer)
        .ok_or("the plan does not say which host id web's root is")?;
    assert!(first >= 3_221_225_472 && (first - 3_221_225_472) % 65_536 == 0);
    // The record says the same, and the plan's hash covers it.
    let record = fs::read_to_string(host.path().join("service-user-ids.json"))?;
    assert!(record.contains(&format!("\"first\": {first}")), "{record}");
    assert_eq!(secrets.canonical_hash()?, secrets.plan_hash);
    Ok(())
}

#[tokio::test]
async fn a_rotated_file_secret_keeps_its_old_revision_until_the_deployment_succeeded()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    let old_revision = initial
        .runtime_secret_plan
        .as_ref()
        .and_then(|plan| plan.items.first())
        .ok_or("initial runtime secret is missing")?
        .source_revision
        .clone();
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    assert_eq!(
        kept_secret_revisions(&fixture)?,
        ["runtime-secret-material"]
    );

    let reference = SecretReference::new(
        StorageProviderId::new("age".to_owned())?,
        "local/db-password",
    )?;
    let new_revision = fixture
        .age
        .replace(
            &SecretScope::new("local"),
            &reference,
            SecretMaterial::new("rotated-runtime-secret".to_owned())?,
            SecretExpectedState {
                active_revision: Some(old_revision.clone()),
                candidate_revision: None,
            },
            1_100,
        )
        .await?;
    let rotation = fixture.service.plan(&fixture.selection).await?;
    let item = rotation
        .runtime_secret_plan
        .as_ref()
        .and_then(|plan| plan.items.first())
        .ok_or("rotated runtime secret is missing")?;
    assert_eq!(item.source_revision, new_revision);
    assert_eq!(
        item.previous_source_revision.as_deref(),
        Some(old_revision.as_str())
    );
    // No plan carries a value or the name of a file.
    let printed = serde_json::to_string(&rotation)?;
    assert!(!printed.contains("rotated-runtime-secret"));
    assert!(!printed.contains("runtime-secret-material"));
    assert!(!printed.contains("runtime-secrets-files"));

    // The rotated deployment fails: the host returns to what it ran, and
    // the revision that ran is still there for it.
    fixture.runtime.clear()?;
    fixture
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .service
            .apply_reviewed(&fixture.selection, &rotation.plan_hash)
            .await
            .is_err()
    );
    fixture
        .runtime
        .fail_convergence
        .store(false, Ordering::SeqCst);
    assert!(fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    assert!(
        kept_secret_revisions(&fixture)?.contains(&"runtime-secret-material".to_owned()),
        "a failed deployment removed the revision the host runs"
    );
    assert_environment_directory_empty(&fixture.runtime_secret_directory)?;

    // Planned again and applied: both revisions are kept while it runs,
    // and only the new one once it succeeded.
    let again = fixture.service.plan(&fixture.selection).await?;
    fixture.runtime.clear()?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &again.plan_hash)
        .await?;
    assert!(fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    assert_eq!(kept_secret_revisions(&fixture)?, ["rotated-runtime-secret"]);
    let retained = fixture
        .runtime_secrets
        .reconciliations
        .lock()
        .map_err(|_| "runtime secret reconciliation recording lock was poisoned")?
        .clone();
    let (before, after) = match retained.as_slice() {
        [.., before, after] => (before, after),
        _ => return Err("the deployment did not reconcile twice".into()),
    };
    assert_eq!(
        before["database.password"],
        BTreeSet::from([old_revision, new_revision.clone()])
    );
    assert_eq!(after["database.password"], BTreeSet::from([new_revision]));
    assert_environment_directory_empty(&fixture.environment_directory)?;
    assert_environment_directory_empty(&fixture.runtime_secret_directory)?;
    Ok(())
}

#[tokio::test]
async fn runtime_and_environment_cleanup_failures_are_both_reported()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    fixture.configure_runtime_secret().await?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime_secrets
        .fail_dispose
        .store(true, Ordering::SeqCst);
    fixture
        .environment_artifacts
        .fail_dispose
        .store(true, Ordering::SeqCst);

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("combined artifact cleanup failure unexpectedly succeeded")?;

    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Applied,
            ..
        }
    ));
    let message = error.to_string();
    assert!(message.contains("runtime secret dispose failed"));
    assert!(message.contains("injected environment disposal failure"));
    Ok(())
}

#[tokio::test]
async fn rollback_attempts_every_independent_cleanup_and_aggregates_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(true)?;
    fixture.runtime.fail_remove.store(true, Ordering::SeqCst);
    fixture
        .rootless
        .fail_uninstall
        .store(true, Ordering::SeqCst);
    fixture
        .privileged
        .fail_uninstall
        .store(true, Ordering::SeqCst);
    let plan = fixture.service.plan(&fixture.selection).await?;

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("multi-failure rollback unexpectedly succeeded")?;

    let message = error.to_string();
    assert!(message.contains("simulated Compose removal failure"));
    assert!(message.contains("injected rootless ingress uninstall failure"));
    assert!(message.contains("injected privileged ingress uninstall failure"));
    assert_eq!(fixture.rootless.uninstall_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.privileged.uninstall_calls.load(Ordering::SeqCst), 1);
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"remove:app".to_owned()));
    assert!(calls.contains(&"remove:ingress".to_owned()));
    assert!(calls.iter().any(|call| call.starts_with("remove-volume:")));
    Ok(())
}

#[tokio::test]
async fn rollback_to_previous_attempts_every_restore_after_runtime_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fixture.runtime.clear()?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "deployment-test-{environment}-{stack}",
        "replacement-{environment}-{stack}",
    );
    fs::write(config_path, config)?;
    let update = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    fixture
        .runtime
        .fail_rollback_mutations
        .store(true, Ordering::SeqCst);
    fixture.verifier.fail_listener_at.store(4, Ordering::SeqCst);
    fixture.verifier.fail_route.store(true, Ordering::SeqCst);
    fixture.rootless.fail_install.store(true, Ordering::SeqCst);
    fixture
        .privileged
        .fail_install
        .store(true, Ordering::SeqCst);

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &update.plan_hash)
        .await
        .err()
        .ok_or("rollback with injected restore failures unexpectedly succeeded")?;

    let message = error.to_string();
    assert!(message.contains("injected rollback runtime mutation failure"));
    assert!(message.contains("injected listener verification failure"));
    assert!(message.contains("simulated route verification failure"));
    assert!(message.contains("injected rootless ingress install failure"));
    assert!(message.contains("injected privileged ingress install failure"));
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"apply:ingress".to_owned()));
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.as_str() == "apply:app")
            .count(),
        2
    );
    assert!(calls.contains(&"remove:app".to_owned()));
    assert!(calls.contains(&"replace:dynamic.yaml".to_owned()));
    assert_eq!(fixture.rootless.calls.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.privileged.calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn initial_rollback_attempts_all_runtime_cleanup_after_the_first_mutation_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "runtime:\n  driver: podman-compose\n",
            "runtime:\n  driver: podman-compose\nnetworks:\n  internal: {}\n",
        )
        .replace(
            "    files: [compose.yaml]\n",
            "    files: [compose.yaml]\n    networks: [internal]\n",
        )
        .replace(
            "  app:\n    path: app\n",
            "  app:\n    path: app\n    networks: [internal]\n",
        );
    fs::write(config_path, config)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture.runtime.fail_mutation_at.store(1, Ordering::SeqCst);
    fixture
        .runtime
        .fail_rollback_mutations
        .store(true, Ordering::SeqCst);
    fixture
        .rootless
        .fail_uninstall
        .store(true, Ordering::SeqCst);
    fixture
        .privileged
        .fail_uninstall
        .store(true, Ordering::SeqCst);

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("initial rollback with injected cleanup failures unexpectedly succeeded")?;

    let message = error.to_string();
    assert!(message.contains("injected rollback runtime mutation failure"));
    assert!(message.contains("injected rootless ingress uninstall failure"));
    assert!(message.contains("injected privileged ingress uninstall failure"));
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"remove:app".to_owned()));
    assert!(calls.contains(&"remove:ingress".to_owned()));
    assert!(calls.iter().any(|call| call.starts_with("remove-volume:")));
    assert!(calls.iter().any(|call| call.starts_with("remove-network:")));
    Ok(())
}

#[tokio::test]
async fn stale_recovery_distinguishes_rollback_cleanup_and_combined_failures()
-> Result<(), Box<dyn std::error::Error>> {
    for (fail_rollback, fail_cleanup, expected) in [
        (true, false, "could not be recovered"),
        (false, true, "injected environment disposal failure"),
        (
            true,
            true,
            "recovery failed with MM_SECURITY and cleanup failed",
        ),
    ] {
        let fixture = Fixture::new(false)?;
        let initial = fixture.service.plan(&fixture.selection).await?;
        fixture
            .service
            .apply_reviewed(&fixture.selection, &initial.plan_hash)
            .await?;
        fs::write(
            fixture.project.join("compose.yaml"),
            "services:\n  web:\n    image: example/web-v2\n    expose: [8080]\n",
        )?;
        let update = fixture.service.plan(&fixture.selection).await?;
        let workspace = fixture.workspace()?;
        let interrupted = OperationId::new();
        fixture.state.begin_operation(&NewOperation {
            id: interrupted,
            workspace_id: workspace.id,
            kind: OperationKind::Deploy,
            environment: "local".to_owned(),
            requested_at_unix_ms: 900,
            plan_hash: Some(update.plan_hash.clone()),
        })?;
        fixture.state.save_deployment_plan(interrupted, &update)?;
        fixture
            .runtime
            .rollback_mutations_active
            .store(fail_rollback, Ordering::SeqCst);
        if fail_cleanup {
            fixture
                .environment_artifacts
                .fail_dispose_at
                .store(3, Ordering::SeqCst);
        }

        let error = fixture
            .service
            .apply_reviewed(&fixture.selection, &update.plan_hash)
            .await
            .err()
            .ok_or("stale recovery failure injection unexpectedly succeeded")?;
        assert!(error.to_string().contains(expected));
        assert!(fixture.runtime.calls()?.contains(&"apply:app".to_owned()));
    }
    Ok(())
}

#[tokio::test]
async fn lock_and_artifact_reconciliation_failures_have_no_runtime_effect()
-> Result<(), Box<dyn std::error::Error>> {
    let locked = Fixture::new(false)?;
    let plan = locked.service.plan(&locked.selection).await?;
    let _guard = locked.locks.acquire(locked.target_id)?;
    assert!(
        locked
            .service
            .apply_reviewed(&locked.selection, &plan.plan_hash)
            .await
            .is_err()
    );
    assert!(locked.runtime.calls()?.is_empty());

    let reconciliation = Fixture::new(false)?;
    let plan = reconciliation
        .service
        .plan(&reconciliation.selection)
        .await?;
    reconciliation
        .environment_artifacts
        .fail_reconcile
        .store(true, Ordering::SeqCst);
    let error = reconciliation
        .service
        .apply_reviewed(&reconciliation.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("artifact reconciliation failure unexpectedly succeeded")?;
    assert!(
        error
            .to_string()
            .contains("environment reconciliation failure")
    );
    assert!(reconciliation.runtime.calls()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn stale_runtime_snapshot_is_rejected_before_preflight_or_recovery_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let current = fixture.service.plan(&fixture.selection).await?;
    let mut stale = current.clone();
    stale.runtime = Some(RuntimeKind::DockerCompose);
    stale.plan_hash = stale.canonical_hash()?;
    let operation_id = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id: operation_id,
        workspace_id: fixture.workspace()?.id,
        kind: OperationKind::Deploy,
        environment: stale.environment.clone(),
        requested_at_unix_ms: 500,
        plan_hash: Some(stale.plan_hash.clone()),
    })?;
    fixture.state.save_deployment_plan(operation_id, &stale)?;

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &current.plan_hash)
        .await
        .err()
        .ok_or("cross-runtime stale recovery unexpectedly succeeded")?;
    assert!(matches!(error, MmError::Security { message } if message.contains("another runtime")));
    assert!(fixture.runtime.calls()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn prior_snapshot_permutations_and_cross_owner_route_conflicts_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let corrupt = Fixture::new(false)?;
    let initial = corrupt.service.plan(&corrupt.selection).await?;
    corrupt
        .service
        .apply_reviewed(&corrupt.selection, &initial.plan_hash)
        .await?;
    rusqlite::Connection::open(&corrupt.state_path)?.execute(
        "UPDATE deployment_snapshots SET desired_json = json_set(desired_json, '$.plan_hash', 'corrupt')",
        [],
    )?;
    assert!(matches!(
        corrupt.service.plan(&corrupt.selection).await,
        Err(MmError::Security { message }) if message.contains("canonical verification")
    ));

    let conflict = Fixture::new(false)?;
    let desired = conflict.service.plan(&conflict.selection).await?;
    let mut foreign = desired.desired_route_claims[0].clone();
    let foreign_project = ProjectId(Uuid::from_u128(77));
    foreign.project_id = foreign_project;
    foreign.project_name = "foreign".to_owned();
    foreign.stack = "foreign".to_owned();
    foreign.route = "foreign".to_owned();
    foreign.router_name = "foreign-router".to_owned();
    foreign.backend_alias = "foreign-backend".to_owned();
    foreign.ownership = mm::domain::ingress::IngressRouteOwnership::Project {
        project_id: foreign_project,
        environment: foreign.environment.clone(),
        stack: foreign.stack.clone(),
        service: foreign.service.clone(),
    };
    let foreign_environment = foreign.environment.clone();
    conflict.routes.replace_owner_claims(
        conflict.target_id,
        foreign_project,
        &foreign_environment,
        &[foreign],
        &BTreeMap::new(),
        900,
    )?;
    assert!(matches!(
        conflict.service.plan(&conflict.selection).await,
        Err(MmError::Environment { message }) if message.contains("conflicts with route")
    ));
    Ok(())
}

#[tokio::test]
async fn converged_reapply_preserves_route_generation_and_has_no_removal_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fixture.runtime.clear()?;
    let settled = fixture.service.plan(&fixture.selection).await?;
    assert!(!settled.route_state_changed);
    assert!(settled.route_diff.added.is_empty());
    assert!(settled.route_diff.changed.is_empty());
    assert!(settled.route_diff.removed.is_empty());

    let result = fixture
        .service
        .apply_reviewed(&fixture.selection, &settled.plan_hash)
        .await?;

    assert_eq!(
        result.route_generation,
        initial.expected_route_generation + 1
    );
    assert!(result.removed_stacks.is_empty());
    assert_eq!(fixture.routes.route_set(fixture.target_id)?.generation, 1);
    assert!(
        !fixture
            .runtime
            .calls()?
            .iter()
            .any(|call| call.starts_with("remove:"))
    );
    Ok(())
}

#[tokio::test]
async fn scheduler_storage_failure_reports_applied_and_a_retry_reconciles_without_route_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let base = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        format!(
            "{base}jobs:\n  cleanup:\n    module: app\n    service: web\n    command: ['true']\n    timeout: 1m\nschedules:\n  cleanup-hourly:\n    job: cleanup\n    cron: '0 * * * *'\n    timezone: UTC\n"
        ),
    )?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let connection = rusqlite::Connection::open(&fixture.state_path)?;
    connection.execute_batch(
        "CREATE TRIGGER fail_scheduler_reconcile BEFORE INSERT ON scheduled_jobs
         BEGIN SELECT RAISE(ABORT, 'injected scheduler storage failure'); END;",
    )?;

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("scheduler storage failure unexpectedly succeeded")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Applied,
            ..
        }
    ));
    assert_eq!(fixture.routes.route_set(fixture.target_id)?.generation, 1);
    assert!(
        fixture
            .state
            .running_operations(fixture.workspace()?.id)?
            .is_empty()
    );
    assert!(
        fixture
            .state
            .recent_operations(fixture.workspace()?.id, 1)?[0]
            .state
            == OperationState::Succeeded
    );

    connection.execute_batch("DROP TRIGGER fail_scheduler_reconcile;")?;
    let retry = fixture.service.plan(&fixture.selection).await?;
    assert!(!retry.route_state_changed);
    fixture
        .service
        .apply_reviewed(&fixture.selection, &retry.plan_hash)
        .await?;
    assert_eq!(fixture.routes.route_set(fixture.target_id)?.generation, 1);
    assert!(
        fixture
            .state
            .list_jobs(ProjectId(Uuid::parse_str(PROJECT_ID)?), "local")?
            .jobs
            .contains_key("cleanup")
    );
    Ok(())
}

#[tokio::test]
async fn planning_rejects_malformed_target_route_and_runtime_adapter_responses()
-> Result<(), Box<dyn std::error::Error>> {
    let wrong_target = Fixture::new(false)?;
    wrong_target.routes.wrong_target_at.store(
        wrong_target.routes.route_set_calls.load(Ordering::SeqCst) + 1,
        Ordering::SeqCst,
    );
    assert!(matches!(
        wrong_target.service.plan(&wrong_target.selection).await,
        Err(MmError::State { message }) if message.contains("route registry returned target")
    ));

    let stale_fingerprint = Fixture::new(false)?;
    stale_fingerprint.targets.stale_fingerprint_at.store(
        stale_fingerprint
            .targets
            .resolve_calls
            .load(Ordering::SeqCst)
            + 1,
        Ordering::SeqCst,
    );
    assert!(matches!(
        stale_fingerprint
            .service
            .plan(&stale_fingerprint.selection)
            .await,
        Err(MmError::Security { message }) if message.contains("fingerprint")
    ));

    let stale_digest = Fixture::new(false)?;
    stale_digest.routes.stale_digest_at.store(
        stale_digest.routes.route_set_calls.load(Ordering::SeqCst) + 1,
        Ordering::SeqCst,
    );
    assert!(matches!(
        stale_digest.service.plan(&stale_digest.selection).await,
        Err(MmError::State { message }) if message.contains("route digest")
    ));

    let malformed = Fixture::new(false)?;
    let initial = malformed.service.plan(&malformed.selection).await?;
    malformed
        .service
        .apply_reviewed(&malformed.selection, &initial.plan_hash)
        .await?;
    malformed.routes.malformed_claim_at.store(
        malformed.routes.route_set_calls.load(Ordering::SeqCst) + 1,
        Ordering::SeqCst,
    );
    assert!(matches!(
        malformed.service.plan(&malformed.selection).await,
        Err(MmError::State { message }) if message.contains("currently claimed target routes")
    ));

    let runtime = Fixture::new(false)?;
    let config_path = runtime.project.join(".config/mm.yaml");
    fs::write(
        &config_path,
        fs::read_to_string(&config_path)?
            .replace("  local:\n", "  local:\n    environment-delivery: direct\n"),
    )?;
    runtime.runtime.report_docker.store(true, Ordering::SeqCst);
    assert!(matches!(
        runtime.service.plan(&runtime.selection).await,
        Err(MmError::Security { message }) if message.contains("runtime adapter")
    ));

    let remote = Fixture::new(false)?;
    let target = remote.targets.resolve_target("local")?;
    remote.targets.update_target_transport(
        "local",
        &target.fingerprint,
        TargetTransport::Ssh {
            host: "example.test".to_owned(),
            port: 22,
            user: "deploy".to_owned(),
            host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU".to_owned(),
            host_key: None,
            identity_file: None,
            remote_root: None,
            daemon_fingerprint: None,
        },
    )?;
    assert!(matches!(
        remote.service.plan(&remote.selection).await,
        Err(MmError::Target { message }) if message.contains("remote deployment planning")
    ));
    Ok(())
}

#[tokio::test]
async fn reviewed_apply_rejects_document_target_runtime_scheduler_and_route_races()
-> Result<(), Box<dyn std::error::Error>> {
    let unresolved_environment = Fixture::new(false)?;
    unresolved_environment
        .documents
        .unresolved_environment_at
        .store(1, Ordering::SeqCst);
    assert!(
        unresolved_environment
            .service
            .plan(&unresolved_environment.selection)
            .await
            .is_err()
    );

    let wrong_scheduler_owner = Fixture::new(false)?;
    wrong_scheduler_owner
        .documents
        .wrong_project_at
        .store(1, Ordering::SeqCst);
    assert!(matches!(
        wrong_scheduler_owner
            .service
            .plan(&wrong_scheduler_owner.selection)
            .await,
        Err(MmError::Security { message }) if message.contains("scheduler reconciliation")
    ));

    let changed_environment = Fixture::new(false)?;
    let plan = changed_environment
        .service
        .plan(&changed_environment.selection)
        .await?;
    let document_call = changed_environment
        .documents
        .load_calls
        .load(Ordering::SeqCst);
    changed_environment
        .documents
        .change_environment_at
        .store(document_call + 3, Ordering::SeqCst);
    assert!(matches!(
        changed_environment
            .service
            .apply_reviewed(&changed_environment.selection, &plan.plan_hash)
            .await,
        Err(MmError::State { message }) if message.contains("environment metadata changed")
    ));

    let changed_target = Fixture::new(false)?;
    let plan = changed_target
        .service
        .plan(&changed_target.selection)
        .await?;
    changed_target.targets.stale_fingerprint_at.store(
        changed_target.targets.resolve_calls.load(Ordering::SeqCst) + 3,
        Ordering::SeqCst,
    );
    assert!(matches!(
        changed_target
            .service
            .apply_reviewed(&changed_target.selection, &plan.plan_hash)
            .await,
        Err(MmError::Security { message }) if message.contains("resolved target changed")
    ));

    let wrong_runtime = Fixture::new(false)?;
    let plan = wrong_runtime.service.plan(&wrong_runtime.selection).await?;
    wrong_runtime
        .runtime
        .report_docker
        .store(true, Ordering::SeqCst);
    assert!(matches!(
        wrong_runtime
            .service
            .apply_reviewed(&wrong_runtime.selection, &plan.plan_hash)
            .await,
        Err(MmError::Security { message }) if message.contains("runtime adapter")
    ));

    let changed_scheduler = Fixture::new(false)?;
    let plan = changed_scheduler
        .service
        .plan(&changed_scheduler.selection)
        .await?;
    changed_scheduler.documents.changed_source_at.store(
        changed_scheduler
            .documents
            .load_calls
            .load(Ordering::SeqCst)
            + 5,
        Ordering::SeqCst,
    );
    let error = changed_scheduler
        .service
        .apply_reviewed(&changed_scheduler.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("changed scheduler definitions unexpectedly applied")?;
    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: mm::MutationStatus::Applied,
            ..
        }
    ));
    assert!(error.to_string().contains("scheduler definitions changed"));

    let changed_workspace = Fixture::new(false)?;
    let plan = changed_workspace
        .service
        .plan(&changed_workspace.selection)
        .await?;
    changed_workspace.documents.wrong_project_at.store(
        changed_workspace
            .documents
            .load_calls
            .load(Ordering::SeqCst)
            + 4,
        Ordering::SeqCst,
    );
    assert!(matches!(
        changed_workspace
            .service
            .apply_reviewed(&changed_workspace.selection, &plan.plan_hash)
            .await,
        Err(MmError::Security { message }) if message.contains("workspace identity changed")
    ));

    let changed_routes = Fixture::new(false)?;
    let initial = changed_routes
        .service
        .plan(&changed_routes.selection)
        .await?;
    changed_routes
        .service
        .apply_reviewed(&changed_routes.selection, &initial.plan_hash)
        .await?;
    let settled = changed_routes
        .service
        .plan(&changed_routes.selection)
        .await?;
    changed_routes.routes.changed_generation_at.store(
        changed_routes.routes.route_set_calls.load(Ordering::SeqCst) + 3,
        Ordering::SeqCst,
    );
    assert!(matches!(
        changed_routes
            .service
            .apply_reviewed(&changed_routes.selection, &settled.plan_hash)
            .await,
        Err(MmError::State { message }) if message.contains("ingress routes changed")
    ));
    Ok(())
}

#[tokio::test]
async fn rollback_handles_new_ingress_and_preserves_foreign_initial_ingress()
-> Result<(), Box<dyn std::error::Error>> {
    let new_ingress = Fixture::new(false)?;
    write_empty_project_config(&new_ingress.project)?;
    let empty = new_ingress.service.plan(&new_ingress.selection).await?;
    new_ingress
        .service
        .apply_reviewed(&new_ingress.selection, &empty.plan_hash)
        .await?;
    write_project(&new_ingress.project)?;
    let update = new_ingress.service.plan(&new_ingress.selection).await?;
    new_ingress
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    assert!(
        new_ingress
            .service
            .apply_reviewed(&new_ingress.selection, &update.plan_hash)
            .await
            .is_err()
    );
    assert_eq!(
        new_ingress.rootless.uninstall_calls.load(Ordering::SeqCst),
        1
    );
    assert_eq!(
        new_ingress
            .privileged
            .uninstall_calls
            .load(Ordering::SeqCst),
        1
    );

    let shared = Fixture::new(false)?;
    let desired = shared.service.plan(&shared.selection).await?;
    let mut foreign = desired.desired_route_claims[0].clone();
    let foreign_project = ProjectId(Uuid::from_u128(77));
    foreign.project_id = foreign_project;
    foreign.project_name = "foreign".to_owned();
    foreign.host = "foreign.example.test".to_owned();
    foreign.router_name = "foreign-router".to_owned();
    foreign.backend_alias = "foreign-backend".to_owned();
    foreign.stack = "foreign".to_owned();
    foreign.route = "foreign".to_owned();
    foreign.ownership = mm::domain::ingress::IngressRouteOwnership::Project {
        project_id: foreign_project,
        environment: foreign.environment.clone(),
        stack: foreign.stack.clone(),
        service: foreign.service.clone(),
    };
    shared.routes.replace_owner_claims(
        shared.target_id,
        foreign_project,
        "local",
        &[foreign],
        &BTreeMap::new(),
        900,
    )?;
    let plan = shared.service.plan(&shared.selection).await?;
    shared
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    assert!(
        shared
            .service
            .apply_reviewed(&shared.selection, &plan.plan_hash)
            .await
            .is_err()
    );
    assert!(
        !shared
            .runtime
            .calls()?
            .iter()
            .any(|call| call == "remove:ingress")
    );
    Ok(())
}

#[tokio::test]
async fn used_networks_and_unpreserved_development_mounts_are_preflighted()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "runtime:\n  driver: podman-compose\n",
            "runtime:\n  driver: podman-compose\nnetworks:\n  internal: {}\n",
        )
        .replace(
            "    files: [compose.yaml]\n",
            "    files: [compose.yaml]\n    networks: [internal]\n",
        )
        .replace(
            "  app:\n    path: app\n",
            "  app:\n    path: app\n    networks: [internal]\n    development-mount:\n      source: .\n      target: /app\n      services: [web]\n      preset: node\n",
        );
    fs::write(config_path, config)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    assert!(plan.preserved_volumes.is_empty());
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    assert!(
        !fixture
            .runtime
            .calls()?
            .iter()
            .any(|call| call.starts_with("remove-network:"))
    );
    Ok(())
}

struct FailingEnvironmentArtifactStore {
    inner: Arc<FilesystemEnvironmentArtifactStore>,
    fail_publish: AtomicBool,
    fail_dispose: AtomicBool,
    fail_dispose_at: AtomicUsize,
    fail_reconcile: AtomicBool,
    dispose_calls: AtomicUsize,
}

struct DeploymentDeliveryCatalog(Vec<DeliveryCapabilityDescriptor>);

struct ControlledDocuments {
    inner: ConfigLoader,
    project: PathBuf,
    load_calls: AtomicUsize,
    fail_at: AtomicUsize,
    wrong_project_at: AtomicUsize,
    unresolved_environment_at: AtomicUsize,
    changed_source_at: AtomicUsize,
    change_environment_at: AtomicUsize,
}

impl ControlledDocuments {
    fn transform(
        &self,
        call: usize,
        mut document: mm::domain::config::ConfigDocument,
    ) -> Result<mm::domain::config::ConfigDocument, MmError> {
        if self.fail_at.load(Ordering::SeqCst) == call {
            return Err(MmError::State {
                message: "injected project document load failure".to_owned(),
            });
        }
        if self.wrong_project_at.load(Ordering::SeqCst) == call {
            document.config.project.id = ProjectId(Uuid::from_u128(99));
        }
        if self.unresolved_environment_at.load(Ordering::SeqCst) == call {
            document.config.default_environment = Some("missing".to_owned());
        }
        if self.changed_source_at.load(Ordering::SeqCst) == call {
            document.source_hash = "changed-after-review".to_owned();
        }
        if self.change_environment_at.load(Ordering::SeqCst) == call {
            fs::write(
                self.project.join("app/.env.example"),
                "APP_MODE=changed-after-review\n",
            )
            .map_err(|source| MmError::Io {
                context: "injecting an environment review change".to_owned(),
                source,
            })?;
        }
        Ok(document)
    }
}

impl ProjectDocumentStore for ControlledDocuments {
    fn discover(&self, start: &Path) -> Result<PathBuf, MmError> {
        ProjectDocumentStore::discover(&self.inner, start)
    }

    fn load_from(&self, start: &Path) -> Result<mm::domain::config::ConfigDocument, MmError> {
        let call = self.load_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let document = ProjectDocumentStore::load_from(&self.inner, start)?;
        self.transform(call, document)
    }

    fn load(&self, path: &Path) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ProjectDocumentStore::load(&self.inner, path)
    }

    fn initialize(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<Box<dyn mm::app::ports::ProjectInitialization>, MmError> {
        ProjectDocumentStore::initialize(&self.inner, root, requested_name)
    }

    fn schema(&self) -> schemars::Schema {
        ProjectDocumentStore::schema(&self.inner)
    }

    fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ProjectDocumentStore::set_default_environment(&self.inner, start, environment)
    }

    fn set_environment_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: mm::domain::environment_delivery::EnvironmentDeliveryMode,
    ) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ProjectDocumentStore::set_environment_delivery(&self.inner, start, environment, mode)
    }

    fn set_development_mount(
        &self,
        start: &Path,
        module: &str,
        mount: Option<&mm::domain::development_mount::DevelopmentMountConfig>,
        expected_source_hash: &str,
    ) -> Result<mm::domain::config::ConfigDocument, MmError> {
        ProjectDocumentStore::set_development_mount(
            &self.inner,
            start,
            module,
            mount,
            expected_source_hash,
        )
    }
}

struct ControlledTargetStore {
    inner: Arc<FilesystemTargetStore>,
    resolve_calls: AtomicUsize,
    stale_fingerprint_at: AtomicUsize,
}

impl TargetStore for ControlledTargetStore {
    fn list_targets(&self) -> Result<Vec<NamedTarget>, MmError> {
        self.inner.list_targets()
    }

    fn resolve_target(&self, name: &str) -> Result<NamedTarget, MmError> {
        let call = self.resolve_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let mut target = self.inner.resolve_target(name)?;
        if self.stale_fingerprint_at.load(Ordering::SeqCst) == call {
            target.fingerprint = "stale-fingerprint".to_owned();
        }
        Ok(target)
    }

    fn insert_target(
        &self,
        name: &str,
        definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        self.inner.insert_target(name, definition)
    }

    fn ensure_target(
        &self,
        name: &str,
        definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        self.inner.ensure_target(name, definition)
    }

    fn remove_target(
        &self,
        name: &str,
        expected_fingerprint: &str,
    ) -> Result<NamedTarget, MmError> {
        self.inner.remove_target(name, expected_fingerprint)
    }

    fn update_target_transport(
        &self,
        name: &str,
        expected_fingerprint: &str,
        transport: TargetTransport,
    ) -> Result<NamedTarget, MmError> {
        self.inner
            .update_target_transport(name, expected_fingerprint, transport)
    }

    fn update_target_ingress(
        &self,
        name: &str,
        expected_fingerprint: &str,
        ingress: TargetIngress,
    ) -> Result<NamedTarget, MmError> {
        self.inner
            .update_target_ingress(name, expected_fingerprint, ingress)
    }
}

struct ControlledRoutes {
    inner: Arc<dyn IngressRouteRegistry>,
    route_set_calls: AtomicUsize,
    fail_at: AtomicUsize,
    wrong_target_at: AtomicUsize,
    stale_digest_at: AtomicUsize,
    malformed_claim_at: AtomicUsize,
    changed_generation_at: AtomicUsize,
}

impl IngressRouteRegistry for ControlledRoutes {
    fn route_set(&self, target_id: TargetId) -> Result<IngressRouteSet, MmError> {
        let call = self.route_set_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at.load(Ordering::SeqCst) == call {
            return Err(MmError::State {
                message: "injected route registry read failure".to_owned(),
            });
        }
        let mut routes = self.inner.route_set(target_id)?;
        if self.wrong_target_at.load(Ordering::SeqCst) == call {
            routes.target_id = TargetId(Uuid::from_u128(98));
        }
        if self.stale_digest_at.load(Ordering::SeqCst) == call {
            routes.config_digest = Some("stale-digest".to_owned());
        }
        if self.malformed_claim_at.load(Ordering::SeqCst) == call
            && let Some(claim) = routes.claims.first_mut()
        {
            claim.router_name = "invalid router name".to_owned();
        }
        if self.changed_generation_at.load(Ordering::SeqCst) == call {
            routes.generation = routes.generation.saturating_add(1);
        }
        Ok(routes)
    }

    fn conflicts(
        &self,
        proposed: &[IngressRouteClaim],
    ) -> Result<Vec<IngressRouteConflict>, MmError> {
        self.inner.conflicts(proposed)
    }

    fn replace_owner_claims_cas(
        &self,
        replacement: IngressClaimsReplacement<'_>,
    ) -> Result<IngressRouteSet, MmError> {
        self.inner.replace_owner_claims_cas(replacement)
    }

    fn replace_owner_claims(
        &self,
        target_id: TargetId,
        project_id: ProjectId,
        environment: &str,
        claims: &[IngressRouteClaim],
        middlewares: &BTreeMap<String, CanonicalMiddleware>,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        self.inner.replace_owner_claims(
            target_id,
            project_id,
            environment,
            claims,
            middlewares,
            now_unix_ms,
        )
    }
}

impl SecretDeliveryCapabilityCatalog for DeploymentDeliveryCatalog {
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
                message: format!("deployment delivery driver '{driver}' is unavailable"),
            })
    }
}

struct FailingRuntimeSecretDelivery {
    inner: mm::adapters::secrets::DockerComposeFileSecretDelivery,
    fail_prepare: AtomicBool,
    fail_dispose: AtomicBool,
    fail_reconcile_at: AtomicUsize,
    reconcile_calls: AtomicUsize,
    prepared_materials: Mutex<Vec<Vec<String>>>,
    reconciliations: Mutex<Vec<BTreeMap<String, BTreeSet<String>>>>,
}

#[async_trait]
impl RuntimeSecretDelivery for FailingRuntimeSecretDelivery {
    fn capability(&self) -> &mm::domain::runtime_secret::RuntimeSecretCapability {
        self.inner.capability()
    }

    async fn prepare(
        &self,
        operation_id: OperationId,
        plan: &mm::domain::runtime_secret::RuntimeSecretPlan,
        materials: Vec<mm::domain::runtime_secret::RuntimeSecretMaterial>,
    ) -> Result<mm::domain::runtime_secret::PreparedRuntimeSecrets, MmError> {
        if self.fail_prepare.swap(false, Ordering::SeqCst) {
            return Err(MmError::Secret {
                message: "runtime secret prepare failed".to_owned(),
            });
        }
        self.prepared_materials
            .lock()
            .map_err(|_| MmError::State {
                message: "runtime secret material recording lock was poisoned".to_owned(),
            })?
            .push(
                materials
                    .iter()
                    .map(|material| material.logical_name.clone())
                    .collect(),
            );
        self.inner.prepare(operation_id, plan, materials).await
    }

    fn dispose(
        &self,
        prepared: &mm::domain::runtime_secret::PreparedRuntimeSecrets,
    ) -> Result<(), MmError> {
        if self.fail_dispose.swap(false, Ordering::SeqCst) {
            return Err(MmError::Secret {
                message: "runtime secret dispose failed".to_owned(),
            });
        }
        self.inner.dispose(prepared)
    }

    async fn remove_revision(
        &self,
        plan: &mm::domain::runtime_secret::RuntimeSecretPlan,
        logical_name: &str,
        source_revision: &str,
    ) -> Result<bool, MmError> {
        self.inner
            .remove_revision(plan, logical_name, source_revision)
            .await
    }

    async fn reconcile(
        &self,
        plan: &mm::domain::runtime_secret::RuntimeSecretPlan,
        retained_revisions: &BTreeMap<String, BTreeSet<String>>,
        active_operations: &HashSet<OperationId>,
    ) -> Result<Vec<String>, MmError> {
        let call = self.reconcile_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_reconcile_at.load(Ordering::SeqCst) == call {
            return Err(MmError::Secret {
                message: "runtime secret reconcile failed".to_owned(),
            });
        }
        self.reconciliations
            .lock()
            .map_err(|_| MmError::State {
                message: "runtime secret reconciliation recording lock was poisoned".to_owned(),
            })?
            .push(retained_revisions.clone());
        self.inner
            .reconcile(plan, retained_revisions, active_operations)
            .await
    }
}

impl EnvironmentArtifactStore for FailingEnvironmentArtifactStore {
    fn bundle_root(&self, operation_id: OperationId) -> PathBuf {
        self.inner.bundle_root(operation_id)
    }

    fn publish(
        &self,
        request: EnvironmentArtifactRequest<'_>,
    ) -> Result<PreparedEnvironmentBundle, MmError> {
        if self.fail_publish.swap(false, Ordering::SeqCst) {
            Err(MmError::Environment {
                message: "injected environment publication failure".to_owned(),
            })
        } else {
            self.inner.publish(request)
        }
    }

    fn dispose(&self, bundle: &PreparedEnvironmentBundle) -> Result<(), MmError> {
        let call = self.dispose_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_dispose.swap(false, Ordering::SeqCst)
            || self.fail_dispose_at.load(Ordering::SeqCst) == call
        {
            Err(MmError::Environment {
                message: "injected environment disposal failure".to_owned(),
            })
        } else {
            self.inner.dispose(bundle)
        }
    }

    fn reconcile(&self, active_operations: &HashSet<OperationId>) -> Result<(), MmError> {
        if self.fail_reconcile.swap(false, Ordering::SeqCst) {
            Err(MmError::Environment {
                message: "injected environment reconciliation failure".to_owned(),
            })
        } else {
            self.inner.reconcile(active_operations)
        }
    }
}

struct Fixture {
    installation: Arc<FakeInstallationOwnership>,
    _temporary: TempDir,
    project: PathBuf,
    state_path: PathBuf,
    selection: WorkspaceSelection,
    target_id: TargetId,
    service: DeploymentService,
    state: Arc<SqliteStateStore>,
    routes: Arc<ControlledRoutes>,
    documents: Arc<ControlledDocuments>,
    targets: Arc<ControlledTargetStore>,
    runtime: Arc<FakeRuntime>,
    verifier: Arc<FakeVerifier>,
    rootless: Arc<FakeRootlessInstaller>,
    privileged: Arc<FakePrivilegedInstaller>,
    environment_artifacts: Arc<FailingEnvironmentArtifactStore>,
    runtime_secrets: Arc<FailingRuntimeSecretDelivery>,
    locks: Arc<FilesystemTargetOperationLock>,
    age: Arc<AgeSecretStore>,
    ca: Arc<FilesystemLocalCaStore>,
    environment_directory: PathBuf,
    runtime_secret_directory: PathBuf,
}

/// A clock that is one millisecond later every time it is read.
struct TickingClock(std::sync::atomic::AtomicI64);

impl mm::app::ports::Clock for TickingClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

#[derive(Default)]
struct FakeInstallationOwnership(AtomicBool);

impl mm::app::ports::InstallationIngressOwnership for FakeInstallationOwnership {
    fn owns_ingress(&self, _target_id: TargetId) -> Result<bool, MmError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}

impl Fixture {
    fn new(fail_route_verification: bool) -> Result<Self, Box<dyn std::error::Error>> {
        Self::build(fail_route_verification, None, None)
    }

    /// The engine as `mmd` on a host that runs rootful Podman runs it:
    /// every service gets user ids of its own from `user_ids`.
    fn with_user_ids(
        user_ids: Arc<dyn mm::app::ports::ServiceUserIds>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::build(false, None, Some(user_ids))
    }

    /// The engine as `mmd` on node `node` of the two-node cluster project
    /// runs it: every service reads the configuration as that node sees it.
    fn on_node(node: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Self::build(
            false,
            Some(mm::domain::placement::NodeScope {
                target: "cluster".to_owned(),
                node: node.to_owned(),
                environment: "production".to_owned(),
                resolver_files: None,
                // What the node's own mesh knows of the two nodes.
                mesh_addresses: [("app-1", [10, 90, 0, 1]), ("app-2", [10, 90, 0, 2])]
                    .into_iter()
                    .map(|(name, address)| (name.to_owned(), address.into()))
                    .collect(),
            }),
            None,
        )
    }

    fn build(
        fail_route_verification: bool,
        node: Option<mm::domain::placement::NodeScope>,
        user_ids: Option<Arc<dyn mm::app::ports::ServiceUserIds>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let temporary = temp_dirs::tempdir()?;
        let project = temporary.path().join("project");
        write_project(&project)?;
        let mut selection = WorkspaceSelection::new(&project, None);
        if node.is_some() {
            write_cluster_project(&project, CLUSTER_SERVICES, CLUSTER_COMPOSE)?;
            selection.target = Some("local".to_owned());
            selection.runtime = Some(RuntimeKind::DockerCompose);
            selection.node.clone_from(&node);
        }
        selection.user_ids.clone_from(&user_ids);
        // What `mmd` does for a request that names a node.
        let seen = |documents: Arc<dyn ProjectDocumentStore>| -> Arc<dyn ProjectDocumentStore> {
            match &node {
                Some(scope) => Arc::new(mm::app::NodeDocuments::new(documents, scope.clone())),
                None => documents,
            }
        };
        let home = MmHome::resolve(Some(&temporary.path().join("home")))?;
        let state_path = home.state_database.clone();
        let environment_directory = home.run_directory.join("env");
        let runtime_secret_directory = home.run_directory.join("runtime-secrets");
        let state = Arc::new(SqliteStateStore::open(&home.state_database)?);
        let route_store: Arc<dyn IngressRouteRegistry> = state.clone();
        let routes = Arc::new(ControlledRoutes {
            inner: route_store,
            route_set_calls: AtomicUsize::new(0),
            fail_at: AtomicUsize::new(0),
            wrong_target_at: AtomicUsize::new(0),
            stale_digest_at: AtomicUsize::new(0),
            malformed_claim_at: AtomicUsize::new(0),
            changed_generation_at: AtomicUsize::new(0),
        });
        let route_port: Arc<dyn IngressRouteRegistry> = routes.clone();
        let state_port: Arc<dyn StateStore> = state.clone();
        let target_store = Arc::new(FilesystemTargetStore::new(home.targets_file.clone()));
        let targets = Arc::new(ControlledTargetStore {
            inner: target_store,
            resolve_calls: AtomicUsize::new(0),
            stale_fingerprint_at: AtomicUsize::new(0),
        });
        let target_id = TargetId(Uuid::parse_str(TARGET_ID)?);
        targets.insert_target("local", TargetDefinition::local(target_id))?;
        let target_port: Arc<dyn TargetStore> = targets.clone();
        let stacks = StackManager::new(
            seen(Arc::new(ConfigLoader)),
            Arc::new(FilesystemStackArtifactStore::new(home.compose_directory)),
            Arc::clone(&target_port),
        )
        .with_user_ids(user_ids);
        let ingress_artifacts = Arc::new(FilesystemTargetIngressArtifactStore::new(
            home.ingress_directory.clone(),
        ));
        let age = Arc::new(AgeSecretStore::new(
            home.identities_directory.join("age.key"),
            home.secrets_directory.join(format!("{PROJECT_ID}.age")),
        ));
        let values: Arc<dyn EnvironmentValueStore> = state.clone();
        let registry = support::secret_registry(age.clone())?;
        let secret_inspector: Arc<dyn SecretMetadataInspector> = registry.clone();
        let secret_resolver: Arc<dyn SecretMaterialResolver> = registry.clone();
        let storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog> = registry;
        let environment_artifact_store = Arc::new(FilesystemEnvironmentArtifactStore::new(
            environment_directory.clone(),
        )?);
        let mut delivery_descriptors = environment_artifact_store.delivery_capabilities();
        delivery_descriptors.push(compose_file_secret_descriptor()?);
        let delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog> =
            Arc::new(DeploymentDeliveryCatalog(delivery_descriptors));
        let environment_artifacts = Arc::new(FailingEnvironmentArtifactStore {
            inner: environment_artifact_store,
            fail_publish: AtomicBool::new(false),
            fail_dispose: AtomicBool::new(false),
            fail_dispose_at: AtomicUsize::new(0),
            fail_reconcile: AtomicBool::new(false),
            dispose_calls: AtomicUsize::new(0),
        });
        let environments = mm::app::EnvironmentManager::new(EnvironmentDependencies {
            documents: seen(Arc::new(ConfigLoader)),
            sources: Arc::new(EnvironmentIndexer),
            writer: Arc::new(FilesystemMaterializedEnvironmentWriter),
            state: Arc::clone(&state_port),
            values,
            inspector: secret_inspector,
            resolver: secret_resolver,
            storage_capabilities,
            delivery_capabilities,
            clock: Arc::new(FixedClock { unix_ms: 1_000 }),
        });
        let runtime = Arc::new(FakeRuntime::default());
        runtime.support_direct.store(true, Ordering::SeqCst);
        runtime
            .report_docker
            .store(node.is_some(), Ordering::SeqCst);
        let runtime_port: Arc<dyn LocalDeploymentRuntime> = runtime.clone();
        let inspector: Arc<dyn RuntimeDriver> = runtime.clone();
        let ca = Arc::new(FilesystemLocalCaStore::new(home.ca_directory.clone()));
        ca.install()?;
        let installation = Arc::new(FakeInstallationOwnership::default());
        let planner = DeploymentManager::new(DeploymentManagerDependencies {
            installation: installation.clone(),
            stacks,
            routes: Arc::clone(&route_port),
            state: Arc::clone(&state_port),
            dynamic_configs: Arc::new(FilesystemTraefikDynamicConfigStore::new(
                home.ingress_directory,
            )),
            target_ingress: TargetIngressManager::new(Arc::clone(&target_port), ingress_artifacts),
            environments: environments.clone(),
            runtime: Arc::clone(&inspector),
            local_ca: ca.clone(),
        });
        assert_eq!(format!("{planner:?}"), "DeploymentManager { .. }");
        let verifier = Arc::new(FakeVerifier {
            fail_listener: AtomicBool::new(false),
            fail_listener_at: AtomicUsize::new(0),
            listener_calls: AtomicUsize::new(0),
            fail_route: AtomicBool::new(fail_route_verification),
            fail_route_commit: AtomicBool::new(false),
            route_calls: AtomicUsize::new(0),
            routes: Arc::clone(&route_port),
            target_id,
        });
        let rootless = Arc::new(FakeRootlessInstaller::default());
        let privileged = Arc::new(FakePrivilegedInstaller::default());
        let runtime_secrets = Arc::new(FailingRuntimeSecretDelivery {
            inner: mm::adapters::secrets::DockerComposeFileSecretDelivery::kept_for(
                runtime_secret_directory.clone(),
                Some(this_user()),
            )?,
            fail_prepare: AtomicBool::new(false),
            fail_dispose: AtomicBool::new(false),
            fail_reconcile_at: AtomicUsize::new(0),
            reconcile_calls: AtomicUsize::new(0),
            prepared_materials: Mutex::new(Vec::new()),
            reconciliations: Mutex::new(Vec::new()),
        });
        let locks = Arc::new(FilesystemTargetOperationLock::new(
            home.target_locks_directory,
        ));
        let documents = Arc::new(ControlledDocuments {
            inner: ConfigLoader,
            project: project.clone(),
            load_calls: AtomicUsize::new(0),
            fail_at: AtomicUsize::new(0),
            wrong_project_at: AtomicUsize::new(0),
            unresolved_environment_at: AtomicUsize::new(0),
            changed_source_at: AtomicUsize::new(0),
            change_environment_at: AtomicUsize::new(0),
        });
        // Deployments are told apart by when they finished; a node fixture
        // applies several, so its clock moves.
        let clock: Arc<dyn mm::app::ports::Clock> = if node.is_some() {
            Arc::new(TickingClock(std::sync::atomic::AtomicI64::new(1_000)))
        } else {
            Arc::new(FixedClock { unix_ms: 1_000 })
        };
        let ports = DeploymentExecutionPorts {
            documents: seen(documents.clone()),
            routes: route_port,
            state: state_port,
            targets: target_port,
            runtime: runtime_port,
            inspector,
            locks: locks.clone(),
            verifier: verifier.clone(),
            rootless: rootless.clone(),
            privileged: privileged.clone(),
            clock,
            environments,
            environment_artifacts: environment_artifacts.clone(),
            runtime_secrets: runtime_secrets.clone(),
            scheduler: state.clone(),
        };
        assert_eq!(format!("{ports:?}"), "DeploymentExecutionPorts { .. }");
        let service = DeploymentService::new(planner, ports);
        assert_eq!(format!("{service:?}"), "DeploymentService { .. }");
        Ok(Self {
            _temporary: temporary,
            installation,
            project,
            state_path,
            selection,
            target_id,
            service,
            state,
            routes,
            documents,
            targets,
            runtime,
            verifier,
            rootless,
            privileged,
            environment_artifacts,
            runtime_secrets,
            locks,
            age,
            ca,
            environment_directory,
            runtime_secret_directory,
        })
    }

    fn workspace(
        &self,
    ) -> Result<mm::domain::workspace::WorkspaceRecord, Box<dyn std::error::Error>> {
        Ok(self.state.register_workspace(
            &WorkspaceRegistration {
                project_id: ProjectId(Uuid::parse_str(PROJECT_ID)?),
                root: dunce::canonicalize(&self.project)?,
                config_path: dunce::canonicalize(self.project.join(".config/mm.yaml"))?,
            },
            1_000,
        )?)
    }

    fn insert_running_operation(&self) -> Result<OperationId, Box<dyn std::error::Error>> {
        let workspace = self.workspace()?;
        let id = OperationId::new();
        self.state.begin_operation(&NewOperation {
            id,
            workspace_id: workspace.id,
            kind: OperationKind::Deploy,
            environment: "local".to_owned(),
            requested_at_unix_ms: 500,
            plan_hash: Some("interrupted-plan".to_owned()),
        })?;
        Ok(id)
    }

    async fn configure_runtime_secret(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.report_docker.store(true, Ordering::SeqCst);
        let config_path = self.project.join(".config/mm.yaml");
        let config = fs::read_to_string(&config_path)?
            .replace("driver: podman-compose", "driver: docker-compose")
            .replace(
                "      web:\n        stack: app\n",
                "      web:\n        stack: app\n        secret-files:\n          database-password:\n            secret: database.password\n",
            )
            .replace(
                "environments:\n",
                "secrets:\n  database.password:\n    storage:\n      provider: age\n      location: local/db-password\nenvironments:\n",
            );
        fs::write(config_path, config)?;
        let reference = SecretReference::new(
            StorageProviderId::new("age".to_owned())?,
            "local/db-password",
        )?;
        self.age.initialize()?;
        self.age
            .create(
                &SecretScope::new("local"),
                &reference,
                SecretMaterial::new("runtime-secret-material".to_owned())?,
                900,
            )
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn first_project_failure_does_not_remove_installation_owned_ingress()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(true)?;
    fixture.installation.0.store(true, Ordering::SeqCst);
    let plan = fixture.service.plan(&fixture.selection).await?;
    assert!(
        fixture
            .service
            .apply_reviewed(&fixture.selection, &plan.plan_hash)
            .await
            .is_err()
    );
    assert_eq!(fixture.rootless.uninstall_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.privileged.uninstall_calls.load(Ordering::SeqCst), 0);
    assert!(
        !fixture
            .runtime
            .calls()?
            .iter()
            .any(|call| call == "remove:ingress")
    );
    Ok(())
}

#[derive(Default)]
struct FakeRuntime {
    calls: Mutex<Vec<String>>,
    fail_remove: AtomicBool,
    fail_preflight: AtomicBool,
    fail_convergence: AtomicBool,
    fail_rollback_mutations: AtomicBool,
    rollback_mutations_active: AtomicBool,
    fail_dynamic_restore: AtomicBool,
    fail_mutation_at: AtomicUsize,
    mutation_count: AtomicUsize,
    dynamic_replace_count: AtomicUsize,
    environment_file_observed: AtomicUsize,
    direct_overlay_safe: AtomicBool,
    direct_environment_count: AtomicUsize,
    block_app_apply: AtomicBool,
    app_apply_entered: AtomicBool,
    support_direct: AtomicBool,
    report_non_rootless: AtomicBool,
    report_docker: AtomicBool,
    /// Running containers, as (service, id), whose configuration differs
    /// from the files about to be applied.
    outdated: Mutex<Vec<(String, String)>>,
    /// The services that have a container, by Compose project.
    running: Mutex<BTreeMap<String, BTreeSet<String>>>,
    /// Each apply with counts and each added container, as the engine was
    /// given it.
    scaling: Mutex<Vec<ScaledCall>>,
    /// How many containers each service runs, for `service_container_ids`.
    containers: Mutex<BTreeMap<String, usize>>,
}

/// One call that runs a number of containers of services.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScaledCall {
    /// `apply`, `ahead:<services>`, or `scale:<service>`.
    call: String,
    counts: BTreeMap<String, u32>,
    /// The text of every Compose file the engine was given, in order, with
    /// the operation's part of each value's name taken out.
    files: Vec<String>,
    /// The values Compose interpolates with, named the same way.
    environment: Vec<(String, String)>,
}

/// `MM_ENV_<operation>_N`, the name a value of the environment takes for
/// one operation, without the operation.
fn without_operation(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("MM_ENV_") {
        let (before, from) = rest.split_at(at + "MM_ENV_".len());
        out.push_str(before);
        let operation = from.bytes().take_while(u8::is_ascii_hexdigit).count();
        out.push_str("<operation>");
        rest = &from[operation..];
    }
    out.push_str(rest);
    out
}

impl FakeRuntime {
    fn note_scaled(
        &self,
        call: String,
        target: &RuntimeTarget,
        counts: BTreeMap<String, u32>,
    ) -> Result<(), MmError> {
        let files = target
            .compose_files
            .iter()
            .map(|file| {
                fs::read_to_string(file)
                    .map(|text| without_operation(&text))
                    .map_err(|source| MmError::Io {
                        context: format!("reading {}", file.display()),
                        source,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let environment = target
            .interpolation_environment
            .values()
            .map(|(name, value)| (without_operation(name), value.to_owned()))
            .collect();
        self.scaling
            .lock()
            .map_err(|_| MmError::State {
                message: "fake runtime scaling lock was poisoned".to_owned(),
            })?
            .push(ScaledCall {
                call,
                counts,
                files,
                environment,
            });
        Ok(())
    }

    fn scaled_calls(&self) -> Result<Vec<ScaledCall>, MmError> {
        self.scaling
            .lock()
            .map(|calls| calls.clone())
            .map_err(|_| MmError::State {
                message: "fake runtime scaling lock was poisoned".to_owned(),
            })
    }

    fn running(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, BTreeSet<String>>>, MmError> {
        self.running.lock().map_err(|_| MmError::State {
            message: "fake runtime container lock was poisoned".to_owned(),
        })
    }

    /// The services with a container in the stack's project, sorted.
    fn runs(&self) -> Result<Vec<String>, MmError> {
        Ok(self
            .running()?
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>())
    }

    fn record(&self, call: impl Into<String>) -> Result<(), MmError> {
        self.calls
            .lock()
            .map_err(|_| MmError::State {
                message: "fake runtime call lock was poisoned".to_owned(),
            })?
            .push(call.into());
        Ok(())
    }

    fn calls(&self) -> Result<Vec<String>, MmError> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .map_err(|_| MmError::State {
                message: "fake runtime call lock was poisoned".to_owned(),
            })
    }

    fn clear(&self) -> Result<(), MmError> {
        self.calls
            .lock()
            .map_err(|_| MmError::State {
                message: "fake runtime call lock was poisoned".to_owned(),
            })?
            .clear();
        self.mutation_count.store(0, Ordering::SeqCst);
        Ok(())
    }

    fn mutation(&self) -> Result<(), MmError> {
        if self.rollback_mutations_active.load(Ordering::SeqCst) {
            return Err(MmError::RuntimeResponse {
                message: "injected rollback runtime mutation failure".to_owned(),
            });
        }
        let mutation = self.mutation_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self
            .fail_mutation_at
            .compare_exchange(mutation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            if self.fail_rollback_mutations.load(Ordering::SeqCst) {
                self.rollback_mutations_active.store(true, Ordering::SeqCst);
            }
            Err(MmError::RuntimeResponse {
                message: format!("injected mutation failure at {mutation}"),
            })
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl RuntimeDriver for FakeRuntime {
    fn kind(&self) -> RuntimeKind {
        if self.report_docker.load(Ordering::SeqCst) {
            RuntimeKind::DockerCompose
        } else {
            RuntimeKind::PodmanCompose
        }
    }

    async fn probe(&self) -> RuntimeProbe {
        let mut capabilities = vec![
            RuntimeCapability::ApplyCompose,
            RuntimeCapability::ObserveServices,
            RuntimeCapability::BindMounts,
            RuntimeCapability::ManageNetworks,
            RuntimeCapability::ManageVolumes,
            RuntimeCapability::MutateVolumeFiles,
            RuntimeCapability::RuntimeSecrets,
        ];
        if self.support_direct.load(Ordering::SeqCst) {
            capabilities.push(RuntimeCapability::DirectEnvironment);
        }
        RuntimeProbe {
            kind: self.kind(),
            available: true,
            engine_version: Some("test".to_owned()),
            compose_provider: Some(ComposeProvider {
                name: "test".to_owned(),
                version: Some("test".to_owned()),
                executable: None,
            }),
            rootless: Some(!self.report_non_rootless.load(Ordering::SeqCst)),
            socket_path: None,
            capabilities,
            diagnostics: Vec::new(),
        }
    }

    async fn render(&self, _target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        Err(MmError::RuntimeResponse {
            message: "unexpected render call".to_owned(),
        })
    }

    async fn observe(&self, _target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        Err(MmError::RuntimeResponse {
            message: "unexpected observe call".to_owned(),
        })
    }

    async fn wait_for_convergence(
        &self,
        target: &RuntimeTarget,
        _policy: ConvergencePolicy,
        _cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        self.record(format!("converge:{}", target.stack_name))?;
        let failed = self.fail_convergence.swap(false, Ordering::SeqCst);
        if failed && self.fail_rollback_mutations.load(Ordering::SeqCst) {
            self.rollback_mutations_active.store(true, Ordering::SeqCst);
        }
        Ok(ConvergenceOutcome {
            state: if failed {
                ConvergenceState::Failed
            } else {
                ConvergenceState::Succeeded
            },
            polls: 1,
            observation: ObservedProject {
                stack: target.stack_name.clone(),
                services: Vec::new(),
                containers: Vec::new(),
            },
            service: None,
            diagnostic_code: failed.then(|| "MM_INJECTED_VERIFY_FAILURE".to_owned()),
            message: failed.then(|| "injected convergence failure".to_owned()),
        })
    }

    async fn project_services(&self, project: &str) -> Result<BTreeSet<String>, MmError> {
        Ok(self.running()?.get(project).cloned().unwrap_or_default())
    }
}

#[async_trait]
impl LocalDeploymentRuntime for FakeRuntime {
    async fn preflight(
        &self,
        _target: &NamedTarget,
        resources: &[RuntimeResource],
        projects: &[RuntimeTarget],
    ) -> Result<(), MmError> {
        self.record(format!("preflight:{}:{}", resources.len(), projects.len()))?;
        if self.fail_preflight.swap(false, Ordering::SeqCst) {
            Err(MmError::Security {
                message: "injected preflight ownership failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }
    async fn ensure_network(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.record(format!("network:{name}"))?;
        self.mutation()?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn inspect_network(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<bool, MmError> {
        self.record(format!("inspect-network:{name}"))?;
        Ok(true)
    }

    async fn remove_network(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<(), MmError> {
        self.record(format!("remove-network:{name}"))?;
        self.mutation()
    }

    async fn ensure_volume(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.record(format!("volume:{name}"))?;
        self.mutation()?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn remove_volume(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<(), MmError> {
        self.record(format!("remove-volume:{name}"))?;
        self.mutation()
    }

    async fn create_volume_file_if_absent(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        file_name: &str,
        _mode: u32,
    ) -> Result<(), MmError> {
        self.record(format!("create:{file_name}"))?;
        self.mutation()
    }

    async fn replace_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        file_name: &str,
        _source: &Path,
        _mode: u32,
    ) -> Result<(), MmError> {
        self.record(format!("replace:{file_name}"))?;
        self.mutation()?;
        if file_name == "dynamic.yaml"
            && self.dynamic_replace_count.fetch_add(1, Ordering::SeqCst) == 1
            && self.fail_dynamic_restore.swap(false, Ordering::SeqCst)
        {
            return Err(MmError::RuntimeResponse {
                message: "injected dynamic route restoration failure".to_owned(),
            });
        }
        Ok(())
    }

    async fn outdated_containers(
        &self,
        _target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        let outdated = self.outdated.lock().map_err(|_| MmError::State {
            message: "fake runtime outdated lock was poisoned".to_owned(),
        })?;
        Ok(outdated
            .iter()
            .filter(|(service, _)| services.contains(service))
            .map(|(_, id)| id.clone())
            .collect())
    }

    async fn apply(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.record(format!("apply:{}", target.stack_name))?;
        self.mutation()?;
        if target.stack_name == "app" {
            self.inspect_environment_delivery(target)?;
            if self.block_app_apply.load(Ordering::SeqCst) {
                self.app_apply_entered.store(true, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
        }
        Ok(())
    }

    async fn remove(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.record(format!("remove:{}", target.stack_name))?;
        self.mutation()?;
        if self.fail_remove.swap(false, Ordering::SeqCst) {
            Err(MmError::RuntimeResponse {
                message: "simulated Compose removal failure".to_owned(),
            })
        } else {
            self.running()?.remove(&target.project_name);
            Ok(())
        }
    }

    /// Starts the services the project's files list and keeps the others,
    /// as `compose up` without `--remove-orphans` does.
    async fn apply_keeping_other_services(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.record(format!("up:{}", target.stack_name))?;
        self.mutation()?;
        let mut services = BTreeSet::new();
        for file in &target.compose_files {
            let text = fs::read_to_string(file).map_err(|source| MmError::Io {
                context: format!("reading {}", file.display()),
                source,
            })?;
            let compose: serde_yaml_ng::Value =
                serde_yaml_ng::from_str(&text).map_err(|error| MmError::RuntimeResponse {
                    message: error.to_string(),
                })?;
            if let Some(listed) = compose
                .get("services")
                .and_then(|services| services.as_mapping())
            {
                services.extend(
                    listed
                        .keys()
                        .filter_map(|name| name.as_str())
                        .map(str::to_owned),
                );
            }
        }
        self.running()?
            .entry(target.project_name.clone())
            .or_default()
            .extend(services);
        Ok(())
    }

    async fn apply_scaled(
        &self,
        target: &RuntimeTarget,
        counts: &BTreeMap<String, u32>,
        keep_others: bool,
    ) -> Result<(), MmError> {
        if !counts.is_empty() {
            self.note_scaled("apply".to_owned(), target, counts.clone())?;
        }
        if keep_others {
            self.apply_keeping_other_services(target).await
        } else {
            self.apply(target).await
        }
    }

    async fn apply_services(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        counts: &BTreeMap<String, u32>,
    ) -> Result<(), MmError> {
        self.record(format!(
            "ahead:{}:{}",
            target.stack_name,
            services.join(",")
        ))?;
        self.mutation()?;
        self.note_scaled(
            format!("ahead:{}", services.join(",")),
            target,
            counts.clone(),
        )
    }

    async fn scale_service(
        &self,
        target: &RuntimeTarget,
        service: &str,
        count: u32,
    ) -> Result<(), MmError> {
        self.note_scaled(
            format!("scale:{service}"),
            target,
            BTreeMap::from([(service.to_owned(), count)]),
        )
    }

    async fn service_container_ids(
        &self,
        _target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        let containers = self.containers.lock().map_err(|_| MmError::State {
            message: "fake runtime container lock was poisoned".to_owned(),
        })?;
        Ok(services
            .iter()
            .flat_map(|service| {
                (0..containers.get(service).copied().unwrap_or(0))
                    .map(move |index| format!("{service}-{index}"))
            })
            .collect())
    }

    async fn remove_services(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        whole_project: bool,
    ) -> Result<(), MmError> {
        self.record(format!(
            "stop:{}:{}{}",
            target.stack_name,
            services.join(","),
            if whole_project { ":project" } else { "" }
        ))?;
        self.mutation()?;
        let mut running = self.running()?;
        if let Some(project) = running.get_mut(&target.project_name) {
            project.retain(|service| !services.contains(service));
            if project.is_empty() {
                running.remove(&target.project_name);
            }
        }
        Ok(())
    }
}

impl FakeRuntime {
    fn inspect_environment_delivery(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.direct_environment_count
            .store(target.interpolation_environment.len(), Ordering::SeqCst);
        for compose_file in target
            .compose_files
            .iter()
            .skip(target.user_compose_file_count)
        {
            let source = fs::read_to_string(compose_file).map_err(|source| MmError::Io {
                context: "reading fake runtime environment overlay".to_owned(),
                source,
            })?;
            if source.contains("env_file:") {
                let value: serde_yaml_ng::Value =
                    serde_yaml_ng::from_str(&source).map_err(|error| MmError::Environment {
                        message: format!(
                            "fake runtime could not parse environment overlay: {error}"
                        ),
                    })?;
                let path = value
                    .get("services")
                    .and_then(serde_yaml_ng::Value::as_mapping)
                    .and_then(|services| {
                        services.values().find_map(|service| {
                            service
                                .get("env_file")
                                .and_then(serde_yaml_ng::Value::as_sequence)
                                .and_then(|paths| paths.first())
                                .and_then(serde_yaml_ng::Value::as_str)
                        })
                    })
                    .ok_or_else(|| MmError::Environment {
                        message: "fake runtime environment overlay omitted env_file".to_owned(),
                    })?;
                let metadata = fs::metadata(path).map_err(|source| MmError::Io {
                    context: "inspecting fake runtime environment file".to_owned(),
                    source,
                })?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if metadata.permissions().mode() & 0o777 != 0o600 {
                        return Err(MmError::Security {
                            message: "fake runtime observed a non-private environment file"
                                .to_owned(),
                        });
                    }
                }
                self.environment_file_observed
                    .fetch_add(1, Ordering::SeqCst);
            }
            if !target.interpolation_environment.is_empty() && source.contains("MM_ENV_") {
                let root = compose_file.parent().ok_or_else(|| MmError::Environment {
                    message: "direct environment overlay has no operation root".to_owned(),
                })?;
                let safe = regular_files(root)?.into_iter().all(|path| {
                    !path.to_string_lossy().ends_with(".env")
                        && fs::read(&path).is_ok_and(|bytes| {
                            !bytes
                                .windows("runtime-public-marker".len())
                                .any(|window| window == b"runtime-public-marker")
                        })
                });
                self.direct_overlay_safe.store(safe, Ordering::SeqCst);
            }
        }
        Ok(())
    }
}

struct FakeVerifier {
    fail_listener: AtomicBool,
    fail_listener_at: AtomicUsize,
    listener_calls: AtomicUsize,
    fail_route: AtomicBool,
    fail_route_commit: AtomicBool,
    route_calls: AtomicUsize,
    routes: Arc<dyn IngressRouteRegistry>,
    target_id: TargetId,
}

#[async_trait]
impl IngressVerifier for FakeVerifier {
    async fn verify_listener(&self, _port: u16) -> Result<(), MmError> {
        let call = self.listener_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_listener.swap(false, Ordering::SeqCst)
            || self.fail_listener_at.load(Ordering::SeqCst) == call
        {
            Err(MmError::RuntimeResponse {
                message: "injected listener verification failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }

    async fn verify_route(&self, _port: u16, _host: &str, _path: &str) -> Result<(), MmError> {
        self.route_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_route.swap(false, Ordering::SeqCst) {
            Err(MmError::RuntimeResponse {
                message: "simulated route verification failure".to_owned(),
            })
        } else if self.fail_route_commit.swap(false, Ordering::SeqCst) {
            self.routes.replace_owner_claims(
                self.target_id,
                ProjectId(Uuid::from_u128(0x119c_527b_6242_7b43_bfd4_bd60_e27c_1de3)),
                "local",
                &[],
                &BTreeMap::new(),
                1_001,
            )
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct FakeRootlessInstaller {
    calls: AtomicUsize,
    uninstall_calls: AtomicUsize,
    fail_install: AtomicBool,
    fail_uninstall: AtomicBool,
}

#[async_trait]
impl RootlessIngressInstaller for FakeRootlessInstaller {
    async fn install(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_install.swap(false, Ordering::SeqCst) {
            Err(MmError::RuntimeResponse {
                message: "injected rootless ingress install failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }

    async fn uninstall(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        self.uninstall_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_uninstall.swap(false, Ordering::SeqCst) {
            Err(MmError::RuntimeResponse {
                message: "injected rootless ingress uninstall failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct FakePrivilegedInstaller {
    calls: AtomicUsize,
    uninstall_calls: AtomicUsize,
    fail_install: AtomicBool,
    fail_uninstall: AtomicBool,
}

#[async_trait]
impl PrivilegedIngressInstaller for FakePrivilegedInstaller {
    async fn install(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_install.swap(false, Ordering::SeqCst) {
            Err(MmError::RuntimeResponse {
                message: "injected privileged ingress install failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }

    async fn uninstall(&self, _plan: &TargetIngressPlan) -> Result<(), MmError> {
        self.uninstall_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_uninstall.swap(false, Ordering::SeqCst) {
            Err(MmError::RuntimeResponse {
                message: "injected privileged ingress uninstall failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

fn write_project(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(
        root.join("app/.env.example"),
        "APP_MODE=runtime-public-marker\n",
    )?;
    fs::write(root.join("app/.dockerignore"), ".env*\n")?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n    expose: [8080]\n",
    )?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: {PROJECT_ID}\n  name: deployment-test\n  development-environment: local\ndefault-environment: local\ntarget: local\nruntime:\n  driver: podman-compose\nstacks:\n  app:\n    project-name: deployment-test-{{environment}}-{{stack}}\n    files: [compose.yaml]\n    ingress:\n      domain: app\nmodules:\n  app:\n    path: app\n    env:\n      template: .env.example\n    services:\n      web:\n        stack: app\nenvironments:\n  local:\n    domains:\n      app: app.example.test\n    bindings: {{}}\n"
        ),
    )?;
    Ok(())
}

const CLUSTER_COMPOSE: &str = "services:
  web: {image: example/web, depends_on: [worker]}
  worker: {image: example/worker}
  legacy: {image: example/legacy}
";
const CLUSTER_SERVICES: &str = "      web: {stack: app}
      worker: {stack: app}
      legacy: {stack: app}";

/// The fixture's project on a cluster of two nodes, `app-1` and `app-2`.
fn write_cluster_project(
    root: &Path,
    services: &str,
    compose: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(root.join("compose.yaml"), compose)?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1
project:
  id: {PROJECT_ID}
  name: deployment-test
default-environment: production
target: local
stacks:
  app:
    project-name: deployment-test-{{environment}}-{{stack}}
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
{services}
targets:
  cluster:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
environments:
  production:
    target: cluster
    bindings: {{}}
"
        ),
    )?;
    Ok(())
}

fn regular_files(root: &Path) -> Result<Vec<PathBuf>, MmError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|source| MmError::Io {
            context: "enumerating fake runtime operation files".to_owned(),
            source,
        })? {
            let entry = entry.map_err(|source| MmError::Io {
                context: "enumerating a fake runtime operation file".to_owned(),
                source,
            })?;
            if entry
                .file_type()
                .map_err(|source| MmError::Io {
                    context: "inspecting a fake runtime operation file".to_owned(),
                    source,
                })?
                .is_dir()
            {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

fn assert_environment_directory_empty(path: &Path) -> Result<(), std::io::Error> {
    if path.exists() {
        assert!(fs::read_dir(path)?.next().is_none());
    }
    Ok(())
}

fn write_empty_project_config(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: {PROJECT_ID}\n  name: deployment-test\n  development-environment: local\ndefault-environment: local\ntarget: local\nruntime:\n  driver: podman-compose\nstacks: {{}}\nmodules: {{}}\nenvironments:\n  local:\n    bindings: {{}}\n"
        ),
    )?;
    Ok(())
}

/// Puts the cluster fixture's stack and module on an mm network `backend`.
fn on_backend(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join(".config/mm.yaml");
    let config = fs::read_to_string(&path)?
        .replace("stacks:\n", "networks:\n  backend: {}\nstacks:\n")
        .replace(
            "    files: [compose.yaml]\n",
            "    files: [compose.yaml]\n    networks: [backend]\n",
        )
        .replace(
            "  app:\n    path: app\n",
            "  app:\n    path: app\n    networks: [backend]\n",
        );
    fs::write(path, config)?;
    Ok(())
}

/// What `app-1`'s resolver says: `web` has a healthy container on `app-2`.
struct WebOnAppTwo;

impl mm::app::ports::ServiceNames for WebOnAppTwo {
    fn report(&self, _prefix: &str) -> mm::domain::discovery::NameReport {
        mm::domain::discovery::NameReport {
            networks: Vec::new(),
            names: vec![mm::domain::discovery::ResolvedName {
                name: "web".to_owned(),
                network: "backend".to_owned(),
                addresses: vec![mm::domain::discovery::NameAddress {
                    address: [10, 64, 16, 2].into(),
                    node: "app-2".to_owned(),
                    health: mm::domain::discovery::Health::Healthy,
                    unknown_to: Vec::new(),
                    restart_limit: false,
                }],
                lookups: Vec::new(),
                fail_open: false,
            }],
            problems: Vec::new(),
        }
    }
}

/// Leaves and recalls among the fake runtime's calls, so their order shows.
struct RecordingLeaving(Arc<FakeRuntime>);

#[async_trait]
impl mm::app::ports::ContainerLeaving for RecordingLeaving {
    async fn leave(
        &self,
        containers: &[String],
        drain: std::time::Duration,
    ) -> Result<(), MmError> {
        self.0.record(format!(
            "leave:{}:{}s",
            containers.join(","),
            drain.as_secs()
        ))
    }

    async fn recall(&self, project: &str) -> Result<Vec<String>, MmError> {
        self.0.record(format!("recall:{project}"))?;
        Ok(Vec::new())
    }
}

struct NoProxy;

#[async_trait]
impl mm::app::ports::IngressHost for NoProxy {
    async fn ensure_proxy(
        &self,
        _table: &str,
        _routes: &mm::domain::cluster_ingress::IngressTable,
    ) -> Result<(), MmError> {
        Ok(())
    }

    fn keep_certificate(
        &self,
        _table: &str,
        _certificate: &[u8],
        _key: &[u8],
    ) -> Result<String, MmError> {
        Ok(String::new())
    }

    async fn publish(
        &self,
        _table: &str,
        _routes: &mm::domain::cluster_ingress::IngressTable,
        _token: Option<&[u8]>,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn withdraw(&self, _table: &str, _network: Option<&str>) -> Result<(), MmError> {
        Ok(())
    }

    async fn tunnel_status(
        &self,
        _table: &str,
    ) -> Option<mm::domain::cluster_ingress::TunnelStatus> {
        None
    }

    fn public_tables(&self, _except: &str) -> Result<Vec<String>, MmError> {
        Ok(Vec::new())
    }
}
