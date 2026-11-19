//! A key this release does not read is ignored with a warning that names
//! the file and the key, so a project written for a newer mm still plans
//! with an older one. `mm config validate --strict` refuses it.

use std::fs;
use std::path::Path;

use mm::adapters::config::ConfigLoader;
use mm::domain::config::UnknownConfigKey;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
}

/// The minimal fixture, with `edit` applied to its `.config/mm.yaml`.
fn project(
    edit: impl Fn(String) -> String,
) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let root = temp_dirs::tempdir()?;
    fs::create_dir_all(root.path().join(".config"))?;
    fs::create_dir_all(root.path().join("api"))?;
    for file in ["compose.yaml", "api/.env.example"] {
        fs::copy(fixture_root().join(file), root.path().join(file))?;
    }
    let config = fs::read_to_string(fixture_root().join(".config/mm.yaml"))?;
    fs::write(root.path().join(".config/mm.yaml"), edit(config))?;
    Ok(root)
}

fn with_unknown_keys(config: String) -> String {
    config
        .replacen(
            "target: local\n",
            "target: local\nnewer-top-level: true\n",
            1,
        )
        .replacen(
            "        stack: app\n",
            "        stack: app\n        replicaz: 2\n",
            1,
        )
        .replacen(
            "        value: database\n",
            "        value: database\n        description: the database host\n",
            1,
        )
        .replacen(
            "      length: 32\n",
            "      length: 32\n      lenght: 40\n",
            1,
        )
        + "agents:\n  local:\n    level: view\n    exce: true\n"
}

fn described(keys: &[UnknownConfigKey]) -> Vec<(&str, &str, Option<&str>)> {
    keys.iter()
        .map(|key| {
            (
                key.file.as_str(),
                key.key.as_str(),
                key.suggestion.as_deref(),
            )
        })
        .collect()
}

#[test]
fn unknown_keys_load_with_their_file_path_and_a_near_miss() -> TestResult {
    let root = project(with_unknown_keys)?;
    let document = ConfigLoader.load_from(root.path())?;
    let mut found = described(&document.unknown_keys);
    found.sort();
    assert_eq!(
        found,
        [
            (".config/mm.yaml", "agents.local.exce", Some("exec")),
            (
                ".config/mm.yaml",
                r#"environments.local.bindings."DB.HOST".description"#,
                None
            ),
            (
                ".config/mm.yaml",
                "modules.api.services.api.replicaz",
                Some("replicas")
            ),
            (".config/mm.yaml", "newer-top-level", None),
            (
                ".config/mm.yaml",
                r#"secrets."database.password".generate.lenght"#,
                Some("length")
            ),
        ]
    );
    let services = &document.config.modules["api"].services;
    assert!(services["api"].replicas.is_none());
    let host = &document.config.environments["local"].bindings["DB.HOST"];
    assert_eq!(
        host.value().map(ToString::to_string).as_deref(),
        Some("database")
    );
    assert_eq!(
        document.unknown_keys[0].to_string(),
        "unknown key `newer-top-level` in .config/mm.yaml; this mm ignores it"
    );
    let agents = &document.config.agents["local"];
    assert!(agents.access().exec.services().is_none());
    Ok(())
}

#[test]
fn a_binding_still_sets_only_one_kind_of_value() -> TestResult {
    let root = project(|config| {
        config.replacen(
            "        value: database\n",
            "        value: database\n        secret: database.password\n",
            1,
        )
    })?;
    let error = ConfigLoader
        .load_from(root.path())
        .err()
        .ok_or("a binding with a value and a secret loaded")?;
    assert!(
        error.to_string().contains("a binding sets one of `value:`"),
        "{error}"
    );
    Ok(())
}

#[test]
fn split_files_and_local_overrides_name_their_own_file() -> TestResult {
    let root = project(|config| config)?;
    let anchor = root.path().join(".config/mm.yaml");
    let source = fs::read_to_string(&anchor)?;
    let (anchor_text, environments) = source
        .split_once("environments:\n")
        .ok_or("the fixture has no environments")?;
    fs::write(&anchor, anchor_text)?;
    let mut environments = environments
        .lines()
        .map(|line| format!("{}\n", line.strip_prefix("  ").unwrap_or(line)))
        .collect::<String>();
    environments.push_str("  domainz: {}\n");
    fs::create_dir_all(root.path().join(".config/mm"))?;
    fs::write(
        root.path().join(".config/mm/environments.yaml"),
        environments,
    )?;
    fs::write(root.path().join(".config/mm/stack.yaml"), "{}\n")?;
    fs::write(
        root.path().join(".config/mm.overrides.yaml"),
        "modules:\n  api:\n    services:\n      api:\n        replicaz: 3\ndevelopment:\n  enabled: []\n  later: true\n",
    )?;
    let document = ConfigLoader.load_from(root.path())?;
    let mut found = described(&document.unknown_keys);
    found.sort();
    assert_eq!(
        found,
        [
            (".config/mm.overrides.yaml", "development.later", None),
            (
                ".config/mm.overrides.yaml",
                "modules.api.services.api.replicaz",
                Some("replicas")
            ),
            (
                ".config/mm/environments.yaml",
                "environments.local.domainz",
                Some("domains")
            ),
            (".config/mm/stack.yaml", "stack", Some("stacks")),
        ]
    );
    Ok(())
}

#[test]
fn a_failure_names_the_unknown_key_that_explains_it() -> TestResult {
    let root = project(|config| config.replacen("    storage:\n", "    storag:\n", 1))?;
    let error = ConfigLoader
        .load_from(root.path())
        .err()
        .ok_or("a secret without storage loaded")?
        .to_string();
    assert!(error.contains("missing field `storage`"), "{error}");
    assert!(
        error.contains(
            "unknown key `secrets.\"database.password\".storag` (did you mean `storage`?)"
        ),
        "{error}"
    );
    Ok(())
}

fn mm_in(
    root: &Path,
    home: &Path,
    arguments: &[&str],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let mut command = temp_dirs::mm();
    command
        .env("HOME", home)
        .arg("--root")
        .arg(root)
        .arg("--mm-home")
        .arg(home.join("mm"))
        .args(arguments);
    Ok(command.output()?)
}

#[test]
fn validate_warns_strict_refuses_and_other_commands_print_each_key_once() -> TestResult {
    let root = project(with_unknown_keys)?;
    let home = temp_dirs::tempdir()?;

    let output = mm_in(root.path(), home.path(), &["config", "validate"])?;
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{printed}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(printed.contains("MM_CONFIG_UNKNOWN_KEY"), "{printed}");
    assert!(
        printed.contains("unknown key `modules.api.services.api.replicaz` in .config/mm.yaml; this mm ignores it (did you mean `replicas`?)"),
        "{printed}"
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Warning:"));

    let strict = mm_in(
        root.path(),
        home.path(),
        &["config", "validate", "--strict"],
    )?;
    assert_eq!(strict.status.code(), Some(2));

    let json = mm_in(
        root.path(),
        home.path(),
        &["--output", "json", "config", "validate"],
    )?;
    assert!(json.status.success());
    let report: serde_json::Value = serde_json::from_slice(&json.stdout)?;
    let unknown = report["validation"]["diagnostics"]
        .as_array()
        .ok_or("no diagnostics")?
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "MM_CONFIG_UNKNOWN_KEY")
        .collect::<Vec<_>>();
    assert_eq!(unknown.len(), 5, "{report}");
    assert!(
        unknown
            .iter()
            .all(|diagnostic| diagnostic["severity"] == "warning")
    );

    let sources = mm_in(root.path(), home.path(), &["config", "sources"])?;
    assert!(sources.status.success());
    let warnings = String::from_utf8_lossy(&sources.stderr);
    assert_eq!(
        warnings
            .matches(
                "Warning: unknown key `newer-top-level` in .config/mm.yaml; this mm ignores it\n"
            )
            .count(),
        1,
        "{warnings}"
    );
    assert_eq!(warnings.matches("Warning:").count(), 5, "{warnings}");
    Ok(())
}
