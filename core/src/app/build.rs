use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::app::WorkspaceSelection;
use crate::app::config_validation::validate;
use crate::app::dockerfile::{DockerfileBaseKind, ParsedDockerfile, parse as parse_dockerfile};
use crate::app::ports::{BuildSourceInspector, ProjectDocumentStore};
use crate::domain::build::{
    BuildDependencyReference, BuildReceipt, BuildReusePolicy, ContextIgnore, FirstPartyBuildEdge,
    IgnoreVerdict, ImageBuildPlan, ImageBuildScope, ImageBuildTrigger, ImageBuildUnitPlan,
    ImageBuildVariantPlan, LocalImageEngine, PromotedReceipt, automatic_tags, build_version,
    is_image_repository, local_image_repository, local_image_tag, local_platform, suffixed_tags,
    unit_input_fingerprint,
};
use crate::domain::config::{
    BuildTarget, BuildTargetSelection, ConfigDocument, ModuleBuildConfig, ModuleConfig,
    resolve_environment,
};
use crate::domain::diagnostic::Severity;
use crate::domain::error::MmError;
use crate::domain::path_policy::ensure_no_symlink_components;

const MAX_CONTEXT_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CONTEXT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_CONTEXT_FILES: usize = 100_000;
const MAX_IGNORE_FILE_BYTES: u64 = 1024 * 1024;
const SNAPSHOT_DOCKERFILE: &str = ".mm-external.Dockerfile";
const SNAPSHOT_DOCKERIGNORE: &str = ".mm-external.Dockerfile.dockerignore";
const DOCKERIGNORE: &str = ".dockerignore";

mod graph;
use graph::*;
mod scope;
pub use scope::BuildRequest;
use scope::{modules_in_scope, publish_scope};
mod source;
#[cfg(test)]
pub(crate) use source::hash_context;
use source::*;
pub(crate) use source::{normalize_image_repository, plan_variant, snapshot_build_context};

#[derive(Clone)]
pub struct BuildManager {
    documents: Arc<dyn ProjectDocumentStore>,
    source: Arc<dyn BuildSourceInspector>,
    engine: Option<LocalImageEngine>,
}

impl std::fmt::Debug for BuildManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuildManager")
            .finish_non_exhaustive()
    }
}

impl BuildManager {
    #[must_use]
    pub fn new(
        documents: Arc<dyn ProjectDocumentStore>,
        source: Arc<dyn BuildSourceInspector>,
    ) -> Self {
        Self {
            documents,
            source,
            engine: None,
        }
    }

    /// Builds without `--publish` load into `engine`, for an environment
    /// on this machine; without one they write an OCI archive.
    #[must_use]
    pub fn with_local_engine(mut self, engine: Option<LocalImageEngine>) -> Self {
        self.engine = engine;
        self
    }

    /// The engine a build without `--publish` loads into.
    #[must_use]
    pub fn local_engine(&self) -> Option<LocalImageEngine> {
        self.engine
    }

    /// Whether the selected environment runs a module that mm builds.
    pub fn builds_modules(&self, selection: &WorkspaceSelection) -> Result<bool, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        Ok(modules_in_scope(&document, &environment)
            .iter()
            .any(|name| builds_image(&document, name)))
    }

    /// The modules `mm build --publish` builds for the selected
    /// environment, each with the environments it is for.
    pub fn publish_scope(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        Ok(publish_scope(&document, &environment)
            .into_iter()
            .filter(|(name, _)| builds_image(&document, name))
            .collect())
    }

    /// Refuses a receipt built for another environment that has no image
    /// of a module the selected environment runs. Only a checkout at the
    /// receipt's commit with the configuration it was built from is checked.
    pub async fn check_receipt_covers(
        &self,
        selection: &WorkspaceSelection,
        receipt: &BuildReceipt,
    ) -> Result<(), MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        if receipt.plan.environment == environment
            || document.source_hash != receipt.plan.config_hash
            || self.source.inspect(&document.root).await?.commit != receipt.source_commit()
        {
            return Ok(());
        }
        refuse_missing_modules(&document, &environment, receipt)
    }

    pub async fn plan(
        &self,
        selection: &WorkspaceSelection,
        request: impl Into<BuildRequest>,
    ) -> Result<ImageBuildPlan, MmError> {
        let request = request.into();
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let validation = validate(&document, &environment);
        let errors = validation
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count();
        if errors > 0 {
            return Err(MmError::invalid(&validation.diagnostics));
        }

        let source = self.source.inspect(&document.root).await?;
        let engine = self.engine.filter(|_| !request.publish);
        let scope = match (request.all, request.publish) {
            (true, _) => None,
            (false, true) => Some(publish_scope(&document, &environment).into_keys().collect()),
            (false, false) => Some(modules_in_scope(&document, &environment)),
        };
        plan_from_source(document, environment, source, (request, engine, scope), &[])
    }

    /// Checks a published build receipt against this checkout for the
    /// selected environment. A receipt built for that environment is used as
    /// it is; deployment applies the usual compatibility rules. A receipt
    /// built for another environment is promoted only when this
    /// environment's configuration at the receipt's commit would build
    /// exactly the same units from the same inputs.
    pub async fn receipt_for_environment(
        &self,
        selection: &WorkspaceSelection,
        receipt: &BuildReceipt,
        checkout: &ReceiptCheckout,
    ) -> Result<PromotedReceipt, MmError> {
        receipt
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        if receipt.plan.project_id != document.config.project.id {
            return Err(MmError::Security {
                message: "the build receipt is for another project".to_owned(),
            });
        }
        let added = match checkout {
            ReceiptCheckout::Worktree => {
                let head = self.source.inspect(&document.root).await?.commit;
                if head != receipt.source_commit() {
                    return Err(MmError::Build {
                        message: format!(
                            "the build receipt is a build of commit {}, but this checkout is at {head}; check out the receipt's commit",
                            receipt.source_commit()
                        ),
                    });
                }
                &[][..]
            }
            ReceiptCheckout::Export { added } => added.as_slice(),
        };
        if receipt.plan.environment == environment {
            return receipt
                .unpromoted()
                .map_err(|message| MmError::Security { message });
        }
        let validation = validate(&document, &environment);
        let errors = validation
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count();
        if errors > 0 {
            return Err(MmError::invalid(&validation.diagnostics));
        }
        refuse_missing_modules(&document, &environment, receipt)?;
        let scope = Some(modules_in_scope(&document, &environment));
        // Plan from the receipt's source so VERSION and tags match the build.
        let current = plan_from_source(
            document,
            environment,
            receipt.plan.source.clone(),
            (BuildRequest::from(true), None, scope),
            added,
        )?;
        receipt
            .promote(&current)
            .map_err(|message| MmError::Build { message })
    }
}

/// Whether mm builds an image for `module`: from its `build:` section, or
/// from a Dockerfile in its directory.
fn builds_image(document: &ConfigDocument, module: &str) -> bool {
    document.config.modules.get(module).is_some_and(|module| {
        module.build.as_ref().map_or_else(
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
}

/// The first module `environment` runs and builds that `receipt`, built
/// for another environment, has no image of, and the command that
/// publishes it.
fn refuse_missing_modules(
    document: &ConfigDocument,
    environment: &str,
    receipt: &BuildReceipt,
) -> Result<(), MmError> {
    let built = receipt.plan.environment.as_str();
    let Some(module) = modules_in_scope(document, environment)
        .into_iter()
        .filter(|name| builds_image(document, name))
        .find(|name| !receipt.plan.units.iter().any(|unit| &unit.module == name))
    else {
        return Ok(());
    };
    let fix = if publish_scope(document, built).contains_key(&module) {
        format!(
            "publish this commit again with `mm -e {built} build --publish`, which builds it for '{environment}', and write a new receipt with `mm -e {built} build receipt FILE`"
        )
    } else {
        let secrets = document
            .config
            .modules
            .get(&module)
            .and_then(|module| module.build.as_ref())
            .is_some_and(|build| reuse_policy_for(build) != BuildReusePolicy::Allowed);
        let why = if secrets {
            "it mounts build secrets whose values can differ between environments, so it is built in each environment that runs it".to_owned()
        } else {
            format!("a published build of '{built}' does not build it for '{environment}'")
        };
        format!("{why}; publish it with `mm -e {environment} build --publish`")
    };
    Err(MmError::Build {
        message: format!(
            "the build receipt from environment '{built}' has no image of module '{module}', which '{environment}' runs; {fix}"
        ),
    })
}

/// Where a build receipt is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptCheckout {
    /// A Git worktree: its HEAD must be the receipt's commit.
    Worktree,
    /// An export of the receipt's commit that the caller has verified, such
    /// as a remote host's checkout. `added` lists files the export added that
    /// are not part of the commit; they are left out of build contexts.
    Export { added: Vec<PathBuf> },
}

fn plan_from_source(
    document: ConfigDocument,
    environment: String,
    source: crate::domain::build::BuildSource,
    (request, engine, scope): (
        BuildRequest,
        Option<LocalImageEngine>,
        Option<BTreeSet<String>>,
    ),
    excluded: &[PathBuf],
) -> Result<ImageBuildPlan, MmError> {
    let publish = request.publish;
    if publish && source.dirty {
        return Err(MmError::Build {
            message: "publishing from a dirty worktree is forbidden".to_owned(),
        });
    }
    if publish && !source.tags_complete {
        return Err(MmError::Build {
            message:
                "publishing requires a complete Git tag inventory; fetch full history and tags"
                    .to_owned(),
        });
    }
    let base_tags = automatic_tags(&source, document.config.build.tag_strategy)
        .map_err(|message| MmError::Build { message })?;
    let version = build_version(&source).map_err(|message| MmError::Build { message })?;
    let environment_variant = document
        .config
        .environments
        .get(&environment)
        .and_then(|config| config.build_variant.as_deref());
    let mut units = Vec::new();
    for (module_name, module) in &document.config.modules {
        if scope
            .as_ref()
            .is_some_and(|scope| !scope.contains(module_name))
        {
            continue;
        }
        if let Some(unit) = plan_module(
            &document,
            (module_name, module),
            environment_variant,
            &base_tags,
            &version,
            excluded,
        )? {
            units.push(unit);
        }
    }
    if units.is_empty() {
        return Err(MmError::Build {
            message: if request.all {
                "no buildable modules were found; add a module Dockerfile or build configuration"
                    .to_owned()
            } else {
                format!(
                    "environment '{environment}' runs no module that mm builds; add a Dockerfile or build configuration to one of its modules, or build every module with `mm -e {environment} build --all`"
                )
            },
        });
    }
    infer_first_party_bases(&document.root, &mut units)?;
    let mut units = topological_build_units(units)?;
    let platforms = match engine {
        Some(_) => vec![local_platform(&document.config.build.platforms)],
        None => document.config.build.platforms.clone(),
    };
    assign_canonical_fingerprints(&mut units, &platforms)?;
    let scope = ImageBuildScope {
        project_id: document.config.project.id,
        config_hash: document.source_hash.clone(),
        environment,
    };
    let strategy = document.config.build.tag_strategy;
    match engine {
        Some(engine) => {
            localize(
                &document.config.project.name,
                &scope.environment,
                &mut units,
            )?;
            let platform = platforms.into_iter().next().unwrap_or_default();
            ImageBuildPlan::new_local(scope, source, engine, strategy, platform, units)
        }
        None => ImageBuildPlan::new(scope, source, publish, strategy, platforms, units),
    }
    .map_err(|message| MmError::Build { message })
}

/// Names each unit's image in the local registry, tagged by its inputs.
fn localize(
    project: &str,
    environment: &str,
    units: &mut [ImageBuildUnitPlan],
) -> Result<(), MmError> {
    let build = |message| MmError::Build { message };
    for unit in units {
        let planned: &ImageBuildUnitPlan = unit;
        let tags = planned
            .variants
            .iter()
            .map(|variant| local_image_tag(environment, planned, variant))
            .collect::<Result<Vec<_>, _>>()
            .map_err(build)?;
        for (variant, tag) in unit.variants.iter_mut().zip(tags) {
            variant.tags = vec![tag];
        }
        unit.image = local_image_repository(project, &unit.module).map_err(build)?;
    }
    Ok(())
}

/// `excluded` files are left out of the build context hash.
fn plan_module(
    document: &ConfigDocument,
    (module_name, module): (&str, &ModuleConfig),
    environment_variant: Option<&str>,
    base_tags: &[String],
    version: &str,
    excluded: &[PathBuf],
) -> Result<Option<ImageBuildUnitPlan>, MmError> {
    let configured = module.build.as_ref();
    if configured.is_some_and(|build| !build.enabled) {
        return Ok(None);
    }

    let context_relative = module.path.join(
        configured
            .and_then(|build| build.context.as_deref())
            .unwrap_or(Path::new(".")),
    );
    let dockerfile_relative = module.path.join(
        configured
            .and_then(|build| build.dockerfile.as_deref())
            .unwrap_or(Path::new("Dockerfile")),
    );
    let context = document.root.join(&context_relative);
    let dockerfile = document.root.join(&dockerfile_relative);
    if configured.is_none() && !dockerfile.is_file() {
        return Ok(None);
    }
    ensure_build_path(&document.root, &context, true, "build context")?;
    ensure_build_path(&document.root, &dockerfile, false, "Dockerfile")?;
    for reserved in [SNAPSHOT_DOCKERFILE, SNAPSHOT_DOCKERIGNORE] {
        if context.join(reserved).exists() {
            return Err(MmError::Security {
                message: format!(
                    "build context reserves the path {reserved} for immutable snapshots"
                ),
            });
        }
    }

    let image = configured
        .and_then(|build| build.image.clone())
        .ok_or_else(|| MmError::Build {
            message: format!("module '{module_name}' requires an authoritative build.image"),
        })?;
    let image = normalize_image_repository(&image)?;
    let context_hash = hash_context_excluding(&document.root, &context, &dockerfile, excluded)?;
    let mut args = configured.map_or_else(BTreeMap::new, |build| {
        build
            .args
            .iter()
            .map(|(name, value)| (name.clone(), value.to_string()))
            .collect()
    });
    args.insert("VERSION".to_owned(), version.to_owned());
    let secret_ids =
        configured.map_or_else(Vec::new, |build| build.secrets.keys().cloned().collect());
    let credential_secret_ids = configured.map_or_else(Vec::new, |build| {
        build
            .secrets
            .keys()
            .filter(|id| build.credential_only_secrets.contains(id))
            .cloned()
            .collect()
    });
    let extra_inputs = configured
        .map(|build| hash_extra_inputs(&document.root, &build.extra_inputs))
        .transpose()?
        .unwrap_or_default();
    let dockerfile_source = read_dockerfile(&dockerfile)?;
    let parsed = parse_dockerfile(&dockerfile_source, &dockerfile.display().to_string())?;
    let version_embedded = parsed.declared_args.contains("VERSION");
    let dependencies = plan_dependencies(configured, module_name, &dockerfile, &parsed)?;
    let reuse_policy = configured.map_or(BuildReusePolicy::Allowed, reuse_policy_for);
    let variants = plan_variants(configured, environment_variant, base_tags)?;

    let dockerfile_hash = blake3::hash(dockerfile_source.as_bytes())
        .to_hex()
        .to_string();
    Ok(Some(ImageBuildUnitPlan {
        module: module_name.to_owned(),
        image,
        context: context_relative,
        dockerfile: dockerfile_relative,
        context_hash,
        dockerfile_hash,
        args,
        secret_ids,
        credential_secret_ids,
        dependencies,
        upstream_node_ids: Vec::new(),
        extra_inputs,
        version_embedded,
        reuse_policy,
        input_fingerprint: "0".repeat(64),
        variants,
    }))
}

fn plan_dependencies(
    configured: Option<&ModuleBuildConfig>,
    module_name: &str,
    dockerfile: &Path,
    parsed: &ParsedDockerfile,
) -> Result<Vec<FirstPartyBuildEdge>, MmError> {
    let configured_dependencies = configured.map(|build| &build.depends_on);
    let mut dependencies = Vec::new();
    let mut used_dependency_args = BTreeSet::new();
    for base in &parsed.bases {
        let DockerfileBaseKind::Variable { name } = &base.kind else {
            continue;
        };
        let candidates = configured_dependencies
            .into_iter()
            .flat_map(|dependencies| dependencies.iter())
            .filter(|(_, arg)| *arg == name)
            .map(|(producer, _)| producer.as_str())
            .collect::<Vec<_>>();
        let producer = match candidates.as_slice() {
            [] => {
                return Err(MmError::Build {
                    message: format!(
                        "{}:{}: variable FROM reference '{}' requires a typed build.depends-on mapping",
                        dockerfile.display(),
                        base.source_line,
                        base.original_reference
                    ),
                });
            }
            [producer] => *producer,
            many => {
                return Err(MmError::Build {
                    message: format!(
                        "{}:{}: variable FROM reference '{}' maps to more than one first-party module ({}); assign a unique build argument to each producer",
                        dockerfile.display(),
                        base.source_line,
                        base.original_reference,
                        many.join(", ")
                    ),
                });
            }
        };
        if !parsed.declared_args.contains(name) {
            return Err(MmError::Build {
                message: format!(
                    "{}:{}: dependency FROM argument {name} must be declared by ARG",
                    dockerfile.display(),
                    base.source_line
                ),
            });
        }
        used_dependency_args.insert(name.clone());
        dependencies.push(FirstPartyBuildEdge {
            producer_module: producer.to_owned(),
            dependent_module: module_name.to_owned(),
            stage: base.stage.clone(),
            reference: BuildDependencyReference::BuildArgument {
                arg: name.clone(),
                instruction_index: base.instruction_index,
                source_line: base.source_line,
            },
        });
    }
    for (producer, arg) in configured_dependencies
        .into_iter()
        .flat_map(|dependencies| dependencies.iter())
    {
        if !parsed.declared_args.contains(arg) {
            return Err(MmError::Build {
                message: format!(
                    "module '{module_name}' Dockerfile must declare ARG {arg} for dependency '{producer}'"
                ),
            });
        }
        if !used_dependency_args.contains(arg) {
            return Err(MmError::Build {
                message: format!(
                    "module '{module_name}' dependency '{producer}' must be consumed by FROM ${{{arg}}}"
                ),
            });
        }
    }
    Ok(dependencies)
}

#[cfg(test)]
mod tests;
