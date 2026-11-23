macro_rules! scheduler_runs { () => {
    fn claim_runnable_run(
        &self,
        project_id: ProjectId,
        environment: &str,
        owner: Uuid,
        now_unix_ms: i64,
        lease: Duration,
    ) -> Result<Option<ClaimedJobRun>, MmError> {
        let expires = lease_expiry(now_unix_ms, lease)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let candidate = transaction
            .query_row(
                "SELECT r.id FROM job_runs r
                 WHERE r.project_id=?1 AND r.environment_name=?2 AND (
                    (r.state IN ('queued','retry_wait') AND (r.next_attempt_at_ms IS NULL OR r.next_attempt_at_ms<=?3))
                    OR r.state IN ('launching','running','cancelling','unknown')
                 ) AND (r.lease_expires_at_ms IS NULL OR r.lease_expires_at_ms<=?3)
                   AND (r.state IN ('launching','running','cancelling','unknown')
                     OR r.schedule_concurrency IS NULL OR r.schedule_concurrency!='forbid'
                     OR NOT EXISTS (
                        SELECT 1 FROM job_runs active
                        WHERE active.project_id=r.project_id
                          AND active.environment_name=r.environment_name
                          AND active.schedule_name=r.schedule_name
                          AND active.id!=r.id
                          AND active.state IN ('launching','running','cancelling','unknown')
                     ))
                 ORDER BY CASE WHEN r.state IN ('running','cancelling','unknown','launching') THEN 0 ELSE 1 END,
                          COALESCE(r.next_attempt_at_ms, r.requested_at_ms), r.requested_at_ms, r.id
                 LIMIT 1",
                params![project_id.0.to_string(), environment, now_unix_ms],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(state_error)?;
        let Some(candidate) = candidate else {
            transaction.commit().map_err(state_error)?;
            return Ok(None);
        };
        let id = parse_uuid(&candidate)?;
        let changed = transaction
            .execute(
                "UPDATE job_runs SET
                    state=CASE WHEN state IN ('queued','retry_wait') THEN 'launching' ELSE state END,
                    lease_owner=?2, lease_expires_at_ms=?3, fence_token=fence_token+1
                 WHERE id=?1 AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms<=?4)",
                params![candidate, owner.to_string(), expires, now_unix_ms],
            )
            .map_err(state_error)?;
        if changed != 1 {
            transaction.commit().map_err(state_error)?;
            return Ok(None);
        }
        let (run, job, schedule) = load_claim(&transaction, &self.path, id)?;
        transaction.commit().map_err(state_error)?;
        Ok(Some(ClaimedJobRun {
            run,
            job_definition: job,
            schedule_definition: schedule,
            lease_owner: owner,
        }))
    }

    fn claim_run(
        &self,
        run_id: Uuid,
        owner: Uuid,
        now_unix_ms: i64,
        lease: Duration,
    ) -> Result<Option<ClaimedJobRun>, MmError> {
        let expires = lease_expiry(now_unix_ms, lease)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let changed = transaction
            .execute(
                "UPDATE job_runs SET
                    state=CASE WHEN state IN ('queued','retry_wait') THEN 'launching' ELSE state END,
                    lease_owner=?2, lease_expires_at_ms=?3, fence_token=fence_token+1
                 WHERE id=?1 AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms<=?4)
                   AND (state IN ('launching','running','cancelling','unknown')
                     OR (state IN ('queued','retry_wait')
                         AND (next_attempt_at_ms IS NULL OR next_attempt_at_ms<=?4)
                         AND (schedule_concurrency IS NULL OR schedule_concurrency!='forbid'
                           OR NOT EXISTS (
                              SELECT 1 FROM job_runs active
                              WHERE active.project_id=job_runs.project_id
                                AND active.environment_name=job_runs.environment_name
                                AND active.schedule_name=job_runs.schedule_name
                                AND active.id!=job_runs.id
                                AND active.state IN ('launching','running','cancelling','unknown')))))",
                params![run_id.to_string(), owner.to_string(), expires, now_unix_ms],
            )
            .map_err(state_error)?;
        if changed != 1 {
            transaction.commit().map_err(state_error)?;
            return Ok(None);
        }
        let (run, job, schedule) = load_claim(&transaction, &self.path, run_id)?;
        transaction.commit().map_err(state_error)?;
        Ok(Some(ClaimedJobRun {
            run,
            job_definition: job,
            schedule_definition: schedule,
            lease_owner: owner,
        }))
    }

    fn mark_run_running(
        &self,
        claim: &ClaimedJobRun,
        runtime_id: &str,
        started_at_unix_ms: i64,
    ) -> Result<JobRun, MmError> {
        validate_runtime_id(runtime_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let changed = transaction
            .execute(
                "UPDATE job_runs SET state='running', runtime_id=?4,
                    started_at_ms=COALESCE(started_at_ms, ?5),
                    lease_owner=NULL, lease_expires_at_ms=?5
                 WHERE id=?1 AND lease_owner=?2 AND fence_token=?3
                   AND state IN ('launching','running','unknown')",
                params![
                    claim.run.id.to_string(),
                    claim.lease_owner.to_string(),
                    i64::try_from(claim.run.fence_token).unwrap_or(i64::MAX),
                    runtime_id,
                    started_at_unix_ms,
                ],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return lost_fence();
        }
        transaction
            .execute(
                "INSERT INTO job_attempts (
                    run_id, attempt, fence_token, runtime_id, started_at_ms
                 ) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(run_id, attempt) DO UPDATE SET runtime_id=excluded.runtime_id
                 WHERE job_attempts.fence_token=excluded.fence_token",
                params![
                    claim.run.id.to_string(),
                    claim.run.attempt,
                    i64::try_from(claim.run.fence_token).unwrap_or(i64::MAX),
                    runtime_id,
                    started_at_unix_ms,
                ],
            )
            .map_err(state_error)?;
        let run = load_run(&transaction, claim.run.id)?.ok_or_else(|| MmError::State {
            message: "running job disappeared".to_owned(),
        })?;
        transaction.commit().map_err(state_error)?;
        Ok(run)
    }

    fn complete_run(&self, completion: &JobRunCompletion) -> Result<JobRun, MmError> {
        if !completion.state.is_terminal() || completion.output.len() > 1024 * 1024 {
            return Err(MmError::Security {
                message: "job completion metadata is invalid or unbounded".to_owned(),
            });
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let changed = transaction
            .execute(
                "UPDATE job_runs SET state=?4, finished_at_ms=?5, exit_code=?6,
                    error_code=?7, diagnostic_code=?8, output_text=?9, output_truncated=?10,
                    lease_owner=NULL, lease_expires_at_ms=NULL, next_attempt_at_ms=NULL
                 WHERE id=?1 AND lease_owner=?2 AND fence_token=?3",
                params![
                    completion.run_id.to_string(),
                    completion.lease_owner.to_string(),
                    i64::try_from(completion.fence_token).unwrap_or(i64::MAX),
                    state_name(completion.state),
                    completion.finished_at_unix_ms,
                    completion.exit_code,
                    completion.error_code,
                    completion.diagnostic_code,
                    completion.output,
                    completion.output_truncated,
                ],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return lost_fence();
        }
        transaction
            .execute(
                "UPDATE job_attempts SET finished_at_ms=?3, exit_code=?4,
                    error_code=?5, diagnostic_code=?6
                 WHERE run_id=?1 AND attempt=(SELECT attempt FROM job_runs WHERE id=?1)
                   AND fence_token=?2",
                params![
                    completion.run_id.to_string(),
                    i64::try_from(completion.fence_token).unwrap_or(i64::MAX),
                    completion.finished_at_unix_ms,
                    completion.exit_code,
                    completion.error_code,
                    completion.diagnostic_code,
                ],
            )
            .map_err(state_error)?;
        let run = load_run(&transaction, completion.run_id)?.ok_or_else(|| MmError::State {
            message: "completed job disappeared".to_owned(),
        })?;
        transaction.commit().map_err(state_error)?;
        Ok(run)
    }

    fn schedule_retry(
        &self,
        claim: &ClaimedJobRun,
        next_attempt_at_unix_ms: i64,
        error_code: &str,
    ) -> Result<JobRun, MmError> {
        validate_code(error_code)?;
        if claim.run.attempt >= claim.run.maximum_attempts {
            return Err(MmError::State {
                message: "job run has exhausted its retry attempts".to_owned(),
            });
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let changed = transaction
            .execute(
                "UPDATE job_runs SET state='retry_wait', attempt=attempt+1,
                    next_attempt_at_ms=?4, runtime_id=NULL, error_code=?5,
                    lease_owner=NULL, lease_expires_at_ms=NULL
                 WHERE id=?1 AND lease_owner=?2 AND fence_token=?3",
                params![
                    claim.run.id.to_string(),
                    claim.lease_owner.to_string(),
                    i64::try_from(claim.run.fence_token).unwrap_or(i64::MAX),
                    next_attempt_at_unix_ms,
                    error_code,
                ],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return lost_fence();
        }
        transaction
            .execute(
                "UPDATE job_attempts SET finished_at_ms=?3, error_code=?4
                 WHERE run_id=?1 AND attempt=?2",
                params![
                    claim.run.id.to_string(),
                    claim.run.attempt,
                    next_attempt_at_unix_ms,
                    error_code,
                ],
            )
            .map_err(state_error)?;
        let run = load_run(&transaction, claim.run.id)?.ok_or_else(|| MmError::State {
            message: "retried job disappeared".to_owned(),
        })?;
        transaction.commit().map_err(state_error)?;
        Ok(run)
    }

    fn mark_run_unknown(
        &self,
        claim: &ClaimedJobRun,
        diagnostic_code: &str,
    ) -> Result<JobRun, MmError> {
        validate_code(diagnostic_code)?;
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE job_runs SET state='unknown', diagnostic_code=?4,
                    lease_owner=NULL
                 WHERE id=?1 AND lease_owner=?2 AND fence_token=?3",
                params![
                    claim.run.id.to_string(),
                    claim.lease_owner.to_string(),
                    i64::try_from(claim.run.fence_token).unwrap_or(i64::MAX),
                    diagnostic_code,
                ],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return lost_fence();
        }
        load_run(&connection, claim.run.id)?.ok_or_else(|| MmError::State {
            message: "unknown job run disappeared".to_owned(),
        })
    }

    fn append_job_audit(&self, audit: &JobAuditRecord) -> Result<(), MmError> {
        validate_audit(audit)?;
        append_audit_tx(&*self.connection()?, audit)
    }

    fn prune_history(
        &self,
        project_id: ProjectId,
        environment: &str,
        job: &str,
        retain: usize,
    ) -> Result<usize, MmError> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM job_runs WHERE id IN (
                    SELECT id FROM job_runs
                    WHERE project_id=?1 AND environment_name=?2 AND job_name=?3
                      AND state IN ('succeeded','failed','timed_out','cancelled')
                    ORDER BY requested_at_ms DESC, id DESC LIMIT -1 OFFSET ?4
                 )",
                params![
                    project_id.0.to_string(),
                    environment,
                    job,
                    i64::try_from(retain).unwrap_or(i64::MAX),
                ],
            )
            .map_err(state_error)
    }
} }
