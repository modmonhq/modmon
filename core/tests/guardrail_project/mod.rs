//! A project whose Compose files a test writes, checked with `mm config
//! validate --for-remote` for its remote environment `staging`.

#![allow(dead_code)]

use std::path::Path;

use super::daemon_host::{OPS, TestResult};
use super::temp_dirs;

const PROJECT_ID: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";

fn config(services: &[&str], files: &str) -> String {
    let declared = services
        .iter()
        .map(|service| format!("      {service}: {{stack: app}}\n"))
        .collect::<String>();
    format!(
        "schema-version: 1
project:
  id: {PROJECT_ID}
  name: example
default-environment: local
target: local
stacks:
  app:
    files: [{files}]
modules:
  api:
    path: .
    services:
{declared}environments:
  local: {{}}
  staging: {{target: staging}}
"
    )
}

pub fn project(services: &[&str], files: &[(&str, &str)]) -> TestResult<tempfile::TempDir> {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    std::fs::create_dir_all(root.join(".config"))?;
    let names = files
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(root.join(".config/mm.yaml"), config(services, &names))?;
    for (name, text) in files {
        std::fs::write(root.join(name), text)?;
    }
    Ok(temporary)
}

pub fn host_policy(temporary: &Path, allow: &str) -> TestResult<String> {
    let path = temporary.join("host.yaml");
    std::fs::write(
        &path,
        format!(
            "schema-version: 1
break-glass:
  - name: ops
    key: {OPS}
projects:
  {PROJECT_ID}:
    name: example
    delivery: push
    environments: [staging]
    allow:
{allow}"
        ),
    )?;
    Ok(path.to_string_lossy().into_owned())
}

/// The exit code of `mm -e staging config validate --for-remote` with
/// `flags`, and its guardrail diagnostics as `CODE message | help`.
pub fn validate(temporary: &Path, flags: &[&str]) -> TestResult<(i32, Vec<String>)> {
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(temporary.join("project"))
        .arg("--mm-home")
        .arg(temporary.join("home"))
        .args(["--output", "json", "-e", "staging"])
        .args(["config", "validate", "--for-remote"])
        .args(flags)
        .env_remove("MM_ENVIRONMENT")
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "mm config validate printed no report ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let findings = report["validation"]["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| {
            diagnostic["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("MM_REMOTE_GUARDRAIL"))
        })
        .map(|diagnostic| {
            format!(
                "{} {} | {}",
                diagnostic["code"].as_str().unwrap_or_default(),
                diagnostic["message"].as_str().unwrap_or_default(),
                diagnostic["help"].as_str().unwrap_or_default()
            )
        })
        .collect();
    Ok((output.status.code().unwrap_or(-1), findings))
}

pub const REFUSED: &str =
    "MM_REMOTE_GUARDRAIL a remote host refuses this in environment 'staging': ";
pub const ALLOWED: &str =
    "MM_REMOTE_GUARDRAIL_ALLOWED the host policy allows this in environment 'staging': ";

pub fn has(findings: &[String], prefix: &str, finding: &str) -> bool {
    findings
        .iter()
        .any(|line| line.starts_with(&format!("{prefix}{finding}")))
}
