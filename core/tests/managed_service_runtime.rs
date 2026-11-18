use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mm::adapters::managed_service_runtime::SelectedTargetManagedServiceRuntime;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    LocalDeploymentRuntime, ManagedServiceRuntime, OwnedVolumeFile, RuntimeDriver, TargetStore,
    VolumeOwnership,
};
use mm::domain::error::MmError;
use mm::domain::managed_service::{ManagedServiceFile, ManagedServicePlanRequest};
use mm::domain::runtime::{
    ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy, ConvergenceState,
    ObservedProject, RenderedProject, ResourceEnsureResult, RuntimeCapability, RuntimeKind,
    RuntimeProbe, RuntimeResource, RuntimeTarget, ServiceIdentity, ServiceObservation,
    ServiceState,
};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

const IMAGE: &str = "registry.example.test/provider@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SENTINEL: &str = "managed-runtime-secret-sentinel-56d2";

#[derive(Default)]
struct FakePorts {
    effects: Mutex<Vec<String>>,
}

impl FakePorts {
    fn record(&self, effect: impl Into<String>) -> Result<(), MmError> {
        self.effects
            .lock()
            .map_err(|_| MmError::State {
                message: "fake runtime lock poisoned".to_owned(),
            })?
            .push(effect.into());
        Ok(())
    }

    fn effects(&self) -> Result<Vec<String>, MmError> {
        self.effects
            .lock()
            .map(|value| value.clone())
            .map_err(|_| MmError::State {
                message: "fake runtime lock poisoned".to_owned(),
            })
    }
}

#[async_trait::async_trait]
impl RuntimeDriver for FakePorts {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::DockerCompose
    }

    async fn probe(&self) -> RuntimeProbe {
        RuntimeProbe {
            kind: RuntimeKind::DockerCompose,
            available: true,
            engine_version: Some("fake".to_owned()),
            compose_provider: None,
            rootless: Some(true),
            socket_path: None,
            capabilities: vec![
                RuntimeCapability::ApplyCompose,
                RuntimeCapability::ObserveServices,
                RuntimeCapability::ContainerHealth,
                RuntimeCapability::ManageVolumes,
                RuntimeCapability::MutateVolumeFiles,
            ],
            diagnostics: Vec::new(),
        }
    }

    async fn render(&self, target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        Ok(RenderedProject {
            stack: target.stack_name.clone(),
            project_name: target.project_name.clone(),
            service_names: vec!["openbao".to_owned()],
            services: Default::default(),
            resources: Vec::new(),
            config_hash: target.artifact_hash.clone(),
        })
    }

    async fn observe(&self, target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        self.record("observe")?;
        Ok(ObservedProject {
            stack: target.stack_name.clone(),
            services: vec![ServiceObservation {
                identity: ServiceIdentity {
                    project_id: target.project_id,
                    environment: target.environment.clone(),
                    stack: target.stack_name.clone(),
                    module: None,
                    service: "openbao".to_owned(),
                },
                desired_count: 1,
                running_count: 1,
                healthy_count: 1,
                state: ServiceState::Running,
                exit_code: None,
                created_at: None,
                started_at: None,
                finished_at: None,
                revision: None,
                digest: None,
                diagnostic_code: None,
            }],
            containers: Vec::new(),
        })
    }

    async fn wait_for_convergence(
        &self,
        target: &RuntimeTarget,
        _policy: ConvergencePolicy,
        _cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        Ok(ConvergenceOutcome {
            state: ConvergenceState::Succeeded,
            polls: 1,
            observation: self.observe(target).await?,
            service: None,
            diagnostic_code: None,
            message: None,
        })
    }
}

#[async_trait::async_trait]
impl LocalDeploymentRuntime for FakePorts {
    async fn preflight(
        &self,
        _target: &NamedTarget,
        resources: &[RuntimeResource],
        projects: &[RuntimeTarget],
    ) -> Result<(), MmError> {
        assert_eq!(resources.len(), 4);
        assert_eq!(projects.len(), 1);
        self.record("preflight")
    }

    async fn ensure_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.record("ensure-network")?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn inspect_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<bool, MmError> {
        Ok(true)
    }

    async fn remove_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<(), MmError> {
        self.record("remove-network")
    }

    async fn ensure_volume(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.record(format!("ensure-volume:{name}"))?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn remove_volume(
        &self,
        _target: &NamedTarget,
        name: &str,
        _resource: &str,
    ) -> Result<(), MmError> {
        self.record(format!("remove-volume:{name}"))
    }

    async fn set_volume_owner(
        &self,
        _target: &NamedTarget,
        name: &str,
        _volume_resource: &str,
        ownership: VolumeOwnership,
    ) -> Result<(), MmError> {
        self.record(format!(
            "owner:{name}:{}:{}:{:o}",
            ownership.uid, ownership.gid, ownership.mode
        ))
    }

    async fn create_volume_file_if_absent(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
        _mode: u32,
    ) -> Result<(), MmError> {
        self.record("create-file")
    }

    async fn replace_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        file_name: &str,
        _source: &Path,
        _mode: u32,
    ) -> Result<(), MmError> {
        self.record(format!("replace:{file_name}"))
    }

    async fn replace_volume_file_owned(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        file: OwnedVolumeFile<'_>,
        _source: &Path,
    ) -> Result<(), MmError> {
        self.record(format!("replace-owned:{}", file.name))
    }

    async fn remove_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        file_name: &str,
    ) -> Result<(), MmError> {
        self.record(format!("remove-file:{file_name}"))
    }

    async fn apply(&self, _target: &RuntimeTarget) -> Result<(), MmError> {
        self.record("apply")
    }

    async fn remove(&self, _target: &RuntimeTarget) -> Result<(), MmError> {
        self.record("remove")
    }
}

struct Fixture {
    _temp: TempDir,
    targets: Arc<FilesystemTargetStore>,
    ports: Arc<FakePorts>,
    runtime: SelectedTargetManagedServiceRuntime,
    source: PathBuf,
}

impl Fixture {
    fn new(kind: RuntimeKind) -> Result<Self, MmError> {
        let temp = temp_dirs::tempdir().map_err(|source| MmError::Io {
            context: "creating managed runtime fixture".to_owned(),
            source,
        })?;
        let targets = Arc::new(FilesystemTargetStore::new(temp.path().join("targets.yaml")));
        let mut target = TargetDefinition::local(TargetId(Uuid::from_u128(1)));
        target.available_runtimes = vec![kind];
        targets.insert_target("local", target)?;
        let source = temp.path().join("config.hcl");
        std::fs::write(&source, "no secrets here\n").map_err(|source| MmError::Io {
            context: "writing fake config".to_owned(),
            source,
        })?;
        let ports = Arc::new(FakePorts::default());
        let deployment: Arc<dyn LocalDeploymentRuntime> = ports.clone();
        let inspector: Arc<dyn RuntimeDriver> = ports.clone();
        let target_store: Arc<dyn TargetStore> = targets.clone();
        let runtime = SelectedTargetManagedServiceRuntime::new(
            temp.path().join("artifacts"),
            target_store,
            kind,
            deployment,
            inspector,
        );
        Ok(Self {
            _temp: temp,
            targets,
            ports,
            runtime,
            source,
        })
    }

    fn request(&self, runtime: RuntimeKind, instance: &str) -> ManagedServicePlanRequest {
        ManagedServicePlanRequest {
            target: "local".to_owned(),
            runtime,
            provider_instance: instance.to_owned(),
            component: "openbao".to_owned(),
            image: IMAGE.to_owned(),
            host_port: 18_200,
            container_port: 8200,
            user: "100:1000".to_owned(),
            entrypoint: "bao".to_owned(),
            command: vec!["server".to_owned()],
            healthcheck: vec!["CMD".to_owned(), "bao".to_owned(), "status".to_owned()],
            config_mount: "/bao/config".to_owned(),
            data_mount: "/bao/data".to_owned(),
            config_files: vec![ManagedServiceFile {
                source: self.source.clone(),
                name: "config.hcl".to_owned(),
                mode: 0o600,
                uid: 100,
                gid: 1000,
            }],
            ingress_files: vec![ManagedServiceFile {
                source: self.source.clone(),
                name: "instance-openbao.dynamic.yaml".to_owned(),
                mode: 0o600,
                uid: 0,
                gid: 0,
            }],
            generation: "b".repeat(64),
        }
    }
}

#[tokio::test]
async fn fake_compose_ports_characterize_apply_observe_and_purge_lifecycle()
-> Result<(), Box<dyn std::error::Error>> {
    for kind in [RuntimeKind::DockerCompose, RuntimeKind::PodmanCompose] {
        let fixture = Fixture::new(kind)?;
        let plan = fixture
            .runtime
            .plan(fixture.request(kind, "team-secrets"))?;
        fixture.runtime.apply(&plan).await?;
        assert!(fixture.runtime.observe(&plan).await?.running);
        fixture.runtime.remove(&plan, true).await?;
        let effects = fixture.ports.effects()?;
        assert!(effects.iter().any(|effect| effect == "apply"));
        assert!(effects.iter().any(|effect| effect == "observe"));
        assert!(effects.iter().any(|effect| effect == "remove"));
        assert_eq!(
            effects
                .iter()
                .filter(|effect| effect.starts_with("remove-volume:"))
                .count(),
            2
        );
        let compose = std::fs::read_to_string(&plan.compose_path)?;
        assert!(compose.contains("cpus: 2.0"));
        assert!(compose.contains("mem_limit: 4g"));
        assert!(!compose.contains("docker.sock"));
        assert!(!compose.contains("podman.sock"));
        assert!(!compose.contains(SENTINEL));
    }
    Ok(())
}

#[tokio::test]
async fn wrong_runtime_stale_target_and_changed_hash_fail_without_effect()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(RuntimeKind::DockerCompose)?;
    assert!(
        fixture
            .runtime
            .plan(fixture.request(RuntimeKind::PodmanCompose, "team-secrets"))
            .is_err()
    );
    assert!(fixture.ports.effects()?.is_empty());

    let mut changed = fixture
        .runtime
        .plan(fixture.request(RuntimeKind::DockerCompose, "team-secrets"))?;
    changed.service.host_port += 1;
    assert!(fixture.runtime.apply(&changed).await.is_err());
    assert!(fixture.ports.effects()?.is_empty());

    let plan = fixture
        .runtime
        .plan(fixture.request(RuntimeKind::DockerCompose, "team-secrets"))?;
    let current = fixture.targets.resolve_target("local")?;
    fixture
        .targets
        .remove_target("local", &current.fingerprint)?;
    let mut replacement = TargetDefinition::local(TargetId(Uuid::from_u128(2)));
    replacement.available_runtimes = vec![RuntimeKind::DockerCompose];
    fixture.targets.insert_target("local", replacement)?;
    assert!(fixture.runtime.apply(&plan).await.is_err());
    assert!(fixture.ports.effects()?.is_empty());
    Ok(())
}

#[test]
fn resource_identity_is_target_instance_scoped() -> Result<(), Box<dyn std::error::Error>> {
    let compose = Fixture::new(RuntimeKind::DockerCompose)?;
    let first = compose
        .runtime
        .plan(compose.request(RuntimeKind::DockerCompose, "first"))?;
    let second = compose
        .runtime
        .plan(compose.request(RuntimeKind::DockerCompose, "second"))?;
    assert_ne!(first.identity.project_name, second.identity.project_name);
    assert_ne!(first.identity.data_volume, second.identity.data_volume);
    assert_eq!(
        first
            .service
            .config_files
            .iter()
            .filter(|file| file.name == "config.hcl")
            .count(),
        1
    );
    assert!(
        first
            .service
            .config_files
            .iter()
            .all(|file| !file.source.to_string_lossy().contains(SENTINEL))
    );
    Ok(())
}
