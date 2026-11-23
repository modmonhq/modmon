macro_rules! sqlite_deployments { () => {
    fn save_target_bootstrap_plan(
        &self,
        operation_id: OperationId,
        plan: &TargetBootstrapPlan,
    ) -> Result<(), MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        let plan_json = serde_json::to_string(plan).map_err(|error| MmError::State {
            message: format!("could not serialize target bootstrap plan snapshot: {error}"),
        })?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let operation = transaction
            .query_row(
                "SELECT operations.plan_hash, workspaces.project_id
                 FROM operations
                 JOIN workspaces ON workspaces.id = operations.workspace_id
                 WHERE operations.id = ?1
                   AND operations.kind = 'bootstrap'
                   AND operations.state = 'running'",
                params![operation_id.0.to_string()],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(state_error)?
            .ok_or_else(|| MmError::State {
                message: format!(
                    "operation {} is not a running target bootstrap",
                    operation_id.0
                ),
            })?;
        if operation.0.as_deref() != Some(plan.plan_hash.as_str())
            || operation.1 != plan.project_id.0.to_string()
        {
            return Err(MmError::Security {
                message: "target bootstrap snapshot does not match its operation journal"
                    .to_owned(),
            });
        }
        transaction
            .execute(
                "INSERT INTO target_bootstrap_snapshots (operation_id, plan_json, result_json)
                 VALUES (?1, ?2, NULL)",
                params![operation_id.0.to_string(), plan_json],
            )
            .map_err(state_error)?;
        transaction.commit().map_err(state_error)
    }

    fn save_target_bootstrap_result(
        &self,
        operation_id: OperationId,
        result: &TargetBootstrapResult,
    ) -> Result<(), MmError> {
        if result.operation_id != operation_id || result.verified_capabilities.is_empty() {
            return Err(MmError::Security {
                message: "target bootstrap result does not match its operation journal".to_owned(),
            });
        }
        let result_json = serde_json::to_string(result).map_err(|error| MmError::State {
            message: format!("could not serialize target bootstrap result snapshot: {error}"),
        })?;
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE target_bootstrap_snapshots
                 SET result_json = ?1
                 WHERE operation_id = ?2
                   AND result_json IS NULL
                   AND EXISTS (
                       SELECT 1 FROM operations
                       WHERE operations.id = target_bootstrap_snapshots.operation_id
                         AND operations.kind = 'bootstrap'
                         AND operations.state = 'running'
                         AND operations.plan_hash = ?3
                   )",
                params![result_json, operation_id.0.to_string(), result.plan_hash],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return Err(MmError::State {
                message: "target bootstrap result could not be attached to its running operation"
                    .to_owned(),
            });
        }
        Ok(())
    }

    fn target_bootstrap_plan(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<TargetBootstrapPlan>, MmError> {
        let connection = self.connection()?;
        let json = connection
            .query_row(
                "SELECT plan_json FROM target_bootstrap_snapshots WHERE operation_id = ?1",
                params![operation_id.0.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(state_error)?;
        json.map(|json| {
            let plan =
                stored_json::<TargetBootstrapPlan>(&self.path, &json, "target bootstrap plan")?;
            plan.validate()
                .map_err(|message| MmError::Security { message })?;
            Ok(plan)
        })
        .transpose()
    }

    fn latest_successful_deployment(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Option<DeploymentPlan>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT snapshots.desired_json
                 FROM deployment_snapshots AS snapshots
                 JOIN operations ON operations.id = snapshots.operation_id
                 WHERE operations.workspace_id = ?1
                   AND operations.environment = ?2
                   AND operations.kind = 'deploy'
                   AND operations.state = 'succeeded'
                 ORDER BY operations.finished_at_ms DESC, operations.requested_at_ms DESC,
                          snapshots.rowid DESC",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![workspace_id.0.to_string(), environment], |row| {
                row.get::<_, String>(0)
            })
            .map_err(state_error)?;
        for row in rows {
            let json = row.map_err(state_error)?;
            if let Some(plan) = parse_deployment_plan_snapshot(&self.path, &json)?
                && plan.target_id == target_id
            {
                return Ok(Some(plan));
            }
        }
        Ok(None)
    }

    fn latest_successful_deployment_for_owner(
        &self,
        project_id: crate::domain::workspace::ProjectId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Option<DeploymentPlan>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT snapshots.desired_json
                 FROM deployment_snapshots AS snapshots
                 JOIN operations ON operations.id = snapshots.operation_id
                 JOIN workspaces ON workspaces.id = operations.workspace_id
                 WHERE workspaces.project_id = ?1
                   AND operations.environment = ?2
                   AND operations.kind = 'deploy'
                   AND operations.state = 'succeeded'
                 ORDER BY operations.finished_at_ms DESC, operations.requested_at_ms DESC,
                          snapshots.rowid DESC",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![project_id.0.to_string(), environment], |row| {
                row.get::<_, String>(0)
            })
            .map_err(state_error)?;
        for row in rows {
            let json = row.map_err(state_error)?;
            if let Some(plan) = parse_deployment_plan_snapshot(&self.path, &json)?
                && plan.target_id == target_id
            {
                return Ok(Some(plan));
            }
        }
        Ok(None)
    }

    fn deployment_candidates_for_owner(
        &self,
        project_id: crate::domain::workspace::ProjectId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Vec<(OperationState, DeploymentPlan)>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT operations.state, snapshots.desired_json
                 FROM deployment_snapshots AS snapshots
                 JOIN operations ON operations.id = snapshots.operation_id
                 JOIN workspaces ON workspaces.id = operations.workspace_id
                 WHERE workspaces.project_id = ?1
                   AND operations.environment = ?2
                   AND operations.kind = 'deploy'
                   AND operations.state IN ('succeeded', 'running', 'failed')
                 ORDER BY operations.requested_at_ms DESC, snapshots.rowid DESC",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![project_id.0.to_string(), environment], |row| {
                let state = row.get::<_, String>(0)?;
                Ok((parse_operation_state(&state, 0)?, row.get::<_, String>(1)?))
            })
            .map_err(state_error)?;
        let mut candidates = Vec::new();
        for row in rows {
            let (state, json) = row.map_err(state_error)?;
            if let Some(plan) = parse_deployment_plan_snapshot(&self.path, &json)?
                && plan.target_id == target_id
            {
                candidates.push((state, plan));
            }
        }
        Ok(candidates)
    }

    fn running_deployments_for_owner(
        &self,
        project_id: crate::domain::workspace::ProjectId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Vec<(OperationId, DeploymentPlan)>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT operations.id, snapshots.desired_json
                 FROM deployment_snapshots AS snapshots
                 JOIN operations ON operations.id = snapshots.operation_id
                 JOIN workspaces ON workspaces.id = operations.workspace_id
                 WHERE workspaces.project_id = ?1
                   AND operations.environment = ?2
                   AND operations.kind = 'deploy'
                   AND operations.state = 'running'
                 ORDER BY operations.requested_at_ms DESC, snapshots.rowid DESC",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![project_id.0.to_string(), environment], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(state_error)?;
        let mut running = Vec::new();
        for row in rows {
            let (id, json) = row.map_err(state_error)?;
            let id = uuid::Uuid::parse_str(&id).map_err(|error| MmError::State {
                message: format!("the operation journal holds an invalid operation id: {error}"),
            })?;
            if let Some(plan) = parse_deployment_plan_snapshot(&self.path, &json)?
                && plan.target_id == target_id
            {
                running.push((OperationId(id), plan));
            }
        }
        Ok(running)
    }
} }
