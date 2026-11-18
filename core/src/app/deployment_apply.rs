use std::collections::BTreeMap;
use std::sync::Arc;

use crate::app::ports::{
    Clock, IngressRouteRegistry, IngressVerifier, LocalDeploymentRuntime,
    PrivilegedIngressInstaller, ProjectDocumentStore, RootlessIngressInstaller, RuntimeDriver,
    RuntimeSecretDelivery, SchedulerRepository, StateStore, TargetOperationLock, TargetStore,
};
use crate::app::{DeploymentManager, WorkspaceSelection};
use crate::domain::deployment::{DeploymentPlan, DeploymentResult, DeploymentStackPlan};
use crate::domain::error::MmError;
use crate::domain::operation::{
    NewOperation, OperationEvent, OperationId, OperationKind, OperationPhase, OperationState,
};
use crate::domain::ownership::ResourceKind;
use crate::domain::runtime::{
    ConvergenceCancellation, ConvergencePolicy, ConvergenceState, RuntimeCapability,
    RuntimeResource, RuntimeTarget,
};
use crate::domain::target::NamedTarget;
use crate::domain::target_ingress::{TargetIngressPlan, ingress_dynamic_config_volume_name};
use crate::domain::workspace::{WorkspaceId, WorkspaceRegistration};

const INGRESS_NETWORK_RESOURCE: &str = "ingress-network";
const INGRESS_ACME_RESOURCE: &str = "ingress-acme";
const INGRESS_CONFIG_RESOURCE: &str = "ingress-config";

mod build_record;
mod cluster_ingress;
mod deploy_jobs;
mod environment_staging;
mod job_target;
mod lifecycle;
mod moves;
mod mutation;
mod preflight;
mod rollback;
mod rollout;
mod scaling;
pub use rollout::{StackOutcome, StackStep};

fn managed_network_resource(logical: &str) -> String {
    format!("app-network:{logical}")
}

#[derive(Clone)]
pub struct DeploymentExecutionPorts {
    pub documents: Arc<dyn ProjectDocumentStore>,
    pub routes: Arc<dyn IngressRouteRegistry>,
    pub state: Arc<dyn StateStore>,
    pub targets: Arc<dyn TargetStore>,
    pub runtime: Arc<dyn LocalDeploymentRuntime>,
    pub inspector: Arc<dyn RuntimeDriver>,
    pub locks: Arc<dyn TargetOperationLock>,
    pub verifier: Arc<dyn IngressVerifier>,
    pub rootless: Arc<dyn RootlessIngressInstaller>,
    pub privileged: Arc<dyn PrivilegedIngressInstaller>,
    pub clock: Arc<dyn Clock>,
    pub environments: crate::app::EnvironmentManager,
    pub environment_artifacts: Arc<dyn crate::app::ports::EnvironmentArtifactStore>,
    pub runtime_secrets: Arc<dyn RuntimeSecretDelivery>,
    pub scheduler: Arc<dyn SchedulerRepository>,
}

impl std::fmt::Debug for DeploymentExecutionPorts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeploymentExecutionPorts")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct DeploymentService {
    planner: DeploymentManager,
    ports: DeploymentExecutionPorts,
    /// Runs the jobs a deployment runs, and cleans up after cut-off runs.
    jobs: Option<Arc<dyn crate::app::ports::DeploymentJobs>>,
}

impl std::fmt::Debug for DeploymentService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeploymentService")
            .finish_non_exhaustive()
    }
}

fn retain_runtime_plan_revisions(
    plan: &crate::domain::runtime_secret::RuntimeSecretPlan,
    retain_previous_revision: bool,
    retained: &mut BTreeMap<String, std::collections::BTreeSet<String>>,
) {
    for item in &plan.items {
        let revisions = retained.entry(item.logical_name.clone()).or_default();
        revisions.insert(item.source_revision.clone());
        if retain_previous_revision && let Some(previous) = &item.previous_source_revision {
            revisions.insert(previous.clone());
        }
    }
}

struct DeploymentOperation {
    id: OperationId,
    previous: Option<DeploymentPlan>,
}

/// A reviewed plan whose operation is open: nothing is changed yet.
struct ReviewedApply {
    plan: DeploymentPlan,
    target: NamedTarget,
    operation: DeploymentOperation,
    /// The target's lock, held until the caller is done.
    guard: Box<dyn crate::app::ports::TargetOperationLockGuard>,
}

struct EnvironmentArtifactGuard {
    store: Arc<dyn crate::app::ports::EnvironmentArtifactStore>,
    bundle: Option<crate::domain::environment::PreparedEnvironmentBundle>,
    runtime_store: Arc<dyn RuntimeSecretDelivery>,
    runtime_bundle: Option<crate::domain::runtime_secret::PreparedRuntimeSecrets>,
}

impl EnvironmentArtifactGuard {
    fn new(
        store: Arc<dyn crate::app::ports::EnvironmentArtifactStore>,
        bundle: Option<crate::domain::environment::PreparedEnvironmentBundle>,
        runtime_store: Arc<dyn RuntimeSecretDelivery>,
        runtime_bundle: Option<crate::domain::runtime_secret::PreparedRuntimeSecrets>,
    ) -> Self {
        Self {
            store,
            bundle,
            runtime_store,
            runtime_bundle,
        }
    }

    fn bundle(&self) -> Option<&crate::domain::environment::PreparedEnvironmentBundle> {
        self.bundle.as_ref()
    }

    fn dispose(&mut self) -> Result<(), MmError> {
        let runtime = self
            .runtime_bundle
            .take()
            .map(|bundle| self.runtime_store.dispose(&bundle))
            .transpose()
            .map(|_| ());
        let environment = self
            .bundle
            .take()
            .map(|bundle| self.store.dispose(&bundle))
            .transpose()
            .map(|_| ());
        match (runtime, environment) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(runtime), Err(environment)) => Err(MmError::Security {
                message: format!(
                    "runtime secret cleanup failed ({runtime}) and environment cleanup failed ({environment})"
                ),
            }),
        }
    }
}

impl Drop for EnvironmentArtifactGuard {
    fn drop(&mut self) {
        if let Err(error) = self.dispose() {
            tracing::warn!(%error, "deployment environment guard cleanup failed");
        }
    }
}

fn require_plan_hash(plan: &DeploymentPlan, expected: &str) -> Result<(), MmError> {
    if plan.plan_hash == expected {
        Ok(())
    } else {
        Err(MmError::State {
            message: format!(
                "deployment plan changed (expected {expected}, current {}); review the new plan",
                plan.plan_hash
            ),
        })
    }
}

fn validate_target(plan: &DeploymentPlan, target: &NamedTarget) -> Result<(), MmError> {
    if target.name != plan.target_name
        || target.definition.id != plan.target_id
        || target.fingerprint != plan.target_fingerprint
        || target.definition.canonical_fingerprint() != plan.target_fingerprint
    {
        return Err(MmError::Security {
            message: "resolved target changed after the reviewed deployment plan".to_owned(),
        });
    }
    Ok(())
}

fn ingress_runtime_target(
    plan: &DeploymentPlan,
    target: &NamedTarget,
    ingress: &TargetIngressPlan,
) -> RuntimeTarget {
    RuntimeTarget {
        root: ingress.artifact.root.clone(),
        project_directory: ingress.artifact.root.clone(),
        target: Some(target.clone()),
        project_id: plan.project_id,
        environment: plan.environment.clone(),
        stack_name: "ingress".to_owned(),
        project_name: ingress.artifact.manifest.compose_project_name.clone(),
        artifact_hash: ingress.artifact.manifest.artifact_hash.clone(),
        compose_files: vec![ingress.artifact.compose_path.clone()],
        user_compose_file_count: 1,
        interpolation_environment: Default::default(),
        installation_scoped: true,
        registry_credentials: Vec::new(),
    }
}

fn stack_runtime_target(
    plan: &DeploymentPlan,
    target: &NamedTarget,
    stack: &DeploymentStackPlan,
    environment_bundle: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
    rollback_secrets: bool,
) -> RuntimeTarget {
    let mut compose_files = stack.artifact.compose_files.clone();
    if let Some(bundle) = environment_bundle {
        let overlay = bundle.root.join(format!("{}.compose.yaml", stack.stack));
        if overlay.is_file() {
            compose_files.push(overlay);
        }
        let secret_overlays = if rollback_secrets {
            &bundle.runtime_secret_rollback_overlays
        } else {
            &bundle.runtime_secret_overlays
        };
        if let Some((_, overlay)) = secret_overlays
            .iter()
            .find(|(stack_name, _)| stack_name == &stack.stack)
        {
            compose_files.push(overlay.clone());
        }
    }
    RuntimeTarget {
        root: stack.project_directory.clone(),
        project_directory: stack.project_directory.clone(),
        target: Some(target.clone()),
        project_id: plan.project_id,
        environment: plan.environment.clone(),
        stack_name: stack.stack.clone(),
        project_name: stack.compose_project_name.clone(),
        artifact_hash: stack.artifact_hash.clone(),
        compose_files,
        user_compose_file_count: stack.artifact.manifest.user_files.len(),
        interpolation_environment: environment_bundle
            .map(|bundle| bundle.interpolation_environment.clone())
            .unwrap_or_default(),
        installation_scoped: false,
        registry_credentials: environment_bundle
            .map(|bundle| bundle.registry_credentials.clone())
            .unwrap_or_default(),
    }
}

fn planned_resources(plan: &DeploymentPlan) -> Vec<RuntimeResource> {
    let mut resources = BTreeMap::<(ResourceKind, String), RuntimeResource>::new();
    for network in &plan.networks {
        resources.insert(
            (ResourceKind::Network, network.physical_name.clone()),
            RuntimeResource {
                kind: ResourceKind::Network,
                name: network.physical_name.clone(),
                resource: managed_network_resource(&network.logical_name),
            },
        );
    }
    for volume in plan
        .preserved_volumes
        .iter()
        .chain(&plan.removed_preserved_volumes)
    {
        resources.insert(
            (ResourceKind::Volume, volume.physical_name.clone()),
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: volume.physical_name.clone(),
                resource: volume.resource.clone(),
            },
        );
    }
    for ingress in plan
        .static_ingress
        .iter()
        .chain(plan.removed_ingress.iter())
    {
        let manifest = &ingress.artifact.manifest;
        resources.insert(
            (ResourceKind::Network, manifest.network_name.clone()),
            RuntimeResource {
                kind: ResourceKind::Network,
                name: manifest.network_name.clone(),
                resource: INGRESS_NETWORK_RESOURCE.to_owned(),
            },
        );
        resources.insert(
            (ResourceKind::Volume, manifest.acme_volume_name.clone()),
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: manifest.acme_volume_name.clone(),
                resource: INGRESS_ACME_RESOURCE.to_owned(),
            },
        );
        resources.insert(
            (
                ResourceKind::Volume,
                manifest.dynamic_config_volume_name.clone(),
            ),
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: manifest.dynamic_config_volume_name.clone(),
                resource: INGRESS_CONFIG_RESOURCE.to_owned(),
            },
        );
    }
    if plan.route_state_changed || plan.expected_route_config_digest.is_some() {
        let volume = ingress_dynamic_config_volume_name(plan.target_id);
        resources.insert(
            (ResourceKind::Volume, volume.clone()),
            RuntimeResource {
                kind: ResourceKind::Volume,
                name: volume,
                resource: INGRESS_CONFIG_RESOURCE.to_owned(),
            },
        );
    }
    resources.into_values().collect()
}

fn planned_runtime_targets(plan: &DeploymentPlan, target: &NamedTarget) -> Vec<RuntimeTarget> {
    let mut projects = BTreeMap::<String, RuntimeTarget>::new();
    for ingress in plan
        .static_ingress
        .iter()
        .chain(plan.removed_ingress.iter())
    {
        let runtime_target = ingress_runtime_target(plan, target, ingress);
        projects.insert(runtime_target.project_name.clone(), runtime_target);
    }
    for stack in plan.stacks.iter().chain(plan.removed_stacks.iter()) {
        let runtime_target = stack_runtime_target(plan, target, stack, None, false);
        projects.insert(runtime_target.project_name.clone(), runtime_target);
    }
    projects.into_values().collect()
}

fn dispose_bundle_root(
    store: &dyn crate::app::ports::EnvironmentArtifactStore,
    root: &std::path::Path,
) -> Result<(), MmError> {
    let Some(name) = root.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let Ok(operation_id) = uuid::Uuid::parse_str(name) else {
        return Ok(());
    };
    store.dispose(&crate::domain::environment::PreparedEnvironmentBundle {
        operation_id: OperationId(operation_id),
        root: root.to_path_buf(),
        manifest: crate::domain::environment::CompiledEnvironmentManifest {
            project_name: String::new(),
            environment: String::new(),
            input_hash: String::new(),
            services: Vec::new(),
        },
        services: Vec::new(),
        interpolation_environment: Default::default(),
        runtime_secret_overlays: Vec::new(),
        runtime_secret_rollback_overlays: Vec::new(),
        registry_credentials: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
