//! A host's audit log: one JSON record per line in `audit.jsonl` under the
//! daemon's state directory, for every request, refusal, and change.
//!
//! It keeps itself bounded with no setting: once the file holds
//! [`AUDIT_ROTATE_BYTES`], it becomes `audit.jsonl.1`, the older files move
//! up by one, and the oldest beyond [`AUDIT_KEPT`] is deleted. A host keeps
//! about 80 MiB of history that way, months of ordinary use. Records are
//! never split between files, and nothing is compressed, so `grep` and
//! `jq` read every file as they are.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::error::MmError;

/// The log the daemon writes to.
pub const AUDIT_FILE: &str = "audit.jsonl";
/// Where an earlier release wrote access changes; read, never written.
const LEGACY_ACCESS_AUDIT: &str = "access-audit.jsonl";
/// The size at which the log is rotated.
pub const AUDIT_ROTATE_BYTES: u64 = 16 * 1024 * 1024;
/// How many rotated files are kept next to the log.
pub const AUDIT_KEPT: u32 = 4;
/// The most records one read returns.
pub const AUDIT_READ_LIMIT: usize = 10_000;

/// The audit log in one directory. Writers must not append at the same
/// time: the daemon holds its journal's lock around [`AuditLog::append`].
#[derive(Debug, Clone)]
pub struct AuditLog {
    directory: PathBuf,
    rotate_bytes: u64,
    kept: u32,
}

impl AuditLog {
    /// The log under `directory`, rotated at the default size.
    #[must_use]
    pub fn new(directory: &Path) -> Self {
        Self::with_limits(directory, AUDIT_ROTATE_BYTES, AUDIT_KEPT)
    }

    /// A log rotated at `rotate_bytes` that keeps `kept` older files.
    #[must_use]
    pub fn with_limits(directory: &Path, rotate_bytes: u64, kept: u32) -> Self {
        Self {
            directory: directory.to_path_buf(),
            rotate_bytes: rotate_bytes.max(1),
            kept: kept.max(1),
        }
    }

    fn path(&self, generation: u32) -> PathBuf {
        if generation == 0 {
            self.directory.join(AUDIT_FILE)
        } else {
            self.directory.join(format!("{AUDIT_FILE}.{generation}"))
        }
    }

    /// Appends one record and syncs it, rotating first when the record
    /// would take the file past its size.
    pub fn append(&self, value: &serde_json::Value) -> Result<(), MmError> {
        let mut line = serde_json::to_vec(value).map_err(|error| MmError::State {
            message: format!("audit record is not JSON: {error}"),
        })?;
        line.push(b'\n');
        let path = self.path(0);
        let size = match fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(source) => return Err(io(&path, "inspecting", source)),
        };
        if size > 0 && size.saturating_add(line.len() as u64) > self.rotate_bytes {
            self.rotate()?;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|source| io(&path, "opening", source))?;
        file.write_all(&line)
            .and_then(|()| file.sync_data())
            .map_err(|source| io(&path, "writing", source))
    }

    /// Moves every file up by one generation and drops the oldest.
    fn rotate(&self) -> Result<(), MmError> {
        let oldest = self.path(self.kept);
        match fs::remove_file(&oldest) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(io(&oldest, "removing", source)),
        }
        for generation in (0..self.kept).rev() {
            let from = self.path(generation);
            match fs::rename(&from, self.path(generation + 1)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(io(&from, "rotating", source)),
            }
        }
        Ok(())
    }

    /// The log's files that exist, newest first.
    #[must_use]
    pub fn files(&self) -> Vec<PathBuf> {
        (0..=self.kept)
            .map(|generation| self.path(generation))
            .chain(std::iter::once(self.directory.join(LEGACY_ACCESS_AUDIT)))
            .filter(|path| path.is_file())
            .collect()
    }

    /// The newest `limit` records, oldest first. Only as many files are
    /// read as it takes to find them.
    pub fn read_last(&self, limit: usize) -> Result<Vec<serde_json::Value>, MmError> {
        if limit == 0 || limit > AUDIT_READ_LIMIT {
            return Err(MmError::Security {
                message: format!("audit limit must be between 1 and {AUDIT_READ_LIMIT}"),
            });
        }
        let mut records = Vec::new();
        for path in self.files() {
            let contents =
                fs::read_to_string(&path).map_err(|source| io(&path, "reading", source))?;
            for line in contents.lines().filter(|line| !line.trim().is_empty()) {
                records.push(
                    serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
                        MmError::State {
                            message: format!("invalid audit record in {}: {error}", path.display()),
                        }
                    })?,
                );
            }
            if records.len() >= limit {
                break;
            }
        }
        // The order the records were written in, across files.
        records.sort_by_key(|record| record["timestamp_unix_ms"].as_i64());
        let start = records.len().saturating_sub(limit);
        Ok(records.split_off(start))
    }
}

fn io(path: &Path, doing: &str, source: std::io::Error) -> MmError {
    MmError::Io {
        context: format!("{doing} the audit log {}", path.display()),
        source,
    }
}
