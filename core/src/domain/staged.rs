//! Secrets and per-environment variables set on an environment's hosts
//! over the relay. A change is staged on every node and goes live with the
//! next deployment.

use std::collections::BTreeSet;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::domain::environment::is_safe_environment_value;
use crate::domain::workspace::ProjectId;

pub const STAGED_VALUES_VERSION: u32 = 1;
pub const MAX_STAGED_CHANGES: usize = 64;
pub const MAX_STAGED_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_SEALED_STAGE_BYTES: usize = 1024 * 1024;
/// How long a node keeps a prepared stage that was neither committed nor reverted.
pub const STAGE_PREPARE_TTL_MS: i64 = 5 * 60 * 1_000;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Secret,
    Variable,
}

impl ValueKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Secret => "secret",
            Self::Variable => "variable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StagedAction {
    /// A new value, sealed to the node. Every node gets the same revision.
    Set { revision: String },
    /// The value is removed at the next deployment.
    Unset { revision: String },
    /// What is staged for the key is dropped.
    Discard,
}

impl StagedAction {
    #[must_use]
    pub fn revision(&self) -> Option<&str> {
        match self {
            Self::Set { revision } | Self::Unset { revision } => Some(revision),
            Self::Discard => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValueChange {
    pub key: String,
    pub kind: ValueKind,
    pub action: StagedAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageStep {
    /// The node checks the changes and keeps them, unapplied, until
    /// `Commit` or `Revert`. `sealed` is the values of `Set` changes,
    /// sealed with age to the node's store.
    Prepare {
        changes: Vec<ValueChange>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sealed: Option<String>,
    },
    Commit,
    /// Drops a prepared stage, or undoes a committed one.
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValueStage {
    pub stage: Uuid,
    pub step: StageStep,
}

impl ValueStage {
    pub fn validate(&self) -> Result<(), String> {
        let StageStep::Prepare { changes, sealed } = &self.step else {
            return Ok(());
        };
        validate_changes(changes)?;
        let sets = changes
            .iter()
            .any(|change| matches!(change.action, StagedAction::Set { .. }));
        match sealed {
            Some(sealed) if sealed.len() > MAX_SEALED_STAGE_BYTES => Err(format!(
                "the sealed values exceed the {MAX_SEALED_STAGE_BYTES}-byte limit"
            )),
            Some(_) if !sets => Err("only a change that sets a value carries one".to_owned()),
            None if sets => Err("a change that sets a value needs its sealed value".to_owned()),
            _ => Ok(()),
        }
    }
}

fn validate_changes(changes: &[ValueChange]) -> Result<(), String> {
    if changes.is_empty() {
        return Err("a stage needs at least one change".to_owned());
    }
    if changes.len() > MAX_STAGED_CHANGES {
        return Err(format!(
            "a stage holds at most {MAX_STAGED_CHANGES} changes"
        ));
    }
    let mut seen = BTreeSet::new();
    for change in changes {
        validate_key(&change.key)?;
        if !seen.insert((change.kind, change.key.as_str())) {
            return Err(format!(
                "{} '{}' is changed twice",
                change.kind.as_str(),
                change.key
            ));
        }
        if let Some(revision) = change.action.revision() {
            crate::domain::secret::validate_secret_revision(revision)?;
        }
    }
    Ok(())
}

/// A secret's name under `secrets:` or a variable's global key.
pub fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.len() > 255
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "'{}' is not a key: use letters, digits, '.', '_' and '-'",
            key.escape_default()
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    Prepared,
    Committed,
    Reverted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageAnswer {
    pub stage: Uuid,
    pub state: StageState,
    /// What the stage changes; never a value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<ValueChange>,
}

/// The plaintext sealed to one node: the values of a stage's `Set` changes.
pub struct StagedPayload {
    pub project_id: ProjectId,
    pub environment: String,
    pub stage: Uuid,
    pub values: Vec<StagedPayloadValue>,
}

pub struct StagedPayloadValue {
    pub key: String,
    pub kind: ValueKind,
    pub revision: String,
    pub value: Zeroizing<String>,
}

impl fmt::Debug for StagedPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StagedPayload")
            .field("project_id", &self.project_id)
            .field("environment", &self.environment)
            .field("stage", &self.stage)
            .field("values", &self.values.len())
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadWire {
    version: u32,
    project_id: ProjectId,
    environment: String,
    stage: Uuid,
    values: Vec<PayloadValueWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadValueWire {
    key: String,
    kind: ValueKind,
    revision: String,
    value: Zeroizing<String>,
}

impl StagedPayload {
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        let wire = PayloadWire {
            version: STAGED_VALUES_VERSION,
            project_id: self.project_id,
            environment: self.environment.clone(),
            stage: self.stage,
            values: self
                .values
                .iter()
                .map(|value| PayloadValueWire {
                    key: value.key.clone(),
                    kind: value.kind,
                    revision: value.revision.clone(),
                    value: value.value.clone(),
                })
                .collect(),
        };
        serde_json::to_vec(&wire)
            .map(Zeroizing::new)
            .map_err(|_| "the staged values could not be encoded".to_owned())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let wire: PayloadWire = serde_json::from_slice(bytes).map_err(|error| {
            format!(
                "the staged values are malformed at line {}, column {}",
                error.line(),
                error.column()
            )
        })?;
        if wire.version != STAGED_VALUES_VERSION {
            return Err(format!(
                "staged values version {} is not supported",
                wire.version
            ));
        }
        let payload = Self {
            project_id: wire.project_id,
            environment: wire.environment,
            stage: wire.stage,
            values: wire
                .values
                .into_iter()
                .map(|value| StagedPayloadValue {
                    key: value.key,
                    kind: value.kind,
                    revision: value.revision,
                    value: value.value,
                })
                .collect(),
        };
        payload.validate()?;
        Ok(payload)
    }

    pub fn validate(&self) -> Result<(), String> {
        crate::domain::host_policy::validate_environment_name(&self.environment)?;
        for value in &self.values {
            validate_value(&value.key, &value.value)?;
        }
        Ok(())
    }

    /// The payload carries exactly the values the stage's `Set` changes name.
    pub fn check_matches(
        &self,
        project: ProjectId,
        environment: &str,
        stage: Uuid,
        changes: &[ValueChange],
    ) -> Result<(), String> {
        if self.project_id != project || self.environment != environment || self.stage != stage {
            return Err(
                "the sealed values are for another project, environment or stage".to_owned(),
            );
        }
        let wanted = changes
            .iter()
            .filter_map(|change| match &change.action {
                StagedAction::Set { revision } => {
                    Some((change.kind, change.key.as_str(), revision.as_str()))
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let carried = self
            .values
            .iter()
            .map(|value| (value.kind, value.key.as_str(), value.revision.as_str()))
            .collect::<BTreeSet<_>>();
        if wanted != carried || carried.len() != self.values.len() {
            return Err("the sealed values do not match the changes they come with".to_owned());
        }
        Ok(())
    }
}

/// A value a host can keep: not empty, bounded, no control characters.
pub fn validate_value(key: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!(
            "the value for '{key}' is empty; use unset to remove it"
        ));
    }
    if value.len() > MAX_STAGED_VALUE_BYTES {
        return Err(format!(
            "the value for '{key}' exceeds the {MAX_STAGED_VALUE_BYTES}-byte limit"
        ));
    }
    if !is_safe_environment_value(value) {
        return Err(format!(
            "the value for '{key}' contains control characters; a value is one line of text"
        ));
    }
    Ok(())
}

/// Where the value a node holds for a key came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueOrigin {
    /// Set over the relay with `mm secret set` or `mm env set`.
    Staged,
    /// Pushed with the environment's other secrets by `mm target setup`.
    Setup,
    /// Read by the host from OpenBao or Vault.
    ExternalStore,
    /// The value is fixed in the repo's configuration.
    Repo,
}

impl ValueOrigin {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Staged => "set on mmd",
            Self::Setup => "pushed by setup",
            Self::ExternalStore => "external store",
            Self::Repo => "repo",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValueRevision {
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StagedRevision {
    pub revision: String,
    /// The value is removed when the stage goes live.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removal: bool,
    pub staged_by: String,
    pub staged_at_unix_ms: i64,
}

/// One declared secret or variable as one node holds it. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostValue {
    pub key: String,
    pub kind: ValueKind,
    /// `None` while nothing is live for the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ValueOrigin>,
    /// The store an external secret is read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The services of the deployed commit that use it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<ValueRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged: Option<StagedRevision>,
}

impl HostValue {
    /// Whether the next deployment leaves the key without a value.
    #[must_use]
    pub fn unset_after_stage(&self) -> bool {
        match &self.staged {
            Some(staged) => staged.removal,
            None => self.active.is_none() && self.origin != Some(ValueOrigin::ExternalStore),
        }
    }
}

/// What one node holds for an environment's declared secrets and the
/// variables that may be set on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The deployed commit whose configuration declares the keys.
    pub commit: String,
    pub values: Vec<HostValue>,
}

/// A change the next deployment makes live, as a plan lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StagedPlanItem {
    pub key: String,
    pub kind: ValueKind,
    pub change: StagedChangeKind,
    pub revision: String,
    pub staged_by: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StagedChangeKind {
    Added,
    Changed,
    Removed,
}

impl StagedPlanItem {
    /// `+ web.app-key`, `~ web.app-key (revision changed)`, `- web.app-key`.
    #[must_use]
    pub fn line(&self) -> String {
        let (mark, note) = match self.change {
            StagedChangeKind::Added => ("+", ""),
            StagedChangeKind::Changed => ("~", " (revision changed)"),
            StagedChangeKind::Removed => ("-", ""),
        };
        let kind = match self.kind {
            ValueKind::Secret => "",
            ValueKind::Variable => " (variable)",
        };
        format!(
            "{mark} {}{kind}{note}, staged by {}",
            self.key, self.staged_by
        )
    }
}

/// What every node of a remote environment holds, and the nodes that did
/// not answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteValues {
    pub target: String,
    pub environment: String,
    pub nodes: Vec<NodeValues>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreachable: Vec<UnreachableNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnreachableNode {
    pub node: String,
    pub problem: String,
}

impl RemoteValues {
    /// Every key any node holds, in the order the first node lists them.
    #[must_use]
    pub fn keys(&self) -> Vec<(ValueKind, String)> {
        let mut keys = Vec::new();
        for node in &self.nodes {
            for value in &node.values {
                let key = (value.kind, value.key.clone());
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        keys
    }

    /// The nodes whose staged revision of a key differs from the others'.
    #[must_use]
    pub fn disagreeing(&self, kind: ValueKind, key: &str) -> bool {
        let staged = self
            .nodes
            .iter()
            .map(|node| {
                node.values
                    .iter()
                    .find(|value| value.kind == kind && value.key == key)
                    .and_then(|value| value.staged.as_ref())
                    .map(|staged| staged.revision.as_str())
            })
            .collect::<BTreeSet<_>>();
        staged.len() > 1
    }
}
