//! What a remote host refuses beyond the literal Compose keys: a container
//! joined by name, a `service:` join that reaches a host namespace without
//! the same exception as the service it joins, and a checked key whose value
//! comes from a variable.

mod daemon_host;
mod guardrail_project;
mod temp_dirs;

use daemon_host::{ALICE, Host, PROMPT, TestResult};
use guardrail_project::{ALLOWED, REFUSED, has, host_policy, project, validate};
use mm::domain::remote::RemoteMethod;

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
            .all(|line| line.ends_with("| no host exception allows this: write the mode itself, and `service:<service>` to join a service of the same stack")),
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
            "app/web uses container-join (pid: ${PID_MODE:-host} in compose.yaml comes from a variable; write the value itself)"
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

#[test]
fn every_checked_key_set_from_a_variable_is_refused() -> TestResult {
    let temporary = project(
        &["web", "loud"],
        &[(
            "compose.yaml",
            r#"services:
  web:
    image: example/web:${TAG:-1}
    environment: {LEVEL: "${LEVEL:-debug}"}
    privileged: ${PRIVILEGED:-true}
    cap_add: ["${CAP:-SYS_ADMIN}"]
    security_opt: ["${SECCOMP:-seccomp:unconfined}"]
    sysctls: ["${SYSCTL:-kernel.shmmax}=1", "net.core.somaxconn=${CONNECTIONS:-1024}"]
    uts: ${UTS:-host}
  loud:
    image: example/web
    privileged: "true"
volumes:
  data: {external: "${EXTERNAL:-true}"}
  bound: {driver_opts: {o: "${OPTS:-bind}"}}
  legacy: {external: {name: shared}}
  plain: {driver_opts: {size: "${SIZE:-1g}"}}
networks:
  outside: {external: "${NET_EXTERNAL:-true}"}
secrets:
  key: {file: "${KEY_FILE:-/etc/shadow}"}
"#,
        )],
    )?;
    let (status, findings) = validate(temporary.path(), &[])?;
    assert_eq!(status, 2, "{findings:?}");
    let mut reported = findings
        .iter()
        .filter_map(|line| line.strip_prefix(REFUSED))
        .map(|line| line.split(" | ").next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    reported.sort();
    let variable = |subject: &str, feature: &str, written: &str| {
        format!(
            "{subject} uses {feature} ({written} in compose.yaml comes from a variable; write the value itself)"
        )
    };
    let mut expected = vec![
        variable(
            "app/network:outside",
            "external-network",
            "external: ${NET_EXTERNAL:-true}",
        ),
        variable(
            "app/secrets:key",
            "file-outside-project",
            "secrets.key.file: ${KEY_FILE:-/etc/shadow}",
        ),
        variable(
            "app/volume:bound",
            "volume-bind",
            "driver_opts.o: ${OPTS:-bind}",
        ),
        variable(
            "app/volume:data",
            "external-volume",
            "external: ${EXTERNAL:-true}",
        ),
        "app/volume:legacy uses external-volume (external is set)".to_owned(),
        "app/loud uses privileged (privileged: true)".to_owned(),
        variable("app/web", "privileged", "privileged: ${PRIVILEGED:-true}"),
        variable("app/web", "cap-add", "cap_add: ${CAP:-SYS_ADMIN}"),
        variable(
            "app/web",
            "security-opt",
            "security_opt: ${SECCOMP:-seccomp:unconfined}",
        ),
        variable("app/web", "sysctls", "sysctls: ${SYSCTL:-kernel.shmmax}"),
        variable("app/web", "container-join", "uts: ${UTS:-host}"),
    ];
    expected.sort();
    assert_eq!(reported, expected, "{findings:?}");
    assert_eq!(findings.len(), expected.len(), "{findings:?}");
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
