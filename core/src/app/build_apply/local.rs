use super::*;
use crate::domain::build::{
    ImageBuildFilter, LocalBuildReview, LocalRebuild, LocalRebuildReason, held_images,
    stale_local_modules,
};

impl BuildService {
    /// The last build of this environment loaded into the plan's engine,
    /// whose unchanged images a local build keeps.
    pub(super) fn earlier_local_build(
        &self,
        project_root: &std::path::Path,
        plan: &ImageBuildPlan,
    ) -> Result<Option<ImageBuildResult>, MmError> {
        let Some(engine) = plan.engine else {
            return Ok(None);
        };
        Ok(self
            .ports
            .state
            .latest_image_build(
                plan.project_id,
                project_root,
                &plan.environment,
                ImageBuildFilter::LoadedInto(engine),
            )?
            .map(|(_, result)| result))
    }

    /// The earlier local image of this variant, when it was built from the
    /// same inputs and the engine still has it under its tag.
    pub(super) async fn reuse_local(
        &self,
        plan: &ImageBuildPlan,
        unit: &ImageBuildUnitPlan,
        variant: &ImageBuildVariantPlan,
        fingerprint: &str,
        earlier: Option<&ImageBuildResult>,
    ) -> Result<Option<BuiltImage>, MmError> {
        let (Some(engine), Some(earlier)) = (plan.engine, earlier) else {
            return Ok(None);
        };
        if unit.reuse_policy == BuildReusePolicy::Forbidden {
            return Ok(None);
        }
        let tags = variant
            .tags
            .iter()
            .map(|tag| format!("{}:{tag}", unit.image))
            .collect::<Vec<_>>();
        let Some(image) = earlier.images.iter().find(|image| {
            image.module == unit.module
                && image.target == variant.target
                && image.execution_fingerprint == fingerprint
                && image.repository == unit.image
                && image.tags == tags
        }) else {
            return Ok(None);
        };
        let Some(reference) = tags.first() else {
            return Ok(None);
        };
        let current = self.ports.builder.local_image_id(engine, reference).await?;
        Ok(
            (current.as_deref() == Some(image.digest.as_str())).then(|| BuiltImage {
                secret_revisions: BTreeMap::new(),
                archive: None,
                reused: true,
                ..image.clone()
            }),
        )
    }
}

/// The local build a deployment on this machine starts with: the current
/// plan of the environment's images, and the modules whose last local
/// build is out of date or missing.
#[derive(Clone)]
pub struct LocalBuild {
    plan: ImageBuildPlan,
    stale: Vec<LocalRebuild>,
    root: std::path::PathBuf,
    builds: BuildService,
}

impl std::fmt::Debug for LocalBuild {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalBuild")
            .field("plan_hash", &self.plan.plan_hash)
            .field("stale", &self.stale)
            .finish_non_exhaustive()
    }
}

impl LocalBuild {
    #[must_use]
    pub fn plan(&self) -> &ImageBuildPlan {
        &self.plan
    }

    /// What the deployment builds first; `None` when nothing is stale.
    #[must_use]
    pub fn review(&self) -> Option<LocalBuildReview> {
        let engine = self.plan.engine?;
        (!self.stale.is_empty()).then(|| LocalBuildReview {
            plan_hash: self.plan.plan_hash.clone(),
            engine,
            modules: self.stale.clone(),
        })
    }

    /// Runs the reviewed build, unless this machine already recorded it.
    pub async fn run(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<Option<ImageBuildResult>, MmError> {
        let Some(engine) = self.plan.engine else {
            return Ok(None);
        };
        let recorded = self.builds.ports.state.latest_image_build(
            self.plan.project_id,
            &self.root,
            &self.plan.environment,
            ImageBuildFilter::LoadedInto(engine),
        )?;
        if self.stale.is_empty()
            || recorded.is_some_and(|(plan, _)| plan.plan_hash == self.plan.plan_hash)
        {
            return Ok(None);
        }
        self.builds
            .apply_reviewed(selection, BuildRequest::from(false), &self.plan.plan_hash)
            .await
            .map(Some)
    }
}

impl BuildService {
    /// The local build of the selected environment and what in it is stale,
    /// or `None` when the environment is not on this machine or runs no
    /// module that mm builds.
    pub async fn local_build(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<Option<LocalBuild>, MmError> {
        let Some(engine) = self.planner.local_engine() else {
            return Ok(None);
        };
        if !self.planner.builds_modules(selection)? {
            return Ok(None);
        }
        let plan = self
            .planner
            .plan(selection, BuildRequest::from(false))
            .await?;
        let root = self.ports.documents.load_from(&selection.start)?.root;
        let earlier = self.ports.state.latest_image_build(
            plan.project_id,
            &root,
            &plan.environment,
            ImageBuildFilter::LoadedInto(engine),
        )?;
        let mut stale = stale_local_modules(&plan, earlier.as_ref());
        if let Some((_, result)) = &earlier {
            for unit in &plan.units {
                if stale.iter().any(|rebuilt| rebuilt.module == unit.module) {
                    continue;
                }
                for image in held_images(result, unit) {
                    let Some(reference) = image.tags.first() else {
                        continue;
                    };
                    let current = self.ports.builder.local_image_id(engine, reference).await?;
                    if current.as_deref() != Some(image.digest.as_str()) {
                        stale.push(LocalRebuild {
                            module: unit.module.clone(),
                            reason: LocalRebuildReason::ImageMissing,
                        });
                        break;
                    }
                }
            }
        }
        Ok(Some(LocalBuild {
            plan,
            stale,
            root,
            builds: self.clone(),
        }))
    }
}
