use super::*;
use crate::domain::config::BuildTargetSelection;

pub(crate) fn compile_document(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    images: &BTreeMap<String, String>,
    middlewares: &LoadedProjectMiddleware,
    networks: &BTreeMap<(String, String), BTreeSet<String>>,
) -> Result<Vec<CompiledStackArtifact>, MmError> {
    compile_document_for(DocumentCompilation {
        document,
        environment,
        target,
        images,
        middlewares,
        networks,
        config_files: Path::new(crate::domain::config_file::CONFIG_FILE_DIRECTORY),
        user_ids: None,
    })
    .map(|(artifacts, _)| artifacts)
}

pub(crate) struct DocumentCompilation<'a> {
    pub document: &'a ConfigDocument,
    pub environment: &'a str,
    pub target: &'a NamedTarget,
    pub images: &'a BTreeMap<String, String>,
    pub middlewares: &'a LoadedProjectMiddleware,
    pub networks: &'a BTreeMap<(String, String), BTreeSet<String>>,
    /// Where the host keeps the versions of the environment's config
    /// files. `compile_document`, which publishes nothing, names a
    /// directory that is never written.
    pub config_files: &'a Path,
    /// Set on a host that runs rootful Podman: where each service's own
    /// user ids are kept.
    pub user_ids: Option<&'a dyn crate::app::ports::ServiceUserIds>,
}

/// Compiles every stack. On a cluster node the configuration is the node's
/// view, so a stack holds only the services placed there; the notes say
/// what that left out.
pub(crate) fn compile_document_for(
    input: DocumentCompilation<'_>,
) -> Result<(Vec<CompiledStackArtifact>, PlacementNotes), MmError> {
    let DocumentCompilation {
        document,
        environment,
        target,
        images,
        middlewares,
        networks,
        ..
    } = input;
    let host = super::user_namespace::ServiceHost::of(&input);
    let active_mounts = crate::app::development_mount::active_mounts(
        document,
        environment,
        target.definition.transport == crate::domain::target::TargetTransport::Local,
    )?;
    let scope = SourceScope::compiling(document, environment, target);
    let project_slug = scope
        .automatic_local_ingress()
        .then(|| canonical_project_slug(document))
        .transpose()?;
    let mut artifacts = Vec::with_capacity(document.config.stacks.len());
    let mut notes = PlacementNotes::default();
    for (stack_name, stack) in &document.config.stacks {
        notes.projects.insert(
            stack_name.clone(),
            stack_project_name(document, environment, target, stack_name),
        );
        let ingress_routes = resolve_ingress_services(document, environment, stack_name, stack)?;
        let policy = SourcePolicy::new(document, environment, stack_name, &ingress_routes, scope);
        let source_files = load_compose_sources(
            document,
            stack,
            &policy,
            StackPlacement::of(document, stack_name, &mut notes),
        )?;
        if !policy.excluded.is_empty() && !sources_have_services(&source_files)? {
            continue;
        }
        let host_network = policy.host_network.services(&source_files)?;
        let services = compile_services(
            document,
            environment,
            target,
            stack_name,
            &source_files,
            images,
            &active_mounts,
        )?;
        require_deployment_images(document, &services)?;
        let restart_defaults = services_without_restart(document, stack_name, &source_files)?;
        let shared = shared_volumes_of(document, environment, stack_name, &source_files)?;
        let stack_host = host.stack(stack_name, &services, &source_files, &shared)?;
        notes.note_stack(
            stack_name,
            &restart_defaults,
            &shared,
            &stack_host.config_files,
        );
        notes.note_user_ids(stack_name, &stack_host.user_ids);
        let routes = stack_routes(
            StackRouteInput {
                document,
                environment,
                stack_name,
                services: &services,
                middlewares,
            },
            &ingress_routes,
            project_slug.as_ref(),
        )?;
        artifacts.push(finish_stack_artifact(StackArtifactInput {
            document,
            environment,
            target,
            stack_name,
            compose_project_name: stack_project_name(document, environment, target, stack_name),
            source_files,
            services,
            routes,
            middlewares,
            networks,
            restart_defaults,
            shared,
            host: stack_host,
            host_network,
        })?);
    }
    Ok((artifacts, notes))
}

impl PlacementNotes {
    /// What the plan says about one compiled stack.
    fn note_stack(
        &mut self,
        stack: &str,
        restart_defaults: &BTreeSet<String>,
        shared: &super::shared_volumes::NodeVolumes,
        config_files: &super::config_files::StackConfigFiles,
    ) {
        self.shared_volumes.extend(shared.uses(stack));
        self.config_files.extend(config_files.uses(stack));
        self.restart_defaults.extend(
            restart_defaults
                .iter()
                .map(|service| (stack.to_owned(), service.clone())),
        );
    }

    fn note_user_ids(&mut self, stack: &str, user_ids: &super::user_namespace::StackUserIds) {
        self.user_ids.extend(
            user_ids
                .iter()
                .map(|(service, ids)| ((stack.to_owned(), service.clone()), *ids)),
        );
    }
}

/// The stack's shared volumes that a service of this node mounts.
fn shared_volumes_of(
    document: &ConfigDocument,
    environment: &str,
    stack_name: &str,
    sources: &[CompiledComposeSource],
) -> Result<super::shared_volumes::NodeVolumes, MmError> {
    match document.config.stacks.get(stack_name) {
        Some(stack) => {
            super::shared_volumes::node_volumes(document, environment, stack_name, stack, sources)
        }
        None => Ok(super::shared_volumes::NodeVolumes::default()),
    }
}

fn stack_project_name(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    stack_name: &str,
) -> String {
    target_scoped_compose_project_name(
        &compose_project_name(document, environment, stack_name),
        target.definition.id,
        document.config.project.id,
        environment,
        stack_name,
    )
}

pub(super) fn routed_service_names<'a>(
    document: &'a ConfigDocument,
    environment: &str,
    stack_name: &str,
    ingress_routes: &'a [ResolvedIngressRoute],
    automatic_local_ingress: bool,
) -> BTreeSet<&'a str> {
    let mut routed = ingress_routes
        .iter()
        .map(|route| route.service.as_str())
        .collect::<BTreeSet<_>>();
    if automatic_local_ingress {
        routed.extend(automatic_ingress_services(
            document,
            environment,
            stack_name,
        ));
    }
    routed
}

struct StackRouteInput<'a> {
    document: &'a ConfigDocument,
    environment: &'a str,
    stack_name: &'a str,
    services: &'a [StackServiceManifest],
    middlewares: &'a LoadedProjectMiddleware,
}

/// Explicit routes plus, in the development environment, automatic local ones.
fn stack_routes(
    input: StackRouteInput<'_>,
    ingress_routes: &[ResolvedIngressRoute],
    project_slug: Option<&ProjectSlug>,
) -> Result<Vec<StackRouteManifest>, MmError> {
    let mut routes = resolve_ingress_ports(
        input.document,
        input.environment,
        input.stack_name,
        ingress_routes,
        input.services,
        &input.middlewares.names,
    )?;
    if let Some(project_slug) = project_slug {
        routes.extend(automatic_local_routes(
            input.document,
            input.environment,
            input.stack_name,
            project_slug,
            input.services,
            ingress_routes,
            &input.middlewares.names,
        )?);
    }
    Ok(routes)
}

fn automatic_ingress_services<'a>(
    document: &'a ConfigDocument,
    environment: &str,
    stack: &str,
) -> Vec<&'a str> {
    document
        .config
        .modules
        .values()
        .flat_map(|module| &module.services)
        .filter(|(_, service)| {
            service.stack == stack
                && service.is_active(environment)
                && service.takes_automatic_local_ingress()
        })
        .map(|(name, _)| name.as_str())
        .collect()
}

fn load_compose_sources(
    document: &ConfigDocument,
    stack: &crate::domain::config::StackConfig,
    policy: &SourcePolicy<'_>,
    mut placement: StackPlacement<'_>,
) -> Result<Vec<CompiledComposeSource>, MmError> {
    let mut files = Vec::with_capacity(stack.files.len());
    for relative_path in &stack.files {
        let path = document.root.join(relative_path);
        ensure_no_symlink_components(&path)?;
        let bytes = read_regular_file(&path, MAX_COMPOSE_FILE_BYTES)?.ok_or_else(|| {
            MmError::Environment {
                message: format!(
                    "Compose file disappeared while compiling: {}",
                    path.display()
                ),
            }
        })?;
        reject_transitive_compose_files(&path, &bytes)?;
        let bytes = placement.place(&path, bytes)?;
        files.push((path, bytes));
    }
    if !placement.elsewhere.is_empty() {
        prune_unused_resources(&mut files)?;
    }
    let mut source_files = Vec::with_capacity(stack.files.len());
    for (relative_path, (path, bytes)) in stack.files.iter().zip(files) {
        let bytes = policy.check(&path, bytes)?;
        let bytes = super::file_references::absolute_file_references(
            &path,
            bytes,
            &policy.compose_directory,
        )?;
        source_files.push(CompiledComposeSource {
            manifest: ComposeSourceManifest {
                path: relative_path.clone(),
                digest: blake3::hash(&bytes).to_hex().to_string(),
            },
            bytes,
        });
    }
    Ok(source_files)
}

fn require_deployment_images(
    document: &ConfigDocument,
    services: &[StackServiceManifest],
) -> Result<(), MmError> {
    let mut missing = Vec::new();
    for service in services {
        let requires_digest = document
            .config
            .modules
            .get(&service.module)
            .and_then(|module| module.build.as_ref())
            .is_some_and(|build| build.enabled);
        if requires_digest && service.image.is_none() {
            missing.push(format!("{}/{}", service.module, service.service));
        }
    }
    if !missing.is_empty() {
        return Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingImages { services: missing },
        });
    }
    Ok(())
}

struct StackArtifactInput<'a> {
    document: &'a ConfigDocument,
    environment: &'a str,
    target: &'a NamedTarget,
    stack_name: &'a str,
    compose_project_name: String,
    source_files: Vec<CompiledComposeSource>,
    services: Vec<StackServiceManifest>,
    routes: Vec<StackRouteManifest>,
    middlewares: &'a LoadedProjectMiddleware,
    networks: &'a BTreeMap<(String, String), BTreeSet<String>>,
    /// Services that get mm's default restart policy.
    restart_defaults: BTreeSet<String>,
    /// The stack's shared volumes on this node.
    shared: super::shared_volumes::NodeVolumes,
    host: super::user_namespace::StackHost,
    host_network: super::host_network::HostNetworkServices,
}

fn finish_stack_artifact(input: StackArtifactInput<'_>) -> Result<CompiledStackArtifact, MmError> {
    // What hardening and the host's runtime policy make of the services;
    // a service with a route takes ingress traffic.
    let routed = input
        .routes
        .iter()
        .map(|route| route.service.as_str())
        .collect();
    let security = super::hardening::stack_security(
        input.document,
        input.environment,
        input.target,
        input.stack_name,
        &input.source_files,
        &routed,
    )?;
    let overlay = generated_overlay(GeneratedOverlayInput {
        document: input.document,
        environment: input.environment,
        stack: input.stack_name,
        managed_services: &input.services,
        routes: &input.routes,
        target: input.target,
        membership: input.networks,
        source_files: &input.source_files,
        restart_defaults: &input.restart_defaults,
        shared: &input.shared,
        security: &security.overlay,
        config_files: &input.host.config_files,
        user_ids: &input.host.user_ids,
        low_ports: &input.host.low_ports,
        host_userns: &input.host_network.host_userns,
        shared_pid: &input.host.shared_pid,
    })?;
    let mut manifest = StackArtifactManifest {
        project_id: input.document.config.project.id,
        project_name: input.document.config.project.name.clone(),
        target: input.document.config.target.clone(),
        target_id: input.target.definition.id,
        environment: input.environment.to_owned(),
        stack: input.stack_name.to_owned(),
        compose_project_name: input.compose_project_name,
        user_files: input
            .source_files
            .iter()
            .map(|file| file.manifest.clone())
            .collect(),
        generated_overlay_digest: blake3::hash(&overlay.bytes).to_hex().to_string(),
        services: input.services,
        routes: input.routes,
        middleware_inputs: input.middlewares.inputs.clone(),
        preserved_volumes: overlay.preserved_volumes,
        security: security.services,
        low_ports: input.host.low_ports.iter().cloned().collect(),
        host_network: input.host_network.uses,
        masked_paths: overlay.masked_paths,
        artifact_hash: String::new(),
    };
    manifest.artifact_hash = manifest
        .canonical_hash()
        .map_err(|message| MmError::Environment { message })?;
    let masks_a_file = manifest.masks_a_file();
    Ok(CompiledStackArtifact {
        user_files: input.source_files,
        manifest,
        generated_overlay: overlay.bytes,
        config_files: input.host.config_files.compiled(masks_a_file),
    })
}

/// Modules whose images the environment deploys from a build.
fn buildable_modules<'a>(document: &'a ConfigDocument, environment: &str) -> Vec<&'a str> {
    document
        .config
        .modules
        .iter()
        .filter(|(_, module)| {
            module
                .services
                .values()
                .any(|service| service.is_active(environment))
                && module.build.as_ref().map_or_else(
                    || {
                        document
                            .root
                            .join(&module.path)
                            .join("Dockerfile")
                            .is_file()
                    },
                    |build| build.enabled,
                )
        })
        .map(|(name, _)| name.as_str())
        .collect()
}

/// Images from a build receipt instead of this machine's build history. The
/// receipt must be a published build of this project and environment whose
/// build settings match the checkout; its identity is checked even when the
/// environment deploys no built images.
pub(crate) fn deployment_images_from_receipt(
    document: &ConfigDocument,
    environment: &str,
    receipt: &crate::domain::build::BuildReceipt,
) -> Result<BTreeMap<String, String>, MmError> {
    receipt
        .validate()
        .map_err(|message| MmError::Security { message })?;
    let buildable = buildable_modules(document, environment);
    deployment_images_from_build(
        document,
        environment,
        &buildable,
        &receipt.plan,
        &receipt.result,
    )
}

/// Where a compilation takes the images of the modules mm builds.
#[derive(Clone, Copy)]
pub(super) enum ImageSource<'a> {
    /// This machine's builds.
    Builds,
    Receipt(&'a crate::domain::build::BuildReceipt),
    /// A reviewed local build, before or after it ran.
    LocalBuild(&'a ImageBuildPlan),
}

/// The images a reviewed local build gives the modules the environment
/// runs, by the tags it loads them under.
pub(crate) fn deployment_images_from_local_plan(
    document: &ConfigDocument,
    environment: &str,
    plan: &ImageBuildPlan,
) -> Result<BTreeMap<String, String>, MmError> {
    if plan.engine.is_none()
        || plan.project_id != document.config.project.id
        || plan.environment != environment
    {
        return Err(MmError::Security {
            message: "the local build does not match the deployment identity".to_owned(),
        });
    }
    let mut resolved = BTreeMap::new();
    let mut missing = Vec::new();
    for module in buildable_modules(document, environment) {
        let selected = selected_build_variant(document, module, environment)?;
        let reference = plan
            .units
            .iter()
            .find(|unit| unit.module == module)
            .and_then(|unit| {
                let tag = unit.variant(selected.as_deref())?.tags.first()?;
                Some(format!("{}:{tag}", unit.image))
            });
        match reference {
            Some(reference) => {
                resolved.insert(module.to_owned(), reference);
            }
            None => missing.extend(
                document.config.modules[module]
                    .services
                    .iter()
                    .filter(|(_, service)| service.is_active(environment))
                    .map(|(service, _)| format!("{module}/{service}")),
            ),
        }
    }
    if !missing.is_empty() {
        return Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingImages { services: missing },
        });
    }
    Ok(resolved)
}

/// The builds a deployment from `selection` runs: on a host, published
/// builds only; on this machine also those loaded into the runtime's engine.
pub(super) fn deployable_builds(
    selection: &WorkspaceSelection,
    target: &NamedTarget,
    runtime: crate::domain::runtime::RuntimeKind,
) -> crate::domain::build::ImageBuildFilter {
    use crate::domain::build::{ImageBuildFilter, LocalImageEngine};
    if selection.host_values
        || selection.node.is_some()
        || target.definition.transport != crate::domain::target::TargetTransport::Local
    {
        ImageBuildFilter::Published
    } else {
        ImageBuildFilter::Deployable(Some(LocalImageEngine::for_runtime(runtime)))
    }
}

/// Images from this machine's newest published build, or one loaded into
/// any local engine.
pub(crate) fn resolve_deployment_images(
    document: &ConfigDocument,
    environment: &str,
    state: Option<&dyn StateStore>,
) -> Result<BTreeMap<String, String>, MmError> {
    resolve_deployment_images_from(
        document,
        environment,
        state,
        crate::domain::build::ImageBuildFilter::Deployable(None),
    )
}

pub(crate) fn resolve_deployment_images_from(
    document: &ConfigDocument,
    environment: &str,
    state: Option<&dyn StateStore>,
    filter: crate::domain::build::ImageBuildFilter,
) -> Result<BTreeMap<String, String>, MmError> {
    let buildable = buildable_modules(document, environment);
    if buildable.is_empty() {
        return Ok(BTreeMap::new());
    }
    let build = state
        .map(|state| {
            state.latest_image_build(
                document.config.project.id,
                &document.root,
                environment,
                filter,
            )
        })
        .transpose()?
        .flatten();
    let Some((plan, result)) = build else {
        return Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingImages {
                services: buildable
                    .iter()
                    .flat_map(|module| {
                        document.config.modules[*module]
                            .services
                            .iter()
                            .filter(|(_, service)| service.is_active(environment))
                            .map(move |(service, _)| format!("{module}/{service}"))
                    })
                    .collect(),
            },
        });
    };
    deployment_images_from_build(document, environment, &buildable, &plan, &result)
}

pub(super) fn deployment_images_from_build(
    document: &ConfigDocument,
    environment: &str,
    buildable: &[&str],
    plan: &ImageBuildPlan,
    result: &ImageBuildResult,
) -> Result<BTreeMap<String, String>, MmError> {
    result
        .validate_against(plan)
        .map_err(|message| MmError::Security { message })?;
    if plan.writes_archive() {
        return Err(MmError::Security {
            message: "deployment image resolution requires a published or locally loaded build"
                .to_owned(),
        });
    }
    if plan.project_id != document.config.project.id || plan.environment != environment {
        return Err(MmError::Security {
            message: "published image build does not match the deployment identity".to_owned(),
        });
    }
    let compatible = super::images::compatible_build_modules(document, plan)?;
    let mut resolved = BTreeMap::new();
    let mut missing = Vec::new();
    for module in buildable {
        let selected = selected_build_variant(document, module, environment)?;
        let candidates = result
            .images
            .iter()
            .filter(|image| {
                image.module == *module && image.target.as_deref() == selected.as_deref()
            })
            .collect::<Vec<_>>();
        let image = match candidates.as_slice() {
            [image] if compatible.contains(*module) => *image,
            _ => {
                missing.extend(
                    document.config.modules[*module]
                        .services
                        .iter()
                        .filter(|(_, service)| service.is_active(environment))
                        .map(|(service, _)| format!("{module}/{service}")),
                );
                continue;
            }
        };
        resolved.insert((*module).to_owned(), deployed_reference(plan, image));
    }
    if !missing.is_empty() {
        return Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::MissingImages { services: missing },
        });
    }
    Ok(resolved)
}

/// A published image by digest; a local one by the tag the engine has.
fn deployed_reference(plan: &ImageBuildPlan, image: &crate::domain::build::BuiltImage) -> String {
    match (plan.engine, image.tags.first()) {
        (Some(_), Some(tag)) => tag.clone(),
        _ => format!("{}@{}", image.repository, image.digest),
    }
}

pub(super) fn selected_build_variant(
    document: &ConfigDocument,
    module: &str,
    environment: &str,
) -> Result<Option<String>, MmError> {
    let Some(build) = document
        .config
        .modules
        .get(module)
        .and_then(|module| module.build.as_ref())
    else {
        return Ok(None);
    };
    let environment_variant = document
        .config
        .environments
        .get(environment)
        .and_then(|config| config.build_variant.as_deref());
    match build.select_target(environment_variant) {
        BuildTargetSelection::Default => Ok(None),
        BuildTargetSelection::Selected(variant) => Ok(Some(variant.to_owned())),
        BuildTargetSelection::Unknown(variant) => Err(MmError::Build {
            message: format!(
                "selected build variant '{variant}' is not configured for module '{module}'; add it under modules.{module}.build.targets or set modules.{module}.build.default-variant"
            ),
        }),
        BuildTargetSelection::Ambiguous => Err(MmError::Build {
            message: format!(
                "module '{module}' has multiple build variants; set environments.{environment}.build-variant or modules.{module}.build.default-variant"
            ),
        }),
    }
}

pub(super) fn canonical_project_slug(document: &ConfigDocument) -> Result<ProjectSlug, MmError> {
    ProjectSlug::normalize(&document.config.project.name)
        .map_err(|message| MmError::Environment { message })
}

pub(super) fn automatic_local_ingress_enabled(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
) -> bool {
    target.definition.transport == crate::domain::target::TargetTransport::Local
        && automatic_local_ingress_configured(document, environment)
}

/// Whether `mm.yaml` gives `environment` automatic local routes, wherever
/// it runs.
pub(super) fn automatic_local_ingress_configured(
    document: &ConfigDocument,
    environment: &str,
) -> bool {
    document.config.project.development_environment.as_deref() == Some(environment)
        && document.config.ingress.automatic_local != Some(false)
        && document
            .config
            .environments
            .get(environment)
            .is_some_and(|environment| environment.automatic_local_ingress != Some(false))
}

#[cfg(test)]
mod tests;
