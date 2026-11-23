//! Namespaces a service reaches through another: a remote host refuses a
//! container joined by name, and a `service:` join that reaches a host
//! namespace needs the same exception as the service it joins.

mod daemon_host;
mod temp_dirs;

use std::path::Path;

use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult};
use mm::domain::remote::RemoteMethod;

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

fn project(services: &[&str], files: &[(&str, &str)]) -> TestResult<tempfile::TempDir> {
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

fn host_policy(temporary: &Path, allow: &str) -> TestResult<String> {
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
fn validate(temporary: &Path, flags: &[&str]) -> TestResult<(i32, Vec<String>)> {
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

const REFUSED: &str = "MM_REMOTE_GUARDRAIL a remote host refuses this in environment 'staging': ";
const ALLOWED: &str =
    "MM_REMOTE_GUARDRAIL_ALLOWED the host policy allows this in environment 'staging': ";

fn has(findings: &[String], prefix: &str, finding: &str) -> bool {
    findings
        .iter()
        .any(|line| line.starts_with(&format!("{prefix}{finding}")))
}

#[test]
fn joining_a_container_by_name_is_refused_even_with_an_exception() -> TestResult {
    let temporary = project(
        &["web"],
        &[(
            "compose.yaml",
            "services:\n  web:\n    image: example/web\n    pid: container:other-db-1\n    ipc: 'container:other-db-1'\n    network_mode: container:other-db-1\n",
        )],
    )?;
    let policy = host_policy(
        temporary.path(),
        "      - service: app/web\n        features: [container-join, pid-host, ipc-host, network-host]\n",
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 2, "{findings:?}");
    assert_eq!(findings.len(), 3, "{findings:?}");
    for key in ["pid", "ipc", "network_mode"] {
        assert!(
            has(
                &findings,
                REFUSED,
                &format!(
                    "app/web uses container-join ({key}: container:other-db-1 in compose.yaml joins any container of that name on the host; write `{key}: service:<service>` to join a service of this stack)"
                )
            ),
            "{findings:?}"
        );
    }
    assert!(
        findings
            .iter()
            .all(|line| line.ends_with("| no host exception allows this; join a service of the same stack with `service:<service>`")),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn joining_an_ordinary_service_needs_no_exception() -> TestResult {
    let temporary = project(
        &["web", "proxy"],
        &[(
            "compose.yaml",
            "services:\n  proxy:\n    image: example/proxy\n  web:\n    image: example/web\n    pid: service:proxy\n    ipc: service:proxy\n    network_mode: service:proxy\n",
        )],
    )?;
    let (status, findings) = validate(temporary.path(), &[])?;
    assert_eq!((status, findings), (0, Vec::new()));
    Ok(())
}

#[test]
fn a_service_chain_to_a_host_namespace_needs_the_same_exception() -> TestResult {
    let temporary = project(
        &["agent", "web", "worker"],
        &[(
            "compose.yaml",
            "services:\n  agent:\n    image: example/agent\n    pid: host\n  web:\n    image: example/web\n    pid: service:agent\n  worker:\n    image: example/worker\n    pid: service:web\n",
        )],
    )?;
    let policy = host_policy(
        temporary.path(),
        "      - service: app/agent\n        features: [pid-host]\n",
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 2, "{findings:?}");
    assert_eq!(findings.len(), 3, "{findings:?}");
    assert!(
        has(&findings, ALLOWED, "app/agent uses pid-host (pid: host)"),
        "{findings:?}"
    );
    assert!(
        has(
            &findings,
            REFUSED,
            "app/web uses pid-host (pid: service:agent in compose.yaml, which reaches `pid: host` of app/agent) | drop the feature, or allow it for app/web under this project's `allow`"
        ),
        "{findings:?}"
    );
    assert!(
        has(
            &findings,
            REFUSED,
            "app/worker uses pid-host (pid: service:web in compose.yaml, which reaches `pid: host` of app/agent)"
        ),
        "{findings:?}"
    );

    host_policy(
        temporary.path(),
        "      - service: app/agent\n        features: [pid-host]\n      - service: app/web\n        features: [pid-host]\n      - service: app/worker\n        features: [pid-host]\n",
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 0, "{findings:?}");
    assert_eq!(findings.len(), 3, "{findings:?}");
    assert!(
        findings.iter().all(|line| line.starts_with(ALLOWED)),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_join_across_files_and_extends_is_followed() -> TestResult {
    let temporary = project(
        &["vpn", "base", "sidecar", "loop-a", "loop-b"],
        &[
            (
                "compose.yaml",
                "services:\n  vpn:\n    image: example/vpn\n  loop-a:\n    image: example/web\n    ipc: service:loop-b\n  loop-b:\n    image: example/web\n    ipc: service:loop-a\n",
            ),
            (
                "override.yaml",
                "services:\n  vpn:\n    network_mode: host\n  base:\n    image: example/web\n    network_mode: service:vpn\n  sidecar:\n    extends: base\n",
            ),
        ],
    )?;
    let (status, findings) = validate(temporary.path(), &[])?;
    assert_eq!(status, 2, "{findings:?}");
    assert_eq!(findings.len(), 3, "{findings:?}");
    assert!(
        has(
            &findings,
            REFUSED,
            "app/vpn uses network-host (network_mode: host)"
        ),
        "{findings:?}"
    );
    for service in ["base", "sidecar"] {
        assert!(
            has(
                &findings,
                REFUSED,
                &format!(
                    "app/{service} uses network-host (network_mode: service:vpn in override.yaml, which reaches `network_mode: host` of app/vpn)"
                )
            ),
            "{findings:?}"
        );
    }
    Ok(())
}

#[test]
fn a_namespace_mode_from_a_variable_counts_as_the_hosts() -> TestResult {
    let temporary = project(
        &["web", "worker"],
        &[(
            "compose.yaml",
            "services:\n  web:\n    image: example/web\n    pid: ${PID_MODE:-host}\n  worker:\n    image: example/worker\n    pid: service:web\n",
        )],
    )?;
    let (status, findings) = validate(temporary.path(), &[])?;
    assert_eq!(status, 2, "{findings:?}");
    assert!(
        has(
            &findings,
            REFUSED,
            "app/web uses pid-host (pid: ${PID_MODE:-host} in compose.yaml comes from a variable; write the mode itself)"
        ),
        "{findings:?}"
    );
    assert!(
        has(
            &findings,
            REFUSED,
            "app/worker uses pid-host (pid: service:web in compose.yaml, which reaches `pid: ${PID_MODE:-host}` of app/web)"
        ),
        "{findings:?}"
    );
    Ok(())
}

fn plan(commit: &str) -> RemoteMethod {
    RemoteMethod::PlanDeployment {
        commit: commit.to_owned(),
        receipt: None,
        release: Default::default(),
    }
}

#[tokio::test]
async fn a_host_refuses_joins_at_plan_time() -> TestResult {
    let host = Host::with_alice().await?;
    let config = host.temporary.path().join("origin-work/.config/mm.yaml");
    let config = std::fs::read_to_string(config)?.replace(
        "      web: {stack: app}\n",
        "      web: {stack: app}\n      agent: {stack: app}\n",
    );

    let by_name = host.commit_files(&[
        (".config/mm.yaml", &config),
        (
            "compose.yaml",
            "services:\n  agent:\n    image: busybox\n  web:\n    image: busybox\n    network_mode: container:other-db-1\n",
        ),
    ])?;
    let (code, message) = host
        .ask(ALICE, plan(&by_name))
        .await?
        .refusal(PROMPT)
        .await?;
    assert_eq!(code, "MM_SECURITY", "{message}");
    assert!(
        message.contains("app/web uses container-join (network_mode: container:other-db-1 in compose.yaml joins any container of that name on the host; write `network_mode: service:<service>` to join a service of this stack)")
            && !message.contains("under `allow`"),
        "{message}"
    );

    let chained = host.commit_files(&[(
        "compose.yaml",
        "services:\n  agent:\n    image: busybox\n    pid: host\n  web:\n    image: busybox\n    pid: service:agent\n",
    )])?;
    let policy = std::fs::read_to_string(&host.config.host_policy_path)?;
    std::fs::write(
        &host.config.host_policy_path,
        format!("{policy}    allow:\n      - service: app/agent\n        features: [pid-host]\n"),
    )?;
    let (code, message) = host
        .ask(ALICE, plan(&chained))
        .await?
        .refusal(PROMPT)
        .await?;
    assert_eq!(code, "MM_SECURITY", "{message}");
    assert!(
        message.contains("app/web uses pid-host (pid: service:agent in compose.yaml, which reaches `pid: host` of app/agent)")
            && !message.contains("app/agent uses")
            && message.contains("under `allow` in /etc/mm/host.yaml"),
        "{message}"
    );

    std::fs::write(
        &host.config.host_policy_path,
        format!(
            "{policy}    allow:\n      - service: app/agent\n        features: [pid-host]\n      - service: app/web\n        features: [pid-host]\n"
        ),
    )?;
    host.plan_at(ALICE, &chained).await?;
    Ok(())
}
