//! Production readiness: the pinned release of an environment, a
//! host's audit log that keeps itself bounded, a host's records backed up
//! and restored without a key in them, the sockets `mmd` keeps across a
//! restart, a host policy with a mistake that leaves names alone, and a
//! team sync that says a busy node was not changed, and an upgrade that
//! puts the new relay in place only as mmd restarts.

mod daemon_host;
mod temp_dirs;

use std::io::Read;
use std::mem::MaybeUninit;
use std::net::{Ipv4Addr, SocketAddr};
use std::os::unix::net::UnixDatagram;

use daemon_host::{ALICE, Host, PROMPT, TestResult, project};
use mm::adapters::kept_sockets::KeptSockets;
use mm::adapters::remote::{AuditLog, export_state, import_state};
use mm::domain::remote::{ClusterApplyReport, NodeApplyOutcome, NodeApplyState, RemoteMethod};
use mm::domain::version_pin::{VersionPin, parse, release_diagnostics};

fn pin(requirement: &str) -> VersionPin {
    VersionPin {
        environment: "production".to_owned(),
        requirement: requirement.to_owned(),
    }
}

#[test]
fn a_pinned_environment_takes_only_its_release_of_mm_and_mmd() -> TestResult {
    let minor = pin("0.9");
    assert!(minor.allows("0.9.0")? && minor.allows("0.9.7")?);
    assert!(!minor.allows("0.10.0")? && !minor.allows("0.8.9")?);
    assert!(!minor.allows("not a version")?);
    assert!(pin("=0.9.3").allows("0.9.3")? && !pin("=0.9.3").allows("0.9.4")?);

    let client = minor
        .check_client("0.10.1")
        .err()
        .ok_or("a client outside the pin was taken")?;
    for needle in [
        "environment 'production' pins mm 0.9",
        "environments.production.deploy.mm-version",
        "this mm is 0.10.1",
        "--version vMAJOR.MINOR.PATCH",
    ] {
        assert!(client.contains(needle), "{needle:?} is not in: {client}");
    }
    let host = minor
        .check_host("node 'app-2'", Some("0.8.3"))
        .err()
        .ok_or("a host outside the pin was taken")?;
    for needle in [
        "node 'app-2' runs mmd 0.8.3",
        "pins 0.9",
        "mm target setup plan",
    ] {
        assert!(host.contains(needle), "{needle:?} is not in: {host}");
    }
    assert!(
        minor.check_host("node 'app-2'", None).is_err(),
        "an unknown mmd is not the pinned one"
    );
    minor.check_client("0.9.2")?;

    let refused = parse("nine")
        .err()
        .ok_or("a requirement that is no version was taken")?;
    assert!(refused.contains("write a release such as 0.9"), "{refused}");
    Ok(())
}

#[test]
fn doctor_names_the_hosts_outside_the_pin_and_the_ones_on_another_release() {
    let hosts = vec![
        ("node 'app-1'".to_owned(), Some("0.9.4".to_owned())),
        ("node 'app-2'".to_owned(), Some("0.9.2".to_owned())),
        ("node 'app-3'".to_owned(), Some("0.10.0".to_owned())),
    ];
    let found = release_diagnostics(Some(&pin("0.9")), "0.9.4", &hosts);
    let codes = found
        .iter()
        .map(|found| found.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        [
            "MM_VERSION_PIN",
            "MM_HOST_RELEASE_OLDER",
            "MM_HOST_RELEASE_NEWER"
        ]
    );
    assert!(
        found[0].message.contains("node 'app-3' runs mmd 0.10.0"),
        "{found:?}"
    );
    assert!(
        found[1].message.contains("node 'app-2' (mmd 0.9.2)"),
        "{found:?}"
    );
    assert!(
        found[1]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("mm target setup plan")),
        "{found:?}"
    );
    // Without a pin only the releases are compared, and one release is quiet.
    assert!(release_diagnostics(None, "0.9.4", &hosts[..1]).is_empty());
    assert_eq!(release_diagnostics(None, "0.9.4", &hosts).len(), 2);
}

#[test]
fn the_pin_is_read_from_the_environments_deploy_settings() -> TestResult {
    let config: mm::domain::config::MmConfig = serde_yaml_ng::from_str(&format!(
        "schema-version: 1\nproject:\n  id: {}\n  name: example\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nenvironments:\n  production:\n    deploy: {{mm-version: \"0.9\"}}\n  staging: {{}}\n",
        project().0
    ))?;
    assert_eq!(VersionPin::of(&config, "production"), Some(pin("0.9")));
    assert_eq!(VersionPin::of(&config, "staging"), None);
    Ok(())
}

#[test]
fn the_audit_log_rotates_by_size_and_keeps_a_bounded_history() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let log = AuditLog::with_limits(directory.path(), 300, 2);
    for number in 0..40_i64 {
        log.append(&serde_json::json!({ "timestamp_unix_ms": number, "event": "test" }))?;
    }
    let names = log
        .files()
        .iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(names, ["audit.jsonl", "audit.jsonl.1", "audit.jsonl.2"]);
    assert!(
        !directory.path().join("audit.jsonl.3").exists(),
        "the oldest is dropped"
    );
    for path in log.files() {
        assert!(
            std::fs::metadata(&path)?.len() <= 300,
            "{} is over its size",
            path.display()
        );
    }
    // The newest records, oldest first, across files.
    let last = log.read_last(5)?;
    let numbers = last
        .iter()
        .filter_map(|record| record["timestamp_unix_ms"].as_i64())
        .collect::<Vec<_>>();
    assert_eq!(numbers, [35, 36, 37, 38, 39]);
    // Every record kept is whole.
    let kept = log.read_last(10_000)?;
    assert!(kept.len() > 5 && kept.len() < 40, "{}", kept.len());
    assert!(log.read_last(0).is_err() && log.read_last(10_001).is_err());
    Ok(())
}

/// The names of an archive's entries, in order.
fn entries(archive: &[u8]) -> TestResult<Vec<String>> {
    let mut names = Vec::new();
    for entry in tar::Archive::new(archive).entries()? {
        names.push(entry?.path()?.to_string_lossy().into_owned());
    }
    Ok(names)
}

/// A host whose state directory also holds every kind of file an export
/// must leave out.
async fn host_with_keys() -> TestResult<Host> {
    let host = Host::with_alice().await?;
    let state = &host.config.state_root;
    let projects = state.join("projects").join(project().0.to_string());
    std::fs::write(&host.config.node_env_path, "MM_NODE_NAME=app-1\n")?;
    for (path, contents) in [
        (state.join("audit.jsonl.1"), "{\"timestamp_unix_ms\":1}\n"),
        (state.join("networks/networks.json"), "{}\n"),
        (
            state.join("mesh/private.key"),
            "a stand-in for a private key\n",
        ),
        (state.join("mesh/mesh.json"), "{}\n"),
        (
            projects.join("deploy-key"),
            "a stand-in for a private key\n",
        ),
        (projects.join("deployments/production.json"), "{}\n"),
        (projects.join("repo.git/HEAD"), "ref: refs/heads/main\n"),
        (state.join("storage/data/file"), "user data\n"),
    ] {
        std::fs::create_dir_all(path.parent().ok_or("no parent")?)?;
        std::fs::write(path, contents)?;
    }
    Ok(host)
}

#[tokio::test]
async fn a_hosts_records_are_exported_without_a_key_and_restored_on_the_same_node() -> TestResult {
    let host = host_with_keys().await?;
    let mut archive = Vec::new();
    let manifest = export_state(&host.config, &mut archive, 1_700_000_000_000)?;
    assert_eq!(manifest.node.as_deref(), Some("app-1"));
    let names = entries(&archive)?;
    assert_eq!(names[0], "mm-state.json");
    let id = project().0;
    for expected in [
        "host.yaml".to_owned(),
        "node.env".to_owned(),
        "state/audit.jsonl.1".to_owned(),
        "state/networks/networks.json".to_owned(),
        format!("state/projects/{id}/access/production.json"),
        format!("state/projects/{id}/deployed/production.json"),
        format!("state/projects/{id}/deployments/production.json"),
    ] {
        assert!(names.contains(&expected), "{expected} is not in {names:?}");
    }
    for name in &names {
        for never in [
            "key", "identity", "mesh", "repo.git", "storage", "plans", "claims", "requests",
            "seals",
        ] {
            assert!(!name.contains(never), "{name} is in the export");
        }
    }
    let text = String::from_utf8_lossy(&archive);
    assert!(!text.contains("a stand-in for a private key") && !text.contains("user data"));

    // The same node, rebuilt: its policy and records come back. The old
    // host and its checkout are gone.
    let access = format!("projects/{id}/access/production.json");
    let policy = std::fs::read(&host.config.host_policy_path)?;
    let synced = std::fs::read(host.config.state_root.join(&access))?;
    drop(host);
    let rebuilt = Host::new()?;
    std::fs::write(&rebuilt.config.node_env_path, "MM_NODE_NAME=app-1\n")?;
    std::fs::write(
        &rebuilt.config.host_policy_path,
        "schema-version: 1\nprojects: {}\n",
    )?;
    let imported = import_state(&rebuilt.config, archive.as_slice())?;
    assert!(
        imported.restored.contains(&"host.yaml".to_owned()),
        "{imported:?}"
    );
    assert_eq!(std::fs::read(&rebuilt.config.host_policy_path)?, policy);
    assert_eq!(
        std::fs::read(rebuilt.config.state_root.join(&access))?,
        synced
    );
    // Its deployed record names a checkout this host does not have.
    assert!(
        imported
            .skipped
            .iter()
            .any(|skipped| skipped.contains("deployed/production.json")
                && skipped.contains("is not on this host")),
        "{imported:?}"
    );
    assert!(!rebuilt.config.state_root.join("mesh/private.key").exists());
    Ok(())
}

#[tokio::test]
async fn an_import_refuses_another_nodes_archive_and_anything_an_export_never_holds() -> TestResult
{
    let host = host_with_keys().await?;
    let mut archive = Vec::new();
    export_state(&host.config, &mut archive, 1)?;

    let other = Host::new()?;
    std::fs::write(&other.config.node_env_path, "MM_NODE_NAME=app-2\n")?;
    let before = std::fs::read(&other.config.host_policy_path)?;
    let refused = import_state(&other.config, archive.as_slice())
        .err()
        .ok_or("another node's archive was imported")?
        .to_string();
    assert!(
        refused.contains("of node 'app-1'") && refused.contains("node 'app-2'"),
        "{refused}"
    );
    assert_eq!(
        std::fs::read(&other.config.host_policy_path)?,
        before,
        "nothing was changed"
    );

    // An archive with a key added to it.
    let mut tampered = tar::Builder::new(Vec::new());
    let mut source = tar::Archive::new(archive.as_slice());
    for entry in source.entries()? {
        let mut entry = entry?;
        let header = entry.header().clone();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        tampered.append(&header, bytes.as_slice())?;
    }
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_mode(0o600);
    tampered.append_data(&mut header, "state/daemon.identity", &b"key\n"[..])?;
    let tampered = tampered.into_inner()?;
    std::fs::write(&other.config.node_env_path, "MM_NODE_NAME=app-1\n")?;
    let refused = import_state(&other.config, tampered.as_slice())
        .err()
        .ok_or("an archive with a key in it was imported")?
        .to_string();
    assert!(
        refused.contains("state/daemon.identity") && refused.contains("never"),
        "{refused}"
    );
    assert_eq!(
        std::fs::read(&other.config.host_policy_path)?,
        before,
        "nothing was changed"
    );
    Ok(())
}

#[tokio::test]
async fn an_import_is_refused_while_mmd_answers() -> TestResult {
    let host = Host::new()?;
    let mut archive = Vec::new();
    export_state(&host.config, &mut archive, 1)?;
    let _listening = std::os::unix::net::UnixListener::bind(&host.config.socket_path)?;
    let refused = import_state(&host.config, archive.as_slice())
        .err()
        .ok_or("an import ran beside a running mmd")?
        .to_string();
    assert!(refused.contains("systemctl stop mmd"), "{refused}");
    Ok(())
}

/// One notification the service manager received, and the descriptor
/// that came with it.
fn received(store: &UnixDatagram) -> TestResult<(String, Option<std::os::fd::OwnedFd>)> {
    let mut text = [0_u8; 512];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = rustix::net::RecvAncillaryBuffer::new(&mut space);
    let message = rustix::net::recvmsg(
        store,
        &mut [std::io::IoSliceMut::new(&mut text)],
        &mut control,
        rustix::net::RecvFlags::empty(),
    )?;
    let mut descriptor = None;
    for item in control.drain() {
        if let rustix::net::RecvAncillaryMessage::ScmRights(mut descriptors) = item {
            descriptor = descriptors.next();
        }
    }
    Ok((
        String::from_utf8_lossy(&text[..message.bytes]).into_owned(),
        descriptor,
    ))
}

#[test]
fn every_socket_mmd_listens_on_is_handed_to_the_service_manager_and_taken_back() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let notify = directory.path().join("notify");
    let store = UnixDatagram::bind(&notify)?;
    store.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let sockets = KeptSockets::to(notify);

    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let datagrams = sockets.udp(address)?;
    let bound = datagrams.local_addr()?;
    let (text, descriptor) = received(&store)?;
    assert_eq!(
        text,
        format!("FDSTORE=1\nFDNAME=udp-127.0.0.1-{}\n", bound.port())
    );
    let kept = std::net::UdpSocket::from(descriptor.ok_or("no descriptor came with it")?);
    assert_eq!(kept.local_addr()?, bound, "the store holds the same socket");

    let streams = sockets.tcp(address)?;
    let (text, descriptor) = received(&store)?;
    assert_eq!(
        text,
        format!(
            "FDSTORE=1\nFDNAME=tcp-127.0.0.1-{}\n",
            streams.local_addr()?.port()
        )
    );
    assert!(descriptor.is_some());

    sockets.forget(bound);
    let (text, descriptor) = received(&store)?;
    assert_eq!(
        text,
        format!("FDSTOREREMOVE=1\nFDNAME=udp-127.0.0.1-{}\n", bound.port())
    );
    assert!(descriptor.is_none());
    let (text, _) = received(&store)?;
    assert!(text.starts_with("FDSTOREREMOVE=1\nFDNAME=tcp-"), "{text}");

    // Outside a service manager nothing is kept, and a socket is bound all
    // the same.
    let alone = KeptSockets::none().udp(address)?;
    assert!(alone.local_addr()?.port() > 0);
    drop((datagrams, kept, streams));
    Ok(())
}

#[tokio::test]
async fn a_host_policy_with_a_mistake_leaves_the_daemon_running_and_refuses_requests() -> TestResult
{
    let host = Host::with_alice().await?;
    let policy = std::fs::read_to_string(&host.config.host_policy_path)?;
    std::fs::write(
        &host.config.host_policy_path,
        format!("{policy}\nprojects: [oops\n"),
    )?;
    // A daemon started on the broken file starts: names and the node's own
    // work do not wait for the policy.
    let started = host.start_again();
    assert!(started.is_ok(), "{:?}", started.err());
    // Every request is refused with the file, the mistake, and the fix.
    let (code, message) = host
        .ask(ALICE, RemoteMethod::WhoAmI)
        .await?
        .refusal(PROMPT)
        .await?;
    assert_eq!(code, "MM_CONFIG_PARSE");
    for needle in [
        "host.yaml",
        "refuses every request until",
        "the containers and their names are not affected",
        "mmd access validate",
    ] {
        assert!(message.contains(needle), "{needle:?} is not in: {message}");
    }
    // Fixed, the next request is answered again without a restart.
    std::fs::write(&host.config.host_policy_path, policy)?;
    host.ask(ALICE, RemoteMethod::WhoAmI)
        .await?
        .result(PROMPT)
        .await?;
    Ok(())
}

fn outcome(node: &str, state: NodeApplyState, error: Option<&str>) -> NodeApplyOutcome<()> {
    NodeApplyOutcome {
        node: node.to_owned(),
        state,
        result: None,
        error: error.map(str::to_owned),
        rolled_back: false,
        log: Vec::new(),
        stopped: Vec::new(),
        unchanged: false,
        code: None,
        kept: false,
    }
}

#[test]
fn a_node_that_was_too_busy_for_a_change_is_listed_as_not_changed() -> TestResult {
    let report = ClusterApplyReport {
        plan_hash: "a".repeat(64),
        target: "production".to_owned(),
        environment: "production".to_owned(),
        nodes: vec![
            outcome(
                "app-1",
                NodeApplyState::NotStarted,
                Some(
                    "operator's deployment has been changing this host since 10:00. This request waited 28 s for it and changed nothing",
                ),
            ),
            outcome("app-2", NodeApplyState::NotStarted, None),
        ],
        skipped: Vec::new(),
    };
    assert!(!report.succeeded());
    let message = report
        .failure("mm team sync")
        .ok_or("a sync refused by a busy node counts as done")?;
    assert!(
        message.contains("failed on node 'app-1'") && message.contains("changed nothing"),
        "{message}"
    );
    assert!(message.contains("Not changed: app-1, app-2"), "{message}");
    assert!(!message.contains("Changed and left"), "{message}");
    Ok(())
}

/// Every SSH session runs the relay (`/usr/bin/mm`), which refuses a daemon
/// of another remote protocol. On the VMs the access playbook installed the
/// new relay about thirty seconds before it restarted the old `mmd`, and the
/// node refused every client meanwhile ("restricted relay rejected a daemon
/// protocol version"). The relay now takes its place only right before the
/// restart.
#[test]
fn setup_puts_the_new_relay_in_place_only_right_before_mmd_restarts() -> TestResult {
    fn copies_to(task: &serde_yaml_ng::Value, path: &str) -> bool {
        match task {
            serde_yaml_ng::Value::Mapping(map) => map.iter().any(|(key, value)| {
                (key.as_str() == Some("dest") && value.as_str() == Some(path))
                    || copies_to(value, path)
            }),
            serde_yaml_ng::Value::Sequence(items) => items.iter().any(|item| copies_to(item, path)),
            _ => false,
        }
    }
    let playbook: serde_yaml_ng::Value = serde_yaml_ng::from_str(include_str!(
        "../src/adapters/target/ansible_bootstrap/access.yml"
    ))?;
    let tasks = playbook[0]["tasks"]
        .as_sequence()
        .ok_or("the access playbook has no tasks")?;
    let name = |task: &serde_yaml_ng::Value| task["name"].as_str().unwrap_or_default().to_owned();
    let start = tasks
        .iter()
        .position(|task| name(task) == "Start mmd")
        .ok_or("no task starts mmd")?;
    let relay = start.checked_sub(1).ok_or("Start mmd is the first task")?;
    assert_eq!(name(&tasks[relay]), "Put the new relay in place");
    let writers = tasks
        .iter()
        .filter(|task| copies_to(task, "/usr/bin/mm"))
        .map(name)
        .collect::<Vec<_>>();
    assert_eq!(writers, ["Put the new relay in place"]);
    let state = tasks[start]["ansible.builtin.systemd_service"]["state"]
        .as_str()
        .ok_or("Start mmd has no state")?;
    assert!(state.contains("mm_relay.changed"), "{state}");
    Ok(())
}
