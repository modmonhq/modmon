use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::*;
use crate::adapters::target::FilesystemTargetStore;
use crate::domain::managed_service::{ManagedServiceFile, ManagedServicePlanRequest};
use crate::domain::runtime::{
    ContainerObservation, ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy,
    ConvergenceState, ObservedProject, RenderedProject, ResourceEnsureResult, RuntimeProbe,
    ServiceIdentity, ServiceObservation,
};
use crate::domain::target::{TargetDefinition, TargetId};

#[derive(Debug)]
struct LocalRuntime {
    fail_cleanup: bool,
}

fn cleanup_result(fail: bool) -> Result<(), MmError> {
    if fail {
        Err(MmError::RuntimeResponse {
            message: "injected cleanup failure".to_owned(),
        })
    } else {
        Ok(())
    }
}

#[async_trait]
impl LocalDeploymentRuntime for LocalRuntime {
    async fn preflight(
        &self,
        _target: &NamedTarget,
        _resources: &[RuntimeResource],
        _projects: &[RuntimeTarget],
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn ensure_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
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
        Ok(())
    }

    async fn ensure_volume(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        Ok(ResourceEnsureResult::Created)
    }

    async fn remove_volume(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<(), MmError> {
        cleanup_result(self.fail_cleanup)
    }

    async fn set_volume_owner(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _ownership: VolumeOwnership,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn create_volume_file_if_absent(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
        _mode: u32,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn replace_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
        _source: &Path,
        _mode: u32,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn remove_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
    ) -> Result<(), MmError> {
        cleanup_result(self.fail_cleanup)
    }

    async fn apply(&self, _target: &RuntimeTarget) -> Result<(), MmError> {
        Ok(())
    }

    async fn remove(&self, _target: &RuntimeTarget) -> Result<(), MmError> {
        cleanup_result(self.fail_cleanup)
    }
}

#[derive(Debug)]
struct Inspector;

#[async_trait]
impl RuntimeDriver for Inspector {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::DockerCompose
    }

    async fn probe(&self) -> RuntimeProbe {
        RuntimeProbe {
            kind: self.kind(),
            available: true,
            engine_version: Some("test".to_owned()),
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

    async fn render(&self, _target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        Err(MmError::State {
            message: "render is unused by the managed runtime test".to_owned(),
        })
    }

    async fn observe(&self, target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        let identity = ServiceIdentity {
            project_id: target.project_id,
            environment: target.environment.clone(),
            stack: target.stack_name.clone(),
            module: None,
            service: "server".to_owned(),
        };
        Ok(ObservedProject {
            stack: target.stack_name.clone(),
            services: vec![ServiceObservation {
                identity,
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
            containers: vec![ContainerObservation {
                id: "container".to_owned(),
                name: "managed-server".to_owned(),
                service: "server".to_owned(),
                image: "example@sha256:test".to_owned(),
                state: "running".to_owned(),
                status: "healthy".to_owned(),
                health: Some("healthy".to_owned()),
                restart_count: None,
                exit_code: None,
                created_at: None,
                started_at: None,
                finished_at: None,
                revision: None,
                digest: None,
                diagnostic_code: None,
                error: None,
                labels: BTreeMap::new(),
                node: None,
                history: false,
            }],
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

fn request(source: &Path) -> ManagedServicePlanRequest {
    let file = ManagedServiceFile {
        source: source.to_path_buf(),
        name: "server.hcl".to_owned(),
        mode: 0o600,
        uid: 100,
        gid: 1000,
    };
    ManagedServicePlanRequest {
        target: "local".to_owned(),
        runtime: RuntimeKind::DockerCompose,
        provider_instance: "secrets".to_owned(),
        component: "server".to_owned(),
        image: format!("example/server@sha256:{}", "a".repeat(64)),
        host_port: 8200,
        container_port: 8200,
        user: "100:1000".to_owned(),
        entrypoint: "/bin/server".to_owned(),
        command: vec!["server".to_owned(), "-config=/config/server.hcl".to_owned()],
        healthcheck: vec!["CMD".to_owned(), "server".to_owned(), "status".to_owned()],
        config_mount: "/config".to_owned(),
        data_mount: "/data".to_owned(),
        config_files: vec![file.clone()],
        ingress_files: vec![ManagedServiceFile {
            name: "secrets.yaml".to_owned(),
            ..file
        }],
        generation: "b".repeat(64),
    }
}

fn runtime(
    root: &Path,
    targets: Arc<dyn TargetStore>,
    kind: RuntimeKind,
    fail_cleanup: bool,
) -> SelectedTargetManagedServiceRuntime {
    SelectedTargetManagedServiceRuntime::new(
        root.to_path_buf(),
        targets,
        kind,
        Arc::new(LocalRuntime { fail_cleanup }),
        Arc::new(Inspector),
    )
}

#[tokio::test]
async fn managed_service_lifecycle_is_bound_to_the_reviewed_local_target()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let source = temp.path().join("source.hcl");
    std::fs::write(&source, b"storage = data")?;
    let store = Arc::new(FilesystemTargetStore::new(temp.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(901))),
    )?;
    let selected = runtime(
        &temp.path().join("artifacts"),
        store.clone(),
        RuntimeKind::DockerCompose,
        false,
    );
    assert_eq!(selected.kind(), RuntimeKind::DockerCompose);
    assert!(format!("{selected:?}").contains("DockerCompose"));

    let plan = selected.plan(request(&source))?;
    assert!(plan.compose_path.is_file());
    let compose = std::fs::read_to_string(&plan.compose_path)?;
    assert!(compose.contains("read_only: true"));
    assert_eq!(
        SelectedTargetManagedServiceRuntime::resources(&plan).len(),
        4
    );

    selected.apply(&plan).await?;
    let status = selected.observe(&plan).await?;
    assert!(status.running && status.healthy);
    assert_eq!(status.image.as_deref(), Some("example@sha256:test"));
    selected.remove(&plan, false).await?;

    let failing = runtime(
        &temp.path().join("artifacts"),
        store.clone(),
        RuntimeKind::DockerCompose,
        true,
    );
    assert!(failing.remove(&plan, true).await.is_err());

    let mut tampered = plan.clone();
    tampered.identity.target_id = TargetId(uuid::Uuid::from_u128(902));
    tampered.plan_hash = tampered.canonical_hash()?;
    assert!(selected.observe(&tampered).await.is_err());
    Ok(())
}

#[test]
fn planning_rejects_invalid_and_mismatched_runtime_requests()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let source = temp.path().join("source.hcl");
    std::fs::write(&source, b"config")?;
    let store = Arc::new(FilesystemTargetStore::new(temp.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(903))),
    )?;
    let selected = runtime(
        &temp.path().join("artifacts"),
        store.clone(),
        RuntimeKind::DockerCompose,
        false,
    );

    let mut invalid = request(&source);
    invalid.host_port = 0;
    assert!(selected.plan(invalid).is_err());
    let mut mismatch = request(&source);
    mismatch.runtime = RuntimeKind::PodmanCompose;
    assert!(selected.plan(mismatch).is_err());

    assert!(write_private(Path::new("/"), b"invalid".to_vec()).is_err());
    assert_ne!(
        resource_digest(TargetId(uuid::Uuid::nil()), "a", "bc"),
        resource_digest(TargetId(uuid::Uuid::nil()), "ab", "c")
    );
    Ok(())
}
