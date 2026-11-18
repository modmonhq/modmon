//! What a host remembers of an environment's deployments: the last one it
//! finished, the one before, and the rollout that holds or last held the
//! environment's deployment lock.

use std::sync::{Mutex, PoisonError};

use super::*;
use crate::domain::access::PrincipalId;
use crate::domain::rollout::{
    DeploymentRecord, NodeDeploymentStatus, NodeRollout, RolloutState, StepFailure,
};

/// A rollout as the host keeps it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoredRollout {
    pub plan_hash: String,
    pub commit: String,
    pub principal_id: PrincipalId,
    pub started_by: String,
    pub started_at_unix_ms: i64,
    pub heard_at_unix_ms: i64,
    /// Until when the client that deploys holds the lock without saying
    /// anything more.
    pub expires_at_unix_ms: i64,
    pub state: RolloutState,
    #[serde(default)]
    pub stacks: Vec<String>,
    #[serde(default)]
    pub failure: Option<StepFailure>,
    /// A rollout that was left unfinished here and that this one took the
    /// lock from. It is forgotten once this one begins, which undoes it.
    #[serde(default)]
    pub interrupted: Option<Box<StoredRollout>>,
    /// When `mmd` started and found this rollout in progress: the step
    /// that ran then ended with the daemon before it. Cleared when the
    /// rollout's client carries on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted_at_unix_ms: Option<i64>,
    /// What the rollout's steps changed here, noted before each step ran;
    /// a rollback puts back only that. `None` on a record an older `mmd`
    /// wrote: that rollout is undone whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<crate::domain::rollout::RolloutChanges>,
    /// The access the rollout began under, once its Begin ran here: its
    /// steps stay its principal's to run (`domain::rollout::BegunUnder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub begun_under: Option<crate::domain::rollout::BegunUnder>,
}

impl StoredRollout {
    /// Whether a client still holds the deployment lock with it.
    #[must_use]
    pub fn holds_lock(&self, now_unix_ms: i64) -> bool {
        matches!(self.state, RolloutState::Held | RolloutState::Running)
            && now_unix_ms < self.expires_at_unix_ms
    }

    /// The moment the review window of the plan is judged at when a node
    /// begins it: when the node took the hold, while that hold is still
    /// alive. The hold itself was refused outside the window, and a node's
    /// turn may come long after the window closed, behind a migration or a
    /// drain on the nodes before it. A hold that lapsed is judged now.
    #[must_use]
    pub fn reviewed_at(&self, now_unix_ms: i64) -> i64 {
        if self.holds_lock(now_unix_ms) {
            self.started_at_unix_ms
        } else {
            now_unix_ms
        }
    }

    /// Whether its client was last heard from more than `silence_ms` ago
    /// while it still holds the lock or runs: the client says every few
    /// seconds that it is still there.
    #[must_use]
    pub fn client_silent(&self, now_unix_ms: i64, silence_ms: i64) -> bool {
        matches!(self.state, RolloutState::Held | RolloutState::Running)
            && now_unix_ms.saturating_sub(self.heard_at_unix_ms) > silence_ms
    }

    fn report(&self, now_unix_ms: i64) -> NodeRollout {
        NodeRollout {
            plan_hash: self.plan_hash.clone(),
            commit: self.commit.clone(),
            started_by: self.started_by.clone(),
            started_at_unix_ms: self.started_at_unix_ms,
            heard_at_unix_ms: self.heard_at_unix_ms,
            active: self.holds_lock(now_unix_ms),
            state: self.state,
            stacks: self.stacks.clone(),
            failure: self
                .failure
                .clone()
                .or_else(|| self.interruption(now_unix_ms)),
        }
    }

    /// Why nobody finished this rollout, when a daemon restart is why.
    fn interruption(&self, now_unix_ms: i64) -> Option<StepFailure> {
        let at = self.interrupted_at_unix_ms?;
        (self.state == RolloutState::Running && !self.holds_lock(now_unix_ms)).then(|| StepFailure {
            stack: String::new(),
            service: None,
            reason: format!(
                "mmd on this node restarted at {} while this rollout ran, which interrupted the step in progress",
                crate::domain::rollout::utc_time(at)
            ),
            log: Vec::new(),
            code: None,
        })
    }
}

/// One environment's deployments on this host.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoredDeployments {
    #[serde(default)]
    pub current: Option<DeploymentRecord>,
    #[serde(default)]
    pub previous: Option<DeploymentRecord>,
    #[serde(default)]
    pub rollout: Option<StoredRollout>,
    /// The `once` jobs of the environment that ran or started, on this
    /// node or another, by job name.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub once: std::collections::BTreeMap<String, crate::domain::scheduler::OnceRecord>,
}

impl StoredDeployments {
    /// Records a finished deployment. The deployment before it stays the
    /// one to return to when this one runs the same commit and images.
    pub fn finished(&mut self, record: DeploymentRecord) {
        match self.current.take() {
            Some(current) if !current.same_release(&record) => self.previous = Some(current),
            _ => {}
        }
        self.current = Some(record);
    }

    /// What a daemon that starts does with the rollout an earlier daemon
    /// left in progress: whatever ran a step then is gone, so the lock is
    /// given up at once instead of when the step's lease runs out, and a
    /// rollout that began is marked interrupted. A client that is still
    /// there takes the lock again with its next sign of life. Returns the
    /// plan and the state it was in, when there was one.
    pub fn interrupt(&mut self, now_unix_ms: i64) -> Option<(String, RolloutState)> {
        let rollout = self
            .rollout
            .as_mut()
            .filter(|rollout| rollout.holds_lock(now_unix_ms))?;
        rollout.expires_at_unix_ms = now_unix_ms;
        if rollout.state == RolloutState::Running {
            rollout.interrupted_at_unix_ms = Some(now_unix_ms);
        }
        Some((rollout.plan_hash.clone(), rollout.state))
    }

    /// What `mm deploy status` shows. A rollout that only holds the lock
    /// does not hide the unfinished one it took the lock from once its
    /// client is gone.
    #[must_use]
    pub fn report(&self, now_unix_ms: i64) -> NodeDeploymentStatus {
        let rollout = self.rollout.as_ref().map(|rollout| {
            match (&rollout.interrupted, rollout.holds_lock(now_unix_ms)) {
                (Some(interrupted), false) if rollout.state == RolloutState::Held => {
                    interrupted.report(now_unix_ms)
                }
                _ => rollout.report(now_unix_ms),
            }
        });
        NodeDeploymentStatus {
            current: self.current.clone(),
            previous: self.previous.clone(),
            rollout,
            operations: Vec::new(),
        }
    }
}

/// Serialises every read-modify-write of the file: the client's "still
/// here" arrives while a step of the same rollout is being recorded.
static DEPLOYMENTS: Mutex<()> = Mutex::new(());

impl HostAccessStore {
    fn deployments_path(&self, project: ProjectId, environment: &str) -> PathBuf {
        self.project_directory(project)
            .join("deployments")
            .join(format!("{environment}.json"))
    }

    /// Every enrolled environment, for a look at each one's deployments.
    pub fn enrolled_environments(&self) -> Result<Vec<(ProjectId, String)>, MmError> {
        let (host, _) = self.load_host()?;
        Ok(host
            .projects
            .iter()
            .flat_map(|(project, enrolled)| {
                enrolled
                    .environments
                    .iter()
                    .map(|environment| (*project, environment.clone()))
            })
            .collect())
    }

    pub fn deployments(
        &self,
        project: ProjectId,
        environment: &str,
    ) -> Result<StoredDeployments, MmError> {
        let path = self.deployments_path(project, environment);
        let Some(bytes) = read_optional(&path)? else {
            return Ok(StoredDeployments::default());
        };
        serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
            path,
            message: error.to_string(),
        })
    }

    /// Changes what the host remembers of one environment's deployments.
    /// `change` may refuse; then nothing is written.
    pub fn update_deployments<T>(
        &self,
        project: ProjectId,
        environment: &str,
        change: impl FnOnce(&mut StoredDeployments) -> Result<T, MmError>,
    ) -> Result<T, MmError> {
        let _serial = DEPLOYMENTS.lock().unwrap_or_else(PoisonError::into_inner);
        let mut stored = self.deployments(project, environment)?;
        let before = stored.clone();
        let result = change(&mut stored)?;
        if stored != before {
            let path = self.deployments_path(project, environment);
            create_private_directory(path.parent().unwrap_or(&self.state_root))?;
            write_atomic(&path, &to_json(&stored)?, 0o600)?;
        }
        Ok(result)
    }
}
