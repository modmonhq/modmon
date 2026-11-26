use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::target::NamedTarget;
use crate::domain::workspace::ProjectId;

pub const COMMON_INHERITED_ENVIRONMENT: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TMP",
    "TEMP",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "DBUS_SESSION_BUS_ADDRESS",
    "SSH_AUTH_SOCK",
];

#[path = "runtime/inherited.rs"]
mod inherited;
pub use inherited::{DOCKER_INHERITED_ENVIRONMENT, PODMAN_INHERITED_ENVIRONMENT};
#[path = "runtime/probe.rs"]
mod probe;
pub use probe::{DOCKER_USERNS_REMAP, PROBE_TIMEOUT_CODE, probe_failure_code};
#[path = "runtime/registry.rs"]
mod registry;
pub use registry::{RegistryCredential, registry_auth_config};
#[path = "runtime/tally.rs"]
mod tally;
pub use tally::{
    HEALTHCHECKS_LABEL, ServiceTally, current_containers, state_across_nodes, tally_service,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeKind {
    PodmanCompose,
    DockerCompose,
}

impl RuntimeKind {
    pub const ALL: [Self; 2] = [Self::DockerCompose, Self::PodmanCompose];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PodmanCompose => "podman-compose",
            Self::DockerCompose => "docker-compose",
        }
    }

    #[must_use]
    pub const fn inherited_environment(self) -> &'static [&'static str] {
        match self {
            Self::PodmanCompose => PODMAN_INHERITED_ENVIRONMENT,
            Self::DockerCompose => DOCKER_INHERITED_ENVIRONMENT,
        }
    }

    #[must_use]
    pub const fn apply_capability(self) -> RuntimeCapability {
        match self {
            Self::PodmanCompose | Self::DockerCompose => RuntimeCapability::ApplyCompose,
        }
    }
}

impl std::str::FromStr for RuntimeKind {
    type Err = String;

    /// A driver name. `docker-swarm`, which earlier releases accepted, is
    /// refused with what replaces it.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "podman-compose" => Ok(Self::PodmanCompose),
            "docker-compose" => Ok(Self::DockerCompose),
            "docker-swarm" => Err(crate::domain::config::removed::DOCKER_SWARM_DRIVER.to_owned()),
            other => Err(format!(
                "unknown runtime driver '{other}'; use docker-compose or podman-compose"
            )),
        }
    }
}

impl<'de> Deserialize<'de> for RuntimeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Behavioral promise advertised by a runtime probe.
///
/// A capability may be present only when every required operation is
/// implemented, fails closed when unsupported, and is covered by a fixture or
/// live test. Empty `Ok(())` placeholders are not implementations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapability {
    RenderCompose,
    ApplyCompose,
    ObserveServices,
    ContainerHealth,
    StreamLogs,
    Exec,
    ResolveImageDigest,
    Rollback,
    ManageNetworks,
    ManageVolumes,
    MutateVolumeFiles,
    RemoteTransport,
    DirectEnvironment,
    BindMounts,
    RuntimeSecrets,
    OneShotJobs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeCapabilityEvidence {
    pub capability: RuntimeCapability,
    pub fixture: &'static str,
    pub live_job: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDescriptor {
    pub kind: RuntimeKind,
    pub capabilities: &'static [RuntimeCapabilityEvidence],
}

impl RuntimeDescriptor {
    #[must_use]
    pub fn advertised_capabilities(self) -> Vec<RuntimeCapability> {
        self.capabilities
            .iter()
            .map(|evidence| evidence.capability)
            .collect()
    }
}

const PODMAN_CAPABILITIES: &[RuntimeCapabilityEvidence] = &[
    evidence(
        RuntimeCapability::OneShotJobs,
        "scheduler",
        "live-jobs-podman",
    ),
    evidence(
        RuntimeCapability::StreamLogs,
        "runtime_observability",
        "live-observability-podman",
    ),
    evidence(
        RuntimeCapability::Exec,
        "runtime_observability",
        "live-observability-podman",
    ),
    evidence(
        RuntimeCapability::RuntimeSecrets,
        "runtime_secret_delivery",
        "live-secret-podman",
    ),
    evidence(
        RuntimeCapability::BindMounts,
        "development_mounts",
        "live-development-mounts-podman",
    ),
    evidence(
        RuntimeCapability::DirectEnvironment,
        "environment_delivery",
        "live-podman-compose",
    ),
    evidence(
        RuntimeCapability::RenderCompose,
        "runtime_conformance",
        "live-podman-compose",
    ),
    evidence(
        RuntimeCapability::ApplyCompose,
        "runtime_conformance",
        "live-podman-compose",
    ),
    evidence(
        RuntimeCapability::ObserveServices,
        "runtime_conformance",
        "live-podman-compose",
    ),
    evidence(
        RuntimeCapability::ContainerHealth,
        "runtime_conformance",
        "live-podman-compose",
    ),
    evidence(
        RuntimeCapability::ManageNetworks,
        "runtime_conformance",
        "live-podman-compose",
    ),
    evidence(
        RuntimeCapability::ManageVolumes,
        "runtime_conformance",
        "live-podman-compose",
    ),
    evidence(
        RuntimeCapability::MutateVolumeFiles,
        "runtime_conformance",
        "live-podman-compose",
    ),
];

const DOCKER_COMPOSE_CAPABILITIES: &[RuntimeCapabilityEvidence] = &[
    evidence(
        RuntimeCapability::OneShotJobs,
        "scheduler",
        "live-jobs-docker-compose",
    ),
    evidence(
        RuntimeCapability::StreamLogs,
        "runtime_observability",
        "live-observability-docker-compose",
    ),
    evidence(
        RuntimeCapability::Exec,
        "runtime_observability",
        "live-observability-docker-compose",
    ),
    evidence(
        RuntimeCapability::RuntimeSecrets,
        "runtime_secret_delivery",
        "live-secret-docker-compose",
    ),
    evidence(
        RuntimeCapability::BindMounts,
        "development_mounts",
        "live-development-mounts-docker",
    ),
    evidence(
        RuntimeCapability::DirectEnvironment,
        "environment_delivery",
        "live-docker-compose",
    ),
    evidence(
        RuntimeCapability::RenderCompose,
        "runtime_conformance",
        "live-docker-compose",
    ),
    evidence(
        RuntimeCapability::ApplyCompose,
        "runtime_conformance",
        "live-docker-compose",
    ),
    evidence(
        RuntimeCapability::ObserveServices,
        "runtime_conformance",
        "live-docker-compose",
    ),
    evidence(
        RuntimeCapability::ContainerHealth,
        "runtime_conformance",
        "live-docker-compose",
    ),
    evidence(
        RuntimeCapability::ManageNetworks,
        "runtime_conformance",
        "live-docker-compose",
    ),
    evidence(
        RuntimeCapability::ManageVolumes,
        "runtime_conformance",
        "live-docker-compose",
    ),
    evidence(
        RuntimeCapability::MutateVolumeFiles,
        "runtime_conformance",
        "live-docker-compose",
    ),
];

const fn evidence(
    capability: RuntimeCapability,
    fixture: &'static str,
    live_job: &'static str,
) -> RuntimeCapabilityEvidence {
    RuntimeCapabilityEvidence {
        capability,
        fixture,
        live_job,
    }
}

#[must_use]
pub const fn runtime_descriptor(kind: RuntimeKind) -> RuntimeDescriptor {
    let capabilities = match kind {
        RuntimeKind::PodmanCompose => PODMAN_CAPABILITIES,
        RuntimeKind::DockerCompose => DOCKER_COMPOSE_CAPABILITIES,
    };
    RuntimeDescriptor { kind, capabilities }
}

impl RuntimeCapability {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RenderCompose => "render_compose",
            Self::ApplyCompose => "apply_compose",
            Self::ObserveServices => "observe_services",
            Self::ContainerHealth => "container_health",
            Self::StreamLogs => "stream_logs",
            Self::Exec => "exec",
            Self::ResolveImageDigest => "resolve_image_digest",
            Self::Rollback => "rollback",
            Self::ManageNetworks => "manage_networks",
            Self::ManageVolumes => "manage_volumes",
            Self::MutateVolumeFiles => "mutate_volume_files",
            Self::RemoteTransport => "remote_transport",
            Self::DirectEnvironment => "direct_environment",
            Self::BindMounts => "bind_mounts",
            Self::RuntimeSecrets => "runtime_secrets",
            Self::OneShotJobs => "one_shot_jobs",
        }
    }

    #[must_use]
    pub const fn behavior(self) -> &'static str {
        match self {
            Self::RenderCompose => {
                "render reviewed Compose files to a canonical service list and content hash"
            }
            Self::ApplyCompose => {
                "apply or remove the exact reviewed Compose files for the selected local project"
            }
            Self::ObserveServices => {
                "return normalized live service/container/task state, not an empty success"
            }
            Self::ContainerHealth => "populate engine health status on observed containers",
            Self::StreamLogs => "stream logs for an owned service without fabricating output",
            Self::Exec => "execute a command in an owned container after capability checks",
            Self::ResolveImageDigest => "resolve an image reference to a content digest",
            Self::Rollback => {
                "restore a reviewed prior artifact/version and record partial rollback"
            }
            Self::ManageNetworks => {
                "create, inspect, and delete networks only after exact ownership labels match"
            }
            Self::ManageVolumes => {
                "create and inspect volumes only after exact ownership labels match"
            }
            Self::MutateVolumeFiles => {
                "atomically create or replace a file in an owned volume after ownership checks"
            }
            Self::RemoteTransport => "execute runtime operations over a non-local transport",
            Self::DirectEnvironment => {
                "interpolate operation-scoped placeholders from a redacted process environment"
            }
            Self::BindMounts => {
                "mount the reviewed local source directory without copying or remote fallback"
            }
            Self::RuntimeSecrets => {
                "deliver versioned runtime secrets through the selected engine-specific driver"
            }
            Self::OneShotJobs => {
                "launch, inspect, cancel, and clean a finite workload with a stable runtime identity"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceEnsureResult {
    Created,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResource {
    pub kind: crate::domain::ownership::ResourceKind,
    pub name: String,
    pub resource: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ComposeProvider {
    pub name: String,
    pub version: Option<String>,
    pub executable: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeProbe {
    pub kind: RuntimeKind,
    pub available: bool,
    pub engine_version: Option<String>,
    pub compose_provider: Option<ComposeProvider>,
    pub rootless: Option<bool>,
    pub socket_path: Option<PathBuf>,
    pub capabilities: Vec<RuntimeCapability>,
    pub diagnostics: Vec<Diagnostic>,
}

impl RuntimeProbe {
    #[must_use]
    pub fn unavailable(kind: RuntimeKind, diagnostic: Diagnostic) -> Self {
        Self {
            kind,
            available: false,
            engine_version: None,
            compose_provider: None,
            rootless: None,
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }

    pub fn require_capabilities(
        &self,
        operation: &str,
        required: &[RuntimeCapability],
        target: Option<&str>,
    ) -> Result<(), MmError> {
        // An engine that did not answer in time lacks no capability.
        if let Some(late) =
            self.late_engine(target.map(|target| format!("target '{target}'")).as_deref())
        {
            return Err(late);
        }
        if let Some(unusable) = self.unusable_engine(target) {
            return Err(unusable);
        }
        let missing = required
            .iter()
            .copied()
            .filter(|capability| !self.capabilities.contains(capability))
            .collect::<Vec<_>>();
        if missing.is_empty() && self.available {
            return Ok(());
        }
        Err(MmError::unsupported_capability(
            self.kind,
            target,
            operation,
            if missing.is_empty() {
                required.to_vec()
            } else {
                missing
            },
            self.engine_version.clone(),
        ))
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct RuntimeEnvironment {
    values: BTreeMap<String, Zeroizing<String>>,
}

impl RuntimeEnvironment {
    #[must_use]
    pub fn from_sensitive_values(values: BTreeMap<String, String>) -> Self {
        Self {
            values: values
                .into_iter()
                .map(|(key, value)| (key, Zeroizing::new(value)))
                .collect(),
        }
    }

    pub fn insert(&mut self, key: String, value: String) -> Option<Zeroizing<String>> {
        self.values.insert(key, Zeroizing::new(value))
    }

    pub fn values(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl std::fmt::Debug for RuntimeEnvironment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeEnvironment")
            .field("keys", &self.values.keys().collect::<Vec<_>>())
            .field("count", &self.values.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTarget {
    pub root: PathBuf,
    pub project_directory: PathBuf,
    pub target: Option<NamedTarget>,
    pub project_id: ProjectId,
    pub environment: String,
    pub stack_name: String,
    pub project_name: String,
    pub artifact_hash: String,
    pub compose_files: Vec<PathBuf>,
    pub user_compose_file_count: usize,
    pub interpolation_environment: RuntimeEnvironment,
    pub installation_scoped: bool,
    /// Credentials the runtime pulls private images with; never logged.
    pub registry_credentials: Vec<RegistryCredential>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderedProject {
    pub stack: String,
    pub project_name: String,
    pub service_names: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub services: BTreeMap<String, RenderedServiceSemantics>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<RenderedResourceSemantics>,
    pub config_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderedServiceSemantics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aliases: BTreeMap<String, Vec<String>>,
    pub healthcheck: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_condition: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placement_constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderedResourceSemantics {
    pub kind: crate::domain::ownership::ResourceKind,
    pub logical_name: String,
    pub physical_name: String,
    pub external: bool,
    pub ownership_labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContainerObservation {
    pub id: String,
    pub name: String,
    pub service: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub health: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub labels: BTreeMap<String, String>,
    /// The cluster node the container is on; absent on a single host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// An earlier container of the service that a newer one replaced, or
    /// one that ran once for a job. It is listed, and it does not count
    /// toward the service's state.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub history: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObservedProject {
    pub stack: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceObservation>,
    pub containers: Vec<ContainerObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceIdentity {
    pub project_id: ProjectId,
    pub environment: String,
    pub stack: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub service: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceObservation {
    pub identity: ServiceIdentity,
    pub desired_count: u32,
    pub running_count: u32,
    pub healthy_count: u32,
    pub state: ServiceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Missing,
    Running,
    Degraded,
    Stopped,
    Unknown,
    JobOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceSnapshot {
    pub stack: String,
    pub name: String,
    pub module: Option<String>,
    /// On a cluster target: what each node says about the service, in the
    /// target's declared order. The counts and the state above are the
    /// service's across these nodes. Empty on a single host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<ServiceOnNode>,
    pub state: ServiceState,
    /// True while the service is `Degraded` only because it is still
    /// coming up: every container it should have runs, none failed its
    /// health check, and at least one check has not passed yet, as during
    /// a rollout step. Set by [`ServiceSnapshot::note_starting`].
    #[serde(default)]
    pub starting: bool,
    pub desired_count: u32,
    pub running_count: u32,
    pub healthy_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub containers: Vec<ContainerObservation>,
    /// For a service with `autoscale` on a cluster target: how many
    /// containers it runs and wants, and the last change with its reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<crate::domain::autoscale::ServiceScale>,
    /// On a cluster target: what mm did about containers of the service
    /// that kept failing their health check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restarts: Option<crate::domain::healing::ServiceRestarts>,
}

/// One node's share of a service that runs on a cluster target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceOnNode {
    pub node: String,
    pub state: ServiceState,
    /// The same as the service's `starting`, about this node's share.
    #[serde(default)]
    pub starting: bool,
    pub desired_count: u32,
    pub running_count: u32,
    pub healthy_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceState {
    Accepted,
    Converging,
    Succeeded,
    Failed,
    RolledBack,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConvergenceOutcome {
    pub state: ConvergenceState,
    pub polls: u32,
    pub observation: ObservedProject,
    /// The service that failed, or the first one that was not healthy when
    /// the time ran out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConvergencePolicy {
    pub timeout: Duration,
    pub initial_backoff: Duration,
    pub maximum_backoff: Duration,
}

impl Default for ConvergencePolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            initial_backoff: Duration::from_millis(250),
            maximum_backoff: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConvergenceCancellation(Arc<AtomicBool>);

impl ConvergenceCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub fn reserved_runtime_environment_names() -> impl Iterator<Item = &'static str> {
    COMMON_INHERITED_ENVIRONMENT
        .iter()
        .chain(PODMAN_INHERITED_ENVIRONMENT)
        .chain(DOCKER_INHERITED_ENVIRONMENT)
        .copied()
}

#[cfg(test)]
mod tests;
