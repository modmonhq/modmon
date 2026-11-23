//! A host path a service bind-mounts, such as `/proc` for a metrics
//! exporter: a remote host runs it read-only when its policy lists the path
//! for the service under `host-path`, and refuses anything else.

mod daemon_host;
mod guardrail_project;
mod temp_dirs;

use daemon_host::{ALICE, Host, PROMPT, TestResult};
use guardrail_project::{ALLOWED, REFUSED, host_policy, project, validate};
use mm::domain::remote::RemoteMethod;

const EXPORTER: &str = "services:
  alloy:
    image: example/alloy
    network_mode: host
    volumes:
      - /proc:/host/proc:ro
      - type: bind
        source: /sys
        target: /host/sys
        read_only: true
      - type: bind
        source: /mnt/data
        target: /host/data
        read_only: true
        bind: {propagation: rslave, recursive: readonly}
      - data:/data
volumes:
  data: {}
";

const LISTED: &str = "      - service: app/alloy
        features: [host-path, network-host]
        paths: [/proc, /sys/, /mnt/data]
";

/// Runs `services` only in `staging`: on this machine mm refuses host
/// paths.
fn remote_only(temporary: &std::path::Path, services: &[&str]) -> TestResult {
    let path = temporary.join("project/.config/mm.yaml");
    let mut config = std::fs::read_to_string(&path)?;
    for service in services {
        config = config.replace(
            &format!("{service}: {{stack: app}}"),
            &format!("{service}: {{stack: app, environments: [staging]}}"),
        );
    }
    std::fs::write(path, config)?;
    Ok(())
}

fn lines(findings: &[String], prefix: &str) -> Vec<String> {
    let mut lines = findings
        .iter()
        .filter_map(|line| line.strip_prefix(prefix))
        .map(|line| line.to_owned())
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

#[test]
fn a_listed_host_path_runs_read_only() -> TestResult {
    let temporary = project(&["alloy"], &[("compose.yaml", EXPORTER)])?;
    remote_only(temporary.path(), &["alloy"])?;
    let (status, findings) = validate(temporary.path(), &[])?;
    assert_eq!(status, 2, "{findings:?}");
    assert_eq!(lines(&findings, REFUSED).len(), 4, "{findings:?}");

    let policy = host_policy(temporary.path(), LISTED)?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 0, "{findings:?}");
    let allowed = lines(&findings, ALLOWED)
        .into_iter()
        .map(|line| line.split(" | ").next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        allowed,
        [
            "app/alloy uses host-path (volumes: /mnt/data:/host/data in compose.yaml mounts the host's /mnt/data read-only)",
            "app/alloy uses host-path (volumes: /proc:/host/proc:ro in compose.yaml mounts the host's /proc read-only)",
            "app/alloy uses host-path (volumes: /sys:/host/sys in compose.yaml mounts the host's /sys read-only)",
            "app/alloy uses network-host (network_mode: host)",
        ],
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_writable_shared_relabelled_ambiguous_or_unlisted_host_path_is_refused() -> TestResult {
    let temporary = project(
        &["probe"],
        &[(
            "compose.yaml",
            "services:\n  probe:\n    image: example/probe\n    volumes:\n      - /srv/data:/data\n      - {type: bind, source: /opt/metrics, target: /metrics}\n      - /sys:/host/sys:ro,rshared\n      - /srv/logs:/logs:ro,Z\n      - /proc:/host/proc:ro,rw\n      - /mnt/a:/a:ro,rslave,rshared\n      - {type: bind, source: /mnt/b, target: /b, read_only: true, bind: {recursive: writable}}\n      - /opt:/opt:ro\n",
        )],
    )?;
    let policy = host_policy(
        temporary.path(),
        "      - service: app/probe\n        features: [host-path]\n        paths: [/srv/data, /opt/metrics, /sys, /srv/logs, /proc, /mnt/a, /mnt/b]\n",
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 2, "{findings:?}");
    let outright = "no host exception allows this: change the mount as the message says";
    let twice =
        "with more than one access or propagation option, where the last wins; write each once";
    let writable = "writable; add `:ro`, or `read_only: true` in the long syntax";
    let mut expected = vec![
        format!("app/probe uses host-path (volumes: /srv/data:/data in compose.yaml mounts the host's /srv/data {writable}) | {outright}"),
        format!("app/probe uses host-path (volumes: /opt/metrics:/metrics in compose.yaml mounts the host's /opt/metrics {writable}) | {outright}"),
        format!("app/probe uses host-path (volumes: /sys:/host/sys:ro,rshared in compose.yaml mounts the host's /sys with shared propagation; use `rslave`, or leave propagation out) | {outright}"),
        format!("app/probe uses host-path (volumes: /srv/logs:/logs:ro,Z in compose.yaml mounts the host's /srv/logs with an SELinux relabel; remove `z` or `Z`) | {outright}"),
        format!("app/probe uses host-path (volumes: /proc:/host/proc:ro,rw in compose.yaml mounts the host's /proc {twice}) | {outright}"),
        format!("app/probe uses host-path (volumes: /mnt/a:/a:ro,rslave,rshared in compose.yaml mounts the host's /mnt/a {twice}) | {outright}"),
        format!("app/probe uses host-path (volumes: /mnt/b:/b in compose.yaml mounts the host's /mnt/b with `bind.recursive: writable`; leave it out, or write `readonly`) | {outright}"),
        "app/probe uses host-path (volumes: /opt:/opt:ro in compose.yaml mounts the host's /opt read-only) | drop the feature, or allow it for app/probe under this project's `allow` in POLICY, with `/opt` under its `paths`".replace("POLICY", &policy),
    ];
    expected.sort();
    assert_eq!(lines(&findings, REFUSED), expected, "{findings:?}");
    Ok(())
}

#[test]
fn a_host_policy_refuses_a_path_that_would_give_root() -> TestResult {
    let temporary = project(&["alloy"], &[("compose.yaml", EXPORTER)])?;
    for (path, protected) in [
        ("/", "/run"),
        ("/var/run/docker.sock", "/var/run"),
        ("/etc", "/etc/mm"),
        ("/home/ops", "/home"),
    ] {
        let policy = host_policy(
            temporary.path(),
            &format!(
                "      - service: app/alloy\n        features: [host-path]\n        paths: [{path}]\n"
            ),
        )?;
        let output = temp_dirs::mm()
            .arg("--root")
            .arg(temporary.path().join("project"))
            .arg("--mm-home")
            .arg(temporary.path().join("home"))
            .args([
                "-e",
                "staging",
                "config",
                "validate",
                "--for-remote",
                "--host-policy",
                &policy,
            ])
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!(
                "project 'example' allow entry for app/alloy: host path '{path}' holds or is inside {protected}, which would give the service root on the host"
            )),
            "{stderr}"
        );
    }
    Ok(())
}

#[test]
fn a_later_file_or_extends_cannot_make_a_listed_path_writable() -> TestResult {
    let temporary = project(
        &["alloy", "debug"],
        &[
            ("compose.yaml", EXPORTER),
            (
                "override.yaml",
                "services:\n  alloy:\n    volumes: ['/proc:/host/proc']\n  debug:\n    extends: alloy\n",
            ),
        ],
    )?;
    let policy = host_policy(
        temporary.path(),
        &format!(
            "{LISTED}      - service: app/debug\n        features: [host-path, network-host]\n        paths: [/proc, /sys, /mnt/data]\n"
        ),
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 2, "{findings:?}");
    let refused = lines(&findings, REFUSED);
    assert_eq!(refused.len(), 2, "{findings:?}");
    for service in ["alloy", "debug"] {
        assert!(
            refused.iter().any(|line| line.starts_with(&format!(
                "app/{service} uses host-path (volumes: /proc:/host/proc in override.yaml mounts the host's /proc writable"
            ))),
            "{findings:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_host_plans_a_host_path_only_when_its_policy_lists_it() -> TestResult {
    let host = Host::with_alice().await?;
    let config = host.temporary.path().join("origin-work/.config/mm.yaml");
    let config = std::fs::read_to_string(config)?.replace(
        "      web: {stack: app}\n",
        "      web: {stack: app}\n      exporter: {stack: app}\n",
    );
    let commit = host.commit_files(&[
        (".config/mm.yaml", &config),
        (
            "compose.yaml",
            "services:\n  web:\n    image: busybox\n  exporter:\n    image: busybox\n    volumes: ['/proc:/host/proc:ro']\n",
        ),
    ])?;
    let plan = || RemoteMethod::PlanDeployment {
        commit: commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let policy = std::fs::read_to_string(&host.config.host_policy_path)?;
    std::fs::write(
        &host.config.host_policy_path,
        format!(
            "{policy}    allow:\n      - service: app/exporter\n        features: [host-path]\n"
        ),
    )?;
    let (code, message) = host.ask(ALICE, plan()).await?.refusal(PROMPT).await?;
    assert_eq!(code, "MM_SECURITY", "{message}");
    assert!(
        message.contains(
            "app/exporter uses host-path (volumes: /proc:/host/proc:ro in compose.yaml mounts the host's /proc read-only)"
        ),
        "{message}"
    );

    std::fs::write(
        &host.config.host_policy_path,
        format!(
            "{policy}    allow:\n      - service: app/exporter\n        features: [host-path]\n        paths: [/proc]\n"
        ),
    )?;
    host.plan_at(ALICE, &commit).await?;
    Ok(())
}
