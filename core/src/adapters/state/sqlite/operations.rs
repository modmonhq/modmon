macro_rules! sqlite_operations { () => {
    fn local_project_preference(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<crate::app::LocalSelectionPreference>, MmError> {
        let connection = self.connection()?;
        read_preference(
            &connection,
            "SELECT runtime, target FROM local_project_preferences WHERE project_id = ?1",
            params![project_id.0.to_string()],
        )
    }

    fn local_environment_preference(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Option<crate::app::LocalSelectionPreference>, MmError> {
        let connection = self.connection()?;
        read_preference(
            &connection,
            "SELECT runtime, target FROM local_environment_preferences
             WHERE project_id = ?1 AND environment = ?2",
            params![project_id.0.to_string(), environment],
        )
    }

    fn set_local_project_preference(
        &self,
        project_id: ProjectId,
        runtime: Option<crate::domain::runtime::RuntimeKind>,
        target: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        self.connection()?
            .execute(
                "INSERT INTO local_project_preferences (project_id, runtime, target, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(project_id) DO UPDATE SET
                    runtime = excluded.runtime,
                    target = excluded.target,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    project_id.0.to_string(),
                    runtime.map(runtime_kind_sql),
                    target,
                    now_unix_ms
                ],
            )
            .map_err(state_error)?;
        Ok(())
    }

    fn set_local_environment_preference(
        &self,
        project_id: ProjectId,
        environment: &str,
        runtime: Option<crate::domain::runtime::RuntimeKind>,
        target: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        self.connection()?
            .execute(
                "INSERT INTO local_environment_preferences
                    (project_id, environment, runtime, target, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(project_id, environment) DO UPDATE SET
                    runtime = excluded.runtime,
                    target = excluded.target,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    project_id.0.to_string(),
                    environment,
                    runtime.map(runtime_kind_sql),
                    target,
                    now_unix_ms
                ],
            )
            .map_err(state_error)?;
        Ok(())
    }

    fn unset_local_project_preference(&self, project_id: ProjectId) -> Result<bool, MmError> {
        let removed = self
            .connection()?
            .execute(
                "DELETE FROM local_project_preferences WHERE project_id = ?1",
                params![project_id.0.to_string()],
            )
            .map_err(state_error)?;
        Ok(removed == 1)
    }

    fn unset_local_environment_preference(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<bool, MmError> {
        let removed = self
            .connection()?
            .execute(
                "DELETE FROM local_environment_preferences
                 WHERE project_id = ?1 AND environment = ?2",
                params![project_id.0.to_string(), environment],
            )
            .map_err(state_error)?;
        Ok(removed == 1)
    }

    fn append_event(
        &self,
        operation_id: OperationId,
        occurred_at_unix_ms: i64,
        event: &OperationEvent,
    ) -> Result<(), MmError> {
        if event_operation_id(event) != operation_id {
            return Err(MmError::State {
                message: "operation event ID does not match its journal operation".to_owned(),
            });
        }
        if !matches!(event, OperationEvent::PhaseChanged { .. }) {
            return Err(MmError::State {
                message: "operation lifecycle events are written by begin/finish transitions"
                    .to_owned(),
            });
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let running = transaction
            .query_row(
                "SELECT 1 FROM operations WHERE id = ?1 AND state = 'running'",
                params![operation_id.0.to_string()],
                |_| Ok(()),
            )
            .optional()
            .map_err(state_error)?
            .is_some();
        if !running {
            return Err(MmError::State {
                message: format!("operation {} is not running", operation_id.0),
            });
        }
        insert_operation_event(&transaction, operation_id, occurred_at_unix_ms, event)?;
        transaction.commit().map_err(state_error)
    }

    fn finish_operation(
        &self,
        operation_id: OperationId,
        state: OperationState,
        finished_at_unix_ms: i64,
        error_code: Option<&str>,
    ) -> Result<(), MmError> {
        if !matches!(
            state,
            OperationState::Succeeded
                | OperationState::Failed
                | OperationState::Cancelled
                | OperationState::Interrupted
        ) {
            return Err(MmError::State {
                message: "an operation may only finish in a terminal state".to_owned(),
            });
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let changed = transaction
            .execute(
                "UPDATE operations SET state = ?1, finished_at_ms = ?2, error_code = ?3
                 WHERE id = ?4 AND state = 'running'",
                params![
                    operation_state(state),
                    finished_at_unix_ms,
                    error_code,
                    operation_id.0.to_string()
                ],
            )
            .map_err(state_error)?;
        if changed == 0 {
            return Err(MmError::State {
                message: format!(
                    "operation {} does not exist or is not running",
                    operation_id.0
                ),
            });
        }
        insert_operation_event(
            &transaction,
            operation_id,
            finished_at_unix_ms,
            &OperationEvent::Completed {
                operation_id,
                state,
            },
        )?;
        transaction.commit().map_err(state_error)
    }

    fn try_acquire_lock(
        &self,
        resource: &str,
        owner: OperationId,
        now_unix_ms: i64,
        lease: Duration,
    ) -> Result<bool, MmError> {
        let lease_ms = i64::try_from(lease.as_millis()).unwrap_or(i64::MAX);
        let expires_at = now_unix_ms.saturating_add(lease_ms);
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(state_error)?;
        transaction
            .execute(
                "DELETE FROM resource_locks WHERE resource = ?1 AND expires_at_ms <= ?2",
                params![resource, now_unix_ms],
            )
            .map_err(state_error)?;
        let inserted = transaction
            .execute(
                "INSERT OR IGNORE INTO resource_locks (resource, owner_operation_id, acquired_at_ms, expires_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![resource, owner.0.to_string(), now_unix_ms, expires_at],
            )
            .map_err(state_error)?;
        transaction.commit().map_err(state_error)?;
        Ok(inserted == 1)
    }

    fn release_lock(&self, resource: &str, owner: OperationId) -> Result<(), MmError> {
        self.connection()?
            .execute(
                "DELETE FROM resource_locks WHERE resource = ?1 AND owner_operation_id = ?2",
                params![resource, owner.0.to_string()],
            )
            .map_err(state_error)?;
        Ok(())
    }
} }
