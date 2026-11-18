use super::*;

impl EnvironmentValueStore for SqliteStateStore {
    fn list_environment_overrides(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
    ) -> Result<Vec<EnvironmentValueOverride>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT global_key, value, revision, updated_at_ms
                 FROM environment_value_overrides
                 WHERE workspace_id = ?1 AND environment = ?2
                 ORDER BY global_key",
            )
            .map_err(state_error)?;
        let rows = statement
            .query_map(params![workspace_id.0.to_string(), environment], |row| {
                Ok(EnvironmentValueOverride {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    revision: row.get(2)?,
                    updated_at_unix_ms: row.get(3)?,
                })
            })
            .map_err(state_error)?;
        let values = rows.collect::<Result<Vec<_>, _>>().map_err(state_error)?;
        if let Some(invalid) = values
            .iter()
            .find(|value| !is_safe_environment_value(&value.value))
        {
            return Err(MmError::State {
                message: format!(
                    "stored environment override '{}' contains unsupported control characters; remove it with `mm env unset {}`",
                    invalid.key, invalid.key
                ),
            });
        }
        Ok(values)
    }

    fn set_environment_override(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
        key: &str,
        value: &str,
        now_unix_ms: i64,
        expected_revision: Option<i64>,
    ) -> Result<EnvironmentValueOverride, MmError> {
        if !is_safe_environment_value(value) {
            return Err(MmError::State {
                message: "environment override values may not contain control characters"
                    .to_owned(),
            });
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let current = transaction
            .query_row(
                "SELECT revision FROM environment_value_overrides
                 WHERE workspace_id = ?1 AND environment = ?2 AND global_key = ?3",
                params![workspace_id.0.to_string(), environment, key],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(state_error)?;
        if expected_revision.is_some() && expected_revision != current {
            return Err(MmError::State {
                message: format!(
                    "environment override '{key}' changed concurrently (expected {expected_revision:?}, found {current:?})"
                ),
            });
        }
        let previous_revision = transaction
            .query_row(
                "SELECT revision FROM environment_override_revisions
                 WHERE workspace_id = ?1 AND environment = ?2 AND global_key = ?3",
                params![workspace_id.0.to_string(), environment, key],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(state_error)?;
        let revision = previous_revision.map_or(Ok(1), |revision| {
            revision.checked_add(1).ok_or_else(|| MmError::State {
                message: format!("environment override '{key}' exhausted its revision counter"),
            })
        })?;
        transaction
            .execute(
                "INSERT INTO environment_override_revisions
                    (workspace_id, environment, global_key, revision)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(workspace_id, environment, global_key) DO UPDATE SET
                    revision = excluded.revision",
                params![workspace_id.0.to_string(), environment, key, revision],
            )
            .map_err(state_error)?;
        transaction
            .execute(
                "INSERT INTO environment_value_overrides
                    (workspace_id, environment, global_key, value, revision, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(workspace_id, environment, global_key) DO UPDATE SET
                    value = excluded.value,
                    revision = excluded.revision,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    workspace_id.0.to_string(),
                    environment,
                    key,
                    value,
                    revision,
                    now_unix_ms
                ],
            )
            .map_err(state_error)?;
        transaction.commit().map_err(state_error)?;
        Ok(EnvironmentValueOverride {
            key: key.to_owned(),
            value: value.to_owned(),
            revision,
            updated_at_unix_ms: now_unix_ms,
        })
    }

    fn unset_environment_override(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
        key: &str,
    ) -> Result<bool, MmError> {
        let removed = self
            .connection()?
            .execute(
                "DELETE FROM environment_value_overrides
                 WHERE workspace_id = ?1 AND environment = ?2 AND global_key = ?3",
                params![workspace_id.0.to_string(), environment, key],
            )
            .map_err(state_error)?;
        Ok(removed == 1)
    }
}
