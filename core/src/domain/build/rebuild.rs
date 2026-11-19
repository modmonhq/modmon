use super::*;

/// Why a deployment on this machine builds a module again first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LocalRebuildReason {
    NotBuilt,
    Context,
    Dockerfile,
    Version,
    Arguments,
    ExtraInputs,
    Base,
    Settings,
    ImageMissing,
}

impl LocalRebuildReason {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::NotBuilt => "no local build yet",
            Self::Context => "its build context changed",
            Self::Dockerfile => "its Dockerfile changed",
            Self::Version => "the VERSION it embeds changed",
            Self::Arguments => "its build arguments changed",
            Self::ExtraInputs => "an extra input changed",
            Self::Base => "an image it builds FROM changed",
            Self::Settings => "its build settings changed",
            Self::ImageMissing => "its image is no longer in the engine",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocalRebuild {
    pub module: String,
    pub reason: LocalRebuildReason,
}

/// The local build a deployment on this machine runs before it starts
/// anything: the reviewed build plan and the modules it builds again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocalBuildReview {
    pub plan_hash: String,
    pub engine: LocalImageEngine,
    pub modules: Vec<LocalRebuild>,
}

/// The modules of the local build `current` that `earlier`, the last local
/// build on the same engine, does not hold from the same inputs.
#[must_use]
pub fn stale_local_modules(
    current: &ImageBuildPlan,
    earlier: Option<&(ImageBuildPlan, ImageBuildResult)>,
) -> Vec<LocalRebuild> {
    let mut stale = Vec::<LocalRebuild>::new();
    for unit in &current.units {
        let reason = match earlier {
            None => Some(LocalRebuildReason::NotBuilt),
            Some((plan, result)) if !holds_unit(result, unit) => Some(
                match plan
                    .units
                    .iter()
                    .find(|before| before.module == unit.module)
                {
                    None => LocalRebuildReason::NotBuilt,
                    Some(before) => difference(before, unit, &stale),
                },
            ),
            Some(_) => None,
        };
        if let Some(reason) = reason {
            stale.push(LocalRebuild {
                module: unit.module.clone(),
                reason,
            });
        }
    }
    stale
}

/// The images of `result` that hold `unit`'s variants under the tags the
/// current plan gives them.
#[must_use]
pub fn held_images<'a>(
    result: &'a ImageBuildResult,
    unit: &ImageBuildUnitPlan,
) -> Vec<&'a BuiltImage> {
    unit.variants
        .iter()
        .filter_map(|variant| {
            let tags = variant
                .tags
                .iter()
                .map(|tag| format!("{}:{tag}", unit.image))
                .collect::<Vec<_>>();
            result.images.iter().find(|image| {
                image.module == unit.module && image.target == variant.target && image.tags == tags
            })
        })
        .collect()
}

fn holds_unit(result: &ImageBuildResult, unit: &ImageBuildUnitPlan) -> bool {
    held_images(result, unit).len() == unit.variants.len()
}

fn difference(
    before: &ImageBuildUnitPlan,
    now: &ImageBuildUnitPlan,
    stale: &[LocalRebuild],
) -> LocalRebuildReason {
    let without_version = |unit: &ImageBuildUnitPlan| {
        unit.args
            .iter()
            .filter(|(name, _)| *name != "VERSION")
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    let variant_args = |unit: &ImageBuildUnitPlan| {
        unit.variants
            .iter()
            .map(|variant| (variant.target.clone(), variant.args.clone()))
            .collect::<Vec<_>>()
    };
    if before.context_hash != now.context_hash {
        LocalRebuildReason::Context
    } else if before.dockerfile_hash != now.dockerfile_hash {
        LocalRebuildReason::Dockerfile
    } else if without_version(before) != without_version(now)
        || variant_args(before) != variant_args(now)
    {
        LocalRebuildReason::Arguments
    } else if before.args != now.args {
        LocalRebuildReason::Version
    } else if before.extra_inputs != now.extra_inputs {
        LocalRebuildReason::ExtraInputs
    } else if now.dependencies.iter().any(|edge| {
        stale
            .iter()
            .any(|rebuilt| rebuilt.module == edge.producer_module)
    }) || before.upstream_node_ids != now.upstream_node_ids
    {
        LocalRebuildReason::Base
    } else {
        LocalRebuildReason::Settings
    }
}
