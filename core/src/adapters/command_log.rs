//! The command log as files: one JSON file per command under
//! `<mm home>/history/<project id>/`, the oldest dropped past a count and
//! a size.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::app::ports::CommandLog;
use crate::domain::command_log::{CommandFilter, CommandRecord, CommandSummary};
use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

/// The most commands kept per project.
pub const MAX_RECORDS: usize = 500;
/// The most bytes kept per project.
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct FileCommandLog {
    root: PathBuf,
    limits: (usize, u64),
}

impl FileCommandLog {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            limits: (MAX_RECORDS, MAX_BYTES),
        }
    }

    /// The same log with other retention limits.
    #[must_use]
    pub const fn keeping(mut self, records: usize, bytes: u64) -> Self {
        self.limits = (records, bytes);
        self
    }

    fn directory(&self, project: ProjectId) -> PathBuf {
        self.root.join(project.0.to_string())
    }

    fn names(directory: &Path) -> Result<Vec<String>, MmError> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(io("reading the command log", source)),
        };
        let mut names = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".json").map(str::to_owned))
            .filter(|name| valid_id(name))
            .collect::<Vec<_>>();
        names.sort_unstable();
        Ok(names)
    }

    fn read(directory: &Path, id: &str) -> Result<Option<CommandRecord>, MmError> {
        let path = directory.join(format!("{id}.json"));
        let Some(bytes) = crate::adapters::private_fs::read_regular_file(&path, MAX_RECORD_BYTES)?
        else {
            return Ok(None);
        };
        Ok(serde_json::from_slice(&bytes).ok())
    }

    fn prune(&self, directory: &Path) -> Result<(), MmError> {
        let lock_path = directory.join("history.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|source| io("opening the command log lock", source))?;
        crate::adapters::file_lock::exclusive(&lock, &lock_path)?;
        let names = Self::names(directory)?;
        let (records, bytes) = self.limits;
        let mut kept_bytes = 0_u64;
        for (position, name) in names.iter().rev().enumerate() {
            let path = directory.join(format!("{name}.json"));
            let size = fs::metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            kept_bytes += size;
            if (position >= records || kept_bytes > bytes)
                && let Err(error) = fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(io("dropping an old command record", error));
            }
        }
        Ok(())
    }
}

impl CommandLog for FileCommandLog {
    fn append(&self, project: ProjectId, record: &CommandRecord) -> Result<(), MmError> {
        if !valid_id(&record.id) {
            return Err(MmError::State {
                message: format!("'{}' is not a command record ID", record.id),
            });
        }
        let directory = self.directory(project);
        private_directory(&directory)?;
        let bytes = serde_json::to_vec(record).map_err(|error| MmError::State {
            message: format!("serializing a command record: {error}"),
        })?;
        let partial = directory.join(format!(".{}.partial", record.id));
        let mut file = private_file(&partial)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| io("writing a command record", source))?;
        fs::rename(&partial, directory.join(format!("{}.json", record.id)))
            .map_err(|source| io("keeping a command record", source))?;
        self.prune(&directory)
    }

    fn list(
        &self,
        project: ProjectId,
        filter: &CommandFilter,
        (cursor, limit): (Option<&str>, usize),
    ) -> Result<(Vec<CommandSummary>, Option<String>), MmError> {
        let directory = self.directory(project);
        let mut found = Vec::new();
        for name in Self::names(&directory)?.iter().rev() {
            if cursor.is_some_and(|cursor| name.as_str() >= cursor) {
                continue;
            }
            let Some(record) = Self::read(&directory, name)? else {
                continue;
            };
            let summary = record.summary();
            if !filter.admits(&summary) {
                continue;
            }
            if found.len() == limit {
                let next = found.last().map(|last: &CommandSummary| last.id.clone());
                return Ok((found, next));
            }
            found.push(summary);
        }
        Ok((found, None))
    }

    fn get(&self, project: ProjectId, id: &str) -> Result<Option<CommandRecord>, MmError> {
        if id.is_empty() || !id.chars().all(id_character) {
            return Ok(None);
        }
        let directory = self.directory(project);
        let matching = Self::names(&directory)?
            .into_iter()
            .filter(|name| name.starts_with(id))
            .collect::<Vec<_>>();
        match matching.as_slice() {
            [one] => Self::read(&directory, one),
            [] => Ok(None),
            _ => Err(MmError::Environment {
                message: format!(
                    "'{id}' starts {} command IDs; give more of it",
                    matching.len()
                ),
            }),
        }
    }
}

fn id_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-'
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(id_character)
}

fn io(context: &str, source: std::io::Error) -> MmError {
    MmError::Io {
        context: context.to_owned(),
        source,
    }
}

fn private_directory(path: &Path) -> Result<(), MmError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
        .create(path)
        .map_err(|source| io("creating the command log", source))
}

fn private_file(path: &Path) -> Result<File, MmError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .map_err(|source| io("writing a command record", source))
}
