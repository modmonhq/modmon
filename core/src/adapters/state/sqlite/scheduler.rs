use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use uuid::Uuid;

use super::{SqliteStateStore, state_error, stored_json};
use crate::app::ports::SchedulerRepository;
use crate::domain::config::{JobConfig, ScheduleConcurrency, ScheduleConfig};
use crate::domain::error::MmError;
use crate::domain::scheduler::{
    ClaimedJobRun, ClaimedSchedule, DesiredDefinitions, JobAuditRecord, JobRun, JobRunCompletion,
    JobRunState, JobTrigger, NewJobRun, ReconciliationCommit, ScheduleRecord, ScheduleTickCommit,
    StoredDefinitionHashes, definition_hash, next_occurrence, reconciliation_plan,
    runtime_job_name,
};
use crate::domain::workspace::ProjectId;

include!("scheduler/definitions.rs");
include!("scheduler/ticks.rs");
include!("scheduler/runs.rs");

impl SchedulerRepository for SqliteStateStore {
    scheduler_definitions!();
    scheduler_ticks!();
    scheduler_runs!();
}

fn stored_hashes(
    connection: &Connection,
    project_id: ProjectId,
    environment: &str,
) -> Result<StoredDefinitionHashes, MmError> {
    Ok(StoredDefinitionHashes {
        jobs: hash_map(
            connection,
            "scheduled_jobs",
            "job_name",
            project_id,
            environment,
        )?,
        schedules: hash_map(
            connection,
            "schedules",
            "schedule_name",
            project_id,
            environment,
        )?,
    })
}

fn hash_map(
    connection: &Connection,
    table: &str,
    name_column: &str,
    project_id: ProjectId,
    environment: &str,
) -> Result<BTreeMap<String, String>, MmError> {
    let sql = format!(
        "SELECT {name_column}, definition_hash FROM {table}
         WHERE project_id=?1 AND environment_name=?2 AND retired_at_ms IS NULL ORDER BY {name_column}"
    );
    let mut statement = connection.prepare(&sql).map_err(state_error)?;
    let rows = statement
        .query_map(params![project_id.0.to_string(), environment], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(state_error)?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(state_error)
}

fn retire_absent<'a>(
    transaction: &Transaction<'_>,
    table: &str,
    name_column: &str,
    project_id: ProjectId,
    environment: &str,
    desired: impl Iterator<Item = &'a String>,
    now_unix_ms: i64,
) -> Result<(), MmError> {
    let desired = desired.cloned().collect::<Vec<_>>();
    let sql = format!(
        "SELECT {name_column} FROM {table}
         WHERE project_id=?1 AND environment_name=?2 AND retired_at_ms IS NULL"
    );
    let mut statement = transaction.prepare(&sql).map_err(state_error)?;
    let existing = statement
        .query_map(params![project_id.0.to_string(), environment], |row| {
            row.get::<_, String>(0)
        })
        .map_err(state_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(state_error)?;
    drop(statement);
    let update = format!(
        "UPDATE {table} SET retired_at_ms=?4, updated_at_ms=?4
         WHERE project_id=?1 AND environment_name=?2 AND {name_column}=?3"
    );
    for name in existing {
        if !desired.contains(&name) {
            transaction
                .execute(
                    &update,
                    params![project_id.0.to_string(), environment, name, now_unix_ms],
                )
                .map_err(state_error)?;
        }
    }
    Ok(())
}

fn load_schedule_definitions(
    connection: &Connection,
    path: &Path,
    project_id: ProjectId,
    environment: &str,
) -> Result<BTreeMap<String, ScheduleConfig>, MmError> {
    let mut schedules = BTreeMap::new();
    let mut statement = connection
        .prepare(
            "SELECT schedule_name, definition_json FROM schedules
             WHERE project_id=?1 AND environment_name=?2 AND retired_at_ms IS NULL
             ORDER BY schedule_name",
        )
        .map_err(state_error)?;
    let rows = statement
        .query_map(params![project_id.0.to_string(), environment], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(state_error)?;
    for row in rows {
        let (name, json) = row.map_err(state_error)?;
        schedules.insert(name, deserialize_definition(path, &json, "schedule")?);
    }
    Ok(schedules)
}

fn insert_run_tx(transaction: &Transaction<'_>, run: &NewJobRun) -> Result<Uuid, MmError> {
    validate_new_run(run)?;
    let retry = run
        .schedule_definition
        .as_ref()
        .and_then(|schedule| schedule.retry.as_ref())
        .unwrap_or(&run.job_definition.retry);
    let runtime_name = runtime_job_name(run.project_id, run.id);
    transaction
        .execute(
            "INSERT INTO job_runs (
                id, project_id, environment_name, job_name, schedule_name, schedule_concurrency, trigger_kind,
                idempotency_key, job_definition_json, schedule_definition_json, config_hash,
                state, attempt, maximum_attempts, runtime_name, requested_at_ms,
                scheduled_for_ms, principal, source, retry_of_run_id
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'queued',1,?12,?13,?14,?15,?16,?17,?18)
             ON CONFLICT(idempotency_key) DO NOTHING",
            params![
                run.id.to_string(),
                run.project_id.0.to_string(),
                run.environment,
                run.job,
                run.schedule,
                run.schedule_definition
                    .as_ref()
                    .map(|schedule| concurrency_name(schedule.concurrency)),
                trigger_name(run.trigger),
                run.idempotency_key,
                serialize_definition(&run.job_definition)?,
                run.schedule_definition
                    .as_ref()
                    .map(serialize_definition)
                    .transpose()?,
                run.config_hash,
                retry.attempts,
                runtime_name,
                run.requested_at_unix_ms,
                run.scheduled_for_unix_ms,
                run.principal,
                run.source,
                run.retry_of_run_id.map(|id| id.to_string()),
            ],
        )
        .map_err(state_error)?;
    let id = transaction
        .query_row(
            "SELECT id FROM job_runs WHERE idempotency_key=?1",
            [&run.idempotency_key],
            |row| row.get::<_, String>(0),
        )
        .map_err(state_error)?;
    parse_uuid(&id)
}

fn load_claim(
    connection: &Connection,
    path: &Path,
    id: Uuid,
) -> Result<(JobRun, JobConfig, Option<ScheduleConfig>), MmError> {
    let (job, schedule) = connection
        .query_row(
            "SELECT job_definition_json, schedule_definition_json FROM job_runs WHERE id=?1",
            [id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map_err(state_error)?;
    let run = load_run(connection, id)?.ok_or_else(|| MmError::State {
        message: "claimed job run disappeared".to_owned(),
    })?;
    Ok((
        run,
        deserialize_definition(path, &job, "job")?,
        schedule
            .as_deref()
            .map(|json| deserialize_definition(path, json, "schedule"))
            .transpose()?,
    ))
}

fn load_run(connection: &Connection, id: Uuid) -> Result<Option<JobRun>, MmError> {
    connection
        .query_row(
            "SELECT project_id, environment_name, job_name, schedule_name, trigger_kind,
                    state, attempt, maximum_attempts, fence_token, runtime_name, runtime_id,
                    requested_at_ms, scheduled_for_ms, started_at_ms, finished_at_ms,
                    next_attempt_at_ms, exit_code, error_code, output_text, output_truncated,
                    cancellation_requested, principal, source
             FROM job_runs WHERE id=?1",
            [id.to_string()],
            |row| {
                let project = row.get::<_, String>(0)?;
                let trigger = row.get::<_, String>(4)?;
                let state = row.get::<_, String>(5)?;
                Ok((
                    project,
                    trigger,
                    state,
                    JobRun {
                        id,
                        project_id: ProjectId(Uuid::nil()),
                        environment: row.get(1)?,
                        job: row.get(2)?,
                        schedule: row.get(3)?,
                        trigger: JobTrigger::Manual,
                        state: JobRunState::Unknown,
                        attempt: row_u16(row, 6)?,
                        maximum_attempts: row_u16(row, 7)?,
                        fence_token: row_u64(row, 8)?,
                        runtime_name: row.get(9)?,
                        runtime_id: row.get(10)?,
                        requested_at_unix_ms: row.get(11)?,
                        scheduled_for_unix_ms: row.get(12)?,
                        started_at_unix_ms: row.get(13)?,
                        finished_at_unix_ms: row.get(14)?,
                        next_attempt_at_unix_ms: row.get(15)?,
                        exit_code: row.get(16)?,
                        error_code: row.get(17)?,
                        output: row.get(18)?,
                        output_truncated: row.get(19)?,
                        cancellation_requested: row.get(20)?,
                        principal: row.get(21)?,
                        source: row.get(22)?,
                        node: None,
                    },
                ))
            },
        )
        .optional()
        .map_err(state_error)?
        .map(|(project, trigger, state, mut run)| {
            run.project_id = ProjectId(parse_uuid(&project)?);
            run.trigger = parse_trigger(&trigger)?;
            run.state = parse_state(&state)?;
            Ok(run)
        })
        .transpose()
}

fn row_u16(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u16> {
    let value = row.get::<_, i64>(index)?;
    u16::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn row_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(index)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn append_audit_tx(connection: &Connection, audit: &JobAuditRecord) -> Result<(), MmError> {
    validate_audit(audit)?;
    let details = serde_json::to_string(&audit.details).map_err(|error| MmError::State {
        message: format!("could not serialize job audit metadata: {error}"),
    })?;
    if details.len() > 4096 {
        return Err(MmError::Security {
            message: "job audit metadata exceeds 4096 bytes".to_owned(),
        });
    }
    connection
        .execute(
            "INSERT INTO job_audit (
                occurred_at_ms, project_id, environment_name, principal, source,
                action, subject_kind, subject_id, definition_hash, details_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                audit.occurred_at_unix_ms,
                audit.project_id.0.to_string(),
                audit.environment,
                audit.principal,
                audit.source,
                audit.action,
                audit.subject_kind,
                audit.subject_id,
                audit.definition_hash,
                details,
            ],
        )
        .map_err(state_error)?;
    Ok(())
}

fn validate_new_run(run: &NewJobRun) -> Result<(), MmError> {
    validate_audit_identity(&run.principal, &run.source)?;
    if run.job.is_empty()
        || run.idempotency_key.is_empty()
        || run.idempotency_key.len() > 512
        || run.config_hash.len() != 64
    {
        return Err(MmError::Security {
            message: "job run identity metadata is invalid".to_owned(),
        });
    }
    Ok(())
}

fn validate_audit(audit: &JobAuditRecord) -> Result<(), MmError> {
    validate_audit_identity(&audit.principal, &audit.source)?;
    if [&audit.action, &audit.subject_kind, &audit.subject_id]
        .iter()
        .any(|value| value.is_empty() || value.len() > 512 || value.chars().any(char::is_control))
        || audit.details.iter().any(|(key, value)| {
            key.len() > 128
                || value.len() > 512
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
        })
    {
        return Err(MmError::Security {
            message: "job audit metadata is invalid".to_owned(),
        });
    }
    Ok(())
}

fn validate_audit_identity(principal: &str, source: &str) -> Result<(), MmError> {
    if principal.is_empty()
        || principal.len() > 255
        || source.is_empty()
        || source.len() > 255
        || principal.chars().any(char::is_control)
        || source.chars().any(char::is_control)
    {
        return Err(MmError::Security {
            message: "job operation requires a valid authenticated principal and source".to_owned(),
        });
    }
    Ok(())
}

fn validate_runtime_id(value: &str) -> Result<(), MmError> {
    if value.is_empty()
        || value.len() > 255
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        Err(MmError::Security {
            message: "runtime job identity is invalid".to_owned(),
        })
    } else {
        Ok(())
    }
}

fn validate_code(value: &str) -> Result<(), MmError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        Err(MmError::Security {
            message: "job diagnostic code is invalid".to_owned(),
        })
    } else {
        Ok(())
    }
}

fn serialize_definition<T: serde::Serialize>(value: &T) -> Result<String, MmError> {
    serde_json::to_string(value).map_err(|error| MmError::State {
        message: format!("could not serialize scheduler definition: {error}"),
    })
}

fn deserialize_definition<T: serde::de::DeserializeOwned>(
    path: &Path,
    value: &str,
    kind: &str,
) -> Result<T, MmError> {
    stored_json(path, value, &format!("{kind} definition"))
}

fn parse_uuid(value: &str) -> Result<Uuid, MmError> {
    Uuid::parse_str(value).map_err(|_| MmError::Security {
        message: "stored scheduler identity is invalid".to_owned(),
    })
}

const fn trigger_name(value: JobTrigger) -> &'static str {
    match value {
        JobTrigger::Manual => "manual",
        JobTrigger::Schedule => "schedule",
        JobTrigger::RunNow => "run_now",
        JobTrigger::Retry => "retry",
        JobTrigger::BeforeDeploy => "before_deploy",
        JobTrigger::AfterDeploy => "after_deploy",
        JobTrigger::Once => "once",
    }
}

const fn concurrency_name(value: ScheduleConcurrency) -> &'static str {
    match value {
        ScheduleConcurrency::Allow => "allow",
        ScheduleConcurrency::Forbid => "forbid",
        ScheduleConcurrency::Replace => "replace",
    }
}

fn parse_trigger(value: &str) -> Result<JobTrigger, MmError> {
    match value {
        "manual" => Ok(JobTrigger::Manual),
        "schedule" => Ok(JobTrigger::Schedule),
        "run_now" => Ok(JobTrigger::RunNow),
        "retry" => Ok(JobTrigger::Retry),
        "before_deploy" => Ok(JobTrigger::BeforeDeploy),
        "after_deploy" => Ok(JobTrigger::AfterDeploy),
        "once" => Ok(JobTrigger::Once),
        _ => Err(MmError::Security {
            message: "stored job trigger is invalid".to_owned(),
        }),
    }
}

const fn state_name(value: JobRunState) -> &'static str {
    match value {
        JobRunState::Queued => "queued",
        JobRunState::Launching => "launching",
        JobRunState::Running => "running",
        JobRunState::RetryWait => "retry_wait",
        JobRunState::Cancelling => "cancelling",
        JobRunState::Succeeded => "succeeded",
        JobRunState::Failed => "failed",
        JobRunState::TimedOut => "timed_out",
        JobRunState::Cancelled => "cancelled",
        JobRunState::Unknown => "unknown",
    }
}

fn parse_state(value: &str) -> Result<JobRunState, MmError> {
    match value {
        "queued" => Ok(JobRunState::Queued),
        "launching" => Ok(JobRunState::Launching),
        "running" => Ok(JobRunState::Running),
        "retry_wait" => Ok(JobRunState::RetryWait),
        "cancelling" => Ok(JobRunState::Cancelling),
        "succeeded" => Ok(JobRunState::Succeeded),
        "failed" => Ok(JobRunState::Failed),
        "timed_out" => Ok(JobRunState::TimedOut),
        "cancelled" => Ok(JobRunState::Cancelled),
        "unknown" => Ok(JobRunState::Unknown),
        _ => Err(MmError::Security {
            message: "stored job state is invalid".to_owned(),
        }),
    }
}

fn lease_expiry(now_unix_ms: i64, lease: Duration) -> Result<i64, MmError> {
    let lease = i64::try_from(lease.as_millis()).map_err(|_| MmError::State {
        message: "scheduler lease exceeds the supported range".to_owned(),
    })?;
    if lease <= 0 {
        return Err(MmError::State {
            message: "scheduler lease must be positive".to_owned(),
        });
    }
    Ok(now_unix_ms.saturating_add(lease))
}

fn lost_fence<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "job run lease was lost to a newer fence token".to_owned(),
    })
}

#[cfg(test)]
mod conversion_tests {
    use super::{row_u16, row_u64};

    #[test]
    fn persisted_scheduler_integer_corruption_is_rejected() -> rusqlite::Result<()> {
        let connection = rusqlite::Connection::open_in_memory()?;
        let negative_u64 = connection.query_row("SELECT -1", [], |row| row_u64(row, 0));
        let overflowing_u16 = connection.query_row("SELECT 65536", [], |row| row_u16(row, 0));

        assert!(negative_u64.is_err());
        assert!(overflowing_u16.is_err());
        Ok(())
    }
}
