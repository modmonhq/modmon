use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;
use crate::domain::config::{ConfigDocument, MmConfig};
use crate::domain::development_mount::{DevelopmentMountConfig, PreservedContainerPath};

fn test_document(root: &Path) -> Result<ConfigDocument, serde_yaml_ng::Error> {
    let config = serde_yaml_ng::from_str::<MmConfig>(
        "schema-version: 1\n\
         project:\n  id: 00000000-0000-0000-0000-000000000001\n  name: project\n  development-environment: development\n\
         target: local\n\
         stacks:\n  app:\n    files: [compose.yaml]\n\
         modules:\n  app:\n    path: app\n    services:\n      web: {stack: app}\n\
         environments:\n  development: {}\n",
    )?;
    Ok(ConfigDocument {
        config,
        root: root.to_path_buf(),
        path: root.join(".config/mm.yaml"),
        sources: Default::default(),
        source_hash: "source".to_owned(),
    })
}

fn mount(source: &str) -> DevelopmentMountConfig {
    DevelopmentMountConfig {
        source: PathBuf::from(source),
        target: "/app".to_owned(),
        mode: DevelopmentMountMode::ReadWrite,
        additional: Vec::new(),
        preserve: Vec::new(),
        excludes: Vec::new(),
        protected: Vec::new(),
        safe_exceptions: Vec::new(),
        services: vec!["web".to_owned()],
        preset: None,
    }
}

fn fixture() -> Result<(TempDir, ConfigDocument), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    fs::create_dir_all(temporary.path().join("app/src"))?;
    let document = test_document(temporary.path())?;
    Ok((temporary, document))
}

fn scan_source(
    document: &ConfigDocument,
    module: &str,
    source: &Path,
    mount: &DevelopmentMountConfig,
) -> Result<(), MmError> {
    super::super::policy::scan_source_at(document, module, source, &mount.target, mount)
}

#[test]
fn intent_requires_canonical_contained_directories_and_valid_container_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temporary, document) = fixture()?;
    let module = &document.config.modules["app"];
    validate_intent(&document, "app", module, &mount("src"))?;

    let mut invalid_target = mount("src");
    invalid_target.target = "relative".to_owned();
    assert!(validate_intent(&document, "app", module, &invalid_target).is_err());

    let noncanonical = mount(r"src\nested");
    assert!(validate_intent(&document, "app", module, &noncanonical).is_err());

    let missing_source = mount("missing");
    assert!(validate_intent(&document, "app", module, &missing_source).is_err());

    fs::write(document.root.join("app/file"), "not a directory")?;
    let file_source = mount("file");
    assert!(validate_intent(&document, "app", module, &file_source).is_err());

    let missing_root = test_document(document.root.join("missing").as_path())?;
    assert!(
        validate_intent(
            &missing_root,
            "app",
            &missing_root.config.modules["app"],
            &mount("src")
        )
        .is_err()
    );

    let module_missing_root = document.root.join("module-missing");
    let module_missing = test_document(&module_missing_root)?;
    fs::create_dir(&module_missing_root)?;
    assert!(
        validate_intent(
            &module_missing,
            "app",
            &module_missing.config.modules["app"],
            &mount("src")
        )
        .is_err()
    );

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::fs::symlink;

        let mut non_unicode = mount("src");
        non_unicode.source = std::ffi::OsString::from_vec(vec![0xff]).into();
        assert!(validate_intent(&document, "app", module, &non_unicode).is_err());

        fs::create_dir(document.root.join("outside"))?;
        symlink("../outside", document.root.join("app/linked"))?;
        assert!(validate_intent(&document, "app", module, &mount("linked")).is_err());
    }
    Ok(())
}

#[test]
fn intent_rejects_duplicate_preserves_excludes_and_unsafe_exceptions()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temporary, document) = fixture()?;
    let module = &document.config.modules["app"];

    let mut invalid_preserve = mount("src");
    invalid_preserve.preserve = vec![PreservedContainerPath {
        path: "relative".to_owned(),
    }];
    assert!(validate_intent(&document, "app", module, &invalid_preserve).is_err());

    let mut duplicate_preserve = mount("src");
    duplicate_preserve.preserve = vec![
        PreservedContainerPath {
            path: "/app/cache".to_owned(),
        },
        PreservedContainerPath {
            path: "/app/cache".to_owned(),
        },
    ];
    assert!(validate_intent(&document, "app", module, &duplicate_preserve).is_err());

    let mut duplicate_exclude = mount("src");
    duplicate_exclude.preserve = vec![PreservedContainerPath {
        path: "/app/cache".to_owned(),
    }];
    duplicate_exclude.excludes = vec![PathBuf::from("cache")];
    assert!(validate_intent(&document, "app", module, &duplicate_exclude).is_err());

    let mut invalid_exclude = mount("src");
    invalid_exclude.excludes = vec![PathBuf::from("../cache")];
    assert!(validate_intent(&document, "app", module, &invalid_exclude).is_err());

    let mut invalid_exception = mount("src");
    invalid_exception.safe_exceptions = vec![PathBuf::from("/absolute")];
    assert!(validate_intent(&document, "app", module, &invalid_exception).is_err());
    Ok(())
}

#[test]
fn source_scan_rejects_sensitive_and_escaping_links_but_honors_exact_exceptions()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temporary, mut document) = fixture()?;
    let source = document.root.join("app/src");
    fs::write(source.join(".env"), "SECRET=value")?;
    assert!(scan_source(&document, "app", &source, &mount("src")).is_err());

    let mut excepted = mount("src");
    excepted.safe_exceptions = vec![PathBuf::from(".env")];
    scan_source(&document, "app", &source, &excepted)?;

    fs::remove_file(source.join(".env"))?;
    document
        .config
        .development_mounts
        .sensitive_patterns
        .push("private/*".to_owned());
    fs::create_dir(source.join("private"))?;
    fs::write(source.join("private/token"), "token")?;
    assert!(scan_source(&document, "app", &source, &mount("src")).is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        fs::remove_dir_all(source.join("private"))?;
        fs::write(document.root.join("outside"), "outside")?;
        symlink("../../outside", source.join("escape"))?;
        assert!(scan_source(&document, "app", &source, &mount("src")).is_err());
        fs::remove_file(source.join("escape"))?;
        symlink("missing", source.join("broken"))?;
        assert!(scan_source(&document, "app", &source, &mount("src")).is_err());
    }

    assert!(
        scan_source(
            &document,
            "app",
            &document.root.join("does-not-exist"),
            &mount("src")
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn scan_limit_mount_conflicts_and_active_environment_gating_are_exact()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        checked_scan_count("app", MAX_SCANNED_ENTRIES - 1)?,
        MAX_SCANNED_ENTRIES
    );
    assert!(checked_scan_count("app", MAX_SCANNED_ENTRIES).is_err());
    assert!(checked_scan_count("app", usize::MAX).is_err());

    let (_temporary, mut document) = fixture()?;
    assert!(active_mounts(&document, "production", true)?.is_empty());
    assert!(active_mounts(&document, "development", false)?.is_empty());

    let configured = mount("src");
    let module = document.config.modules.get_mut("app");
    assert!(module.is_some());
    if let Some(module) = module {
        module.development_mount = Some(configured.clone());
    }
    let mounts = active_mounts(&document, "development", true)?;
    assert_eq!(mounts.get("app"), Some(&configured));

    let mut second = document.config.modules["app"].clone();
    second.path = PathBuf::from("second");
    document.config.modules.insert("second".to_owned(), second);
    let conflicts = BTreeMap::from([
        ("app".to_owned(), configured.clone()),
        ("second".to_owned(), configured),
    ]);
    assert!(reject_mount_conflicts(&document, &conflicts).is_err());
    assert!(
        reject_mount_conflicts(
            &document,
            &BTreeMap::from([("app".to_owned(), mount("src"))]),
        )
        .is_ok()
    );

    assert!(read_bounded(&document.root.join("missing")).is_err());
    Ok(())
}
