//! What the running command prints and reports, kept for its record in the
//! command log: the plain text the non-terminal output has, its progress
//! events, and its JSON result.

use std::io::{self, Write};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use crate::domain::command_log::{MAX_EVENTS, MAX_OUTPUT_BYTES, RecordedEvent};
use crate::domain::progress::ProgressEvent;
use crate::presentation::inline::{Palette, Tasks};

static RECORDING: Mutex<Option<Recording>> = Mutex::new(None);

struct Recording {
    started: Instant,
    started_unix_ms: i64,
    /// `--output json`: the plain lines of progress are not printed then.
    json: bool,
    muted: bool,
    output: String,
    truncated: bool,
    events: Vec<RecordedEvent>,
    tasks: Tasks,
    result: Option<serde_json::Value>,
}

/// What a finished recording holds.
#[derive(Debug)]
pub struct Recorded {
    pub started_unix_ms: i64,
    pub output: String,
    pub truncated: bool,
    pub events: Vec<RecordedEvent>,
    pub result: Option<serde_json::Value>,
}

fn recording() -> MutexGuard<'static, Option<Recording>> {
    RECORDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts recording this process's command.
pub fn begin(json: bool) {
    *recording() = Some(Recording {
        started: Instant::now(),
        started_unix_ms: unix_ms(),
        json,
        muted: false,
        output: String::new(),
        truncated: false,
        events: Vec::new(),
        tasks: Tasks::new(Palette::plain()),
        result: None,
    });
}

/// Ends the recording and returns what it holds.
#[must_use]
pub fn finish() -> Option<Recorded> {
    recording().take().map(|recording| Recorded {
        started_unix_ms: recording.started_unix_ms,
        output: recording.output,
        truncated: recording.truncated,
        events: recording.events,
        result: recording.result,
    })
}

impl Recording {
    fn append(&mut self, text: &str) {
        if self.output.len() + text.len() > MAX_OUTPUT_BYTES {
            self.truncated = true;
            return;
        }
        self.output.push_str(text);
    }
}

/// Text the command printed.
pub fn output(text: &str) {
    if let Some(recording) = recording().as_mut() {
        recording.append(text);
    }
}

/// A line the plain output prints on standard error.
pub fn line(text: &str) {
    if let Some(recording) = recording().as_mut()
        && !recording.json
    {
        recording.append(text);
        recording.append("\n");
    }
}

/// A progress event, and the plain line of a step it finishes.
pub fn event(event: &ProgressEvent) {
    let mut guard = recording();
    let Some(recording) = guard.as_mut() else {
        return;
    };
    let at_ms = u64::try_from(recording.started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if recording.events.len() < MAX_EVENTS {
        recording.events.push(RecordedEvent {
            at_ms,
            event: event.clone(),
        });
    }
    recording.tasks.apply(event.clone(), Instant::now());
    for line in recording.tasks.take_plain() {
        if !recording.json {
            recording.append(&line);
            recording.append("\n");
        }
    }
}

/// The command's JSON result.
pub fn result(value: &serde_json::Value) {
    if let Some(recording) = recording().as_mut() {
        recording.result = Some(value.clone());
    }
}

/// Runs `write` with standard output going to the record alone: the
/// terminal shows a card, and the record keeps the text scripts read.
pub fn muted<T>(write: impl FnOnce() -> T) -> T {
    set_muted(true);
    let written = write();
    set_muted(false);
    written
}

fn set_muted(muted: bool) {
    if let Some(recording) = recording().as_mut() {
        recording.muted = muted;
    }
}

fn is_muted() -> bool {
    recording()
        .as_ref()
        .is_some_and(|recording| recording.muted)
}

/// Standard output, also kept in the record.
#[must_use]
pub fn stdout() -> Stdout {
    Stdout
}

/// Standard output as [`stdout`] returns it.
#[derive(Debug, Clone, Copy)]
pub struct Stdout;

impl Stdout {
    #[must_use]
    pub fn lock(self) -> Self {
        self
    }
}

impl Write for Stdout {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        output(&String::from_utf8_lossy(bytes));
        if is_muted() {
            return Ok(bytes.len());
        }
        io::stdout().write_all(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()
    }
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Writes `text` to [`stdout`]; a closed standard output is not an error.
pub fn print(text: &str) {
    if let Err(error) = stdout().write_all(text.as_bytes()) {
        tracing::debug!(%error, "standard output is closed");
    }
}

/// Prints like `println!`, through [`stdout`].
#[macro_export]
macro_rules! outln {
    () => {
        $crate::presentation::record::print("\n")
    };
    ($($argument:tt)*) => {{
        let mut line = ::std::format!($($argument)*);
        line.push('\n');
        $crate::presentation::record::print(&line);
    }};
}
