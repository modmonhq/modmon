use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::error::MmError;
use crate::domain::target::{NamedTarget, TargetId};
use crate::domain::workspace::ProjectId;

pub const LABEL_PROJECT_ID: &str = "io.mm.project-id";
pub const LABEL_ENVIRONMENT: &str = "io.mm.environment";
pub const LABEL_STACK: &str = "io.mm.stack";
pub const LABEL_TARGET_ID: &str = "io.mm.target-id";
pub const LABEL_RESOURCE_KIND: &str = "io.mm.resource-kind";
pub const LABEL_RESOURCE: &str = "io.mm.resource";
pub const LABEL_GENERATION: &str = "io.mm.generation";
pub const OWNERSHIP_GENERATION: u32 = 1;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Network,
    Volume,
    Config,
    Secret,
    Service,
    Stack,
}

impl ResourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Volume => "volume",
            Self::Config => "config",
            Self::Secret => "secret",
            Self::Service => "service",
            Self::Stack => "stack",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRecordState {
    Created,
    Adopted,
    ExternallyManaged,
    Discovered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceOwnership {
    pub project_id: Option<ProjectId>,
    pub environment: Option<String>,
    pub target_id: TargetId,
    pub kind: ResourceKind,
    pub logical_name: String,
    pub resource: String,
    pub generation: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipVerdict {
    Owned,
    Conflict {
        expected: BTreeMap<String, String>,
        observed: BTreeMap<String, String>,
    },
}

impl ResourceOwnership {
    #[must_use]
    pub fn for_named_target(
        target: &NamedTarget,
        kind: ResourceKind,
        logical_name: impl Into<String>,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            project_id: None,
            environment: None,
            target_id: target.definition.id,
            kind,
            logical_name: logical_name.into(),
            resource: resource.into(),
            generation: OWNERSHIP_GENERATION,
        }
    }

    #[must_use]
    pub fn for_runtime_target(
        target: &crate::domain::runtime::RuntimeTarget,
        kind: ResourceKind,
        logical_name: impl Into<String>,
        resource: impl Into<String>,
    ) -> Option<Self> {
        Some(Self {
            project_id: (!target.installation_scoped).then_some(target.project_id),
            environment: (!target.installation_scoped).then(|| target.environment.clone()),
            target_id: target.target.as_ref()?.definition.id,
            kind,
            logical_name: logical_name.into(),
            resource: resource.into(),
            generation: OWNERSHIP_GENERATION,
        })
    }

    #[must_use]
    pub fn labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_TARGET_ID.to_owned(), self.target_id.0.to_string());
        labels.insert(LABEL_RESOURCE.to_owned(), self.resource.clone());
        labels.insert(
            LABEL_RESOURCE_KIND.to_owned(),
            self.kind.as_str().to_owned(),
        );
        labels.insert(LABEL_GENERATION.to_owned(), self.generation.to_string());
        if let Some(project_id) = self.project_id {
            labels.insert(LABEL_PROJECT_ID.to_owned(), project_id.0.to_string());
        }
        if let Some(environment) = &self.environment {
            labels.insert(LABEL_ENVIRONMENT.to_owned(), environment.clone());
        }
        if self.kind == ResourceKind::Stack {
            labels.insert(LABEL_STACK.to_owned(), self.resource.clone());
        }
        labels
    }

    #[must_use]
    pub fn verify(
        &self,
        observed_name: &str,
        observed_labels: Option<&BTreeMap<String, String>>,
    ) -> OwnershipVerdict {
        let expected = self.labels();
        let mut observed = BTreeMap::new();
        observed.insert("name".to_owned(), observed_name.to_owned());
        if observed_name != self.logical_name {
            return OwnershipVerdict::Conflict { expected, observed };
        }
        let Some(labels) = observed_labels else {
            return OwnershipVerdict::Conflict { expected, observed };
        };
        for (key, value) in labels {
            if !key.chars().any(char::is_control) && !value.chars().any(char::is_control) {
                observed.insert(key.clone(), value.clone());
            }
        }
        let owned = expected.iter().all(|(key, value)| {
            labels
                .get(key)
                .is_some_and(|observed_value| observed_value == value)
        });
        if owned {
            OwnershipVerdict::Owned
        } else {
            OwnershipVerdict::Conflict { expected, observed }
        }
    }

    pub fn require_owned(
        &self,
        observed_name: &str,
        observed_labels: Option<&BTreeMap<String, String>>,
    ) -> Result<(), MmError> {
        match self.verify(observed_name, observed_labels) {
            OwnershipVerdict::Owned => Ok(()),
            OwnershipVerdict::Conflict { expected, observed } => {
                Err(ownership_conflict(self, expected, observed))
            }
        }
    }
}

pub fn ownership_conflict(
    ownership: &ResourceOwnership,
    expected: BTreeMap<String, String>,
    observed: BTreeMap<String, String>,
) -> MmError {
    MmError::Security {
        message: format!(
            "refusing to use {} '{}' because its mm ownership labels do not match (expected {expected:?}, observed {observed:?}); import it with an explicit reviewed adopt command rather than auto-adopting",
            ownership.kind.as_str(),
            ownership.logical_name
        ),
    }
}

pub fn validate_resource_name(name: &str) -> Result<(), MmError> {
    if name.is_empty()
        || name.len() > 255
        || !name.bytes().enumerate().all(|(index, value)| {
            value.is_ascii_alphanumeric() || (index > 0 && matches!(value, b'_' | b'.' | b'-'))
        })
    {
        return Err(MmError::Security {
            message: format!("invalid runtime resource name '{name}'"),
        });
    }
    Ok(())
}

pub fn validate_resource_label(resource: &str) -> Result<(), MmError> {
    if resource.is_empty() || resource.len() > 255 || resource.chars().any(char::is_control) {
        return Err(MmError::Security {
            message: "mm runtime resource label is empty, too long, or contains control characters"
                .to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
