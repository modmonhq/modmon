//! What a plan says when a service's OCI runtime changes against the
//! deployment before it. A plan shows each service's options as they will
//! be, never what runs now, so a service that a host's policy stopped
//! putting under a runtime (`exposed-runtime` taken out) would otherwise
//! move without a word.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::deployment::DeploymentStackPlan;

/// One service whose OCI runtime the plan changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RuntimeChange {
    pub stack: String,
    pub service: String,
    /// The runtime it ran under; absent is the engine's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The runtime it runs under after the plan; absent is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl RuntimeChange {
    /// The plan's line for it.
    #[must_use]
    pub fn line(&self) -> String {
        let named = |runtime: &Option<String>| {
            runtime.as_deref().map_or_else(
                || "the engine's default runtime".to_owned(),
                |runtime| format!("runtime {runtime}"),
            )
        };
        format!(
            "{}: {} (was {}); its containers are created again",
            self.service,
            named(&self.after),
            named(&self.before)
        )
    }
}

/// Each service of `after` that `before`, the deployment before it, ran
/// under another OCI runtime. A service or stack `before` did not have is
/// new, not changed.
#[must_use]
pub fn runtime_changes(
    before: &[DeploymentStackPlan],
    after: &[DeploymentStackPlan],
) -> Vec<RuntimeChange> {
    let runtime = |stack: &DeploymentStackPlan, service: &str| {
        stack
            .artifact
            .manifest
            .security
            .iter()
            .find(|security| security.service == service)
            .and_then(|security| security.runtime.clone())
    };
    let mut changes = Vec::new();
    for stack in after {
        let Some(earlier) = before.iter().find(|earlier| earlier.stack == stack.stack) else {
            continue;
        };
        for service in &stack.artifact.manifest.services {
            let name = service.service.as_str();
            let known = earlier
                .artifact
                .manifest
                .services
                .iter()
                .any(|known| known.service == name);
            let (was, now) = (runtime(earlier, name), runtime(stack, name));
            if known && was != now {
                changes.push(RuntimeChange {
                    stack: stack.stack.clone(),
                    service: name.to_owned(),
                    before: was,
                    after: now,
                });
            }
        }
    }
    changes
}
