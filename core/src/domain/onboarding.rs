use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigResourceKind {
    Module,
    Service,
    Stack,
    Build,
    Environment,
    Binding,
    Secret,
    Ingress,
    LiveMount,
    Job,
    Schedule,
    Gate,
    User,
}

impl ConfigResourceKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Service => "service",
            Self::Stack => "stack",
            Self::Build => "build",
            Self::Environment => "environment",
            Self::Binding => "binding",
            Self::Secret => "secret",
            Self::Ingress => "ingress route",
            Self::LiveMount => "live mount",
            Self::Job => "job",
            Self::Schedule => "schedule",
            Self::Gate => "gate",
            Self::User => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigMutationAction {
    Add,
    Edit,
    Remove,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigMutationRequest {
    pub action: ConfigMutationAction,
    pub resource: ConfigResourceKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<serde_json::Value>,
}

impl std::fmt::Debug for ConfigMutationRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfigMutationRequest")
            .field("action", &self.action)
            .field("resource", &self.resource)
            .field("name", &self.name)
            .field("parent", &self.parent)
            .field(
                "definition",
                &if self.resource == ConfigResourceKind::Secret {
                    self.definition.as_ref().map(|_| "[REDACTED]")
                } else {
                    self.definition.as_ref().map(|_| "[PUBLIC CONFIG]")
                },
            )
            .finish()
    }
}

/// One person's or automation's entry for the users file, as `mm team add`,
/// `mm team join`, and the terminal UI propose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProposal {
    pub name: String,
    /// OpenSSH public key line, `ssh-ed25519 AAAA… comment`.
    pub key: String,
    /// A CI or automation identity rather than a person.
    pub machine: bool,
    /// Level per environment name, or `*` for every remote environment.
    pub access: Vec<(String, crate::domain::host_policy::AccessLevel)>,
}

impl UserProposal {
    /// Reads access the way the command line spells it: `ENV=LEVEL` pairs.
    pub fn parse_access(
        entries: &[String],
    ) -> Result<Vec<(String, crate::domain::host_policy::AccessLevel)>, String> {
        use crate::domain::host_policy::AccessLevel;
        entries
            .iter()
            .map(|entry| {
                let (environment, level) = entry.split_once('=').ok_or_else(|| {
                    format!("'{entry}' must be ENV=LEVEL, for example staging=deploy")
                })?;
                let level = [
                    AccessLevel::View,
                    AccessLevel::Operate,
                    AccessLevel::Deploy,
                    AccessLevel::Admin,
                ]
                .into_iter()
                .find(|candidate| candidate.as_str() == level)
                .ok_or_else(|| {
                    format!("level '{level}' must be view, operate, deploy, or admin")
                })?;
                Ok((environment.to_owned(), level))
            })
            .collect()
    }

    /// The reviewed change that adds this user to the users file. Rejects a
    /// name or key the host would refuse.
    pub fn request(&self) -> Result<ConfigMutationRequest, String> {
        crate::domain::host_policy::parse_public_key(&self.key)?;
        let mut user = serde_json::Map::new();
        if self.machine {
            user.insert("machine".to_owned(), serde_json::Value::Bool(true));
        }
        user.insert("keys".to_owned(), serde_json::json!([self.key.trim()]));
        user.insert(
            "access".to_owned(),
            serde_json::Value::Object(
                self.access
                    .iter()
                    .map(|(environment, level)| {
                        (
                            environment.clone(),
                            serde_json::Value::String(level.as_str().to_owned()),
                        )
                    })
                    .collect(),
            ),
        );
        ConfigMutationRequest::user(
            ConfigMutationAction::Add,
            &self.name,
            Some(serde_json::Value::Object(user)),
        )
    }
}

impl ConfigMutationRequest {
    /// A change to one entry of the users file, after checking its name.
    pub fn user(
        action: ConfigMutationAction,
        name: &str,
        definition: Option<serde_json::Value>,
    ) -> Result<Self, String> {
        if !crate::domain::host_policy::valid_user_name(name) {
            return Err(format!(
                "user name '{name}' must match [a-z][a-z0-9-]{{0,62}}"
            ));
        }
        Ok(Self {
            action,
            resource: ConfigResourceKind::User,
            name: name.to_owned(),
            parent: None,
            definition,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigMutationPreview {
    pub config_path: PathBuf,
    pub source_hash: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigMutationPlan {
    pub request: ConfigMutationRequest,
    pub affected_files: Vec<PathBuf>,
    pub canonical_diff: String,
    pub generated_resources: Vec<String>,
    pub generated_routes: Vec<String>,
    pub privilege_prompts: Vec<String>,
    pub source_hash: String,
    pub result_hash: String,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigMutationResult {
    pub config_path: PathBuf,
    pub source_hash: String,
    pub plan_hash: String,
}

/// Moves one top-level key between `.config/mm.yaml` and its own file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigLayoutAction {
    /// Move the key out to `.config/mm/<key>.yaml`.
    Split,
    /// Move the key back into `.config/mm.yaml`.
    Join,
}

impl ConfigLayoutAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Split => "split",
            Self::Join => "join",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigLayoutRequest {
    pub action: ConfigLayoutAction,
    pub key: String,
}

/// One file's whole text before and after a layout change. `None` means the
/// file does not exist on that side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigFileChange {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLayoutPreview {
    pub anchor_path: PathBuf,
    pub source_hash: String,
    pub changes: Vec<ConfigFileChange>,
    /// Caveats for the reviewer, for example comments that could not move.
    pub notes: Vec<String>,
}

impl ConfigLayoutPreview {
    /// Binds the resulting text of every file, so applying a reviewed plan
    /// writes exactly what was reviewed.
    #[must_use]
    pub fn result_hash(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mm-config-layout-v1\0");
        for change in &self.changes {
            let path = change.path.to_string_lossy();
            hasher.update(&(path.len() as u64).to_le_bytes());
            hasher.update(path.as_bytes());
            match &change.after {
                None => {
                    hasher.update(&[0]);
                }
                Some(text) => {
                    hasher.update(&[1]);
                    hasher.update(&(text.len() as u64).to_le_bytes());
                    hasher.update(text.as_bytes());
                }
            }
        }
        hasher.finalize().to_hex().to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigLayoutPlan {
    pub request: ConfigLayoutRequest,
    pub affected_files: Vec<PathBuf>,
    pub canonical_diff: String,
    pub notes: Vec<String>,
    pub source_hash: String,
    pub result_hash: String,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigLayoutResult {
    pub request: ConfigLayoutRequest,
    /// The file that now declares the key.
    pub config_path: PathBuf,
    pub source_hash: String,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationPreview {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub project_name: String,
    pub project_id: crate::domain::workspace::ProjectId,
    pub config_source: String,
    pub config_hash: String,
    pub starter_files: Vec<PathBuf>,
    pub gitignore: InitializationFileChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitializationFileChange {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
}

impl InitializationFileChange {
    #[must_use]
    pub fn changed(&self) -> bool {
        self.before.as_deref() != Some(self.after.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InitializationPlan {
    pub root: PathBuf,
    pub project_name: String,
    pub project_id: crate::domain::workspace::ProjectId,
    pub affected_files: Vec<PathBuf>,
    pub canonical_diff: String,
    pub generated_resources: Vec<String>,
    pub generated_routes: Vec<String>,
    pub privilege_prompts: Vec<String>,
    pub result_hash: String,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum GuidedJourney {
    GlobalSetup,
    NewProject,
    ExistingProject,
    NodeSetup,
    ProjectEditing,
}

impl GuidedJourney {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::GlobalSetup => "Set up this computer",
            Self::NewProject => "Create a project configuration",
            Self::ExistingProject => "Set up an existing project",
            Self::NodeSetup => "Set up a local or remote node",
            Self::ProjectEditing => "Edit project configuration",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiscoveryProposal {
    pub subject: String,
    pub proposed_value: String,
    pub confidence: DiscoveryConfidence,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectDiscovery {
    pub root: PathBuf,
    pub proposals: Vec<DiscoveryProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowRequest {
    pub journey: GuidedJourney,
    pub root: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation: Option<ConfigMutationRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowReview {
    pub journey: GuidedJourney,
    pub title: String,
    pub proposals: Vec<DiscoveryProposal>,
    pub affected_files: Vec<PathBuf>,
    pub canonical_diff: String,
    pub generated_resources: Vec<String>,
    pub generated_routes: Vec<String>,
    pub privilege_prompts: Vec<String>,
    pub next_actions: Vec<String>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowStep {
    ChooseJourney,
    Discover,
    Configure,
    Review,
    Applying,
    Complete,
    Error,
    Cancelled,
}

/// Serializable workflow state intentionally contains no secret-bearing field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowState {
    pub step: WorkflowStep,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<WorkflowRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<WorkflowReview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self {
            step: WorkflowStep::ChooseJourney,
            request: None,
            review: None,
            message: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowEvent {
    Start(WorkflowRequest),
    Discovered(WorkflowReview),
    Configure,
    Review,
    Apply,
    Applied(String),
    Back,
    Cancel,
    Resume,
    Failed(String),
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowEffect {
    None,
    Discover(WorkflowRequest),
    ApplyReviewed {
        request: WorkflowRequest,
        plan_hash: String,
    },
    /// The presentation obtains material through a hidden, zeroizing buffer and
    /// submits it directly to the secret use case. Plaintext never enters state.
    RequestSecretInput {
        key: String,
        prompt: String,
    },
}

#[cfg(test)]
mod tests;
