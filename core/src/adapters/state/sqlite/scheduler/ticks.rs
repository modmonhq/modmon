macro_rules! scheduler_ticks { () => {
    fn claim_due_schedules(
        &self,
        project_id: ProjectId,
        environment_filter: &str,
        owner: Uuid,
        now_unix_ms: i64,
        lease: Duration,
        limit: usize,
    ) -> Result<Vec<ClaimedSchedule>, MmError> {
        let expires = lease_expiry(now_unix_ms, lease)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let mut statement = transaction
            .prepare(
                "SELECT s.project_id, s.environment_name, s.schedule_name, s.definition_json,
                        j.definition_json, s.config_hash, s.next_due_at_ms
                 FROM schedules s JOIN scheduled_jobs j
                   ON j.project_id=s.project_id AND j.environment_name=s.environment_name
                  AND j.job_name=s.job_name
                 WHERE s.project_id=?1 AND s.environment_name=?2
                   AND s.enabled=1 AND s.retired_at_ms IS NULL AND j.retired_at_ms IS NULL
                   AND s.next_due_at_ms<=?3
                   AND (s.lease_expires_at_ms IS NULL OR s.lease_expires_at_ms<=?3)
                 ORDER BY s.next_due_at_ms, s.project_id, s.environment_name, s.schedule_name
                 LIMIT ?4",
            )
            .map_err(state_error)?;
        let candidates = statement
            .query_map(
                params![
                    project_id.0.to_string(),
                    environment_filter,
                    now_unix_ms,
                    i64::try_from(limit).unwrap_or(i64::MAX)
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(state_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(state_error)?;
        drop(statement);
        let mut claims = Vec::new();
        for (project, environment, name, schedule_json, job_json, config_hash, next_due) in
            candidates
        {
            let changed = transaction
                .execute(
                    "UPDATE schedules SET lease_owner=?4, lease_expires_at_ms=?5,
                        fence_token=fence_token+1
                     WHERE project_id=?1 AND environment_name=?2 AND schedule_name=?3
                       AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms<=?6)",
                    params![
                        project,
                        environment,
                        name,
                        owner.to_string(),
                        expires,
                        now_unix_ms
                    ],
                )
                .map_err(state_error)?;
            if changed == 1 {
                let fence = transaction
                    .query_row(
                        "SELECT fence_token FROM schedules
                         WHERE project_id=?1 AND environment_name=?2 AND schedule_name=?3",
                        params![project, environment, name],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(state_error)?;
                claims.push(ClaimedSchedule {
                    project_id: ProjectId(parse_uuid(&project)?),
                    environment,
                    name,
                    definition: deserialize_definition(&schedule_json, "schedule")?,
                    job_definition: deserialize_definition(&job_json, "job")?,
                    config_hash,
                    next_due_at_unix_ms: next_due,
                    lease_owner: owner,
                    fence_token: u64::try_from(fence).map_err(|_| MmError::State {
                        message: "schedule fence token is invalid".to_owned(),
                    })?,
                });
            }
        }
        transaction.commit().map_err(state_error)?;
        Ok(claims)
    }

    fn commit_schedule_tick(&self, commit: &ScheduleTickCommit) -> Result<Vec<JobRun>, MmError> {
                validate_audit_identity(&commit.principal, &commit.source)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let changed = transaction
            .execute(
                "UPDATE schedules SET next_due_at_ms=?7, lease_owner=NULL,
                    lease_expires_at_ms=NULL, updated_at_ms=?8
                 WHERE project_id=?1 AND environment_name=?2 AND schedule_name=?3
                   AND lease_owner=?4 AND fence_token=?5 AND next_due_at_ms=?6",
                params![
                    commit.claim.project_id.0.to_string(),
                    commit.claim.environment,
                    commit.claim.name,
                    commit.claim.lease_owner.to_string(),
                    i64::try_from(commit.claim.fence_token).unwrap_or(i64::MAX),
                    commit.claim.next_due_at_unix_ms,
                    commit.next_due_at_unix_ms,
                    commit.now_unix_ms,
                ],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return Err(MmError::State {
                message: "schedule lease was lost to a newer fence token".to_owned(),
            });
        }
        let active = transaction
            .query_row(
                "SELECT COUNT(*) FROM job_runs
                 WHERE project_id=?1 AND environment_name=?2 AND schedule_name=?3
                   AND state IN ('launching','running','cancelling','unknown')",
                params![
                    commit.claim.project_id.0.to_string(),
                    commit.claim.environment,
                    commit.claim.name,
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(state_error)?;
        let launch = match (commit.claim.definition.concurrency, active) {
            (_, 0) | (ScheduleConcurrency::Allow, _) => true,
            (ScheduleConcurrency::Forbid, _) => false,
            (ScheduleConcurrency::Replace, _) => {
                transaction
                    .execute(
                        "UPDATE job_runs SET cancellation_requested=1, state='cancelling',
                            lease_owner=NULL, lease_expires_at_ms=?4
                         WHERE project_id=?1 AND environment_name=?2 AND schedule_name=?3
                           AND state IN ('launching','running','unknown')",
                        params![
                            commit.claim.project_id.0.to_string(),
                            commit.claim.environment,
                            commit.claim.name,
                            commit.now_unix_ms,
                        ],
                    )
                    .map_err(state_error)?;
                true
            }
        };
        let runs = schedule_tick_runs(&transaction, commit, launch)?;
        append_audit_tx(
            &transaction,
            &JobAuditRecord {
                occurred_at_unix_ms: commit.now_unix_ms,
                project_id: commit.claim.project_id,
                environment: commit.claim.environment.clone(),
                principal: commit.principal.clone(),
                source: commit.source.clone(),
                action: if launch {
                    "trigger"
                } else {
                    "suppress_concurrent"
                }
                .to_owned(),
                subject_kind: "schedule".to_owned(),
                subject_id: commit.claim.name.clone(),
                definition_hash: None,
                details: BTreeMap::from([(
                    "due_count".to_owned(),
                    commit.due_at_unix_ms.len().to_string(),
                )]),
            },
        )?;
        transaction.commit().map_err(state_error)?;
        Ok(runs)
    }

} }

fn schedule_tick_runs(
    transaction: &rusqlite::Transaction<'_>,
    commit: &ScheduleTickCommit,
    launch: bool,
) -> Result<Vec<JobRun>, MmError> {
    let mut runs = Vec::new();
    if !launch {
        return Ok(runs);
    }
    for due in &commit.due_at_unix_ms {
        let run = NewJobRun {
            id: Uuid::new_v4(),
            project_id: commit.claim.project_id,
            environment: commit.claim.environment.clone(),
            job: commit.claim.definition.job.clone(),
            schedule: Some(commit.claim.name.clone()),
            trigger: JobTrigger::Schedule,
            idempotency_key: format!(
                "schedule:{}:{}:{}:{}",
                commit.claim.project_id.0, commit.claim.environment, commit.claim.name, due
            ),
            job_definition: commit.claim.job_definition.clone(),
            schedule_definition: Some(commit.claim.definition.clone()),
            config_hash: commit.claim.config_hash.clone(),
            requested_at_unix_ms: commit.now_unix_ms,
            scheduled_for_unix_ms: Some(*due),
            principal: commit.principal.clone(),
            source: commit.source.clone(),
            retry_of_run_id: None,
        };
        let id = insert_run_tx(transaction, &run)?;
        if let Some(run) = load_run(transaction, id)? {
            runs.push(run);
        }
    }
    Ok(runs)
}
