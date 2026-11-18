use std::fs;

use crate::adapters::state::SqliteStateStore;
use crate::domain::build::{
    BuildReusePolicy, BuildSource, BuildTagStrategy, BuiltImage, ImageBuildScope,
    ImageBuildUnitPlan, ImageBuildVariantPlan, unit_execution_fingerprint, unit_input_fingerprint,
};
use crate::domain::config::{ModuleBuildConfig, ServiceConfig};
use crate::domain::operation::OperationId;

use super::*;
use crate::app::stacks::tests::support::{StackFixture, candidate, service};

fn published_build(
    document: &ConfigDocument,
    module: &str,
    publish: bool,
) -> Result<(ImageBuildPlan, ImageBuildResult), Box<dyn std::error::Error>> {
    let platforms = document.config.build.platforms.clone();
    let mut unit = ImageBuildUnitPlan {
        module: module.to_owned(),
        image: format!("registry.example/{module}"),
        context: PathBuf::from(module),
        dockerfile: PathBuf::from(format!("{module}/Dockerfile")),
        context_hash: "a".repeat(64),
        dockerfile_hash: "b".repeat(64),
        args: BTreeMap::from([("VERSION".to_owned(), "01234567".to_owned())]),
        secret_ids: Vec::new(),
        credential_secret_ids: Vec::new(),
        dependencies: Vec::new(),
        upstream_node_ids: Vec::new(),
        extra_inputs: Vec::new(),
        version_embedded: false,
        reuse_policy: BuildReusePolicy::Allowed,
        input_fingerprint: String::new(),
        variants: vec![ImageBuildVariantPlan {
            target: None,
            tag_suffix: String::new(),
            tags: vec![
                "edge".to_owned(),
                "sha-0123456789abcdef0123456789abcdef01234567".to_owned(),
            ],
            stage: None,
            args: BTreeMap::new(),
        }],
    };
    unit.input_fingerprint = unit_input_fingerprint(&unit, &platforms)?;
    let plan = ImageBuildPlan::new(
        ImageBuildScope {
            project_id: document.config.project.id,
            config_hash: document.source_hash.clone(),
            environment: "development".to_owned(),
        },
        BuildSource {
            commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            branch: Some("main".to_owned()),
            tags_at_head: Vec::new(),
            repository_tags: Vec::new(),
            tags_complete: true,
            default_branch: true,
            dirty: false,
        },
        publish,
        BuildTagStrategy::Auto,
        platforms,
        vec![unit],
    )?;
    let unit = &plan.units[0];
    let result = ImageBuildResult {
        operation_id: OperationId::new(),
        plan_hash: plan.plan_hash.clone(),
        images: vec![BuiltImage {
            module: module.to_owned(),
            target: None,
            repository: unit.image.clone(),
            tags: unit.variants[0]
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest: format!("sha256:{}", "c".repeat(64)),
            execution_fingerprint: unit_execution_fingerprint(
                &unit.input_fingerprint,
                &plan.environment,
                None,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )?,
            secret_revisions: BTreeMap::new(),
            archive: (!publish).then(|| PathBuf::from("/tmp/app.tar")),
            reused: false,
        }],
        dependencies: Vec::new(),
    };
    Ok((plan, result))
}

#[test]
fn build_receipts_hold_published_builds_and_supply_only_the_environment_they_are_bound_to()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::build::BuildReceipt;
    use crate::domain::remote::check_receipt_scope;
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let mut document = fixture.document.clone();
    document
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .build = Some(ModuleBuildConfig {
        enabled: true,
        image: Some("registry.example/app".to_owned()),
        ..ModuleBuildConfig::default()
    });

    let (plan, result) = published_build(&document, "app", false)?;
    assert!(BuildReceipt::new(plan, result).is_err());
    let (plan, result) = published_build(&document, "app", true)?;
    let receipt = BuildReceipt::new(plan, result)?;
    let images = deployment_images_from_receipt(&document, "development", &receipt)?;
    assert_eq!(
        images.get("app").map(String::as_str),
        Some(format!("registry.example/app@sha256:{}", "c".repeat(64)).as_str())
    );
    // A build for one environment never supplies another's images.
    assert!(matches!(
        deployment_images_from_receipt(&document, "production", &receipt),
        Err(MmError::Security { .. })
    ));
    let mut tampered = receipt.clone();
    tampered.result.images[0].digest = format!("sha256:{}", "d".repeat(64));
    assert!(matches!(
        deployment_images_from_receipt(&document, "development", &tampered),
        Err(MmError::Security { .. })
    ));

    let project = document.config.project.id;
    let commit = receipt.source_commit().to_owned();
    assert!(check_receipt_scope(&receipt, project, &commit).is_ok());
    assert!(check_receipt_scope(&receipt, project, &"f".repeat(40)).is_err());
    let other = crate::domain::workspace::ProjectId(uuid::Uuid::from_u128(7));
    assert!(check_receipt_scope(&receipt, other, &commit).is_err());
    Ok(())
}

#[test]
fn runtime_status_uses_the_recorded_published_digest() -> Result<(), Box<dyn std::error::Error>> {
    use crate::app::ports::TargetStore;
    use crate::domain::operation::{NewOperation, OperationKind, OperationState};
    use crate::domain::workspace::WorkspaceRegistration;
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let path = fixture.root.path().join(".config/mm.yaml");
    fs::write(
        &path,
        fs::read_to_string(&path)?.replace(
            "    path: app",
            "    path: app\n    build:\n      enabled: true\n      image: registry.example/app",
        ),
    )?;
    let document = crate::adapters::config::ConfigLoader.load_from(fixture.root.path())?;
    let home = crate::test_support::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let workspace = state.register_workspace(
        &WorkspaceRegistration {
            project_id: document.config.project.id,
            root: document.root.clone(),
            config_path: document.path.clone(),
        },
        1,
    )?;
    let (plan, result) = published_build(&document, "app", true)?;
    state.begin_operation(&NewOperation {
        id: result.operation_id,
        workspace_id: workspace.id,
        kind: OperationKind::Build,
        environment: "development".to_owned(),
        requested_at_unix_ms: 2,
        plan_hash: Some(plan.plan_hash.clone()),
    })?;
    state.save_image_build_plan(result.operation_id, &plan)?;
    state.save_image_build_result(result.operation_id, &result)?;
    state.finish_operation(result.operation_id, OperationState::Succeeded, 3, None)?;
    let targets = Arc::new(crate::adapters::target::FilesystemTargetStore::new(
        home.path().join("targets.yaml"),
    ));
    targets.insert_target("local", fixture.target.definition.clone())?;
    let manager = StackManager::new(
        Arc::new(crate::adapters::config::ConfigLoader),
        Arc::new(crate::adapters::stack::FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        targets,
    )
    .with_state(state);
    let runtimes = manager.runtime_targets(&document, "development", &fixture.target)?;
    let overlay = fs::read_to_string(runtimes[0].compose_files.last().ok_or("missing overlay")?)?;
    assert!(overlay.contains(&format!("registry.example/app@sha256:{}", "c".repeat(64))));
    Ok(())
}

#[test]
fn document_compilation_builds_canonical_artifacts_and_fails_closed_on_sources_and_images()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {image: example/web, expose: [8080]}}")?;
    let membership = resolve_network_membership(&fixture.document)?;
    let middlewares = load_active_middlewares(&fixture.document, "development")?;
    let artifacts = compile_document(
        &fixture.document,
        "development",
        &fixture.target,
        &BTreeMap::new(),
        &middlewares,
        &membership,
    )?;
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].manifest.services[0].service, "web");
    assert_eq!(artifacts[0].manifest.routes[0].port, 8080);
    assert!(!artifacts[0].manifest.artifact_hash.is_empty());
    assert!(
        String::from_utf8(artifacts[0].generated_overlay.clone())?.contains("mm-ingress-backend")
    );

    fs::remove_file(fixture.root.path().join("compose.yaml"))?;
    let error = compile_document(
        &fixture.document,
        "development",
        &fixture.target,
        &BTreeMap::new(),
        &middlewares,
        &membership,
    )
    .err()
    .ok_or("missing Compose source unexpectedly compiled")?;
    assert!(error.to_string().contains("disappeared"));

    fs::write(
        fixture.root.path().join("compose.yaml"),
        "services: {web: {image: example/web, expose: [8080]}}",
    )?;
    let mut buildable = fixture.document.clone();
    buildable
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .build = Some(ModuleBuildConfig::default());
    let error = compile_document(
        &buildable,
        "development",
        &fixture.target,
        &BTreeMap::new(),
        &middlewares,
        &membership,
    )
    .err()
    .ok_or("buildable service without a digest unexpectedly compiled")?;
    assert!(matches!(
        error,
        MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingImages { .. }
        }
    ));

    let invalid_images = BTreeMap::from([("app".to_owned(), "example/web:latest".to_owned())]);
    assert!(
        compile_document(
            &buildable,
            "development",
            &fixture.target,
            &invalid_images,
            &middlewares,
            &membership,
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn build_variant_selection_is_deterministic_and_rejects_ambiguity()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    assert_eq!(
        selected_build_variant(&fixture.document, "app", "development")?,
        None
    );

    let mut document = fixture.document.clone();
    let build = ModuleBuildConfig {
        targets: BTreeMap::from([
            ("api".to_owned(), "api".into()),
            ("worker".to_owned(), "worker".into()),
        ]),
        ..ModuleBuildConfig::default()
    };
    document
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .build = Some(build);
    assert!(selected_build_variant(&document, "app", "development").is_err());

    document
        .config
        .modules
        .get_mut("app")
        .and_then(|module| module.build.as_mut())
        .ok_or("missing build")?
        .default_variant = Some("api".to_owned());
    assert_eq!(
        selected_build_variant(&document, "app", "development")?.as_deref(),
        Some("api")
    );
    document
        .config
        .environments
        .get_mut("development")
        .ok_or("missing environment")?
        .build_variant = Some("worker".to_owned());
    assert_eq!(
        selected_build_variant(&document, "app", "development")?.as_deref(),
        Some("worker")
    );
    document
        .config
        .environments
        .get_mut("development")
        .ok_or("missing environment")?
        .build_variant = Some("missing".to_owned());
    assert!(selected_build_variant(&document, "app", "development").is_err());

    let build = document
        .config
        .modules
        .get_mut("app")
        .and_then(|module| module.build.as_mut())
        .ok_or("missing build")?;
    build.default_variant = None;
    build.targets.remove("worker");
    document
        .config
        .environments
        .get_mut("development")
        .ok_or("missing environment")?
        .build_variant = None;
    assert_eq!(
        selected_build_variant(&document, "app", "development")?.as_deref(),
        Some("api")
    );
    Ok(())
}

#[test]
fn local_ingress_gate_slug_and_image_requirements_cover_policy_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    assert!(automatic_local_ingress_enabled(
        &fixture.document,
        "development",
        &fixture.target
    ));
    assert!(!automatic_local_ingress_enabled(
        &fixture.document,
        "production",
        &fixture.target
    ));
    assert!(!automatic_local_ingress_enabled(
        &fixture.document,
        "development",
        &fixture.remote_target()
    ));

    let mut disabled = fixture.document.clone();
    disabled.config.ingress.automatic_local = Some(false);
    assert!(!automatic_local_ingress_enabled(
        &disabled,
        "development",
        &fixture.target
    ));
    disabled.config.ingress.automatic_local = None;
    disabled
        .config
        .environments
        .get_mut("development")
        .ok_or("missing environment")?
        .automatic_local_ingress = Some(false);
    assert!(!automatic_local_ingress_enabled(
        &disabled,
        "development",
        &fixture.target
    ));

    let mut invalid_slug = fixture.document.clone();
    invalid_slug.config.project.name = "---".to_owned();
    assert!(canonical_project_slug(&invalid_slug).is_err());

    let mut buildable = fixture.document.clone();
    buildable
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .build = Some(ModuleBuildConfig::default());
    let web = service("web", true, vec![candidate(8080, TransportProtocol::Tcp)]);
    assert!(require_deployment_images(&buildable, &[web]).is_err());
    assert!(require_deployment_images(&fixture.document, &[]).is_ok());
    Ok(())
}

#[test]
fn compilation_propagates_mount_service_route_and_portability_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    let middlewares = load_active_middlewares(&fixture.document, "development")?;
    let membership = resolve_network_membership(&fixture.document)?;

    let mut invalid_mount = fixture.document.clone();
    invalid_mount
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .development_mount = Some(crate::domain::development_mount::DevelopmentMountConfig {
        additional: Vec::new(),
        source: PathBuf::from("missing"),
        target: "/app".to_owned(),
        mode: Default::default(),
        preserve: Vec::new(),
        excludes: Vec::new(),
        protected: Vec::new(),
        safe_exceptions: Vec::new(),
        services: vec!["web".to_owned()],
        preset: None,
    });
    assert!(
        compile_document(
            &invalid_mount,
            "development",
            &fixture.target,
            &BTreeMap::new(),
            &middlewares,
            &membership,
        )
        .is_err()
    );

    fs::write(
        fixture.root.path().join("compose.yaml"),
        "services: {web: {extends: base}}",
    )?;
    assert!(
        compile_document(
            &fixture.document,
            "development",
            &fixture.target,
            &BTreeMap::new(),
            &middlewares,
            &membership,
        )
        .is_err()
    );

    let mut invalid_slug = fixture.document.clone();
    let service_config = invalid_slug
        .config
        .modules
        .get_mut("app")
        .and_then(|module| module.services.remove("web"))
        .ok_or("missing service")?;
    invalid_slug
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .services
        .insert("---".to_owned(), ServiceConfig { ..service_config });
    fs::write(
        fixture.root.path().join("compose.yaml"),
        "services: {'---': {expose: [8080]}}",
    )?;
    let invalid_membership = resolve_network_membership(&invalid_slug)?;
    assert!(
        compile_document(
            &invalid_slug,
            "development",
            &fixture.target,
            &BTreeMap::new(),
            &middlewares,
            &invalid_membership,
        )
        .is_err()
    );

    fs::write(
        fixture.root.path().join("compose.yaml"),
        "services: {web: {expose: [8080], networks: [backend]}}",
    )?;
    assert!(
        compile_document(
            &fixture.document,
            "development",
            &fixture.target,
            &BTreeMap::new(),
            &middlewares,
            &membership,
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn deployment_images_require_current_published_exact_variant_results()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = StackFixture::new("services: {web: {expose: [8080]}}")?;
    fixture
        .document
        .config
        .modules
        .get_mut("app")
        .ok_or("missing app")?
        .build = Some(ModuleBuildConfig {
        image: Some("registry.example/app".to_owned()),
        ..ModuleBuildConfig::default()
    });
    let buildable = ["app"];
    let (plan, result) = published_build(&fixture.document, "app", true)?;
    let images =
        deployment_images_from_build(&fixture.document, "development", &buildable, &plan, &result)?;
    assert_eq!(
        images["app"],
        format!("registry.example/app@sha256:{}", "c".repeat(64))
    );

    let mut stale = result.clone();
    stale.plan_hash = "stale".to_owned();
    assert!(matches!(
        deployment_images_from_build(&fixture.document, "development", &buildable, &plan, &stale,),
        Err(MmError::Security { .. })
    ));

    let (unpublished_plan, unpublished_result) = published_build(&fixture.document, "app", false)?;
    assert!(matches!(
        deployment_images_from_build(
            &fixture.document,
            "development",
            &buildable,
            &unpublished_plan,
            &unpublished_result,
        ),
        Err(MmError::Security { .. })
    ));

    let (other_plan, other_result) = published_build(&fixture.document, "other", true)?;
    assert!(matches!(
        deployment_images_from_build(
            &fixture.document,
            "development",
            &buildable,
            &other_plan,
            &other_result,
        ),
        Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingImages { .. }
        })
    ));

    let mut buildable_document = fixture.document.clone();
    buildable_document
        .config
        .modules
        .get_mut("app")
        .ok_or("missing module")?
        .build = Some(ModuleBuildConfig::default());
    let state_dir = crate::test_support::tempdir()?;
    let state_path = state_dir.path().join("state.sqlite3");
    let state = SqliteStateStore::open(&state_path)?;
    assert!(matches!(
        resolve_deployment_images(&buildable_document, "development", Some(&state)),
        Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingImages { .. }
        })
    ));
    rusqlite::Connection::open(&state_path)?.execute("DROP TABLE image_build_snapshots", [])?;
    assert!(matches!(
        resolve_deployment_images(&buildable_document, "development", Some(&state)),
        Err(MmError::State { .. })
    ));
    Ok(())
}
