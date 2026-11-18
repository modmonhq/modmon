use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use super::*;
use crate::app::ports::{CommandOutput, CommandSpec, LocalDeploymentRuntime, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::ownership::ResourceKind;
use crate::domain::runtime::{ConvergenceState, RuntimeEnvironment, RuntimeTarget};
use crate::domain::target::{NamedTarget, TargetDefinition, TargetId};
use crate::domain::workspace::ProjectId;

#[derive(Default)]
struct QueueProcess {
    outputs: Mutex<VecDeque<Result<CommandOutput, MmError>>>,
    commands: Mutex<Vec<String>>,
}

impl QueueProcess {
    fn with(outputs: Vec<Result<CommandOutput, MmError>>) -> Arc<Self> {
        Arc::new(Self {
            outputs: Mutex::new(outputs.into()),
            commands: Mutex::new(Vec::new()),
        })
    }

    fn commands(&self) -> Result<Vec<String>, MmError> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
            .map_err(|_| MmError::State {
                message: "Docker Compose command lock poisoned".to_owned(),
            })
    }
}

#[async_trait]
impl ProcessRunner for QueueProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "Docker Compose command lock poisoned".to_owned(),
            })?
            .push(specification.redacted_display());
        self.outputs
            .lock()
            .map_err(|_| MmError::State {
                message: "Docker Compose output lock poisoned".to_owned(),
            })?
            .pop_front()
            .unwrap_or_else(|| {
                Err(MmError::RuntimeResponse {
                    message: "unexpected process".to_owned(),
                })
            })
    }
}

fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

fn ok(stdout: &str) -> Result<CommandOutput, MmError> {
    Ok(output(0, stdout, ""))
}

fn local_target() -> NamedTarget {
    NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(7))),
    )
}

fn runtime_target(root: &Path) -> RuntimeTarget {
    RuntimeTarget {
        root: root.to_path_buf(),
        project_directory: root.to_path_buf(),
        target: Some(local_target()),
        project_id: ProjectId(uuid::Uuid::from_u128(8)),
        environment: "test".to_owned(),
        stack_name: "web".to_owned(),
        project_name: "mm-web".to_owned(),
        artifact_hash: "a".repeat(64),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

fn inspected(kind: ResourceKind, name: &str, resource: &str) -> Result<String, MmError> {
    let ownership = DockerComposeRuntime::ownership(&local_target(), kind, name, resource)?;
    Ok(serde_json::json!([{"Name": name, "Labels": ownership.labels()}]).to_string())
}

#[test]
fn command_and_validation_helpers_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = DockerComposeRuntime::new(Arc::new(QueueProcess::default()));
    assert!(format!("{runtime:?}").contains("DockerComposeRuntime"));
    let temp = crate::test_support::tempdir()?;
    let target = runtime_target(temp.path());
    let args = runtime.compose_args(&target)?;
    assert!(args.windows(2).any(|pair| pair[0] == "--project-name" && pair[1] == "mm-web"));
    let mut missing = target.clone();
    missing.compose_files.clear();
    assert!(matches!(runtime.compose_args(&missing), Err(MmError::Security { .. })));

    let failure = runtime.ensure_success(output(4, "", " denied \n"));
    assert!(matches!(failure, Err(MmError::ProcessFailed { code: Some(4), stderr, .. }) if stderr == "denied"));
    assert!(DockerComposeRuntime::ownership(
        &local_target(), ResourceKind::Network, "../bad", "network"
    )
    .is_err());
    assert_eq!(DockerComposeRuntime::volume_helper_args("data").last(), Some(&"data:/volume".to_owned()));
    assert!(validate_volume_file("../secret").is_err());
    assert!(validate_volume_file("nested/secret").is_err());
    assert!(validate_volume_file("secret").is_ok());

    assert!(canonical_volume_source(&temp.path().join("missing")).is_err());
    assert!(canonical_volume_source(temp.path()).is_err());
    let source = temp.path().join("source");
    std::fs::write(&source, "value")?;
    assert_eq!(canonical_volume_source(&source)?, dunce::canonicalize(&source)?);

    let mut diagnostics = Vec::new();
    assert_eq!(
        docker_rootless(&output(0, r#"{"SecurityOptions":["name=rootless"]}"#, ""), &mut diagnostics),
        Some(true)
    );
    assert!(docker_rootless(&output(0, "not-json", ""), &mut diagnostics).is_none());
    assert_eq!(diagnostics.len(), 1);
    Ok(())
}

#[tokio::test]
async fn network_and_volume_lifecycles_issue_owned_mutations() -> Result<(), Box<dyn std::error::Error>> {
    let missing_network = Ok(output(1, "", "Error: No such network"));
    let owned_network = inspected(ResourceKind::Network, "edge", "network")?;
    let process = QueueProcess::with(vec![
        missing_network,
        ok("created"),
        ok(&owned_network),
        ok(&owned_network),
        ok("removed"),
        Ok(output(1, "", "No such volume")),
        ok("created"),
        Ok(output(1, "", "No such volume")),
    ]);
    let runtime = DockerComposeRuntime::new(process.clone());
    let target = local_target();

    assert_eq!(
        runtime.ensure_network(&target, "edge", "network").await?,
        ResourceEnsureResult::Created
    );
    assert!(runtime.inspect_network(&target, "edge", "network").await?);
    runtime.remove_network(&target, "edge", "network").await?;
    assert_eq!(
        runtime.ensure_volume(&target, "data", "volume").await?,
        ResourceEnsureResult::Created
    );
    runtime.remove_volume(&target, "data", "volume").await?;

    let commands = process.commands()?;
    assert!(commands.iter().any(|command| command.contains("network create")));
    assert!(commands.iter().any(|command| command == "docker network rm edge"));
    assert!(commands.iter().any(|command| command.contains("volume create")));
    Ok(())
}

#[tokio::test]
async fn apply_remove_render_observe_and_probe_use_bounded_commands() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let target = runtime_target(temp.path());
    let process = QueueProcess::with(vec![
        ok("Docker version 28\n"),
        ok(r#"{"SecurityOptions":["name=rootless"]}"#),
        ok("Docker Compose version v2\n"),
        ok("[]"),
        ok("applied"),
        ok("[]"),
        ok("removed"),
        ok("services:\n  web:\n    image: nginx:latest\n"),
        ok("services:\n  web:\n    image: nginx:latest\n"),
        ok(""),
    ]);
    let runtime = DockerComposeRuntime::new(process.clone());

    let probe = runtime.probe().await;
    assert!(probe.available);
    assert_eq!(probe.rootless, Some(true));
    assert_eq!(probe.engine_version.as_deref(), Some("Docker version 28"));
    runtime.apply(&target).await?;
    runtime.remove(&target).await?;
    let rendered = runtime.render(&target).await?;
    assert_eq!(rendered.service_names, vec!["web"]);
    let observed = runtime.observe(&target).await?;
    assert_eq!(observed.stack, "web");

    let commands = process.commands()?;
    assert!(commands.iter().any(|command| command.ends_with("up -d --remove-orphans --no-build")));
    assert!(commands.iter().any(|command| command.ends_with("down --remove-orphans")));
    assert!(commands.iter().filter(|command| command.ends_with("config")).count() >= 2);
    Ok(())
}

#[tokio::test]
async fn convergence_reports_cancellation_timeout_success_and_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let target = runtime_target(temp.path());
    let cancellation = ConvergenceCancellation::default();
    cancellation.cancel();
    let runtime = DockerComposeRuntime::new(Arc::new(QueueProcess::default()));
    let cancelled = runtime
        .wait_for_convergence(&target, ConvergencePolicy::default(), cancellation)
        .await?;
    assert_eq!(cancelled.state, ConvergenceState::Cancelled);
    assert_eq!(cancelled.polls, 0);

    let config = "services:\n  web:\n    image: nginx:latest\n";
    let pending = QueueProcess::with(vec![ok(config), ok("")]);
    let timed_out = DockerComposeRuntime::new(pending)
        .wait_for_convergence(
            &target,
            ConvergencePolicy {
                timeout: Duration::ZERO,
                initial_backoff: Duration::ZERO,
                maximum_backoff: Duration::ZERO,
            },
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(timed_out.state, ConvergenceState::TimedOut);
    assert_eq!(timed_out.polls, 1);

    let running = serde_json::json!([{
        "ID": "abc",
        "Name": "mm-web-web-1",
        "Service": "web",
        "Image": "nginx:latest",
        "State": "running",
        "Status": "Up 1 second (healthy)"
    }])
    .to_string();
    let process = QueueProcess::with(vec![ok(config), ok(&running)]);
    let succeeded = DockerComposeRuntime::new(process)
        .wait_for_convergence(
            &target,
            ConvergencePolicy::default(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(succeeded.state, ConvergenceState::Succeeded);

    let unhealthy = serde_json::json!([{
        "ID": "def",
        "Name": "mm-web-web-1",
        "Service": "web",
        "Image": "nginx:latest",
        "State": "running",
        "Status": "Up 1 second (unhealthy)"
    }])
    .to_string();
    let process = QueueProcess::with(vec![ok(config), ok(&unhealthy)]);
    let failed = DockerComposeRuntime::new(process)
        .wait_for_convergence(
            &target,
            ConvergencePolicy::default(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(failed.state, ConvergenceState::Failed);
    assert!(failed.diagnostic_code.is_some());
    Ok(())
}

#[tokio::test]
async fn volume_files_are_staged_started_and_cleaned_up() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let source = temp.path().join("source");
    std::fs::write(&source, "secret")?;
    let owned = inspected(ResourceKind::Volume, "data", "volume")?;
    let process = QueueProcess::with(vec![
        ok(&owned),
        ok(""),
        ok(&owned),
        ok("abcdef1234\n"),
        ok(""),
        ok(""),
        ok(""),
        ok(&owned),
        ok(""),
        ok(&owned),
        ok(""),
    ]);
    let runtime = DockerComposeRuntime::new(process.clone());
    let target = local_target();

    runtime
        .create_volume_file_if_absent(&target, "data", "volume", "empty", 0o600)
        .await?;
    runtime
        .replace_volume_file(&target, "data", "volume", "secret", &source, 0o640)
        .await?;
    runtime
        .set_volume_owner(
            &target,
            "data",
            "volume",
            VolumeOwnership { uid: 1000, gid: 1001, mode: 0o750 },
        )
        .await?;
    runtime
        .remove_volume_file(&target, "data", "volume", "empty")
        .await?;

    let commands = process.commands()?;
    assert!(commands.iter().any(|command| command == "docker cp" || command.starts_with("docker cp ")));
    assert!(commands.iter().any(|command| command == "docker start --attach abcdef1234"));
    assert!(commands.iter().any(|command| command == "docker rm --force abcdef1234"));
    Ok(())
}

#[tokio::test]
async fn malformed_helper_ids_and_cleanup_failures_are_reported() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let source = temp.path().join("source");
    std::fs::write(&source, "secret")?;
    let owned = inspected(ResourceKind::Volume, "data", "volume")?;

    let invalid = QueueProcess::with(vec![ok(&owned), ok("not-an-id")]);
    let runtime = DockerComposeRuntime::new(invalid);
    assert!(matches!(
        runtime.replace_volume_file(&local_target(), "data", "volume", "secret", &source, 0o600).await,
        Err(MmError::RuntimeResponse { .. })
    ));

    let cleanup = QueueProcess::with(vec![
        ok(&owned),
        ok("abcdef"),
        Ok(output(1, "", "copy failed")),
        Ok(output(1, "", "cleanup failed")),
    ]);
    let runtime = DockerComposeRuntime::new(cleanup);
    assert!(runtime
        .replace_volume_file(&local_target(), "data", "volume", "secret", &source, 0o600)
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn unsafe_volume_file_names_fail_before_side_effects() {
    let runtime = DockerComposeRuntime::new(Arc::new(QueueProcess::default()));
    let target = local_target();
    assert!(matches!(
        runtime
            .create_volume_file_if_absent(&target, "vol", "res", "../file", 0o644)
            .await,
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        runtime
            .replace_volume_file(
                &target,
                "vol",
                "res",
                "dir/file",
                Path::new("/tmp/x"),
                0o644,
            )
            .await,
        Err(MmError::Security { .. })
    ));
}

fn cluster_network() -> Result<crate::domain::cluster_network::LocalNetwork, String> {
    Ok(crate::domain::cluster_network::LocalNetwork {
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

/// `docker network inspect` of the owned network `backend` with `config`
/// as its addressing and `options` as its driver options.
fn inspected_network(config: serde_json::Value, options: serde_json::Value) -> Result<String, MmError> {
    let ownership =
        DockerComposeRuntime::ownership(&local_target(), ResourceKind::Network, "backend", "network")?;
    Ok(serde_json::json!([{
        "Name": "backend",
        "Driver": "bridge",
        "Internal": false,
        "Labels": ownership.labels(),
        "IPAM": {"Config": [config]},
        "Options": options,
    }])
    .to_string())
}

fn spanning_network() -> Result<String, MmError> {
    spanning_network_on(serde_json::json!({
        "Subnet": "10.64.0.0/20", "IPRange": "10.64.1.0/24", "Gateway": "10.64.1.1",
        "AuxiliaryAddresses": {"first": "10.64.1.0", "last": "10.64.1.255"},
    }))
}

/// The cluster network's bridge with `config` as its addressing.
fn spanning_network_on(config: serde_json::Value) -> Result<String, MmError> {
    inspected_network(
        config,
        serde_json::json!({
            "com.docker.network.bridge.name": "mmn-0123456789a",
            "com.docker.network.driver.mtu": "1420",
            "com.docker.network.bridge.enable_ip_masquerade": "false",
            "com.docker.network.bridge.trusted_host_interfaces": "mm0",
        }),
    )
}

#[tokio::test]
async fn a_cluster_network_is_a_bridge_on_the_block_with_the_slice_as_range()
-> Result<(), Box<dyn std::error::Error>> {
    let process = QueueProcess::with(vec![
        ok("iptables\n"),
        Ok(output(1, "", "Error: No such network")),
        ok("created"),
        ok("iptables\n"),
        ok(&spanning_network()?),
    ]);
    let runtime = DockerComposeRuntime::new(process.clone());
    let (target, network) = (local_target(), cluster_network()?);
    assert_eq!(
        runtime
            .ensure_cluster_network(&target, "backend", "network", &network)
            .await?,
        ResourceEnsureResult::Created
    );
    assert_eq!(
        runtime
            .ensure_cluster_network(&target, "backend", "network", &network)
            .await?,
        ResourceEnsureResult::Unchanged
    );
    let commands = process.commands()?;
    assert_eq!(commands[0], "docker info --format {{.FirewallBackend.Driver}}");
    let create = &commands[2];
    for part in [
        "docker network create --driver bridge --subnet 10.64.0.0/20 --ip-range 10.64.1.0/24 --gateway 10.64.1.1 --aux-address first=10.64.1.0 --aux-address last=10.64.1.255",
        "--opt com.docker.network.bridge.name=mmn-0123456789a",
        "--opt com.docker.network.driver.mtu=1420",
        "--opt com.docker.network.bridge.enable_ip_masquerade=false",
        "--opt com.docker.network.bridge.trusted_host_interfaces=mm0",
        "--label io.mm.resource=network",
    ] {
        assert!(create.contains(part), "{part} is not in: {create}");
    }
    assert!(create.ends_with(" backend"), "{create}");
    assert_eq!(commands.len(), 5, "an unchanged network gets no command");
    Ok(())
}

#[tokio::test]
async fn a_host_local_network_is_removed_with_its_containers_and_created_again()
-> Result<(), Box<dyn std::error::Error>> {
    let host_local = inspected_network(
        serde_json::json!({"Subnet": "172.19.0.0/16", "Gateway": "172.19.0.1"}),
        serde_json::json!({}),
    )?;
    let process = QueueProcess::with(vec![
        // Planning: the engine's network is not the one the node stored.
        ok("iptables\n"),
        ok(&host_local),
        // Applying.
        ok("iptables\n"),
        ok(&host_local),
        ok("aaa111\nbbb222\n"),
        ok("aaa111\nbbb222\n"),
        ok("backend\n"),
        ok("created"),
    ]);
    let runtime = DockerComposeRuntime::new(process.clone());
    let (target, network) = (local_target(), cluster_network()?);
    let observed = runtime
        .observe_cluster_network("backend", Some(&network))
        .await?
        .ok_or("the network was not observed")?;
    assert_eq!((observed.addresses.as_str(), observed.current), ("172.19.0.0/16", false));
    assert_eq!(
        runtime
            .ensure_cluster_network(&target, "backend", "network", &network)
            .await?,
        ResourceEnsureResult::Created
    );
    let commands = process.commands()?;
    assert_eq!(
        commands[4..7],
        [
            "docker ps --all --quiet --filter network=backend",
            "docker rm --force aaa111 bbb222",
            "docker network rm backend",
        ]
    );
    assert!(commands[7].starts_with("docker network create --driver bridge --subnet 10.64.0.0/20"));
    Ok(())
}

#[tokio::test]
async fn a_network_that_would_hand_out_the_slices_first_address_is_not_current()
-> Result<(), Box<dyn std::error::Error>> {
    // Same block, range, and gateway, and nothing kept back: the engine
    // would give the first container 10.64.1.0.
    let open = spanning_network_on(serde_json::json!({
        "Subnet": "10.64.0.0/20", "IPRange": "10.64.1.0/24", "Gateway": "10.64.1.1",
    }))?;
    let other = spanning_network_on(serde_json::json!({
        "Subnet": "10.64.0.0/20", "IPRange": "10.64.1.0/24", "Gateway": "10.64.1.1",
        "AuxiliaryAddresses": {"first": "10.64.1.0", "last": "10.64.1.254"},
    }))?;
    let process = QueueProcess::with(vec![ok("iptables\n"), ok(&open), ok("iptables\n"), ok(&other)]);
    let runtime = DockerComposeRuntime::new(process);
    let network = cluster_network()?;
    for _ in 0..2 {
        let observed = runtime
            .observe_cluster_network("backend", Some(&network))
            .await?
            .ok_or("the network was not observed")?;
        assert_eq!(observed.addresses, "10.64.0.0/20, range 10.64.1.0/24");
        assert!(!observed.current);
    }
    Ok(())
}

#[tokio::test]
async fn the_current_network_is_reported_as_such_and_a_missing_one_as_none()
-> Result<(), Box<dyn std::error::Error>> {
    let process = QueueProcess::with(vec![
        ok("iptables\n"),
        ok(&spanning_network()?),
        ok("iptables\n"),
        Ok(output(1, "", "Error response from daemon: network backend not found")),
    ]);
    let runtime = DockerComposeRuntime::new(process);
    let network = cluster_network()?;
    let observed = runtime
        .observe_cluster_network("backend", Some(&network))
        .await?
        .ok_or("the network was not observed")?;
    assert_eq!(observed.addresses, "10.64.0.0/20, range 10.64.1.0/24");
    assert!(observed.current);
    assert!(runtime.observe_cluster_network("backend", Some(&network)).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn an_engine_that_filters_with_nftables_is_refused_with_the_fix()
-> Result<(), Box<dyn std::error::Error>> {
    let process = QueueProcess::with(vec![ok("nftables\n")]);
    let runtime = DockerComposeRuntime::new(process);
    let Err(error) = runtime.observe_cluster_network("backend", None).await else {
        return Err("an nftables engine was accepted".into());
    };
    let message = error.to_string();
    assert!(
        message.contains("keeps its firewall in nftables")
            && message.contains("Remove \"firewall-backend\" from /etc/docker/daemon.json")
            && message.contains("systemctl restart docker"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn an_internal_network_is_created_internal_and_never_changed_in_place()
-> Result<(), Box<dyn std::error::Error>> {
    let process = QueueProcess::with(vec![
        Ok(output(1, "", "Error: No such network")),
        ok("created"),
        ok(&inspected_network(serde_json::json!({"Subnet": "172.19.0.0/16"}), serde_json::json!({}))?),
    ]);
    let runtime = DockerComposeRuntime::new(process.clone());
    let target = local_target();
    assert_eq!(
        runtime.ensure_internal_network(&target, "backend", "network").await?,
        ResourceEnsureResult::Created
    );
    assert!(process.commands()?[1].starts_with("docker network create --internal "));
    let Err(error) = runtime.ensure_internal_network(&target, "backend", "network").await else {
        return Err("a network that is not internal was accepted".into());
    };
    assert!(error.to_string().contains("docker network rm backend"), "{error}");
    Ok(())
}
