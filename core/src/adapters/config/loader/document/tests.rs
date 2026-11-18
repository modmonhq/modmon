use std::fs;

use super::*;
use crate::domain::config::ModuleConfig;
use crate::domain::development_mount::{DevelopmentMountConfig, DevelopmentMountMode};

#[test]
fn initialization_discovery_loading_and_editing_form_a_reversible_workflow()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("project");
    fs::create_dir(&root)?;
    let loader = ConfigLoader;
    let initialized = loader.initialize(&root, Some("coverage-project"))?;
    let path = root.join(CONFIG_RELATIVE_PATH);
    assert_eq!(initialized.document.path, path);
    assert_eq!(initialized.document.config.project.name, "coverage-project");
    assert_eq!(initialized.starter_files.len(), 3);
    assert!(root.join(ANSIBLE_CONFIG_RELATIVE_PATH).is_file());
    assert!(root.join(ANSIBLE_PLAYBOOK_RELATIVE_PATH).is_file());

    let nested = root.join("nested/deeper");
    fs::create_dir_all(&nested)?;
    assert_eq!(loader.discover(&nested)?, path);
    assert_eq!(loader.discover(&initialized.starter_files[0])?, path);
    assert_eq!(loader.load_from(&nested)?, initialized.document);
    assert!(loader.initialize(&root, None).is_err());
    assert!(loader.discover(&temp.path().join("missing")).is_err());
    assert!(loader.discover(Path::new(".")).is_err());

    let environment = initialized
        .document
        .config
        .environments
        .keys()
        .next()
        .ok_or("initial environment missing")?
        .clone();
    let selected = loader.set_default_environment(&root, Some(&environment))?;
    assert_eq!(selected.config.default_environment.as_deref(), Some(environment.as_str()));
    let cleared = loader.set_default_environment(&root, None)?;
    assert!(cleared.config.default_environment.is_none());
    assert!(loader.set_default_environment(&root, Some("missing")).is_err());
    let direct = loader.set_environment_delivery(
        &root,
        &environment,
        EnvironmentDeliveryMode::Direct,
    )?;
    assert_eq!(
        direct.config.environments[&environment].environment_delivery,
        Some(EnvironmentDeliveryMode::Direct)
    );
    let env_file = loader.set_environment_delivery(
        &root,
        &environment,
        EnvironmentDeliveryMode::EnvFile,
    )?;
    assert_eq!(
        env_file.config.environments[&environment].environment_delivery,
        Some(EnvironmentDeliveryMode::EnvFile)
    );
    assert!(loader.set_environment_delivery(&root, "missing", EnvironmentDeliveryMode::Direct).is_err());
    assert!(loader.schema().get("$comment").is_some());

    fs::remove_file(path.with_extension("yaml.lock"))?;
    initialized.rollback()?;
    assert!(!path.exists());
    assert!(!root.join(".config/ansible").exists());
    Ok(())
}

#[test]
fn project_document_port_delegates_and_rollback_removes_only_created_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("port-project");
    fs::create_dir(&root)?;
    let loader = ConfigLoader;
    let port: &dyn crate::app::ports::ProjectDocumentStore = &loader;
    let initialized = port.initialize(&root, None)?;
    let path = root.join(CONFIG_RELATIVE_PATH);
    assert_eq!(port.discover(&root)?, path);
    assert_eq!(port.load(&path)?, *initialized.document());
    assert_eq!(port.load_from(&root)?, *initialized.document());
    assert_eq!(initialized.starter_files().len(), 3);
    assert!(port.schema().get("$comment").is_some());
    let environment = initialized
        .document()
        .config
        .environments
        .keys()
        .next()
        .ok_or("initial environment missing")?
        .clone();
    port.set_default_environment(&root, Some(&environment))?;
    port.set_environment_delivery(&root, &environment, EnvironmentDeliveryMode::Direct)?;
    fs::remove_file(path.with_extension("yaml.lock"))?;
    initialized.rollback()?;
    assert!(!path.exists());
    Ok(())
}

#[test]
fn loading_rejects_wrong_locations_encoding_yaml_and_schema()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("invalid-project");
    fs::create_dir(&root)?;
    let loader = ConfigLoader;
    let initialized = loader.initialize(&root, Some("invalid-cases"))?;
    let path = initialized.document.path.clone();
    let valid = fs::read(&path)?;

    let wrong = root.join("mm.yaml");
    fs::write(&wrong, &valid)?;
    assert!(loader.load(&wrong).is_err());
    assert!(loader.load(&root.join("missing.yaml")).is_err());

    fs::write(&path, [0xff, 0xfe])?;
    assert!(loader.load(&path).is_err());
    fs::write(&path, b"not: [valid")?;
    assert!(loader.load(&path).is_err());
    let source = String::from_utf8(valid.clone())?;
    let unsupported = source.replacen(
        &format!("schema-version: {CONFIG_SCHEMA_VERSION}"),
        "schema-version: 999999",
        1,
    );
    fs::write(&path, unsupported)?;
    assert!(loader.load(&path).is_err());
    fs::write(&path, valid)?;
    assert!(loader.load(&path).is_ok());
    Ok(())
}

#[test]
fn development_mount_updates_require_current_hash_and_existing_module()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("mount-project");
    fs::create_dir(&root)?;
    let loader = ConfigLoader;
    let initialized = loader.initialize(&root, Some("mount-project"))?;
    let path = initialized.document.path.clone();
    let mut config = initialized.document.config.clone();
    config.modules.insert(
        "api".to_owned(),
        ModuleConfig {
            path: "api".into(),
            services: Default::default(),
            networks: Vec::new(),
            env: None,
            build: None,
            development_mount: None,
            development_mail: None,
            development: None,
        },
    );
    fs::write(&path, serde_yaml_ng::to_string(&config)?)?;
    fs::write(root.join(OVERRIDES_RELATIVE_PATH), "build: {platforms: [linux/arm64]}\n")?;
    let current = loader.load(&path)?;
    let mount = DevelopmentMountConfig {
        additional: Vec::new(),
        source: ".".into(),
        target: "/workspace".to_owned(),
        mode: DevelopmentMountMode::ReadOnly,
        preserve: Vec::new(),
        excludes: vec!["vendor".into()],
        protected: Vec::new(),
        safe_exceptions: Vec::new(),
        services: vec!["web".to_owned()],
        preset: None,
    };

    assert!(loader.set_development_mount(&root, "api", Some(&mount), "stale").is_err());
    assert!(loader.set_development_mount(&root, "missing", Some(&mount), &current.source_hash).is_err());
    let updated = loader.set_development_mount(&root, "api", Some(&mount), &current.source_hash)?;
    assert_eq!(updated.config.modules["api"].development_mount.as_ref(), Some(&mount));
    let cleared = loader.set_development_mount(&root, "api", None, &updated.source_hash)?;
    assert!(cleared.config.modules["api"].development_mount.is_none());

    let port: &dyn crate::app::ports::ProjectDocumentStore = &loader;
    let via_port = port.set_development_mount(&root, "api", Some(&mount), &cleared.source_hash)?;
    assert_eq!(via_port.config.modules["api"].development_mount.as_ref(), Some(&mount));
    assert_eq!(via_port.config.build.platforms, ["linux/arm64"]);
    assert!(!fs::read_to_string(&path)?.contains("linux/arm64"));
    Ok(())
}

#[test]
fn initialization_recovers_marker_and_rejects_unsafe_root_shapes()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("recovery-project");
    fs::create_dir(&root)?;
    let loader = ConfigLoader;
    let initialized = loader.initialize(&root, None)?;
    let marker = root.join(".config/ansible").join(INITIALIZING_MARKER);
    fs::write(&marker, INITIALIZING_MARKER_CONTENT)?;
    let recovered = loader.initialize(&root, None)?;
    assert_eq!(recovered.document, initialized.document);
    assert!(!marker.exists());
    assert!(loader.initialize(&root, None).is_err());

    let file_root = temp.path().join("not-directory");
    fs::write(&file_root, b"file")?;
    assert!(loader.initialize(&file_root, None).is_err());

    let blocked = temp.path().join("blocked");
    fs::create_dir(&blocked)?;
    fs::write(blocked.join(".config"), b"file")?;
    assert!(loader.initialize(&blocked, None).is_err());
    Ok(())
}

#[test]
fn loading_rejects_symlinks_directories_and_oversized_documents()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("bounded-project");
    fs::create_dir_all(root.join(".config"))?;
    let path = root.join(CONFIG_RELATIVE_PATH);
    fs::write(&path, vec![b'x'; 1024 * 1024 + 1])?;
    assert!(ConfigLoader.load(&path).is_err());
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(ConfigLoader.load(&path).is_err());
    fs::remove_dir(&path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let source = root.join("source.yaml");
        fs::write(&source, b"schema-version: 1")?;
        symlink(&source, &path)?;
        assert!(ConfigLoader.load(&path).is_err());
    }
    Ok(())
}

#[test]
fn initialization_helpers_cover_scaffold_recovery_publication_and_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("project");
    fs::create_dir_all(root.join(".config"))?;

    let scaffold = initialize_ansible_scaffold(&root)?;
    assert_eq!(scaffold.starter_files.len(), 2);
    verify_recoverable_ansible_scaffold(&root.join(".config/ansible"))?;
    let recovered = initialize_ansible_scaffold(&root)?;
    assert_eq!(recovered.created_files.len(), 3);

    fs::write(root.join(ANSIBLE_CONFIG_RELATIVE_PATH), b"changed")?;
    assert!(verify_recoverable_ansible_scaffold(&root.join(".config/ansible")).is_err());
    fs::write(root.join(ANSIBLE_CONFIG_RELATIVE_PATH), ANSIBLE_CONFIG)?;
    fs::remove_file(&scaffold.marker)?;
    assert!(initialize_ansible_scaffold(&root).is_err());

    let standalone = root.join("standalone.txt");
    write_new_project_file(&standalone, b"one")?;
    assert!(write_new_project_file(&standalone, b"two").is_err());
    let published = root.join("published.txt");
    publish_new_project_file(&published, b"published")?;
    assert_eq!(fs::read(&published)?, b"published");
    assert!(publish_new_project_file(&published, b"again").is_err());

    let rollback_file = root.join("rollback-file");
    let rollback_directory = root.join("rollback-directory");
    fs::write(&rollback_file, b"remove")?;
    fs::create_dir(&rollback_directory)?;
    rollback_paths(
        std::slice::from_ref(&rollback_file),
        std::slice::from_ref(&rollback_directory),
    )?;
    assert!(!rollback_file.exists());
    assert!(!rollback_directory.exists());

    let nonempty = root.join("nonempty");
    fs::create_dir(&nonempty)?;
    fs::write(nonempty.join("child"), b"keep")?;
    assert!(rollback_paths(&[], std::slice::from_ref(&nonempty)).is_err());
    assert!(rollback_paths(std::slice::from_ref(&nonempty), &[]).is_err());
    let original = MmError::State { message: "original".to_owned() };
    assert!(matches!(
        rollback_or(original, &[], std::slice::from_ref(&nonempty)),
        MmError::State { .. }
    ));

    let not_directory = root.join("not-directory");
    fs::write(&not_directory, b"file")?;
    assert!(verify_recoverable_ansible_scaffold(&not_directory).is_err());
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt as _;

        let invalid_names = root.join("invalid-names");
        fs::create_dir(&invalid_names)?;
        fs::write(invalid_names.join(OsString::from_vec(vec![0xff])), b"file")?;
        assert!(verify_recoverable_ansible_scaffold(&invalid_names).is_err());
    }
    assert!(write_new_project_file(Path::new("/proc/mm-project-file"), b"value").is_err());
    assert!(matches!(
        project_file_cleanup_error(
            MmError::State { message: "primary".to_owned() },
            &nonempty
        ),
        MmError::State { .. }
    ));
    assert!(publish_new_project_file(Path::new("/"), b"value").is_err());
    assert!(sync_project_directory(&root.join("missing-directory")).is_err());
    Ok(())
}

#[test]
fn edit_entry_points_map_lock_and_lossless_editor_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("edit-errors");
    fs::create_dir(&root)?;
    let loader = ConfigLoader;
    let initialized = loader.initialize(&root, None)?;
    let path = initialized.document.path.clone();
    let environment = initialized
        .document
        .config
        .environments
        .keys()
        .next()
        .ok_or("initial environment")?
        .clone();
    let lock = path.with_extension("yaml.lock");
    if lock.exists() {
        fs::remove_file(&lock)?;
    }
    fs::create_dir(&lock)?;
    assert!(loader.set_default_environment(&root, Some(&environment)).is_err());
    assert!(loader
        .set_environment_delivery(&root, &environment, EnvironmentDeliveryMode::Direct)
        .is_err());
    assert!(loader
        .set_development_mount(&root, "missing", None, &initialized.document.source_hash)
        .is_err());
    fs::remove_dir(&lock)?;

    let mut config = initialized.document.config.clone();
    config.modules.insert(
        "api".to_owned(),
        ModuleConfig {
            path: "api".into(),
            services: Default::default(),
            networks: Vec::new(),
            env: None,
            build: None,
            development_mount: None,
            development_mail: None,
            development: None,
        },
    );
    fs::write(&path, serde_json::to_vec(&config)?)?;
    let current = loader.load(&path)?;
    assert!(loader.set_default_environment(&root, Some(&environment)).is_err());
    assert!(loader
        .set_environment_delivery(&root, &environment, EnvironmentDeliveryMode::Direct)
        .is_err());
    assert!(loader
        .set_development_mount(&root, "api", None, &current.source_hash)
        .is_err());
    Ok(())
}

#[test]
fn initialization_and_loading_reject_missing_and_non_unicode_roots()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = crate::test_support::tempdir()?;
    let loader = ConfigLoader;
    assert!(loader.initialize(&temp.path().join("missing"), None).is_err());

    let valid_root = temp.path().join("valid");
    fs::create_dir(&valid_root)?;
    let initialized = loader.initialize(&valid_root, None)?;
    let source = fs::read(&initialized.document.path)?;
    if !crate::test_support::NON_UTF8_FILE_NAMES {
        return Ok(());
    }
    let invalid_root = temp.path().join(OsString::from_vec(vec![0xff]));
    fs::create_dir_all(invalid_root.join(".config"))?;
    let path = invalid_root.join(CONFIG_RELATIVE_PATH);
    fs::write(&path, source)?;
    assert!(loader.load(&path).is_err());
    Ok(())
}

#[test]
fn locked_edit_rejects_invalid_output_concurrent_changes_and_disappearing_parent()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("locked-edit-errors");
    fs::create_dir(&root)?;
    let loader = ConfigLoader;
    let initialized = loader.initialize(&root, None)?;
    let path = initialized.document.path.clone();

    assert!(
        loader
            .edit_locked(&root, "project", &mut |_document, _source, _split| Ok("not: [valid".to_owned()))
            .is_err()
    );
    assert!(
        loader
            .edit_locked(&root, "project", &mut |document, source, _split| {
                fs::write(&document.path, format!("{source}# concurrent\n")).map_err(
                    |source| MmError::Io {
                        context: "injecting a concurrent configuration change".to_owned(),
                        source,
                    },
                )?;
                Ok(source.to_owned())
            })
            .is_err()
    );

    let changed = fs::read_to_string(&path)?;
    fs::write(&path, changed.trim_end_matches("# concurrent\n"))?;
    let config_directory = root.join(".config");
    let moved = root.join(".config-moved");
    let result = loader.edit_locked(&root, "project", &mut |_document, source, _split| {
        fs::rename(&config_directory, &moved).map_err(|source| MmError::Io {
            context: "moving the configuration directory during the test".to_owned(),
            source,
        })?;
        Ok(source.to_owned())
    });
    fs::rename(&moved, &config_directory)?;
    assert!(result.is_err());
    Ok(())
}
