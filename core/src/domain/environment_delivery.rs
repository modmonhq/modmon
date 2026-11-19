use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::MmConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EnvironmentDeliveryMode {
    Direct,
    EnvFile,
}

impl EnvironmentDeliveryMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::EnvFile => "env-file",
        }
    }

    #[must_use]
    pub const fn driver_id(self) -> &'static str {
        match self {
            Self::Direct => "direct-environment",
            Self::EnvFile => "operation-env-file",
        }
    }

    #[must_use]
    pub fn driver(self) -> crate::domain::secret_contract::DeliveryDriverId {
        crate::domain::secret_contract::DeliveryDriverId::from_static(self.driver_id())
    }

    #[must_use]
    pub const fn required_runtime_capability(
        self,
    ) -> Option<crate::domain::runtime::RuntimeCapability> {
        match self {
            Self::Direct => Some(crate::domain::runtime::RuntimeCapability::DirectEnvironment),
            Self::EnvFile => None,
        }
    }
}

impl std::fmt::Display for EnvironmentDeliveryMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EnvironmentDeliveryProvenance {
    EnvironmentOverride,
    DevelopmentDefault,
    PolicyDefault,
    BuiltInDefault,
}

impl EnvironmentDeliveryProvenance {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EnvironmentOverride => "environment-override",
            Self::DevelopmentDefault => "development-default",
            Self::PolicyDefault => "policy-default",
            Self::BuiltInDefault => "built-in-default",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ResolvedEnvironmentDelivery {
    pub mode: EnvironmentDeliveryMode,
    pub provenance: EnvironmentDeliveryProvenance,
}

impl Default for ResolvedEnvironmentDelivery {
    fn default() -> Self {
        Self {
            mode: EnvironmentDeliveryMode::Direct,
            provenance: EnvironmentDeliveryProvenance::BuiltInDefault,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct EnvironmentDeliveryPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<EnvironmentDeliveryMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_default: Option<EnvironmentDeliveryMode>,
}

impl EnvironmentDeliveryPolicy {
    #[must_use]
    pub const fn is_unspecified(&self) -> bool {
        self.default.is_none() && self.development_default.is_none()
    }
}

#[must_use]
pub fn resolve_environment_delivery(
    config: &MmConfig,
    environment: &str,
) -> ResolvedEnvironmentDelivery {
    if let Some(mode) = config
        .environments
        .get(environment)
        .and_then(|environment_config| environment_config.environment_delivery)
    {
        return ResolvedEnvironmentDelivery {
            mode,
            provenance: EnvironmentDeliveryProvenance::EnvironmentOverride,
        };
    }

    let development_environment = config.project.development_environment.as_deref();
    let is_development = development_environment == Some(environment);
    if is_development && let Some(mode) = config.environment_delivery.development_default {
        return ResolvedEnvironmentDelivery {
            mode,
            provenance: EnvironmentDeliveryProvenance::DevelopmentDefault,
        };
    }

    if let Some(mode) = config.environment_delivery.default {
        return ResolvedEnvironmentDelivery {
            mode,
            provenance: EnvironmentDeliveryProvenance::PolicyDefault,
        };
    }

    if is_development {
        return ResolvedEnvironmentDelivery {
            mode: EnvironmentDeliveryMode::EnvFile,
            provenance: EnvironmentDeliveryProvenance::BuiltInDefault,
        };
    }

    ResolvedEnvironmentDelivery {
        mode: EnvironmentDeliveryMode::Direct,
        provenance: EnvironmentDeliveryProvenance::BuiltInDefault,
    }
}

#[cfg(test)]
mod tests;
