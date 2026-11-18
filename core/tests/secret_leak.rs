use predicates::prelude::*;

mod temp_dirs;

const SENTINEL: &str = "mm-leak-sentinel-9f3c2a1b7e44d0c8";

#[test]
fn plans_status_and_help_never_echo_seeded_secret_values() -> Result<(), Box<dyn std::error::Error>>
{
    let project = temp_dirs::tempdir()?;
    std::fs::create_dir_all(project.path().join(".config"))?;
    std::fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 219c527b-6242-7b43-bfd4-bd60e27c1de3
  name: leak
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web:
        stack: app
secrets:
  leak.token:
    storage:
      provider: env
      location: MM_LEAK_TOKEN
environments:
  local:
    bindings: {}
"#,
    )?;
    std::fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: docker.io/library/alpine:3.22\n",
    )?;
    std::fs::create_dir(project.path().join("app"))?;
    let home = temp_dirs::tempdir()?;

    for args in [
        vec!["--output", "json", "config", "validate"],
        vec!["--output", "json", "env", "status", "--no-check-secrets"],
        vec!["--help"],
        vec!["deploy", "--help"],
    ] {
        let output = temp_dirs::mm()
            .env("MM_LEAK_TOKEN", SENTINEL)
            .args([
                "--root",
                project.path().to_str().ok_or("utf8")?,
                "--mm-home",
                home.path().to_str().ok_or("utf8")?,
            ])
            .args(&args)
            .output()?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !combined.contains(SENTINEL),
            "secret leaked in {args:?}: {combined}"
        );
    }
    temp_dirs::mm()
        .env("MM_LEAK_TOKEN", SENTINEL)
        .args(["config", "schema"])
        .assert()
        .success()
        .stdout(predicate::str::contains(SENTINEL).not());
    Ok(())
}
