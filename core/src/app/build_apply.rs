use std::collections::BTreeMap;
use std::sync::Arc;

use crate::app::ports::{
    BuildOperationLock, BuildVariantRequest, Clock, ImageBuilder, ProjectDocumentStore,
    SecretMaterialResolver, SecretMetadataInspector, StateStore, UpstreamBuildContext,
};
use crate::app::{BuildManager, WorkspaceSelection};
use crate::domain::build::{
    BuildDependencyReference, BuildReusePolicy, BuiltImage, ImageBuildPlan, ImageBuildResult,
    ImageBuildUnitPlan, ImageBuildVariantPlan, ResolvedBuildDependency, ReusableImageRecord,
    unit_execution_fingerprint,
};
use crate::domain::config::MmConfig;
use crate::domain::error::MmError;
use crate::domain::operation::{
    NewOperation, OperationEvent, OperationId, OperationKind, OperationPhase, OperationState,
};
use crate::domain::secret::SecretMaterial;
use crate::domain::workspace::WorkspaceRegistration;

#[derive(Clone)]
pub struct BuildExecutionPorts {
    pub documents: Arc<dyn ProjectDocumentStore>,
    pub state: Arc<dyn StateStore>,
    pub builder: Arc<dyn ImageBuilder>,
    pub secret_inspector: Arc<dyn SecretMetadataInspector>,
    pub secret_resolver: Arc<dyn SecretMaterialResolver>,
    pub locks: Arc<dyn BuildOperationLock>,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for BuildExecutionPorts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuildExecutionPorts")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct BuildService {
    planner: BuildManager,
    ports: BuildExecutionPorts,
}

impl std::fmt::Debug for BuildService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuildService")
            .finish_non_exhaustive()
    }
}

impl BuildService {
    #[must_use]
    pub fn new(planner: BuildManager, ports: BuildExecutionPorts) -> Self {
        Self { planner, ports }
    }

    pub async fn plan(
        &self,
        selection: &WorkspaceSelection,
        publish: bool,
    ) -> Result<ImageBuildPlan, MmError> {
        self.planner.plan(selection, publish).await
    }

    /// The latest published build of this checkout and environment, as a
    /// receipt another machine can deploy (`mm deploy plan --receipt`).
    pub fn receipt(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<crate::domain::build::BuildReceipt, MmError> {
        let document = self.ports.documents.load_from(&selection.start)?;
        let environment = crate::domain::config::resolve_environment(
            &document,
            selection.environment.as_deref(),
        )?;
        let (plan, result) = self
            .ports
            .state
            .latest_successful_image_build(
                document.config.project.id,
                &document.root,
                &environment,
            )?
            .ok_or_else(|| MmError::Build {
                message: format!(
                    "this checkout has no published build for environment '{environment}'; run `mm --environment {environment} build --publish` first"
                ),
            })?;
        crate::domain::build::BuildReceipt::new(plan, result)
            .map_err(|message| MmError::Build { message })
    }

    pub async fn apply_reviewed(
        &self,
        selection: &WorkspaceSelection,
        publish: bool,
        expected_plan_hash: &str,
    ) -> Result<ImageBuildResult, MmError> {
        let preview = self.planner.plan(selection, publish).await?;
        require_plan_hash(&preview, expected_plan_hash)?;
        let _guard = self.ports.locks.acquire(preview.project_id)?;
        let plan = self.planner.plan(selection, publish).await?;
        require_plan_hash(&plan, expected_plan_hash)?;
        plan.validate()
            .map_err(|message| MmError::Security { message })?;

        let document = self.ports.documents.load_from(&selection.start)?;
        if document.config.project.id != plan.project_id || document.source_hash != plan.config_hash
        {
            return Err(MmError::Security {
                message: "workspace configuration changed after the reviewed build plan".to_owned(),
            });
        }
        let now = self.ports.clock.now_unix_ms();
        let workspace = self.ports.state.register_workspace(
            &WorkspaceRegistration {
                project_id: plan.project_id,
                root: document.root.clone(),
                config_path: document.path,
            },
            now,
        )?;
        for operation in self.ports.state.running_operations(workspace.id)? {
            if operation.kind == OperationKind::Build {
                self.ports.builder.cleanup_operation(operation.id)?;
                self.ports.state.finish_operation(
                    operation.id,
                    OperationState::Interrupted,
                    now,
                    Some("MM_BUILD_INTERRUPTED"),
                )?;
            }
        }

        let operation_id = OperationId::new();
        self.ports.state.begin_operation(&NewOperation {
            id: operation_id,
            workspace_id: workspace.id,
            kind: OperationKind::Build,
            environment: plan.environment.clone(),
            requested_at_unix_ms: now,
            plan_hash: Some(plan.plan_hash.clone()),
        })?;
        if let Err(error) = self.ports.state.save_image_build_plan(operation_id, &plan) {
            if let Err(journal) = self.ports.state.finish_operation(
                operation_id,
                OperationState::Failed,
                self.ports.clock.now_unix_ms(),
                Some(error.code()),
            ) {
                return Err(MmError::operation_failure(
                    operation_id.0.to_string(),
                    crate::domain::error::MutationStatus::NotStarted,
                    error,
                    vec![journal.to_string()],
                    "repair the incomplete build journal before retrying",
                ));
            }
            return Err(error);
        }

        let result = self
            .execute(operation_id, &document.root, &document.config, &plan)
            .await;
        self.finish_execution(operation_id, result)
    }

    fn finish_execution(
        &self,
        operation_id: OperationId,
        result: Result<ExecutedBuild, MmError>,
    ) -> Result<ImageBuildResult, MmError> {
        match result {
            Ok(executed) => {
                if let Err(error) = self
                    .ports
                    .state
                    .save_image_build_result(operation_id, &executed.result)
                {
                    if let Err(journal) =
                        self.finish(operation_id, OperationState::Failed, Some(error.code()))
                    {
                        return Err(MmError::operation_failure(
                            operation_id.0.to_string(),
                            crate::domain::error::MutationStatus::Applied,
                            error,
                            vec![journal.to_string()],
                            "the image build completed; repair its result journal before retrying",
                        ));
                    }
                    return Err(error);
                }
                self.finish(operation_id, OperationState::Succeeded, None)?;
                executed.operation_guard.preserve();
                Ok(executed.result)
            }
            Err(error) => {
                if let Err(journal_error) =
                    self.finish(operation_id, OperationState::Failed, Some(error.code()))
                {
                    return Err(MmError::operation_failure(
                        operation_id.0.to_string(),
                        crate::domain::error::MutationStatus::Unknown,
                        error,
                        vec![journal_error.to_string()],
                        "inspect build artifacts and repair the operation journal before retrying",
                    ));
                }
                Err(error)
            }
        }
    }

    async fn execute(
        &self,
        operation_id: OperationId,
        project_root: &std::path::Path,
        config: &MmConfig,
        plan: &ImageBuildPlan,
    ) -> Result<ExecutedBuild, MmError> {
        self.record_phase(operation_id, OperationPhase::Build)?;
        let operation_guard = self.ports.builder.begin_operation(operation_id)?;
        let mut images = Vec::new();
        let mut dependencies = Vec::new();
        let mut built_by_fingerprint = BTreeMap::<String, usize>::new();
        for unit in &plan.units {
            let secrets = self
                .resolve_unit_secrets(unit, config, &plan.environment)
                .await?;
            let secret_revisions = self
                .secret_revisions_for_reuse(unit, config, &plan.environment)
                .await?;
            for variant in &unit.variants {
                let prepared = inject_upstream_digests(plan, unit, variant, &images)?;
                let executable = prepared.executable;
                let upstream_digests = prepared.upstream_digests;
                let variant_dependencies = prepared.dependencies;
                let upstream_contexts = prepared.upstream_contexts;
                let fingerprint = unit_execution_fingerprint(
                    &unit.input_fingerprint,
                    &plan.environment,
                    Some(variant),
                    &upstream_digests,
                    &secret_revisions,
                )
                .map_err(|message| MmError::Build { message })?;
                let previous = (unit.reuse_policy != BuildReusePolicy::Forbidden)
                    .then(|| {
                        built_by_fingerprint
                            .get(&fingerprint)
                            .and_then(|index| images.get(*index))
                    })
                    .flatten();
                let image = if let Some(previous) = previous {
                    self.reuse_current_execution(
                        previous,
                        unit,
                        variant,
                        &fingerprint,
                        plan.publish,
                    )
                    .await?
                } else if let Some(reused) = self
                    .try_reuse(operation_id, plan, unit, variant, &fingerprint)
                    .await?
                {
                    reused
                } else {
                    let built = self
                        .ports
                        .builder
                        .build_variant(BuildVariantRequest {
                            operation_id,
                            project_root,
                            plan_hash: &plan.plan_hash,
                            platforms: &plan.platforms,
                            publish: plan.publish,
                            unit: &executable,
                            variant,
                            secrets: &secrets,
                            dependencies: &variant_dependencies,
                            upstream_contexts: &upstream_contexts,
                        })
                        .await?;
                    if plan.publish && unit.reuse_policy != BuildReusePolicy::Forbidden {
                        self.ports
                            .state
                            .record_reusable_image(ReusableImageRecord {
                                project_id: plan.project_id,
                                environment: &plan.environment,
                                fingerprint: &fingerprint,
                                module: &unit.module,
                                variant: variant.target.as_deref(),
                                repository: &built.repository,
                                digest: &built.digest,
                                operation_id,
                            })?;
                    }
                    let mut built = built;
                    built.execution_fingerprint = fingerprint.clone();
                    built
                };
                let mut image = image;
                image.secret_revisions = secret_revisions.clone();
                built_by_fingerprint.insert(fingerprint, images.len());
                images.push(image);
                dependencies.extend(variant_dependencies);
            }
        }
        self.record_phase(operation_id, OperationPhase::Verify)?;
        validated_build_result(operation_id, plan, images, dependencies, operation_guard)
    }

    async fn reuse_current_execution(
        &self,
        previous: &BuiltImage,
        unit: &ImageBuildUnitPlan,
        variant: &ImageBuildVariantPlan,
        fingerprint: &str,
        publish: bool,
    ) -> Result<BuiltImage, MmError> {
        let digest = if publish {
            self.ports
                .builder
                .retag_variant(
                    &previous.repository,
                    &previous.digest,
                    &unit.image,
                    &variant.tags,
                )
                .await?
        } else {
            previous.digest.clone()
        };
        Ok(BuiltImage {
            module: unit.module.clone(),
            target: variant.target.clone(),
            repository: unit.image.clone(),
            tags: variant
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest,
            execution_fingerprint: fingerprint.to_owned(),
            secret_revisions: BTreeMap::new(),
            archive: previous.archive.clone(),
            reused: true,
        })
    }

    async fn secret_revisions_for_reuse(
        &self,
        unit: &ImageBuildUnitPlan,
        config: &MmConfig,
        environment: &str,
    ) -> Result<BTreeMap<String, String>, MmError> {
        if unit.reuse_policy != BuildReusePolicy::RevisionGated {
            return Ok(BTreeMap::new());
        }
        let configured = config
            .modules
            .get(&unit.module)
            .and_then(|module| module.build.as_ref())
            .map(|build| &build.secrets);
        let mut revisions = BTreeMap::new();
        for id in unit.output_secret_ids() {
            let name = configured
                .and_then(|secrets| secrets.get(id))
                .ok_or_else(|| MmError::Security {
                    message: format!(
                        "reviewed build secret '{id}' no longer references a declared secret"
                    ),
                })?;
            let declaration = config.secrets.get(name).ok_or_else(|| MmError::Security {
                message: format!(
                    "reviewed build secret '{id}' no longer references a declared secret"
                ),
            })?;
            let scope = crate::domain::secret::SecretScope::new(environment);
            let reference = declaration
                .storage
                .reference()
                .map_err(|message| MmError::Build {
                    message: format!(
                        "build secret '{name}' has an invalid storage location: {message}"
                    ),
                })?;
            let metadata = self
                .ports
                .secret_inspector
                .inspect(&scope, &reference)
                .await?;
            let revision = metadata.revision.ok_or_else(|| MmError::Secret {
                message: format!("build secret '{id}' has no revision for reuse"),
            })?;
            revisions.insert(id.clone(), revision);
        }
        Ok(revisions)
    }

    async fn try_reuse(
        &self,
        operation_id: crate::domain::operation::OperationId,
        plan: &ImageBuildPlan,
        unit: &ImageBuildUnitPlan,
        variant: &ImageBuildVariantPlan,
        fingerprint: &str,
    ) -> Result<Option<BuiltImage>, MmError> {
        if !plan.publish || unit.reuse_policy == BuildReusePolicy::Forbidden {
            return Ok(None);
        }
        let Some(reusable) = self.ports.state.lookup_reusable_image(
            plan.project_id,
            &plan.environment,
            fingerprint,
            &unit.module,
            variant.target.as_deref(),
        )?
        else {
            return Ok(None);
        };
        if reusable.repository != unit.image {
            return Err(MmError::Security {
                message: format!(
                    "reusable image for module '{}' does not match the reviewed repository",
                    unit.module
                ),
            });
        }
        let digest = self
            .ports
            .builder
            .retag_variant(
                &reusable.repository,
                &reusable.digest,
                &unit.image,
                &variant.tags,
            )
            .await?;
        if digest != reusable.digest {
            return Err(MmError::Security {
                message: format!(
                    "retagging module '{}' changed the immutable digest",
                    unit.module
                ),
            });
        }
        self.ports
            .state
            .record_reusable_image(ReusableImageRecord {
                project_id: plan.project_id,
                environment: &plan.environment,
                fingerprint,
                module: &unit.module,
                variant: variant.target.as_deref(),
                repository: &unit.image,
                digest: &digest,
                operation_id,
            })?;
        Ok(Some(BuiltImage {
            module: unit.module.clone(),
            target: variant.target.clone(),
            repository: unit.image.clone(),
            tags: variant
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect(),
            digest,
            execution_fingerprint: fingerprint.to_owned(),
            secret_revisions: BTreeMap::new(),
            archive: None,
            reused: true,
        }))
    }

    async fn resolve_unit_secrets(
        &self,
        unit: &ImageBuildUnitPlan,
        config: &MmConfig,
        environment: &str,
    ) -> Result<BTreeMap<String, SecretMaterial>, MmError> {
        let configured = config
            .modules
            .get(&unit.module)
            .and_then(|module| module.build.as_ref())
            .map(|build| &build.secrets);
        let configured_ids = configured
            .into_iter()
            .flat_map(|secrets| secrets.keys())
            .cloned()
            .collect::<Vec<_>>();
        if configured_ids != unit.secret_ids {
            return Err(MmError::Security {
                message: "build secret configuration changed after plan review".to_owned(),
            });
        }
        let mut resolved = BTreeMap::new();
        for (id, name) in unit.secret_ids.iter().zip(
            configured
                .into_iter()
                .flat_map(std::collections::BTreeMap::values),
        ) {
            let declaration = config.secrets.get(name).ok_or_else(|| MmError::Security {
                message: format!(
                    "reviewed build secret '{id}' no longer references a declared secret"
                ),
            })?;
            let scope = crate::domain::secret::SecretScope::new(environment);
            let reference = declaration
                .storage
                .reference()
                .map_err(|message| MmError::Build {
                    message: format!(
                        "build secret '{name}' has an invalid storage location: {message}"
                    ),
                })?;
            let resolution = self
                .ports
                .secret_resolver
                .resolve(&scope, &reference)
                .await?;
            let material = resolution.material.ok_or_else(|| MmError::Secret {
                message: format!("build secret '{id}' is unavailable"),
            })?;
            resolved.insert(id.clone(), material);
        }
        Ok(resolved)
    }

    fn record_phase(
        &self,
        operation_id: OperationId,
        phase: OperationPhase,
    ) -> Result<(), MmError> {
        self.ports.state.append_event(
            operation_id,
            self.ports.clock.now_unix_ms(),
            &OperationEvent::PhaseChanged {
                operation_id,
                phase,
            },
        )
    }

    fn finish(
        &self,
        operation_id: OperationId,
        state: OperationState,
        error_code: Option<&str>,
    ) -> Result<(), MmError> {
        self.ports.state.finish_operation(
            operation_id,
            state,
            self.ports.clock.now_unix_ms(),
            error_code,
        )
    }
}

struct ExecutedBuild {
    result: ImageBuildResult,
    operation_guard: Box<dyn crate::app::ports::BuildOperationGuard>,
}

fn validated_build_result(
    operation_id: OperationId,
    plan: &ImageBuildPlan,
    images: Vec<BuiltImage>,
    dependencies: Vec<ResolvedBuildDependency>,
    operation_guard: Box<dyn crate::app::ports::BuildOperationGuard>,
) -> Result<ExecutedBuild, MmError> {
    if images.is_empty() {
        return Err(MmError::Build {
            message: "image builder produced no results".to_owned(),
        });
    }
    let result = ImageBuildResult {
        operation_id,
        plan_hash: plan.plan_hash.clone(),
        images,
        dependencies,
    };
    result
        .validate_against(plan)
        .map_err(|message| MmError::Security { message })?;
    Ok(ExecutedBuild {
        result,
        operation_guard,
    })
}

struct PreparedDependentBuild {
    executable: ImageBuildUnitPlan,
    upstream_digests: BTreeMap<String, String>,
    dependencies: Vec<ResolvedBuildDependency>,
    upstream_contexts: Vec<UpstreamBuildContext>,
}

fn inject_upstream_digests(
    plan: &ImageBuildPlan,
    unit: &ImageBuildUnitPlan,
    variant: &ImageBuildVariantPlan,
    images: &[BuiltImage],
) -> Result<PreparedDependentBuild, MmError> {
    let mut executable = unit.clone();
    let mut upstream_digests = BTreeMap::new();
    let mut dependencies = Vec::new();
    let mut upstream_contexts = Vec::new();
    for dependency in &unit.dependencies {
        let upstream = plan
            .units
            .iter()
            .find(|candidate| candidate.module == dependency.producer_module)
            .ok_or_else(|| MmError::Build {
                message: format!(
                    "module '{}' depends on unknown module '{}'",
                    unit.module, dependency.producer_module
                ),
            })?;
        let image = select_upstream_image(
            images,
            &dependency.producer_module,
            variant.target.as_deref(),
        )?;
        let reference = format!("{}@{}", upstream.image, image.digest);
        if let BuildDependencyReference::BuildArgument { arg, .. } = &dependency.reference {
            executable.args.insert(arg.clone(), reference.clone());
        }
        upstream_digests.insert(dependency.fingerprint_key(), image.digest.clone());
        dependencies.push(ResolvedBuildDependency {
            edge: dependency.clone(),
            dependent_target: variant.target.clone(),
            digest: image.digest.clone(),
            immutable_reference: reference.clone(),
        });
        if !upstream_contexts
            .iter()
            .any(|context: &UpstreamBuildContext| context.immutable_reference == reference)
        {
            upstream_contexts.push(UpstreamBuildContext {
                immutable_reference: reference,
                digest: image.digest.clone(),
                local_archive: image.archive.clone(),
            });
        }
    }
    Ok(PreparedDependentBuild {
        executable,
        upstream_digests,
        dependencies,
        upstream_contexts,
    })
}

fn select_upstream_image<'a>(
    images: &'a [BuiltImage],
    module: &str,
    variant: Option<&str>,
) -> Result<&'a BuiltImage, MmError> {
    let matches = images
        .iter()
        .filter(|image| image.module == module)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(MmError::Build {
            message: format!("dependency '{module}' was not built before its dependents"),
        }),
        [image] => Ok(*image),
        many => many
            .iter()
            .copied()
            .find(|image| image.target.as_deref() == variant)
            .or_else(|| many.iter().copied().find(|image| image.target.is_none()))
            .ok_or_else(|| MmError::Build {
                message: format!(
                    "dependency '{module}' has no image matching variant {}",
                    variant.unwrap_or("default")
                ),
            }),
    }
}

fn require_plan_hash(plan: &ImageBuildPlan, expected: &str) -> Result<(), MmError> {
    if plan.plan_hash == expected {
        Ok(())
    } else {
        Err(MmError::State {
            message: format!(
                "image build plan changed (expected {expected}, current {}); review the new plan",
                plan.plan_hash
            ),
        })
    }
}

mod import;

#[cfg(test)]
mod tests;
