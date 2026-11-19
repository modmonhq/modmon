use std::path::{Path, PathBuf};

use clap::{CommandFactory, Parser};
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::TargetStore;
use mm::domain::target::{DEFAULT_TRAEFIK_IMAGE, TargetDefinition, TargetId, TargetTransport};
use mm::presentation::cli::Cli;
use predicates::prelude::*;

mod temp_dirs;

const SENTINEL: &str = "TOP_SECRET_SENTINEL_DO_NOT_LEAK";
const HOST_KEY: &str = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU";

#[test]
fn runtime_preference_is_independent_of_global_runtime() -> Result<(), Box<dyn std::error::Error>> {
    for (flags, expected) in [
        (vec![], None),
        (
            vec!["--runtime", "podman-compose"],
            Some(mm::presentation::cli::RuntimeKindArg::PodmanCompose),
        ),
    ] {
        let cli = Cli::try_parse_from(["mm"].into_iter().chain(flags).chain([
            "target",
            "preference",
            "set-runtime",
            "docker-compose",
        ]))?;
        assert_eq!(cli.runtime, expected);
    }
    let project = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    temp_dirs::mm()
        .arg("--root")
        .arg(project.path())
        .arg("--mm-home")
        .arg(home.path())
        .args(["target", "preference", "set-runtime", "docker-compose"])
        .assert()
        .success();
    temp_dirs::mm()
        .arg("--root")
        .arg(project.path())
        .arg("--mm-home")
        .arg(home.path())
        .args(["target", "preference", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"runtime\": \"docker-compose\""));
    Ok(())
}

#[test]
fn first_run_status_reports_missing_build_instead_of_aborting()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    add_local_target(home.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = std::fs::read_to_string(&config_path)?.replace(
        "    path: api",
        "    path: api\n    build:\n      enabled: true\n      image: example/api",
    );
    std::fs::write(config_path, config)?;
    std::fs::write(project.path().join("api/Dockerfile"), "FROM scratch\n")?;
    for command in [vec!["service", "status"], vec!["doctor"]] {
        let output = temp_dirs::mm()
            .env("PATH", home.path())
            .arg("--root")
            .arg(project.path())
            .arg("--mm-home")
            .arg(home.path())
            .args(["--output", "json"])
            .args(&command)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let snapshot: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert!(
            snapshot["validation"]["diagnostics"]
                .as_array()
                .ok_or("diagnostics")?
                .iter()
                .any(|diagnostic| diagnostic["code"] == "MM_BUILD_REQUIRED"),
            "{command:?}: {snapshot}"
        );
    }
    Ok(())
}

#[test]
fn build_receipts_need_a_published_build_and_a_valid_file() -> Result<(), Box<dyn std::error::Error>>
{
    let project = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    add_local_target(home.path())?;
    let receipt = project.path().join("build-receipt.json");
    let mm = |args: &[&str]| {
        let mut command = temp_dirs::mm();
        command
            .env("PATH", home.path())
            .arg("--root")
            .arg(project.path())
            .arg("--mm-home")
            .arg(home.path())
            .args(args);
        command
    };
    mm(&["build", "receipt"])
        .arg(&receipt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no published build"));
    assert!(!receipt.exists());
    std::fs::write(&receipt, "{}")?;
    mm(&["deploy", "plan", "--receipt"])
        .arg(&receipt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a build receipt"));
    mm(&["build", "import"])
        .arg(&receipt)
        .arg("--cache-only")
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a build receipt"));
    Ok(())
}

#[test]
fn schema_is_machine_readable() -> Result<(), Box<dyn std::error::Error>> {
    let mut command = temp_dirs::mm();
    command
        .args(["config", "schema"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"title\": \"MmConfig\""));
    Ok(())
}

#[test]
fn provider_capabilities_do_not_require_a_project_or_state()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dirs::tempdir()?;
    let blocked_home = root.path().join("blocked-home");
    std::fs::write(&blocked_home, "not a directory")?;
    temp_dirs::mm()
        .args([
            "--root",
            path_string(root.path()).as_str(),
            "--mm-home",
            path_string(&blocked_home).as_str(),
            "secret",
            "providers",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("age"));
    let output = temp_dirs::mm()
        .args([
            "--root",
            path_string(root.path()).as_str(),
            "--mm-home",
            path_string(&blocked_home).as_str(),
            "--output",
            "json",
            "secret",
            "providers",
        ])
        .output()?;
    assert!(output.status.success());
    let capabilities: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(capabilities["storage"][0]["id"], "age");
    assert_eq!(capabilities["storage"][1]["id"], "env");
    assert_eq!(capabilities["delivery"][0]["id"], "direct-environment");
    assert_eq!(capabilities["delivery"][1]["id"], "operation-env-file");
    Ok(())
}

#[test]
fn cli_has_no_config_migration_commands_or_aliases() -> Result<(), Box<dyn std::error::Error>> {
    for arguments in [
        ["mm", "secret", "migrate"].as_slice(),
        ["mm", "env", "delivery", "migrate"].as_slice(),
    ] {
        assert!(Cli::command().try_get_matches_from(arguments).is_err());
    }

    let mut secret_help = Vec::new();
    Cli::command()
        .find_subcommand_mut("secret")
        .ok_or("secret command missing")?
        .write_long_help(&mut secret_help)?;
    let secret_help = String::from_utf8_lossy(&secret_help);
    assert!(!secret_help.contains("migrate config") && !secret_help.contains("config migrate"));
    Ok(())
}

#[test]
fn invalid_schemas_fail_planning_before_state_or_runtime_effects()
-> Result<(), Box<dyn std::error::Error>> {
    for version in [0, 2, 4, 5] {
        let project = temp_dirs::tempdir()?;
        copy_fixture(project.path())?;
        let config_path = project.path().join(".config/mm.yaml");
        let source = std::fs::read_to_string(&config_path)?.replacen(
            "schema-version: 1",
            &format!("schema-version: {version}"),
            1,
        );
        std::fs::write(&config_path, &source)?;
        let untouched_home = project.path().join("must-not-be-created");
        let empty_path = project.path().join("empty-path");
        std::fs::create_dir(&empty_path)?;

        temp_dirs::mm()
            .env("PATH", &empty_path)
            .args([
                "--root",
                path_string(project.path()).as_str(),
                "--mm-home",
                path_string(&untouched_home).as_str(),
                "deploy",
                "plan",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "unsupported schema-version {version}"
            )))
            .stderr(predicate::str::contains("expected schema-version: 1"));

        assert!(!untouched_home.exists());
        assert_eq!(std::fs::read_to_string(&config_path)?, source);
    }
    Ok(())
}

#[test]
fn removed_configuration_fails_validation_naming_its_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    const NODES: &str = "targets:\n  production:\n    nodes:\n      - {name: app-1, host: 203.0.113.11}\n      - {name: app-2, host: 203.0.113.12}\n      - {name: db-1, host: 203.0.113.13}\n";
    let validate = |config: &str| -> Result<std::process::Output, Box<dyn std::error::Error>> {
        let project = temp_dirs::tempdir()?;
        let home = temp_dirs::tempdir()?;
        copy_fixture(project.path())?;
        std::fs::write(project.path().join(".config/mm.yaml"), config)?;
        Ok(temp_dirs::mm()
            .args([
                "--root",
                path_string(project.path()).as_str(),
                "--mm-home",
                path_string(home.path()).as_str(),
                "config",
                "validate",
            ])
            .output()?)
    };
    let fixture = std::fs::read_to_string(fixture_root().join(".config/mm.yaml"))?;

    // What replaces the removed forms is accepted: a target that only lists
    // its nodes, with or without cluster settings.
    for accepted in [
        fixture.clone(),
        format!("{fixture}{NODES}"),
        format!("{fixture}{NODES}    cluster:\n      mesh: {{mtu: 1380}}\n      storage: db-1\n"),
    ] {
        let output = validate(&accepted)?;
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let removed = [
        (
            fixture.replacen("driver: podman-compose", "driver: docker-swarm", 1),
            "the docker-swarm runtime driver was removed",
            "docker-compose",
        ),
        (
            format!(
                "{fixture}{NODES}    swarm:\n      storage: app-1\n      groups: {{web: [app-1, app-2]}}\n"
            ),
            "swarm: was removed",
            "cluster:",
        ),
        (
            format!(
                "{fixture}targets:\n  production:\n    host: app.example.test\n    members:\n      - {{host: standby.example.test}}\n"
            ),
            "members: was removed",
            "nodes:",
        ),
    ];
    for (config, removal, replacement) in removed {
        assert_ne!(
            config, fixture,
            "the fixture no longer has the line this case edits"
        );
        let output = validate(&config)?;
        let printed = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.status.success(), "{removal}: {printed}");
        assert!(printed.contains(removal), "{removal}: {printed}");
        assert!(printed.contains(replacement), "{replacement}: {printed}");
        assert!(
            !printed.contains("unknown field") && !printed.contains("unknown variant"),
            "{printed}"
        );
    }
    Ok(())
}

#[test]
fn cli_parse_errors_honor_json_output() -> Result<(), Box<dyn std::error::Error>> {
    temp_dirs::mm()
        .args(["--output", "json", "not-a-command"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("\"code\":\"MM_CLI_PARSE\""));
    Ok(())
}

#[cfg(unix)]
#[test]
fn privileged_ingress_helper_is_hidden_and_rejects_unprivileged_execution()
-> Result<(), Box<dyn std::error::Error>> {
    temp_dirs::mm()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("__install-ingress").not());
    if rustix::process::geteuid().as_raw() != 0 {
        temp_dirs::mm()
            .args([
                "__install-ingress",
                "--artifact-root",
                "/tmp/mm-invalid-ingress-artifact",
                "--target-id",
                "319c527b-6242-7b43-bfd4-bd60e27c1de3",
                "--artifact-hash",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "privileged ingress installation must run through sudo",
            ));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn json_parse_errors_do_not_panic_on_non_utf8_arguments() -> Result<(), Box<dyn std::error::Error>>
{
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    temp_dirs::mm()
        .args(["--output", "json"])
        .arg(OsString::from_vec(vec![0xff]))
        .assert()
        .code(2)
        .stderr(predicate::str::contains("MM_CLI_PARSE"));
    Ok(())
}

#[test]
fn validate_supports_json_for_agents() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let state_path_that_cannot_be_a_directory = home.path().join("not-a-directory");
    std::fs::write(&state_path_that_cannot_be_a_directory, "occupied")?;
    let output = temp_dirs::mm()
        .args([
            "--root",
            path_string(&fixture_root()).as_str(),
            "--mm-home",
            path_string(&state_path_that_cannot_be_a_directory).as_str(),
            "--output",
            "json",
            "config",
            "validate",
        ])
        .output()?;
    assert!(output.status.success());
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        actual,
        serde_json::json!({
            "project_name": "minimal",
            "root": fixture_root(),
            "config_path": fixture_root().join(".config/mm.yaml"),
            "target": "local",
            "environment": "local",
            "config_hash": "0deff7bebf0f09ed4d0083be71aee638c5cfd777ccf58a92e7df822afd43c33b",
            "environment_delivery": {
                "mode": "env-file",
                "provenance": "built-in-default"
            },
            "validation": { "diagnostics": [] }
        })
    );
    Ok(())
}

#[test]
fn bare_command_requires_a_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let mut command = temp_dirs::mm();
    command
        .args(["--root", path_string(&fixture_root()).as_str()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "the Ratatui interface requires an interactive terminal",
        ));
    Ok(())
}

#[test]
fn environment_status_is_available_without_unlocking_secrets()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let mut command = temp_dirs::mm();
    command
        .args([
            "--root",
            path_string(&fixture_root()).as_str(),
            "--mm-home",
            path_string(home.path()).as_str(),
            "env",
            "status",
            "--no-check-secrets",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("DB_PASSWORD"))
        .stdout(predicate::str::contains(SENTINEL).not());
    Ok(())
}

#[test]
fn stack_compile_publishes_metadata_only_overlay_and_manifest()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    add_local_target(home.path())?;
    let output = temp_dirs::mm()
        .args([
            "--root",
            path_string(&fixture_root()).as_str(),
            "--mm-home",
            path_string(home.path()).as_str(),
            "--output",
            "json",
            "config",
            "stack",
            "compile",
        ])
        .output()?;
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["target"], "local");
    assert_eq!(report["stacks"][0]["manifest"]["stack"], "app");
    let overlay = report["stacks"][0]["generated_overlay_path"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("generated overlay path is missing"))?;
    let last_file = report["stacks"][0]["compose_files"]
        .as_array()
        .and_then(|files| files.last())
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| std::io::Error::other("compiled Compose file list is empty"))?;
    assert_eq!(last_file, overlay);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("local/db-password"));
    Ok(())
}

#[test]
fn deploy_plan_is_machine_readable_without_contacting_the_runtime()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    add_local_target(home.path())?;
    let root = path_string(&fixture_root());
    let home_path = path_string(home.path());

    let output = temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "deploy",
            "plan",
        ])
        .output()?;

    assert!(output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(plan["project_name"], "minimal");
    assert_eq!(plan["target_name"], "local");
    assert_eq!(plan["stacks"][0]["stack"], "app");
    assert_eq!(
        plan["plan_hash"].as_str().map(str::len),
        Some(64),
        "deployment plan must carry a review hash"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(SENTINEL));
    Ok(())
}

#[test]
fn build_plan_is_machine_readable_without_opening_state_or_docker()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    std::fs::create_dir_all(project.path().join(".config"))?;
    std::fs::create_dir(project.path().join("app"))?;
    std::fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 119c527b-6242-7b43-bfd4-bd60e27c1de3
  name: build-plan
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    build:
      image: ghcr.io/example/build-plan
    services:
      web:
        stack: app
environments:
  local:
    bindings: {}
"#,
    )?;
    std::fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: ghcr.io/example/build-plan:old\n",
    )?;
    std::fs::write(project.path().join("app/Dockerfile"), "FROM scratch\n")?;
    git(project.path(), &["init", "--initial-branch=main"])?;
    git(project.path(), &["add", "."])?;
    git(
        project.path(),
        &[
            "-c",
            "user.name=mm-test",
            "-c",
            "user.email=mm@example.test",
            "commit",
            "-m",
            "fixture",
        ],
    )?;
    let home_parent = temp_dirs::tempdir()?;
    let occupied_home = home_parent.path().join("occupied-home");
    std::fs::write(&occupied_home, "not a directory")?;

    let output = temp_dirs::mm()
        .args([
            "--root",
            path_string(project.path()).as_str(),
            "--mm-home",
            path_string(&occupied_home).as_str(),
            "--output",
            "json",
            "build",
            "plan",
        ])
        .output()?;

    assert!(output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(plan["schema"], "mm-image-build-plan-v1");
    assert_eq!(plan["units"][0]["module"], "app");
    assert_eq!(plan["config_hash"].as_str().map(str::len), Some(64));
    assert!(!occupied_home.join("state.sqlite3").exists());
    Ok(())
}

#[test]
fn json_deploy_requires_explicit_approval() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    add_local_target(home.path())?;
    temp_dirs::mm()
        .args([
            "--root",
            path_string(&fixture_root()).as_str(),
            "--mm-home",
            path_string(home.path()).as_str(),
            "--output",
            "json",
            "deploy",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("JSON deployment requires --yes"));
    Ok(())
}

#[test]
fn target_catalog_commands_do_not_require_a_project_or_sqlite()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let home_path = path_string(home.path());

    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "target",
            "add",
            "production",
            "--host",
            "host.example.test",
            "--user",
            "mm",
            "--host-key-fingerprint",
            HOST_KEY,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added target production"))
        .stdout(predicate::str::contains("ssh mm@host.example.test:22"));
    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"production\""))
        .stdout(predicate::str::contains("host.example.test"));
    let listed = temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "list",
        ])
        .output()?;
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout)?;
    let fingerprint = listed["targets"][0]["fingerprint"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("target fingerprint is missing"))?;
    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "target",
            "update",
            "production",
            "--host",
            "replacement.example.test",
            "--user",
            "deploy",
            "--host-key-fingerprint",
            HOST_KEY,
            "--expected-fingerprint",
            fingerprint,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated target production"))
        .stdout(predicate::str::contains("replacement.example.test"));
    let listed = temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "list",
        ])
        .output()?;
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout)?;
    let fingerprint = listed["targets"][0]["fingerprint"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("updated target fingerprint is missing"))?;
    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "target",
            "ingress",
            "configure",
            "production",
            "--image",
            "docker.io/library/traefik@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--expected-fingerprint",
            fingerprint,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated target production"))
        .stdout(predicate::str::contains("ingress:     traefik"))
        .stdout(predicate::str::contains("ACME email").not());
    let ingress = temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "ingress",
            "plan",
            "production",
        ])
        .output()?;
    assert!(ingress.status.success());
    let ingress: serde_json::Value = serde_json::from_slice(&ingress.stdout)?;
    assert_eq!(ingress["target_name"], "production");
    assert!(ingress["artifact"]["manifest"].get("acme_email").is_none());
    let compose_path = ingress["artifact"]["compose_path"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("ingress Compose path is missing"))?;
    assert!(compose_path.ends_with("compose.yaml"));
    assert!(!std::fs::read_to_string(compose_path)?.contains(".acme.email"));

    let shown = temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "show",
            "production",
        ])
        .output()?;
    assert!(shown.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&shown.stdout)?;
    let current_fingerprint = shown["fingerprint"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("configured target fingerprint is missing"))?;
    assert_ne!(current_fingerprint, fingerprint);
    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "target",
            "remove",
            "production",
            "--expected-fingerprint",
            current_fingerprint,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --yes"));
    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "target",
            "remove",
            "production",
            "--expected-fingerprint",
            fingerprint,
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("changed since it was reviewed"));
    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "target",
            "remove",
            "production",
            "--expected-fingerprint",
            current_fingerprint,
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed target production"));
    temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("production").not());
    assert!(!home.path().join("state.sqlite3").exists());
    Ok(())
}

#[test]
fn init_creates_a_local_target_and_deterministic_bootstrap_plan()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    std::fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    let root = path_string(project.path());
    let home_path = path_string(home.path());

    let review = temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "init",
            "--name",
            "bootstrap-test",
        ])
        .output()?;
    assert!(
        review.status.success(),
        "initialization review failed: {}",
        String::from_utf8_lossy(&review.stderr)
    );
    let review: serde_json::Value = serde_json::from_slice(&review.stdout)?;
    let init_plan = review["plan_hash"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("initialization plan hash is missing"))?;
    assert!(
        review["canonical_diff"]
            .as_str()
            .is_some_and(|diff| diff.contains("project:"))
    );
    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "init",
            "--name",
            "bootstrap-test",
            "--plan",
            init_plan,
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(".config/ansible/ansible.cfg"))
        .stdout(predicate::str::contains(".config/ansible/bootstrap.yml"));
    assert!(home.path().join("targets.yaml").is_file());
    let targets = std::fs::read_to_string(home.path().join("targets.yaml"))?;
    assert!(targets.contains(DEFAULT_TRAEFIK_IMAGE));
    assert!(!targets.contains("acme-email"));
    assert!(
        project
            .path()
            .join(".config/ansible/bootstrap.yml")
            .is_file()
    );

    for name in [
        "state.sqlite3",
        "state.sqlite3-wal",
        "state.sqlite3-shm",
        "state.sqlite3-journal",
    ] {
        let path = home.path().join(name);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    let ingress = temp_dirs::mm()
        .args([
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "ingress",
            "plan",
            "local",
        ])
        .output()?;
    // On macOS the local ingress plan pins Docker Desktop and Homebrew HAProxy,
    // so a Mac without them reports that prerequisite instead of planning.
    if cfg!(target_os = "macos") && !ingress.status.success() {
        let error = format!(
            "{}{}",
            String::from_utf8_lossy(&ingress.stdout),
            String::from_utf8_lossy(&ingress.stderr)
        );
        assert!(
            ["Docker Desktop", "HAProxy", "Docker CLI"]
                .iter()
                .any(|prerequisite| error.contains(prerequisite)),
            "{error}"
        );
    } else {
        assert!(ingress.status.success());
        let ingress: serde_json::Value = serde_json::from_slice(&ingress.stdout)?;
        assert_eq!(
            ingress["artifact"]["manifest"]["image"],
            DEFAULT_TRAEFIK_IMAGE
        );
        assert!(ingress["artifact"]["manifest"].get("acme_email").is_none());
    }
    let arguments = [
        "--root",
        root.as_str(),
        "--mm-home",
        home_path.as_str(),
        "--output",
        "json",
        "target",
        "setup",
        "plan",
    ];
    let first = temp_dirs::mm().args(arguments).output()?;
    let second = temp_dirs::mm().args(arguments).output()?;
    assert!(first.status.success());
    assert!(second.status.success());
    let first: serde_json::Value = serde_json::from_slice(&first.stdout)?;
    let second: serde_json::Value = serde_json::from_slice(&second.stdout)?;
    assert_eq!(first["plan_hash"], second["plan_hash"]);
    let plan_hash = first["plan_hash"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("bootstrap plan hash is missing"))?;
    assert_eq!(first["target"]["name"], "local");
    assert!(
        first["required_capabilities"]
            .as_array()
            .is_some_and(|capabilities| capabilities.contains(&serde_json::json!("haproxy")))
    );
    assert!(first.get("root").is_none());
    assert!(!home.path().join("state.sqlite3").exists());
    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "target",
            "setup",
            "apply",
            "--plan",
            plan_hash,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "JSON target bootstrap requires --yes",
        ));
    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "target",
            "setup",
            "apply",
            "--plan",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("target bootstrap plan changed"));
    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "target",
            "setup",
            "plan",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("transport: local"))
        .stdout(predicate::str::contains("requires: ansible-playbook"))
        .stdout(predicate::str::contains("requires: haproxy"))
        .stdout(predicate::str::contains("step:    provision-host"));
    Ok(())
}

#[test]
fn dev_setup_creates_the_local_target() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    temp_dirs::mm()
        .args([
            "--mm-home",
            path_string(home.path()).as_str(),
            "dev",
            "setup",
            "--no-trust",
        ])
        .assert()
        .success();

    let target =
        FilesystemTargetStore::new(home.path().join("targets.yaml")).resolve_target("local")?;
    assert_eq!(target.name, "local");
    assert_eq!(target.definition.transport, TargetTransport::Local);
    Ok(())
}

#[cfg(unix)]
#[test]
fn age_secret_cli_never_prints_or_stores_plaintext_in_sqlite()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let root = path_string(&fixture_root());
    let home_path = path_string(home.path());

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "secret",
            "init",
            "age",
        ])
        .assert()
        .success();

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "secret",
            "set",
            "database.password",
            "--stdin",
        ])
        .write_stdin(format!("{SENTINEL}\n"))
        .assert()
        .success()
        .stdout(predicate::str::contains(SENTINEL).not())
        .stderr(predicate::str::contains(SENTINEL).not());

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "secret",
            "status",
            "database.password",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"set\""))
        .stdout(predicate::str::contains("\"revision\""))
        .stdout(predicate::str::contains(SENTINEL).not());

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "env",
            "check",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Environment check passed"))
        .stdout(predicate::str::contains(SENTINEL).not());

    let state = std::fs::read(home.path().join("state.sqlite3"))?;
    assert!(
        !state
            .windows(SENTINEL.len())
            .any(|window| window == SENTINEL.as_bytes())
    );
    Ok(())
}

#[test]
fn secret_status_reports_runtime_delivery_metadata_without_runtime_names_or_plaintext()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    std::fs::create_dir_all(project.path().join(".config"))?;
    std::fs::create_dir(project.path().join("api"))?;
    std::fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: runtime-status
default-environment: local
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    env:
      template: .env.example
    services:
      web:
        stack: app
        secret-files:
          database-password:
            secret: database.password
secrets:
  database.password:
    storage:
      provider: env
      location: MM_STATUS_SECRET_SENTINEL
environments:
  local:
    bindings: {}
"#,
    )?;
    std::fs::write(project.path().join("api/.env.example"), "")?;
    std::fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: docker.io/library/alpine:3.22\n",
    )?;

    let output = temp_dirs::mm()
        .env("MM_STATUS_SECRET_SENTINEL", SENTINEL)
        .args([
            "--root",
            path_string(project.path()).as_str(),
            "--mm-home",
            path_string(home.path()).as_str(),
            "--output",
            "json",
            "secret",
            "status",
            "database.password",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    let status: serde_json::Value = serde_json::from_str(&stdout)?;
    let runtime = &status["runtime_delivery"][0];
    assert_eq!(runtime["logical_name"], "database.password");
    assert_eq!(runtime["backend"], "env");
    assert_eq!(runtime["delivery"], "compose-file-secret");
    assert_eq!(runtime["consumers"][0], "app/web:/database-password");
    assert!(runtime.get("revision").is_some());
    assert!(!stdout.contains("mm-"));
    assert!(!stdout.contains(SENTINEL));
    Ok(())
}

#[cfg(unix)]
#[test]
fn generated_secret_cli_is_create_only_and_rotation_plans_are_metadata_only()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let root = path_string(&fixture_root());
    let home_path = path_string(home.path());

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "secret",
            "init",
            "age",
        ])
        .assert()
        .success();

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "secret",
            "generate",
            "database.password",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"generated\""))
        .stdout(predicate::str::contains("local/db-password").not());

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "secret",
            "generate",
            "--missing",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"already_set\""));

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "secret",
            "rotate",
            "database.password",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rotated secret database.password"));

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "--output",
            "json",
            "secret",
            "rotation",
            "plan",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"plan_hash\""))
        .stdout(predicate::str::contains("\"status\": \"not_scheduled\""))
        .stdout(predicate::str::contains("local/db-password").not());

    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--mm-home",
            home_path.as_str(),
            "secret",
            "generate",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--missing"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn unscheduled_environment_secrets_do_not_make_rotation_hashes_volatile()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = std::fs::read_to_string(&config_path)?.replace(
        "environments:\n",
        "  ci.token:\n    storage:\n      provider: env\n      location: MM_ROTATION_TEST_VALUE\nenvironments:\n",
    );
    std::fs::write(config_path, config)?;
    let home = temp_dirs::tempdir()?;
    let root_path = path_string(project.path());
    let home_path = path_string(home.path());
    let arguments = [
        "--root",
        root_path.as_str(),
        "--mm-home",
        home_path.as_str(),
        "--output",
        "json",
        "secret",
        "rotation",
        "plan",
    ];
    let first = temp_dirs::mm()
        .args(arguments)
        .env("MM_ROTATION_TEST_VALUE", "test-only-value")
        .output()?;
    let second = temp_dirs::mm()
        .args(arguments)
        .env("MM_ROTATION_TEST_VALUE", "test-only-value")
        .output()?;
    assert!(first.status.success());
    assert!(second.status.success());
    let first: serde_json::Value = serde_json::from_slice(&first.stdout)?;
    let second: serde_json::Value = serde_json::from_slice(&second.stdout)?;
    assert_eq!(first["plan_hash"], second["plan_hash"]);
    let environment_secret = first["secrets"]
        .as_array()
        .and_then(|secrets| secrets.iter().find(|secret| secret["name"] == "ci.token"))
        .ok_or_else(|| std::io::Error::other("ci.token missing from rotation plan"))?;
    assert_eq!(environment_secret["status"], "not_scheduled");
    assert!(environment_secret.get("current_revision").is_none());
    Ok(())
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal")
}

fn copy_fixture(destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = fixture_root();
    std::fs::create_dir_all(destination.join(".config"))?;
    std::fs::create_dir_all(destination.join("api"))?;
    std::fs::copy(
        source.join(".config/mm.yaml"),
        destination.join(".config/mm.yaml"),
    )?;
    std::fs::copy(
        source.join("compose.yaml"),
        destination.join("compose.yaml"),
    )?;
    std::fs::copy(
        source.join("api/.env.example"),
        destination.join("api/.env.example"),
    )?;
    Ok(())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn gate_cli_never_accepts_password_or_cap_secret_values_in_argv()
-> Result<(), Box<dyn std::error::Error>> {
    let add_help = temp_dirs::mm().args(["access", "add", "--help"]).output()?;
    assert!(add_help.status.success());
    let add_help = String::from_utf8(add_help.stdout)?;
    assert!(!add_help.contains("cap-site-key"));
    assert!(!add_help.contains("cap-site-secret"));

    let user_help = temp_dirs::mm()
        .args(["access", "user", "add", "--help"])
        .output()?;
    assert!(user_help.status.success());
    let user_help = String::from_utf8(user_help.stdout)?;
    assert!(user_help.contains("--stdin"));
    assert!(!user_help.contains("--password"));

    temp_dirs::mm()
        .args([
            "access",
            "user",
            "add",
            "policy",
            "alice",
            "mm-password-sentinel-never-argv",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
    Ok(())
}

fn git(root: &Path, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed with {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into())
    }
}

#[test]
fn env_delivery_show_is_metadata_only() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let output = temp_dirs::mm()
        .args([
            "--root",
            path_string(&fixture_root()).as_str(),
            "--mm-home",
            path_string(home.path()).as_str(),
            "--output",
            "json",
            "env",
            "delivery",
            "show",
        ])
        .output()?;
    assert!(output.status.success());
    let body = String::from_utf8(output.stdout)?;
    assert!(!body.contains(SENTINEL));
    let value: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(value["environment"], "local");
    assert_eq!(value["mode"], "env-file");
    assert_eq!(value["provenance"], "built-in-default");

    let setup = temp_dirs::mm()
        .args([
            "--root",
            path_string(&fixture_root()).as_str(),
            "--mm-home",
            path_string(home.path()).as_str(),
            "env",
            "setup",
        ])
        .output()?;
    let setup_body = String::from_utf8(setup.stdout)?;
    assert!(setup_body.contains("delivery: env-file (from built-in-default)"));
    assert!(!setup_body.contains(SENTINEL));
    Ok(())
}

#[test]
fn clap_model_is_internally_consistent() {
    Cli::command().debug_assert();
}

#[test]
fn root_exposes_exactly_the_public_families() {
    let actual = Cli::command()
        .get_subcommands()
        .filter(|command| !command.is_hide_set() && command.get_name() != "help")
        .map(|command| command.get_name().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        [
            "init",
            "dev",
            "config",
            "doctor",
            "ingress",
            "service",
            "job",
            "env",
            "secret",
            "access",
            "build",
            "deploy",
            "team",
            "forward",
            "key",
            "target",
            "agent",
            "history",
            "workspace",
            "update",
            "mcp",
        ]
    );
}

#[test]
fn secret_provider_exposes_only_the_generic_instance_workflow()
-> Result<(), Box<dyn std::error::Error>> {
    let command = Cli::command();
    let provider = command
        .find_subcommand("secret")
        .and_then(|secret| secret.find_subcommand("provider"))
        .ok_or("secret provider command missing")?;
    let leaves = provider
        .get_subcommands()
        .filter(|command| command.get_name() != "help")
        .map(|command| command.get_name())
        .collect::<Vec<_>>();
    assert_eq!(
        leaves,
        [
            "list", "show", "add", "edit", "test", "status", "plan", "apply", "backup", "restore",
            "snapshot", "recovery", "remove"
        ]
    );
    for concrete in ["openbao", "vault"] {
        assert!(
            Cli::command()
                .try_get_matches_from(["mm", "secret", "provider", concrete])
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn removed_roots_are_unknown_and_are_not_hidden_aliases() {
    for removed in [
        "setup",
        "guide",
        "module",
        "node",
        "validate",
        "status",
        "logs",
        "exec",
        "shell",
        "schedule",
        "openbao",
        "vault",
        "gate",
        "stack",
        "prefer",
        "ca",
        "tls",
        "services",
        "dashboard",
        "generate",
        "schema",
    ] {
        assert!(
            Cli::command()
                .try_get_matches_from(["mm", removed])
                .is_err(),
            "removed root '{removed}' still parses"
        );
    }
}

#[test]
fn json_family_defaults_never_prompt() -> Result<(), Box<dyn std::error::Error>> {
    temp_dirs::mm()
        .args(["--output", "json", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "requires an explicit `mm dev` subcommand",
        ));
    Ok(())
}

#[test]
fn every_visible_command_has_help_and_exits_zero() {
    assert_help(&Cli::command());
    for path in [
        ["doctor"],
        ["ingress"],
        ["config"],
        ["service"],
        ["env"],
        ["secret"],
        ["build"],
        ["deploy"],
        ["target"],
        ["access"],
        ["dev"],
        ["job"],
        ["mcp"],
        ["init"],
    ] {
        temp_dirs::mm()
            .args(path)
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage:"));
    }
    assert_recursive_help_exits_zero(&Cli::command(), &[]);
}

fn assert_recursive_help_exits_zero(command: &clap::Command, parents: &[String]) {
    if command.is_hide_set() {
        return;
    }
    let mut path = parents.to_vec();
    if command.get_name() != "mm" {
        path.push(command.get_name().to_owned());
    }
    temp_dirs::mm()
        .args(&path)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
    for subcommand in command.get_subcommands() {
        assert_recursive_help_exits_zero(subcommand, &path);
    }
}

#[test]
fn generated_reference_manpage_and_completions_have_no_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for (arguments, path) in [
        (
            vec!["__generate", "reference"],
            root.join("docs/reference/cli.md"),
        ),
        (vec!["__generate", "manpage"], root.join("contrib/man/mm.1")),
        (
            vec!["__generate", "completions", "bash"],
            root.join("contrib/completions/mm.bash"),
        ),
        (
            vec!["__generate", "completions", "zsh"],
            root.join("contrib/completions/_mm"),
        ),
        (
            vec!["__generate", "completions", "fish"],
            root.join("contrib/completions/mm.fish"),
        ),
        (
            vec!["__generate", "completions", "power-shell"],
            root.join("contrib/completions/_mm.ps1"),
        ),
        (
            vec!["__generate", "completions", "elvish"],
            root.join("contrib/completions/mm.elvish"),
        ),
    ] {
        let generated = temp_dirs::mm().args(arguments).output()?;
        assert!(generated.status.success());
        let generated_text = String::from_utf8_lossy(&generated.stdout);
        for internal in [
            "Generate maintained artifacts from the canonical Clap tree",
            "Run the SSH forced-command relay",
            "__dashboard-schema",
            "__install-ingress",
            "__uninstall-ingress",
        ] {
            assert!(
                !generated_text.contains(internal),
                "generated artifact exposes internal command marker '{internal}'"
            );
        }
        assert_eq!(generated.stdout, std::fs::read(path)?);
    }
    Ok(())
}

#[test]
fn module_cli_reviews_then_applies_the_same_typed_plan() -> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    copy_fixture(project.path())?;
    let config_path = project.path().join(".config/mm.yaml");
    let source = std::fs::read_to_string(&config_path)?;
    std::fs::write(&config_path, format!("# keep this comment\n{source}"))?;
    std::fs::create_dir(project.path().join("worker"))?;
    let root = path_string(project.path());
    let definition = r#"{"path":"worker","services":{}}"#;
    let review = temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--output",
            "json",
            "config",
            "module",
            "add",
            "worker",
            "--definition",
            definition,
        ])
        .output()?;
    assert!(
        review.status.success(),
        "module review failed: {}",
        String::from_utf8_lossy(&review.stderr)
    );
    let review: serde_json::Value = serde_json::from_slice(&review.stdout)?;
    let plan = review["plan_hash"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("module review hash is missing"))?;
    assert!(
        review["canonical_diff"]
            .as_str()
            .is_some_and(|diff| diff.contains("worker"))
    );
    temp_dirs::mm()
        .args([
            "--root",
            root.as_str(),
            "--output",
            "json",
            "config",
            "module",
            "add",
            "worker",
            "--definition",
            definition,
            "--plan",
            plan,
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(plan));
    let applied = std::fs::read_to_string(config_path)?;
    assert!(applied.starts_with("# keep this comment\n"));
    assert!(applied.contains("  worker:"));
    Ok(())
}

fn assert_help(command: &clap::Command) {
    if command.is_hide_set() {
        return;
    }
    assert!(
        command.get_about().is_some(),
        "command '{}' is missing about text",
        command.get_name()
    );
    for argument in command.get_arguments() {
        if argument.is_hide_set() || argument.get_id() == "version" || argument.get_id() == "help" {
            continue;
        }
        assert!(
            argument.get_help().is_some()
                || argument.get_long_help().is_some()
                || argument.is_positional(),
            "argument '{}' on '{}' is missing help",
            argument.get_id(),
            command.get_name()
        );
    }
    for subcommand in command.get_subcommands() {
        assert_help(subcommand);
    }
}

fn add_local_target(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let store = FilesystemTargetStore::new(home.join("targets.yaml"));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    Ok(())
}

/// A target has one deploy key: each node keeps one per project. A second
/// environment on it takes that key into its own store from an environment
/// that holds it, and makes no key of its own.
#[test]
fn a_second_environment_on_a_target_takes_the_targets_deploy_key()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    let config = |public_key: &str| {
        format!(
            "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: shop
target: live
stacks:
  app: {{files: [compose.yaml]}}
secrets:
  origin.deploy-key: {{storage: {{provider: age, location: origin.deploy-key}}}}
environments:
  production: {{target: live}}
  staging2: {{target: live}}
targets:
  live:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
    setup:
      origin: https://git.example.test/shop.git
      deploy-key: {{secret: origin.deploy-key, public-key: {public_key}}}
"
        )
    };
    std::fs::create_dir_all(project.path().join(".config"))?;
    std::fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
    std::fs::write(
        project.path().join(".config/mm.yaml"),
        config(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBPnHBZZYETwo334Dl5Vu8LQSzJiO4gxHeq5zQGE+eAL mm-deploy",
        ),
    )?;
    let deploy_key = |environment: &str| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let output = temp_dirs::mm()
            .env("HOME", home.path())
            .args(["--root"])
            .arg(project.path())
            .args(["--mm-home"])
            .arg(home.path().join("mm"))
            .args(["--environment", environment, "--output", "json"])
            .args(["target", "setup", "deploy-key"])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };

    temp_dirs::mm()
        .env("HOME", home.path())
        .args(["--root"])
        .arg(project.path())
        .args(["--mm-home"])
        .arg(home.path().join("mm"))
        .args(["secret", "init", "age"])
        .assert()
        .success();
    let first = deploy_key("production")?;
    assert_eq!(first["created"], true, "{first}");
    assert!(first["copied-from"].is_null(), "{first}");
    let public_key = first["public-key"]
        .as_str()
        .ok_or("no public key")?
        .to_owned();
    assert!(public_key.starts_with("ssh-ed25519 "), "{public_key}");

    // The target names the key; the second environment's store gets the
    // same one, copied, and nothing new goes to the git host.
    std::fs::write(project.path().join(".config/mm.yaml"), config(&public_key))?;
    let second = deploy_key("staging2")?;
    assert_eq!(second["copied-from"], "production", "{second}");
    assert_eq!(second["created"], false, "{second}");
    assert_eq!(second["public-key"], public_key.as_str());

    // From then on, staging2 holds it itself.
    let again = deploy_key("staging2")?;
    assert!(again["copied-from"].is_null(), "{again}");
    assert_eq!(again["created"], false, "{again}");
    assert_eq!(again["public-key"], public_key.as_str());

    // The text says where the key came from and what comes next.
    std::fs::write(
        project.path().join(".config/mm.yaml"),
        config(&public_key).replace(
            "  staging2: {target: live}\n",
            "  staging2: {target: live}\n  qa: {target: live}\n",
        ),
    )?;
    temp_dirs::mm()
        .env("HOME", home.path())
        .args(["--root"])
        .arg(project.path())
        .args(["--mm-home"])
        .arg(home.path().join("mm"))
        .args(["--environment", "qa", "target", "setup", "deploy-key"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Copied the target's deploy key from production into qa's secrets",
        ))
        .stdout(predicate::str::contains("Next: mm -e qa target setup plan"));
    Ok(())
}
