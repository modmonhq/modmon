use std::fs;

use super::*;
use crate::adapters::config::ConfigLoader;
use crate::adapters::environment::EnvironmentIndexer;
use crate::app::stacks::tests::support::StackFixture;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn override_service_filters_prune_compose_overlays_routes_networks_and_environment_inputs()
-> TestResult {
    let mut fixture = StackFixture::new(
        "services:\n  web: {image: example/web, expose: [8080]}\n  scheduler: {image: example/worker}\n",
    )?;
    let source = fs::read(fixture.root.path().join("compose.yaml"))?;
    fs::write(
        fixture.root.path().join("app/.env.example"),
        "WORKER_TOKEN= # mm:from=WORKER.TOKEN\n",
    )?;
    fs::write(
        fixture.root.path().join(".config/mm.overrides.yaml"),
        "modules:\n  worker:\n    path: app\n    env: {template: .env.example}\n    networks: [backend]\n    services:\n      scheduler:\n        stack: app\n        role: scheduler\n        environments: [production]\n",
    )?;
    fixture.document = ConfigLoader.load_from(fixture.root.path())?;
    let membership = active_network_membership(&fixture.document, "development")?;
    assert!(!membership.contains_key(&("app".to_owned(), "scheduler".to_owned())));
    let compiled = compile_document(
        &fixture.document,
        "development",
        &fixture.target,
        &BTreeMap::new(),
        &load_active_middlewares(&fixture.document, "development")?,
        &membership,
    )?;
    assert_eq!(compiled.len(), 1);
    assert_eq!(compiled[0].manifest.services.len(), 1);
    for file in &compiled[0].user_files {
        let model: Value = serde_yaml_ng::from_slice(&file.bytes)?;
        assert!(model["services"].get("scheduler").is_none());
        assert!(model["services"].get("web").is_some());
        assert_eq!(
            file.manifest.digest,
            blake3::hash(&file.bytes).to_hex().to_string()
        );
    }
    let overlay: Value = serde_yaml_ng::from_slice(&compiled[0].generated_overlay)?;
    assert!(overlay["services"].get("scheduler").is_none());
    assert!(
        compiled[0]
            .manifest
            .routes
            .iter()
            .all(|route| route.service != "scheduler")
    );
    let excluded = excluded_services(&fixture.document, "development");
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0].service, "scheduler");
    assert!(
        EnvironmentIndexer
            .index(&fixture.document, "development")
            .modules
            .is_empty()
    );
    let production_index = EnvironmentIndexer.index(&fixture.document, "production");
    assert_eq!(production_index.modules[0].services[0].name, "scheduler");
    let production = compile_document(
        &fixture.document,
        "production",
        &fixture.target,
        &BTreeMap::new(),
        &load_active_middlewares(&fixture.document, "production")?,
        &active_network_membership(&fixture.document, "production")?,
    )?;
    assert_eq!(production[0].manifest.services.len(), 2);
    assert!(excluded_services(&fixture.document, "production").is_empty());
    assert_eq!(fs::read(fixture.root.path().join("compose.yaml"))?, source);
    Ok(())
}

#[test]
fn fully_excluded_stacks_are_omitted_and_need_no_published_images() -> TestResult {
    let mut fixture = StackFixture::new("services: {web: {image: example/web}}\n")?;
    let module = fixture
        .document
        .config
        .modules
        .get_mut("app")
        .ok_or("module")?;
    module
        .services
        .get_mut("web")
        .ok_or("service")?
        .environments = vec!["production".to_owned()];
    module.build = Some(crate::domain::config::ModuleBuildConfig {
        image: Some("example/web".to_owned()),
        ..Default::default()
    });
    assert!(resolve_deployment_images(&fixture.document, "development", None)?.is_empty());
    let compiled = compile_document(
        &fixture.document,
        "development",
        &fixture.target,
        &BTreeMap::new(),
        &load_active_middlewares(&fixture.document, "development")?,
        &active_network_membership(&fixture.document, "development")?,
    )?;
    assert!(compiled.is_empty());
    assert_eq!(excluded_services(&fixture.document, "development").len(), 1);
    assert!(resolve_deployment_images(&fixture.document, "production", None).is_err());
    Ok(())
}

#[test]
fn required_dependencies_cannot_reintroduce_excluded_services_and_optional_ones_are_removed()
-> TestResult {
    let excluded = BTreeSet::from(["worker".to_owned()]);
    for dependency in [
        "depends_on: [worker]",
        "depends_on: {worker: {condition: service_started}}",
        "network_mode: service:worker",
        "ipc: service:worker",
        "pid: service:worker",
        "links: [worker:alias]",
        "volumes_from: [worker:ro]",
    ] {
        let source = format!(
            "services:\n  web:\n    image: example/web\n    {dependency}\n  worker: {{image: example/worker}}\n"
        );
        let error =
            filter_compose_services(Path::new("compose.yaml"), source.into_bytes(), &excluded)
                .err()
                .ok_or("dependency accepted")?;
        assert!(
            error.to_string().contains("excluded service 'worker'"),
            "{error}"
        );
    }
    let source = b"x-base: &base\n  image: example/web\n  depends_on: {worker: {condition: service_started, required: false}}\nservices:\n  web:\n    <<: *base\n  worker: {image: example/worker}\n";
    let filtered = filter_compose_services(Path::new("compose.yaml"), source.to_vec(), &excluded)?;
    let model: Value = serde_yaml_ng::from_slice(&filtered)?;
    assert!(model["services"].get("worker").is_none());
    assert!(
        model["services"]["web"]["depends_on"]
            .get("worker")
            .is_none()
    );
    assert_eq!(
        model["services"]["web"]["image"].as_str(),
        Some("example/web")
    );
    Ok(())
}

#[test]
fn validation_rejects_unknown_environments_and_active_routes_to_excluded_services() -> TestResult {
    let mut fixture = StackFixture::new("services: {web: {image: example/web, expose: [8080]}}\n")?;
    fixture
        .document
        .config
        .modules
        .get_mut("app")
        .ok_or("module")?
        .services
        .get_mut("web")
        .ok_or("service")?
        .environments = vec!["typo".to_owned()];
    assert!(
        validate(&fixture.document, "development")
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_SERVICE_ENVIRONMENT_UNKNOWN")
    );
    fixture
        .document
        .config
        .modules
        .get_mut("app")
        .ok_or("module")?
        .services
        .get_mut("web")
        .ok_or("service")?
        .environments = vec!["production".to_owned()];
    fixture
        .document
        .config
        .stacks
        .get_mut("app")
        .ok_or("stack")?
        .ingress = serde_yaml_ng::from_str("{domain: public, service: web, port: 8080}")?;
    assert!(
        validate(&fixture.document, "development")
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_INGRESS_SERVICE_EXCLUDED")
    );
    assert!(
        compile_document(
            &fixture.document,
            "development",
            &fixture.target,
            &BTreeMap::new(),
            &load_active_middlewares(&fixture.document, "development")?,
            &BTreeMap::new()
        )
        .is_err()
    );
    Ok(())
}
