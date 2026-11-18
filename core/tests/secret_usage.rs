//! Which declared secrets `mm config validate` calls unused.
//!
//! Found on the test VMs: a secret that only a service's `secret-files`
//! mounts was reported as `MM_SECRET_UNUSED` ("is not used by any
//! environment binding") on every validation and plan.

mod temp_dirs;

#[test]
fn a_secret_only_secret_files_mounts_is_not_reported_unused()
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
  name: secret-usage
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
    services:
      web:
        stack: app
        secret-files:
          app-token:
            secret: app.token
secrets:
  app.token:
    storage:
      provider: env
      location: MM_SECRET_USAGE_TOKEN
  nobody.uses:
    storage:
      provider: env
      location: MM_SECRET_USAGE_NOBODY
environments:
  local:
    bindings: {}
"#,
    )?;
    std::fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: docker.io/library/alpine:3.22\n",
    )?;
    let output = temp_dirs::mm()
        .args([
            "--root",
            &project.path().to_string_lossy(),
            "--mm-home",
            &home.path().to_string_lossy(),
            "config",
            "validate",
        ])
        .output()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let unused: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("MM_SECRET_UNUSED"))
        .collect();
    assert_eq!(unused.len(), 1, "{text}");
    assert!(unused[0].contains("'nobody.uses'"), "{text}");
    Ok(())
}
