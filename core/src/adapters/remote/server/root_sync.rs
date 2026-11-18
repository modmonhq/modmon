/// Who `mmd access sync` records as having reconciled access, and as the
/// actor in the audit log.
pub const HOST_ROOT: &str = "host-root";

/// Bounds the fetch and checkout of a root access sync.
const ROOT_SYNC_TIMEOUT: Duration = Duration::from_secs(600);

/// What `mmd access sync` reconciles: one environment's users file at one
/// commit.
#[derive(Debug, Clone)]
pub struct RootAccessSyncRequest {
    pub project_id: crate::domain::workspace::ProjectId,
    pub environment: String,
    pub commit: String,
    /// Report what would change and write nothing.
    pub dry_run: bool,
}

/// What `mmd access sync` did, or would do with `dry_run`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RootAccessSync {
    pub project: crate::domain::workspace::ProjectId,
    pub environment: String,
    pub commit: String,
    /// The commit whose users file this host enforced before.
    pub previous_commit: Option<String>,
    /// Whether access was (or, in a dry run, would be) written. `false`
    /// when this host already enforces this commit's users file.
    pub changed: bool,
    pub dry_run: bool,
    pub deploy_from: Option<Vec<String>>,
    /// Everyone with access to the environment at this commit, within the
    /// host's ceiling.
    pub users: Vec<RootSyncedUser>,
    /// What changed for each user against the previous record.
    pub changes: Vec<crate::domain::users::AccessChangeEntry>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RootSyncedUser {
    pub user: String,
    pub machine: bool,
    pub access: crate::domain::host_policy::EnvironmentAccess,
    /// `SHA256:` fingerprints of the user's unexpired keys.
    pub keys: Vec<String>,
}

impl RemoteDaemon {
    /// Reconciles one environment's users file from `commit` as host root,
    /// without the relay: the first team sync of a new host. It checks the
    /// commit exactly as `mm team sync` does and writes access the same way.
    ///
    /// Unlike a reviewed team sync it only moves forward: a commit that the
    /// recorded one is not an ancestor of is refused. Re-running at the
    /// recorded commit writes nothing.
    pub async fn sync_access_as_root(
        &self,
        request: &RootAccessSyncRequest,
    ) -> Result<RootAccessSync, MmError> {
        let project = request.project_id;
        let environment = request.environment.as_str();
        let checked = self
            .checkout_commit(project, environment, HOST_ROOT, &request.commit, ROOT_SYNC_TIMEOUT)
            .await?;
        // Without the repository nothing says which commit follows which:
        // on a host the project is pushed to, root decides.
        if let Some(current) = &checked.current
            && current.commit != request.commit
            && !self.delivery(project)?.is_push()
        {
            self.require_forward(project, &current.commit, &request.commit).await?;
        }
        let unchanged = checked
            .current
            .as_ref()
            .is_some_and(|current| same_access(current, &checked.access.record));
        let mut summary = self.root_sync_summary(request, &checked.access)?;
        summary.changed = !unchanged;
        if request.dry_run || !summary.changed {
            return Ok(summary);
        }
        self.access
            .locked("mmd access sync", |guard| {
                let locked = guard.snapshot().records.iter().find(|record| {
                    record.project_id == project && record.environment == environment
                });
                if locked != checked.current.as_ref() {
                    return Err(MmError::Security {
                        message: "access to this environment changed while syncing; run the sync again"
                            .to_owned(),
                    });
                }
                self.apply_access(HOST_ROOT, guard, &checked.access)
            })
            .await?;
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "event": "access-synced",
            "principal": HOST_ROOT, "project_id": project, "environment": environment,
            "commit": request.commit, "previous_commit": summary.previous_commit,
            "changes": summary.changes,
            "result": "succeeded", "daemon_version": env!("CARGO_PKG_VERSION"),
        }))?;
        Ok(summary)
    }

    /// Refuses `commit` unless the recorded `current` commit leads to it.
    async fn require_forward(
        &self,
        project: crate::domain::workspace::ProjectId,
        current: &str,
        commit: &str,
    ) -> Result<(), MmError> {
        let snapshot = self.access.load()?;
        let enrolled = snapshot.host.projects.get(&project).ok_or_else(|| MmError::Security {
            message: "project is not enrolled on this host".to_owned(),
        })?;
        let forward = self
            .repositories
            .is_ancestor(project, enrolled, current, commit)
            .await
            .map_err(|error| MmError::Security {
                message: format!(
                    "cannot tell whether {commit} follows {current}, whose users file this host enforces: {error}"
                ),
            })?;
        if forward {
            return Ok(());
        }
        Err(MmError::Security {
            message: format!(
                "this host enforces the users file of {current}, which {commit} does not follow; `mmd access sync` only moves access forward. Review going back with `mm team sync`"
            ),
        })
    }

    fn root_sync_summary(
        &self,
        request: &RootAccessSyncRequest,
        access: &AccessChange,
    ) -> Result<RootAccessSync, MmError> {
        let ceiling = self
            .access
            .load()?
            .host
            .projects
            .get(&request.project_id)
            .map(|project| project.ceiling_for(&request.environment))
            .ok_or_else(|| MmError::Security {
                message: "project is not enrolled on this host".to_owned(),
            })?;
        Ok(RootAccessSync {
            project: request.project_id,
            environment: request.environment.clone(),
            commit: request.commit.clone(),
            previous_commit: access.previous_commit.clone(),
            changed: false,
            dry_run: request.dry_run,
            deploy_from: access.record.deploy_from.clone(),
            users: access
                .record
                .users
                .iter()
                .map(|(name, user)| RootSyncedUser {
                    user: name.clone(),
                    machine: user.machine,
                    access: user.access.within(&ceiling),
                    keys: user.keys.iter().map(|key| key.fingerprint.clone()).collect(),
                })
                .collect(),
            changes: access.entries.clone(),
        })
    }
}

/// Whether two records grant the same access from the same commit, whoever
/// reconciled them and when.
fn same_access(
    left: &crate::domain::host_policy::EnvironmentAccessRecord,
    right: &crate::domain::host_policy::EnvironmentAccessRecord,
) -> bool {
    let mut right = right.clone();
    right.reconciled_at_unix_ms = left.reconciled_at_unix_ms;
    right.reconciled_by.clone_from(&left.reconciled_by);
    *left == right
}
