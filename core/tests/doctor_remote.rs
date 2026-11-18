//! `mm doctor` for an environment on remote hosts: this machine's container
//! engine is not asked and does not decide the exit code.

use std::sync::Arc;

use mm::adapters::config::ConfigLoader;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::TargetManager;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROJECT: &str = r"schema-version: 1
default-environment: staging
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de5
  name: doctor-remote
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    project-name: doctor-remote-{environment}-{stack}
    files: [compose.yaml]
modules:
  api:
    path: api
    services:
      api:
        stack: app
targets:
  remote:
    host: 203.0.113.11
    host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
    daemon-fingerprint: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
environments:
  local: {}
  staging:
    target: remote
";

struct Doctor {
    code: Option<i32>,
    stdout: String,
}

/// Runs doctor for one environment on a machine with no container engine:
/// `PATH` is an empty directory.
fn doctor(environment: &str, arguments: &[&str]) -> Result<Doctor, Box<dyn std::error::Error>> {
    let root = temp_dirs::tempdir()?;
    let home = temp_dirs::tempdir()?;
    let empty = temp_dirs::tempdir()?;
    std::fs::create_dir(root.path().join(".config"))?;
    std::fs::create_dir(root.path().join("api"))?;
    std::fs::write(root.path().join(".config/mm.yaml"), PROJECT)?;
    // This machine is registered as a place to run environments, as
    // `mm target local` does.
    TargetManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml"))),
    )
    .ensure_local()?;
    std::fs::write(
        root.path().join("compose.yaml"),
        "services:\n  api:\n    image: busybox:1.36.1\n",
    )?;
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(root.path())
        .arg("--mm-home")
        .arg(home.path())
        .args(["--environment", environment])
        .args(arguments)
        .arg("doctor")
        .env("PATH", empty.path())
        .output()?;
    Ok(Doctor {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout)?,
    })
}

#[test]
fn a_remote_environment_is_not_failed_by_this_machines_engine() -> TestResult {
    let text = doctor("staging", &[])?;
    assert_eq!(text.code, Some(0), "{}", text.stdout);
    assert!(
        text.stdout.contains(
            "runtime:     DockerCompose (on the target's hosts; this machine's engine is not used)"
        ),
        "{}",
        text.stdout
    );
    assert!(!text.stdout.contains("(unavailable)"), "{}", text.stdout);
    assert!(!text.stdout.contains("error["), "{}", text.stdout);

    let json = doctor("staging", &["--output", "json"])?;
    assert_eq!(json.code, Some(0), "{}", json.stdout);
    let report: serde_json::Value = serde_json::from_str(&json.stdout)?;
    assert_eq!(report["runtime"]["available"], false, "{report}");
    let notes = report["runtime"]["diagnostics"]
        .as_array()
        .ok_or("runtime diagnostics")?;
    assert!(
        notes
            .iter()
            .all(|note| note["severity"] == "info"
                && note["code"] == "MM_REMOTE_RUNTIME_NOT_INSPECTED"),
        "{report}"
    );
    Ok(())
}

/// The same project and the same machine, an environment that runs here:
/// the engine was asked, is reported as unavailable, and doctor fails.
#[test]
fn a_local_environment_still_fails_without_an_engine() -> TestResult {
    let text = doctor("local", &[])?;
    assert!(
        matches!(text.code, Some(2 | 3)),
        "{:?}: {}",
        text.code,
        text.stdout
    );
    assert!(
        text.stdout
            .contains("runtime:     DockerCompose (unavailable)"),
        "{}",
        text.stdout
    );
    assert!(
        !text.stdout.contains("MM_REMOTE_RUNTIME_NOT_INSPECTED"),
        "{}",
        text.stdout
    );
    Ok(())
}
