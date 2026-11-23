//! Rootful Podman on remote hosts: the network netavark is asked for on a
//! cluster node and how it is read back, the containers on it, the
//! engine's numbers, what a node says about its engine, which engine a node
//! records, and the user namespace a service keeps.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use mm::adapters::config::ConfigLoader;
use mm::adapters::remote::DaemonConfig;
use mm::adapters::runtime::PodmanComposeRuntime;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    CommandOutput, CommandSpec, LocalDeploymentRuntime, ProcessRunner, RuntimeDriver, TargetStore,
};
use mm::app::{StackManager, WorkspaceSelection};
use mm::domain::cluster_network::LocalNetwork;
use mm::domain::diagnostic::Diagnostic;
use mm::domain::error::MmError;
use mm::domain::ownership::{ResourceKind, ResourceOwnership};
use mm::domain::remote::{EngineFacts, NodeReport, NodeState, engine_diagnostics};
use mm::domain::runtime::{ResourceEnsureResult, RuntimeKind};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// An engine command line that answers from a script, one answer per
/// command, and keeps what it was asked.
#[derive(Default)]
struct Podman {
    answers: Mutex<VecDeque<(i32, String)>>,
    commands: Mutex<Vec<String>>,
}

impl Podman {
    fn answering(answers: Vec<(i32, String)>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into()),
            commands: Mutex::default(),
        })
    }

    fn commands(&self) -> Result<Vec<String>, String> {
        Ok(self.commands.lock().map_err(|_| "poisoned")?.clone())
    }
}

#[async_trait::async_trait]
impl ProcessRunner for Podman {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let poisoned = || MmError::State {
            message: "the test engine's lock was poisoned".to_owned(),
        };
        let record = specification.redacted_record();
        self.commands.lock().map_err(|_| poisoned())?.push(format!(
            "{} {}",
            record.program,
            record.arguments.join(" ")
        ));
        let (code, stdout) = self
            .answers
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: format!("the test engine has no answer for `{}`", record.program),
            })?;
        Ok(CommandOutput {
            code: Some(code),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

fn local_target() -> NamedTarget {
    NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(7))),
    )
}

fn backend() -> Result<LocalNetwork, String> {
    Ok(LocalNetwork {
        key: "0123456789ab-production-backend".to_owned(),
        network: "backend".to_owned(),
        bridge: "mmn-0123456789a".to_owned(),
        block: "10.64.0.0/20".parse()?,
        slice: "10.64.1.0/24".parse()?,
        gateway: std::net::Ipv4Addr::new(10, 64, 1, 1),
        internal: false,
        mtu: 1420,
        mesh_interface: "mm0".to_owned(),
    })
}

/// `podman network inspect backend` as Podman 5 prints it, owned by the
/// local target, with DNS on or off.
fn inspected(dns: bool, end: &str) -> String {
    let labels = ResourceOwnership {
        project_id: None,
        environment: None,
        target_id: TargetId(uuid::Uuid::from_u128(7)),
        kind: ResourceKind::Network,
        logical_name: "backend".to_owned(),
        resource: "network".to_owned(),
        generation: mm::domain::ownership::OWNERSHIP_GENERATION,
    }
    .labels();
    serde_json::json!([{
        "name": "backend",
        "driver": "bridge",
        "network_interface": "mmn-0123456789a",
        "subnets": [{
            "subnet": "10.64.0.0/20",
            "gateway": "10.64.1.1",
            "lease_range": {"start_ip": "10.64.1.2", "end_ip": end},
        }],
        "ipv6_enabled": false,
        "internal": false,
        "dns_enabled": dns,
        "labels": labels,
        "options": {"mtu": "1420"},
        "ipam_options": {"driver": "host-local"},
    }])
    .to_string()
}

#[tokio::test]
async fn a_cluster_network_on_podman_is_a_netavark_bridge_whose_dns_mmd_answers() -> TestResult {
    let podman = Podman::answering(vec![
        // Not there yet: created.
        (1, String::new()),
        (0, String::new()),
        // There, owned, and as wanted: left alone.
        (0, String::new()),
        (0, inspected(false, "10.64.1.254")),
        (0, inspected(false, "10.64.1.254")),
    ]);
    let runtime = PodmanComposeRuntime::new(podman.clone());
    let network = backend()?;
    for expected in [
        ResourceEnsureResult::Created,
        ResourceEnsureResult::Unchanged,
    ] {
        assert_eq!(
            runtime
                .ensure_cluster_network(&local_target(), "backend", "network", &network)
                .await?,
            expected
        );
    }
    let commands = podman.commands()?;
    let create = &commands[1];
    for part in [
        "podman network create --driver bridge --subnet 10.64.0.0/20 --ip-range 10.64.1.2-10.64.1.254 --gateway 10.64.1.1",
        "--interface-name mmn-0123456789a",
        // aardvark-dns would take the gateway's port 53, which mmd answers.
        "--disable-dns",
        "--opt mtu=1420",
        "--label io.mm.resource=network",
    ] {
        assert!(create.contains(part), "{part} is not in: {create}");
    }
    assert!(create.ends_with(" backend"), "{create}");
    assert_eq!(commands.len(), 5, "an unchanged network gets no command");
    Ok(())
}

/// `mm doctor` asks Podman which binary a runtime's name stands for before
/// it runs a container under it: Podman has no list of runtimes like
/// Docker Engine's, it resolves the name through `[engine.runtimes]`.
#[tokio::test]
async fn a_runtime_podman_cannot_resolve_is_named_before_any_container_runs() -> TestResult {
    let known = Podman::answering(vec![
        (
            0,
            r#"{"name":"crun","package":"crun_1.21-1_amd64","path":"/usr/bin/crun","version":"crun version 1.21"}"#
                .to_owned(),
        ),
        (0, String::new()),
    ]);
    PodmanComposeRuntime::new(known.clone())
        .check_oci_runtime("crun")
        .await?;
    let commands = known.commands()?;
    assert_eq!(commands.len(), 2, "{commands:?}");
    assert!(
        commands[0].ends_with("--runtime crun info --format {{json .Host.OCIRuntime}}"),
        "{commands:?}"
    );
    assert!(
        commands[1].ends_with("run --rm --runtime crun --network none hello-world"),
        "{commands:?}"
    );

    let missing = Podman::answering(vec![(125, String::new())]);
    let problem = PodmanComposeRuntime::new(missing.clone())
        .check_oci_runtime("runsc")
        .await
        .err()
        .ok_or("a runtime Podman does not know passed")?;
    for part in [
        "Podman on this host has no runtime named 'runsc'",
        "[engine.runtimes]",
        "/etc/mm/host.yaml",
    ] {
        assert!(problem.contains(part), "{part}: {problem}");
    }
    assert_eq!(missing.commands()?.len(), 1, "no container ran");
    Ok(())
}

#[tokio::test]
async fn a_podman_network_with_its_dns_on_or_another_range_is_not_the_cluster_network() -> TestResult
{
    let podman = Podman::answering(vec![
        (0, String::new()),
        (0, inspected(false, "10.64.1.254")),
        (0, String::new()),
        (0, inspected(true, "10.64.1.254")),
        (0, String::new()),
        (0, inspected(false, "10.64.1.255")),
        (1, String::new()),
    ]);
    let runtime = PodmanComposeRuntime::new(podman.clone());
    let wanted = backend()?;
    let observed = runtime
        .observe_cluster_networks(&[wanted.clone(), wanted.clone(), wanted.clone()])
        .await?;
    let seen = observed
        .iter()
        .map(|network| {
            network
                .as_ref()
                .map(|network| (network.current, network.addresses.clone()))
        })
        .collect::<Vec<_>>();
    let range = "10.64.0.0/20, range 10.64.1.2-10.64.1.254".to_owned();
    assert_eq!(
        seen,
        [
            Some((true, range.clone())),
            Some((false, range)),
            Some((
                false,
                "10.64.0.0/20, range 10.64.1.2-10.64.1.255".to_owned()
            )),
        ]
    );
    assert_eq!(
        runtime
            .observe_cluster_network("backend", Some(&wanted))
            .await?,
        None
    );
    Ok(())
}

#[tokio::test]
async fn the_containers_on_a_podman_network_are_named_by_their_aliases() -> TestResult {
    // Podman prints no slash before a name and no `DNSNames`; Compose's
    // service name is among the aliases.
    let inspect = serde_json::json!([{
        "Id": "56e277dd8c7ca2d3acb016c3264fa9360cf0c93c1b8c1ee90047bd009fc6ee63",
        "Name": "shop-api-1",
        "State": {"Status": "running", "Health": {"Status": "healthy"}},
        "Config": {"Labels": {"com.docker.compose.service": "api"}},
        "NetworkSettings": {"Networks": {"backend": {
            "IPAddress": "10.64.1.6",
            "Aliases": ["shop-api-1", "api", "56e277dd8c7c"],
        }}},
    }])
    .to_string();
    let podman = Podman::answering(vec![
        (
            0,
            "56e277dd8c7ca2d3acb016c3264fa9360cf0c93c1b8c1ee90047bd009fc6ee63\n".to_owned(),
        ),
        (0, inspect),
    ]);
    let runtime = PodmanComposeRuntime::new(podman.clone());
    let containers = runtime.network_containers(&["backend".to_owned()]).await?;
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].name, "shop-api-1");
    assert_eq!(containers[0].service.as_deref(), Some("api"));
    assert_eq!(
        containers[0].attachments[0].address.to_string(),
        "10.64.1.6"
    );
    assert!(
        containers[0].attachments[0]
            .names
            .contains(&"api".to_owned())
    );
    assert_eq!(
        podman.commands()?[0],
        "podman ps --all --quiet --no-trunc --filter network=backend"
    );
    Ok(())
}

#[tokio::test]
async fn podmans_own_numbers_are_read_like_dockers() -> TestResult {
    let id = "ddb312acc383e69840f2e71ba04ec64107329f5059018912a202b5f7c826889c";
    let inspect = serde_json::json!([{"Id": id, "HostConfig": {"NanoCpus": 0, "Memory": 0}}]);
    let stats = serde_json::json!({
        "ContainerID": id, "Name": "shop-api-1", "CPU": 20.31, "MemPerc": 0.0028, "PIDs": 1,
    });
    let podman = Podman::answering(vec![(0, inspect.to_string()), (0, stats.to_string())]);
    let runtime = PodmanComposeRuntime::new(podman);
    let samples = runtime.container_usage(&[id[..12].to_owned()]).await?;
    assert_eq!(samples.len(), 1, "{samples:?}");
    assert!(
        samples[0].cpu.is_some() && samples[0].memory.is_some(),
        "{samples:?}"
    );
    Ok(())
}

fn report(name: &str, engine: Option<EngineFacts>) -> NodeReport {
    NodeReport {
        name: name.to_owned(),
        host: "203.0.113.11".to_owned(),
        port: 22,
        repo_address: None,
        mesh_address: None,
        state: NodeState::Ready,
        reachable: true,
        problem: None,
        mmd_version: Some("0.1.0".to_owned()),
        deployed: None,
        rollout: None,
        engine,
        daemon: None,
        secret_store: None,
    }
}

fn podman(version: &str, compose: Option<(&str, &str)>) -> EngineFacts {
    EngineFacts {
        runtime: RuntimeKind::PodmanCompose,
        version: Some(version.to_owned()),
        compose: compose.map(|(name, _)| name.to_owned()),
        compose_version: compose.map(|(_, version)| version.to_owned()),
        problem: None,
    }
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

#[test]
fn doctor_names_an_old_podman_and_a_compose_provider_mm_cannot_drive() {
    let docker_compose = Some(("docker-compose", "Docker Compose version 2.26.1-4"));
    let fine = [
        report(
            "app-1",
            Some(podman("podman version 5.4.2", docker_compose)),
        ),
        report("app-2", Some(podman("5.8.9", docker_compose))),
        report("app-3", None),
    ];
    assert!(engine_diagnostics(&fine).is_empty());

    let old = [report(
        "app-1",
        Some(podman("podman version 4.9.3", docker_compose)),
    )];
    let found = engine_diagnostics(&old);
    assert_eq!(codes(&found), ["MM_PODMAN_VERSION"]);
    assert!(
        found[0].message.contains("'app-1' runs Podman 4.9"),
        "{found:?}"
    );
    assert!(
        found[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("mm target setup plan")),
        "{found:?}"
    );

    let python = [report(
        "app-2",
        Some(podman(
            "podman version 5.4.2",
            Some(("podman-compose", "podman-compose version 1.3.0")),
        )),
    )];
    let found = engine_diagnostics(&python);
    assert_eq!(codes(&found), ["MM_PODMAN_COMPOSE"]);
    assert!(
        found[0]
            .message
            .contains("runs podman-compose version 1.3.0"),
        "{found:?}"
    );
    assert!(
        found[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("apt-get install docker-compose")
                && help.contains("systemctl enable --now podman.socket")),
        "{found:?}"
    );
    let none = [report("app-2", Some(podman("podman version 5.4.2", None)))];
    assert!(
        engine_diagnostics(&none)[0]
            .message
            .contains("has no Compose provider")
    );

    let down = EngineFacts {
        problem: Some("Cannot connect to Podman".to_owned()),
        ..podman("podman version 5.4.2", docker_compose)
    };
    let found = engine_diagnostics(&[report("app-3", Some(down))]);
    assert_eq!(codes(&found), ["MM_NODE_ENGINE"]);
    assert!(
        found[0]
            .message
            .contains("podman-compose engine of 'app-3' cannot be used")
    );

    // Docker Engine brings its own Compose: only an engine that does not
    // answer is reported.
    let docker = EngineFacts {
        runtime: RuntimeKind::DockerCompose,
        version: Some("Docker version 29.0.1, build abc".to_owned()),
        compose: None,
        compose_version: None,
        problem: None,
    };
    assert!(engine_diagnostics(&[report("app-1", Some(docker))]).is_empty());
}

/// The terminal UI's Environments view lists each node with its engine,
/// from the same report `mm target nodes --output json` prints.
#[test]
fn the_terminal_ui_names_each_nodes_engine() {
    let line = mm::presentation::tui::NodeLine::from(&report(
        "app-1",
        Some(podman(
            "podman version 5.4.2",
            Some(("docker-compose", "2.26.1")),
        )),
    ));
    assert_eq!(line.detail, "mmd 0.1.0  nothing deployed  Podman 5.4");
    let mut silent = podman("podman version 5.4.2", None);
    silent.problem = Some("the engine does not answer".to_owned());
    assert!(
        mm::presentation::tui::NodeLine::from(&report("app-2", Some(silent)))
            .detail
            .ends_with("Podman not answering")
    );
    // A node of an older mmd says nothing about its engine.
    assert_eq!(
        mm::presentation::tui::NodeLine::from(&report("app-3", None)).detail,
        "mmd 0.1.0  nothing deployed"
    );
}

#[test]
fn a_nodes_release_is_read_from_the_way_engines_name_themselves() {
    for (version, release) in [
        ("podman version 5.4.2", Some((5, 4))),
        ("5.8.9", Some((5, 8))),
        ("Docker version 29.0.1, build dfc520d", Some((29, 0))),
        ("podman version 4.9.3-dev", Some((4, 9))),
        ("podman version", None),
    ] {
        assert_eq!(podman(version, None).release(), release, "{version}");
    }
}

#[test]
fn a_node_records_the_engine_setup_installed_and_hosts_before_it_run_docker() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let config = |node_env: &Path| -> Result<DaemonConfig, Box<dyn std::error::Error>> {
        let config: DaemonConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "socket_path": "/run/mm/mm.sock",
            "state_root": temporary.path().join("state"),
            "mm_home": temporary.path().join("home"),
            "socket_uid": 0,
            "socket_gid": 0,
            "idle_timeout_ms": 30000,
            "host_policy_path": temporary.path().join("host.yaml"),
            "relay_uid": 0,
            "node_env_path": node_env,
        }))?;
        config.validate()?;
        Ok(config)
    };
    let node_env = temporary.path().join("node.env");
    std::fs::write(
        &node_env,
        "MM_NODE_NAME=app-1\nMM_PRIVATE_INTERFACE=mm0\nMM_PRIVATE_ADDRESS=10.90.0.1\nMM_RUNTIME=podman-compose\n",
    )?;
    let podman_node = config(&node_env)?;
    assert_eq!(
        podman_node.node_env()?.and_then(|node| node.runtime),
        Some(RuntimeKind::PodmanCompose)
    );
    assert_eq!(podman_node.host_runtime(), RuntimeKind::PodmanCompose);

    std::fs::write(
        &node_env,
        "MM_NODE_NAME=app-1\nMM_PRIVATE_ADDRESS=10.90.0.1\n",
    )?;
    assert_eq!(
        config(&node_env)?.host_runtime(),
        RuntimeKind::DockerCompose
    );

    std::fs::write(&node_env, "MM_NODE_NAME=app-1\nMM_RUNTIME=docker-swarm\n")?;
    assert!(config(&node_env)?.node_env().is_err());
    Ok(())
}

const USERNS_CONFIG: &str = "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1df0
  name: shop
default-environment: production
target: local
stacks:
  app:
    files: [compose.yaml]
    shared-volumes: [uploads]
modules:
  shop:
    path: .
    services:
      web: {stack: app, placement: all}
      api: {stack: app}
environments:
  production: {target: production, bindings: {}, runtime: {driver: podman-compose}}
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
";

const USERNS_COMPOSE: &str = "services:
  web:
    image: example/web
    volumes: [\"uploads:/data\"]
  api:
    image: example/api
volumes:
  uploads: {}
";

/// What `mm config validate` says about user namespaces.
fn user_namespace_findings(config: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    user_namespace_findings_with(config, &[])
}

/// The same for a project that also holds `files` (path, content).
fn user_namespace_findings_with(
    config: &str,
    files: &[(&str, &str)],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::write(root.join(".config/mm.yaml"), config)?;
    std::fs::write(root.join("compose.yaml"), USERNS_COMPOSE)?;
    for (path, content) in files {
        std::fs::write(root.join(path), content)?;
    }
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(&root)
        .arg("--mm-home")
        .arg(temporary.path().join("home"))
        .args(["--output", "json", "config", "validate"])
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "mm config validate printed no report ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    Ok(report["validation"]["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == "MM_USER_NAMESPACE")
        .filter_map(|diagnostic| diagnostic["message"].as_str().map(str::to_owned))
        .collect())
}

/// On Podman each service has user ids of its own, the same on every
/// node: one service uses a volume, shared or not, and a volume two
/// services mount needs the host's ids on every one of them (Podman gives
/// a new volume to the first container that mounts it).
#[test]
fn a_volume_two_services_with_ids_of_their_own_mount_is_refused() -> TestResult {
    assert!(user_namespace_findings(USERNS_CONFIG)?.is_empty());
    let both = "services:
  web:
    image: example/web
    volumes: [\"uploads:/data\"]
  api:
    image: example/api
    volumes: [{type: volume, source: uploads, target: /data, read_only: true}]
volumes:
  uploads: {}
";
    let found = user_namespace_findings_with(USERNS_CONFIG, &[("compose.yaml", both)])?;
    assert_eq!(found.len(), 1, "{found:?}");
    for part in [
        "services 'api', 'web' of stack 'app' all mount volume 'uploads'",
        "user ids of its own",
        "the volume belongs to the one that starts first",
        "`host-user-namespace: <why>` under modules.shop.services.api and modules.shop.services.web",
    ] {
        assert!(found[0].contains(part), "{part}: {found:?}");
    }
    // One of them with the host's ids is not enough; both is.
    let one = USERNS_CONFIG.replace(
        "api: {stack: app}",
        "api: {stack: app, host-user-namespace: shares uploads with the host}",
    );
    let found = user_namespace_findings_with(&one, &[("compose.yaml", both)])?;
    assert!(
        found.len() == 1 && found[0].ends_with("under modules.shop.services.web"),
        "{found:?}"
    );
    let all = one.replace(
        "web: {stack: app, placement: all}",
        "web: {stack: app, placement: all, host-user-namespace: shares uploads with the host}",
    );
    assert!(user_namespace_findings_with(&all, &[("compose.yaml", both)])?.is_empty());

    let blank = USERNS_CONFIG.replace(
        "api: {stack: app}",
        "api: {stack: app, host-user-namespace: ' '}",
    );
    let found = user_namespace_findings(&blank)?;
    assert!(
        found.iter().any(|finding| finding
            .contains("modules.shop.services.api.host-user-namespace has no reason")),
        "{found:?}"
    );

    // Docker Engine gives containers the host's ids: nothing to say.
    let docker = USERNS_CONFIG.replace(", runtime: {driver: podman-compose}", "");
    assert!(user_namespace_findings_with(&docker, &[("compose.yaml", both)])?.is_empty());
    Ok(())
}

/// A config file or a secret file is given to the host ids its `uid` and
/// `gid` are in the service's user namespace, whatever its mode: they have
/// to be among the namespace's 65536 ids.
#[test]
fn a_file_mm_hands_a_podman_service_names_one_of_its_ids() -> TestResult {
    let files = [("api.conf", "listen 8080\n")];
    let with = |api: &str| USERNS_CONFIG.replace("api: {stack: app}", api);
    for readable in [
        "api: {stack: app, config-files: {/etc/api.conf: {file: api.conf, uid: \"101\", mode: 0o440}}}",
        "api: {stack: app, config-files: {/etc/api.conf: {file: api.conf, mode: 0o600}}}",
    ] {
        assert!(
            user_namespace_findings_with(&with(readable), &files)?.is_empty(),
            "{readable}"
        );
    }
    let outside = with(
        "api: {stack: app, config-files: {/etc/api.conf: {file: api.conf, uid: \"70000\"}}, secret-files: {token: {secret: api.token, gid: \"65536\"}}}",
    );
    let found = user_namespace_findings_with(&outside, &files)?;
    assert_eq!(found.len(), 2, "{found:?}");
    for part in [
        "modules.shop.services.api.config-files.\"/etc/api.conf\" names id 70000",
        "modules.shop.services.api.secret-files.\"token\" names id 65536",
        "the service's containers have the ids 0 to 65535",
    ] {
        assert!(
            found.iter().any(|finding| finding.contains(part)),
            "{part}: {found:?}"
        );
    }
    let kept = outside.replace(
        "api: {stack: app, ",
        "api: {stack: app, host-user-namespace: its files belong to host users, ",
    );
    assert!(user_namespace_findings_with(&kept, &files)?.is_empty());
    let docker = outside.replace(", runtime: {driver: podman-compose}", "");
    assert!(user_namespace_findings_with(&docker, &files)?.is_empty());
    Ok(())
}

#[test]
fn a_service_that_keeps_the_hosts_ids_runs_in_the_hosts_user_namespace() -> TestResult {
    let project = temp_dirs::tempdir()?;
    let root = project.path();
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n  api:\n    image: example/api\n",
    )?;
    std::fs::write(
        root.join(".config/mm.yaml"),
        "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: demo\ndefault-environment: local\ntarget: local\nruntime:\n  driver: podman-compose\nstacks:\n  app:\n    files: [compose.yaml]\nmodules:\n  shop:\n    path: .\n    services:\n      web:\n        stack: app\n        host-user-namespace: writes what every node reads\n      api:\n        stack: app\nenvironments:\n  local:\n    bindings: {}\n",
    )?;
    let home = temp_dirs::tempdir()?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(9))),
    )?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        targets,
    );
    let compiled = manager.compile(&WorkspaceSelection::new(root, None))?;
    let overlay: serde_yaml_ng::Value = serde_yaml_ng::from_str(&std::fs::read_to_string(
        &compiled.stacks[0].generated_overlay_path,
    )?)?;
    assert_eq!(
        overlay["services"]["web"]["userns_mode"].as_str(),
        Some("host")
    );
    assert!(
        overlay["services"]["api"]["userns_mode"].is_null(),
        "{overlay:?}"
    );
    Ok(())
}

#[test]
fn podmans_log_command_is_read_line_by_line_with_its_nanosecond_stamps() {
    use mm::adapters::runtime::engine_logs::LogDemux;
    use mm::domain::observability::LogSource;

    let mut lines = LogDemux::new(false, 8);
    let mut read = lines.push_text(LogSource::Stdout, b"2026-11-14T16:29:12.567385926Z sta");
    assert!(read.is_empty(), "a line is not over before its newline");
    read.extend(lines.push_text(
        LogSource::Stdout,
        b"rted\n2026-11-14T16:29:12.6Z a long line\n",
    ));
    read.extend(lines.push_text(LogSource::Stderr, b"2026-11-14T16:29:12.7+00:00 oops\n"));
    let seen = read
        .iter()
        .map(|line| {
            (
                line.source,
                line.timestamp.clone(),
                String::from_utf8_lossy(&line.bytes).into_owned(),
                line.truncated,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        seen,
        [
            (
                LogSource::Stdout,
                Some("2026-11-14T16:29:12.567385926Z".to_owned()),
                "started".to_owned(),
                false
            ),
            (
                LogSource::Stdout,
                Some("2026-11-14T16:29:12.6Z".to_owned()),
                "a long l".to_owned(),
                true
            ),
            (
                LogSource::Stderr,
                Some("2026-11-14T16:29:12.7+00:00".to_owned()),
                "oops".to_owned(),
                false
            ),
        ]
    );
}

#[test]
fn the_stamps_podman_puts_inside_a_long_line_are_taken_out_and_the_programs_own_kept() {
    use mm::adapters::runtime::engine_logs::LogDemux;
    use mm::domain::observability::LogSource;

    let mut lines = LogDemux::new(false, 1024);
    // A line the engine kept in three pieces, as `podman logs --timestamps`
    // joins them; then a program's own stamps at its line's start and in
    // milliseconds inside it, which stay.
    let read = lines.push_text(
        LogSource::Stdout,
        b"2026-11-14T17:35:15.168587794Z xxxx2026-11-14T17:35:15.168626474Z yyyy2026-11-14T17:35:15.16865Z zz\n\
          2026-11-14T17:35:16.000000001Z 2026-11-14T17:35:16.123456789Z started at 2026-11-14T17:35:16.123Z ok\n",
    );
    let messages = read
        .iter()
        .map(|line| String::from_utf8_lossy(&line.bytes).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        messages,
        [
            "xxxxyyyy2026-11-14T17:35:15.16865Z zz".to_owned(),
            "2026-11-14T17:35:16.123456789Z started at 2026-11-14T17:35:16.123Z ok".to_owned(),
        ],
        "nanosecond stamps inside the line go; one of five digits, one at the start, and one of milliseconds stay"
    );
}

/// start-first on Podman nodes: the container that a newer one of its
/// service replaces is the older one, and removing it stops it, then
/// removes it, through Podman's command line as on Docker.
#[tokio::test]
async fn podman_removes_the_container_a_newer_one_of_its_service_replaces() -> TestResult {
    let labels = r#"{"com.docker.compose.project":"shop-app","com.docker.compose.service":"web"}"#;
    let inspected = format!(
        r#"[{{"Id":"new","Created":"2026-11-15T01:00:05.000000001Z","State":{{"Running":true}},"Config":{{"Labels":{labels}}}}},{{"Id":"old","Created":"2026-11-15T01:00:00.123Z","State":{{"Running":true}},"Config":{{"Labels":{labels}}}}}]"#
    );
    let podman = Podman::answering(vec![
        (0, r#"[{"Id":"old"},{"Id":"new"}]"#.to_owned()),
        (0, inspected),
        (0, String::new()),
        (0, String::new()),
    ]);
    let runtime = PodmanComposeRuntime::new(podman.clone());
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = mm::domain::runtime::RuntimeTarget {
        root: root.clone(),
        project_directory: root.clone(),
        target: Some(local_target()),
        project_id: mm::domain::workspace::ProjectId(uuid::Uuid::from_u128(1)),
        environment: "production".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "shop-app".to_owned(),
        artifact_hash: "hash".to_owned(),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: mm::domain::runtime::RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    };
    let replaced = runtime
        .replaced_containers(&target, &["web".to_owned()])
        .await?;
    assert_eq!(replaced, ["old"]);
    runtime.remove_containers(&target, &replaced).await?;
    let commands = podman.commands()?;
    assert_eq!(commands.len(), 4, "{commands:?}");
    assert!(
        commands[0].contains("ps --all --filter label=com.docker.compose.project=shop-app --filter label=com.docker.compose.service=web"),
        "{commands:?}"
    );
    assert!(commands[2].ends_with("stop old"), "{commands:?}");
    assert!(commands[3].ends_with("rm --force old"), "{commands:?}");
    Ok(())
}

/// One stack of the local target, with one Compose file under `root`.
fn scaled_target(root: &Path) -> Result<mm::domain::runtime::RuntimeTarget, std::io::Error> {
    let compose = root.join("compose.yaml");
    std::fs::write(&compose, "services:\n  web: {image: example/web}\n")?;
    Ok(mm::domain::runtime::RuntimeTarget {
        root: root.to_path_buf(),
        project_directory: root.to_path_buf(),
        target: Some(local_target()),
        project_id: mm::domain::workspace::ProjectId(uuid::Uuid::from_u128(9)),
        environment: "production".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "shop-production-app".to_owned(),
        artifact_hash: "a".repeat(64),
        compose_files: vec![compose],
        user_compose_file_count: 1,
        interpolation_environment: mm::domain::runtime::RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    })
}

#[tokio::test]
async fn a_podman_node_runs_a_scaled_count_through_compose() -> TestResult {
    let root = temp_dirs::tempdir()?;
    let target = scaled_target(root.path())?;
    let counts =
        std::collections::BTreeMap::from([("web".to_owned(), 2), ("worker".to_owned(), 3)]);
    // Each call: the project's containers first (none), then Compose.
    let compose = |podman: &Podman| -> Result<String, String> {
        let commands = podman.commands()?;
        commands
            .get(1)
            .cloned()
            .ok_or_else(|| format!("{commands:?}"))
    };

    // A deployment on a node: its share of each scaled service, and the
    // services that left keep running for `finish_moves`.
    let podman = Podman::answering(vec![(0, "[]".to_owned()), (0, String::new())]);
    PodmanComposeRuntime::new(podman.clone())
        .apply_scaled(&target, &counts, true)
        .await?;
    let up = compose(&podman)?;
    assert!(up.contains(" compose --file "), "{up}");
    assert!(
        up.ends_with(" up -d --scale web=2 --scale worker=3 --no-build"),
        "{up}"
    );

    // The autoscaler adds one: that service alone, nothing recreated.
    let podman = Podman::answering(vec![(0, "[]".to_owned()), (0, String::new())]);
    PodmanComposeRuntime::new(podman.clone())
        .scale_service(&target, "web", 3)
        .await?;
    let up = compose(&podman)?;
    assert!(
        up.ends_with(" up -d --no-deps --no-recreate --no-build --scale web=3 web"),
        "{up}"
    );

    // An ahead step starts only the worker, at its count.
    let podman = Podman::answering(vec![(0, "[]".to_owned()), (0, String::new())]);
    PodmanComposeRuntime::new(podman.clone())
        .apply_services(&target, &["worker".to_owned()], &counts)
        .await?;
    let up = compose(&podman)?;
    assert!(
        up.ends_with(" up -d --scale worker=3 --no-build --no-deps worker"),
        "{up}"
    );
    Ok(())
}
