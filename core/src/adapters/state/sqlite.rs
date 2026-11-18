use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::adapters::private_fs::{ensure_no_symlink_components, ensure_owned_by_current_user};
use crate::app::ports::{
    EnvironmentValueStore, ExecAuditJournal, IngressRouteRegistry, McpAuditJournal, StateStore,
};
use crate::domain::build::{
    ImageBuildPlan, ImageBuildResult, PLAN_SCHEMA, ReusableImage, ReusableImageRecord,
};
use crate::domain::config_resolve::{route_paths_overlap, route_priority};
use crate::domain::deployment::DeploymentPlan;
use crate::domain::environment::{EnvironmentValueOverride, is_safe_environment_value};
use crate::domain::error::MmError;
use crate::domain::ingress::{
    CanonicalMiddleware, IngressRouteClaim, IngressRouteConflict, IngressRouteOwnership,
    IngressRouteProvenance, IngressRouteSet,
};
use crate::domain::operation::{
    NewOperation, OperationEvent, OperationId, OperationKind, OperationState, OperationSummary,
};
use crate::domain::target::{TargetBootstrapPlan, TargetBootstrapResult, TargetId};
use crate::domain::workspace::{ProjectId, WorkspaceId, WorkspaceRecord, WorkspaceRegistration};

const INITIAL_SCHEMA: &str = include_str!("../../../migrations/0001_initial.sql");
const MIGRATIONS: &[(&str, &str)] = &[("0001_initial", INITIAL_SCHEMA)];

mod environment_values;
mod gate;
mod home;
mod ingress;
mod recovery;
mod scheduler;
pub use recovery::SqliteStateRecovery;

pub use home::MmHome;
use home::make_private_file;

pub struct SqliteStateStore {
    connection: Mutex<Connection>,
}

impl std::fmt::Debug for SqliteStateStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqliteStateStore")
            .finish_non_exhaustive()
    }
}

impl SqliteStateStore {
    /// Existing state only: no migrations, WAL changes, or file creation.
    pub fn inspect(path: &Path) -> Result<Option<Self>, MmError> {
        ensure_no_symlink_components(path)?;
        if !path.exists() {
            return Ok(None);
        }
        #[cfg(unix)]
        ensure_owned_by_current_user(path)?;
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(state_error)?;
        if !inspect_schema(&connection, path)? {
            return Ok(None);
        }
        Ok(Some(Self {
            connection: Mutex::new(connection),
        }))
    }

    pub fn open(path: &Path) -> Result<Self, MmError> {
        let _phase = crate::app::timing::phase("state.open");
        ensure_no_symlink_components(path)?;
        #[cfg(unix)]
        if path.exists() {
            ensure_owned_by_current_user(path)?;
        }
        let mut connection = Connection::open(path).map_err(state_error)?;
        // Before the first question: another process may be creating the
        // schema right now.
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(state_error)?;
        inspect_schema(&connection, path)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(state_error)?;
        enable_wal(&connection)?;
        initialize_schema(&mut connection)?;
        make_private_file(path)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, MmError> {
        self.connection.lock().map_err(|_| MmError::State {
            message: "SQLite connection lock was poisoned".to_owned(),
        })
    }
}

fn inspect_schema(connection: &Connection, path: &Path) -> Result<bool, MmError> {
    // One snapshot for every question. Asked one by one, a database that
    // another process finishes creating in between has no migration table
    // for the first question and a `workspaces` table for the last, which
    // reads like the state of an earlier mm.
    let connection = &connection.unchecked_transaction().map_err(state_error)?;
    let has_migrations: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
        [], |row| row.get(0),
    ).map_err(state_error)?;
    if has_migrations {
        let unsupported = connection
            .query_row(
                "SELECT version FROM schema_migrations WHERE version <> '0001_initial' LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(state_error)?;
        if let Some(version) = unsupported {
            return Err(MmError::State {
                message: format!(
                    "unsupported SQLite migration '{version}' at {}; use a compatible mm version",
                    path.display()
                ),
            });
        }
        let applied: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = '0001_initial')",
                [],
                |row| row.get(0),
            )
            .map_err(state_error)?;
        if applied {
            return Ok(true);
        }
    }
    let legacy: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'workspaces')",
        [], |row| row.get(0),
    ).map_err(state_error)?;
    if legacy {
        return Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::LegacyState {
                path: path.to_owned(),
            },
        });
    }
    Ok(false)
}

fn enable_wal(connection: &Connection) -> Result<(), MmError> {
    let mut retries = 500_u16;
    loop {
        match connection.execute_batch("PRAGMA journal_mode = WAL;") {
            Ok(()) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if retries > 0
                    && matches!(
                        error.code,
                        rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                    ) =>
            {
                retries -= 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(state_error(error)),
        }
    }
}

impl ExecAuditJournal for SqliteStateStore {
    fn append_exec_audit(
        &self,
        record: &crate::domain::observability::ExecAuditRecord,
    ) -> Result<(), MmError> {
        if record.principal.is_empty()
            || record.principal.len() > 255
            || record.executable.is_empty()
            || record.executable.len() > 4096
            || record.argv_hash.len() != 64
            || !record
                .argv_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || record.error_code.as_ref().is_some_and(|code| {
                code.len() > 128
                    || !code.bytes().all(|byte| {
                        byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            })
        {
            return Err(MmError::Security {
                message: "exec audit metadata is invalid".to_owned(),
            });
        }
        let runtime = match record.runtime {
            crate::domain::runtime::RuntimeKind::PodmanCompose => "podman-compose",
            crate::domain::runtime::RuntimeKind::DockerCompose => "docker-compose",
        };
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "INSERT INTO runtime_exec_audit (
                    id, principal, runtime, project_id, environment_name, stack_name,
                    module_name, service_name, container_id, executable, argv_hash, started_at_ms,
                    finished_at_ms, exit_code, error_code
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
                 ON CONFLICT(id) DO UPDATE SET
                    container_id=excluded.container_id,
                    finished_at_ms=excluded.finished_at_ms,
                    exit_code=excluded.exit_code,
                    error_code=excluded.error_code
                 WHERE runtime_exec_audit.principal=excluded.principal
                   AND runtime_exec_audit.runtime=excluded.runtime
                   AND runtime_exec_audit.project_id=excluded.project_id
                   AND runtime_exec_audit.environment_name=excluded.environment_name
                   AND runtime_exec_audit.stack_name=excluded.stack_name
                   AND runtime_exec_audit.module_name IS excluded.module_name
                   AND runtime_exec_audit.service_name=excluded.service_name
                   AND runtime_exec_audit.executable=excluded.executable
                   AND runtime_exec_audit.argv_hash=excluded.argv_hash
                   AND runtime_exec_audit.started_at_ms=excluded.started_at_ms
                   AND runtime_exec_audit.finished_at_ms IS NULL",
                params![
                    record.id.to_string(),
                    record.principal,
                    runtime,
                    record.project_id.0.to_string(),
                    record.environment,
                    record.stack,
                    record.module,
                    record.service,
                    record.container_id,
                    record.executable,
                    record.argv_hash,
                    record.started_at_unix_ms,
                    record.finished_at_unix_ms,
                    record.exit_code,
                    record.error_code,
                ],
            )
            .map_err(state_error)?;
        if changed != 1 {
            return Err(MmError::Security {
                message: "exec audit transition did not match its immutable metadata".to_owned(),
            });
        }
        Ok(())
    }
}

impl McpAuditJournal for SqliteStateStore {
    fn append_mcp_audit(&self, record: &crate::domain::mcp::McpAuditRecord) -> Result<(), MmError> {
        if record.principal.is_empty()
            || record.principal.len() > 255
            || record.source.is_empty()
            || record.source.len() > 255
            || record.operation.is_empty()
            || record.operation.len() > 128
            || record.outcome_code.is_empty()
            || record.outcome_code.len() > 128
            || record
                .plan_hash
                .as_ref()
                .is_some_and(|hash| hash.len() != 64)
        {
            return Err(MmError::Security {
                message: "MCP audit attribution is invalid".to_owned(),
            });
        }
        let root = record
            .project_root
            .to_str()
            .ok_or_else(|| MmError::Security {
                message: "MCP audit root must be valid Unicode".to_owned(),
            })?;
        self.connection()?
            .execute(
                "INSERT INTO mcp_audit (
                    id, request_id, principal, source, project_root, environment_name,
                    target_name, operation, mutation, plan_hash, outcome_code, occurred_at_ms
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    record.id.to_string(),
                    record.request_id.to_string(),
                    record.principal,
                    record.source,
                    root,
                    record.environment,
                    record.target,
                    record.operation,
                    record.mutation,
                    record.plan_hash,
                    record.outcome_code,
                    record.occurred_at_unix_ms,
                ],
            )
            .map_err(state_error)?;
        Ok(())
    }
}

include!("sqlite/workspaces.rs");
include!("sqlite/builds.rs");
include!("sqlite/deployments.rs");
include!("sqlite/operations.rs");

impl StateStore for SqliteStateStore {
    sqlite_workspaces!();
    sqlite_builds!();
    sqlite_deployments!();
    sqlite_operations!();
}

fn workspace_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceRecord> {
    let id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let root: String = row.get(2)?;
    let config_path: String = row.get(3)?;
    Ok(WorkspaceRecord {
        id: WorkspaceId(parse_uuid(&id, 0)?),
        project_id: ProjectId(parse_uuid(&project_id, 1)?),
        root: PathBuf::from(root),
        config_path: PathBuf::from(config_path),
    })
}

fn read_preference(
    connection: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Option<crate::app::LocalSelectionPreference>, MmError> {
    connection
        .query_row(sql, params, |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })
        .optional()
        .map_err(state_error)?
        .map(|(runtime, target)| {
            Ok(crate::app::LocalSelectionPreference {
                runtime: runtime.map(parse_runtime_preference).transpose()?.flatten(),
                target,
            })
        })
        .transpose()
}

/// The runtime earlier releases could store and this one cannot run.
const REMOVED_RUNTIME: &str = "docker-swarm";

/// A stored runtime preference. One for the removed `docker-swarm` runtime
/// counts as no preference, so selection falls back to the configuration.
fn parse_runtime_preference(
    value: String,
) -> Result<Option<crate::domain::runtime::RuntimeKind>, MmError> {
    match value.as_str() {
        "podman-compose" => Ok(Some(crate::domain::runtime::RuntimeKind::PodmanCompose)),
        "docker-compose" => Ok(Some(crate::domain::runtime::RuntimeKind::DockerCompose)),
        REMOVED_RUNTIME => Ok(None),
        _ => Err(MmError::State {
            message: format!("stored runtime preference '{value}' is invalid"),
        }),
    }
}

const fn runtime_kind_sql(kind: crate::domain::runtime::RuntimeKind) -> &'static str {
    match kind {
        crate::domain::runtime::RuntimeKind::PodmanCompose => "podman-compose",
        crate::domain::runtime::RuntimeKind::DockerCompose => "docker-compose",
    }
}

fn parse_uuid(value: &str, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn operation_kind(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Build => "build",
        OperationKind::Bootstrap => "bootstrap",
        OperationKind::Deploy => "deploy",
        OperationKind::Down => "down",
        OperationKind::Reconcile => "reconcile",
        OperationKind::Rollback => "rollback",
        OperationKind::Job => "job",
    }
}

fn parse_operation_kind(value: &str, column: usize) -> rusqlite::Result<OperationKind> {
    match value {
        "build" => Ok(OperationKind::Build),
        "bootstrap" => Ok(OperationKind::Bootstrap),
        "deploy" => Ok(OperationKind::Deploy),
        "down" => Ok(OperationKind::Down),
        "reconcile" => Ok(OperationKind::Reconcile),
        "rollback" => Ok(OperationKind::Rollback),
        "job" => Ok(OperationKind::Job),
        _ => Err(invalid_enum(column, value, "operation kind")),
    }
}

fn operation_state(state: OperationState) -> &'static str {
    match state {
        OperationState::Planned => "planned",
        OperationState::Running => "running",
        OperationState::Succeeded => "succeeded",
        OperationState::Failed => "failed",
        OperationState::Cancelled => "cancelled",
        OperationState::Interrupted => "interrupted",
    }
}

fn parse_operation_state(value: &str, column: usize) -> rusqlite::Result<OperationState> {
    match value {
        "planned" => Ok(OperationState::Planned),
        "running" => Ok(OperationState::Running),
        "succeeded" => Ok(OperationState::Succeeded),
        "failed" => Ok(OperationState::Failed),
        "cancelled" => Ok(OperationState::Cancelled),
        "interrupted" => Ok(OperationState::Interrupted),
        _ => Err(invalid_enum(column, value, "operation state")),
    }
}

fn invalid_enum(column: usize, value: &str, kind: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown {kind}: {value}"),
        )),
    )
}

fn initialize_schema(connection: &mut Connection) -> Result<(), MmError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(state_error)?;
    transaction
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY NOT NULL,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .map_err(state_error)?;
    let unsupported = transaction
        .query_row(
            "SELECT version FROM schema_migrations WHERE version <> '0001_initial' LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(state_error)?;
    if let Some(version) = unsupported {
        return Err(MmError::State {
            message: format!("unsupported pre-release SQLite migration '{version}'"),
        });
    }
    for &(version, sql) in MIGRATIONS {
        let applied = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get::<_, bool>(0),
            )
            .map_err(state_error)?;
        if !applied {
            let existing_application_schema = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'workspaces')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(state_error)?;
            if existing_application_schema {
                return Err(MmError::State {
                    message: "unsupported pre-baseline SQLite schema; create fresh state"
                        .to_owned(),
                });
            }
            transaction.execute_batch(sql).map_err(state_error)?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations (version) VALUES (?1)",
                    [version],
                )
                .map_err(state_error)?;
        }
    }
    transaction.commit().map_err(state_error)
}

const fn event_operation_id(event: &OperationEvent) -> OperationId {
    match event {
        OperationEvent::Started { operation_id, .. }
        | OperationEvent::PhaseChanged { operation_id, .. }
        | OperationEvent::Completed { operation_id, .. } => *operation_id,
    }
}

fn insert_operation_event(
    connection: &Connection,
    operation_id: OperationId,
    occurred_at_unix_ms: i64,
    event: &OperationEvent,
) -> Result<(), MmError> {
    let payload = serde_json::to_string(event).map_err(|error| MmError::State {
        message: format!("could not serialize operation event: {error}"),
    })?;
    connection
        .execute(
            "INSERT INTO operation_events (operation_id, occurred_at_ms, payload_json)
             VALUES (?1, ?2, ?3)",
            params![operation_id.0.to_string(), occurred_at_unix_ms, payload],
        )
        .map_err(state_error)?;
    Ok(())
}

/// A stored deployment plan. `None` for one made with the removed
/// `docker-swarm` runtime: mm no longer manages those stacks, so such a plan
/// is no previous deployment.
fn parse_deployment_plan_snapshot(json: &str) -> Result<Option<DeploymentPlan>, MmError> {
    let invalid = |error: serde_json::Error| MmError::Security {
        message: format!("stored deployment plan snapshot is invalid: {error}"),
    };
    let value = serde_json::from_str::<serde_json::Value>(json).map_err(invalid)?;
    if value.get("runtime").and_then(serde_json::Value::as_str) == Some(REMOVED_RUNTIME) {
        return Ok(None);
    }
    let plan: DeploymentPlan = serde_json::from_value(value).map_err(invalid)?;
    let canonical = plan
        .canonical_hash()
        .map_err(|message| MmError::Security { message })?;
    if canonical == plan.plan_hash {
        Ok(Some(plan))
    } else {
        Err(MmError::Security {
            message: "stored deployment plan snapshot failed canonical verification".to_owned(),
        })
    }
}

fn state_error(error: rusqlite::Error) -> MmError {
    MmError::State {
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "sqlite/tests.rs"]
mod tests;
