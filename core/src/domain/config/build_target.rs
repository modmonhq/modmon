use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::de::value::MapAccessDeserializer;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{EnvScalar, ModuleBuildConfig};

/// A build target under `modules.<m>.build.targets`, which an environment's
/// `build-variant` selects: the tag suffix alone, or a mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum BuildTarget {
    Suffix(String),
    Variant(BuildVariantConfig),
}

impl<'de> Deserialize<'de> for BuildTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(BuildTargetVisitor)
    }
}

struct BuildTargetVisitor;

impl<'de> Visitor<'de> for BuildTargetVisitor {
    type Value = BuildTarget;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a tag suffix, or a mapping with target, suffix and args")
    }

    fn visit_str<E>(self, suffix: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BuildTarget::Suffix(suffix.to_owned()))
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        BuildVariantConfig::deserialize(MapAccessDeserializer::new(map)).map(BuildTarget::Variant)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct BuildVariantConfig {
    /// The Dockerfile stage; defaults to the target's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The image tag suffix; defaults to the target's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// Build arguments merged over the module's `build.args`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, EnvScalar>,
}

static NO_ARGS: BTreeMap<String, EnvScalar> = BTreeMap::new();

impl BuildTarget {
    #[must_use]
    pub fn stage<'a>(&'a self, name: &'a str) -> &'a str {
        match self {
            Self::Suffix(_) => name,
            Self::Variant(variant) => variant.target.as_deref().unwrap_or(name),
        }
    }

    #[must_use]
    pub fn suffix<'a>(&'a self, name: &'a str) -> &'a str {
        match self {
            Self::Suffix(suffix) => suffix,
            Self::Variant(variant) => variant.suffix.as_deref().unwrap_or(name),
        }
    }

    #[must_use]
    pub fn args(&self) -> &BTreeMap<String, EnvScalar> {
        match self {
            Self::Suffix(_) => &NO_ARGS,
            Self::Variant(variant) => &variant.args,
        }
    }
}

impl From<&str> for BuildTarget {
    fn from(suffix: &str) -> Self {
        Self::Suffix(suffix.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildTargetSelection<'a> {
    /// The module builds without a Dockerfile target.
    Default,
    Selected(&'a str),
    Unknown(&'a str),
    Ambiguous,
}

impl ModuleBuildConfig {
    /// The target an environment builds and deploys: its `build-variant`,
    /// else `default-variant`, else the only target.
    #[must_use]
    pub fn select_target<'a>(
        &'a self,
        environment_variant: Option<&'a str>,
    ) -> BuildTargetSelection<'a> {
        if self.targets.is_empty() {
            return BuildTargetSelection::Default;
        }
        match environment_variant.or(self.default_variant.as_deref()) {
            Some(name) => match self.targets.get_key_value(name) {
                Some((name, _)) => BuildTargetSelection::Selected(name),
                None => BuildTargetSelection::Unknown(name),
            },
            None if self.targets.len() == 1 => self
                .targets
                .keys()
                .next()
                .map_or(BuildTargetSelection::Default, |name| {
                    BuildTargetSelection::Selected(name.as_str())
                }),
            None => BuildTargetSelection::Ambiguous,
        }
    }
}
