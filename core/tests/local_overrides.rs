use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use mm::adapters::build::GitBuildSourceInspector;
use mm::adapters::config::{CONFIG_RELATIVE_PATH, ConfigLoader, OVERRIDES_RELATIVE_PATH};
use mm::adapters::process::TokioProcessRunner;
use mm::app::{BuildManager, OnboardingService, WorkspaceSelection};
use mm::domain::config::MmConfig;
use mm::domain::onboarding::{ConfigMutationAction, ConfigMutationRequest, ConfigResourceKind};
use tempfile::TempDir;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    fs::write(
        project.path().join("compose.yaml"),
        "services: {web: {image: example/web}}\n",
    )?;
    fs::create_dir(project.path().join("app"))?;
    fs::write(project.path().join("app/Dockerfile"), "FROM scratch\n")?;
    fs::write(
        project.path().join("app/.env.example"),
        "COUNT=7\nDEBUG=true\n",
    )?;
    fs::write(project.path().join("app/.dockerignore"), ".env*\n")?;
    let initialized = ConfigLoader.initialize(project.path(), Some("overrides-test"))?;
    let mut config = initialized.document.config;
    config.build.platforms = vec!["linux/amd64".to_owned()];
    config.modules.insert("app".to_owned(), serde_yaml_ng::from_str(
        "path: app\nenv: {template: .env.example}\nbuild: {image: example/web}\nservices:\n  web:\n    stack: app\n    env: {COUNT: 7, DEBUG: true}\n",
    )?);
    fs::write(
        project.path().join(CONFIG_RELATIVE_PATH),
        format!(
            "# shared configuration\n{}",
            serde_yaml_ng::to_string(&config)?
        ),
    )?;
    Ok(project)
}

#[test]
fn local_overrides_merge_maps_replace_lists_and_preserve_explicit_values() -> TestResult {
    let project = project()?;
    let root = project.path();
    let path = root.join(CONFIG_RELATIVE_PATH);
    let source = fs::read(&path)?;
    let base = ConfigLoader.load(&path)?;
    fs::write(
        root.join(OVERRIDES_RELATIVE_PATH),
        "build:\n  platforms: [linux/arm64]\ndefault-environment: null\nmodules:\n  app:\n    services:\n      web:\n        env: {COUNT: 0, DEBUG: false}\n",
    )?;
    let effective = ConfigLoader.load(&path)?;
    assert_eq!(effective.config.build.platforms, ["linux/arm64"]);
    assert_eq!(
        effective.config.build.tag_strategy,
        base.config.build.tag_strategy
    );
    assert!(effective.config.default_environment.is_none());
    assert_eq!(effective.config.modules["app"].path, Path::new("app"));
    assert_eq!(
        effective.config.modules["app"].services["web"].env["COUNT"].to_string(),
        "0"
    );
    assert_eq!(
        effective.config.modules["app"].services["web"].env["DEBUG"].to_string(),
        "false"
    );
    assert_ne!(effective.source_hash, base.source_hash);
    assert_eq!(fs::read(&path)?, source);
    fs::create_dir_all(root.join("nested/deeper"))?;
    assert_eq!(
        ConfigLoader.load_from(&root.join("nested/deeper"))?,
        effective
    );
    assert_eq!(
        ConfigLoader.load_from(&root.join("app/Dockerfile"))?,
        effective
    );
    fs::write(root.join(OVERRIDES_RELATIVE_PATH), "{}\n")?;
    let empty = ConfigLoader.load(&path)?;
    assert_eq!(empty.config, base.config);
    assert_ne!(empty.source_hash, effective.source_hash);
    fs::remove_file(root.join(OVERRIDES_RELATIVE_PATH))?;
    assert_eq!(ConfigLoader.load(&path)?, base);
    Ok(())
}

#[test]
fn malformed_overrides_are_errors_and_cannot_repair_an_invalid_shared_document() -> TestResult {
    let project = project()?;
    let path = project.path().join(OVERRIDES_RELATIVE_PATH);
    for source in [
        "[]",
        "null",
        "",
        "build: [",
        "build: {platform: [linux/arm64]}",
        "target: null",
        "schema-version: 2",
        "build: {platforms: wrong}",
        "build: {}\nbuild: {}",
    ] {
        fs::write(&path, source)?;
        let error = ConfigLoader
            .load_from(project.path())
            .err()
            .ok_or("invalid override accepted")?;
        assert!(error.to_string().contains("mm.overrides.yaml"), "{error}");
    }
    fs::write(&path, [0xff])?;
    assert!(ConfigLoader.load_from(project.path()).is_err());
    fs::write(&path, vec![b' '; 1024 * 1024 + 1])?;
    assert!(ConfigLoader.load_from(project.path()).is_err());
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(ConfigLoader.load_from(project.path()).is_err());
    fs::remove_dir(&path)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("missing.yaml", &path)?;
        assert!(ConfigLoader.load_from(project.path()).is_err());
        fs::remove_file(&path)?;
    }
    fs::write(
        project.path().join(CONFIG_RELATIVE_PATH),
        "schema-version: 1\n",
    )?;
    fs::write(&path, "project: {name: rescued}\n")?;
    assert!(ConfigLoader.load_from(project.path()).is_err());
    Ok(())
}

#[test]
fn shared_editors_bind_the_effective_review_but_only_write_shared_yaml() -> TestResult {
    let project = project()?;
    let root = project.path();
    let path = root.join(OVERRIDES_RELATIVE_PATH);
    let override_source = "build: {platforms: [linux/arm64]}\n";
    fs::write(&path, override_source)?;
    let editor = OnboardingService::new(Arc::new(ConfigLoader));
    let request = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Module,
        name: "other".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({"path":"app","services":{}})),
    };
    let stale = editor.plan_mutation(root, request.clone())?;
    fs::write(
        &path,
        "build: {platforms: [linux/arm64]}\n# changed after review\n",
    )?;
    assert!(
        editor
            .apply_mutation(root, request.clone(), &stale.plan_hash)
            .is_err()
    );
    let plan = editor.plan_mutation(root, request.clone())?;
    editor.apply_mutation(root, request, &plan.plan_hash)?;
    ConfigLoader.set_default_environment(root, Some("local"))?;
    ConfigLoader.set_environment_delivery(
        root,
        "local",
        mm::domain::environment_delivery::EnvironmentDeliveryMode::Direct,
    )?;
    let shared = fs::read_to_string(root.join(CONFIG_RELATIVE_PATH))?;
    assert!(shared.starts_with("# shared configuration"));
    assert!(!shared.contains("linux/arm64"));
    let config: MmConfig = serde_yaml_ng::from_str(&shared)?;
    assert_eq!(config.build.platforms, ["linux/amd64"]);
    assert!(config.modules.contains_key("other"));
    assert_eq!(
        fs::read_to_string(path)?,
        format!("{override_source}# changed after review\n")
    );
    Ok(())
}

#[test]
fn init_reviews_preserves_and_rolls_back_gitignore_changes() -> TestResult {
    for before in [
        None,
        Some(""),
        Some("# custom\nnode_modules"),
        Some("# custom\r\nnode_modules\r\n"),
        Some("/.config/mm.overrides.yaml\n!/.config/mm.overrides.yaml\n"),
    ] {
        let root = temp_dirs::tempdir()?;
        let path = root.path().join(".gitignore");
        if let Some(before) = before {
            fs::write(&path, before)?;
        }
        let service = OnboardingService::new(Arc::new(ConfigLoader));
        let plan = service.plan_initialization(root.path(), None)?;
        assert!(plan.affected_files.contains(&path));
        assert!(plan.canonical_diff.contains("+/.config/mm.overrides.yaml"));
        let initialized = ConfigLoader.initialize_reviewed(root.path(), None, &plan.plan_hash)?;
        let after = fs::read_to_string(&path)?;
        assert!(after.starts_with(before.unwrap_or("")));
        assert!(after.contains("/.config/mm.overrides.yaml"));
        if before.is_some_and(|before| before.contains("\r\n")) {
            assert!(after.ends_with(".yaml\r\n"));
        }
        assert!(initialized.starter_files.contains(&path));
        initialized.rollback()?;
        assert!(!root.path().join(CONFIG_RELATIVE_PATH).exists());
        match before {
            Some(before) => assert_eq!(fs::read_to_string(&path)?, before),
            None => assert!(!path.exists()),
        }
    }
    Ok(())
}

#[test]
fn init_is_a_noop_for_existing_ignore_rules_and_rejects_stale_reviews() -> TestResult {
    for source in [
        "/.config/mm.overrides.yaml\n",
        ".config/\n",
        "*.overrides.yaml\n",
    ] {
        let root = temp_dirs::tempdir()?;
        let path = root.path().join(".gitignore");
        fs::write(&path, source)?;
        let service = OnboardingService::new(Arc::new(ConfigLoader));
        let plan = service.plan_initialization(root.path(), None)?;
        assert!(!plan.affected_files.contains(&path));
        ConfigLoader.initialize_reviewed(root.path(), None, &plan.plan_hash)?;
        assert_eq!(fs::read_to_string(&path)?, source);
        assert!(service.plan_initialization(root.path(), None).is_err());
    }
    for before in [
        None,
        Some("# existing\n"),
        Some("/.config/mm.overrides.yaml\n"),
    ] {
        let root = temp_dirs::tempdir()?;
        let path = root.path().join(".gitignore");
        if let Some(before) = before {
            fs::write(&path, before)?;
        }
        let service = OnboardingService::new(Arc::new(ConfigLoader));
        let plan = service.plan_initialization(root.path(), None)?;
        fs::write(&path, "# concurrent edit\n")?;
        assert!(
            ConfigLoader
                .initialize_reviewed(root.path(), None, &plan.plan_hash)
                .is_err()
        );
        assert!(!root.path().join(CONFIG_RELATIVE_PATH).exists());
        assert_eq!(fs::read_to_string(path)?, "# concurrent edit\n");
    }
    let root = temp_dirs::tempdir()?;
    let initialized = ConfigLoader.initialize(root.path(), None)?;
    fs::write(root.path().join(".gitignore"), "# newer contents\n")?;
    assert!(initialized.rollback().is_err());
    assert_eq!(
        fs::read_to_string(root.path().join(".gitignore"))?,
        "# newer contents\n"
    );
    Ok(())
}

#[test]
fn initialization_binds_preexisting_overrides_without_overwriting_them() -> TestResult {
    let project = temp_dirs::tempdir()?;
    fs::create_dir(project.path().join(".config"))?;
    let path = project.path().join(OVERRIDES_RELATIVE_PATH);
    fs::write(&path, "build: {platforms: [linux/arm64]}\n")?;
    let service = OnboardingService::new(Arc::new(ConfigLoader));
    let stale = service.plan_initialization(project.path(), None)?;
    fs::write(&path, "build: {platforms: [linux/amd64]}\n")?;
    assert!(
        ConfigLoader
            .initialize_reviewed(project.path(), None, &stale.plan_hash)
            .is_err()
    );
    assert!(!project.path().join(CONFIG_RELATIVE_PATH).exists());
    let plan = service.plan_initialization(project.path(), None)?;
    let initialized = ConfigLoader.initialize_reviewed(project.path(), None, &plan.plan_hash)?;
    assert_eq!(initialized.document.config.build.platforms, ["linux/amd64"]);
    initialized.rollback()?;
    assert_eq!(
        fs::read_to_string(path)?,
        "build: {platforms: [linux/amd64]}\n"
    );
    Ok(())
}

#[test]
fn init_rejects_nested_unignore_rules_and_unsafe_ignore_files_before_writing() -> TestResult {
    let root = temp_dirs::tempdir()?;
    fs::create_dir(root.path().join(".config"))?;
    fs::write(
        root.path().join(".config/.gitignore"),
        "!mm.overrides.yaml\n",
    )?;
    assert!(ConfigLoader.initialize(root.path(), None).is_err());
    assert!(!root.path().join(CONFIG_RELATIVE_PATH).exists());
    assert!(!root.path().join(".gitignore").exists());
    fs::remove_file(root.path().join(".config/.gitignore"))?;
    fs::create_dir(root.path().join(".gitignore"))?;
    assert!(ConfigLoader.initialize(root.path(), None).is_err());
    fs::remove_dir(root.path().join(".gitignore"))?;
    #[cfg(unix)]
    {
        fs::write(root.path().join("real-ignore"), "# operator file\n")?;
        std::os::unix::fs::symlink("real-ignore", root.path().join(".gitignore"))?;
        assert!(ConfigLoader.initialize(root.path(), None).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("real-ignore"))?,
            "# operator file\n"
        );
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git").current_dir(root).args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

#[tokio::test]
async fn ignored_arm64_override_allows_publishing_but_other_dirty_changes_do_not() -> TestResult {
    let project = project()?;
    let root = project.path();
    git(root, &["init", "--initial-branch=main"])?;
    git(root, &["add", "."])?;
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture",
        ],
    )?;
    let original = fs::read(root.join(CONFIG_RELATIVE_PATH))?;
    fs::write(
        root.join(OVERRIDES_RELATIVE_PATH),
        "build: {platforms: [linux/arm64]}\n",
    )?;
    assert!(git(root, &["status", "--porcelain"])?.is_empty());
    assert!(!git(root, &["check-ignore", ".config/mm.overrides.yaml"])?.is_empty());
    let manager = BuildManager::new(
        Arc::new(ConfigLoader),
        Arc::new(GitBuildSourceInspector::new(Arc::new(TokioProcessRunner))),
    );
    let selection = WorkspaceSelection::new(root, Some("local".to_owned()));
    let plan = manager.plan(&selection, true).await?;
    assert_eq!(plan.platforms, ["linux/arm64"]);
    assert_eq!(fs::read(root.join(CONFIG_RELATIVE_PATH))?, original);
    for (path, contents) in [
        ("untracked.txt", b"untracked".as_slice()),
        (".config/mm.yaml", b"# dirty\n".as_slice()),
    ] {
        if path.ends_with("mm.yaml") {
            let mut bytes = original.clone();
            bytes.extend_from_slice(contents);
            fs::write(root.join(path), bytes)?;
        } else {
            fs::write(root.join(path), contents)?;
        }
        let error = manager
            .plan(&selection, true)
            .await
            .err()
            .ok_or("dirty publish accepted")?;
        assert!(error.to_string().contains("dirty worktree"));
        if path.ends_with("mm.yaml") {
            fs::write(root.join(path), &original)?;
        } else {
            fs::remove_file(root.join(path))?;
        }
    }
    fs::write(root.join(".gitignore"), "")?;
    git(root, &["add", ".gitignore"])?;
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "remove ignore rule",
        ],
    )?;
    assert!(manager.plan(&selection, true).await.is_err());
    Ok(())
}

#[test]
fn an_override_cannot_grant_agents_access_and_init_scaffolds_a_safe_block() -> TestResult {
    let project = project()?;
    let document = ConfigLoader.load_from(project.path())?;
    // `mm init` opens the development environment to agents and nothing else.
    let agents = serde_yaml_ng::to_string(&document.config.agents)?;
    assert_eq!(agents, "'*': view\nlocal:\n  level: deploy\n  exec: true\n");

    let path = project.path().join(OVERRIDES_RELATIVE_PATH);
    for source in ["agents: {local: deploy}", "agents: {}"] {
        fs::write(&path, source)?;
        let error = ConfigLoader
            .load_from(project.path())
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains("cannot set 'agents'"), "{source}: {error}");
    }
    fs::remove_file(&path)?;

    // An invalid block is a validation error that names the agents key.
    let config = project.path().join(CONFIG_RELATIVE_PATH);
    let source = fs::read_to_string(&config)?.replace("'*': view", "'*': admin\n  qa: view");
    fs::write(&config, source)?;
    let document = ConfigLoader.load_from(project.path())?;
    let report = mm::adapters::config::validate(&document, "local");
    let problems = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "MM_AGENTS_INVALID")
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(problems.contains("cannot be admin"), "{problems}");
    assert!(problems.contains("unknown environment 'qa'"), "{problems}");
    Ok(())
}
