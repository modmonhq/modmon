use std::path::Path;

use super::*;
use crate::domain::compose_timing::{COMPOSE_FLOOR, ComposeOrder};
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
        project_limit(&both, ComposeOrder::Dependencies, COMPOSE_FLOOR),
        Duration::from_secs(300)
    );
    let first = target(temp.path(), &["compose.yaml"]);
    assert_eq!(
        project_limit(&first, ComposeOrder::Dependencies, COMPOSE_FLOOR),
        Duration::from_secs(180)
    );
    assert_eq!(
        project_limit(&first, ComposeOrder::Dependencies, Duration::from_secs(300)),
        Duration::from_secs(300)
    );
    Ok(())
}

#[test]
fn a_file_that_cannot_be_read_leaves_the_floor() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    std::fs::write(temp.path().join("broken.yaml"), "services: [")?;
    let target = target(temp.path(), &["missing.yaml", "broken.yaml"]);
    assert_eq!(
        project_limit(&target, ComposeOrder::Dependencies, COMPOSE_FLOOR),
        COMPOSE_FLOOR
    );
    Ok(())
}

#[test]
fn a_grace_from_a_variable_reads_the_targets_interpolation_environment() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    std::fs::write(
        temp.path().join("compose.yaml"),
        "services:\n  db:\n    image: db\n    stop_grace_period: ${DB_GRACE:-2m}\n  web:\n    image: web\n",
    )?;
    let mut target = target(temp.path(), &["compose.yaml"]);
    assert_eq!(
        project_limit(&target, ComposeOrder::Dependencies, COMPOSE_FLOOR),
        Duration::from_secs(180)
    );
    assert_eq!(
        project_limit(&target, ComposeOrder::OneByOne, COMPOSE_FLOOR),
        Duration::from_secs(190)
    );
    target
        .interpolation_environment
        .insert("DB_GRACE".to_owned(), "5m".to_owned());
    assert_eq!(
        project_limit(&target, ComposeOrder::Dependencies, COMPOSE_FLOOR),
        Duration::from_secs(360)
    );
    Ok(())
}
