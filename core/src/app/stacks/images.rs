use crate::domain::build::{BuildDependencyReference, ImageBuildUnitPlan};
use crate::domain::config::ModuleConfig;

use super::*;

/// Deployment consumes an already-published artifact, not a new build of the
/// worktree. Compare the requested build settings with its reviewed evidence;
/// runtime configuration and source/tag changes do not invalidate that artifact.
pub(super) fn compatible_build_modules(
    document: &ConfigDocument,
    plan: &ImageBuildPlan,
) -> Result<BTreeSet<String>, MmError> {
    let mut compatible = BTreeSet::new();
    let configured = &document.config.build.platforms;
    let platforms_match = if plan.engine.is_some() {
        plan.platforms == [crate::domain::build::local_platform(configured)]
    } else {
        configured.iter().collect::<BTreeSet<_>>() == plan.platforms.iter().collect()
    };
    if !platforms_match {
        return Ok(compatible);
    }
    // Validated plans are topologically ordered, including base-image modules
    // with no services. A changed base definition also invalidates dependents.
    for unit in &plan.units {
        if let Some(module) = document.config.modules.get(&unit.module)
            && repository_matches(document, plan, module, unit)?
            && matches_build_configuration(module, unit)?
            && unit
                .dependencies
                .iter()
                .all(|edge| compatible.contains(&edge.producer_module))
        {
            compatible.insert(unit.module.clone());
        }
    }
    Ok(compatible)
}

/// A local build names its images after the project and module; a
/// published one after `build.image`.
fn repository_matches(
    document: &ConfigDocument,
    plan: &ImageBuildPlan,
    module: &ModuleConfig,
    unit: &ImageBuildUnitPlan,
) -> Result<bool, MmError> {
    if plan.engine.is_some() {
        return Ok(crate::domain::build::local_image_repository(
            &document.config.project.name,
            &unit.module,
        )
        .is_ok_and(|repository| repository == unit.image));
    }
    let Some(image) = module
        .build
        .as_ref()
        .and_then(|build| build.image.as_deref())
    else {
        return Ok(false);
    };
    Ok(crate::app::build::normalize_image_repository(image)? == unit.image)
}

fn matches_build_configuration(
    module: &ModuleConfig,
    unit: &ImageBuildUnitPlan,
) -> Result<bool, MmError> {
    let Some(build) = module.build.as_ref().filter(|build| build.enabled) else {
        return Ok(false);
    };
    let context = module
        .path
        .join(build.context.as_deref().unwrap_or(Path::new(".")));
    let dockerfile = module.path.join(
        build
            .dockerfile
            .as_deref()
            .unwrap_or(Path::new("Dockerfile")),
    );
    // VERSION is generated from the build's source revision, not runtime config.
    let args = build
        .args
        .iter()
        .filter(|(key, _)| *key != "VERSION")
        .map(|(key, value)| (key.clone(), value.to_string()))
        .collect::<BTreeMap<_, _>>();
    let built_args = unit
        .args
        .iter()
        .filter(|(key, _)| *key != "VERSION")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let dependencies = unit
        .dependencies
        .iter()
        .filter_map(|edge| match &edge.reference {
            BuildDependencyReference::BuildArgument { arg, .. } => {
                Some((&edge.producer_module, arg))
            }
            BuildDependencyReference::Dockerfile { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    let variants_match = unit.variants.iter().all(|variant| {
        let Some(name) = variant.target.as_deref() else {
            return build.targets.is_empty();
        };
        build.targets.get(name).is_some_and(|target| {
            crate::app::build::plan_variant(name, target, &[]).is_ok_and(|configured| {
                configured.stage == variant.stage
                    && configured.tag_suffix == variant.tag_suffix
                    && configured.args == variant.args
            })
        })
    });
    Ok(context == unit.context
        && dockerfile == unit.dockerfile
        && args == built_args
        && build.secrets.keys().eq(unit.secret_ids.iter())
        && build.extra_inputs.iter().collect::<BTreeSet<_>>()
            == unit.extra_inputs.iter().map(|input| &input.path).collect()
        && build.depends_on.iter().collect::<BTreeSet<_>>() == dependencies
        && variants_match)
}
