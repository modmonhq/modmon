use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use atomic_write_file::AtomicWriteFile;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_private_file, ensure_user_controlled_directory,
};
use crate::app::ports::{
    LocalDeploymentRuntime, ManagedServiceRuntime, OwnedVolumeFile, RuntimeDriver, TargetStore,
    VolumeOwnership,
};
use crate::domain::error::MmError;
use crate::domain::managed_service::{
    MANAGED_SERVICE_SCHEMA, ManagedServiceIdentity, ManagedServicePlan, ManagedServicePlanRequest,
    ManagedServiceSpec, ManagedServiceStatus,
};
use crate::domain::ownership::{ResourceKind, ResourceOwnership, validate_resource_name};
use crate::domain::runtime::{
    RuntimeCapability, RuntimeKind, RuntimeResource, RuntimeTarget, ServiceState,
};
use crate::domain::target::{NamedTarget, TargetTransport};
use crate::domain::workspace::ProjectId;

const DATA_RESOURCE: &str = "managed-secret-data";
const CONFIG_RESOURCE: &str = "managed-secret-config";
const INGRESS_NETWORK_RESOURCE: &str = "ingress-network";
const INGRESS_CONFIG_RESOURCE: &str = "ingress-config";

#[derive(Clone)]
pub struct SelectedTargetManagedServiceRuntime {
    root: PathBuf,
    targets: Arc<dyn TargetStore>,
    kind: RuntimeKind,
    runtime: Arc<dyn LocalDeploymentRuntime>,
    inspector: Arc<dyn RuntimeDriver>,
}

impl std::fmt::Debug for SelectedTargetManagedServiceRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SelectedTargetManagedServiceRuntime")
            .field("root", &self.root)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl SelectedTargetManagedServiceRuntime {
    #[must_use]
    pub fn new(
        root: PathBuf,
        targets: Arc<dyn TargetStore>,
        kind: RuntimeKind,
        runtime: Arc<dyn LocalDeploymentRuntime>,
        inspector: Arc<dyn RuntimeDriver>,
    ) -> Self {
        Self {
            root,
            targets,
            kind,
            runtime,
            inspector,
        }
    }

    fn resolve_plan_target(
        &self,
        request: &ManagedServicePlanRequest,
    ) -> Result<NamedTarget, MmError> {
        if request.runtime != self.kind {
            return Err(MmError::Security {
                message: format!(
                    "managed service requested {:?}, but the selected runtime is {:?}",
                    request.runtime, self.kind
                ),
            });
        }
        let target = self.targets.resolve_target(&request.target)?;
        if !target.definition.supports_runtime(self.kind) {
            return Err(MmError::unsupported_capability(
                self.kind,
                Some(&request.target),
                "managed secret backend planning",
                vec![RuntimeCapability::ApplyCompose],
                None,
            ));
        }
        if !matches!(target.definition.transport, TargetTransport::Local) {
            return Err(MmError::unsupported_capability(
                self.kind,
                Some(&request.target),
                "managed secret backend remote execution through mmd",
                vec![RuntimeCapability::RemoteTransport],
                None,
            ));
        }
        Ok(target)
    }

    fn exact_target(&self, plan: &ManagedServicePlan) -> Result<NamedTarget, MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        if plan.identity.runtime != self.kind {
            return Err(MmError::Security {
                message: "managed service plan belongs to another selected runtime".to_owned(),
            });
        }
        let target = self.targets.resolve_target(&plan.identity.target)?;
        if target.definition.id != plan.identity.target_id
            || target.definition.canonical_fingerprint() != plan.identity.target_fingerprint
        {
            return Err(MmError::Security {
                message: "managed service target changed after review".to_owned(),
            });
        }
        Ok(target)
    }

    fn runtime_target(plan: &ManagedServicePlan, target: &NamedTarget) -> RuntimeTarget {
        RuntimeTarget {
            root: plan.artifact_root.clone(),
            project_directory: plan.artifact_root.clone(),
            target: Some(target.clone()),
            project_id: ProjectId(uuid::Uuid::nil()),
            environment: "managed-services".to_owned(),
            stack_name: format!("managed-secret-{}", plan.identity.provider_instance),
            project_name: plan.identity.project_name.clone(),
            artifact_hash: plan.plan_hash.clone(),
            compose_files: vec![plan.compose_path.clone()],
            user_compose_file_count: 1,
            interpolation_environment: Default::default(),
            installation_scoped: true,
            registry_credentials: Vec::new(),
        }
    }

    fn resources(plan: &ManagedServicePlan) -> Vec<RuntimeResource> {
        vec![
            RuntimeResource {
                kind: ResourceKind::Network,
                name: plan.identity.ingress_network.clone(),
                resource: INGRESS_NETWORK_RESOURCE.to_owned(),
            },
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: plan.identity.ingress_config_volume.clone(),
                resource: INGRESS_CONFIG_RESOURCE.to_owned(),
            },
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: plan.identity.data_volume.clone(),
                resource: DATA_RESOURCE.to_owned(),
            },
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: plan.identity.config_volume.clone(),
                resource: CONFIG_RESOURCE.to_owned(),
            },
        ]
    }

    async fn require_capabilities(&self, target: &NamedTarget) -> Result<(), MmError> {
        self.inspector.probe().await.require_capabilities(
            "managed secret backend lifecycle",
            &[
                RuntimeCapability::ApplyCompose,
                RuntimeCapability::ObserveServices,
                RuntimeCapability::ContainerHealth,
                RuntimeCapability::ManageVolumes,
                RuntimeCapability::MutateVolumeFiles,
            ],
            Some(&target.name),
        )
    }
}

#[async_trait]
impl ManagedServiceRuntime for SelectedTargetManagedServiceRuntime {
    fn kind(&self) -> RuntimeKind {
        self.kind
    }

    fn plan(&self, request: ManagedServicePlanRequest) -> Result<ManagedServicePlan, MmError> {
        validate_request(&request)?;
        let target = self.resolve_plan_target(&request)?;
        let identity_digest = resource_digest(
            target.definition.id,
            &request.provider_instance,
            &request.component,
        );
        let identity = ManagedServiceIdentity {
            target: target.name.clone(),
            target_id: target.definition.id,
            target_fingerprint: target.definition.canonical_fingerprint(),
            runtime: self.kind,
            provider_instance: request.provider_instance,
            component: request.component,
            project_name: format!("mm-managed-{identity_digest}"),
            service_name: format!("mm-managed-service-{identity_digest}"),
            data_volume: format!("mm-managed-data-{identity_digest}"),
            config_volume: format!("mm-managed-config-{identity_digest}"),
            ingress_network: crate::domain::target_ingress::ingress_network_name(
                target.definition.id,
            ),
            ingress_config_volume:
                crate::domain::target_ingress::ingress_dynamic_config_volume_name(
                    target.definition.id,
                ),
        };
        let generation = request.generation;
        let artifact_root = self
            .root
            .join(target.definition.id.0.to_string())
            .join(&identity.provider_instance)
            .join(&generation);
        let compose_path = artifact_root.join("compose.yaml");
        let mut plan = ManagedServicePlan {
            schema: MANAGED_SERVICE_SCHEMA,
            identity,
            service: ManagedServiceSpec {
                image: request.image,
                host_port: request.host_port,
                container_port: request.container_port,
                user: request.user,
                entrypoint: request.entrypoint,
                command: request.command,
                healthcheck: request.healthcheck,
                config_mount: request.config_mount,
                data_mount: request.data_mount,
                config_files: request.config_files,
                ingress_files: request.ingress_files,
                generation,
            },
            artifact_root,
            compose_path,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::State { message })?;
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        write_private(&plan.compose_path, render_compose(&plan, &target)?)?;
        Ok(plan)
    }

    async fn apply(&self, plan: &ManagedServicePlan) -> Result<(), MmError> {
        let target = self.exact_target(plan)?;
        self.require_capabilities(&target).await?;
        let runtime_target = Self::runtime_target(plan, &target);
        self.runtime
            .preflight(
                &target,
                &Self::resources(plan),
                std::slice::from_ref(&runtime_target),
            )
            .await?;
        self.runtime
            .ensure_volume(&target, &plan.identity.data_volume, DATA_RESOURCE)
            .await?;
        self.runtime
            .ensure_volume(&target, &plan.identity.config_volume, CONFIG_RESOURCE)
            .await?;
        self.runtime
            .set_volume_owner(
                &target,
                &plan.identity.data_volume,
                DATA_RESOURCE,
                VolumeOwnership {
                    uid: 100,
                    gid: 1000,
                    mode: 0o700,
                },
            )
            .await?;
        self.runtime
            .set_volume_owner(
                &target,
                &plan.identity.config_volume,
                CONFIG_RESOURCE,
                VolumeOwnership {
                    uid: 100,
                    gid: 1000,
                    mode: 0o700,
                },
            )
            .await?;
        for file in &plan.service.config_files {
            self.runtime
                .replace_volume_file_owned(
                    &target,
                    &plan.identity.config_volume,
                    CONFIG_RESOURCE,
                    OwnedVolumeFile {
                        name: &file.name,
                        ownership: VolumeOwnership {
                            uid: file.uid,
                            gid: file.gid,
                            mode: file.mode,
                        },
                    },
                    &file.source,
                )
                .await?;
        }
        for file in &plan.service.ingress_files {
            self.runtime
                .replace_volume_file_owned(
                    &target,
                    &plan.identity.ingress_config_volume,
                    INGRESS_CONFIG_RESOURCE,
                    OwnedVolumeFile {
                        name: &file.name,
                        ownership: VolumeOwnership {
                            uid: file.uid,
                            gid: file.gid,
                            mode: file.mode,
                        },
                    },
                    &file.source,
                )
                .await?;
        }
        self.runtime.apply(&runtime_target).await
    }

    async fn observe(&self, plan: &ManagedServicePlan) -> Result<ManagedServiceStatus, MmError> {
        let target = self.exact_target(plan)?;
        let observed = self
            .inspector
            .observe(&Self::runtime_target(plan, &target))
            .await?;
        let service = observed
            .services
            .iter()
            .find(|service| service.identity.service == plan.identity.component);
        let container = observed
            .containers
            .iter()
            .find(|container| container.service == plan.identity.component);
        Ok(ManagedServiceStatus {
            running: service.is_some_and(|service| service.state == ServiceState::Running),
            healthy: service.is_some_and(|service| {
                service.running_count > 0 && service.healthy_count == service.running_count
            }),
            image: container.map(|container| container.image.clone()),
        })
    }

    async fn remove(&self, plan: &ManagedServicePlan, purge_data: bool) -> Result<(), MmError> {
        let target = self.exact_target(plan)?;
        self.require_capabilities(&target).await?;
        self.runtime
            .preflight(
                &target,
                &Self::resources(plan),
                &[Self::runtime_target(plan, &target)],
            )
            .await?;
        let mut failures = Vec::new();
        if let Err(error) = self
            .runtime
            .remove(&Self::runtime_target(plan, &target))
            .await
        {
            failures.push(error.to_string());
        }
        for file in &plan.service.ingress_files {
            if let Err(error) = self
                .runtime
                .remove_volume_file(
                    &target,
                    &plan.identity.ingress_config_volume,
                    INGRESS_CONFIG_RESOURCE,
                    &file.name,
                )
                .await
            {
                failures.push(error.to_string());
            }
        }
        if purge_data {
            for (volume, resource) in [
                (&plan.identity.config_volume, CONFIG_RESOURCE),
                (&plan.identity.data_volume, DATA_RESOURCE),
            ] {
                if let Err(error) = self.runtime.remove_volume(&target, volume, resource).await {
                    failures.push(error.to_string());
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(MmError::RuntimeResponse {
                message: format!(
                    "managed service cleanup failed; secondary failures: {}",
                    failures.join("; ")
                ),
            })
        }
    }
}

fn validate_request(request: &ManagedServicePlanRequest) -> Result<(), MmError> {
    crate::domain::target::validate_target_name(&request.target)
        .map_err(|message| MmError::Security { message })?;
    validate_resource_name(&request.provider_instance)?;
    validate_resource_name(&request.component)?;
    if request.host_port == 0
        || request.container_port == 0
        || request.command.is_empty()
        || request.healthcheck.is_empty()
        || request.generation.len() != 64
        || !request
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !request.image.contains("@sha256:")
    {
        return Err(MmError::Security {
            message: "managed service request is incomplete or not immutable".to_owned(),
        });
    }
    Ok(())
}

fn resource_digest(
    target_id: crate::domain::target::TargetId,
    instance: &str,
    component: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [
        target_id.0.to_string(),
        instance.to_owned(),
        component.to_owned(),
    ] {
        hasher.update(&(value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex()[..20].to_owned()
}

fn render_compose(plan: &ManagedServicePlan, target: &NamedTarget) -> Result<Vec<u8>, MmError> {
    let ownership = ResourceOwnership::for_named_target(
        target,
        ResourceKind::Stack,
        plan.identity.project_name.clone(),
        format!("managed-secret-{}", plan.identity.provider_instance),
    );
    let labels = ownership
        .labels()
        .into_iter()
        .map(|(key, value)| format!("      {key}: {}\n", yaml_string(&value)))
        .collect::<String>();
    let command = serde_json::to_string(&plan.service.command).map_err(|error| MmError::State {
        message: format!("could not render managed service command: {error}"),
    })?;
    let healthcheck =
        serde_json::to_string(&plan.service.healthcheck).map_err(|error| MmError::State {
            message: format!("could not render managed service healthcheck: {error}"),
        })?;
    let compose = format!(
        "services:\n  {component}:\n    image: {image}\n    container_name: {container}\n    user: {user}\n    entrypoint: {entrypoint}\n    command: {command}\n    restart: unless-stopped\n    read_only: true\n    cap_drop: [ALL]\n    security_opt: [no-new-privileges:true]\n    tmpfs: [/tmp:rw,noexec,nosuid,nodev,size=64m]\n    cpus: 2.0\n    mem_limit: 4g\n    pids_limit: 512\n    ports: [\"127.0.0.1:{host_port}:{container_port}\"]\n    volumes:\n      - {config_key}:{config_mount}:ro\n      - {data_key}:{data_mount}\n    networks: [{network_key}]\n    healthcheck:\n      test: {healthcheck}\n      interval: 2s\n      timeout: 2s\n      retries: 30\n    labels:\n{labels}      io.mm.managed-provider-instance: {instance}\n      io.mm.managed-service-generation: {generation}\nnetworks:\n  {network_key}:\n    external: true\n    name: {network}\nvolumes:\n  {config_key}:\n    external: true\n    name: {config_volume}\n  {data_key}:\n    external: true\n    name: {data_volume}\n",
        component = plan.identity.component,
        image = yaml_string(&plan.service.image),
        container = yaml_string(&plan.identity.service_name),
        user = yaml_string(&plan.service.user),
        entrypoint = yaml_string(&plan.service.entrypoint),
        host_port = plan.service.host_port,
        container_port = plan.service.container_port,
        config_volume = yaml_string(&plan.identity.config_volume),
        config_mount = plan.service.config_mount,
        data_volume = yaml_string(&plan.identity.data_volume),
        data_mount = plan.service.data_mount,
        network_key = "managed-ingress",
        network = yaml_string(&plan.identity.ingress_network),
        config_key = "managed-config",
        data_key = "managed-data",
        instance = yaml_string(&plan.identity.provider_instance),
        generation = yaml_string(&plan.service.generation),
    );
    Ok(compose.into_bytes())
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"invalid\"".to_owned())
}

fn write_private(path: &Path, bytes: Vec<u8>) -> Result<(), MmError> {
    let parent = path.parent().ok_or_else(|| MmError::State {
        message: "managed service artifact path has no parent".to_owned(),
    })?;
    ensure_no_symlink_components(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|source| MmError::Io {
                context: format!(
                    "creating managed service artifact directory {}",
                    parent.display()
                ),
                source,
            })?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!(
                    "securing managed service artifact directory {}",
                    parent.display()
                ),
                source,
            }
        })?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(parent).map_err(|source| MmError::Io {
        context: format!(
            "creating managed service artifact directory {}",
            parent.display()
        ),
        source,
    })?;
    ensure_user_controlled_directory(parent)?;
    ensure_no_symlink_components(path)?;
    let mut options = AtomicWriteFile::options();
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .preserve_mode(false)
            .preserve_owner(false);
    }
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("opening managed service artifact {}", path.display()),
        source,
    })?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing managed service artifact {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing managed service artifact {}", path.display()),
        source,
    })?;
    ensure_private_file(path)
}

#[cfg(test)]
#[path = "managed_service_runtime_tests.rs"]
mod tests;
