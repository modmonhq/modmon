use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::environment::EnvironmentReview;
use crate::domain::ingress::{CanonicalMiddleware, IngressRouteClaim};
use crate::domain::stack::PreparedStackArtifact;
use crate::domain::target::TargetId;
use crate::domain::target_ingress::TargetIngressPlan;
use crate::domain::traefik_dynamic::PreparedTraefikDynamicConfig;
use crate::domain::workspace::ProjectId;

#[path = "deployment/node.rs"]
mod node;
pub use node::{DroppedDependency, LeavingService, NodeProjectPlan};

pub const DEPLOYMENT_PLAN_SCHEMA: &str = "mm-local-deployment-plan-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentStackPlan {
    pub stack: String,
    pub compose_project_name: String,
    pub artifact_hash: String,
    pub project_directory: PathBuf,
    pub artifact: PreparedStackArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentRouteBehavior {
    pub stack: String,
    pub service: String,
    pub port: u16,
    pub tls: bool,
    pub provenance: crate::domain::ingress::IngressRouteProvenance,
    pub certificate_reference: crate::domain::ingress::IngressCertificateReference,
    pub ownership: crate::domain::ingress::IngressRouteOwnership,
    pub router_name: String,
    pub backend_alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentRouteSnapshot {
    pub host: String,
    pub path_prefix: String,
    pub behavior: DeploymentRouteBehavior,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentRouteChange {
    pub host: String,
    pub path_prefix: String,
    pub before: DeploymentRouteBehavior,
    pub after: DeploymentRouteBehavior,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentRouteDiff {
    pub added: Vec<DeploymentRouteSnapshot>,
    pub changed: Vec<DeploymentRouteChange>,
    pub removed: Vec<DeploymentRouteSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentPlan {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_services: Vec<crate::domain::stack::ExcludedService>,
    pub schema: String,
    pub project_id: ProjectId,
    pub project_name: String,
    pub target_name: String,
    pub target_id: TargetId,
    pub target_fingerprint: String,
    pub environment: String,
    pub runtime: Option<crate::domain::runtime::RuntimeKind>,
    pub runtime_source: Option<String>,
    pub workload_intent_hash: Option<String>,
    pub networks: Vec<crate::domain::stack::CanonicalNetworkIntent>,
    pub expected_route_generation: u64,
    pub expected_route_config_digest: Option<String>,
    pub expected_previous_plan_hash: Option<String>,
    pub stacks: Vec<DeploymentStackPlan>,
    pub removed_stacks: Vec<DeploymentStackPlan>,
    pub preserved_volumes: Vec<crate::domain::development_mount::PreservedVolumeManifest>,
    pub removed_preserved_volumes: Vec<crate::domain::development_mount::PreservedVolumeManifest>,
    pub previous_owner_claims: Vec<IngressRouteClaim>,
    pub desired_route_claims: Vec<IngressRouteClaim>,
    pub complete_route_claims: Vec<IngressRouteClaim>,
    pub desired_middlewares: BTreeMap<String, CanonicalMiddleware>,
    pub environment_review: EnvironmentReview,
    pub runtime_secret_plan: Option<crate::domain::runtime_secret::RuntimeSecretPlan>,
    pub scheduler_reconciliation: Option<crate::domain::scheduler::ReconciliationPlan>,
    pub environment_bundle_root: Option<PathBuf>,
    pub previous_dynamic_config: PreparedTraefikDynamicConfig,
    pub dynamic_config: PreparedTraefikDynamicConfig,
    pub local_certificates: Vec<crate::domain::ca::IssuedCertificate>,
    pub static_ingress: Option<TargetIngressPlan>,
    pub removed_ingress: Option<TargetIngressPlan>,
    pub route_diff: DeploymentRouteDiff,
    pub ingress: Vec<crate::domain::ingress::IngressServiceStatus>,
    pub route_state_changed: bool,
    /// The build receipt whose images the plan deploys, and the environment
    /// it was built for; absent when the plan uses this machine's builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_receipt: Option<crate::domain::build::BuildReceiptOrigin>,
    /// The local build the deployment runs first, and the modules it
    /// builds again; absent when the images on this machine are current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_build: Option<crate::domain::build::LocalBuildReview>,
    /// Registries the runtime logs in to, and the secrets holding the logins.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registries: BTreeMap<String, crate::domain::config::RegistryConfig>,
    /// On a node of a cluster target: the services placed on this node and
    /// the ones that leave it. Absent on a single host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeProjectPlan>,
    /// How the stacks roll out: how long each may take to become healthy,
    /// their order, and the services' `update:` settings. Empty in a plan
    /// stored before rollouts existed.
    #[serde(default)]
    pub rollout: crate::domain::rollout::RolloutPolicy,
    /// The config files the host's services mount, and which of them the
    /// last finished deployment mounted in another version: those
    /// services' containers are replaced. The stacks' artifact hashes
    /// cover the versions, so the list itself is not hashed again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_files: Vec<crate::domain::config_file::ConfigFileUse>,
    /// The services whose OCI runtime the plan changes against the last
    /// finished deployment: their containers are created again. The stacks'
    /// artifact hashes cover the runtimes, so the list is not hashed again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_changes: Vec<crate::domain::hardening::RuntimeChange>,
    /// The release the plan deploys: what `MM.` and `RELEASE.` keys read.
    /// Absent when nothing is released, such as for `mm dev`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<crate::domain::release_facts::ReleaseFacts>,
    /// Keys the configuration sets that the release that planned does not
    /// read, and ignored. They say what the planner is, so they are not part
    /// of the plan's hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_config_keys: Vec<crate::domain::config::UnknownConfigKey>,
    pub plan_hash: String,
}

impl DeploymentPlan {
    pub fn canonical_hash(&self) -> Result<String, String> {
        self.validate_canonical_inputs()?;
        let mut hasher = blake3::Hasher::new();
        self.hash_identity(&mut hasher);
        self.hash_stack_changes(&mut hasher);
        self.hash_route_intent(&mut hasher)?;
        self.hash_ingress(&mut hasher);
        if !self.excluded_services.is_empty() {
            hash_field(&mut hasher, "excluded-services-v1");
            hash_field(
                &mut hasher,
                &serde_json::to_string(&self.excluded_services)
                    .map_err(|error| error.to_string())?,
            );
        }
        if let Some(receipt) = &self.build_receipt {
            hash_field(&mut hasher, "build-receipt-v1");
            hash_field(&mut hasher, &receipt.receipt_hash);
            hash_field(&mut hasher, &receipt.environment);
            hash_field(&mut hasher, &receipt.commit);
        }
        if let Some(build) = &self.local_build {
            hash_field(&mut hasher, "local-build-v1");
            hash_field(&mut hasher, &build.plan_hash);
        }
        for (host, registry) in &self.registries {
            hash_field(&mut hasher, "registry-v1");
            hash_field(&mut hasher, host);
            hash_field(&mut hasher, &registry.username);
            hash_field(&mut hasher, &registry.password.secret);
        }
        if let Some(node) = &self.node {
            hash_field(&mut hasher, "node-project-v1");
            hash_field(
                &mut hasher,
                &serde_json::to_string(&node.hashed()).map_err(|error| error.to_string())?,
            );
        }
        if let Some(release) = &self.release {
            hash_field(&mut hasher, "release-v1");
            hash_field(&mut hasher, &release.digest());
        }
        if self.rollout != crate::domain::rollout::RolloutPolicy::default() {
            hash_field(&mut hasher, "rollout-v1");
            hash_field(
                &mut hasher,
                &serde_json::to_string(&self.rollout).map_err(|error| error.to_string())?,
            );
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    fn hash_identity(&self, hasher: &mut blake3::Hasher) {
        hash_field(hasher, &self.schema);
        hash_field(hasher, &self.project_id.0.to_string());
        hash_field(hasher, &self.project_name);
        hash_field(hasher, &self.target_name);
        hash_field(hasher, &self.target_id.0.to_string());
        hash_field(hasher, &self.target_fingerprint);
        hash_field(hasher, &self.environment);
        match self.runtime {
            Some(runtime) => {
                hash_field(hasher, "runtime");
                hash_field(
                    hasher,
                    match runtime {
                        crate::domain::runtime::RuntimeKind::PodmanCompose => "podman-compose",
                        crate::domain::runtime::RuntimeKind::DockerCompose => "docker-compose",
                    },
                );
            }
            None => hash_field(hasher, "no-runtime"),
        }
        hash_optional(hasher, self.runtime_source.as_deref());
        hash_optional(hasher, self.workload_intent_hash.as_deref());
        hash_count(hasher, self.networks.len());
        for network in &self.networks {
            hash_field(hasher, &network.logical_name);
            hash_field(hasher, &network.physical_name);
            if network.internal {
                hash_field(hasher, "internal");
            }
            hash_count(hasher, network.members.len());
            for member in &network.members {
                hash_field(hasher, &member.stack);
                hash_field(hasher, &member.service);
            }
        }
        hash_field(hasher, &self.expected_route_generation.to_string());
        hash_optional(hasher, self.expected_route_config_digest.as_deref());
        hash_optional(hasher, self.expected_previous_plan_hash.as_deref());
    }

    fn hash_stack_changes(&self, hasher: &mut blake3::Hasher) {
        let mut stacks = self.stacks.iter().collect::<Vec<_>>();
        stacks.sort_by(|left, right| left.stack.cmp(&right.stack));
        hash_count(hasher, stacks.len());
        for stack in stacks {
            hash_field(hasher, &stack.stack);
            hash_field(hasher, &stack.compose_project_name);
            hash_field(hasher, &stack.artifact_hash);
        }

        let mut removed_stacks = self.removed_stacks.iter().collect::<Vec<_>>();
        removed_stacks.sort_by(|left, right| left.stack.cmp(&right.stack));
        hash_count(hasher, removed_stacks.len());
        for stack in removed_stacks {
            hash_field(hasher, &stack.stack);
            hash_field(hasher, &stack.compose_project_name);
            hash_field(hasher, &stack.artifact_hash);
        }
        hash_count(hasher, self.preserved_volumes.len());
        for volume in &self.preserved_volumes {
            hash_preserved_volume(hasher, volume);
        }
        hash_count(hasher, self.removed_preserved_volumes.len());
        for volume in &self.removed_preserved_volumes {
            hash_preserved_volume(hasher, volume);
        }
    }

    fn hash_route_intent(&self, hasher: &mut blake3::Hasher) -> Result<(), String> {
        let mut claims = self.desired_route_claims.iter().collect::<Vec<_>>();
        claims.sort_by(|left, right| canonical_claim_order(left, right));
        hash_count(hasher, claims.len());
        for claim in claims {
            hash_claim(hasher, claim)?;
        }
        hash_count(hasher, self.desired_middlewares.len());
        for (name, definition) in &self.desired_middlewares {
            hash_field(hasher, name);
            hash_field(hasher, &definition.digest()?);
        }
        hash_field(hasher, &self.environment_review.digest());
        match &self.runtime_secret_plan {
            Some(plan) => hash_field(hasher, &plan.plan_hash),
            None => hash_field(hasher, "no-runtime-secrets"),
        }
        match &self.scheduler_reconciliation {
            Some(plan) => hash_field(hasher, &plan.plan_hash),
            None => hash_field(hasher, "no-scheduler-reconciliation"),
        }
        hash_field(
            hasher,
            if self.route_state_changed {
                "route-state-changed"
            } else {
                "route-state-unchanged"
            },
        );
        hash_field(hasher, &self.previous_dynamic_config.manifest.digest);
        Ok(())
    }

    fn hash_ingress(&self, hasher: &mut blake3::Hasher) {
        match &self.static_ingress {
            Some(ingress) => {
                hash_field(hasher, "static-ingress");
                hash_field(hasher, &ingress.artifact.manifest.artifact_hash);
            }
            None => hash_field(hasher, "no-static-ingress"),
        }
        match &self.removed_ingress {
            Some(ingress) => {
                hash_field(hasher, "removed-static-ingress");
                hash_field(hasher, &ingress.artifact.manifest.artifact_hash);
            }
            None => hash_field(hasher, "no-removed-static-ingress"),
        }
        hash_field(hasher, &self.dynamic_config.manifest.digest);
        hash_count(hasher, self.local_certificates.len());
        for certificate in &self.local_certificates {
            hash_count(hasher, certificate.hosts.len());
            for host in &certificate.hosts {
                hash_field(hasher, host);
            }
            hash_field(hasher, &certificate.fingerprint);
            hash_field(hasher, &certificate.generation.to_string());
            hash_field(hasher, &certificate.expires_at_unix.to_string());
        }
    }

    fn validate_canonical_inputs(&self) -> Result<(), String> {
        if self.schema != DEPLOYMENT_PLAN_SCHEMA {
            return Err(format!(
                "unsupported deployment plan schema '{}'",
                self.schema
            ));
        }
        if let Some(runtime_secrets) = &self.runtime_secret_plan
            && (runtime_secrets.project_id != self.project_id.0.to_string()
                || runtime_secrets.target_id != self.target_id.0.to_string()
                || runtime_secrets.environment != self.environment
                || Some(runtime_secrets.runtime) != self.runtime
                || runtime_secrets.canonical_hash()? != runtime_secrets.plan_hash)
        {
            return Err(
                "deployment runtime secret plan is not canonical or does not match its owner"
                    .to_owned(),
            );
        }
        if let Some(scheduler) = &self.scheduler_reconciliation
            && (scheduler.project_id != self.project_id
                || scheduler.environment != self.environment
                || scheduler.calculate_hash()? != scheduler.plan_hash)
        {
            return Err(
                "deployment scheduler reconciliation is not canonical or does not match its owner"
                    .to_owned(),
            );
        }
        if self.dynamic_config.manifest.target_id != self.target_id {
            return Err("deployment dynamic configuration belongs to another target".to_owned());
        }
        if self.previous_dynamic_config.manifest.target_id != self.target_id {
            return Err(
                "deployment previous dynamic configuration belongs to another target".to_owned(),
            );
        }
        if let Some(expected) = &self.expected_route_config_digest
            && expected != &self.previous_dynamic_config.manifest.digest
        {
            return Err(
                "deployment previous dynamic configuration does not match stored route state"
                    .to_owned(),
            );
        }
        if self.dynamic_config.manifest.claim_count != self.complete_route_claims.len() {
            return Err(
                "deployment dynamic configuration claim count does not match the complete route set"
                    .to_owned(),
            );
        }
        if self
            .complete_route_claims
            .iter()
            .any(|claim| claim.target_id != self.target_id)
        {
            return Err("deployment complete route set contains another target".to_owned());
        }
        if self.desired_route_claims.iter().any(|claim| {
            claim.target_id != self.target_id
                || claim.project_id != self.project_id
                || claim.environment != self.environment
        }) {
            return Err("deployment desired claims do not match the planned owner".to_owned());
        }
        if self.previous_owner_claims.iter().any(|claim| {
            claim.target_id != self.target_id
                || claim.project_id != self.project_id
                || claim.environment != self.environment
        }) {
            return Err("deployment previous claims do not match the planned owner".to_owned());
        }

        self.validate_stack_contracts()?;
        self.validate_ingress_contracts()?;
        Ok(())
    }
    fn validate_stack_contracts(&self) -> Result<(), String> {
        let mut stack_names = std::collections::BTreeSet::new();
        let mut compose_projects = std::collections::BTreeSet::new();
        for stack in &self.stacks {
            let manifest = &stack.artifact.manifest;
            if !stack_names.insert(stack.stack.as_str()) {
                return Err(format!(
                    "deployment contains stack '{}' more than once",
                    stack.stack
                ));
            }
            if !compose_projects.insert(stack.compose_project_name.as_str()) {
                return Err(format!(
                    "deployment contains Compose project '{}' more than once",
                    stack.compose_project_name
                ));
            }
            if stack.stack != manifest.stack
                || stack.compose_project_name != manifest.compose_project_name
                || stack.artifact_hash != manifest.artifact_hash
                || manifest.project_id != self.project_id
                || manifest.project_name != self.project_name
                || manifest.target != self.target_name
                || manifest.target_id != self.target_id
                || manifest.environment != self.environment
            {
                return Err(format!(
                    "deployment stack '{}' does not match its artifact manifest",
                    stack.stack
                ));
            }
            let canonical_artifact_hash = manifest.canonical_hash()?;
            if canonical_artifact_hash != stack.artifact_hash {
                return Err(format!(
                    "deployment stack '{}' has a non-canonical artifact hash",
                    stack.stack
                ));
            }
        }

        let mut removed_names = std::collections::BTreeSet::new();
        let mut removed_projects = std::collections::BTreeSet::new();
        for stack in &self.removed_stacks {
            let manifest = &stack.artifact.manifest;
            if !removed_names.insert(stack.stack.as_str()) {
                return Err(format!(
                    "deployment removes stack '{}' more than once",
                    stack.stack
                ));
            }
            if !removed_projects.insert(stack.compose_project_name.as_str()) {
                return Err(format!(
                    "deployment removes Compose project '{}' more than once",
                    stack.compose_project_name
                ));
            }
            if compose_projects.contains(stack.compose_project_name.as_str()) {
                return Err(format!(
                    "deployment both applies and removes Compose project '{}'",
                    stack.compose_project_name
                ));
            }
            if stack.stack != manifest.stack
                || stack.compose_project_name != manifest.compose_project_name
                || stack.artifact_hash != manifest.artifact_hash
                || manifest.project_id != self.project_id
                || manifest.target_id != self.target_id
                || manifest.environment != self.environment
                || manifest.canonical_hash()? != stack.artifact_hash
            {
                return Err(format!(
                    "removed deployment stack '{}' does not match its artifact manifest",
                    stack.stack
                ));
            }
        }
        if (!self.removed_stacks.is_empty()
            || self.removed_ingress.is_some()
            || !self.removed_preserved_volumes.is_empty())
            && self.expected_previous_plan_hash.is_none()
        {
            return Err("deployment removals require a previous reviewed plan".to_owned());
        }

        Ok(())
    }

    fn validate_ingress_contracts(&self) -> Result<(), String> {
        if let Some(ingress) = &self.static_ingress
            && (ingress.target_id != self.target_id
                || ingress.target_name != self.target_name
                || ingress.plan_hash != ingress.artifact.manifest.artifact_hash
                || ingress.artifact.manifest.target_fingerprint != self.target_fingerprint
                || ingress.artifact.manifest.canonical_hash()
                    != ingress.artifact.manifest.artifact_hash)
        {
            return Err("deployment static ingress plan does not match the target".to_owned());
        }
        if let Some(ingress) = &self.removed_ingress
            && (self.static_ingress.is_some()
                || !self.complete_route_claims.is_empty()
                || ingress.target_id != self.target_id
                || ingress.plan_hash != ingress.artifact.manifest.artifact_hash
                || ingress.artifact.manifest.canonical_hash()
                    != ingress.artifact.manifest.artifact_hash)
        {
            return Err(
                "removed deployment ingress does not match an empty target route set".to_owned(),
            );
        }
        let desired_volume_names = self
            .preserved_volumes
            .iter()
            .map(|volume| volume.physical_name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if desired_volume_names.len() != self.preserved_volumes.len()
            || self
                .removed_preserved_volumes
                .iter()
                .any(|volume| desired_volume_names.contains(volume.physical_name.as_str()))
        {
            return Err(
                "deployment preserved-volume ownership is duplicated or both desired and removed"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentResult {
    pub plan_hash: String,
    pub target_name: String,
    pub target_id: TargetId,
    pub environment: String,
    pub applied_stacks: Vec<String>,
    pub removed_stacks: Vec<String>,
    pub route_generation: u64,
    pub route_count: usize,
    /// On a cluster node: services that moved to other nodes and still run
    /// here. They stop once every node of the plan has applied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved_away: Vec<LeavingService>,
    /// The `after-deploy` jobs it ran and the `once` jobs it started, once
    /// it was in place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<crate::domain::scheduler::JobOutcome>,
}

impl DeploymentResult {
    /// What the terminal UI says once the deployment ran on `target`, the
    /// target's name in the project: a remote host's own result names the
    /// target it runs on itself, `local`.
    #[must_use]
    pub fn started_on(&self, target: &str) -> String {
        let routes = match self.route_count {
            0 => String::new(),
            1 => " (1 route)".to_owned(),
            routes => format!(" ({routes} routes)"),
        };
        if self.applied_stacks.is_empty() {
            format!("Nothing to start on {target}{routes}.")
        } else {
            format!(
                "Started {} on {target}{routes}.",
                self.applied_stacks.join(", ")
            )
        }
    }
}

fn canonical_claim_order(left: &IngressRouteClaim, right: &IngressRouteClaim) -> Ordering {
    left.host
        .cmp(&right.host)
        .then_with(|| left.path_prefix.cmp(&right.path_prefix))
        .then_with(|| left.project_id.0.cmp(&right.project_id.0))
        .then_with(|| left.environment.cmp(&right.environment))
        .then_with(|| left.stack.cmp(&right.stack))
        .then_with(|| left.route.cmp(&right.route))
        .then_with(|| left.service.cmp(&right.service))
        .then_with(|| left.port.cmp(&right.port))
        .then_with(|| left.tls.cmp(&right.tls))
        .then_with(|| left.priority.cmp(&right.priority))
        .then_with(|| left.router_name.cmp(&right.router_name))
        .then_with(|| left.backend_alias.cmp(&right.backend_alias))
        .then_with(|| left.artifact_hash.cmp(&right.artifact_hash))
}

fn hash_claim(hasher: &mut blake3::Hasher, claim: &IngressRouteClaim) -> Result<(), String> {
    hash_field(hasher, &claim.target);
    hash_field(hasher, &claim.target_id.0.to_string());
    hash_field(hasher, &claim.project_id.0.to_string());
    hash_field(hasher, &claim.project_name);
    hash_field(hasher, &claim.environment);
    hash_field(hasher, &claim.stack);
    hash_field(hasher, &claim.route);
    hash_field(hasher, &claim.host);
    hash_field(hasher, &claim.path_prefix);
    hash_field(hasher, &claim.service);
    hash_field(hasher, &claim.port.to_string());
    hash_field(hasher, if claim.tls { "tls" } else { "plain" });
    hash_field(
        hasher,
        match claim.provenance {
            crate::domain::ingress::IngressRouteProvenance::ExplicitConfig => "explicit-config",
            crate::domain::ingress::IngressRouteProvenance::AutomaticConvention => {
                "automatic-convention"
            }
            crate::domain::ingress::IngressRouteProvenance::MmManagedService => {
                "mm-managed-service"
            }
        },
    );
    let certificate = serde_json::to_string(&claim.certificate_reference)
        .map_err(|error| format!("could not canonicalize route certificate: {error}"))?;
    hash_field(hasher, &certificate);
    let ownership = serde_json::to_string(&claim.ownership)
        .map_err(|error| format!("could not canonicalize route ownership: {error}"))?;
    hash_field(hasher, &ownership);
    hash_field(hasher, &claim.priority.to_string());
    hash_field(hasher, &claim.router_name);
    hash_field(hasher, &claim.backend_alias);
    hash_field(hasher, &claim.artifact_hash);
    hash_count(hasher, claim.middlewares.len());
    for middleware in &claim.middlewares {
        hash_field(hasher, middleware);
    }
    Ok(())
}

fn hash_optional(hasher: &mut blake3::Hasher, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            hash_field(hasher, value);
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hash_count(hasher: &mut blake3::Hasher, value: usize) {
    hasher.update(&(value as u64).to_le_bytes());
}

fn hash_preserved_volume(
    hasher: &mut blake3::Hasher,
    volume: &crate::domain::development_mount::PreservedVolumeManifest,
) {
    hash_field(hasher, &volume.module);
    hash_field(hasher, &volume.container_path);
    hash_field(hasher, &volume.logical_name);
    hash_field(hasher, &volume.physical_name);
    hash_field(hasher, &volume.resource);
}
