use std::path::Path;

use super::*;
use crate::domain::compose_timing::COMPOSE_FLOOR;
use crate::domain::runtime::RuntimeEnvironment;
use crate::domain::workspace::ProjectId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn target(root: &Path, files: &[&str]) -> RuntimeTarget {
    RuntimeTarget {
        root: root.to_path_buf(),
        project_directory: root.to_path_buf(),
        target: None,
        project_id: ProjectId(uuid::Uuid::from_u128(8)),
        environment: "test".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "mm-app".to_owned(),
        artifact_hash: "a".repeat(64),
        compose_files: files.iter().map(|file| root.join(file)).collect(),
        user_compose_file_count: 1,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

#[test]
fn the_files_of_the_project_set_its_limit_and_a_later_file_wins() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    std::fs::write(
        temp.path().join("compose.yaml"),
        "services:\n  db:\n    image: db\n    stop_grace_period: 2m\n  web:\n    image: web\n",
    )?;
    std::fs::write(
        temp.path().join("override.yaml"),
        "services:\n  db:\n    stop_grace_period: 4m\n",
    )?;
    let both = target(temp.path(), &["compose.yaml", "override.yaml"]);
    assert_eq!(
        project_limit(&both, COMPOSE_FLOOR),
        Duration::from_secs(300)
    );
    let first = target(temp.path(), &["compose.yaml"]);
    assert_eq!(
        project_limit(&first, COMPOSE_FLOOR),
        Duration::from_secs(180)
    );
    assert_eq!(
        project_limit(&first, Duration::from_secs(300)),
        Duration::from_secs(300)
    );
    Ok(())
}

#[test]
fn a_file_that_cannot_be_read_leaves_the_floor() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    std::fs::write(temp.path().join("broken.yaml"), "services: [")?;
    let target = target(temp.path(), &["missing.yaml", "broken.yaml"]);
    assert_eq!(project_limit(&target, COMPOSE_FLOOR), COMPOSE_FLOOR);
    Ok(())
}
