use std::fs::{File, OpenOptions};
use std::io::Read;

use super::*;
use crate::app::ports::StateRecoveryStore;
use crate::domain::readiness::{SetupIssue, StateRecoveryPlan};

/// Recovers only the known pre-baseline schema. The backup is a SQLite snapshot
/// (including committed WAL data), and replacement happens in one transaction
/// on the original database, never by unlinking an open database.
pub struct SqliteStateRecovery {
    pub path: PathBuf,
}

impl StateRecoveryStore for SqliteStateRecovery {
    fn plan(&self) -> Result<StateRecoveryPlan, MmError> {
        match SqliteStateStore::inspect(&self.path) {
            Err(MmError::Setup {
                issue: SetupIssue::LegacyState { .. },
            }) => {}
            Err(error) => return Err(error),
            Ok(_) => {
                return Err(MmError::State {
                    message:
                        "state recovery is only available for an unsupported pre-baseline database"
                            .to_owned(),
                });
            }
        }
        Ok(StateRecoveryPlan {
            database: self.path.clone(),
            plan_hash: fingerprint(&self.path)?,
        })
    }

    fn recover(&self, expected_plan: &str) -> Result<PathBuf, MmError> {
        let plan = self.plan()?;
        if plan.plan_hash != expected_plan {
            return Err(stale_plan());
        }
        let backup = self
            .path
            .with_file_name(format!("state-backup-{}.sqlite3", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&backup).map_err(|source| MmError::Io {
            context: "creating state backup".to_owned(),
            source,
        })?;
        drop(file);
        let mut connection =
            Connection::open_with_flags(&self.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(state_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(state_error)?;
        let version: i64 = connection
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .map_err(state_error)?;
        connection
            .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
            .map_err(state_error)?;
        File::open(&backup)
            .and_then(|file| file.sync_all())
            .map_err(|source| MmError::Io {
                context: "syncing state backup".to_owned(),
                source,
            })?;
        #[cfg(unix)]
        if let Some(parent) = backup.parent() {
            File::open(parent)
                .and_then(|file| file.sync_all())
                .map_err(|source| MmError::Io {
                    context: "syncing state backup directory".to_owned(),
                    source,
                })?;
        }
        connection
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .map_err(state_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .map_err(state_error)?;
        let current: i64 = transaction
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .map_err(state_error)?;
        if current != version || fingerprint(&self.path)? != expected_plan {
            return Err(stale_plan());
        }
        // Views go first; dropping tables also removes their indexes and triggers.
        let objects = {
            let mut statement = transaction.prepare("SELECT type, name FROM sqlite_master WHERE type IN ('view', 'table') AND name NOT LIKE 'sqlite_%' ORDER BY type DESC, name").map_err(state_error)?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(state_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(state_error)?
        };
        for (kind, name) in objects {
            let kind = if kind == "view" { "VIEW" } else { "TABLE" };
            transaction
                .execute_batch(&format!("DROP {kind} \"{}\";", name.replace('"', "\"\"")))
                .map_err(state_error)?;
        }
        transaction
            .execute_batch(INITIAL_SCHEMA)
            .map_err(state_error)?;
        transaction.execute_batch("CREATE TABLE schema_migrations (version TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP); INSERT INTO schema_migrations (version) VALUES ('0001_initial');").map_err(state_error)?;
        transaction.commit().map_err(state_error)?;
        Ok(backup)
    }
}

fn fingerprint(path: &Path) -> Result<String, MmError> {
    let mut hash = blake3::Hasher::new();
    hash.update(b"mm-state-recovery-v1");
    hash.update(path.as_os_str().as_encoded_bytes());
    for path in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
    ] {
        ensure_no_symlink_components(&path)?;
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(MmError::Io {
                    context: "reading state recovery input".to_owned(),
                    source,
                });
            }
        };
        let mut buffer = [0; 65536];
        loop {
            let length = file.read(&mut buffer).map_err(|source| MmError::Io {
                context: "hashing state recovery input".to_owned(),
                source,
            })?;
            if length == 0 {
                break;
            }
            hash.update(&buffer[..length]);
        }
    }
    Ok(hash.finalize().to_hex().to_string())
}

fn stale_plan() -> MmError {
    MmError::State { message: "state changed since recovery was reviewed; stop other mm processes and review `mm doctor recover-state` again. The original database was not reset".to_owned() }
}
