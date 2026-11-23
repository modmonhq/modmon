//! A host path a service bind-mounts, such as `/proc` for a metrics
//! exporter: a remote host runs it recursively read-only when its policy
//! lists the path for the service under `host-path`, and refuses anything
//! else.

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
      - type: bind
        source: /proc
        target: /host/proc
        read_only: true
        bind: {recursive: readonly}
      - type: bind
        source: /sys
        target: /host/sys
        read_only: true
        bind: {recursive: readonly, propagation: rprivate}
      - data:/data
volumes:
  data: {}
";

const LISTED: &str = "      - service: app/alloy
        features: [host-path, network-host]
        paths: [/proc, /sys/]
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

fn bind(source: &str, extra: &str) -> String {
    format!(
        "      - {{type: bind, source: {source}, target: /host{source}, read_only: true{extra}}}\n"
    )
}

#[test]
fn a_listed_host_path_runs_recursively_read_only() -> TestResult {
    let temporary = project(&["alloy"], &[("compose.yaml", EXPORTER)])?;
    remote_only(temporary.path(), &["alloy"])?;
    let (status, findings) = validate(temporary.path(), &[])?;
    assert_eq!(status, 2, "{findings:?}");
    assert_eq!(lines(&findings, REFUSED).len(), 3, "{findings:?}");

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
            "app/alloy uses host-path (volumes: /proc:/host/proc in compose.yaml mounts the host's /proc read-only)",
            "app/alloy uses host-path (volumes: /sys:/host/sys in compose.yaml mounts the host's /sys read-only)",
            "app/alloy uses network-host (network_mode: host)",
        ],
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_host_path_that_is_not_recursively_read_only_or_unlisted_is_refused() -> TestResult {
    let volumes = [
        "      - /srv/data:/data:ro\n".to_owned(),
        "      - {type: bind, source: /opt/metrics, target: /metrics, bind: {recursive: readonly}}\n".to_owned(),
        bind("/srv/plain", ""),
        bind("/srv/writable", ", bind: {recursive: writable}"),
        bind("/sys", ", bind: {recursive: readonly, propagation: rslave}"),
        bind("/srv/logs", ", bind: {recursive: readonly, selinux: Z}"),
        bind("/opt", ", bind: {recursive: readonly}"),
    ]
    .concat();
    let temporary = project(
        &["probe"],
        &[(
            "compose.yaml",
            &format!("services:\n  probe:\n    image: example/probe\n    volumes:\n{volumes}"),
        )],
    )?;
    let policy = host_policy(
        temporary.path(),
        "      - service: app/probe\n        features: [host-path]\n        paths: [/srv/data, /opt/metrics, /srv/plain, /srv/writable, /sys, /srv/logs]\n",
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 2, "{findings:?}");
    let outright = "no host exception allows this: change the mount as the message says";
    let mut expected = [
        "/srv/data:/data:ro in compose.yaml mounts the host's /srv/data in the short syntax, which cannot mount it recursively read-only; write it in the long syntax with `read_only: true` and `bind.recursive: readonly`",
        "/opt/metrics:/metrics in compose.yaml mounts the host's /opt/metrics writable; add `read_only: true`",
        "/srv/plain:/host/srv/plain in compose.yaml mounts the host's /srv/plain without `bind.recursive: readonly`, so the mounts under it stay writable; add it",
        "/srv/writable:/host/srv/writable in compose.yaml mounts the host's /srv/writable with `bind.recursive: writable`; write `readonly`",
        "/sys:/host/sys in compose.yaml mounts the host's /sys with `rslave` propagation; leave propagation out",
        "/srv/logs:/host/srv/logs in compose.yaml mounts the host's /srv/logs with an SELinux relabel; remove `z` or `Z`",
    ]
    .map(|detail| format!("app/probe uses host-path (volumes: {detail}) | {outright}"))
    .to_vec();
    expected.push(
        "app/probe uses host-path (volumes: /opt:/host/opt in compose.yaml mounts the host's /opt read-only) | drop the feature, or allow it for app/probe under this project's `allow` in POLICY, with `/opt` under its `paths`".replace("POLICY", &policy),
    );
    expected.sort();
    assert_eq!(lines(&findings, REFUSED), expected, "{findings:?}");
    Ok(())
}

#[test]
fn a_host_policy_refuses_a_path_that_would_give_root() -> TestResult {
    let temporary = project(&["alloy"], &[("compose.yaml", EXPORTER)])?;
    for (path, problem) in [
        (
            "/",
            "holds or is inside /run, which would give the service root on the host",
        ),
        (
            "/var/run/docker.sock",
            "holds or is inside /var/run, which would give the service root on the host",
        ),
        (
            "/etc",
            "holds or is inside /etc/mm, which would give the service root on the host",
        ),
        (
            "/home/ops",
            "holds or is inside /home, which would give the service root on the host",
        ),
        (
            "/proc/1/root",
            "is inside /proc, which reaches other processes' files and the host's root",
        ),
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
                "project 'example' allow entry for app/alloy: host path '{path}' {problem}"
            )),
            "{stderr}"
        );
    }
    Ok(())
}

#[test]
fn a_later_file_or_extends_cannot_weaken_a_listed_mount() -> TestResult {
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
            "{LISTED}      - service: app/debug\n        features: [host-path, network-host]\n        paths: [/proc, /sys]\n"
        ),
    )?;
    let (status, findings) = validate(temporary.path(), &["--host-policy", &policy])?;
    assert_eq!(status, 2, "{findings:?}");
    let refused = lines(&findings, REFUSED);
    assert_eq!(refused.len(), 2, "{findings:?}");
    for service in ["alloy", "debug"] {
        assert!(
            refused.iter().any(|line| line.starts_with(&format!(
                "app/{service} uses host-path (volumes: /proc:/host/proc in override.yaml mounts the host's /proc in the short syntax"
            ))),
            "{findings:?}"
        );
    }
    Ok(())
}

#[test]
fn a_host_whose_engine_cannot_mount_recursively_read_only_refuses_host_paths() -> TestResult {
    let temporary = project(&["alloy"], &[("compose.yaml", EXPORTER)])?;
    let document =
        mm::adapters::config::ConfigLoader.load_from(&temporary.path().join("project"))?;
    let findings =
        mm::app::unsupported_host_path_guardrails(&document, "staging", "this host cannot")?;
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|finding| {
        !finding.grantable()
            && finding
                .to_string()
                .ends_with("; this host cannot, so no exception allows it here)")
    }));
    assert!(mm::domain::remote::recursive_read_only(
        "1.44",
        "5.12.0-100-generic"
    ));
    assert!(mm::domain::remote::recursive_read_only(
        "1.56",
        "7.2.11-200.fc44.x86_64"
    ));
    assert!(!mm::domain::remote::recursive_read_only("1.43", "6.8.0"));
    assert!(!mm::domain::remote::recursive_read_only("1.44", "5.11.22"));
    assert!(!mm::domain::remote::recursive_read_only("", ""));
    Ok(())
}

const PROC: &str = "services:\n  web:\n    image: busybox\n  exporter:\n    image: busybox\n    volumes:\n      - {type: bind, source: /proc, target: /host/proc, read_only: true, bind: {recursive: readonly}}\n";

#[tokio::test]
async fn a_host_plans_a_host_path_only_when_its_policy_lists_it() -> TestResult {
    let host = Host::with_alice().await?;
    let config = host.temporary.path().join("origin-work/.config/mm.yaml");
    let config = std::fs::read_to_string(config)?.replace(
        "      web: {stack: app}\n",
        "      web: {stack: app}\n      exporter: {stack: app}\n",
    );
    let commit = host.commit_files(&[(".config/mm.yaml", &config), ("compose.yaml", PROC)])?;
    let plan = || RemoteMethod::PlanDeployment {
        commit: commit.clone(),
        receipt: None,
        release: Default::default(),
    };
    let policy = std::fs::read_to_string(&host.config.host_policy_path)?;
    let allow = |paths: &str| {
        format!(
            "{policy}    allow:\n      - service: app/exporter\n        features: [host-path]\n{paths}"
        )
    };
    std::fs::write(&host.config.host_policy_path, allow(""))?;
    let (code, message) = host.ask(ALICE, plan()).await?.refusal(PROMPT).await?;
    assert_eq!(code, "MM_SECURITY", "{message}");
    assert!(
        message.contains(
            "app/exporter uses host-path (volumes: /proc:/host/proc in compose.yaml mounts the host's /proc read-only)"
        ),
        "{message}"
    );

    let link = host.temporary.path().join("metrics");
    std::os::unix::fs::symlink("/run", &link)?;
    std::fs::write(
        &host.config.host_policy_path,
        allow(&format!("        paths: [{}]\n", link.display())),
    )?;
    let (_, message) = host.ask(ALICE, plan()).await?.refusal(PROMPT).await?;
    assert!(
        message.contains(&format!(
            "host path '{}' resolves to /run, which holds or is inside /run",
            link.display()
        )),
        "{message}"
    );

    std::fs::write(
        &host.config.host_policy_path,
        allow("        paths: [/proc]\n"),
    )?;
    let (_, message) = host.ask(ALICE, plan()).await?.refusal(PROMPT).await?;
    assert!(
        message.contains("this host's Docker Engine cannot be relied on to mount a host path recursively read-only: that needs Docker Engine 25 (API 1.44) or later on Linux 5.12 or later, so no exception allows it here"),
        "{message}"
    );

    *host.handlers.engine.lock().map_err(|_| "poisoned")? = Some(mm::domain::remote::EngineFacts {
        runtime: mm::domain::runtime::RuntimeKind::DockerCompose,
        version: Some("Docker version 29.9.1".to_owned()),
        compose: None,
        compose_version: None,
        problem: None,
        recursive_read_only: true,
    });
    host.plan_at(ALICE, &commit).await?;
    Ok(())
}
