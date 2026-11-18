//! `secret-files` and `config-files` entries that name the environments
//! they apply in.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use mm::adapters::config::{ConfigLoader, validate};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::TargetStore;
use mm::app::{StackManager, WorkspaceSelection};
use mm::domain::target::{TargetDefinition, TargetId};
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A project whose service `web` has `files` as its own keys, in the
/// environments `staging` and `production`.
fn project(files: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path();
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    fs::write(root.join("app/.env.example"), "")?;
    fs::write(root.join("app/motd"), "staging only\n")?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: files\ndefault-environment: staging\ntarget: local\nruntime:\n  driver: docker-compose\nstacks:\n  app:\n    files: [compose.yaml]\nmodules:\n  app:\n    path: app\n    env:\n      template: .env.example\n    services:\n      web:\n        stack: app\n{files}secrets:\n  staging.token:\n    storage: {{provider: age, location: local/staging-token}}\nenvironments:\n  staging:\n    bindings: {{}}\n  production:\n    bindings: {{}}\n"
        ),
    )?;
    Ok(temporary)
}

fn compiled_overlay(
    root: &Path,
    environment: &str,
) -> Result<serde_yaml_ng::Value, Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        store,
    );
    let report = manager.compile(&WorkspaceSelection::new(root, Some(environment.to_owned())))?;
    let overlay = fs::read_to_string(&report.stacks[0].generated_overlay_path)?;
    Ok(serde_yaml_ng::from_str(&overlay)?)
}

#[test]
fn a_file_with_environments_is_mounted_only_there() -> TestResult {
    let project = project(
        "        config-files:\n          /etc/motd: {file: motd, environments: [staging]}\n        secret-files:\n          staging-token: {secret: staging.token, environments: [staging]}\n",
    )?;
    let document = ConfigLoader.load_from(project.path())?;
    let web = &document.config.modules["app"].services["web"];
    assert_eq!(web.secret_files_in("staging").count(), 1);
    assert_eq!(web.secret_files_in("production").count(), 0);
    assert_eq!(web.config_files_in("staging").count(), 1);
    assert_eq!(web.config_files_in("production").count(), 0);
    for environment in ["staging", "production"] {
        let report = validate(&document, environment);
        assert!(report.is_valid(), "{environment}: {:?}", report.diagnostics);
    }

    let mounts_motd = |overlay: &serde_yaml_ng::Value| {
        overlay["services"]["web"]["volumes"]
            .as_sequence()
            .is_some_and(|volumes| {
                volumes
                    .iter()
                    .filter_map(serde_yaml_ng::Value::as_str)
                    .any(|volume| volume.contains(":/etc/motd:"))
            })
    };
    assert!(mounts_motd(&compiled_overlay(project.path(), "staging")?));
    assert!(!mounts_motd(&compiled_overlay(
        project.path(),
        "production"
    )?));
    Ok(())
}

#[test]
fn a_file_environment_that_is_not_declared_is_refused() -> TestResult {
    let project = project(
        "        secret-files:\n          staging-token: {secret: staging.token, environments: [stagign]}\n",
    )?;
    let document = ConfigLoader.load_from(project.path())?;
    let report = validate(&document, "staging");
    let message = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "MM_SERVICE_ENVIRONMENT_UNKNOWN")
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_default();
    for expected in [
        "modules.app.services.web.secret-files.\"staging-token\".environments names 'stagign'",
        "declare it or remove it from the list",
    ] {
        assert!(message.contains(expected), "{expected}: {message}");
    }
    assert!(!report.is_valid());
    Ok(())
}
