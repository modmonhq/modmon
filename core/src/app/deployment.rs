use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::app::ports::{
    IngressRouteRegistry, LocalCaStore, RuntimeDriver, StateStore, TraefikDynamicConfigStore,
};
use crate::app::{EnvironmentManager, StackManager, TargetIngressManager, WorkspaceSelection};
use crate::domain::config_resolve::route_paths_overlap;
use crate::domain::deployment::{
    DeploymentPlan, DeploymentRouteBehavior, DeploymentRouteChange, DeploymentRouteDiff,
    DeploymentRouteSnapshot, DeploymentStackPlan,
};
use crate::domain::error::MmError;
use crate::domain::ingress::IngressRouteClaim;
use crate::domain::operation::OperationState;
use crate::domain::target::TargetTransport;

#[derive(Clone)]
pub struct DeploymentManager {
    installation: Arc<dyn crate::app::ports::InstallationIngressOwnership>,
    stacks: StackManager,
    routes: Arc<dyn IngressRouteRegistry>,
    state: Arc<dyn StateStore>,
    dynamic_configs: Arc<dyn TraefikDynamicConfigStore>,
    target_ingress: TargetIngressManager,
    environments: EnvironmentManager,
    runtime: Arc<dyn RuntimeDriver>,
    local_ca: Arc<dyn LocalCaStore>,
    prerequisites: Vec<Arc<dyn crate::app::ports::ReadinessProbe>>,
    receipt: Option<crate::domain::build::PromotedReceipt>,
    /// On a cluster node: the addresses the node holds for the cluster's
    /// networks.
    cluster_networks: Option<Arc<dyn crate::app::ports::ClusterNetworkHost>>,
    /// On a cluster node: the node's name resolver.
    names: Option<Arc<dyn crate::app::ports::ServiceNames>>,
    /// On a cluster node: how a container leaves without dropping a
    /// request, and the node's ingress proxy.
    leaving: Option<Arc<dyn crate::app::ports::ContainerLeaving>>,
    ingress: Option<Arc<dyn crate::app::ports::IngressHost>>,
    /// On a cluster node: the count of each scaled service.
    scale: Option<Arc<dyn crate::app::ports::ScaleState>>,
}

pub type DeploymentPlanner = DeploymentManager;

pub struct DeploymentManagerDependencies {
    pub installation: Arc<dyn crate::app::ports::InstallationIngressOwnership>,
    pub stacks: StackManager,
    pub routes: Arc<dyn IngressRouteRegistry>,
    pub state: Arc<dyn StateStore>,
    pub dynamic_configs: Arc<dyn TraefikDynamicConfigStore>,
    pub target_ingress: TargetIngressManager,
    pub environments: EnvironmentManager,
    pub runtime: Arc<dyn RuntimeDriver>,
    pub local_ca: Arc<dyn LocalCaStore>,
}

impl std::fmt::Debug for DeploymentManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeploymentManager")
            .finish_non_exhaustive()
    }
}

mod node_plan;
mod planning;

impl DeploymentManager {
    #[must_use]
    pub fn new(dependencies: DeploymentManagerDependencies) -> Self {
        Self {
            installation: dependencies.installation,
            stacks: dependencies.stacks,
            routes: dependencies.routes,
            state: dependencies.state,
            dynamic_configs: dependencies.dynamic_configs,
            target_ingress: dependencies.target_ingress,
            environments: dependencies.environments,
            runtime: dependencies.runtime,
            local_ca: dependencies.local_ca,
            prerequisites: Vec::new(),
            receipt: None,
            cluster_networks: None,
            names: None,
            leaving: None,
            ingress: None,
            scale: None,
        }
    }

    /// On a cluster node: a container that a rollout replaces leaves
    /// through `leaving`, and the node runs the cluster's ingress proxy
    /// through `ingress` when the plan says so.
    #[must_use]
    pub fn with_node_host(
        mut self,
        leaving: Arc<dyn crate::app::ports::ContainerLeaving>,
        ingress: Arc<dyn crate::app::ports::IngressHost>,
    ) -> Self {
        self.leaving = Some(leaving);
        self.ingress = Some(ingress);
        self
    }

    /// On a cluster node: a deployment runs each scaled service's share of
    /// the count the nodes agreed on, which `scale` knows.
    #[must_use]
    pub fn with_scale_state(mut self, scale: Arc<dyn crate::app::ports::ScaleState>) -> Self {
        self.scale = Some(scale);
        self
    }

    pub(crate) fn scale_state(&self) -> Option<&Arc<dyn crate::app::ports::ScaleState>> {
        self.scale.as_ref()
    }

    pub(crate) fn leaving(&self) -> Option<&Arc<dyn crate::app::ports::ContainerLeaving>> {
        self.leaving.as_ref()
    }

    pub(crate) fn ingress_host(&self) -> Option<&Arc<dyn crate::app::ports::IngressHost>> {
        self.ingress.as_ref()
    }

    /// On a cluster node: the node's name resolver. A rollout waits until
    /// it lists a new container, and has it stop answering with an old one
    /// before that one stops.
    #[must_use]
    pub fn with_names(mut self, names: Arc<dyn crate::app::ports::ServiceNames>) -> Self {
        self.names = Some(names);
        self
    }

    /// Removes the config file versions `plan` no longer mounts.
    pub(crate) fn prune_config_files(&self, plan: &DeploymentPlan) -> Result<Vec<String>, MmError> {
        self.stacks.prune_config_files(plan)
    }

    pub(crate) fn names(&self) -> Option<&Arc<dyn crate::app::ports::ServiceNames>> {
        self.names.as_ref()
    }

    /// On a cluster node: networks declared in `mm.yaml` are created on the
    /// addresses this host stores for them, so they span the nodes.
    #[must_use]
    pub fn with_cluster_networks(
        mut self,
        networks: Arc<dyn crate::app::ports::ClusterNetworkHost>,
    ) -> Self {
        self.cluster_networks = Some(networks);
        self
    }

    pub(crate) fn cluster_networks(
        &self,
    ) -> Option<&Arc<dyn crate::app::ports::ClusterNetworkHost>> {
        self.cluster_networks.as_ref()
    }

    /// Deploys the images of a build made elsewhere, such as CI, instead of
    /// this machine's latest published build. The receipt is bound to the
    /// selected environment (`BuildManager::receipt_for_environment`).
    #[must_use]
    pub fn with_receipt(mut self, receipt: crate::domain::build::PromotedReceipt) -> Self {
        self.receipt = Some(receipt);
        self
    }

    pub(crate) fn receipt(&self) -> Option<&crate::domain::build::PromotedReceipt> {
        self.receipt.as_ref()
    }

    #[must_use]
    pub fn with_prerequisite(
        mut self,
        prerequisite: Arc<dyn crate::app::ports::ReadinessProbe>,
    ) -> Self {
        self.prerequisites.push(prerequisite);
        self
    }

    pub(crate) fn reconcile_local_certificates(
        &self,
        claims: &[IngressRouteClaim],
    ) -> Result<(), MmError> {
        reconcile_local_certificates(self.local_ca.as_ref(), claims)
    }

    pub(crate) fn installation_owns_ingress(
        &self,
        target_id: crate::domain::target::TargetId,
    ) -> Result<bool, MmError> {
        self.installation.owns_ingress(target_id)
    }

    pub(crate) async fn check_prerequisites(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<(), MmError> {
        for prerequisite in &self.prerequisites {
            let diagnostics = prerequisite.inspect(selection).await?;
            let errors = diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.severity == crate::domain::diagnostic::Severity::Error
                })
                .count();
            if errors > 0 {
                return Err(MmError::Validation { errors });
            }
        }
        Ok(())
    }

    pub(crate) fn local_certificate_paths(
        &self,
        host: &str,
    ) -> Result<(std::path::PathBuf, std::path::PathBuf), MmError> {
        local_certificate_paths(self.local_ca.as_ref(), host)
    }
}

fn reconcile_local_certificates(
    local_ca: &dyn LocalCaStore,
    claims: &[IngressRouteClaim],
) -> Result<(), MmError> {
    let status = local_ca.status()?;
    let mut hosts = local_tls_hosts(claims);
    // A host without a local CA (a server) has nothing to
    // reconcile when no route asks for a local certificate.
    if hosts.is_empty() && !status.installed {
        return Ok(());
    }
    hosts.extend(
        status
            .issued_hosts
            .into_iter()
            .filter(|host| is_service_dashboard_host(host)),
    );
    local_ca.reconcile(&hosts)?;
    Ok(())
}

fn local_certificate_paths(
    local_ca: &dyn LocalCaStore,
    host: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), MmError> {
    Ok((local_ca.certificate_path(host)?, local_ca.key_path(host)?))
}

fn is_service_dashboard_host(host: &str) -> bool {
    host == crate::domain::service_stack::DASHBOARD_HOST
        || host == crate::domain::service_stack::MAILPIT_HOST
        || host
            .strip_prefix("mm.")
            .and_then(|value| value.strip_suffix(".test"))
            .is_some_and(|slug| !slug.is_empty() && !slug.contains('.'))
}

fn local_tls_hosts(claims: &[IngressRouteClaim]) -> std::collections::BTreeSet<String> {
    claims
        .iter()
        .filter(|claim| claim.tls && crate::domain::ca::is_local_ca_host(&claim.host))
        .map(|claim| claim.host.clone())
        .collect()
}

fn deployment_stacks(
    project_id: crate::domain::workspace::ProjectId,
    project_name: &str,
    target_id: crate::domain::target::TargetId,
    target_name: &str,
    environment: &str,
    artifacts: Vec<crate::domain::stack::PreparedStackArtifact>,
    mut project_directories: BTreeMap<String, std::path::PathBuf>,
) -> Result<Vec<DeploymentStackPlan>, MmError> {
    let mut stacks = Vec::with_capacity(artifacts.len());
    let mut seen_stacks = std::collections::BTreeSet::new();
    for artifact in artifacts {
        let manifest = &artifact.manifest;
        if manifest.project_id != project_id
            || manifest.project_name != project_name
            || manifest.target_id != target_id
            || manifest.target != target_name
            || manifest.environment != environment
        {
            return Err(MmError::Security {
                message: format!(
                    "prepared stack artifact '{}' does not match the deployment identity",
                    manifest.stack
                ),
            });
        }
        if !seen_stacks.insert(manifest.stack.clone()) {
            return Err(MmError::Security {
                message: "prepared deployment contains a stack more than once".to_owned(),
            });
        }
        let project_directory =
            project_directories
                .remove(&manifest.stack)
                .ok_or_else(|| MmError::Environment {
                    message: format!(
                        "prepared stack artifact '{}' has no Compose project directory",
                        manifest.stack
                    ),
                })?;
        stacks.push(DeploymentStackPlan {
            stack: manifest.stack.clone(),
            compose_project_name: manifest.compose_project_name.clone(),
            artifact_hash: manifest.artifact_hash.clone(),
            project_directory,
            artifact,
        });
    }
    if let Some(stack) = project_directories.keys().next() {
        return Err(MmError::Environment {
            message: format!("stack '{stack}' has no prepared deployment artifact"),
        });
    }
    stacks.sort_by(|left, right| left.stack.cmp(&right.stack));
    Ok(stacks)
}

fn validate_desired_claims(
    target_id: crate::domain::target::TargetId,
    project_id: crate::domain::workspace::ProjectId,
    environment: &str,
    claims: &[IngressRouteClaim],
) -> Result<(), MmError> {
    if claims.iter().any(|claim| {
        claim.target_id != target_id
            || claim.project_id != project_id
            || claim.environment != environment
            || claim.provenance == crate::domain::ingress::IngressRouteProvenance::MmManagedService
            || !matches!(
                &claim.ownership,
                crate::domain::ingress::IngressRouteOwnership::Project {
                    project_id: owner_project_id,
                    environment: owner_environment,
                    stack,
                    service,
                } if *owner_project_id == claim.project_id
                    && owner_environment == &claim.environment
                    && stack == &claim.stack
                    && service == &claim.service
            )
    }) {
        return Err(MmError::Security {
            message: "compiled deployment routes do not have consistent project ownership"
                .to_owned(),
        });
    }
    if let Some(claim) = claims
        .iter()
        .find(|claim| crate::domain::local_ingress::is_reserved_mm_host(&claim.host))
    {
        return Err(MmError::Security {
            message: format!(
                "project route '{}' attempts to claim reserved mm host '{}'",
                claim.route, claim.host
            ),
        });
    }
    Ok(())
}

fn reject_route_conflicts(claims: &[IngressRouteClaim]) -> Result<(), MmError> {
    for (index, left) in claims.iter().enumerate() {
        for right in claims.iter().skip(index + 1) {
            if left.host != right.host {
                break;
            }
            let same_owner =
                left.project_id == right.project_id && left.environment == right.environment;
            let conflict = if same_owner {
                left.path_prefix == right.path_prefix
            } else {
                route_paths_overlap(&left.path_prefix, &right.path_prefix)
            };
            if conflict {
                return Err(MmError::Environment {
                    message: format!(
                        "route {}{} for project '{}' conflicts with route {}{} for project '{}' on target '{}'",
                        left.host,
                        left.path_prefix,
                        left.project_name,
                        right.host,
                        right.path_prefix,
                        right.project_name,
                        left.target
                    ),
                });
            }
        }
    }
    Ok(())
}

fn route_diff(
    previous: &[IngressRouteClaim],
    desired: &[IngressRouteClaim],
) -> Result<DeploymentRouteDiff, MmError> {
    let previous = routes_by_identity(previous)?;
    let desired = routes_by_identity(desired)?;
    let mut diff = DeploymentRouteDiff::default();

    for ((host, path_prefix), after) in &desired {
        match previous.get(&(host.clone(), path_prefix.clone())) {
            None => diff.added.push(route_snapshot(
                host.clone(),
                path_prefix.clone(),
                after.clone(),
            )),
            Some(before) if before != after => diff.changed.push(DeploymentRouteChange {
                host: host.clone(),
                path_prefix: path_prefix.clone(),
                before: (*before).clone(),
                after: after.clone(),
            }),
            Some(_) => {}
        }
    }
    for ((host, path_prefix), before) in &previous {
        if !desired.contains_key(&(host.clone(), path_prefix.clone())) {
            diff.removed.push(route_snapshot(
                host.clone(),
                path_prefix.clone(),
                before.clone(),
            ));
        }
    }
    Ok(diff)
}

fn routes_by_identity(
    claims: &[IngressRouteClaim],
) -> Result<BTreeMap<(String, String), DeploymentRouteBehavior>, MmError> {
    let mut routes = BTreeMap::new();
    for claim in claims {
        let identity = (claim.host.clone(), claim.path_prefix.clone());
        if routes.insert(identity, route_behavior(claim)).is_some() {
            return Err(MmError::State {
                message: "an ingress route owner contains the same host and path more than once"
                    .to_owned(),
            });
        }
    }
    Ok(routes)
}

fn route_behavior(claim: &IngressRouteClaim) -> DeploymentRouteBehavior {
    DeploymentRouteBehavior {
        stack: claim.stack.clone(),
        service: claim.service.clone(),
        port: claim.port,
        tls: claim.tls,
        provenance: claim.provenance,
        certificate_reference: claim.certificate_reference.clone(),
        ownership: claim.ownership.clone(),
        router_name: claim.router_name.clone(),
        backend_alias: claim.backend_alias.clone(),
    }
}

fn route_snapshot(
    host: String,
    path_prefix: String,
    behavior: DeploymentRouteBehavior,
) -> DeploymentRouteSnapshot {
    DeploymentRouteSnapshot {
        host,
        path_prefix,
        behavior,
    }
}

fn owner_middlewares(
    claims: &[IngressRouteClaim],
    definitions: &BTreeMap<String, crate::domain::ingress::CanonicalMiddleware>,
) -> BTreeMap<String, crate::domain::ingress::CanonicalMiddleware> {
    let mut selected = BTreeMap::new();
    for claim in claims {
        for name in &claim.middlewares {
            if let Some(definition) = definitions.get(name) {
                selected.insert(name.clone(), definition.clone());
            }
        }
    }
    selected
}

fn preserve_gate_attachments(
    current_claims: &[IngressRouteClaim],
    current_middlewares: &BTreeMap<String, crate::domain::ingress::CanonicalMiddleware>,
    desired_claims: &mut [IngressRouteClaim],
    desired_middlewares: &mut BTreeMap<String, crate::domain::ingress::CanonicalMiddleware>,
) -> Result<(), MmError> {
    for current in current_claims {
        for name in &current.middlewares {
            let Some(definition) = current_middlewares.get(name) else {
                continue;
            };
            if !is_gate_middleware(name, definition) {
                continue;
            }
            let desired = desired_claims
                .iter_mut()
                .find(|claim| claim.router_name == current.router_name)
                .ok_or_else(|| MmError::Security {
                    message: format!(
                        "router '{}' has an access gate; detach it before removing the route",
                        current.router_name
                    ),
                })?;
            if desired.host != current.host
                || desired.path_prefix != current.path_prefix
                || desired.tls != current.tls
            {
                return Err(MmError::Security {
                    message: format!(
                        "router '{}' has an access gate; detach it before changing its host, path, or TLS setting",
                        current.router_name
                    ),
                });
            }
            if desired_middlewares
                .get(name)
                .is_some_and(|desired_definition| desired_definition != definition)
            {
                return Err(MmError::Security {
                    message: format!(
                        "access-gate middleware '{name}' conflicts with the desired ingress configuration"
                    ),
                });
            }
            desired.middlewares.push(name.clone());
            desired.middlewares.sort();
            desired.middlewares.dedup();
            desired_middlewares.insert(name.clone(), definition.clone());
        }
    }
    Ok(())
}

fn claims_without_gate_attachments(
    claims: &[IngressRouteClaim],
    middlewares: &BTreeMap<String, crate::domain::ingress::CanonicalMiddleware>,
) -> Vec<IngressRouteClaim> {
    let mut claims = claims.to_vec();
    for claim in &mut claims {
        claim.middlewares.retain(|name| {
            !middlewares
                .get(name)
                .is_some_and(|definition| is_gate_middleware(name, definition))
        });
    }
    claims
}

fn is_gate_middleware(
    name: &str,
    definition: &crate::domain::ingress::CanonicalMiddleware,
) -> bool {
    matches!(
        definition,
        crate::domain::ingress::CanonicalMiddleware::ForwardAuth { address, .. }
            if name.starts_with("mm-gate-")
                && address.starts_with("http://mm-control-host:42831/_mm/gate/forward/")
    )
}

fn merge_middlewares(
    current: &BTreeMap<String, crate::domain::ingress::CanonicalMiddleware>,
    complete_claims: &[IngressRouteClaim],
    desired: &BTreeMap<String, crate::domain::ingress::CanonicalMiddleware>,
) -> BTreeMap<String, crate::domain::ingress::CanonicalMiddleware> {
    let mut merged = BTreeMap::new();
    for claim in complete_claims {
        for name in &claim.middlewares {
            if let Some(definition) = desired.get(name).or_else(|| current.get(name)) {
                merged.insert(name.clone(), definition.clone());
            }
        }
    }
    for (name, definition) in desired {
        merged.insert(name.clone(), definition.clone());
    }
    merged
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
        .then_with(|| left.router_name.cmp(&right.router_name))
        .then_with(|| left.backend_alias.cmp(&right.backend_alias))
        .then_with(|| left.artifact_hash.cmp(&right.artifact_hash))
}

#[cfg(test)]
mod tests;
