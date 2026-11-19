macro_rules! sqlite_builds { () => {
    fn save_image_build_plan(
        &self,
        operation_id: OperationId,
        plan: &ImageBuildPlan,
    ) -> Result<(), MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        let plan_json = serde_json::to_string(plan).map_err(|error| MmError::State {
            message: format!("could not serialize image build plan snapshot: {error}"),
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
                   AND operations.kind = 'build'
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
                message: format!("operation {} is not a running image build", operation_id.0),
            })?;
        if operation.0.as_deref() != Some(plan.plan_hash.as_str())
            || operation.1 != plan.environment
            || operation.2 != plan.project_id.0.to_string()
        {
            return Err(MmError::Security {
                message: "image build snapshot does not match its operation journal".to_owned(),
            });
        }
        transaction
            .execute(
                "INSERT INTO image_build_snapshots (operation_id, plan_json, result_json)
                 VALUES (?1, ?2, NULL)",
                params![operation_id.0.to_string(), plan_json],
            )
            .map_err(state_error)?;
        transaction.commit().map_err(state_error)
    }

    fn save_image_build_result(
        &self,
        operation_id: OperationId,
        result: &ImageBuildResult,
    ) -> Result<(), MmError> {
        if result.operation_id != operation_id || result.images.is_empty() {
            return Err(MmError::Security {
                message: "image build result does not match its operation journal".to_owned(),
            });
        }
        let connection = self.connection()?;
        let plan_json = connection
            .query_row(
                "SELECT plan_json FROM image_build_snapshots WHERE operation_id = ?1",
                params![operation_id.0.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(state_error)?
            .ok_or_else(|| MmError::State {
                message: "image build result has no reviewed plan snapshot".to_owned(),
            })?;
        let plan = parse_image_build_plan(&self.path, &plan_json)?;
        result
            .validate_against(&plan)
            .map_err(|message| MmError::Security { message })?;
        let result_json = serde_json::to_string(result).map_err(|error| MmError::State {
            message: format!("could not serialize image build result snapshot: {error}"),
        })?;
        let changed = connection
            .execute(
                "UPDATE image_build_snapshots
                 SET result_json = ?1
                 WHERE operation_id = ?2
                   AND result_json IS NULL
                   AND EXISTS (
                       SELECT 1 FROM operations
                       WHERE operations.id = image_build_snapshots.operation_id
                         AND operations.kind = 'build'
                         AND operations.state = 'running'
                         AND operations.plan_hash = ?3
                   )",
                params![result_json, operation_id.0.to_string(), result.plan_hash],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return Err(MmError::State {
                message: "image build result could not be attached to its running operation"
                    .to_owned(),
            });
        }
        Ok(())
    }

    fn latest_successful_image_build(
        &self,
        project_id: crate::domain::workspace::ProjectId,
        project_root: &std::path::Path,
        environment: &str,
    ) -> Result<Option<(ImageBuildPlan, ImageBuildResult)>, MmError> {
        self.latest_image_build(
            project_id,
            project_root,
            environment,
            crate::domain::build::ImageBuildFilter::Published,
        )
    }

    fn latest_image_build(
        &self,
        project_id: crate::domain::workspace::ProjectId,
        project_root: &std::path::Path,
        environment: &str,
        filter: crate::domain::build::ImageBuildFilter,
    ) -> Result<Option<(ImageBuildPlan, ImageBuildResult)>, MmError> {
        let project_root = project_root.to_str().ok_or_else(|| MmError::State {
            message: "workspace roots must be valid Unicode paths".to_owned(),
        })?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT operations.id, operations.plan_hash, snapshots.plan_json, snapshots.result_json
                 FROM image_build_snapshots AS snapshots
                 JOIN operations ON operations.id = snapshots.operation_id
                 JOIN workspaces ON workspaces.id = operations.workspace_id
                 WHERE workspaces.project_id = ?1
                   AND workspaces.root = ?2
                   AND operations.environment = ?3
                   AND operations.kind = 'build'
                   AND operations.state = 'succeeded'
                   AND snapshots.result_json IS NOT NULL
                 ORDER BY operations.finished_at_ms DESC, operations.requested_at_ms DESC,
                          snapshots.rowid DESC",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(
                params![project_id.0.to_string(), project_root, environment],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(state_error)?;
        for row in rows {
            let (operation_id, operation_plan_hash, plan_json, result_json) =
                row.map_err(state_error)?;
            let operation_id =
                OperationId(
                    Uuid::parse_str(&operation_id).map_err(|error| MmError::State {
                        message: format!("stored image build operation ID is invalid: {error}"),
                    })?,
                );
            let plan = parse_image_build_plan(&self.path, &plan_json)?;
            let result =
                stored_json::<ImageBuildResult>(&self.path, &result_json, "image build result")?;
            result
                .validate_against(&plan)
                .map_err(|message| MmError::Security { message })?;
            if result.operation_id != operation_id
                || operation_plan_hash.as_deref() != Some(plan.plan_hash.as_str())
                || plan.project_id != project_id
                || plan.environment != environment
            {
                return Err(MmError::Security {
                    message: "stored image build does not match its operation owner".to_owned(),
                });
            }
            if filter.accepts(&plan) {
                return Ok(Some((plan, result)));
            }
        }
        Ok(None)
    }

    fn lookup_reusable_image(
        &self,
        project_id: ProjectId,
        environment: &str,
        fingerprint: &str,
        module: &str,
        variant: Option<&str>,
    ) -> Result<Option<ReusableImage>, MmError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT repository, digest
                 FROM image_build_unit_cache
                 WHERE project_id = ?1
                   AND environment = ?2
                   AND fingerprint = ?3
                   AND module = ?4
                   AND variant_target = ?5",
                params![
                    project_id.0.to_string(),
                    environment,
                    fingerprint,
                    module,
                    variant.unwrap_or("")
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(state_error)?;
        Ok(row.map(|(repository, digest)| ReusableImage { repository, digest }))
    }

    fn record_reusable_image(&self, record: ReusableImageRecord<'_>) -> Result<(), MmError> {
        if !crate::domain::build::is_sha256_digest(record.digest) {
            return Err(MmError::Security {
                message: "reusable image digest is invalid".to_owned(),
            });
        }
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO image_build_unit_cache (
                    project_id, environment, fingerprint, module, variant_target,
                    repository, digest, operation_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(project_id, environment, fingerprint, module, variant_target)
                 DO UPDATE SET
                    repository = excluded.repository,
                    digest = excluded.digest,
                    operation_id = excluded.operation_id",
                params![
                    record.project_id.0.to_string(),
                    record.environment,
                    record.fingerprint,
                    record.module,
                    record.variant.unwrap_or(""),
                    record.repository,
                    record.digest,
                    record.operation_id.0.to_string()
                ],
            )
            .map_err(state_error)?;
        Ok(())
    }
} }

fn parse_image_build_plan(path: &Path, plan_json: &str) -> Result<ImageBuildPlan, MmError> {
    let stored = stored_json::<serde_json::Value>(path, plan_json, "image build plan")?;
    known_schema(path, &stored, "image build plan", PLAN_SCHEMA)?;
    stored_json(path, plan_json, "image build plan")
}
