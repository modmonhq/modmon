use super::*;

pub const RECEIPT_SCHEMA: &str = "mm-build-receipt-v1";

/// A published build another machine can deploy: the reviewed build plan,
/// the digests it pushed, and a hash over both. The hash detects corruption;
/// it is not a signature, so a receipt is evidence only from whoever hands it
/// over. Deploying one still checks it against the deployed commit's config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildReceipt {
    pub schema: String,
    pub plan: ImageBuildPlan,
    pub result: ImageBuildResult,
    pub receipt_hash: String,
}

impl BuildReceipt {
    pub fn new(plan: ImageBuildPlan, result: ImageBuildResult) -> Result<Self, String> {
        let mut receipt = Self {
            schema: RECEIPT_SCHEMA.to_owned(),
            plan,
            result,
            receipt_hash: String::new(),
        };
        receipt.receipt_hash = receipt.computed_hash()?;
        receipt.validate()?;
        Ok(receipt)
    }

    /// A receipt holds one complete, published build of a clean commit.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != RECEIPT_SCHEMA {
            return Err(format!(
                "unsupported build receipt schema '{}'",
                self.schema
            ));
        }
        if !self.plan.publish {
            return Err(
                "a build receipt needs a published build (`mm build --publish`)".to_owned(),
            );
        }
        if self.plan.source.dirty {
            return Err(
                "a build receipt needs a build of a clean checkout, not uncommitted changes"
                    .to_owned(),
            );
        }
        self.result.validate_against(&self.plan)?;
        if self.receipt_hash != self.computed_hash()? {
            return Err("build receipt hash does not match its contents".to_owned());
        }
        Ok(())
    }

    /// The commit the images were built from.
    #[must_use]
    pub fn source_commit(&self) -> &str {
        &self.plan.source.commit
    }

    fn computed_hash(&self) -> Result<String, String> {
        serde_json::to_vec(&(&self.schema, &self.plan, &self.result))
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
            .map_err(|error| format!("serializing build receipt: {error}"))
    }
}

/// Where the images of a deployment come from when it deploys a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildReceiptOrigin {
    pub receipt_hash: String,
    /// The environment the receipt was built for.
    pub environment: String,
    pub commit: String,
}

/// A receipt checked against the environment it is deployed to or imported
/// into. `receipt` is bound to that environment: the original when it was
/// built there, otherwise its promotion. `origin` names the original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotedReceipt {
    pub receipt: BuildReceipt,
    pub origin: BuildReceiptOrigin,
}

impl BuildReceipt {
    #[must_use]
    pub fn origin(&self) -> BuildReceiptOrigin {
        BuildReceiptOrigin {
            receipt_hash: self.receipt_hash.clone(),
            environment: self.plan.environment.clone(),
            commit: self.plan.source.commit.clone(),
        }
    }

    /// The receipt for its own environment, unchanged.
    pub fn unpromoted(&self) -> Result<PromotedReceipt, String> {
        self.validate()?;
        Ok(PromotedReceipt {
            receipt: self.clone(),
            origin: self.origin(),
        })
    }

    /// The same build bound to another environment. `current` is the plan
    /// that environment's configuration makes for the receipt's commit; the
    /// receipt must build each of its units from the same inputs, and no
    /// such unit may mount a build secret whose value can differ between
    /// environments. Units the environment does not build are left out.
    pub fn promote(&self, current: &ImageBuildPlan) -> Result<PromotedReceipt, String> {
        self.validate()?;
        self.check_promotion(current)?;
        Ok(PromotedReceipt {
            receipt: self
                .narrowed_to(current)?
                .rebind(&current.environment, &current.config_hash)?,
            origin: self.origin(),
        })
    }

    /// The same build with only the modules `current` builds.
    fn narrowed_to(&self, current: &ImageBuildPlan) -> Result<Self, String> {
        let keep = |module: &str| current.units.iter().any(|unit| unit.module == module);
        if self.plan.units.iter().all(|unit| keep(&unit.module)) {
            return Ok(self.clone());
        }
        let plan = ImageBuildPlan::new(
            ImageBuildScope {
                project_id: self.plan.project_id,
                config_hash: self.plan.config_hash.clone(),
                environment: self.plan.environment.clone(),
            },
            self.plan.source.clone(),
            self.plan.publish,
            self.plan.tag_strategy,
            self.plan.platforms.clone(),
            self.plan
                .units
                .iter()
                .filter(|unit| keep(&unit.module))
                .cloned()
                .collect(),
        )?;
        let mut result = self.result.clone();
        result.plan_hash.clone_from(&plan.plan_hash);
        result.images.retain(|image| keep(&image.module));
        result
            .dependencies
            .retain(|dependency| keep(&dependency.edge.dependent_module));
        Self::new(plan, result)
    }

    /// Why the receipt cannot stand in for `current`, naming the first module
    /// that differs.
    pub fn check_promotion(&self, current: &ImageBuildPlan) -> Result<(), String> {
        let built = &self.plan;
        let target = &current.environment;
        if built.project_id != current.project_id {
            return Err("the build receipt is for another project".to_owned());
        }
        if built.source.commit != current.source.commit {
            return Err(format!(
                "the build receipt is a build of commit {}, not {}",
                built.source.commit, current.source.commit
            ));
        }
        if built.platforms != current.platforms {
            return Err(format!(
                "the build receipt was built for platforms {} but environment '{target}' builds for {}",
                built.platforms.join(", "),
                current.platforms.join(", ")
            ));
        }
        let refuse = |module: &str, reason: String| {
            Err(format!(
                "the build receipt from environment '{}' cannot be deployed to '{target}': module '{module}' {reason}",
                built.environment
            ))
        };
        for unit in &current.units {
            let Some(receipt_unit) = built.units.iter().find(|built| built.module == unit.module)
            else {
                return refuse(&unit.module, "is not in the build receipt".to_owned());
            };
            if let Some(difference) = unit_difference(receipt_unit, unit) {
                return refuse(
                    &unit.module,
                    format!(
                        "differs: {difference}; build it for '{target}' with `mm -e {target} build --publish`"
                    ),
                );
            }
            if receipt_unit.reuse_policy != BuildReusePolicy::Allowed {
                return refuse(
                    &unit.module,
                    format!(
                        "mounts build secrets whose values can differ between environments ({}); build it for '{target}', or list secrets that only fetch inputs under credential-only-secrets",
                        receipt_unit
                            .output_secret_ids()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
        Ok(())
    }

    /// The same build recorded for `environment`: execution fingerprints
    /// include the environment, so they are recomputed.
    pub fn rebind(&self, environment: &str, config_hash: &str) -> Result<Self, String> {
        if environment == self.plan.environment && config_hash == self.plan.config_hash {
            return Ok(self.clone());
        }
        let plan = ImageBuildPlan::new(
            ImageBuildScope {
                project_id: self.plan.project_id,
                config_hash: config_hash.to_owned(),
                environment: environment.to_owned(),
            },
            self.plan.source.clone(),
            self.plan.publish,
            self.plan.tag_strategy,
            self.plan.platforms.clone(),
            self.plan.units.clone(),
        )?;
        let mut result = self.result.clone();
        result.plan_hash.clone_from(&plan.plan_hash);
        for image in &mut result.images {
            let unit = plan
                .units
                .iter()
                .find(|unit| unit.module == image.module)
                .ok_or("image build result contains an unknown module")?;
            image.execution_fingerprint = unit_execution_fingerprint(
                &unit.input_fingerprint,
                environment,
                unit.variant(image.target.as_deref()),
                &upstream_digests(&result.dependencies, image),
                &image.secret_revisions,
            )?;
        }
        Self::new(plan, result)
    }
}

/// The first build input of `built` that differs from `current`.
fn unit_difference(built: &ImageBuildUnitPlan, current: &ImageBuildUnitPlan) -> Option<String> {
    if built.image != current.image {
        return Some(format!(
            "image repository {} in the receipt, {} here",
            built.image, current.image
        ));
    }
    if built.context != current.context || built.dockerfile != current.dockerfile {
        return Some("build context or Dockerfile path".to_owned());
    }
    if built.dockerfile_hash != current.dockerfile_hash {
        return Some("Dockerfile contents".to_owned());
    }
    if built.context_hash != current.context_hash {
        return Some("build context contents".to_owned());
    }
    let compared = |unit: &ImageBuildUnitPlan| -> BTreeMap<String, String> {
        unit.args
            .iter()
            .filter(|(name, _)| unit.version_embedded || *name != "VERSION")
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    };
    let (built_args, current_args) = (compared(built), compared(current));
    if let Some(name) = built_args
        .keys()
        .chain(current_args.keys())
        .find(|name| built_args.get(*name) != current_args.get(*name))
    {
        return Some(format!("build argument {name}"));
    }
    if !built.output_secret_ids().eq(current.output_secret_ids())
        || built.reuse_policy != current.reuse_policy
    {
        return Some("build secrets".to_owned());
    }
    if built.extra_inputs != current.extra_inputs {
        return Some("extra inputs".to_owned());
    }
    if built.version_embedded != current.version_embedded {
        return Some("use of the VERSION argument".to_owned());
    }
    if let Some(difference) = variant_difference(built, current) {
        return Some(difference);
    }
    if built.dependencies != current.dependencies {
        return Some("build dependencies".to_owned());
    }
    (built.input_fingerprint != current.input_fingerprint)
        .then(|| "build inputs of a module it depends on".to_owned())
}

fn variant_difference(built: &ImageBuildUnitPlan, current: &ImageBuildUnitPlan) -> Option<String> {
    let names = |unit: &ImageBuildUnitPlan| {
        unit.variants
            .iter()
            .map(ImageBuildVariantPlan::name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    for variant in &current.variants {
        let name = variant.name();
        let Some(built_variant) = built.variant(variant.target.as_deref()) else {
            return Some(format!(
                "build variant {} here, {} in the receipt",
                names(current),
                names(built)
            ));
        };
        if built_variant.dockerfile_stage() != variant.dockerfile_stage() {
            return Some(format!("Dockerfile target of build variant {name}"));
        }
        if built_variant.tag_suffix != variant.tag_suffix {
            return Some(format!("tag suffix of build variant {name}"));
        }
        let (built_args, args) = (
            built.variant_args(built_variant),
            current.variant_args(variant),
        );
        if let Some(arg) = built_args
            .keys()
            .chain(args.keys())
            .filter(|arg| current.version_embedded || *arg != "VERSION")
            .find(|arg| built_args.get(*arg) != args.get(*arg))
        {
            return Some(format!("build argument {arg} of build variant {name}"));
        }
    }
    None
}
