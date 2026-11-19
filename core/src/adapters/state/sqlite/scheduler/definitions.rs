macro_rules! scheduler_definitions { () => {
    fn definition_hashes(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<StoredDefinitionHashes, MmError> {
        let connection = self.connection()?;
        stored_hashes(&connection, project_id, environment)
    }

    fn reconcile(&self, commit: &ReconciliationCommit) -> Result<(), MmError> {
                validate_audit_identity(&commit.principal, &commit.source)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let stored = stored_hashes(
            &transaction,
            commit.plan.project_id,
            &commit.plan.environment,
        )?;
        let expected = reconciliation_plan(
            commit.plan.project_id,
            commit.plan.environment.clone(),
            commit.plan.config_hash.clone(),
            &commit.desired,
            &stored,
        )
        .map_err(|message| MmError::State { message })?;
        let canonical_hash = commit
            .plan
            .calculate_hash()
            .map_err(|message| MmError::State { message })?;
        if expected != commit.plan || canonical_hash != commit.plan.plan_hash {
            return Err(MmError::Security {
                message: "scheduler reconciliation plan is stale or failed canonical verification"
                    .to_owned(),
            });
        }

        reconcile_jobs(&transaction, commit)?;
        retire_absent(
            &transaction,
            "schedules",
            "schedule_name",
            commit.plan.project_id,
            &commit.plan.environment,
            commit.desired.schedules.keys(),
            commit.now_unix_ms,
        )?;
        retire_absent(
            &transaction,
            "scheduled_jobs",
            "job_name",
            commit.plan.project_id,
            &commit.plan.environment,
            commit.desired.jobs.keys(),
            commit.now_unix_ms,
        )?;
        reconcile_schedules(&transaction, commit)?;
        append_reconciliation_audit(&transaction, commit)?;
        transaction.commit().map_err(state_error)
    }

    fn list_jobs(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<DesiredDefinitions, MmError> {
        let connection = self.connection()?;
        let mut jobs = BTreeMap::new();
        let mut statement = connection
            .prepare(
                "SELECT job_name, definition_json FROM scheduled_jobs
                 WHERE project_id=?1 AND environment_name=?2 AND retired_at_ms IS NULL
                 ORDER BY job_name",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![project_id.0.to_string(), environment], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(state_error)?;
        for row in rows {
            let (name, json) = row.map_err(state_error)?;
            jobs.insert(name, deserialize_definition(&self.path, &json, "job")?);
        }
        Ok(DesiredDefinitions {
            jobs,
            schedules: load_schedule_definitions(&connection, &self.path, project_id, environment)?,
        })
    }

    fn list_schedules(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Vec<ScheduleRecord>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT schedule_name, job_name, enabled, retired_at_ms IS NOT NULL,
                        next_due_at_ms, definition_hash, generation
                 FROM schedules WHERE project_id=?1 AND environment_name=?2
                 ORDER BY schedule_name",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![project_id.0.to_string(), environment], |row| {
                Ok(ScheduleRecord {
                    name: row.get(0)?,
                    project_id,
                    environment: environment.to_owned(),
                    job: row.get(1)?,
                    enabled: row.get(2)?,
                    retired: row.get(3)?,
                    next_due_at_unix_ms: row.get(4)?,
                    definition_hash: row.get(5)?,
                    generation: row_u64(row, 6)?,
                })
            })
            .map_err(state_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(state_error)
    }

    fn set_schedule_enabled(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
        enabled: bool,
        next_due_at_unix_ms: Option<i64>,
        audit: &JobAuditRecord,
    ) -> Result<ScheduleRecord, MmError> {
        validate_audit(audit)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let changed = transaction
            .execute(
                "UPDATE schedules SET enabled=?4, next_due_at_ms=?5, generation=generation+1,
                    lease_owner=NULL, lease_expires_at_ms=NULL, updated_at_ms=?6
                 WHERE project_id=?1 AND environment_name=?2 AND schedule_name=?3
                   AND retired_at_ms IS NULL",
                params![
                    project_id.0.to_string(),
                    environment,
                    name,
                    enabled,
                    next_due_at_unix_ms,
                    audit.occurred_at_unix_ms,
                ],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return Err(MmError::Environment {
                message: format!("schedule '{name}' is not active"),
            });
        }
        append_audit_tx(&transaction, audit)?;
        transaction.commit().map_err(state_error)?;
        drop(connection);
        self.list_schedules(project_id, environment)?
            .into_iter()
            .find(|schedule| schedule.name == name)
            .ok_or_else(|| MmError::State {
                message: "updated schedule disappeared".to_owned(),
            })
    }

    fn insert_run(&self, run: &NewJobRun, audit: &JobAuditRecord) -> Result<JobRun, MmError> {
        validate_audit(audit)?;
        validate_new_run(run)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let id = insert_run_tx(&transaction, run)?;
        append_audit_tx(&transaction, audit)?;
        let result = load_run(&transaction, id)?.ok_or_else(|| MmError::State {
            message: "inserted job run disappeared".to_owned(),
        })?;
        transaction.commit().map_err(state_error)?;
        Ok(result)
    }

    fn run(&self, run_id: Uuid) -> Result<Option<JobRun>, MmError> {
        load_run(&*self.connection()?, run_id)
    }

    fn history(
        &self,
        project_id: ProjectId,
        environment: &str,
        job: Option<&str>,
        schedule: Option<&str>,
        limit: usize,
    ) -> Result<Vec<JobRun>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id FROM job_runs
                 WHERE project_id=?1 AND environment_name=?2
                   AND (?3 IS NULL OR job_name=?3)
                   AND (?4 IS NULL OR schedule_name=?4)
                 ORDER BY requested_at_ms DESC, id DESC LIMIT ?5",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id.0.to_string(),
                    environment,
                    job,
                    schedule,
                    i64::try_from(limit).unwrap_or(i64::MAX),
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(state_error)?;
        let mut runs = Vec::new();
        for row in rows {
            let id = parse_uuid(&row.map_err(state_error)?)?;
            if let Some(run) = load_run(&connection, id)? {
                runs.push(run);
            }
        }
        Ok(runs)
    }

    fn request_cancel(&self, run_id: Uuid, audit: &JobAuditRecord) -> Result<JobRun, MmError> {
        validate_audit(audit)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let current = load_run(&transaction, run_id)?.ok_or_else(|| MmError::Environment {
            message: format!("job run '{run_id}' was not found"),
        })?;
        if !current.state.is_terminal() {
            let queued = matches!(current.state, JobRunState::Queued | JobRunState::RetryWait);
            transaction
                .execute(
                    "UPDATE job_runs SET cancellation_requested=1,
                        state=CASE WHEN ?2 THEN 'cancelled' ELSE 'cancelling' END,
                        finished_at_ms=CASE WHEN ?2 THEN ?3 ELSE finished_at_ms END,
                        error_code=CASE WHEN ?2 THEN 'MM_CANCELLED' ELSE error_code END,
                        lease_owner=NULL, lease_expires_at_ms=CASE WHEN ?2 THEN NULL ELSE ?3 END
                     WHERE id=?1",
                    params![run_id.to_string(), queued, audit.occurred_at_unix_ms],
                )
                .map_err(state_error)?;
        }
        append_audit_tx(&transaction, audit)?;
        let result = load_run(&transaction, run_id)?.ok_or_else(|| MmError::State {
            message: "cancelled job run disappeared".to_owned(),
        })?;
        transaction.commit().map_err(state_error)?;
        Ok(result)
    }
} }

fn reconcile_jobs(
    transaction: &rusqlite::Transaction<'_>,
    commit: &ReconciliationCommit,
) -> Result<(), MmError> {
    for (name, definition) in &commit.desired.jobs {
        let json = serialize_definition(definition)?;
        let hash = definition_hash(definition).map_err(|message| MmError::State { message })?;
        transaction.execute(
            "INSERT INTO scheduled_jobs (
                project_id, environment_name, job_name, definition_json,
                definition_hash, config_hash, retired_at_ms, updated_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,NULL,?7)
             ON CONFLICT(project_id, environment_name, job_name) DO UPDATE SET
                definition_json=excluded.definition_json,
                definition_hash=excluded.definition_hash,
                config_hash=excluded.config_hash,
                retired_at_ms=NULL,
                updated_at_ms=excluded.updated_at_ms",
            params![commit.plan.project_id.0.to_string(), commit.plan.environment, name,
                json, hash, commit.plan.config_hash, commit.now_unix_ms],
        ).map_err(state_error)?;
    }
    Ok(())
}

fn reconcile_schedules(
    transaction: &rusqlite::Transaction<'_>,
    commit: &ReconciliationCommit,
) -> Result<(), MmError> {
    for (name, definition) in &commit.desired.schedules {
        let json = serialize_definition(definition)?;
        let hash = definition_hash(definition).map_err(|message| MmError::State { message })?;
        let start = definition.starts_at.as_deref()
            .map(crate::domain::scheduler::parse_instant).transpose()
            .map_err(|message| MmError::Environment { message })?;
        let end = definition.ends_at.as_deref()
            .map(crate::domain::scheduler::parse_instant).transpose()
            .map_err(|message| MmError::Environment { message })?;
        let jitter = definition.jitter.parse().map_err(|message| MmError::Environment { message })?;
        let next = if definition.enabled {
            next_occurrence(&definition.cron, &definition.timezone, commit.now_unix_ms.saturating_sub(1), start, end)
                .map_err(|message| MmError::Environment { message })?
                .map(|next| crate::domain::scheduler::apply_jitter(
                    &format!("{}:{}:{}", commit.plan.project_id.0, commit.plan.environment, name),
                    next,
                    jitter,
                ))
        } else {
            None
        };
        transaction.execute(
            "INSERT INTO schedules (
                project_id, environment_name, schedule_name, job_name, definition_json,
                definition_hash, config_hash, enabled, next_due_at_ms, generation,
                retired_at_ms, updated_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,1,NULL,?10)
             ON CONFLICT(project_id, environment_name, schedule_name) DO UPDATE SET
                job_name=excluded.job_name,
                definition_json=excluded.definition_json,
                definition_hash=excluded.definition_hash,
                config_hash=excluded.config_hash,
                enabled=excluded.enabled,
                next_due_at_ms=CASE
                    WHEN schedules.definition_hash=excluded.definition_hash
                         AND schedules.enabled=excluded.enabled
                    THEN schedules.next_due_at_ms ELSE excluded.next_due_at_ms END,
                generation=schedules.generation + CASE
                    WHEN schedules.definition_hash=excluded.definition_hash THEN 0 ELSE 1 END,
                lease_owner=NULL, lease_expires_at_ms=NULL,
                retired_at_ms=NULL, updated_at_ms=excluded.updated_at_ms",
            params![commit.plan.project_id.0.to_string(), commit.plan.environment, name,
                definition.job, json, hash, commit.plan.config_hash, definition.enabled,
                next, commit.now_unix_ms],
        ).map_err(state_error)?;
    }
    Ok(())
}

fn append_reconciliation_audit(
    transaction: &rusqlite::Transaction<'_>,
    commit: &ReconciliationCommit,
) -> Result<(), MmError> {
    append_audit_tx(transaction, &JobAuditRecord {
        occurred_at_unix_ms: commit.now_unix_ms,
        project_id: commit.plan.project_id,
        environment: commit.plan.environment.clone(),
        principal: commit.principal.clone(),
        source: commit.source.clone(),
        action: "reconcile".to_owned(),
        subject_kind: "definitions".to_owned(),
        subject_id: commit.plan.plan_hash.clone(),
        definition_hash: Some(commit.plan.config_hash.clone()),
        details: BTreeMap::from([(
            "change_count".to_owned(), commit.plan.changes.len().to_string(),
        )]),
    })
}
