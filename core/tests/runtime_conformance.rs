use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mm::adapters::runtime::{DockerComposeRuntime, PodmanComposeRuntime};
use mm::app::ports::{
    CommandOutput, CommandSpec, LocalDeploymentRuntime, ProcessRunner, RedactedCommandRecord,
    RuntimeDriver,
};
use mm::domain::error::MmError;
use mm::domain::ownership::{ResourceKind, ResourceOwnership};
use mm::domain::runtime::{
    ConvergenceCancellation, ConvergencePolicy, ConvergenceState, RuntimeKind, RuntimeResource,
    RuntimeTarget, runtime_descriptor,
};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;
use zeroize::Zeroizing;

const PROJECT_ID: u128 = 0x019c_527b_6242_7b43_bfd4_bd60_e27c_1de3;
const TARGET_ID: u128 = 0x319c_527b_6242_7b43_bfd4_bd60_e27c_1de3;

struct QueueProcess {
    outputs: Mutex<VecDeque<CommandOutput>>,
    calls: Mutex<usize>,
    records: Mutex<Vec<RedactedCommandRecord>>,
}

#[async_trait::async_trait]
impl ProcessRunner for QueueProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.records
            .lock()
            .map_err(|_| MmError::State {
                message: "command record lock was poisoned".to_owned(),
            })?
            .push(specification.redacted_record());
        *self.calls.lock().map_err(|_| MmError::State {
            message: "call counter lock was poisoned".to_owned(),
        })? += 1;
        self.outputs
            .lock()
            .map_err(|_| MmError::State {
                message: "output queue lock was poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "output queue is empty".to_owned(),
            })
    }
}

impl QueueProcess {
    fn new(outputs: impl IntoIterator<Item = CommandOutput>) -> Arc<Self> {
        Arc::new(Self {
            outputs: Mutex::new(outputs.into_iter().collect()),
            calls: Mutex::new(0),
            records: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Result<usize, MmError> {
        self.calls
            .lock()
            .map(|calls| *calls)
            .map_err(|_| MmError::State {
                message: "call counter lock was poisoned".to_owned(),
            })
    }

    fn records(&self) -> Result<Vec<RedactedCommandRecord>, MmError> {
        self.records
            .lock()
            .map(|records| records.clone())
            .map_err(|_| MmError::State {
                message: "command record lock was poisoned".to_owned(),
            })
    }
}

fn output(stdout: &str) -> CommandOutput {
    output_with_code(0, stdout, "")
}

fn output_with_code(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

fn fixture(name: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/runtime-conformance")
            .join(format!("{name}.compose.yaml")),
    )
}

fn target(stack: &str) -> RuntimeTarget {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-conformance");
    RuntimeTarget {
        root: root.clone(),
        project_directory: root.clone(),
        target: Some(NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(Uuid::from_u128(TARGET_ID))),
        )),
        project_id: ProjectId(Uuid::from_u128(PROJECT_ID)),
        environment: "conformance".to_owned(),
        stack_name: stack.to_owned(),
        project_name: format!("mm-conformance-{stack}"),
        artifact_hash: "fixture-hash".to_owned(),
        compose_files: vec![root.join(format!("{stack}.compose.yaml"))],
        user_compose_file_count: 1,
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

#[tokio::test]
async fn canonical_multi_stack_fixture_has_equivalent_normalized_renderings()
-> Result<(), Box<dyn std::error::Error>> {
    for stack in ["app", "data"] {
        let yaml = fixture(stack)?;
        let podman = PodmanComposeRuntime::new(QueueProcess::new([output(&yaml)]));
        let docker = DockerComposeRuntime::new(QueueProcess::new([output(&yaml)]));
        let target = target(stack);

        let podman = podman.render(&target).await?;
        let docker = docker.render(&target).await?;

        assert_eq!(podman.services, docker.services);
        assert_eq!(podman.resources, docker.resources);
        assert_eq!(podman.config_hash, docker.config_hash);
        assert!(!podman.services.is_empty());
    }

    let app = fixture("app")?;
    let rendered = DockerComposeRuntime::new(QueueProcess::new([output(&app)]))
        .render(&target("app"))
        .await?;
    let api = rendered
        .services
        .get("api")
        .ok_or_else(|| std::io::Error::other("api semantics are missing"))?;
    assert_eq!(api.environment_names, ["MM_CONFORMANCE_REVISION"]);
    assert_eq!(api.aliases["conformance"], ["api.internal"]);
    assert!(api.healthcheck);
    assert!(
        serde_json::to_string(&rendered)?.contains("MM_CONFORMANCE_REVISION")
            && !serde_json::to_string(&rendered)?.contains("set by the private")
    );
    Ok(())
}

#[tokio::test]
async fn providers_normalize_to_the_same_logical_service_observation()
-> Result<(), Box<dyn std::error::Error>> {
    let yaml = fixture("app")?;
    let compose_row = r#"[{"ID":"abc","Name":"mm-conformance-app-api-1","Image":"docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d","State":"running","Status":"Up 2 seconds (healthy)","Health":"healthy","Labels":{"com.docker.compose.project":"mm-conformance-app","com.docker.compose.service":"api"}}]"#;
    let podman = PodmanComposeRuntime::new(QueueProcess::new([
        output(&yaml),
        output(compose_row),
        output("abc\thealthy\n"),
    ]));
    let docker = DockerComposeRuntime::new(QueueProcess::new([output(&yaml), output(compose_row)]));

    let podman = podman.observe(&target("app")).await?;
    let docker = docker.observe(&target("app")).await?;

    assert_eq!(podman.services, docker.services);
    let service = &docker.services[0];
    assert_eq!(service.identity.service, "api");
    assert_eq!(service.desired_count, 1);
    assert_eq!(service.running_count, 1);
    assert_eq!(service.healthy_count, 1);
    assert_eq!(service.state, mm::domain::runtime::ServiceState::Running);
    Ok(())
}

#[tokio::test]
async fn convergence_cancelled_before_it_starts_runs_no_engine_command()
-> Result<(), Box<dyn std::error::Error>> {
    let cancelled = || {
        let cancellation = ConvergenceCancellation::default();
        cancellation.cancel();
        cancellation
    };
    let process = QueueProcess::new([]);
    let outcome = DockerComposeRuntime::new(process.clone())
        .wait_for_convergence(&target("app"), ConvergencePolicy::default(), cancelled())
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Cancelled);
    assert_eq!(process.calls()?, 0);

    let process = QueueProcess::new([]);
    let outcome = PodmanComposeRuntime::new(process.clone())
        .wait_for_convergence(&target("app"), ConvergencePolicy::default(), cancelled())
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Cancelled);
    assert_eq!(process.calls()?, 0);
    Ok(())
}

#[tokio::test]
async fn foreign_resource_collisions_fail_during_preflight_before_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let named = target("app")
        .target
        .ok_or_else(|| std::io::Error::other("target identity is missing"))?;
    let resource = RuntimeResource {
        kind: ResourceKind::Network,
        name: "mm-runtime-conformance-network".to_owned(),
        resource: "conformance-network".to_owned(),
    };
    let foreign = r#"[{"Name":"mm-runtime-conformance-network","Labels":{"foreign":"true"}}]"#;

    let docker_process = QueueProcess::new([output(foreign)]);
    let docker = DockerComposeRuntime::new(docker_process.clone());
    assert!(matches!(
        docker
            .preflight(&named, std::slice::from_ref(&resource), &[])
            .await,
        Err(MmError::Security { .. })
    ));
    assert_eq!(docker_process.calls()?, 1);

    let podman_process = QueueProcess::new([output(""), output(foreign)]);
    let podman = PodmanComposeRuntime::new(podman_process.clone());
    assert!(matches!(
        podman
            .preflight(&named, std::slice::from_ref(&resource), &[])
            .await,
        Err(MmError::Security { .. })
    ));
    assert_eq!(podman_process.calls()?, 2);
    Ok(())
}

#[tokio::test]
async fn every_runtime_uses_a_redacted_process_environment_for_direct_delivery()
-> Result<(), Box<dyn std::error::Error>> {
    const VALUE: &str = "DIRECT_RUNTIME_VALUE_MUST_NOT_BE_RECORDED";
    let mut target = target("app");
    target.interpolation_environment =
        mm::domain::runtime::RuntimeEnvironment::from_sensitive_values(BTreeMap::from([(
            "MM_ENV_OPERATION_1".to_owned(),
            VALUE.to_owned(),
        )]));

    let podman_process = QueueProcess::new([output("[]"), output("")]);
    PodmanComposeRuntime::new(podman_process.clone())
        .apply(&target)
        .await?;
    let docker_process = QueueProcess::new([output("[]"), output("")]);
    DockerComposeRuntime::new(docker_process.clone())
        .apply(&target)
        .await?;

    for process in [podman_process, docker_process] {
        let records = process.records()?;
        let serialized = serde_json::to_string(&records)?;
        assert!(!serialized.contains(VALUE));
        assert!(records.iter().any(|record| {
            record
                .environment_keys
                .contains(&"MM_ENV_OPERATION_1".to_owned())
        }));
        assert!(records.iter().all(|record| {
            !record
                .arguments
                .iter()
                .any(|argument| argument.contains(VALUE))
        }));
    }
    Ok(())
}

#[test]
fn capability_documentation_matches_executable_descriptors() -> Result<(), std::io::Error> {
    let documentation = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/reference/capability-matrix.md"),
    )?;
    assert_eq!(
        documentation
            .lines()
            .filter(|line| line.starts_with("descriptor:"))
            .count(),
        RuntimeKind::ALL.len(),
        "the matrix documents exactly the runtimes mm can run"
    );
    for kind in RuntimeKind::ALL {
        let descriptor = runtime_descriptor(kind);
        let encoded = descriptor
            .capabilities
            .iter()
            .map(|evidence| evidence.capability.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let line = format!("descriptor:{kind:?}={encoded}");
        assert!(documentation.lines().any(|candidate| candidate == line));
        assert!(
            descriptor
                .capabilities
                .iter()
                .all(|evidence| { !evidence.fixture.is_empty() && !evidence.live_job.is_empty() })
        );
    }
    Ok(())
}

#[test]
fn ownership_fixture_uses_exact_generation_kind_and_resource_labels() {
    let target = target("app");
    let ownership = ResourceOwnership::for_runtime_target(
        &target,
        ResourceKind::Stack,
        target.project_name.clone(),
        target.stack_name.clone(),
    );
    let labels = ownership
        .map(|ownership| ownership.labels())
        .unwrap_or_default();
    assert_eq!(labels["io.mm.generation"], "1");
    assert_eq!(labels["io.mm.resource-kind"], "stack");
    assert_eq!(labels["io.mm.resource"], "app");
    assert_eq!(
        labels["io.mm.project-id"],
        Uuid::from_u128(PROJECT_ID).to_string()
    );
    assert_eq!(labels["io.mm.environment"], "conformance");
    assert_eq!(labels["io.mm.stack"], "app");
}
