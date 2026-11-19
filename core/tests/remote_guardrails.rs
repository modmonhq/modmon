//! A remote host checks the Compose features of the services the planned
//! environment runs, and `mm config validate --for-remote` checks the same
//! services, environment by environment.

mod daemon_host;
mod temp_dirs;

use std::path::Path;

use daemon_host::{ALICE, Host, PROMPT, TestResult};
use mm::domain::remote::{RemoteMethod, RemoteResult};

const CONFIG: &str = "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: example
default-environment: local
target: local
stacks:
  app:
    files: [compose.yaml]
  tools:
    files: [tools.yaml]
modules:
  api:
    path: .
    services:
      web: {stack: app}
      vpn: {stack: tools, environments: [local]}
environments:
  local: {}
  staging: {target: staging}
";

const APP: &str = "services:
  web:
    image: example/web
";

const TOOLS: &str = "services:
  vpn:
    image: example/vpn
    network_mode: host
    cap_add: [NET_ADMIN]
";

fn project(app: &str) -> TestResult<tempfile::TempDir> {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::write(root.join(".config/mm.yaml"), CONFIG)?;
    std::fs::write(root.join("compose.yaml"), app)?;
    std::fs::write(root.join("tools.yaml"), TOOLS)?;
    Ok(temporary)
}

/// The exit code and the guardrail diagnostics of `mm config validate
/// --for-remote`, with `global` options before it and `flags` after.
fn validate(
    temporary: &Path,
    global: &[&str],
    flags: &[&str],
) -> TestResult<(i32, Vec<serde_json::Value>)> {
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(temporary.join("project"))
        .arg("--mm-home")
        .arg(temporary.join("home"))
        .args(["--output", "json"])
        .args(global)
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
        .cloned()
        .collect();
    Ok((output.status.code().unwrap_or(-1), findings))
}

fn messages(findings: &[serde_json::Value]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| {
            format!(
                "{} {}",
                finding["code"].as_str().unwrap_or_default(),
                finding["message"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn a_service_only_one_environment_runs_is_checked_only_there() -> TestResult {
    let temporary = project(APP)?;

    let (status, findings) = validate(temporary.path(), &["-e", "staging"], &[])?;
    assert_eq!((status, messages(&findings)), (0, Vec::new()));

    let (status, findings) = validate(temporary.path(), &["-e", "local"], &[])?;
    assert_eq!(status, 2);
    let reported = messages(&findings);
    for feature in [
        "tools/vpn uses network-host (network_mode: host)",
        "tools/vpn uses cap-add (cap_add: NET_ADMIN)",
    ] {
        assert!(
            reported.contains(&format!(
                "MM_REMOTE_GUARDRAIL a remote host refuses this in environment 'local': {feature}"
            )),
            "{reported:?}"
        );
    }

    // Without -e: the environments whose target is not `local`, and the
    // default environment is not one of them.
    let (status, findings) = validate(temporary.path(), &[], &[])?;
    assert_eq!((status, messages(&findings)), (0, Vec::new()));
    Ok(())
}

#[test]
fn without_an_environment_each_remote_environment_is_named() -> TestResult {
    let temporary = project("services:\n  web:\n    image: example/web\n    pid: host\n")?;
    let config = temporary.path().join("project/.config/mm.yaml");
    std::fs::write(
        &config,
        CONFIG.replace(
            "  staging: {target: staging}\n",
            "  staging: {target: staging}\n  production: {target: prod-eu}\n",
        ),
    )?;
    let (status, findings) = validate(temporary.path(), &[], &[])?;
    assert_eq!(status, 2);
    assert_eq!(
        messages(&findings),
        [
            "MM_REMOTE_GUARDRAIL a remote host refuses this in environment 'production': app/web uses pid-host (pid: host)",
            "MM_REMOTE_GUARDRAIL a remote host refuses this in environment 'staging': app/web uses pid-host (pid: host)",
        ]
    );
    Ok(())
}

#[test]
fn a_feature_behind_a_yaml_merge_key_is_found() -> TestResult {
    let temporary = project(
        "x-host: &host\n  network_mode: host\nservices:\n  web:\n    <<: *host\n    image: example/web\n",
    )?;
    let (status, findings) = validate(temporary.path(), &["-e", "staging"], &[])?;
    assert_eq!(status, 2);
    assert_eq!(
        messages(&findings),
        [
            "MM_REMOTE_GUARDRAIL a remote host refuses this in environment 'staging': app/web uses network-host (network_mode: host)"
        ]
    );
    Ok(())
}

const LOCAL_ONLY: (&str, &str) = (
    "  app:\n    files: [compose.yaml]\nenvironments:\n  production: {}\nmodules:\n  api:\n    path: .\n    services:\n      web: {stack: app}\n",
    "  app:\n    files: [compose.yaml]\n  tools:\n    files: [tools.yaml]\nenvironments:\n  production: {}\n  local: {}\nmodules:\n  api:\n    path: .\n    services:\n      web: {stack: app}\n      vpn: {stack: tools, environments: [local]}\n",
);

fn plan(commit: &str) -> RemoteMethod {
    RemoteMethod::PlanDeployment {
        commit: commit.to_owned(),
        receipt: None,
        release: Default::default(),
    }
}

#[tokio::test]
async fn a_host_plans_without_an_exception_for_a_service_its_environment_does_not_run() -> TestResult
{
    let host = Host::with_alice().await?;
    let config = host.temporary.path().join("origin-work/.config/mm.yaml");
    let text = std::fs::read_to_string(&config)?;
    assert!(text.contains(LOCAL_ONLY.0), "{text}");
    let commit = host.commit_files(&[
        (".config/mm.yaml", &text.replace(LOCAL_ONLY.0, LOCAL_ONLY.1)),
        ("tools.yaml", TOOLS),
    ])?;
    match host.ask(ALICE, plan(&commit)).await?.result(PROMPT).await? {
        RemoteResult::DeploymentPlan(plan) => {
            assert_eq!(plan.binding.environment, "production");
        }
        other => return Err(format!("unexpected plan result: {other:?}").into()),
    }

    let everywhere = host.commit_with(
        "vpn: {stack: tools, environments: [local]}",
        "vpn: {stack: tools, environments: [local, production]}",
    )?;
    let (code, message) = host
        .ask(ALICE, plan(&everywhere))
        .await?
        .refusal(PROMPT)
        .await?;
    assert_eq!(code, "MM_SECURITY");
    assert!(
        message.contains("in environment 'production'")
            && message.contains("tools/vpn uses network-host (network_mode: host)")
            && message.contains("tools/vpn uses cap-add (cap_add: NET_ADMIN)"),
        "{message}"
    );
    Ok(())
}
