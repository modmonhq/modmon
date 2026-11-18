use std::fs;

use super::*;
use crate::domain::development_mount::{DevelopmentMountConfig, DevelopmentMountMode};

fn mount() -> DevelopmentMountConfig {
    DevelopmentMountConfig {
        additional: Vec::new(),
        source: ".".into(),
        target: "/workspace".to_owned(),
        mode: DevelopmentMountMode::ReadOnly,
        preserve: Vec::new(),
        excludes: Vec::new(),
        protected: Vec::new(),
        safe_exceptions: Vec::new(),
        services: Vec::new(),
        preset: None,
    }
}

#[test]
fn default_environment_editor_rejects_ambiguous_shapes_and_preserves_simple_edits()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "not: [yaml",
        "---\na: one\n---\na: two\n",
        "- item\n",
        "{default-environment: local}\n",
        "default-environment: one\ndefault-environment: two\n",
        "default-environment:\n  nested: value\n",
    ] {
        assert!(edit_default_environment(source, Some("next")).is_err(), "{source:?}");
    }
    assert!(edit_default_environment("project: demo\r\n", Some("local")).is_err());
    assert_eq!(edit_default_environment("project: demo\n", None)?, "project: demo\n");
    let added = edit_default_environment("project: demo\n", Some("local"))?;
    assert!(added.contains("default-environment: local"));
    let unchanged = edit_default_environment(&added, Some("local"))?;
    assert_eq!(unchanged, added);
    let changed = edit_default_environment(&added, Some("prod"))?;
    assert!(changed.contains("default-environment: prod"));
    assert!(!edit_default_environment(&changed, None)?.contains("default-environment"));
    Ok(())
}

#[test]
fn environment_delivery_editor_covers_shape_validation_insertion_and_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "not: [yaml",
        "---\na: one\n---\na: two\n",
        "- item\n",
        "{environments: {local: {value: one}}}\n",
        "project: demo\n",
        "environments: value\n",
        "environments: {local: {value: one}}\n",
        "environments:\n  local: value\n",
        "environments:\n  local: {value: one}\n",
        "environments:\n  local:\n    environment-delivery: direct\n    environment-delivery: env-file\n",
        "environments:\n  local:\n    environment-delivery:\n      nested: value\n",
    ] {
        assert!(edit_environment_delivery(source, "local", EnvironmentDeliveryMode::Direct, false).is_err(), "{source:?}");
    }
    assert!(edit_environment_delivery(
        "environments:\r\n  local:\r\n    value: one\r\n",
        "local",
        EnvironmentDeliveryMode::Direct,
        false,
    ).is_err());
    assert!(edit_environment_delivery(
        "environments:\n  local: {}\n",
        "local",
        EnvironmentDeliveryMode::Direct,
        false,
    ).is_err());
    let source = "environments:\n  local:\n    value: one\n";
    let added = edit_environment_delivery(source, "local", EnvironmentDeliveryMode::Direct, false)?;
    assert!(added.contains("environment-delivery: direct"));
    let unchanged = edit_environment_delivery(&added, "local", EnvironmentDeliveryMode::Direct, false)?;
    assert_eq!(unchanged, added);
    let changed = edit_environment_delivery(&added, "local", EnvironmentDeliveryMode::EnvFile, false)?;
    assert!(changed.contains("environment-delivery: env-file"));
    Ok(())
}

#[test]
fn development_mount_editor_covers_shape_validation_and_block_lifecycle()
-> Result<(), Box<dyn std::error::Error>> {
    let value = mount();
    for source in [
        "not: [yaml",
        "---\na: one\n---\na: two\n",
        "- item\n",
        "project: demo\n",
        "modules: value\n",
        "modules:\n  other:\n    path: .\n",
        "modules: {api: {path: .}}\n",
        "modules:\n  api: {path: .}\n",
        "modules:\n  api:\n    development-mount:\n      source: .\n    development-mount:\n      source: ..\n",
    ] {
        assert!(edit_development_mount(source, "api", Some(&value), false).is_err(), "{source:?}");
    }
    assert!(edit_development_mount(
        "modules:\r\n  api:\r\n    path: .\r\n",
        "api",
        Some(&value),
        false,
    ).is_err());
    assert!(edit_development_mount(
        "modules:\n  api:\n    path: .\n    'development-mount':\n      source: .\n",
        "api",
        None,
        false,
    ).is_err());
    let source = "modules:\n  api:\n    path: .\n";
    assert_eq!(edit_development_mount(source, "api", None, false)?, source);
    let added = edit_development_mount(source, "api", Some(&value), false)?;
    assert!(added.contains("development-mount:"));
    let replaced = edit_development_mount(&added, "api", Some(&value), false)?;
    assert_eq!(replaced.matches("development-mount:").count(), 1, "{replaced}");
    let removed = edit_development_mount(&replaced, "api", None, false)?;
    assert!(!removed.contains("development-mount:"));
    Ok(())
}

#[test]
fn split_files_edit_their_own_root_mapping() -> Result<(), Box<dyn std::error::Error>> {
    let environments = "local:\n  value: one\n";
    let delivered =
        edit_environment_delivery(environments, "local", EnvironmentDeliveryMode::Direct, true)?;
    assert_eq!(delivered, "local:\n  value: one\n  environment-delivery: direct\n");
    assert!(
        edit_environment_delivery("environments:\n  local: {}\n", "local", EnvironmentDeliveryMode::Direct, true)
            .is_err()
    );

    let modules = "api:\n  path: .\n";
    let mounted = edit_development_mount(modules, "api", Some(&mount()), true)?;
    assert!(mounted.starts_with("api:\n  path: .\n  development-mount:\n"), "{mounted}");
    assert_eq!(edit_development_mount(&mounted, "api", None, true)?, modules);

    assert_eq!(edit_split_scalar(Some("staging"))?, "staging\n");
    assert_eq!(edit_split_scalar(Some("null"))?, "'null'\n");
    assert_eq!(edit_split_scalar(None)?, "null\n");
    Ok(())
}

#[test]
fn initialization_naming_and_compose_discovery_are_deterministic()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join(" My Project! ");
    fs::create_dir(&root)?;
    assert_eq!(sanitize_name(" __ "), "project");
    assert_eq!(sanitize_name(" My Project! "), "my-project");
    assert_eq!(project_name_from_root(Path::new("/")), "project");
    let first = initial_config(&root, None);
    assert_eq!(first.project.name, "my-project");
    assert_eq!(first.stacks["app"].files, vec![PathBuf::from("compose.yaml")]);
    fs::write(root.join("docker-compose.yml"), b"services: {}")?;
    let discovered = initial_config(&root, Some("Explicit Name"));
    assert_eq!(discovered.project.name, "explicit-name");
    assert_eq!(discovered.stacks["app"].files, vec![PathBuf::from("docker-compose.yml")]);
    assert_eq!(first.project.id, initial_config(&root, None).project.id);
    let preview = initialization_preview(&root, None)?;
    assert_eq!(preview.0, dunce::canonicalize(&root)?);
    fs::create_dir_all(root.join(".config"))?;
    fs::write(root.join(CONFIG_RELATIVE_PATH), b"existing")?;
    assert!(initialization_preview(&root, None).is_err());
    assert!(initialization_preview(&temp.path().join("missing"), None).is_err());
    Ok(())
}
