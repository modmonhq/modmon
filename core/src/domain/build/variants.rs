use super::*;

impl ImageBuildVariantPlan {
    #[must_use]
    pub fn dockerfile_stage(&self) -> Option<&str> {
        self.stage.as_deref().or(self.target.as_deref())
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.target.as_deref().unwrap_or("default")
    }
}

impl ImageBuildUnitPlan {
    #[must_use]
    pub fn variant(&self, target: Option<&str>) -> Option<&ImageBuildVariantPlan> {
        self.variants
            .iter()
            .find(|variant| variant.target.as_deref() == target)
    }

    /// The arguments a variant is built with: the unit's, then the variant's.
    #[must_use]
    pub fn variant_args(&self, variant: &ImageBuildVariantPlan) -> BTreeMap<String, String> {
        let mut args = self.args.clone();
        args.extend(
            variant
                .args
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        args
    }
}

pub(super) fn validate_build_variants(
    unit: &ImageBuildUnitPlan,
    expected_tags: &dyn Fn(&ImageBuildVariantPlan) -> Result<Vec<String>, String>,
    image_tags: &mut BTreeSet<(String, String)>,
) -> Result<(), String> {
    let mut targets = BTreeSet::new();
    let mut tags = BTreeSet::new();
    for variant in &unit.variants {
        validate_variant_identity(unit, variant)?;
        if !targets.insert(&variant.target) {
            return Err(format!(
                "module '{}' contains a duplicate build variant",
                unit.module
            ));
        }
        if variant.tags.is_empty() {
            return Err(format!(
                "module '{}' contains a build variant with no tags",
                unit.module
            ));
        }
        if variant.tags != expected_tags(variant)? {
            return Err(format!(
                "module '{}' build tags do not match its source policy",
                unit.module
            ));
        }
        for tag in &variant.tags {
            if !is_oci_tag(tag) {
                return Err(format!("'{tag}' is not a valid OCI tag"));
            }
            if !tags.insert(tag) {
                return Err(format!(
                    "module '{}' assigns OCI tag '{tag}' more than once",
                    unit.module
                ));
            }
            if !image_tags.insert((unit.image.clone(), tag.clone())) {
                return Err(format!(
                    "OCI image '{}:{tag}' is assigned by more than one build unit",
                    unit.image
                ));
            }
        }
    }
    Ok(())
}

fn validate_variant_identity(
    unit: &ImageBuildUnitPlan,
    variant: &ImageBuildVariantPlan,
) -> Result<(), String> {
    let stage_is_valid = match (&variant.target, &variant.stage) {
        (_, None) => true,
        (Some(target), Some(stage)) => stage != target && is_identifier(stage),
        (None, Some(_)) => false,
    };
    if variant
        .target
        .as_deref()
        .is_some_and(|target| !is_identifier(target))
        || !stage_is_valid
        || !variant.tag_suffix.is_empty() && !is_identifier(&variant.tag_suffix)
    {
        return Err(format!(
            "module '{}' has an invalid build variant",
            unit.module
        ));
    }
    if variant.args.iter().any(|(name, value)| {
        !is_variable_name(name) || name == "VERSION" || value.chars().any(char::is_control)
    }) {
        return Err(format!(
            "module '{}' build variant '{}' has an invalid build argument",
            unit.module,
            variant.name()
        ));
    }
    Ok(())
}
