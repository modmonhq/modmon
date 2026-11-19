//! Split configuration files: any top-level key may live in
//! `.config/mm/<key>.yaml` instead of `.config/mm.yaml`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm::adapters::config::ConfigLoader;
use mm::app::OnboardingService;
use mm::app::ports::ProjectDocumentStore;
use mm::domain::config::SPLITTABLE_CONFIG_KEYS;
use mm::domain::environment_delivery::EnvironmentDeliveryMode;
use mm::domain::error::MmError;
use mm::domain::onboarding::{
    ConfigLayoutAction, ConfigLayoutRequest, ConfigMutationAction, ConfigMutationRequest,
    ConfigResourceKind,
};
use tempfile::TempDir;

mod temp_dirs;

const HEADER: &str = "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: split-demo
target: local
";

const STACKS: &str = "app:
  files: [compose.yaml] # main stack
";

const MODULES: &str = "api:
  path: api
  services:
    api:
      stack: app
";

const ENVIRONMENTS: &str = "local:
  bindings: {}
staging:
  bindings: {}
";

fn indent(text: &str) -> String {
    text.lines().map(|line| format!("  {line}\n")).collect()
}

fn unsplit() -> String {
    format!(
        "{HEADER}# Compose stacks\nstacks:\n{}modules:\n{}environments:\n{}",
        indent(STACKS),
        indent(MODULES),
        indent(ENVIRONMENTS)
    )
}

fn project(files: &[(&str, &str)]) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    for (relative, text) in files {
        let path = temporary.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
    }
    Ok(temporary)
}

fn split_project() -> Result<TempDir, Box<dyn std::error::Error>> {
    project(&[
        (
            ".config/mm.yaml",
            &format!("{HEADER}modules:\n{}", indent(MODULES)),
        ),
        (".config/mm/stacks.yaml", STACKS),
        (".config/mm/environments.yaml", ENVIRONMENTS),
    ])
}

fn parse_error(root: &Path) -> Result<(PathBuf, String), Box<dyn std::error::Error>> {
    match ConfigLoader.load_from(root) {
        Err(MmError::ConfigParse { path, message }) => Ok((path, message)),
        other => Err(format!("expected a configuration parse error, got {other:?}").into()),
    }
}

#[test]
fn split_files_assemble_into_the_same_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let whole = project(&[(".config/mm.yaml", &unsplit())])?;
    let whole_document = ConfigLoader.load_from(whole.path())?;
    // Unsplit projects keep the historical hash, so existing plans stay valid.
    assert_eq!(
        whole_document.source_hash,
        blake3::hash(unsplit().as_bytes()).to_hex().to_string()
    );
    assert_eq!(whole_document.split_sources().count(), 0);
    assert_eq!(whole_document.source("stacks"), whole_document.path);

    let split = split_project()?;
    fs::write(
        split.path().join(".config/mm/.stacks.yaml.swp"),
        "binary junk",
    )?;
    fs::write(split.path().join(".config/mm/stacks.yaml~"), "backup: [")?;
    let document = ConfigLoader.load_from(split.path())?;
    assert_eq!(document.config, whole_document.config);
    assert!(
        document
            .source("stacks")
            .ends_with(".config/mm/stacks.yaml")
    );
    assert!(
        document
            .source("environments")
            .ends_with(".config/mm/environments.yaml")
    );
    assert_eq!(document.source("modules"), document.path);
    assert_eq!(document.source("jobs"), document.path);
    assert_eq!(
        document
            .split_sources()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        ["environments", "stacks"]
    );
    assert_ne!(document.source_hash, whole_document.source_hash);

    fs::write(
        split.path().join(".config/mm/stacks.yaml"),
        "app:\n  files: [other.yaml]\n",
    )?;
    assert_ne!(
        ConfigLoader.load_from(split.path())?.source_hash,
        document.source_hash
    );
    Ok(())
}

#[test]
fn split_file_errors_name_the_file_and_line() -> Result<(), Box<dyn std::error::Error>> {
    let duplicate = split_project()?;
    fs::write(
        duplicate.path().join(".config/mm.yaml"),
        format!("{HEADER}modules:\n{}stacks: {{}}\n", indent(MODULES)),
    )?;
    let (path, message) = parse_error(duplicate.path())?;
    assert!(path.ends_with(".config/mm/stacks.yaml"));
    assert!(message.contains("set in both"), "{message}");

    for (name, expected) in [
        ("stacks.yml", "use the .yaml extension"),
        ("schema-version.yaml", "must stay in .config/mm.yaml"),
        ("notes.txt", "unexpected file"),
    ] {
        let project = split_project()?;
        fs::write(project.path().join(".config/mm").join(name), "{}\n")?;
        let (path, message) = parse_error(project.path())?;
        assert!(path.ends_with(name), "{path:?}");
        assert!(message.contains(expected), "{name}: {message}");
        assert!(message.contains("environment-delivery"), "{message}");
    }

    let directory = split_project()?;
    fs::create_dir(directory.path().join(".config/mm/modules"))?;
    assert!(
        parse_error(directory.path())?
            .1
            .contains("unexpected entry")
    );

    let empty = split_project()?;
    fs::write(
        empty.path().join(".config/mm/stacks.yaml"),
        "# nothing yet\n",
    )?;
    let (path, message) = parse_error(empty.path())?;
    assert!(path.ends_with("stacks.yaml"));
    assert!(message.contains("empty"), "{message}");

    let wrong_type = split_project()?;
    fs::write(
        wrong_type.path().join(".config/mm/environments.yaml"),
        "local:\n  bindings: {}\nstaging:\n  bindings: [not, a, map]\n",
    )?;
    let (path, message) = parse_error(wrong_type.path())?;
    assert!(path.ends_with("environments.yaml"));
    assert!(message.contains("line 4"), "{message}");

    let anchor_type = split_project()?;
    fs::write(
        anchor_type.path().join(".config/mm.yaml"),
        format!(
            "{HEADER}runtime:\n  driver: [nope]\nmodules:\n{}",
            indent(MODULES)
        ),
    )?;
    let (path, message) = parse_error(anchor_type.path())?;
    assert!(path.ends_with(".config/mm.yaml"));
    assert!(message.contains("line 7"), "{message}");

    let unknown = split_project()?;
    fs::write(
        unknown.path().join(".config/mm.yaml"),
        format!("{HEADER}compose: {{}}\nmodules:\n{}", indent(MODULES)),
    )?;
    let ignored = ConfigLoader.load_from(unknown.path())?.unknown_keys;
    assert_eq!(ignored.len(), 1, "{ignored:?}");
    assert_eq!(
        (ignored[0].file.as_str(), ignored[0].key.as_str()),
        (".config/mm.yaml", "compose")
    );

    let missing = split_project()?;
    fs::remove_file(missing.path().join(".config/mm/stacks.yaml"))?;
    let (path, message) = parse_error(missing.path())?;
    assert!(path.ends_with(".config/mm.yaml"));
    assert!(message.contains("stacks"), "{message}");

    let file = project(&[
        (".config/mm.yaml", &unsplit()),
        (".config/mm", "not a directory"),
    ])?;
    assert!(parse_error(file.path())?.1.contains("must be a directory"));

    #[cfg(unix)]
    {
        let linked = split_project()?;
        let target = linked.path().join("elsewhere.yaml");
        fs::rename(linked.path().join(".config/mm/stacks.yaml"), &target)?;
        std::os::unix::fs::symlink(&target, linked.path().join(".config/mm/stacks.yaml"))?;
        assert!(parse_error(linked.path())?.1.contains("cannot be symlinks"));
    }

    let oversized = split_project()?;
    fs::write(
        oversized.path().join(".config/mm/stacks.yaml"),
        format!(
            "app:\n  files: [compose.yaml]\n#{}\n",
            "x".repeat(1024 * 1024)
        ),
    )?;
    assert!(ConfigLoader.load_from(oversized.path()).is_err());
    Ok(())
}

#[test]
fn local_overrides_apply_on_top_of_split_files() -> Result<(), Box<dyn std::error::Error>> {
    let split = split_project()?;
    let before = ConfigLoader.load_from(split.path())?;
    fs::write(
        split.path().join(".config/mm.overrides.yaml"),
        "build: {platforms: [linux/arm64]}\n",
    )?;
    let document = ConfigLoader.load_from(split.path())?;
    assert_eq!(document.config.build.platforms, ["linux/arm64"]);
    assert_eq!(document.config.stacks, before.config.stacks);
    assert_ne!(document.source_hash, before.source_hash);

    fs::write(
        split.path().join(".config/mm.overrides.yaml"),
        "environments: {local: {bindings: [wrong]}}\n",
    )?;
    let (path, message) = parse_error(split.path())?;
    assert!(path.ends_with("mm.overrides.yaml"));
    assert!(message.contains("local overrides"), "{message}");
    Ok(())
}

#[test]
fn edits_write_to_the_file_that_owns_the_key() -> Result<(), Box<dyn std::error::Error>> {
    let split = split_project()?;
    let anchor = split.path().join(".config/mm.yaml");
    let anchor_text = fs::read_to_string(&anchor)?;

    let delivered = ConfigLoader.set_environment_delivery(
        split.path(),
        "staging",
        EnvironmentDeliveryMode::Direct,
    )?;
    assert_eq!(
        delivered.config.environments["staging"].environment_delivery,
        Some(EnvironmentDeliveryMode::Direct)
    );
    assert!(
        fs::read_to_string(split.path().join(".config/mm/environments.yaml"))?
            .contains("environment-delivery: direct")
    );
    assert_eq!(fs::read_to_string(&anchor)?, anchor_text);

    fs::write(
        split.path().join(".config/mm/default-environment.yaml"),
        "local\n",
    )?;
    let cleared = ConfigLoader.set_default_environment(split.path(), None)?;
    assert!(cleared.config.default_environment.is_none());
    let selected = ConfigLoader.set_default_environment(split.path(), Some("staging"))?;
    assert_eq!(
        selected.config.default_environment.as_deref(),
        Some("staging")
    );
    assert_eq!(
        fs::read_to_string(split.path().join(".config/mm/default-environment.yaml"))?,
        "staging\n"
    );
    assert_eq!(fs::read_to_string(&anchor)?, anchor_text);

    // The guided editor drops the first path segment inside a split file.
    fs::write(
        split.path().join(".config/mm/jobs.yaml"),
        "# Finite jobs\n{}\n",
    )?;
    let service = OnboardingService::new(Arc::new(ConfigLoader));
    let request = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Job,
        name: "migrate".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({
            "module": "api",
            "service": "api",
            "command": ["migrate"],
        })),
    };
    let plan = service.plan_mutation(split.path(), request.clone())?;
    assert_eq!(plan.affected_files.len(), 1);
    assert!(plan.affected_files[0].ends_with(".config/mm/jobs.yaml"));
    assert!(
        plan.canonical_diff.contains("+++ b/.config/mm/jobs.yaml"),
        "{}",
        plan.canonical_diff
    );
    let result = service.apply_mutation(split.path(), request, &plan.plan_hash)?;
    assert!(result.config_path.ends_with(".config/mm/jobs.yaml"));
    let jobs = fs::read_to_string(split.path().join(".config/mm/jobs.yaml"))?;
    assert!(jobs.starts_with("# Finite jobs\nmigrate:\n  "), "{jobs}");
    assert!(
        ConfigLoader
            .load_from(split.path())?
            .config
            .jobs
            .contains_key("migrate")
    );
    assert_eq!(fs::read_to_string(&anchor)?, anchor_text);

    let remove = ConfigMutationRequest {
        action: ConfigMutationAction::Remove,
        resource: ConfigResourceKind::Job,
        name: "migrate".to_owned(),
        parent: None,
        definition: None,
    };
    let plan = service.plan_mutation(split.path(), remove.clone())?;
    service.apply_mutation(split.path(), remove, &plan.plan_hash)?;
    assert!(ConfigLoader.load_from(split.path())?.config.jobs.is_empty());
    assert_eq!(
        fs::read_to_string(split.path().join(".config/mm/jobs.yaml"))?,
        "# Finite jobs\n{}\n"
    );
    Ok(())
}

#[test]
fn split_and_join_move_a_key_without_changing_its_meaning() -> Result<(), Box<dyn std::error::Error>>
{
    let root = project(&[(".config/mm.yaml", &unsplit())])?;
    let original = ConfigLoader.load_from(root.path())?;
    let service = OnboardingService::new(Arc::new(ConfigLoader));
    let split = ConfigLayoutRequest {
        action: ConfigLayoutAction::Split,
        key: "stacks".to_owned(),
    };
    let plan = service.plan_layout(root.path(), split.clone())?;
    assert_eq!(plan.affected_files.len(), 2);
    assert!(plan.affected_files[1].ends_with(".config/mm/stacks.yaml"));
    assert!(
        plan.canonical_diff
            .contains(".config/mm/stacks.yaml (new file)")
    );
    assert!(plan.notes.is_empty(), "{:?}", plan.notes);
    assert!(
        service
            .apply_layout(root.path(), split.clone(), "stale")
            .is_err()
    );
    let result = service.apply_layout(root.path(), split.clone(), &plan.plan_hash)?;
    assert!(result.config_path.ends_with(".config/mm/stacks.yaml"));

    let stacks = fs::read_to_string(root.path().join(".config/mm/stacks.yaml"))?;
    assert_eq!(
        stacks,
        format!("# Compose stacks\n{STACKS}"),
        "comments and layout move with the value"
    );
    let anchor = fs::read_to_string(root.path().join(".config/mm.yaml"))?;
    assert!(!anchor.contains("stacks:"));
    assert!(anchor.contains("target: local\nmodules:\n"), "{anchor}");
    let document = ConfigLoader.load_from(root.path())?;
    assert_eq!(document.config, original.config);
    assert!(
        document
            .source("stacks")
            .ends_with(".config/mm/stacks.yaml")
    );

    assert!(
        service.plan_layout(root.path(), split).is_err(),
        "already split"
    );
    for key in ["schema-version", "jobs", "compose"] {
        let request = ConfigLayoutRequest {
            action: ConfigLayoutAction::Split,
            key: key.to_owned(),
        };
        assert!(service.plan_layout(root.path(), request).is_err(), "{key}");
    }

    // A reviewed plan never applies over a file that changed after review.
    let environments = ConfigLayoutRequest {
        action: ConfigLayoutAction::Split,
        key: "environments".to_owned(),
    };
    let stale = service.plan_layout(root.path(), environments.clone())?;
    fs::write(
        root.path().join(".config/mm.yaml"),
        format!("{anchor}# edited after review\n"),
    )?;
    assert!(
        service
            .apply_layout(root.path(), environments, &stale.plan_hash)
            .is_err()
    );
    assert!(!root.path().join(".config/mm/environments.yaml").exists());

    let join = ConfigLayoutRequest {
        action: ConfigLayoutAction::Join,
        key: "stacks".to_owned(),
    };
    let plan = service.plan_layout(root.path(), join.clone())?;
    assert!(
        plan.canonical_diff
            .contains(".config/mm/stacks.yaml (removed)")
    );
    service.apply_layout(root.path(), join.clone(), &plan.plan_hash)?;
    assert!(!root.path().join(".config/mm").exists());
    let joined = fs::read_to_string(root.path().join(".config/mm.yaml"))?;
    assert!(
        joined.ends_with(&format!("stacks:\n  # Compose stacks\n{}", indent(STACKS))),
        "{joined}"
    );
    assert_eq!(ConfigLoader.load_from(root.path())?.config, original.config);
    assert!(service.plan_layout(root.path(), join).is_err(), "not split");

    // Scalars and flow values move as one line.
    let target = ConfigLayoutRequest {
        action: ConfigLayoutAction::Split,
        key: "target".to_owned(),
    };
    let plan = service.plan_layout(root.path(), target.clone())?;
    service.apply_layout(root.path(), target, &plan.plan_hash)?;
    assert_eq!(
        fs::read_to_string(root.path().join(".config/mm/target.yaml"))?,
        "local\n"
    );
    let target = ConfigLayoutRequest {
        action: ConfigLayoutAction::Join,
        key: "target".to_owned(),
    };
    let plan = service.plan_layout(root.path(), target.clone())?;
    service.apply_layout(root.path(), target, &plan.plan_hash)?;
    assert!(fs::read_to_string(root.path().join(".config/mm.yaml"))?.contains("\ntarget: local\n"));
    assert_eq!(ConfigLoader.load_from(root.path())?.config, original.config);
    Ok(())
}

#[test]
fn ignored_split_files_are_reported() -> Result<(), Box<dyn std::error::Error>> {
    let split = split_project()?;
    fs::create_dir(split.path().join(".git"))?;
    let document = ConfigLoader.load_from(split.path())?;
    assert!(ConfigLoader.ignored_sources(&document).is_empty());

    fs::write(split.path().join(".gitignore"), "/.config/mm/stacks.yaml\n")?;
    let ignored = ConfigLoader.ignored_sources(&document);
    assert_eq!(ignored.len(), 1);
    assert!(ignored[0].ends_with(".config/mm/stacks.yaml"));

    fs::write(split.path().join(".gitignore"), ".config/\n")?;
    assert_eq!(ConfigLoader.ignored_sources(&document).len(), 2);
    Ok(())
}

#[test]
fn every_top_level_key_has_a_published_split_schema() -> Result<(), Box<dyn std::error::Error>> {
    let full = serde_json::to_value(ConfigLoader.schema())?;
    let mut properties = full["properties"]
        .as_object()
        .ok_or("schema has no properties")?
        .keys()
        .filter(|key| *key != "schema-version")
        .cloned()
        .collect::<Vec<_>>();
    properties.sort();
    let mut splittable = SPLITTABLE_CONFIG_KEYS
        .iter()
        .map(|key| (*key).to_owned())
        .collect::<Vec<_>>();
    splittable.sort();
    assert_eq!(
        properties, splittable,
        "a new top-level key must be splittable"
    );

    let published = Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/public/schema/mm");
    for key in SPLITTABLE_CONFIG_KEYS {
        let schema = ConfigLoader.key_schema(key).ok_or("missing key schema")?;
        let mut generated = serde_json::to_string_pretty(&schema)?;
        generated.push('\n');
        let path = published.join(format!("{key}.schema.json"));
        assert_eq!(
            generated.as_bytes(),
            fs::read(&path)?,
            "{} is stale; run ./scripts/generate-artifacts.sh",
            path.display()
        );
    }
    assert!(ConfigLoader.key_schema("schema-version").is_none());
    assert!(ConfigLoader.key_schema("compose").is_none());
    Ok(())
}
