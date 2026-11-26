use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::domain::build::BuildTagStrategy;
use crate::domain::development_mail::DevelopmentMailPolicy;
use crate::domain::development_mount::{DevelopmentMountConfig, DevelopmentMountPolicy};
use crate::domain::environment_delivery::{EnvironmentDeliveryMode, EnvironmentDeliveryPolicy};
use crate::domain::runtime::RuntimeKind;
use crate::domain::secret::SecretReference;
use crate::domain::secret_backend::{ProviderInstanceConfig, ProviderInstanceName};
use crate::domain::secret_contract::StorageProviderId;
use crate::domain::workspace::ProjectId;

#[path = "config/document.rs"]
mod document;
pub use document::{ConfigDocument, SPLITTABLE_CONFIG_KEYS};
#[path = "config/registry.rs"]
mod registry;
pub use registry::{RegistryConfig, RegistryPassword, registry_token_hint, validate_registry_host};
#[path = "config/build_target.rs"]
mod build_target;
pub use build_target::{BuildTarget, BuildTargetSelection, BuildVariantConfig};
#[path = "config/activity.rs"]
mod activity;
#[path = "config/binding.rs"]
mod binding;
#[path = "config/scalar.rs"]
mod scalar;
pub use binding::{
    CastBinding, EnvironmentBinding, ReferenceBinding, SecretBinding, ValueBinding, ValueCast,
};
#[path = "config/generator.rs"]
mod generator;
#[path = "config/unknown_keys.rs"]
pub mod unknown_keys;
#[path = "config/word_or_mapping.rs"]
pub(crate) mod word_or_mapping;
pub use unknown_keys::UnknownConfigKey;
#[path = "config/secret_usage.rs"]
mod secret_usage;
pub use scalar::EnvScalar;
#[path = "config/network.rs"]
mod network;
#[path = "config/removed.rs"]
pub mod removed;
pub use network::NetworkConfig;
#[path = "config/service.rs"]
mod service;
pub use removed::Removed;
pub use service::{ServiceConfigFileConfig, ServiceSecretFileConfig};
#[path = "config/stack.rs"]
mod stack;
pub use stack::StackConfig;
#[path = "config/jobs.rs"]
mod jobs;
pub use jobs::{
    DurationSpec, JobConfig, JobEnvironments, JobWhen, MisfirePolicy, RetryPolicyConfig,
    ScheduleConcurrency, ScheduleConfig,
};
#[path = "config/tls.rs"]
mod tls;
pub use tls::EnvironmentTlsConfig;
#[path = "config/ingress.rs"]
mod ingress;
pub use ingress::{CloudflaredConfig, EnvironmentIngressConfig, IngressType};
#[path = "config/update.rs"]
mod update;
pub use update::{
    DEFAULT_CONVERGENCE_TIMEOUT, DEFAULT_PROGRESS_TIMEOUT, DEFAULT_PULL_TIMEOUT, DeployConfig,
    UpdateConfig, UpdateFailure, UpdateOrder,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct MmConfig {
    #[serde(deserialize_with = "deserialize_config_schema_version")]
    #[schemars(range(min = 1, max = 1))]
    pub schema_version: u32,
    pub project: ProjectConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_environment: Option<String>,
    pub target: String,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default, skip_serializing_if = "BuildConfig::is_default")]
    pub build: BuildConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub networks: BTreeMap<String, NetworkConfig>,
    pub stacks: BTreeMap<String, StackConfig>,
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<ProviderInstanceName, ProviderInstanceConfig>,
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretDeclaration>,
    #[serde(default)]
    pub bindings: BTreeMap<String, EnvironmentBinding>,
    #[serde(
        default,
        skip_serializing_if = "EnvironmentDeliveryPolicy::is_unspecified"
    )]
    pub environment_delivery: EnvironmentDeliveryPolicy,
    pub environments: BTreeMap<String, EnvironmentConfig>,
    #[serde(default, skip_serializing_if = "ProjectIngressConfig::is_empty")]
    pub ingress: ProjectIngressConfig,
    #[serde(default, skip_serializing_if = "DevelopmentMountPolicy::is_empty")]
    pub development_mounts: DevelopmentMountPolicy,
    #[serde(default, skip_serializing_if = "DevelopmentMailPolicy::is_empty")]
    pub development_mail: DevelopmentMailPolicy,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub jobs: BTreeMap<String, JobConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub schedules: BTreeMap<String, ScheduleConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gates: BTreeMap<String, GateConfig>,
    /// Who may reach each remote environment, reconciled on deploy.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub users: BTreeMap<String, crate::domain::users::UserConfig>,
    /// What an AI agent may do through `mm mcp serve`, per environment.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agents: crate::domain::agents::AgentsConfig,
    /// Public connection facts for remote targets; clients pin them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub targets: BTreeMap<String, crate::domain::repo_target::RepoTargetConfig>,
    /// Image registries and the credentials deploys pull with, by host.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registries: BTreeMap<String, RegistryConfig>,
    /// `strict` hardens every service of a deployment: read-only root
    /// filesystem, no capabilities, no new privileges, the engine's default
    /// seccomp profile, a tmpfs at `/tmp`. Not applied by `mm dev`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardening: Option<crate::domain::hardening::Hardening>,
    /// Values `mm deploy plan --release-input NAME=VALUE` supplies per
    /// deployment, read as `RELEASE.<NAME>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub release_inputs: BTreeMap<String, crate::domain::release_facts::ReleaseInputConfig>,
    /// Never read from a file. `mmd` sets it to what its host policy says
    /// about OCI runtimes.
    #[serde(skip)]
    #[schemars(skip)]
    pub host_runtimes: Option<crate::domain::hardening::HostRuntimes>,
    /// Never read from a file: the release a deployment plans, which `MM.`
    /// and `RELEASE.` keys read.
    #[serde(skip)]
    #[schemars(skip)]
    pub release: Option<crate::domain::release_facts::ReleaseFacts>,
    /// Never read from `mm.yaml`: the optional units this machine turns on.
    #[serde(skip)]
    #[schemars(skip)]
    pub development: crate::domain::development_units::DevelopmentChoices,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct GateConfig {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ingress: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", default)]
#[schemars(deny_unknown_fields)]
pub struct BuildConfig {
    pub platforms: Vec<String>,
    pub tag_strategy: BuildTagStrategy,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            platforms: vec!["linux/amd64".to_owned(), "linux/arm64".to_owned()],
            tag_strategy: BuildTagStrategy::Auto,
        }
    }
}

impl BuildConfig {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ProjectConfig {
    pub id: ProjectId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_environment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<RuntimeKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum IngressConfig {
    Route(IngressRouteConfig),
    Routes(Vec<IngressRouteConfig>),
}

impl IngressConfig {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Routes(routes) if routes.is_empty())
    }

    #[must_use]
    pub fn routes(&self) -> Vec<&IngressRouteConfig> {
        match self {
            Self::Route(route) => vec![route],
            Self::Routes(routes) => routes.iter().collect(),
        }
    }

    pub fn routes_mut(&mut self) -> Vec<&mut IngressRouteConfig> {
        match self {
            Self::Route(route) => vec![route],
            Self::Routes(routes) => routes.iter_mut().collect(),
        }
    }
}

impl Default for IngressConfig {
    fn default() -> Self {
        Self::Routes(Vec::new())
    }
}

impl<'de> Deserialize<'de> for IngressConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(IngressConfigVisitor)
    }
}

struct IngressConfigVisitor;

impl<'de> Visitor<'de> for IngressConfigVisitor {
    type Value = IngressConfig;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("one detailed ingress mapping or a list of mappings")
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        IngressRouteConfig::deserialize(MapAccessDeserializer::new(map)).map(IngressConfig::Route)
    }

    fn visit_seq<A>(self, sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        Vec::<IngressRouteConfig>::deserialize(SeqAccessDeserializer::new(sequence))
            .map(IngressConfig::Routes)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ProjectIngressConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_local: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub middlewares: BTreeMap<String, MiddlewareBundleConfig>,
}

impl ProjectIngressConfig {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.automatic_local.is_none() && self.middlewares.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct MiddlewareBundleConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct IngressRouteConfig {
    pub domain: String,
    #[serde(default = "default_route_path")]
    pub route: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default = "default_true")]
    pub https: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    /// On a cluster target: `spread` sends this route's requests to the
    /// service's containers on every node. The default is the
    /// environment's `ingress.balance`, else `spread` for a service with
    /// `autoscale` and `local-first` for every other one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<crate::domain::cluster_ingress::Balance>,
    /// On a cluster target: where this route stands among the routes of
    /// its host, from 1 to 1000000. By default the longer path goes first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    /// On a cluster target: response headers, on top of the environment's
    /// `ingress.headers`. An empty value removes the header.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// On a cluster target: the only requests that reach the service; the
    /// proxy answers 404 to every other one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<crate::domain::cluster_ingress::AllowRule>,
    /// Never read from a file: the development environment, when the route
    /// goes to a service `development:` keeps off on this machine.
    #[serde(skip)]
    #[schemars(skip)]
    pub off_locally: Option<String>,
}

fn default_route_path() -> String {
    "/".to_owned()
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ModuleConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<ModuleEnvConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<ModuleBuildConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_mount: Option<DevelopmentMountConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_mail: Option<bool>,
    /// `on` (the default), `optional` (off in the development environment
    /// until a developer turns it on with `mm dev enable`), or `off`. Its
    /// services take it unless they set their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<crate::domain::development_units::Development>,
}

impl DevelopmentMountPolicy {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sensitive_patterns.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", default)]
#[schemars(deny_unknown_fields)]
pub struct ModuleBuildConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dockerfile: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_variant: Option<String>,
    pub targets: BTreeMap<String, BuildTarget>,
    pub args: BTreeMap<String, EnvScalar>,
    pub secrets: BTreeMap<String, String>,
    /// Keys of `secrets` that only authenticate fetching inputs, such as a
    /// package registry token. They are still mounted, but they do not change
    /// the image, so they never block reuse or promotion of a build.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_only_secrets: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub depends_on: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_inputs: Vec<PathBuf>,
}

impl Default for ModuleBuildConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            context: None,
            dockerfile: None,
            image: None,
            default_variant: None,
            targets: BTreeMap::new(),
            args: BTreeMap::new(),
            secrets: BTreeMap::new(),
            credential_only_secrets: Vec::new(),
            depends_on: BTreeMap::new(),
            extra_inputs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ModuleEnvConfig {
    pub template: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ServiceConfig {
    pub stack: String,
    /// Environments in which this service runs; an empty list enables all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ServiceRole>,
    /// `on`, `optional` or `off` in the development environment; without
    /// it, the module's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<crate::domain::development_units::Development>,
    /// A persisted HTTP selection made explicitly or by the guided editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_local_ingress: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_https: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_mail: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvScalar>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secret_files: BTreeMap<String, ServiceSecretFileConfig>,
    /// Files the service ships with, by their path in the container.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config_files: BTreeMap<String, ServiceConfigFileConfig>,
    /// Which nodes of a cluster target run this service, one container on
    /// each: a node name, a group name, `all` for every node, or a list of
    /// those. Without it the service runs on the target's first node. The
    /// names must exist on the environment's target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<crate::domain::placement::PlacementSpec>,
    /// On a cluster target: the containers the service runs over its nodes
    /// when nothing scales it; default one per node, and `autoscale`'s `min`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    /// On a cluster target: follow the load between `scale.min` and
    /// `scale.max`, the one required key (`domain::autoscale`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoscale: Option<crate::domain::autoscale::AutoscaleConfig>,
    /// How a deployment replaces this service's containers. Every key has
    /// a default: stop the old container first, one node at a time, and
    /// roll the deployment back when the new one fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<UpdateConfig>,
    /// Lets a service with role `database` or `queue` mount a shared
    /// volume, which mm otherwise refuses: a shared volume is a network
    /// mount, slower than local disk and without the locking guarantees
    /// such a service needs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_shared_volumes: Option<bool>,
    /// The defaults of `hardening: strict` this service leaves out, each
    /// with a reason: `hardening: {except: {read-only: "writes its cache"}}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardening: Option<crate::domain::hardening::ServiceHardening>,
    /// On a remote host that runs Podman, keeps this service in the host's
    /// user namespace instead of one of its own (`userns: auto`). The value
    /// says why; a service that mounts a shared volume needs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_user_namespace: Option<String>,
    /// Lets the service run in its host's network namespace
    /// (`network_mode: host` in its Compose file) on this machine. The
    /// value says why, and the plan shows it. On a remote host the host's
    /// policy decides instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_network: Option<String>,
    /// On a cluster target: how long a container of this service that
    /// leaves (a rolling update replaces it, it moves, or it is scaled
    /// away) keeps serving what is in flight once no node sends it new
    /// requests, such as `30s`. Then it gets its stop signal and its
    /// `stop_grace_period`. The default is `1m`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drain: Option<DurationSpec>,
    /// On a cluster target: what the service's node does with a container
    /// that keeps failing its health check. Without it, the node restarts
    /// one that stays unhealthy for 3m, at most 5 times in 1h.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<crate::domain::healing::HealthConfig>,
    /// Never read from a file. `mmd` on a cluster node sets it to the nodes
    /// that run this service when this node does not (see `service.rs`).
    #[serde(skip)]
    #[schemars(skip)]
    pub placed_elsewhere: Option<Vec<String>>,
    /// Never read from a file. `mmd` on a cluster node sets it on the
    /// services the node runs: one without a `restart:` line gets a default.
    #[serde(skip)]
    #[schemars(skip)]
    pub restarts_with_node: bool,
    /// Never read from a file: on a cluster node, the placement's nodes.
    #[serde(skip)]
    #[schemars(skip)]
    pub placed_on: Option<Vec<String>>,
    /// Never read from a file: set when the service does not run in the
    /// development environment on this machine.
    #[serde(skip)]
    #[schemars(skip)]
    pub off_locally: Option<crate::domain::development_units::OffLocally>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceRole {
    Http,
    Database,
    Queue,
    Worker,
    Scheduler,
    Job,
}

impl ServiceRole {
    #[must_use]
    pub const fn excludes_automatic_ingress(self) -> bool {
        matches!(
            self,
            Self::Database | Self::Queue | Self::Worker | Self::Scheduler | Self::Job
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct EnvironmentConfig {
    #[serde(default)]
    pub domains: BTreeMap<String, String>,
    #[serde(default)]
    pub bindings: BTreeMap<String, EnvironmentBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_delivery: Option<EnvironmentDeliveryMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_local_ingress: Option<bool>,
    /// Branches a commit must be reachable from to be deployed here: names, or
    /// patterns where `*` matches within one `/` segment (`*.x`); `*` alone is
    /// any branch. A commit that changes this list must pass the current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_from: Option<Vec<String>>,
    /// How deployments to this environment behave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<DeployConfig>,
    /// A certificate the environment provides for its domains, in place
    /// of one the ingress requests with ACME.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<EnvironmentTlsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<EnvironmentIngressConfig>,
    /// Whether the services' Compose health checks run. Default `true`,
    /// except `false` for the development environment on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthchecks: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct SecretDeclaration {
    pub storage: SecretStorageConfig,
    /// Present for generated secrets created by `mm secret generate`. Omit for operator-supplied values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate: Option<SecretGeneratorConfig>,
    /// Hidden-input prompt for operator-supplied secrets set with `mm secret set` or `mm secret rotate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<SecretInputConfig>,
    #[serde(default)]
    pub rotation: SecretRotationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct SecretStorageConfig {
    pub provider: StorageProviderId,
    pub location: String,
}

impl SecretStorageConfig {
    pub fn reference(&self) -> Result<SecretReference, String> {
        SecretReference::new(self.provider.clone(), self.location.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "preset", rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub enum SecretGeneratorConfig {
    Password {
        #[serde(default = "default_password_length")]
        length: usize,
    },
    Token {
        #[serde(default = "default_token_length")]
        length: usize,
    },
    Bytes {
        #[serde(default = "default_byte_length")]
        length: usize,
        #[serde(default)]
        encoding: SecretEncoding,
    },
    LaravelAppKey,
    Uuid,
}

impl SecretGeneratorConfig {
    #[must_use]
    pub const fn length(&self) -> Option<usize> {
        match self {
            Self::Password { length } | Self::Token { length } | Self::Bytes { length, .. } => {
                Some(*length)
            }
            Self::LaravelAppKey | Self::Uuid => None,
        }
    }
}

const fn default_password_length() -> usize {
    32
}

const fn default_token_length() -> usize {
    48
}

const fn default_byte_length() -> usize {
    32
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SecretEncoding {
    Base64,
    #[default]
    Base64Url,
    Hex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct SecretInputConfig {
    pub prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct SecretRotationConfig {
    #[serde(default)]
    pub strategy: SecretRotationStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_days: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SecretRotationStrategy {
    #[default]
    Manual,
    ReplaceAndRedeploy,
    Coordinated,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct SecretRef(pub SecretReference);

impl std::fmt::Debug for SecretRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretRef([REDACTED])")
    }
}

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

fn deserialize_config_schema_version<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let version = u32::deserialize(deserializer)?;
    if version == CONFIG_SCHEMA_VERSION {
        Ok(version)
    } else {
        Err(serde::de::Error::custom(format!(
            "unsupported schema-version {version}; expected schema-version: {CONFIG_SCHEMA_VERSION}"
        )))
    }
}

pub fn resolve_environment(
    document: &ConfigDocument,
    requested: Option<&str>,
) -> Result<String, crate::domain::error::MmError> {
    if let Some(requested) = requested {
        return Ok(requested.to_owned());
    }
    if let Some(default_environment) = &document.config.default_environment {
        return Ok(default_environment.clone());
    }
    Err(crate::domain::error::MmError::EnvironmentRequired)
}
