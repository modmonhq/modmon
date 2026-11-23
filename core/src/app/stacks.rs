use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::app::WorkspaceSelection;
use crate::app::config_middleware::{LoadedProjectMiddleware, load_active_middlewares};
use crate::app::config_validation::{
    compose_project_directory, compose_project_name, ingress_route_name,
    target_scoped_compose_project_name, validate,
};
use crate::app::ports::{
    IngressRouteRegistry, ProjectDocumentStore, StackArtifactStore, StateStore, TargetStore,
};
use crate::domain::build::{ImageBuildPlan, ImageBuildResult};
use crate::domain::config::{ConfigDocument, resolve_environment};
use crate::domain::config::{ServiceConfig, StackConfig};
use crate::domain::config_resolve::{effective_domains, route_is_active, route_priority};
use crate::domain::diagnostic::Severity;
use crate::domain::error::MmError;
use crate::domain::ingress::{
    CanonicalMiddleware, HttpEvidenceSource, HttpPortCandidate, IngressCertificateReference,
    IngressRouteClaim, IngressRouteOwnership, IngressRouteProvenance, IngressServiceStatus,
    ServiceHttpEvidence, TransportProtocol,
};
use crate::domain::local_ingress::{ProjectSlug, ServiceSlug};
use crate::domain::path_policy::{ensure_no_symlink_components, read_regular_file};
use crate::domain::runtime::RuntimeTarget;
use crate::domain::stack::{
    CompiledComposeSource, CompiledStackArtifact, ComposeSourceManifest, PreparedStackArtifact,
    StackArtifactManifest, StackCompilationReport, StackRouteManifest, StackServiceManifest,
};
use crate::domain::target::NamedTarget;
use crate::domain::target_ingress::{
    TRAEFIK_CERTIFICATE_RESOLVER, ingress_loopback_ports, ingress_network_name,
};
use crate::domain::workspace::ProjectId;

const MAX_COMPOSE_FILE_BYTES: usize = 16 * 1024 * 1024;

/// Compile in memory for diagnostics, without publishing stack artifacts.
pub(crate) fn inspect_document(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    state: Option<&dyn StateStore>,
) -> Result<Vec<IngressServiceStatus>, MmError> {
    let images = inspect_deployment_images(document, environment, state)?;
    let middlewares = load_active_middlewares(document, environment)?;
    let networks = active_network_membership(document, environment)?;
    let compiled = compile_document(
        document,
        environment,
        target,
        &images,
        &middlewares,
        &networks,
    )?;
    ingress_statuses_from_manifests(
        document,
        environment,
        target,
        &compiled
            .iter()
            .map(|stack| &stack.manifest)
            .collect::<Vec<_>>(),
    )
}

mod deploy_policy;
mod network_overlay;

mod compilation;
pub mod guardrails;
pub(crate) use compilation::resolve_deployment_images as inspect_deployment_images;
use compilation::*;
mod images;
mod service_intent;
use service_intent::*;

pub(crate) fn needs_development_services(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
) -> bool {
    document.config.modules.iter().any(|(module_name, module)| {
        module.services.values().any(|service| {
            !development_mail_bindings(document, environment, target, module_name, service)
                .is_empty()
        })
    })
}
mod routes;
use routes::*;
mod ports;
use ports::*;
mod source_policy;
use source_policy::*;
mod compose_sources;
pub(crate) use compose_sources::compose_source_diagnostics;
use compose_sources::{SourcePolicy, SourceScope};
mod file_references;
mod host_network;
mod service_filter;
use service_filter::*;
mod placement;
pub(crate) use placement::{PlacementNotes, ScaleNote};
use placement::{
    StackPlacement, ingress_proxy_member, prune_unused_resources, served_by_cluster,
    services_without_restart,
};
mod manifest;
use manifest::*;
mod shared_volumes;
pub(crate) use shared_volumes::volume_users;
pub(crate) use user_namespace::{chosen_user_namespaces, files_in_chosen_namespaces};
mod compose_environment;
mod hardening;
pub(crate) use compose_environment::compose_environment_refusals;
mod config_files;
pub(crate) use config_files::{config_file_entries, resolve_config_file};
mod low_ports;
mod user_namespace;

mod network_intent;
use network_intent::canonical_networks;
use network_overlay::{
    GeneratedOverlayInput, canonical_workload_intent_hash, generated_overlay,
    resolve_network_membership,
};

#[derive(Clone)]
pub struct StackManager {
    documents: Arc<dyn ProjectDocumentStore>,
    artifacts: Arc<dyn StackArtifactStore>,
    routes: Option<Arc<dyn IngressRouteRegistry>>,
    targets: Arc<dyn TargetStore>,
    state: Option<Arc<dyn StateStore>>,
    /// Set on a host that runs rootful Podman: each service's own user ids.
    user_ids: Option<Arc<dyn crate::app::ports::ServiceUserIds>>,
}

pub(crate) struct DeploymentStackCompilation {
    pub project_id: ProjectId,
    pub project_name: String,
    pub environment: String,
    pub target: NamedTarget,
    pub selection: crate::app::ResolvedSelection,
    pub stacks: Vec<PreparedStackArtifact>,
    pub route_claims: Vec<IngressRouteClaim>,
    pub middlewares: BTreeMap<String, CanonicalMiddleware>,
    pub project_directories: BTreeMap<String, PathBuf>,
    pub networks: Vec<crate::domain::stack::CanonicalNetworkIntent>,
    pub workload_intent_hash: String,
    pub ingress: Vec<IngressServiceStatus>,
    pub excluded_services: Vec<crate::domain::stack::ExcludedService>,
    /// On a cluster node: what placing services on other nodes left out.
    pub placement: PlacementNotes,
    /// How the stacks roll out, as the configuration says.
    pub rollout: crate::domain::rollout::RolloutPolicy,
    /// The release the deployment runs, with the images resolved here.
    pub release: Option<crate::domain::release_facts::ReleaseFacts>,
    /// Keys the configuration and its middleware files set that this
    /// release ignores.
    pub unknown_keys: Vec<crate::domain::config::UnknownConfigKey>,
}

struct StackPreparation<'a> {
    document: &'a ConfigDocument,
    environment: &'a str,
    target: &'a NamedTarget,
    check_route_registry: bool,
    images: &'a BTreeMap<String, String>,
    middlewares: &'a LoadedProjectMiddleware,
    networks: &'a BTreeMap<(String, String), BTreeSet<String>>,
}

impl std::fmt::Debug for StackManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StackManager")
            .finish_non_exhaustive()
    }
}

impl StackManager {
    #[must_use]
    pub fn new(
        documents: Arc<dyn ProjectDocumentStore>,
        artifacts: Arc<dyn StackArtifactStore>,
        targets: Arc<dyn TargetStore>,
    ) -> Self {
        Self {
            documents,
            artifacts,
            routes: None,
            targets,
            state: None,
            user_ids: None,
        }
    }

    #[must_use]
    pub fn with_route_registry(
        documents: Arc<dyn ProjectDocumentStore>,
        artifacts: Arc<dyn StackArtifactStore>,
        routes: Arc<dyn IngressRouteRegistry>,
        targets: Arc<dyn TargetStore>,
    ) -> Self {
        Self {
            documents,
            artifacts,
            routes: Some(routes),
            targets,
            state: None,
            user_ids: None,
        }
    }

    #[must_use]
    pub fn with_state(mut self, state: Arc<dyn StateStore>) -> Self {
        self.state = Some(state);
        self
    }

    /// Gives each service of a stack the user ids `user_ids` keeps for it,
    /// as rootful Podman's hosts do.
    #[must_use]
    pub fn with_user_ids(
        mut self,
        user_ids: Option<Arc<dyn crate::app::ports::ServiceUserIds>>,
    ) -> Self {
        self.user_ids = user_ids;
        self
    }

    pub fn compile(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<StackCompilationReport, MmError> {
        let compiled =
            self.compile_selection(selection, true, self.state.as_deref(), ImageSource::Builds)?;
        Ok(StackCompilationReport {
            project_name: compiled.project_name,
            target: compiled.target.name,
            target_id: compiled.target.definition.id,
            environment: compiled.environment,
            stacks: compiled.stacks,
            route_claims: compiled.route_claims,
            ingress: compiled.ingress,
            excluded_services: compiled.excluded_services,
        })
    }

    /// `receipt`, when set, supplies the images instead of `state`'s builds.
    pub(crate) fn compile_for_deployment(
        &self,
        selection: &WorkspaceSelection,
        state: &dyn StateStore,
        receipt: Option<&crate::domain::build::BuildReceipt>,
    ) -> Result<DeploymentStackCompilation, MmError> {
        let images = receipt.map_or(ImageSource::Builds, ImageSource::Receipt);
        self.compile_selection(selection, false, Some(state), images)
    }

    /// Compiles with the images a reviewed local build makes.
    pub(crate) fn compile_for_local_build(
        &self,
        selection: &WorkspaceSelection,
        state: &dyn StateStore,
        plan: &ImageBuildPlan,
    ) -> Result<DeploymentStackCompilation, MmError> {
        self.compile_selection(selection, false, Some(state), ImageSource::LocalBuild(plan))
    }

    fn compile_selection(
        &self,
        selection: &WorkspaceSelection,
        check_route_registry: bool,
        state: Option<&dyn StateStore>,
        images: ImageSource<'_>,
    ) -> Result<DeploymentStackCompilation, MmError> {
        let mut document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let (environment_pref, project_pref) =
            crate::app::load_selection_preferences(state, &document, &environment)?;
        let (resolved, target) = crate::app::resolve_selection(
            &document,
            &environment,
            selection.runtime,
            selection.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            self.targets.as_ref(),
        )?;
        let validation = validate(&document, &environment);
        let errors = validation
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count();
        if errors > 0 {
            return Err(MmError::invalid(&validation.diagnostics));
        }
        let images = match (images, state) {
            (ImageSource::Receipt(receipt), _) => {
                deployment_images_from_receipt(&document, &environment, receipt)?
            }
            (ImageSource::LocalBuild(plan), _) => {
                deployment_images_from_local_plan(&document, &environment, plan)?
            }
            (ImageSource::Builds, Some(state)) => resolve_deployment_images_from(
                &document,
                &environment,
                Some(state),
                deployable_builds(selection, &target, resolved.runtime),
            )?,
            (ImageSource::Builds, None) => BTreeMap::new(),
        };
        document.config.release = planned_release(selection, &document, &images)?;
        let network_membership = active_network_membership(&document, &environment)?;
        let networks = canonical_networks(&document, &target, &environment, &network_membership)?;
        let workload_intent_hash = canonical_workload_intent_hash(
            document.config.project.id,
            &environment,
            &images,
            &networks,
        );
        let middlewares = load_active_middlewares(&document, &environment)?;
        let (stacks, placement) = self.prepare_document_for(StackPreparation {
            document: &document,
            environment: &environment,
            target: &target,
            check_route_registry,
            images: &images,
            middlewares: &middlewares,
            networks: &network_membership,
        })?;
        // A cluster node's routes are served by the ingress nodes, from the
        // cluster plan; nothing is claimed on this host.
        let route_claims = if served_by_cluster(&document) {
            Vec::new()
        } else {
            route_claims(&stacks)
        };
        let ingress = ingress_statuses(&document, &environment, &target, &stacks)?;
        let project_directories = stacks
            .iter()
            .map(|stack| {
                (
                    stack.manifest.stack.clone(),
                    compose_project_directory(&document, &stack.manifest.stack),
                )
            })
            .collect();
        Ok(DeploymentStackCompilation {
            unknown_keys: [document.unknown_keys.clone(), middlewares.unknown_keys].concat(),
            release: document.config.release.clone(),
            excluded_services: environment_excluded_services(&document, &environment),
            placement,
            rollout: deploy_policy::rollout_policy(&document, &environment)?,
            project_id: document.config.project.id,
            project_name: document.config.project.name,
            environment,
            target,
            selection: resolved,
            stacks,
            route_claims,
            middlewares: middlewares.definitions,
            project_directories,
            networks,
            workload_intent_hash,
            ingress,
        })
    }

    /// Removes the config file versions `plan` does not mount, once its
    /// deployment has converged. Returns the versions removed.
    pub(crate) fn prune_config_files(
        &self,
        plan: &crate::domain::deployment::DeploymentPlan,
    ) -> Result<Vec<String>, MmError> {
        let mut keep = plan
            .config_files
            .iter()
            .map(|file| file.version.clone())
            .collect::<BTreeSet<_>>();
        if plan
            .stacks
            .iter()
            .any(|stack| stack.artifact.manifest.masks_a_file())
        {
            keep.insert(crate::domain::development_mount::mask_file_version());
        }
        self.artifacts
            .prune_config_files(plan.target_id, plan.project_id, &plan.environment, &keep)
    }

    pub(crate) fn resolve_target(&self, name: &str) -> Result<NamedTarget, MmError> {
        self.targets.resolve_target(name)
    }

    pub(crate) fn targets(&self) -> &dyn TargetStore {
        self.targets.as_ref()
    }

    fn prepare_document_for(
        &self,
        preparation: StackPreparation<'_>,
    ) -> Result<(Vec<PreparedStackArtifact>, PlacementNotes), MmError> {
        let (compiled, placement) = compile_document_for(DocumentCompilation {
            document: preparation.document,
            environment: preparation.environment,
            target: preparation.target,
            images: preparation.images,
            middlewares: preparation.middlewares,
            networks: preparation.networks,
            config_files: &self.artifacts.config_file_root(
                preparation.target.definition.id,
                preparation.document.config.project.id,
                preparation.environment,
            ),
            user_ids: self.user_ids.as_deref(),
        })?;
        if preparation.check_route_registry
            && let Some(routes) = &self.routes
        {
            let claims = compiled_route_claims(&compiled);
            let conflicts = routes.conflicts(&claims)?;
            if let Some(conflict) = conflicts.first() {
                return Err(MmError::Environment {
                    message: format!(
                        "route {}{} for stack '{}' conflicts with project '{}' route '{}' of environment '{}' on target '{}'; give one of them another host or path in its project's configuration and deploy that project, then run `mm deploy plan` again",
                        conflict.proposed.host,
                        conflict.proposed.path_prefix,
                        conflict.proposed.stack,
                        conflict.existing.project_name,
                        conflict.existing.route,
                        conflict.existing.environment,
                        conflict.proposed.target
                    ),
                });
            }
        }
        Ok((self.artifacts.publish(&compiled)?, placement))
    }

    pub(crate) fn runtime_targets(
        &self,
        document: &ConfigDocument,
        environment: &str,
        target: &crate::domain::target::NamedTarget,
    ) -> Result<Vec<RuntimeTarget>, MmError> {
        let middlewares = load_active_middlewares(document, environment)?;
        let networks = active_network_membership(document, environment)?;
        let images = self.state.as_deref().map_or_else(
            || Ok(BTreeMap::new()),
            |state| resolve_deployment_images(document, environment, Some(state)),
        )?;
        Ok(self
            .prepare_document_for(StackPreparation {
                document,
                environment,
                target,
                check_route_registry: true,
                images: &images,
                middlewares: &middlewares,
                networks: &networks,
            })?
            .0
            .into_iter()
            .map(|artifact| RuntimeTarget {
                root: document.root.clone(),
                project_directory: compose_project_directory(document, &artifact.manifest.stack),
                target: Some(target.clone()),
                project_id: artifact.manifest.project_id,
                environment: artifact.manifest.environment.clone(),
                stack_name: artifact.manifest.stack.clone(),
                project_name: artifact.manifest.compose_project_name.clone(),
                artifact_hash: artifact.manifest.artifact_hash.clone(),
                user_compose_file_count: artifact.manifest.user_files.len(),
                compose_files: artifact.compose_files,
                interpolation_environment: Default::default(),
                installation_scoped: false,
                registry_credentials: Vec::new(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;

/// The release `selection` plans, with the images the compilation
/// resolved, once the commit's `release-inputs:` accept its name and inputs.
fn planned_release(
    selection: &WorkspaceSelection,
    document: &ConfigDocument,
    images: &BTreeMap<String, String>,
) -> Result<Option<crate::domain::release_facts::ReleaseFacts>, MmError> {
    let Some(release) = &selection.release else {
        return Ok(None);
    };
    release
        .check(&document.config.release_inputs)
        .map_err(|message| MmError::ReleaseInput { message })?;
    Ok(Some(release.as_ref().clone().with_images(images.clone())))
}
