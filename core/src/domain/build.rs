use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::domain::operation::OperationId;
use crate::domain::workspace::ProjectId;

pub const PLAN_SCHEMA: &str = "mm-image-build-plan-v1";

mod tags;
use tags::*;
pub use tags::{automatic_tags, build_version, suffixed_tags};
mod build_import;
pub use build_import::{BuildImportImage, BuildImportPlan, IMPORT_PLAN_SCHEMA};
mod build_receipt;
mod context_ignore;
pub use build_receipt::{BuildReceipt, BuildReceiptOrigin, PromotedReceipt, RECEIPT_SCHEMA};
pub use context_ignore::{ContextIgnore, IgnoreVerdict, InvalidIgnoreLine};
mod fingerprint;
use fingerprint::*;
mod rebuild;
pub use rebuild::{
    LocalBuildReview, LocalRebuild, LocalRebuildReason, held_images, stale_local_modules,
};
mod local;
pub use local::{
    ImageBuildFilter, LOCAL_IMAGE_REGISTRY, LocalImageEngine, is_local_image_pin,
    is_local_image_reference, local_image_repository, local_image_tag, local_platform,
    upstream_reference,
};
mod variants;
pub use fingerprint::{build_node_key, unit_execution_fingerprint, unit_input_fingerprint};
pub use fingerprint::{is_image_repository, is_sha256_digest};
use variants::validate_build_variants;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BuildTagStrategy {
    #[default]
    Auto,
    Immutable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BuildSource {
    pub commit: String,
    pub branch: Option<String>,
    pub tags_at_head: Vec<String>,
    pub repository_tags: Vec<String>,
    pub tags_complete: bool,
    pub default_branch: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBuildScope {
    pub project_id: ProjectId,
    pub config_hash: String,
    pub environment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImageBuildPlan {
    pub schema: String,
    pub project_id: ProjectId,
    pub config_hash: String,
    pub environment: String,
    pub source: BuildSource,
    pub publish: bool,
    /// The engine on this machine the images are loaded into, for a
    /// deployment here; absent for a published build or an OCI archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<LocalImageEngine>,
    pub tag_strategy: BuildTagStrategy,
    pub platforms: Vec<String>,
    pub units: Vec<ImageBuildUnitPlan>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BuildReusePolicy {
    #[default]
    Allowed,
    Forbidden,
    RevisionGated,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BuildDependencyReference {
    BuildArgument {
        arg: String,
        instruction_index: u32,
        source_line: u32,
    },
    Dockerfile {
        original_reference: String,
        instruction_index: u32,
        source_line: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct FirstPartyBuildEdge {
    pub producer_module: String,
    pub dependent_module: String,
    pub stage: Option<String>,
    pub reference: BuildDependencyReference,
}

impl FirstPartyBuildEdge {
    #[must_use]
    pub fn instruction_index(&self) -> u32 {
        match self.reference {
            BuildDependencyReference::BuildArgument {
                instruction_index, ..
            }
            | BuildDependencyReference::Dockerfile {
                instruction_index, ..
            } => instruction_index,
        }
    }

    #[must_use]
    pub fn source_line(&self) -> u32 {
        match self.reference {
            BuildDependencyReference::BuildArgument { source_line, .. }
            | BuildDependencyReference::Dockerfile { source_line, .. } => source_line,
        }
    }

    #[must_use]
    pub fn fingerprint_key(&self) -> String {
        format!(
            "{}:{}",
            self.instruction_index(),
            match &self.reference {
                BuildDependencyReference::BuildArgument { arg, .. } => format!("arg:{arg}"),
                BuildDependencyReference::Dockerfile {
                    original_reference, ..
                } => format!("from:{original_reference}"),
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImageBuildTrigger {
    pub path: PathBuf,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImageBuildUnitPlan {
    pub module: String,
    pub image: String,
    pub context: PathBuf,
    pub dockerfile: PathBuf,
    pub context_hash: String,
    pub dockerfile_hash: String,
    pub args: BTreeMap<String, String>,
    /// Every BuildKit secret the build mounts.
    pub secret_ids: Vec<String>,
    /// Secrets in `secret_ids` that only authenticate fetching inputs. They do
    /// not change the image, so they are left out of its fingerprint and never
    /// block reuse.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_secret_ids: Vec<String>,
    pub dependencies: Vec<FirstPartyBuildEdge>,
    pub upstream_node_ids: Vec<String>,
    pub extra_inputs: Vec<ImageBuildTrigger>,
    pub version_embedded: bool,
    pub reuse_policy: BuildReusePolicy,
    pub input_fingerprint: String,
    pub variants: Vec<ImageBuildVariantPlan>,
}

impl ImageBuildUnitPlan {
    /// Build secrets that can change the image: every mounted secret except
    /// the credential-only ones.
    pub fn output_secret_ids(&self) -> impl Iterator<Item = &String> {
        self.secret_ids
            .iter()
            .filter(|id| !self.credential_secret_ids.contains(id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildNodeKey<'a> {
    pub context_digest: &'a str,
    pub dockerfile_digest: &'a str,
    pub args: BTreeMap<&'a str, &'a str>,
    /// Secrets that can change the image; credential-only secrets are absent.
    pub secret_ids: Vec<&'a str>,
    pub extra_input_digests: Vec<&'a str>,
    pub version_embedded: bool,
    pub reuse_policy: BuildReusePolicy,
    pub platforms: &'a [String],
    pub dependencies: Vec<BuildNodeDependencyKey<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildNodeDependencyKey<'a> {
    pub upstream_node_id: &'a str,
    pub stage: Option<&'a str>,
    pub reference: &'a BuildDependencyReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImageBuildVariantPlan {
    /// The build target's name, and its Dockerfile stage unless `stage` is set.
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub tag_suffix: String,
    pub tags: Vec<String>,
    /// Build arguments of this target, merged over the unit's `args`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImageBuildResult {
    pub operation_id: OperationId,
    pub plan_hash: String,
    pub images: Vec<BuiltImage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ResolvedBuildDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BuiltImage {
    pub module: String,
    pub target: Option<String>,
    pub repository: String,
    pub tags: Vec<String>,
    pub digest: String,
    pub execution_fingerprint: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secret_revisions: BTreeMap<String, String>,
    pub archive: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedBuildDependency {
    pub edge: FirstPartyBuildEdge,
    pub dependent_target: Option<String>,
    pub digest: String,
    pub immutable_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReusableImage {
    pub repository: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReusableImageRecord<'a> {
    pub project_id: ProjectId,
    pub environment: &'a str,
    pub fingerprint: &'a str,
    pub module: &'a str,
    pub variant: Option<&'a str>,
    pub repository: &'a str,
    pub digest: &'a str,
    pub operation_id: OperationId,
}

#[derive(Serialize)]
struct CanonicalBuildPlan<'a> {
    schema: &'a str,
    project_id: ProjectId,
    config_hash: &'a str,
    environment: &'a str,
    source: &'a BuildSource,
    publish: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<LocalImageEngine>,
    tag_strategy: BuildTagStrategy,
    platforms: &'a [String],
    units: &'a [ImageBuildUnitPlan],
}

impl ImageBuildPlan {
    pub fn new(
        scope: ImageBuildScope,
        source: BuildSource,
        publish: bool,
        tag_strategy: BuildTagStrategy,
        platforms: Vec<String>,
        units: Vec<ImageBuildUnitPlan>,
    ) -> Result<Self, String> {
        Self::assemble(
            scope,
            source,
            (publish, None),
            tag_strategy,
            platforms,
            units,
        )
    }

    /// A build loaded into `engine` on this machine, for one platform.
    pub fn new_local(
        scope: ImageBuildScope,
        source: BuildSource,
        engine: LocalImageEngine,
        tag_strategy: BuildTagStrategy,
        platform: String,
        units: Vec<ImageBuildUnitPlan>,
    ) -> Result<Self, String> {
        Self::assemble(
            scope,
            source,
            (false, Some(engine)),
            tag_strategy,
            vec![platform],
            units,
        )
    }

    fn assemble(
        scope: ImageBuildScope,
        source: BuildSource,
        (publish, engine): (bool, Option<LocalImageEngine>),
        tag_strategy: BuildTagStrategy,
        platforms: Vec<String>,
        units: Vec<ImageBuildUnitPlan>,
    ) -> Result<Self, String> {
        let ImageBuildScope {
            project_id,
            config_hash,
            environment,
        } = scope;
        let mut plan = Self {
            schema: PLAN_SCHEMA.to_owned(),
            project_id,
            config_hash,
            environment,
            source,
            publish,
            engine,
            tag_strategy,
            platforms,
            units,
            plan_hash: String::new(),
        };
        plan.validate_without_hash()?;
        plan.plan_hash = plan.computed_hash()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.validate_without_hash()?;
        if self.plan_hash != self.computed_hash()? {
            return Err("image build plan hash does not match its contents".to_owned());
        }
        Ok(())
    }

    fn validate_without_hash(&self) -> Result<(), String> {
        if self.schema != PLAN_SCHEMA {
            return Err(format!(
                "unsupported image build plan schema '{}'",
                self.schema
            ));
        }
        if !is_git_object_id(&self.source.commit) {
            return Err(
                "image build source commit must be a full hexadecimal object id".to_owned(),
            );
        }
        if !is_blake3_hash(&self.config_hash) {
            return Err("image build configuration hash is invalid".to_owned());
        }
        if self.environment.is_empty() || self.environment.chars().any(char::is_control) {
            return Err("image build environment is invalid".to_owned());
        }
        if self.publish && self.source.dirty {
            return Err("publishing from a dirty worktree is forbidden".to_owned());
        }
        if self.publish && !self.source.tags_complete {
            return Err(
                "publishing requires a complete Git tag inventory; fetch full history and tags"
                    .to_owned(),
            );
        }
        if self.units.is_empty() {
            return Err("image build plan has no build units".to_owned());
        }
        if self.platforms.is_empty() {
            return Err("image build plan has no target platforms".to_owned());
        }
        let mut platforms = BTreeSet::new();
        for platform in &self.platforms {
            if !is_oci_platform(platform) || !platforms.insert(platform) {
                return Err(format!(
                    "image build platform '{platform}' is invalid or duplicated"
                ));
            }
        }
        self.validate_destination()?;

        let base_tags = automatic_tags(&self.source, self.tag_strategy)?;
        let version = build_version(&self.source)?;

        let mut modules = BTreeSet::new();
        let mut image_tags = BTreeSet::new();
        for (unit_index, unit) in self.units.iter().enumerate() {
            validate_build_unit_identity(unit, &mut modules, &version)?;
            self.validate_build_dependencies(unit, unit_index)?;
            self.validate_build_inputs(unit)?;
            validate_build_variants(
                unit,
                &|variant| self.expected_tags(unit, variant, &base_tags),
                &mut image_tags,
            )?;
        }
        Ok(())
    }

    fn validate_build_dependencies(
        &self,
        unit: &ImageBuildUnitPlan,
        unit_index: usize,
    ) -> Result<(), String> {
        let mut dependencies = BTreeSet::new();
        for dependency in &unit.dependencies {
            if !is_identifier(&dependency.producer_module)
                || dependency.producer_module == unit.module
                || dependency.dependent_module != unit.module
                || !dependencies.insert(dependency)
            {
                return Err(format!(
                    "module '{}' has an invalid build dependency",
                    unit.module
                ));
            }
            match &dependency.reference {
                BuildDependencyReference::BuildArgument {
                    arg, source_line, ..
                } => {
                    if !is_variable_name(arg)
                        || arg == "VERSION"
                        || unit.args.contains_key(arg)
                        || unit
                            .variants
                            .iter()
                            .any(|variant| variant.args.contains_key(arg))
                        || *source_line == 0
                    {
                        return Err(format!(
                            "module '{}' has an invalid dependency build argument",
                            unit.module
                        ));
                    }
                }
                BuildDependencyReference::Dockerfile {
                    original_reference,
                    source_line,
                    ..
                } => {
                    if original_reference.is_empty()
                        || original_reference.chars().any(char::is_control)
                        || *source_line == 0
                    {
                        return Err(format!(
                            "module '{}' has an invalid Dockerfile dependency reference",
                            unit.module
                        ));
                    }
                }
            }
            let Some(position) = self
                .units
                .iter()
                .position(|candidate| candidate.module == dependency.producer_module)
            else {
                return Err(format!(
                    "module '{}' depends on unknown module '{}'",
                    unit.module, dependency.producer_module
                ));
            };
            if position >= unit_index {
                return Err(format!(
                    "module '{}' depends on '{}' before it is built",
                    unit.module, dependency.producer_module
                ));
            }
        }
        Ok(())
    }

    fn validate_build_inputs(&self, unit: &ImageBuildUnitPlan) -> Result<(), String> {
        let mut extra_paths = BTreeSet::new();
        for trigger in &unit.extra_inputs {
            if !is_relative_build_path(&trigger.path)
                || !is_blake3_hash(&trigger.digest)
                || !extra_paths.insert(&trigger.path)
            {
                return Err(format!(
                    "module '{}' has an invalid extra rebuild input",
                    unit.module
                ));
            }
        }
        if !is_blake3_hash(&unit.input_fingerprint)
            || unit.input_fingerprint != unit_input_fingerprint(unit, &self.platforms)?
        {
            return Err(format!(
                "module '{}' has an invalid input fingerprint",
                unit.module
            ));
        }
        if unit.variants.is_empty() {
            return Err(format!("module '{}' has no build variants", unit.module));
        }
        Ok(())
    }

    fn computed_hash(&self) -> Result<String, String> {
        let canonical = CanonicalBuildPlan {
            schema: &self.schema,
            project_id: self.project_id,
            config_hash: &self.config_hash,
            environment: &self.environment,
            source: &self.source,
            publish: self.publish,
            engine: self.engine,
            tag_strategy: self.tag_strategy,
            platforms: &self.platforms,
            units: &self.units,
        };
        serde_json::to_vec(&canonical)
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
            .map_err(|error| format!("serializing image build plan: {error}"))
    }
}

fn validate_build_unit_identity<'a>(
    unit: &'a ImageBuildUnitPlan,
    modules: &mut BTreeSet<&'a String>,
    version: &str,
) -> Result<(), String> {
    if !is_identifier(&unit.module) || !modules.insert(&unit.module) {
        return Err(format!("module '{}' appears more than once", unit.module));
    }
    if !is_image_repository(&unit.image) {
        return Err(format!(
            "module '{}' has an invalid image repository",
            unit.module
        ));
    }
    if !is_relative_build_path(&unit.context) || !is_relative_build_path(&unit.dockerfile) {
        return Err(format!("module '{}' has an unsafe build path", unit.module));
    }
    if !is_blake3_hash(&unit.context_hash) {
        return Err(format!(
            "module '{}' has an invalid context hash",
            unit.module
        ));
    }
    if unit.args.get("VERSION").map(String::as_str) != Some(version) {
        return Err(format!(
            "module '{}' does not use the canonical source VERSION",
            unit.module
        ));
    }
    if unit
        .args
        .iter()
        .any(|(name, value)| !is_variable_name(name) || value.chars().any(char::is_control))
    {
        return Err(format!(
            "module '{}' has an invalid build argument",
            unit.module
        ));
    }
    let mut secret_ids = BTreeSet::new();
    if unit
        .secret_ids
        .iter()
        .any(|id| !is_identifier(id) || !secret_ids.insert(id))
    {
        return Err(format!(
            "module '{}' has an invalid build secret",
            unit.module
        ));
    }
    let mut credentials = BTreeSet::new();
    if unit
        .credential_secret_ids
        .iter()
        .any(|id| !secret_ids.contains(id) || !credentials.insert(id))
    {
        return Err(format!(
            "module '{}' has an invalid credential-only build secret",
            unit.module
        ));
    }
    Ok(())
}

impl ImageBuildResult {
    pub fn validate_against(&self, plan: &ImageBuildPlan) -> Result<(), String> {
        plan.validate()?;
        if self.plan_hash != plan.plan_hash {
            return Err("image build result does not match its reviewed plan".to_owned());
        }
        let expected_count = plan
            .units
            .iter()
            .map(|unit| unit.variants.len())
            .sum::<usize>();
        if self.images.len() != expected_count {
            return Err("image build result is incomplete".to_owned());
        }
        self.validate_images(plan)?;
        self.validate_dependency_evidence(plan)?;
        self.validate_execution_fingerprints(plan)?;
        Ok(())
    }

    fn validate_images(&self, plan: &ImageBuildPlan) -> Result<(), String> {
        let mut identities = BTreeSet::new();
        for image in &self.images {
            let unit = plan
                .units
                .iter()
                .find(|unit| unit.module == image.module)
                .ok_or_else(|| "image build result contains an unknown module".to_owned())?;
            let variant = unit
                .variants
                .iter()
                .find(|variant| variant.target == image.target)
                .ok_or_else(|| "image build result contains an unknown variant".to_owned())?;
            if !identities.insert((&image.module, &image.target)) {
                return Err("image build result contains a duplicate variant".to_owned());
            }
            let expected_tags = variant
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect::<Vec<_>>();
            if image.repository != unit.image || image.tags != expected_tags {
                return Err("image build result references unexpected tags".to_owned());
            }
            if !is_sha256_digest(&image.digest) {
                return Err("image build result contains an invalid OCI digest".to_owned());
            }
            if !is_blake3_hash(&image.execution_fingerprint) {
                return Err(
                    "image build result contains an invalid execution fingerprint".to_owned(),
                );
            }
            let revisions_are_valid = image.secret_revisions.iter().all(|(id, revision)| {
                unit.output_secret_ids().any(|output| output == id)
                    && !revision.is_empty()
                    && !revision.chars().any(char::is_control)
            });
            if !revisions_are_valid
                || unit.reuse_policy == BuildReusePolicy::RevisionGated
                    && image.secret_revisions.len() != unit.output_secret_ids().count()
                || unit.reuse_policy != BuildReusePolicy::RevisionGated
                    && !image.secret_revisions.is_empty()
            {
                return Err(
                    "image build result contains invalid secret revision evidence".to_owned(),
                );
            }
            if plan.writes_archive() != image.archive.is_some() {
                return Err("image build result output does not match its publish mode".to_owned());
            }
            if image
                .archive
                .as_ref()
                .is_some_and(|path| path.as_os_str().is_empty() || !path.is_absolute())
            {
                return Err("image build result contains an unsafe archive path".to_owned());
            }
        }
        Ok(())
    }

    fn validate_dependency_evidence(&self, plan: &ImageBuildPlan) -> Result<(), String> {
        let mut expected_dependencies = BTreeSet::new();
        for unit in &plan.units {
            for dependency in &unit.dependencies {
                for variant in &unit.variants {
                    expected_dependencies.insert((dependency, variant.target.as_deref()));
                }
            }
        }
        let mut observed_dependencies = BTreeSet::new();
        for dependency in &self.dependencies {
            let expected_reference = if plan.engine.is_some() {
                self.upstream_image(dependency)
                    .map(|upstream| upstream_reference(plan, upstream))
            } else {
                plan.units
                    .iter()
                    .find(|unit| unit.module == dependency.edge.producer_module)
                    .map(|unit| format!("{}@{}", unit.image, dependency.digest))
            };
            if !is_identifier(&dependency.edge.producer_module)
                || !is_identifier(&dependency.edge.dependent_module)
                || !is_sha256_digest(&dependency.digest)
                || expected_reference.as_deref() != Some(dependency.immutable_reference.as_str())
            {
                return Err("image build result contains an invalid dependency digest".to_owned());
            }
            if !observed_dependencies
                .insert((&dependency.edge, dependency.dependent_target.as_deref()))
            {
                return Err("image build result contains a duplicate dependency".to_owned());
            }
            if self
                .upstream_image(dependency)
                .is_none_or(|image| image.digest != dependency.digest)
            {
                return Err(
                    "image build result dependency digest does not match an upstream image"
                        .to_owned(),
                );
            }
        }
        if observed_dependencies != expected_dependencies {
            return Err(
                "image build result dependencies do not match the reviewed plan".to_owned(),
            );
        }
        Ok(())
    }

    fn validate_execution_fingerprints(&self, plan: &ImageBuildPlan) -> Result<(), String> {
        for image in &self.images {
            let unit = plan
                .units
                .iter()
                .find(|unit| unit.module == image.module)
                .ok_or_else(|| "image build result contains an unknown module".to_owned())?;
            let expected = unit_execution_fingerprint(
                &unit.input_fingerprint,
                &plan.environment,
                unit.variant(image.target.as_deref()),
                &upstream_digests(&self.dependencies, image),
                &image.secret_revisions,
            )?;
            if image.execution_fingerprint != expected {
                return Err(
                    "image build result execution fingerprint does not match its evidence"
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
}

/// The upstream digests in `image`'s execution fingerprint, keyed by edge.
pub(crate) fn upstream_digests(
    dependencies: &[ResolvedBuildDependency],
    image: &BuiltImage,
) -> BTreeMap<String, String> {
    dependencies
        .iter()
        .filter(|dependency| {
            dependency.edge.dependent_module == image.module
                && dependency.dependent_target == image.target
        })
        .map(|dependency| (dependency.edge.fingerprint_key(), dependency.digest.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
