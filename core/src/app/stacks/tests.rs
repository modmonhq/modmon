use std::fs;

use crate::adapters::config::ConfigLoader;
use crate::adapters::stack::FilesystemStackArtifactStore;
use crate::adapters::state::SqliteStateStore;
use crate::adapters::target::FilesystemTargetStore;
use crate::domain::target::TargetDefinition;
use crate::domain::workspace::ProjectId;

use super::*;

pub(super) mod support;
use support::StackFixture;

#[test]
fn manager_compilation_runtime_projection_and_registry_conflicts_use_the_same_artifact_model()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {image: example/web, expose: [8080]}}")?;
    let home = crate::test_support::tempdir()?;
    let targets = Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target("local", fixture.target.definition.clone())?;
    let artifacts = Arc::new(FilesystemStackArtifactStore::new(
        home.path().join("compose"),
    ));
    let state = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let manager = StackManager::new(Arc::new(ConfigLoader), artifacts.clone(), targets.clone())
        .with_state(state.clone());
    let selection = WorkspaceSelection::new(fixture.root.path(), None);
    let report = manager.compile(&selection)?;
    assert_eq!(report.stacks.len(), 1);
    assert_eq!(report.route_claims.len(), 1);
    assert_eq!(manager.resolve_target("local")?.name, "local");
    assert_eq!(manager.targets().list_targets()?.len(), 1);

    let runtimes = manager.runtime_targets(&fixture.document, "development", &fixture.target)?;
    assert_eq!(runtimes.len(), 1);
    assert_eq!(runtimes[0].stack_name, "app");
    assert_eq!(runtimes[0].user_compose_file_count, 1);

    let deployment = manager.compile_for_deployment(&selection, state.as_ref(), None)?;
    assert_eq!(deployment.project_id, fixture.document.config.project.id);
    assert_eq!(deployment.networks.len(), 2);
    assert!(!deployment.workload_intent_hash.is_empty());
    assert_eq!(deployment.project_directories.len(), 1);

    let mut existing = report.route_claims[0].clone();
    existing.project_id = ProjectId(uuid::Uuid::parse_str(
        "119c527b-6242-7b43-bfd4-bd60e27c1de3",
    )?);
    existing.project_name = "other-project".to_owned();
    existing.ownership = IngressRouteOwnership::Project {
        project_id: existing.project_id,
        environment: "development".to_owned(),
        stack: "app".to_owned(),
        service: "web".to_owned(),
    };
    state.replace_owner_claims(
        report.target_id,
        existing.project_id,
        "development",
        &[existing],
        &BTreeMap::new(),
        1,
    )?;
    let guarded =
        StackManager::with_route_registry(Arc::new(ConfigLoader), artifacts, state, targets);
    let error = guarded
        .compile(&selection)
        .err()
        .ok_or("conflicting route unexpectedly compiled")?;
    assert!(
        error
            .to_string()
            .contains("conflicts with project 'other-project'")
    );
    assert!(
        guarded
            .runtime_targets(&fixture.document, "development", &fixture.target)
            .is_err()
    );
    Ok(())
}

#[test]
fn manager_propagates_document_target_validation_and_artifact_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = StackFixture::new("services: {web: {image: example/web, expose: [8080]}}")?;
    let home = crate::test_support::tempdir()?;
    let targets = Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(fixture.target.definition.id),
    )?;
    let artifacts = Arc::new(FilesystemStackArtifactStore::new(
        home.path().join("compose"),
    ));
    let manager = StackManager::new(Arc::new(ConfigLoader), artifacts, targets);
    let selection = WorkspaceSelection::new(fixture.root.path(), None);
    let first = manager.compile(&selection)?;
    fs::write(first.stacks[0].root.join("unexpected"), "tamper")?;
    assert!(manager.compile(&selection).is_err());

    let config_path = fixture.root.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace("public: public.example.test", "public: invalid host");
    fs::write(config_path, config)?;
    assert!(matches!(
        manager.compile(&selection),
        Err(MmError::Validation { .. })
    ));

    let missing = WorkspaceSelection::new(fixture.root.path().join("missing"), None);
    assert!(manager.compile(&missing).is_err());
    Ok(())
}
