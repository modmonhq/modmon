macro_rules! sqlite_workspaces { () => {
    fn list_workspaces(&self, limit: usize) -> Result<Vec<WorkspaceRecord>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, project_id, root, config_path FROM workspaces ORDER BY last_seen_at_ms DESC, id LIMIT ?1",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(
                [i64::try_from(limit).unwrap_or(i64::MAX)],
                workspace_from_row,
            )
            .map_err(state_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(state_error)
    }

    fn register_workspace(
        &self,
        registration: &WorkspaceRegistration,
        now_unix_ms: i64,
    ) -> Result<WorkspaceRecord, MmError> {
        let mut connection = self.connection()?;
        let root = registration.root.to_str().ok_or_else(|| MmError::State {
            message: "workspace roots must be valid Unicode paths".to_owned(),
        })?;
        let config_path = registration
            .config_path
            .to_str()
            .ok_or_else(|| MmError::State {
                message: "configuration paths must be valid Unicode paths".to_owned(),
            })?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let existing = transaction
            .query_row(
                "SELECT id, project_id, root, config_path FROM workspaces WHERE root = ?1",
                params![root],
                workspace_from_row,
            )
            .optional()
            .map_err(state_error)?;

        if let Some(existing) = existing {
            if existing.project_id != registration.project_id {
                return Err(MmError::State {
                    message: format!(
                        "workspace root {} is already registered to a different project",
                        registration.root.display()
                    ),
                });
            }
            transaction
                .execute(
                    "UPDATE workspaces SET config_path = ?1, last_seen_at_ms = ?2 WHERE id = ?3",
                    params![config_path, now_unix_ms, existing.id.0.to_string()],
                )
                .map_err(state_error)?;
            transaction.commit().map_err(state_error)?;
            return Ok(WorkspaceRecord {
                config_path: registration.config_path.clone(),
                ..existing
            });
        }

        let id = WorkspaceId(Uuid::new_v4());
        transaction
            .execute(
                "INSERT INTO workspaces (id, project_id, root, config_path, created_at_ms, last_seen_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    id.0.to_string(),
                    registration.project_id.0.to_string(),
                    root,
                    config_path,
                    now_unix_ms
                ],
            )
            .map_err(state_error)?;
        transaction.commit().map_err(state_error)?;

        Ok(WorkspaceRecord {
            id,
            project_id: registration.project_id,
            root: registration.root.clone(),
            config_path: registration.config_path.clone(),
        })
    }

    fn recent_operations(
        &self,
        workspace_id: WorkspaceId,
        limit: usize,
    ) -> Result<Vec<OperationSummary>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, kind, environment, state, requested_at_ms, finished_at_ms, error_code
                 FROM operations WHERE workspace_id = ?1 ORDER BY requested_at_ms DESC LIMIT ?2",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(
                params![
                    workspace_id.0.to_string(),
                    i64::try_from(limit).unwrap_or(i64::MAX)
                ],
                |row| {
                    let id: String = row.get(0)?;
                    let kind: String = row.get(1)?;
                    let state: String = row.get(3)?;
                    Ok(OperationSummary {
                        id: OperationId(parse_uuid(&id, 0)?),
                        kind: parse_operation_kind(&kind, 1)?,
                        environment: row.get(2)?,
                        state: parse_operation_state(&state, 3)?,
                        requested_at_unix_ms: row.get(4)?,
                        finished_at_unix_ms: row.get(5)?,
                        error_code: row.get(6)?,
                    })
                },
            )
            .map_err(state_error)?;

        rows.collect::<Result<Vec<_>, _>>().map_err(state_error)
    }

    fn running_operations(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<OperationSummary>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, kind, environment, state, requested_at_ms, finished_at_ms, error_code
                 FROM operations WHERE workspace_id = ?1 AND state = 'running'
                 ORDER BY requested_at_ms ASC",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![workspace_id.0.to_string()], |row| {
                let id: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let state: String = row.get(3)?;
                Ok(OperationSummary {
                    id: OperationId(parse_uuid(&id, 0)?),
                    kind: parse_operation_kind(&kind, 1)?,
                    environment: row.get(2)?,
                    state: parse_operation_state(&state, 3)?,
                    requested_at_unix_ms: row.get(4)?,
                    finished_at_unix_ms: row.get(5)?,
                    error_code: row.get(6)?,
                })
            })
            .map_err(state_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(state_error)
    }

    fn running_operation_ids(&self) -> Result<std::collections::HashSet<OperationId>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id FROM operations WHERE state = 'running' ORDER BY id")
            .map_err(state_error)?;
        let rows = statement
            .query_map([], |row| {
                let id: String = row.get(0)?;
                Ok(OperationId(parse_uuid(&id, 0)?))
            })
            .map_err(state_error)?;
        rows.collect::<Result<_, _>>().map_err(state_error)
    }

    fn begin_operation(&self, operation: &NewOperation) -> Result<(), MmError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        transaction
            .execute(
                "INSERT INTO operations (id, workspace_id, kind, environment, state, requested_at_ms, plan_hash)
                 VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6)",
                params![
                    operation.id.0.to_string(),
                    operation.workspace_id.0.to_string(),
                    operation_kind(operation.kind),
                    operation.environment,
                    operation.requested_at_unix_ms,
                    operation.plan_hash
                ],
            )
            .map_err(state_error)?;
        insert_operation_event(
            &transaction,
            operation.id,
            operation.requested_at_unix_ms,
            &OperationEvent::Started {
                operation_id: operation.id,
                kind: operation.kind,
            },
        )?;
        transaction.commit().map_err(state_error)
    }

    fn save_deployment_plan(
        &self,
        operation_id: OperationId,
        plan: &DeploymentPlan,
    ) -> Result<(), MmError> {
        let canonical = plan
            .canonical_hash()
            .map_err(|message| MmError::Security { message })?;
        if canonical != plan.plan_hash {
            return Err(MmError::Security {
                message: "refusing to journal a non-canonical deployment plan".to_owned(),
            });
        }
        let desired_json = serde_json::to_string(plan).map_err(|error| MmError::State {
            message: format!("could not serialize deployment plan snapshot: {error}"),
        })?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let operation = transaction
            .query_row(
                "SELECT operations.plan_hash, operations.environment, workspaces.project_id
                 FROM operations
                 JOIN workspaces ON workspaces.id = operations.workspace_id
                 WHERE operations.id = ?1
                   AND operations.kind = 'deploy'
                   AND operations.state = 'running'",
                params![operation_id.0.to_string()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(state_error)?
            .ok_or_else(|| MmError::State {
                message: format!("operation {} is not a running deployment", operation_id.0),
            })?;
        if operation.0.as_deref() != Some(plan.plan_hash.as_str())
            || operation.1 != plan.environment
            || operation.2 != plan.project_id.0.to_string()
        {
            return Err(MmError::Security {
                message: "deployment snapshot does not match its operation journal".to_owned(),
            });
        }
        transaction
            .execute(
                "INSERT INTO deployment_snapshots (
                    operation_id, desired_json, observed_before_json, observed_after_json,
                    config_hash, rendered_hash
                 ) VALUES (?1, ?2, '{}', NULL, ?3, ?4)",
                params![
                    operation_id.0.to_string(),
                    desired_json,
                    plan.plan_hash,
                    plan.dynamic_config.manifest.digest
                ],
            )
            .map_err(state_error)?;
        transaction.commit().map_err(state_error)
    }

    fn deployment_plan(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<DeploymentPlan>, MmError> {
        let connection = self.connection()?;
        let json = connection
            .query_row(
                "SELECT desired_json FROM deployment_snapshots WHERE operation_id = ?1",
                params![operation_id.0.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(state_error)?;
        Ok(json
            .map(|json| parse_deployment_plan_snapshot(&json))
            .transpose()?
            .flatten())
    }
} }
