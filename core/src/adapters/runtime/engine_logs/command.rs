//! A container's lines through the engine's `logs --timestamps` command,
//! for an engine whose API stamps them too coarsely.

use std::sync::Arc;
use std::time::Duration;

use super::{EngineContainer, LogDemux, Resume, Shared, Sink, engine_time};
use crate::domain::error::MmError;
use crate::domain::observability::{
    LogOutputPolicy, LogSource, OperationCancellation, log_instant,
};

/// The engine's command line, for [`super::LineSource::Command`].
pub(super) struct Command {
    pub(super) streaming: Arc<dyn crate::app::ports::StreamingProcessRunner>,
    pub(super) program: &'static str,
    pub(super) inherited: &'static [&'static str],
}

/// How long one `logs` command may run: a read that ends, and one that
/// follows (the read looks for the container again when it ends).
const READ_FOR: Duration = Duration::from_secs(120);
const FOLLOW_FOR: Duration = Duration::from_secs(24 * 60 * 60);

impl Shared {
    /// Reads one container's log through the engine's command line into
    /// `sink`: its standard output and standard error, each line beginning
    /// with its timestamp.
    pub(super) async fn command_lines(
        &self,
        container: &EngineContainer,
        resume: Resume,
        sink: &mut Sink,
        command: Command,
    ) -> Result<(), MmError> {
        use crate::app::ports::{ProcessOutputSource, ProcessStreamEvent, StreamCommandSpec};

        let request = &self.request;
        let mut args = vec!["logs".to_owned(), "--timestamps".to_owned()];
        if request.follow {
            args.push("--follow".to_owned());
        }
        match resume {
            Resume {
                after: Some((seconds, nanoseconds)),
                ..
            } => args.extend(["--since".to_owned(), format!("{seconds}.{nanoseconds:09}")]),
            Resume {
                everything: true, ..
            } => {}
            Resume { .. } => {
                args.extend(["--tail".to_owned(), request.tail.to_string()]);
                if let Some(since) = request.since_unix_ms {
                    args.extend(["--since".to_owned(), engine_time(since)]);
                }
            }
        }
        if let Some(until) = request.until_unix_ms {
            args.extend(["--until".to_owned(), engine_time(until)]);
        }
        args.push(container.observation.id.clone());
        let specification = StreamCommandSpec::new(command.program)
            .args(args)
            .inherit_environment(command.inherited)
            .timeout(if request.follow { FOLLOW_FOR } else { READ_FOR });
        let mut stream = command
            .streaming
            .spawn(specification, OperationCancellation::default())
            .await?;
        let wants = |source: LogSource| match request.output {
            LogOutputPolicy::Both => true,
            LogOutputPolicy::Stdout => source == LogSource::Stdout,
            LogOutputPolicy::Stderr => source == LogSource::Stderr,
        };
        let mut lines = LogDemux::new(container.tty, request.limits.maximum_line_bytes);
        while let Some(event) = stream.recv().await {
            let (source, bytes) = match event {
                ProcessStreamEvent::Output {
                    source: ProcessOutputSource::Stdout,
                    bytes,
                } => (LogSource::Stdout, bytes),
                ProcessStreamEvent::Output {
                    source: ProcessOutputSource::Stderr,
                    bytes,
                } => (LogSource::Stderr, bytes),
                ProcessStreamEvent::Exited { .. }
                | ProcessStreamEvent::Failed(MmError::ProcessTimeout { .. }) => break,
                ProcessStreamEvent::Failed(error) => return Err(error),
            };
            for line in lines.push_text(source, &bytes) {
                if wants(line.source) && !self.deliver(container, line, resume, sink).await? {
                    return Ok(());
                }
            }
        }
        for line in lines.finish() {
            if wants(line.source) && !self.deliver(container, line, resume, sink).await? {
                break;
            }
        }
        Ok(())
    }
}

/// How long after a line's own timestamp the timestamps of its pieces fall.
const PIECES_WITHIN_SECONDS: i64 = 60;

/// `text` without the timestamps of its pieces. The engine keeps a line it
/// read in several pieces (one longer than its buffer, 8 KiB) as several
/// entries, and Podman 5.4's `logs --timestamps` joins them with each
/// piece's timestamp and a space inside the line (measured). Such a stamp
/// is written like the line's own `leading` one, to the nanosecond, at
/// most a minute after it; a stamp of the program's own (milliseconds, or
/// at the start of its line) is left as it is.
pub(super) fn without_piece_stamps(text: &[u8], leading: &str) -> Vec<u8> {
    let Some((first, _)) = log_instant(leading) else {
        return text.to_vec();
    };
    let in_utc = leading.ends_with('Z');
    let mut kept = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if index > 0
            && let Some(length) = piece_stamp(&text[index..], first, in_utc)
        {
            index += length;
            continue;
        }
        kept.push(text[index]);
        index += 1;
    }
    kept
}

/// The length of a piece's stamp and the space after it, when `text`
/// begins with one.
fn piece_stamp(text: &[u8], first: i64, in_utc: bool) -> Option<usize> {
    let shaped = text.len() > 20
        && text[0].is_ascii_digit()
        && text[4] == b'-'
        && text[7] == b'-'
        && text[10] == b'T'
        && text[19] == b'.';
    if !shaped {
        return None;
    }
    let space = text.iter().take(40).position(|byte| *byte == b' ')?;
    let Ok(stamp) = std::str::from_utf8(&text[..space]) else {
        return None;
    };
    let digits = stamp
        .get(20..)?
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let (at, _) = log_instant(stamp)?;
    (digits >= 7
        && stamp.ends_with('Z') == in_utc
        && (first..=first + PIECES_WITHIN_SECONDS).contains(&at))
    .then_some(space + 1)
}
