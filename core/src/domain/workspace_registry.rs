//! The project folders mm has run in on this machine, and forgetting them.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::workspace::{ProjectId, WorkspaceId, WorkspaceRecord};

/// How many registered workspaces the dashboard reads.
pub const MAX_REGISTERED_WORKSPACES: usize = 10_000;

pub const FORGET_STALE_COMMAND: &str = "mm workspace forget --stale";

/// A registered workspace as the state store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceUsage {
    pub workspace: WorkspaceRecord,
    pub last_used_at_unix_ms: i64,
    pub operations: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePresence {
    Present,
    RootMissing,
    ConfigMissing,
}

impl WorkspacePresence {
    #[must_use]
    pub const fn is_stale(self) -> bool {
        !matches!(self, Self::Present)
    }

    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::RootMissing => "stale: folder gone",
            Self::ConfigMissing => "stale: mm.yaml gone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RegisteredWorkspace {
    pub id: WorkspaceId,
    pub project_id: ProjectId,
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub last_used_at_unix_ms: i64,
    /// Operations mm journalled for it: builds, deployments, setups.
    pub operations: u64,
    pub presence: WorkspacePresence,
    /// Its folder or its configuration file is gone.
    pub stale: bool,
}

impl RegisteredWorkspace {
    #[must_use]
    pub fn new(usage: WorkspaceUsage, presence: WorkspacePresence) -> Self {
        Self {
            id: usage.workspace.id,
            project_id: usage.workspace.project_id,
            root: usage.workspace.root,
            config_path: usage.workspace.config_path,
            last_used_at_unix_ms: usage.last_used_at_unix_ms,
            operations: usage.operations,
            presence,
            stale: presence.is_stale(),
        }
    }

    #[must_use]
    pub fn usage(&self) -> WorkspaceUsage {
        WorkspaceUsage {
            workspace: WorkspaceRecord {
                id: self.id,
                project_id: self.project_id,
                root: self.root.clone(),
                config_path: self.config_path.clone(),
            },
            last_used_at_unix_ms: self.last_used_at_unix_ms,
            operations: self.operations,
        }
    }
}

/// Which registered workspaces `mm workspace forget` forgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSelector {
    Stale,
    /// A project or workspace ID, or a root as typed; `path` is that root
    /// with its links resolved, or made absolute when it no longer exists.
    One {
        given: String,
        path: PathBuf,
    },
}

impl WorkspaceSelector {
    /// The command that reviews this selection.
    #[must_use]
    pub fn command(&self) -> String {
        match self {
            Self::Stale => FORGET_STALE_COMMAND.to_owned(),
            Self::One { given, .. } => format!("mm workspace forget {given}"),
        }
    }

    #[must_use]
    pub fn names(&self, workspace: &RegisteredWorkspace) -> bool {
        match self {
            Self::Stale => workspace.stale,
            Self::One { given, path } => {
                let id = uuid::Uuid::parse_str(given.trim()).ok();
                id.is_some_and(|id| workspace.id.0 == id || workspace.project_id.0 == id)
                    || workspace.root == *path
            }
        }
    }
}

/// The reviewed removal of registered workspaces, their operation history and
/// their local value overrides from this machine's state. No file is deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceForgetPlan {
    pub workspaces: Vec<RegisteredWorkspace>,
    pub operations: u64,
    pub plan_hash: String,
}

impl WorkspaceForgetPlan {
    #[must_use]
    pub fn new(workspaces: Vec<RegisteredWorkspace>) -> Self {
        let bytes =
            serde_json::to_vec(&("mm-workspace-forget-v1", &workspaces)).unwrap_or_default();
        Self {
            operations: workspaces
                .iter()
                .map(|workspace| workspace.operations)
                .sum(),
            plan_hash: blake3::hash(&bytes).to_hex().to_string(),
            workspaces,
        }
    }

    /// Refuses a hash that is not this plan's: a workspace was used,
    /// registered or forgotten since the review, or the hash is another
    /// command's.
    pub fn check(&self, reviewed: &str, command: &str) -> Result<(), String> {
        if reviewed == self.plan_hash {
            return Ok(());
        }
        Err(format!(
            "the registered workspaces changed since that review, or the hash is another command's; run `{command}` and review it again"
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceForgetResult {
    pub forgotten: Vec<RegisteredWorkspace>,
    pub operations: u64,
}
