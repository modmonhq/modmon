//! The member's audit device: a file the member writes, which mmd rotates
//! as it rotates its own audit log and prints next to it.

use std::collections::VecDeque;
use std::io::BufRead as _;
use std::path::{Path, PathBuf};

use crate::domain::error::MmError;

/// The size at which the log is rotated, as mmd's own.
const ROTATE_AT: u64 = 16 * 1024 * 1024;
/// Rotated files kept: `audit.log.1` to `.4`.
const KEEP: usize = 4;

fn rotated(log: &Path, index: usize) -> PathBuf {
    let mut name = log.as_os_str().to_owned();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> MmError + '_ {
    move |source| MmError::Io {
        context: format!("rotating {}", path.display()),
        source,
    }
}

fn rename_if_there(from: &Path, to: &Path) -> Result<(), MmError> {
    match std::fs::rename(from, to) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other.map_err(io(from)),
    }
}

/// Moves `log` to `log.1`, and the older files up, past 16 MiB; true when it did.
pub(super) fn rotate(log: &Path) -> Result<bool, MmError> {
    let size = match std::fs::metadata(log) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(io(log)(source)),
    };
    if size < ROTATE_AT {
        return Ok(false);
    }
    for index in (1..KEEP).rev() {
        rename_if_there(&rotated(log, index), &rotated(log, index + 1))?;
    }
    rename_if_there(log, &rotated(log, 1))?;
    Ok(true)
}

/// The newest `limit` records of `log` and its rotated files, oldest first.
pub(super) fn newest(log: &Path, limit: usize) -> Result<Vec<serde_json::Value>, MmError> {
    let mut records = VecDeque::with_capacity(limit.min(4096));
    let files = (1..=KEEP)
        .rev()
        .map(|index| rotated(log, index))
        .chain([log.to_path_buf()]);
    for path in files {
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading {}", path.display()),
                    source,
                });
            }
        };
        for line in std::io::BufReader::new(file).lines() {
            let line = line.map_err(|source| MmError::Io {
                context: format!("reading {}", path.display()),
                source,
            })?;
            let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if records.len() == limit {
                records.pop_front();
            }
            if limit > 0 {
                records.push_back(record);
            }
        }
    }
    Ok(records.into())
}
