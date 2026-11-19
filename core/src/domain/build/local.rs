use super::*;
use crate::domain::runtime::RuntimeKind;

/// The registry of every image a local build loads. `.invalid` never
/// resolves (RFC 2606), so such an image cannot be pulled or pushed.
pub const LOCAL_IMAGE_REGISTRY: &str = "mm.invalid";

/// The container engine on this machine that a local build loads into.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum LocalImageEngine {
    Docker,
    Podman,
}

impl LocalImageEngine {
    #[must_use]
    pub const fn for_runtime(runtime: RuntimeKind) -> Self {
        match runtime {
            RuntimeKind::DockerCompose => Self::Docker,
            RuntimeKind::PodmanCompose => Self::Podman,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Podman => "Podman",
        }
    }
}

/// Which of a checkout's builds a lookup returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageBuildFilter {
    /// Builds pushed to a registry: what receipts hold and hosts run.
    Published,
    /// Builds loaded into this engine.
    LoadedInto(LocalImageEngine),
    /// What a deployment on this machine runs: a published build, or one
    /// loaded into the engine (into any engine when `None`).
    Deployable(Option<LocalImageEngine>),
}

impl ImageBuildFilter {
    #[must_use]
    pub fn accepts(self, plan: &ImageBuildPlan) -> bool {
        match (self, plan.engine) {
            (Self::Published | Self::Deployable(_), None) => plan.publish,
            (Self::Published, Some(_)) => false,
            (Self::LoadedInto(wanted), Some(engine))
            | (Self::Deployable(Some(wanted)), Some(engine)) => wanted == engine,
            (Self::Deployable(None), Some(_)) => true,
            (Self::LoadedInto(_), None) => false,
        }
    }
}

/// `mm.invalid/<project>/<module>`, in the characters a repository allows.
pub fn local_image_repository(project: &str, module: &str) -> Result<String, String> {
    let component = |value: &str, kind: &str| {
        let normalized = repository_component(value);
        if normalized.is_empty() {
            Err(format!(
                "{kind} name '{value}' has no letter or digit to name its local image"
            ))
        } else {
            Ok(normalized)
        }
    };
    Ok(format!(
        "{LOCAL_IMAGE_REGISTRY}/{}/{}",
        component(project, "project")?,
        component(module, "module")?
    ))
}

fn repository_component(value: &str) -> String {
    let mut component = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !component.is_empty() {
                component.push('-');
            }
            component.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    component.truncate(64);
    component.trim_end_matches('-').to_owned()
}

#[derive(Serialize)]
struct LocalTagIdentity<'a> {
    module: &'a str,
    input_fingerprint: &'a str,
    environment: &'a str,
    variant: Option<&'a str>,
    stage: Option<&'a str>,
    args: &'a BTreeMap<String, String>,
}

/// The tag of a local image: the environment and a hash of the module's
/// build inputs and variant, so the same inputs give the same reference.
pub fn local_image_tag(
    environment: &str,
    unit: &ImageBuildUnitPlan,
    variant: &ImageBuildVariantPlan,
) -> Result<String, String> {
    let identity = LocalTagIdentity {
        module: &unit.module,
        input_fingerprint: &unit.input_fingerprint,
        environment,
        variant: variant.target.as_deref(),
        stage: variant.stage.as_deref(),
        args: &variant.args,
    };
    let bytes = serde_json::to_vec(&identity)
        .map_err(|error| format!("serializing a local image identity: {error}"))?;
    let hash = blake3::hash(&bytes).to_hex();
    let prefix = repository_component(environment);
    let tag = format!("{prefix}-{}", &hash[..20]);
    Ok(if is_oci_tag(&tag) {
        tag
    } else {
        hash[..20].to_owned()
    })
}

#[must_use]
pub fn is_local_image_reference(reference: &str) -> bool {
    reference
        .strip_prefix(LOCAL_IMAGE_REGISTRY)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// A local image named by its content-addressed tag, which pins it as a
/// digest pins a published one.
#[must_use]
pub fn is_local_image_pin(reference: &str) -> bool {
    is_local_image_reference(reference)
        && reference
            .rsplit_once(':')
            .is_some_and(|(repository, tag)| is_image_repository(repository) && is_oci_tag(tag))
}

/// The one platform a local build builds: this machine's, when the
/// configured platforms list it, else the first configured.
#[must_use]
pub fn local_platform(configured: &[String]) -> String {
    let native = native_platform();
    configured
        .iter()
        .find(|platform| {
            platform.as_str() == native
                || platform
                    .strip_prefix(native.as_str())
                    .is_some_and(|variant| variant.starts_with('/'))
        })
        .or_else(|| configured.first())
        .cloned()
        .unwrap_or(native)
}

fn native_platform() -> String {
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "powerpc64" => "ppc64le",
        other => other,
    };
    format!("linux/{architecture}")
}

impl ImageBuildPlan {
    /// A build that writes an OCI archive, which no deployment runs.
    #[must_use]
    pub fn writes_archive(&self) -> bool {
        !self.publish && self.engine.is_none()
    }

    pub(super) fn expected_tags(
        &self,
        unit: &ImageBuildUnitPlan,
        variant: &ImageBuildVariantPlan,
        base_tags: &[String],
    ) -> Result<Vec<String>, String> {
        if self.engine.is_some() {
            Ok(vec![local_image_tag(&self.environment, unit, variant)?])
        } else {
            suffixed_tags(base_tags, &variant.tag_suffix)
        }
    }

    pub(super) fn validate_destination(&self) -> Result<(), String> {
        let local = self.engine.is_some();
        if local && self.publish {
            return Err("a published build cannot also load into a local engine".to_owned());
        }
        if local && self.platforms.len() != 1 {
            return Err("a local build builds exactly one platform".to_owned());
        }
        if let Some(unit) = self
            .units
            .iter()
            .find(|unit| is_local_image_reference(&unit.image) != local)
        {
            return Err(if local {
                format!(
                    "module '{}' of a local build must use a {LOCAL_IMAGE_REGISTRY} repository",
                    unit.module
                )
            } else {
                format!(
                    "module '{}' uses the {LOCAL_IMAGE_REGISTRY} registry, which only local builds use; choose another build.image",
                    unit.module
                )
            });
        }
        Ok(())
    }
}

impl ImageBuildResult {
    /// The upstream image `dependency` was built from, matching its
    /// dependent's variant.
    #[must_use]
    pub fn upstream_image(&self, dependency: &ResolvedBuildDependency) -> Option<&BuiltImage> {
        let upstreams = self
            .images
            .iter()
            .filter(|image| image.module == dependency.edge.producer_module)
            .collect::<Vec<_>>();
        match upstreams.as_slice() {
            [image] => Some(*image),
            many => many
                .iter()
                .copied()
                .find(|image| image.target == dependency.dependent_target)
                .or_else(|| many.iter().copied().find(|image| image.target.is_none())),
        }
    }
}

/// How a dependent build names its upstream image: by digest in a
/// registry, or by its tag in the local engine.
#[must_use]
pub fn upstream_reference(plan: &ImageBuildPlan, upstream: &BuiltImage) -> String {
    if plan.engine.is_some() {
        upstream.tags.first().cloned().unwrap_or_default()
    } else {
        format!("{}@{}", upstream.repository, upstream.digest)
    }
}
