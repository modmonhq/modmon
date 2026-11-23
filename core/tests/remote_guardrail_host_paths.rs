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
        source: /
        target: /host/root
        read_only: true
        bind: {propagation: rslave}
      - data:/data
volumes:
  data: {}
";

const LISTED: &str = "      - service: app/alloy
        features: [host-path, network-host]
        paths: [/proc, /sys/, /]
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
            "app/alloy uses host-path (volumes: /:/host/root in compose.yaml mounts the host's / read-only)",
            "app/alloy uses host-path (volumes: /proc:/host/proc:ro in compose.yaml mounts the host's /proc read-only)",
            "app/alloy uses host-path (volumes: /sys:/host/sys in compose.yaml mounts the host's /sys read-only)",
            "app/alloy uses network-host (network_mode: host)",
        ],
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_writable_shared_relabelled_or_unlisted_host_path_is_refused() -> TestResult {
    let temporary = project(
        &["probe"],
        &[(
            "compose.yaml",
            "services:\n  probe:\n    image: example/probe\n    volumes:\n      - /etc:/host/etc\n      - {type: bind, source: /var/lib, target: /data}\n      - /:/rootfs:ro,rshared\n      - /srv:/srv:ro,Z\n      - /opt:/opt:ro\n",
        )],
    )?;
    let policy = host_policy(
        temporary.path(),
        "      - service: app/probe\n        features: [host-path]\n        paths: [/etc, /var/lib, /, /srv]\n",
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 2, "{findings:?}");
    let outright = "no host exception allows this: change the mount as the message says";
    assert_eq!(
        lines(&findings, REFUSED),
        [
            format!("app/probe uses host-path (volumes: /:/rootfs:ro,rshared in compose.yaml mounts the host's / with shared propagation; use `rslave`, or leave propagation out) | {outright}"),
            format!("app/probe uses host-path (volumes: /etc:/host/etc in compose.yaml mounts the host's /etc writable; add `:ro`, or `read_only: true` in the long syntax) | {outright}"),
            "app/probe uses host-path (volumes: /opt:/opt:ro in compose.yaml mounts the host's /opt read-only) | drop the feature, or allow it for app/probe under this project's `allow` in POLICY, with `/opt` under its `paths`".replace("POLICY", &policy),
            format!("app/probe uses host-path (volumes: /srv:/srv:ro,Z in compose.yaml mounts the host's /srv with an SELinux relabel; remove `z` or `Z`) | {outright}"),
            format!("app/probe uses host-path (volumes: /var/lib:/data in compose.yaml mounts the host's /var/lib writable; add `:ro`, or `read_only: true` in the long syntax) | {outright}"),
        ],
        "{findings:?}"
    );
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
            "{LISTED}      - service: app/debug\n        features: [host-path, network-host]\n        paths: [/proc, /sys, /]\n"
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
