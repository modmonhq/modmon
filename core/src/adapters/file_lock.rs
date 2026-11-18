//! The one place that waits for a file lock.
//!
//! A blocking lock call waits in the kernel for as long as the holder
//! likes. On a thread of an async runtime that stops every task queued
//! behind the thread, and for ever when the holder is one of those tasks:
//! that wedged `mmd`. So nothing waits without a bound. The lock is
//! tried, and tried again after a short pause, until a deadline; then the
//! caller gets an error that names the file and, where holders leave a
//! note, who holds it.
//!
//! `exclusive` is for code that is not async and keeps the lock for one
//! short section. `exclusive_async` never stops its thread. A source test
//! refuses a blocking lock call anywhere else in the crate.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use fs4::{FileExt, TryLockError};

use crate::domain::error::MmError;

/// How long a short section waits for another short section.
pub(crate) const PATIENCE: Duration = Duration::from_secs(30);
const RETRY: Duration = Duration::from_millis(10);
/// A holder's note is one line; anything longer is not one.
const NOTE_BYTES: u64 = 512;

/// Takes the exclusive lock on `file`, waiting at most `PATIENCE`. For
/// sections that do not await: the holder of such a lock is never waiting
/// for this thread.
pub(crate) fn exclusive(file: &File, path: &Path) -> Result<(), MmError> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if try_exclusive(file, path)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(held(file, path, PATIENCE));
        }
        std::thread::sleep(RETRY);
    }
}

/// Takes the exclusive lock on `file` without ever stopping the thread,
/// waiting at most `patience`.
pub(crate) async fn exclusive_async(
    file: &File,
    path: &Path,
    patience: Duration,
) -> Result<(), MmError> {
    let deadline = tokio::time::Instant::now() + patience;
    loop {
        if try_exclusive(file, path)? {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(held(file, path, patience));
        }
        tokio::time::sleep(RETRY).await;
    }
}

fn try_exclusive(file: &File, path: &Path) -> Result<bool, MmError> {
    match FileExt::try_lock(file) {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(source)) => Err(MmError::Io {
            context: format!("locking {}", path.display()),
            source,
        }),
    }
}

/// Leaves a note in a lock file this process holds, for whoever waits in
/// vain: which process, doing what. Best effort; the lock does not depend
/// on it.
pub(crate) fn note_holder(file: &File, what: &str) {
    let note = format!("process {} ({what})\n", std::process::id());
    let mut file = file;
    let written = file
        .set_len(0)
        .and_then(|()| file.seek(SeekFrom::Start(0)))
        .and_then(|_| file.write_all(note.as_bytes()));
    if let Err(error) = written {
        tracing::debug!(%error, "could not note the holder of a file lock");
    }
}

fn holder_note(file: &File) -> Option<String> {
    let mut file = file;
    let mut note = String::new();
    let read = file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.take(NOTE_BYTES).read_to_string(&mut note));
    if read.is_err() {
        return None;
    }
    let note = note.lines().next()?.trim().to_owned();
    (!note.is_empty() && note.chars().all(|character| !character.is_control())).then_some(note)
}

fn held(file: &File, path: &Path, waited: Duration) -> MmError {
    let holder = holder_note(file).map_or_else(
        || "another process".to_owned(),
        |note| format!("{note}, by the note it left in the file,"),
    );
    MmError::State {
        message: format!(
            "{} is locked: {holder} has held it for more than {} s, so nothing was changed. Run the command again; if it stays locked, look at that process on the machine that holds the file",
            path.display(),
            waited.as_secs()
        ),
    }
}
