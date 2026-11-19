//! A binding: the value, secret or release fact a key of an environment has.

use schemars::JsonSchema;
use serde::de::value::MapAccessDeserializer;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::EnvScalar;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum EnvironmentBinding {
    Cast(CastBinding),
    Secret(SecretBinding),
    Value(ValueBinding),
    Reference(ReferenceBinding),
}

/// A release fact under another key: `value-ref: MM.RELEASE`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ReferenceBinding {
    pub value_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ValueBinding {
    pub value: EnvScalar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct SecretBinding {
    pub secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct CastBinding {
    pub cast: ValueCast,
    pub value_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ValueCast {
    Url,
}

impl EnvironmentBinding {
    #[must_use]
    pub const fn value(&self) -> Option<&EnvScalar> {
        match self {
            Self::Value(binding) => Some(&binding.value),
            Self::Secret(_) | Self::Cast(_) | Self::Reference(_) => None,
        }
    }

    #[must_use]
    pub fn secret_name(&self) -> Option<&str> {
        match self {
            Self::Secret(binding) => Some(&binding.secret),
            Self::Value(_) | Self::Cast(_) | Self::Reference(_) => None,
        }
    }

    #[must_use]
    pub const fn cast(&self) -> Option<&CastBinding> {
        match self {
            Self::Cast(binding) => Some(binding),
            Self::Value(_) | Self::Secret(_) | Self::Reference(_) => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BindingFields {
    #[serde(default)]
    cast: Option<ValueCast>,
    #[serde(default)]
    value_ref: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    value: Option<EnvScalar>,
}

impl TryFrom<BindingFields> for EnvironmentBinding {
    type Error = &'static str;

    fn try_from(fields: BindingFields) -> Result<Self, Self::Error> {
        match (fields.cast, fields.value_ref, fields.secret, fields.value) {
            (Some(cast), Some(value_ref), None, None) => {
                Ok(Self::Cast(CastBinding { cast, value_ref }))
            }
            (None, Some(value_ref), None, None) => {
                Ok(Self::Reference(ReferenceBinding { value_ref }))
            }
            (None, None, Some(secret), None) => Ok(Self::Secret(SecretBinding { secret })),
            (None, None, None, Some(value)) => Ok(Self::Value(ValueBinding { value })),
            (Some(_), None, None, None) => {
                Err("`cast:` needs `value-ref:`, the key whose value it casts")
            }
            _ => Err(
                "a binding sets one of `value:`, `secret:` or `value-ref:` (with an optional `cast:`)",
            ),
        }
    }
}

impl<'de> Deserialize<'de> for EnvironmentBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(BindingVisitor)
    }
}

struct BindingVisitor;

impl<'de> Visitor<'de> for BindingVisitor {
    type Value = EnvironmentBinding;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a mapping with `value:`, `secret:` or `value-ref:`")
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        BindingFields::deserialize(MapAccessDeserializer::new(map))?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}
