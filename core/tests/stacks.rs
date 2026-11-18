use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;

use mm::adapters::config::{ConfigLoader, compose_project_directory, validate};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{IngressRouteRegistry, TargetStore};
use mm::app::{StackManager, WorkspaceSelection};
use mm::domain::ingress::{
    IngressCertificateReference, IngressRouteOwnership, IngressRouteProvenance,
};
use mm::domain::target::{TargetDefinition, TargetId};
use mm::domain::target_ingress::ingress_loopback_ports;
use uuid::Uuid;

mod temp_dirs;

#[test]
fn compiles_named_stacks_deterministically_with_generated_overlay_last()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );
    assert_eq!(format!("{manager:?}"), "StackManager { .. }");
    let selection = WorkspaceSelection::new(project.path(), None);

    let first = manager.compile(&selection)?;
    let second = manager.compile(&selection)?;
    assert_eq!(first, second);
    assert_eq!(first.target, "local");
    assert_eq!(first.stacks.len(), 2);
    assert_eq!(first.stacks[0].manifest.stack, "app");
    assert_eq!(first.stacks[1].manifest.stack, "jobs");
    assert_eq!(
        first.stacks[0]
            .manifest
            .user_files
            .iter()
            .map(|file| file.path.as_path())
            .collect::<Vec<_>>(),
        vec![Path::new("base.yaml"), Path::new("app.yaml")]
    );
    assert_eq!(
        first.stacks[0].compose_files.last(),
        Some(&first.stacks[0].generated_overlay_path)
    );
    assert!(
        first.stacks[0].compose_files[..2]
            .iter()
            .all(|path| path.starts_with(&first.stacks[0].root))
    );
    assert_eq!(first.stacks[0].manifest.services[0].service, "api");
    assert_eq!(first.stacks[1].manifest.services[0].service, "api");

    let overlay = fs::read_to_string(&first.stacks[0].generated_overlay_path)?;
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&overlay)?;
    assert!(
        parsed["services"]["api"]["labels"]
            .as_mapping()
            .is_some_and(|labels| labels
                .keys()
                .filter_map(serde_yaml_ng::Value::as_str)
                .all(|key| !key.starts_with("traefik.")))
    );
    assert_eq!(parsed["networks"]["mm-ingress"]["external"], true);
    assert_eq!(
        parsed["services"]["api"]["labels"]["io.mm.target-id"],
        "319c527b-6242-7b43-bfd4-bd60e27c1de3"
    );
    assert_eq!(parsed["x-mm"]["target"], "local");
    assert_eq!(
        parsed["x-mm"]["target-id"],
        "319c527b-6242-7b43-bfd4-bd60e27c1de3"
    );
    assert_eq!(parsed["x-mm"]["environment"], "local");
    assert_eq!(parsed["x-mm"]["stack"], "app");
    assert_eq!(first.route_claims.len(), 1);
    assert_eq!(first.route_claims[0].host, "app.example.test");
    assert_eq!(first.route_claims[0].path_prefix, "/api");
    assert_eq!(first.route_claims[0].service, "api");
    assert_eq!(first.route_claims[0].port, 8080);
    assert!(first.route_claims[0].tls);
    assert_eq!(
        parsed["services"]["api"]["networks"]["mm-ingress"]["aliases"][0],
        first.stacks[0].manifest.routes[0].backend_alias
    );
    assert!(!overlay.contains(&project.path().display().to_string()));

    let replacement_manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store_at(
            &home.path().join("replacement-targets.yaml"),
            "local",
            "419c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?,
    );
    let replacement = replacement_manager.compile(&selection)?;
    assert_ne!(replacement.target_id, first.target_id);
    assert_ne!(
        replacement.stacks[0].manifest.artifact_hash,
        first.stacks[0].manifest.artifact_hash
    );
    assert_ne!(
        replacement.stacks[0].manifest.compose_project_name,
        first.stacks[0].manifest.compose_project_name
    );
    assert!(
        replacement.stacks[0].root.starts_with(
            home.path()
                .join("compose/419c527b-6242-7b43-bfd4-bd60e27c1de3")
        )
    );
    let replacement_overlay = fs::read_to_string(&replacement.stacks[0].generated_overlay_path)?;
    let replacement_overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(&replacement_overlay)?;
    assert_ne!(
        replacement_overlay["networks"]["mm-ingress"]["name"],
        parsed["networks"]["mm-ingress"]["name"]
    );
    assert_eq!(
        replacement_overlay["services"]["api"]["labels"]["io.mm.target-id"],
        "419c527b-6242-7b43-bfd4-bd60e27c1de3"
    );

    let original_hash = first.stacks[0].manifest.artifact_hash.clone();
    let config_path = project.path().join(".config/mm.yaml");
    let mut config = fs::read_to_string(&config_path)?;
    config.push_str("# unrelated formatting comment\n");
    fs::write(&config_path, config)?;
    let reformatted = manager.compile(&selection)?;
    assert_eq!(reformatted.stacks[0].manifest.artifact_hash, original_hash);
    assert_eq!(
        reformatted.stacks[1].manifest.artifact_hash,
        first.stacks[1].manifest.artifact_hash
    );
    fs::write(
        project.path().join("base.yaml"),
        "# changed\nservices: {}\n",
    )?;
    assert_eq!(
        fs::read_to_string(&first.stacks[0].compose_files[0])?,
        "services: {}\n"
    );
    let changed = manager.compile(&selection)?;
    assert_ne!(changed.stacks[0].manifest.artifact_hash, original_hash);
    assert_eq!(
        changed.stacks[1].manifest.artifact_hash,
        first.stacks[1].manifest.artifact_hash
    );
    fs::write(changed.stacks[0].root.join("unexpected"), "tampered")?;
    assert!(manager.compile(&selection).is_err());
    Ok(())
}

#[test]
fn compilation_stops_at_target_resolution_and_configuration_validation()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let home = temp_dirs::tempdir()?;
    let artifacts = Arc::new(FilesystemStackArtifactStore::new(
        home.path().join("compose"),
    ));
    let missing_target = StackManager::new(
        Arc::new(ConfigLoader),
        artifacts.clone(),
        target_store_at(
            &home.path().join("other-targets.yaml"),
            "other",
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?,
    );
    let selection = WorkspaceSelection::new(project.path(), None);
    assert!(missing_target.compile(&selection).is_err());

    let config_path = project.path().join(".config/mm.yaml");
    let invalid =
        fs::read_to_string(&config_path)?.replace("app: app.example.test", "app: invalid host");
    fs::write(config_path, invalid)?;
    let invalid_config = StackManager::new(
        Arc::new(ConfigLoader),
        artifacts,
        target_store_at(
            &home.path().join("local-targets.yaml"),
            "local",
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?,
    );
    assert!(matches!(
        invalid_config.compile(&selection),
        Err(mm::domain::error::MmError::Validation { .. })
    ));
    Ok(())
}

#[test]
fn concurrent_first_compiles_publish_one_complete_artifact()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let home = temp_dirs::tempdir()?;
    let artifact_root = home.path().join("compose");
    let targets = target_store(home.path(), "local")?;
    let barrier = Arc::new(Barrier::new(2));
    let handles = [(), ()].map(|()| {
        let project = project.path().to_path_buf();
        let artifact_root = artifact_root.clone();
        let targets = Arc::clone(&targets);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            let manager = StackManager::new(
                Arc::new(ConfigLoader),
                Arc::new(FilesystemStackArtifactStore::new(artifact_root)),
                targets,
            );
            barrier.wait();
            manager.compile(&WorkspaceSelection::new(project, None))
        })
    });
    let [first, second] = handles;
    let first = first.join().map_err(|_| "first compiler panicked")??;
    let second = second.join().map_err(|_| "second compiler panicked")??;
    assert_eq!(first, second);
    for stack in first.stacks {
        assert!(stack.manifest_path.is_file());
        assert!(stack.generated_overlay_path.is_file());
        assert_eq!(
            stack.compose_files.len(),
            stack.manifest.user_files.len() + 1
        );
    }
    Ok(())
}

#[test]
fn target_registry_rejects_cross_project_route_overlap() -> Result<(), Box<dyn std::error::Error>> {
    let first_project = temp_dirs::tempdir()?;
    let second_project = temp_dirs::tempdir()?;
    let non_overlapping_project = temp_dirs::tempdir()?;
    write_route_project(
        first_project.path(),
        "019c527b-6242-7b43-bfd4-bd60e27c1de3",
        "first",
        "/api",
    )?;
    write_route_project(
        second_project.path(),
        "119c527b-6242-7b43-bfd4-bd60e27c1de3",
        "second",
        "/api/v1",
    )?;
    write_route_project(
        non_overlapping_project.path(),
        "219c527b-6242-7b43-bfd4-bd60e27c1de3",
        "third",
        "/apix",
    )?;
    let home = temp_dirs::tempdir()?;
    let registry = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let artifacts = Arc::new(FilesystemStackArtifactStore::new(
        home.path().join("compose"),
    ));
    let manager = StackManager::with_route_registry(
        Arc::new(ConfigLoader),
        artifacts,
        registry.clone(),
        target_store(home.path(), "shared")?,
    );
    let first = manager.compile(&WorkspaceSelection::new(first_project.path(), None))?;
    let second_before_claim =
        manager.compile(&WorkspaceSelection::new(second_project.path(), None))?;
    assert_ne!(
        first.stacks[0].manifest.compose_project_name,
        second_before_claim.stacks[0].manifest.compose_project_name
    );
    registry.replace_owner_claims(
        first.target_id,
        first.route_claims[0].project_id,
        &first.environment,
        &first.route_claims,
        &BTreeMap::new(),
        1_000,
    )?;
    let replacement_target_id = TargetId(Uuid::parse_str("419c527b-6242-7b43-bfd4-bd60e27c1de3")?);
    let mut claims_on_replaced_target = first.route_claims.clone();
    for claim in &mut claims_on_replaced_target {
        claim.target_id = replacement_target_id;
    }
    assert!(registry.conflicts(&claims_on_replaced_target)?.is_empty());
    assert!(
        manager
            .compile(&WorkspaceSelection::new(first_project.path(), None))
            .is_ok()
    );
    assert!(
        manager
            .compile(&WorkspaceSelection::new(second_project.path(), None))
            .is_err()
    );
    assert!(
        manager
            .compile(&WorkspaceSelection::new(
                non_overlapping_project.path(),
                None,
            ))
            .is_ok()
    );
    registry.replace_owner_claims(
        first.target_id,
        first.route_claims[0].project_id,
        &first.environment,
        &[],
        &BTreeMap::new(),
        2_000,
    )?;
    assert!(
        manager
            .compile(&WorkspaceSelection::new(second_project.path(), None))
            .is_ok()
    );
    Ok(())
}

#[test]
fn concurrent_conflicting_claim_replacements_publish_one_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let first_project = temp_dirs::tempdir()?;
    let second_project = temp_dirs::tempdir()?;
    write_route_project(
        first_project.path(),
        "019c527b-6242-7b43-bfd4-bd60e27c1de3",
        "first",
        "/api",
    )?;
    write_route_project(
        second_project.path(),
        "119c527b-6242-7b43-bfd4-bd60e27c1de3",
        "second",
        "/api/v1",
    )?;
    let home = temp_dirs::tempdir()?;
    let registry = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "shared")?,
    );
    let first = manager.compile(&WorkspaceSelection::new(first_project.path(), None))?;
    let second = manager.compile(&WorkspaceSelection::new(second_project.path(), None))?;
    let barrier = Arc::new(Barrier::new(2));
    let handles = [first, second].map(|report| {
        let registry = Arc::clone(&registry);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            registry.replace_owner_claims(
                report.target_id,
                report.route_claims[0].project_id,
                &report.environment,
                &report.route_claims,
                &BTreeMap::new(),
                1_000,
            )
        })
    });
    let [first, second] = handles;
    let first = first.join().map_err(|_| "first claimant panicked")?;
    let second = second.join().map_err(|_| "second claimant panicked")?;
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    Ok(())
}

#[test]
fn rejects_compose_project_name_collisions_between_stacks() -> Result<(), Box<dyn std::error::Error>>
{
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), true)?;
    let document = ConfigLoader.load_from(project.path())?;
    let report = validate(&document, "local");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "MM_COMPOSE_PROJECT_NAME_DUPLICATE" })
    );
    Ok(())
}

#[test]
fn rejects_non_normalized_ingress_paths() -> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace("      route: /api\n", "      route: /api/../admin\n");
    fs::write(config_path, config)?;
    let document = ConfigLoader.load_from(project.path())?;
    let report = validate(&document, "local");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_INGRESS_PATH_INVALID")
    );
    Ok(())
}

#[test]
fn derives_route_identity_from_normalized_host_and_path() -> Result<(), Box<dyn std::error::Error>>
{
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );
    let selection = WorkspaceSelection::new(project.path(), None);
    let compiled = manager.compile(&selection)?;
    let route = &compiled.stacks[0].manifest.routes[0];
    assert_eq!(route.host, "app.example.test");
    assert_eq!(route.path_prefix, "/api");
    assert!(route.tls);
    assert!(route.priority > 1);

    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace("      route: /api\n", "      https: false\n")
        .replace(
            "      app: app.example.test\n",
            "      app: app.localhost\n",
        );
    fs::write(&config_path, config)?;
    let http = manager.compile(&selection)?;
    let http_route = &http.stacks[0].manifest.routes[0];
    assert_eq!(http_route.host, "app.localhost");
    assert_eq!(http_route.path_prefix, "/");
    assert!(!http_route.tls);
    assert!(http.ingress[0].warning.is_some());
    Ok(())
}

#[test]
fn a_route_spreads_when_its_environment_or_the_route_itself_says_so()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::cluster_ingress::Balance;
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );
    let selection = WorkspaceSelection::new(project.path(), None);
    let balance = |manager: &StackManager| -> Result<Balance, Box<dyn std::error::Error>> {
        Ok(manager.compile(&selection)?.stacks[0].manifest.routes[0].balance)
    };
    assert_eq!(balance(&manager)?, Balance::LocalFirst);

    let config_path = project.path().join(".config/mm.yaml");
    let original = fs::read_to_string(&config_path)?;
    let spread = original.replace(
        "  local:\n    domains:\n",
        "  local:\n    ingress:\n      balance: spread\n    domains:\n",
    );
    fs::write(&config_path, &spread)?;
    assert_eq!(balance(&manager)?, Balance::Spread);

    let local = spread.replace(
        "      route: /api\n",
        "      route: /api\n      balance: local-first\n",
    );
    fs::write(&config_path, local)?;
    assert_eq!(balance(&manager)?, Balance::LocalFirst);

    let autoscaled = original.replace(
        "      api:\n        stack: app\n",
        "      api:\n        stack: app\n        autoscale: {scale: {max: 3}}\n",
    );
    fs::write(&config_path, autoscaled)?;
    assert_eq!(
        balance(&manager)?,
        Balance::Spread,
        "containers autoscale adds on other nodes take requests"
    );
    Ok(())
}

#[test]
fn infers_only_one_declared_target_port_from_the_ordered_compose_model()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );
    let selection = WorkspaceSelection::new(project.path(), None);

    for compose in [
        "services:\n  api:\n    image: example/app\n    expose: ['8080/tcp']\n",
        "services:\n  api:\n    image: example/app\n    ports: ['127.0.0.1:18080:8080/tcp']\n",
        "services:\n  api:\n    image: example/app\n    ports:\n      - target: 8080\n        published: 18080\n        protocol: tcp\n",
    ] {
        fs::write(project.path().join("app.yaml"), compose)?;
        let report = manager.compile(&selection)?;
        assert_eq!(report.stacks[0].manifest.routes[0].port, 8080);
    }

    fs::write(
        project.path().join("base.yaml"),
        "services:\n  api:\n    expose: [9090]\n",
    )?;
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    expose: [8080]\n",
    )?;
    let error = match manager.compile(&selection) {
        Err(error) => error,
        Ok(_) => {
            return Err(std::io::Error::other(
                "multiple effective target ports unexpectedly compiled",
            )
            .into());
        }
    };
    assert!(
        error
            .to_string()
            .contains("multiple target ports (8080, 9090)")
    );
    assert!(error.to_string().contains("set ingress.port explicitly"));

    fs::write(project.path().join("base.yaml"), "services: {}\n")?;
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n",
    )?;
    let error = match manager.compile(&selection) {
        Err(error) => error,
        Ok(_) => {
            return Err(std::io::Error::other("missing target port unexpectedly compiled").into());
        }
    };
    assert!(error.to_string().contains("declares no target port"));
    assert!(
        error
            .to_string()
            .contains("service.expose or service.ports")
    );
    Ok(())
}

#[test]
fn omitted_service_requires_exactly_one_configured_stack_service()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "      api:\n        stack: app\n  frontend:",
        "      api:\n        stack: app\n      worker:\n        stack: app\n  frontend:",
    );
    fs::write(config_path, config)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );
    let error = match manager.compile(&WorkspaceSelection::new(project.path(), None)) {
        Err(error) => error,
        Ok(_) => {
            return Err(std::io::Error::other("ambiguous service unexpectedly compiled").into());
        }
    };
    assert!(
        error
            .to_string()
            .contains("multiple configured services (api, worker)")
    );
    assert!(error.to_string().contains("set ingress.service explicitly"));
    Ok(())
}

#[test]
fn rejects_transitive_compose_topology_files() -> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_multi_stack_project(project.path(), false)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );
    let selection = WorkspaceSelection::new(project.path(), None);

    fs::write(
        project.path().join("app.yaml"),
        "include:\n  - included.yaml\nservices:\n  api:\n    image: example/app\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    extends:\n      file: included.yaml\n      service: api\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "x-shared: &shared\n  include: [included.yaml]\n<<: *shared\nservices:\n  api:\n    image: example/app\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "x-base: &base\n  extends:\n    file: included.yaml\n    service: api\nservices:\n  api:\n    <<: *base\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    labels:\n      traefik.enable: 'true'\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    networks: [mm-ingress]\nnetworks:\n  mm-ingress:\n    external: true\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\nvolumes:\n  stolen:\n    external: true\n    name: mm-ingress-acme-forbidden\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\nvolumes:\n  stolen:\n    external:\n      name: mm-ingress-config-forbidden\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    network_mode: host\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  mm-ingress-backend-spoof:\n    image: example/app\n  api:\n    image: example/app\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    let proxy_ports = ingress_loopback_ports(TargetId(Uuid::parse_str(
        "319c527b-6242-7b43-bfd4-bd60e27c1de3",
    )?));
    for proxy_port in [proxy_ports.0, proxy_ports.1, proxy_ports.2] {
        fs::write(
            project.path().join("app.yaml"),
            format!(
                "services:\n  api:\n    image: example/app\n    ports: ['127.0.0.1:{proxy_port}:8080']\n"
            ),
        )?;
        assert!(manager.compile(&selection).is_err());
        fs::write(
            project.path().join("app.yaml"),
            format!(
                "services:\n  api:\n    image: example/app\n    ports:\n      - target: 8080\n        published: '{}-{}'\n",
                proxy_port - 1,
                proxy_port
            ),
        )?;
        assert!(manager.compile(&selection).is_err());
    }
    for ports in [
        "['127.0.0.1:${PUBLISHED_PORT}:8080']",
        "\n      - target: 8080\n        published: ${PUBLISHED_PORT:-30000}",
    ] {
        fs::write(
            project.path().join("app.yaml"),
            format!("services:\n  api:\n    image: example/app\n    ports: {ports}\n"),
        )?;
        assert!(manager.compile(&selection).is_err());
    }
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    volumes_from: [container:mm-ingress]\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services: !override\n  api:\n    image: example/app\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  MM-INGRESS-BACKEND-spoof:\n    image: example/app\n  api:\n    image: example/app\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    for identity in [
        "hostname: MM-INGRESS-BACKEND-spoof",
        "container_name: mm-ingress-backend-spoof",
        "container_name: ${NAME:-safe}",
        "links: [worker:MM-INGRESS-BACKEND-spoof]",
    ] {
        fs::write(
            project.path().join("app.yaml"),
            format!(
                "services:\n  worker:\n    image: example/worker\n  api:\n    image: example/app\n    {identity}\n"
            ),
        )?;
        assert!(manager.compile(&selection).is_err());
    }
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n  worker:\n    image: example/worker\n    network_mode: ${MODE:-host}\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    volumes: [/tmp:/stolen]\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    volumes: [stolen:/stolen]\nvolumes:\n  stolen:\n    driver: local\n    driver_opts:\n      type: none\n      device: /tmp\n      o: bind\n",
    )?;
    assert!(manager.compile(&selection).is_err());
    fs::write(
        project.path().join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    expose: [8080]\n    volumes: [./backend:/app, cache:/cache]\nvolumes:\n  cache:\n    driver: local\n",
    )?;
    assert!(manager.compile(&selection).is_ok());
    Ok(())
}

#[test]
fn nested_first_compose_file_defines_the_project_directory()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir_all(project.path().join("deploy"))?;
    fs::write(project.path().join("deploy/compose.yaml"), "services: {}\n")?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: nested\ndefault-environment: local\ntarget: local\nstacks:\n  app:\n    files: [deploy/compose.yaml]\nenvironments:\n  local:\n    bindings: {}\n",
    )?;
    let document = ConfigLoader.load_from(project.path())?;
    assert_eq!(
        compose_project_directory(&document, "app"),
        project.path().join("deploy")
    );
    Ok(())
}

#[test]
fn generates_trusted_local_https_with_auditable_metadata() -> Result<(), Box<dyn std::error::Error>>
{
    let project = temp_dirs::tempdir()?;
    write_local_convention_project(
        project.path(),
        "services:\n  web_api:\n    image: example/web\n    expose: [8080]\n",
        "      web_api:\n        stack: app\n",
        "",
    )?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );

    let first = manager.compile(&WorkspaceSelection::new(project.path(), None))?;
    let second = manager.compile(&WorkspaceSelection::new(project.path(), None))?;

    assert_eq!(first, second);
    assert_eq!(first.route_claims.len(), 1);
    let route = &first.route_claims[0];
    assert_eq!(route.host, "web-api.file-factory.test");
    assert_eq!(route.port, 8080);
    assert!(route.tls);
    assert_eq!(
        route.provenance,
        IngressRouteProvenance::AutomaticConvention
    );
    assert_eq!(
        route.certificate_reference,
        IngressCertificateReference::LocalCa {
            host: route.host.clone()
        }
    );
    assert!(matches!(
        &route.ownership,
        IngressRouteOwnership::Project {
            environment,
            stack,
            service,
            ..
        } if environment == "development" && stack == "app" && service == "web_api"
    ));
    assert_eq!(
        first.ingress[0].url.as_deref(),
        Some("https://web-api.file-factory.test")
    );
    assert_eq!(first.ingress[0].reason, None);
    Ok(())
}

#[test]
fn explicit_route_precedes_the_local_convention() -> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_local_convention_project(
        project.path(),
        "services:\n  web:\n    image: example/web\n    expose: [8080]\n",
        "      web:\n        stack: app\n",
        "    ingress:\n      domain: custom\n      service: web\n      port: 9090\n",
    )?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "  development:\n    bindings: {}\n",
        "  development:\n    domains:\n      custom: custom.example.test\n    bindings: {}\n",
    );
    fs::write(config_path, config)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );

    let report = manager.compile(&WorkspaceSelection::new(project.path(), None))?;

    assert_eq!(report.route_claims.len(), 1);
    assert_eq!(report.route_claims[0].host, "custom.example.test");
    assert_eq!(report.route_claims[0].port, 9090);
    assert_eq!(
        report.route_claims[0].provenance,
        IngressRouteProvenance::ExplicitConfig
    );
    assert!(
        !report
            .route_claims
            .iter()
            .any(|route| route.host == "web.file-factory.test")
    );
    Ok(())
}

#[test]
fn development_mail_bindings_preserve_project_values_and_honor_opt_outs()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::development_mail::MailBindingProvenance;

    let project = temp_dirs::tempdir()?;
    write_local_convention_project(
        project.path(),
        "services:\n  web:\n    image: example/web\n  worker:\n    image: example/worker\n  database:\n    image: example/database\n",
        "      web:\n        stack: app\n        env:\n          MM_MAIL_SMTP_HOST: project-mail\n      worker:\n        stack: app\n        role: worker\n        development-mail: false\n      database:\n        stack: app\n        role: database\n",
        "",
    )?;
    fs::write(
        project.path().join("app/.env.example"),
        "MM_MAIL_SMTP_HOST=\n",
    )?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "    path: app\n    services:\n",
        "    path: app\n    env:\n      template: .env.example\n    services:\n",
    );
    fs::write(&config_path, config)?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );

    let report = manager.compile(&WorkspaceSelection::new(project.path(), None))?;
    let manifest = &report.stacks[0].manifest;
    let web = manifest
        .services
        .iter()
        .find(|service| service.service == "web")
        .ok_or_else(|| std::io::Error::other("web service is missing"))?;
    assert_eq!(web.mail_bindings.len(), 4);
    assert!(web.mail_bindings.iter().any(|binding| {
        binding.key == "MM_MAIL_SMTP_HOST"
            && binding.provenance == MailBindingProvenance::PreservedProjectValue
    }));
    assert!(
        manifest
            .services
            .iter()
            .filter(|service| service.service != "web")
            .all(|service| service.mail_bindings.is_empty())
    );
    let overlay = fs::read_to_string(&report.stacks[0].generated_overlay_path)?;
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&overlay)?;
    assert_eq!(
        parsed["services"]["web"]["environment"]["MM_MAIL_SMTP_PORT"],
        "1025"
    );
    assert!(parsed["services"]["web"]["environment"]["MM_MAIL_SMTP_HOST"].is_null());
    assert!(parsed["services"]["web"]["networks"]["mm-services-development"].is_mapping());
    assert!(parsed["services"]["worker"]["networks"]["mm-services-development"].is_null());

    let config = fs::read_to_string(&config_path)?.replace(
        "    path: app\n    env:\n",
        "    path: app\n    development-mail: false\n    env:\n",
    );
    fs::write(&config_path, config)?;
    let report = manager.compile(&WorkspaceSelection::new(project.path(), None))?;
    assert!(
        report.stacks[0]
            .manifest
            .services
            .iter()
            .all(|service| service.mail_bindings.is_empty())
    );
    assert!(parsed["services"]["database"]["networks"]["mm-services-development"].is_null());

    let production = manager.compile(&WorkspaceSelection::new(
        project.path(),
        Some("production".to_owned()),
    ))?;
    let production_overlay = fs::read_to_string(&production.stacks[0].generated_overlay_path)?;
    assert!(!production_overlay.contains("mm-services-development"));
    assert!(!production_overlay.contains("MM_MAIL_SMTP_PORT"));
    Ok(())
}

#[test]
fn automatic_routes_require_unambiguous_http_evidence_or_a_guided_port()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_local_convention_project(
        project.path(),
        "services:\n  web:\n    image: example/web\n    expose: [8080, 9090]\n  worker:\n    image: example/worker\n    expose: [7000]\n  queue:\n    image: example/queue\n    expose: [5672]\n",
        "      web:\n        stack: app\n      worker:\n        stack: app\n        role: worker\n      queue:\n        stack: app\n        role: queue\n",
        "",
    )?;
    let home = temp_dirs::tempdir()?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        target_store(home.path(), "local")?,
    );

    let ambiguous = manager.compile(&WorkspaceSelection::new(project.path(), None))?;
    assert!(ambiguous.route_claims.is_empty());
    assert!(ambiguous.ingress.iter().any(|status| {
        status.service == "web"
            && status
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("ambiguous"))
    }));
    assert!(ambiguous.ingress.iter().any(|status| {
        status.service == "worker"
            && status
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("not http-routable"))
    }));
    assert!(ambiguous.ingress.iter().any(|status| {
        status.service == "queue"
            && status
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("not http-routable"))
    }));

    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "      web:\n        stack: app\n",
        "      web:\n        stack: app\n        http-port: 9090\n",
    );
    fs::write(config_path, config)?;
    let guided = manager.compile(&WorkspaceSelection::new(project.path(), None))?;
    assert_eq!(guided.route_claims.len(), 1);
    assert_eq!(guided.route_claims[0].port, 9090);

    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  worker:\n    image: example/worker\n    expose: [7000]\n",
    )?;
    let absent = manager.compile(&WorkspaceSelection::new(project.path(), None))?;
    assert!(absent.route_claims.is_empty());
    assert!(absent.ingress.iter().any(|status| {
        status.service == "web"
            && status
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("absent"))
    }));
    Ok(())
}

#[test]
fn automatic_routes_are_limited_to_the_local_development_target()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    write_local_convention_project(
        project.path(),
        "services:\n  web:\n    image: example/web\n    expose: [8080]\n",
        "      web:\n        stack: app\n",
        "",
    )?;
    let home = temp_dirs::tempdir()?;
    let local = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("local-compose"),
        )),
        target_store(home.path(), "local")?,
    );
    let production = local.compile(&WorkspaceSelection::new(
        project.path(),
        Some("production".to_owned()),
    ))?;
    assert!(production.route_claims.is_empty());

    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(
        home.path().join("remote-targets.yaml"),
    ));
    let mut remote = TargetDefinition::local(TargetId(Uuid::parse_str(
        "519c527b-6242-7b43-bfd4-bd60e27c1de3",
    )?));
    remote.transport = mm::domain::target::TargetTransport::Ssh {
        host: "example.test".to_owned(),
        port: 22,
        user: "deploy".to_owned(),
        host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU".to_owned(),
        host_key: None,
        identity_file: None,
        remote_root: None,
        daemon_fingerprint: None,
    };
    targets.insert_target("remote", remote)?;
    let remote_manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("remote-compose"),
        )),
        targets,
    );
    let mut remote_selection = WorkspaceSelection::new(project.path(), None);
    remote_selection.target = Some("remote".to_owned());
    let remote = remote_manager.compile(&remote_selection)?;
    assert!(remote.route_claims.is_empty());
    assert!(
        remote.ingress[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("remote"))
    );
    Ok(())
}

fn write_multi_stack_project(
    root: &Path,
    collide_project_names: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("backend"))?;
    fs::create_dir_all(root.join("frontend"))?;
    fs::write(root.join("base.yaml"), "services: {}\n")?;
    fs::write(
        root.join("app.yaml"),
        "services:\n  api:\n    image: example/app\n    expose: [8080]\n",
    )?;
    fs::write(
        root.join("jobs.yaml"),
        "services:\n  api:\n    image: example/jobs\n",
    )?;
    let jobs_project_name = if collide_project_names {
        "shared"
    } else {
        "demo-{environment}-{stack}"
    };
    let app_project_name = if collide_project_names {
        "shared"
    } else {
        "demo-{environment}-{stack}"
    };
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: demo\ndefault-environment: local\ntarget: local\nruntime:\n  driver: podman-compose\nstacks:\n  app:\n    project-name: {app_project_name}\n    files: [base.yaml, app.yaml]\n    ingress:\n      domain: app\n      route: /api\n  jobs:\n    project-name: {jobs_project_name}\n    files: [jobs.yaml]\nmodules:\n  backend:\n    path: backend\n    services:\n      api:\n        stack: app\n  frontend:\n    path: frontend\n    services:\n      api:\n        stack: jobs\nenvironments:\n  local:\n    domains:\n      app: app.example.test\n    bindings: {{}}\n"
        ),
    )?;
    Ok(())
}

fn write_route_project(
    root: &Path,
    project_id: &str,
    project_name: &str,
    path_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: {project_id}\n  name: {project_name}\ndefault-environment: local\ntarget: shared\nruntime:\n  driver: podman-compose\nstacks:\n  app:\n    project-name: shared-runtime\n    files: [compose.yaml]\n    ingress:\n      domain: shared\n      route: {path_prefix}\n      service: web\n      port: 8080\nmodules:\n  app:\n    path: app\n    services:\n      web:\n        stack: app\nenvironments:\n  local:\n    domains:\n      shared: shared.example.test\n    bindings: {{}}\n"
        ),
    )?;
    Ok(())
}

fn write_local_convention_project(
    root: &Path,
    compose: &str,
    services: &str,
    stack_extra: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(root.join("compose.yaml"), compose)?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: File_Factory\n  development-environment: development\ndefault-environment: development\ntarget: local\nruntime:\n  driver: podman-compose\nstacks:\n  app:\n    files: [compose.yaml]\n{stack_extra}modules:\n  app:\n    path: app\n    services:\n{services}environments:\n  development:\n    bindings: {{}}\n  production:\n    bindings: {{}}\n"
        ),
    )?;
    Ok(())
}

fn target_store(
    root: &Path,
    name: &str,
) -> Result<Arc<dyn TargetStore>, Box<dyn std::error::Error>> {
    target_store_at(
        &root.join("targets.yaml"),
        name,
        "319c527b-6242-7b43-bfd4-bd60e27c1de3",
    )
}

fn target_store_at(
    path: &Path,
    name: &str,
    id: &str,
) -> Result<Arc<dyn TargetStore>, Box<dyn std::error::Error>> {
    let store: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(path.to_path_buf()));
    store.insert_target(
        name,
        TargetDefinition::local(TargetId(Uuid::parse_str(id)?)),
    )?;
    Ok(store)
}
