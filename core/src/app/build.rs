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
    ImageBuildVariantPlan, PromotedReceipt, automatic_tags, build_version, is_image_repository,
    suffixed_tags, unit_input_fingerprint,
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
use scope::modules_in_scope;
mod source;
#[cfg(test)]
pub(crate) use source::hash_context;
use source::*;
pub(crate) use source::{normalize_image_repository, plan_variant, snapshot_build_context};

#[derive(Clone)]
pub struct BuildManager {
    documents: Arc<dyn ProjectDocumentStore>,
    source: Arc<dyn BuildSourceInspector>,
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
        Self { documents, source }
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
            return Err(MmError::Validation { errors });
        }

        let source = self.source.inspect(&document.root).await?;
        plan_from_source(document, environment, source, request, &[])
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
            return Err(MmError::Validation { errors });
        }
        // Plan from the receipt's source so VERSION and tags match the build.
        let current = plan_from_source(
            document,
            environment,
            receipt.plan.source.clone(),
            BuildRequest::from(true),
            added,
        )?;
        receipt
            .promote(&current)
            .map_err(|message| MmError::Build { message })
    }
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
    request: BuildRequest,
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
    let scope = (!request.all).then(|| modules_in_scope(&document, &environment));
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
    assign_canonical_fingerprints(&mut units, &document.config.build.platforms)?;

    ImageBuildPlan::new(
        ImageBuildScope {
            project_id: document.config.project.id,
            config_hash: document.source_hash,
            environment,
        },
        source,
        publish,
        document.config.build.tag_strategy,
        document.config.build.platforms,
        units,
    )
    .map_err(|message| MmError::Build { message })
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
