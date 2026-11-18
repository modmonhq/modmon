//! A service's log lines read from the engine's API, every container at
//! once, as one stream in the order the engine recorded them.
//!
//! Nothing is started on the host: each container's log is one request on
//! the engine's socket. A read that ends asks every container for its last
//! lines and keeps the newest of all of them. A read that follows merges
//! the containers' streams as they arrive and looks every few seconds for
//! containers that started since, so a deployment that replaces a
//! container does not end the stream.
//!
//! Podman 5.4's API stamps each line to the second only, which cannot
//! order the lines of several nodes; for Podman each container's lines
//! come from `podman logs --timestamps` instead ([`LineSource::Command`]),
//! and only the containers are found through the API.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zeroize::Zeroizing;

use crate::adapters::engine_api::EngineApi;
use crate::adapters::runtime::engine_containers::{EngineContainer, project_containers};
use crate::app::ports::LogStream;
use crate::domain::error::MmError;
use crate::domain::observability::{
    LogInstant, LogMerge, LogOutputPolicy, LogRecord, LogRequest, LogSource, OperationCancellation,
    log_instant, merged_tail, redact_output,
};
use crate::domain::runtime::ServiceIdentity;

mod command;
use command::Command;

/// How long a followed line waits for an older line of another container
/// on the same host.
const HOST_HOLD: Duration = Duration::from_millis(100);
/// How often a followed read looks for containers that started since.
const LOOK_FOR_CONTAINERS: Duration = Duration::from_secs(2);
/// How often a followed read checks whether it was cancelled.
const CANCEL_CHECK: Duration = Duration::from_millis(100);

/// Where a container's lines are read from.
#[derive(Clone)]
pub(crate) enum LineSource {
    /// The engine's API, with nanosecond timestamps.
    Api,
    /// The engine's `logs --timestamps` command, one process per container.
    Command {
        streaming: Arc<dyn crate::app::ports::StreamingProcessRunner>,
        program: &'static str,
        inherited: &'static [&'static str],
    },
}

/// What to read: the containers of one Compose project.
pub(crate) struct EngineLogRead {
    pub api: EngineApi,
    pub lines: LineSource,
    pub project: String,
    /// The identity of every line; its service is each container's own.
    pub identity: ServiceIdentity,
    pub request: LogRequest,
    pub redactions: Vec<Zeroizing<String>>,
    pub cancellation: OperationCancellation,
}

struct Shared {
    api: EngineApi,
    lines: LineSource,
    project: String,
    identity: ServiceIdentity,
    request: LogRequest,
    redactions: Vec<Zeroizing<String>>,
    rate: Mutex<LogRate>,
}

/// Starts the read and returns its stream.
pub(crate) async fn read(read: EngineLogRead) -> Result<LogStream, MmError> {
    let EngineLogRead {
        api,
        lines,
        project,
        identity,
        request,
        redactions,
        cancellation,
    } = read;
    let containers = selected_containers(&api, &project, &request).await?;
    let (sender, receiver) = tokio::sync::mpsc::channel(request.limits.buffered_records);
    let shared = Arc::new(Shared {
        api,
        lines,
        project,
        identity,
        rate: Mutex::new(LogRate::new(&request)),
        request,
        redactions,
    });
    let task_cancellation = cancellation.clone();
    tokio::spawn(async move {
        let result = if shared.request.follow {
            follow(&shared, containers, &sender, &task_cancellation).await
        } else {
            read_tail(&shared, &containers, &sender).await
        };
        if let Err(error) = result {
            if sender.send(Err(error)).await.is_err() {
                tracing::debug!("log receiver closed before failure diagnostics");
            }
            task_cancellation.cancel();
        }
    });
    Ok(LogStream::new(receiver, cancellation))
}

/// The containers the request reads: the service's or the project's, never
/// one that ran once for a job, and only the named one when one is named.
async fn selected_containers(
    api: &EngineApi,
    project: &str,
    request: &LogRequest,
) -> Result<Vec<EngineContainer>, MmError> {
    let mut containers = project_containers(api, project, request.service()).await?;
    containers.retain(|container| !container.observation.is_one_off());
    let named = request
        .selector
        .as_ref()
        .and_then(|selector| selector.container.as_deref());
    if let Some(named) = named {
        containers.retain(|container| {
            container.observation.id.starts_with(named) || container.observation.name == named
        });
        if containers.is_empty() {
            return Err(MmError::RuntimeResponse {
                message: "the requested container is not owned by the selected service".to_owned(),
            });
        }
    }
    Ok(containers)
}

/// The last lines of all the containers together, oldest first.
async fn read_tail(
    shared: &Arc<Shared>,
    containers: &[EngineContainer],
    sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
) -> Result<(), MmError> {
    let reads = containers.iter().map(|container| async move {
        let mut sink = Sink::Collect(Vec::new());
        shared
            .container_lines(container, Resume::default(), &mut sink)
            .await?;
        Ok::<_, MmError>(match sink {
            Sink::Collect(lines) => lines,
            Sink::Forward { .. } => Vec::new(),
        })
    });
    let lists = futures_util::future::join_all(reads)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let tail = usize::try_from(shared.request.tail).unwrap_or(usize::MAX);
    for record in merged_tail(lists, tail) {
        if sender.send(Ok(record)).await.is_err() {
            break;
        }
    }
    Ok(())
}

/// Where a container's lines go as they are read.
enum Sink {
    /// A read that ends keeps them until every container is read.
    Collect(Vec<LogRecord>),
    /// A read that follows hands each to the merge.
    Forward {
        source: usize,
        arrivals: tokio::sync::mpsc::Sender<Arrival>,
        newest: Arc<Mutex<Option<LogInstant>>>,
    },
}

impl Sink {
    /// Takes a line; `false` when nobody reads any more.
    async fn take(&mut self, record: LogRecord) -> bool {
        match self {
            Self::Collect(lines) => {
                lines.push(record);
                true
            }
            Self::Forward {
                source,
                arrivals,
                newest,
            } => {
                if let Some(at) = record.timestamp.as_deref().and_then(log_instant) {
                    *lock(newest) = Some(at);
                }
                arrivals.send((*source, Some(Ok(record)))).await.is_ok()
            }
        }
    }
}

/// One container's reader, as the followed read knows it.
struct Attached {
    /// Whether its stream is still open.
    open: bool,
    /// The newest line it delivered: where a new stream takes up after the
    /// container restarted.
    newest: Arc<Mutex<Option<LogInstant>>>,
}

/// A line of the merge's source, or `None` when that source has ended.
type Arrival = (usize, Option<Result<LogRecord, MmError>>);

/// A read that follows: the containers it reads and their merge.
struct Following {
    shared: Arc<Shared>,
    merge: LogMerge,
    /// By container id.
    attached: BTreeMap<String, Attached>,
    /// The container of each of the merge's sources.
    sources: Vec<String>,
    readers: tokio::task::JoinSet<()>,
    arrivals: tokio::sync::mpsc::Sender<Arrival>,
}

impl Following {
    /// Starts reading `container` unless a stream of it is open. The first
    /// look honours the request's tail; a container that appears later is
    /// read from its beginning, and one that restarted from where its last
    /// stream ended.
    fn attach(&mut self, container: EngineContainer, first: bool) {
        let id = container.observation.id.clone();
        let known = self.attached.entry(id.clone()).or_insert_with(|| Attached {
            open: false,
            newest: Arc::default(),
        });
        if known.open {
            return;
        }
        known.open = true;
        let source = self.merge.add_source();
        self.sources.push(id);
        let resume = Resume {
            everything: !first,
            after: *lock(&known.newest),
        };
        let shared = Arc::clone(&self.shared);
        let arrivals = self.arrivals.clone();
        let mut sink = Sink::Forward {
            source,
            arrivals: arrivals.clone(),
            newest: Arc::clone(&known.newest),
        };
        self.readers.spawn(async move {
            let ended = match shared.container_lines(&container, resume, &mut sink).await {
                Ok(()) => None,
                Err(error) => Some(Err(error)),
            };
            if arrivals.send((source, ended)).await.is_err() {
                tracing::debug!("a followed log ended after its reader");
            }
        });
    }

    fn close(&mut self, source: usize) {
        self.merge.end(source);
        if let Some(known) = self
            .sources
            .get(source)
            .and_then(|id| self.attached.get_mut(id))
        {
            known.open = false;
        }
    }

    /// Attaches the containers that run now and are not being read.
    async fn look(&mut self) {
        let service = self.shared.request.service();
        match project_containers(&self.shared.api, &self.shared.project, service).await {
            Ok(found) => {
                for container in found {
                    if !container.observation.is_one_off() && container.observation.is_running() {
                        self.attach(container, false);
                    }
                }
            }
            Err(error) => tracing::debug!(%error, "could not look for new containers"),
        }
    }
}

async fn follow(
    shared: &Arc<Shared>,
    containers: Vec<EngineContainer>,
    sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
    cancellation: &OperationCancellation,
) -> Result<(), MmError> {
    let started = tokio::time::Instant::now();
    let (arrivals, mut arrived) = tokio::sync::mpsc::channel::<Arrival>(256);
    // Dropping the readers with this value ends their streams.
    let mut following = Following {
        shared: Arc::clone(shared),
        merge: LogMerge::following(0, HOST_HOLD),
        attached: BTreeMap::new(),
        sources: Vec::new(),
        readers: tokio::task::JoinSet::new(),
        arrivals,
    };
    for container in containers {
        following.attach(container, true);
    }
    let mut look = tokio::time::interval_at(started + LOOK_FOR_CONTAINERS, LOOK_FOR_CONTAINERS);
    loop {
        let wake = following.merge.next_release().map(|at| started + at);
        tokio::select! {
            arrival = arrived.recv() => match arrival {
                Some((source, Some(Ok(record)))) => {
                    following.merge.push(source, record, started.elapsed());
                }
                Some((source, Some(Err(error)))) => {
                    // The rate bound is the caller's; anything else is one
                    // container's stream, which the next look takes up.
                    if is_rate_bound(&error) {
                        return Err(error);
                    }
                    tracing::debug!(%error, "a container's log stream ended with an error");
                    following.close(source);
                }
                Some((source, None)) => following.close(source),
                None => return Ok(()),
            },
            () = async {
                match wake {
                    Some(wake) => tokio::time::sleep_until(wake).await,
                    None => std::future::pending().await,
                }
            } => {}
            _ = look.tick() => following.look().await,
            () = tokio::time::sleep(CANCEL_CHECK) => {
                if cancellation.is_cancelled() {
                    return Ok(());
                }
            }
        }
        for record in following.merge.release(started.elapsed()) {
            if sender.send(Ok(record)).await.is_err() {
                return Ok(());
            }
        }
    }
}

fn is_rate_bound(error: &MmError) -> bool {
    matches!(error, MmError::RuntimeResponse { message } if message == RATE_BOUND)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where one container's stream begins.
#[derive(Debug, Clone, Copy, Default)]
struct Resume {
    /// The whole log instead of the request's tail.
    everything: bool,
    /// Only lines newer than this one.
    after: Option<LogInstant>,
}

impl Shared {
    /// Reads one container's log into `sink` until the engine ends it or
    /// nobody reads any more.
    async fn container_lines(
        &self,
        container: &EngineContainer,
        resume: Resume,
        sink: &mut Sink,
    ) -> Result<(), MmError> {
        if let LineSource::Command {
            streaming,
            program,
            inherited,
        } = &self.lines
        {
            let command = Command {
                streaming: Arc::clone(streaming),
                program,
                inherited,
            };
            return self.command_lines(container, resume, sink, command).await;
        }
        let request = &self.request;
        let wants = |source: LogOutputPolicy| {
            // A terminal's output is one stream, which the engine files
            // under standard output.
            container.tty
                || matches!(request.output, LogOutputPolicy::Both)
                || request.output == source
        };
        let mut path = format!(
            "/containers/{}/logs?timestamps=true&stdout={}&stderr={}&follow={}",
            container.observation.id,
            wants(LogOutputPolicy::Stdout),
            wants(LogOutputPolicy::Stderr),
            request.follow,
        );
        match resume {
            Resume {
                after: Some((seconds, nanoseconds)),
                ..
            } => {
                path.push_str(&format!("&tail=all&since={seconds}.{nanoseconds:09}"));
            }
            Resume {
                everything: true, ..
            } => path.push_str("&tail=all"),
            Resume { .. } => {
                path.push_str(&format!("&tail={}", request.tail));
                if let Some(since) = request.since_unix_ms {
                    path.push_str(&format!("&since={}", engine_time(since)));
                }
            }
        }
        if let Some(until) = request.until_unix_ms {
            path.push_str(&format!("&until={}", engine_time(until)));
        }
        let Some(mut stream) = self.api.stream(&path).await? else {
            // Removed since it was listed: it has no lines any more.
            return Ok(());
        };
        let mut lines = LogDemux::new(container.tty, request.limits.maximum_line_bytes);
        while let Some(bytes) = stream.next().await? {
            for line in lines.push(&bytes) {
                if !self.deliver(container, line, resume, sink).await? {
                    return Ok(());
                }
            }
        }
        for line in lines.finish() {
            if !self.deliver(container, line, resume, sink).await? {
                break;
            }
        }
        Ok(())
    }

    async fn deliver(
        &self,
        container: &EngineContainer,
        line: Line,
        resume: Resume,
        sink: &mut Sink,
    ) -> Result<bool, MmError> {
        if let (Some(after), Some(at)) = (
            resume.after,
            line.timestamp.as_deref().and_then(log_instant),
        ) && at <= after
        {
            return Ok(true);
        }
        lock(&self.rate).consume(line.bytes.len())?;
        Ok(sink.take(self.record(container, line)).await)
    }

    fn record(&self, container: &EngineContainer, line: Line) -> LogRecord {
        let decoded = String::from_utf8_lossy(&line.bytes);
        let lossy_utf8 = matches!(decoded, std::borrow::Cow::Owned(_));
        let message = Zeroizing::new(decoded.into_owned());
        LogRecord {
            identity: ServiceIdentity {
                service: container.observation.service.clone(),
                ..self.identity.clone()
            },
            sequence: 0,
            source: line.source,
            node: None,
            container: Some(container.observation.name.clone()),
            timestamp: line.timestamp,
            message: redact_output(&message, &self.redactions),
            lossy_utf8,
            truncated: line.truncated,
            reconnect: 0,
        }
    }
}

/// Seconds with a fraction, as the engine's `since` and `until` read them.
fn engine_time(unix_ms: i64) -> String {
    format!(
        "{}.{:03}",
        unix_ms.div_euclid(1000),
        unix_ms.rem_euclid(1000)
    )
}

const RATE_BOUND: &str = "runtime log rate exceeded the configured bound";

/// The request's bound on lines and bytes per second, over all containers.
struct LogRate {
    started: tokio::time::Instant,
    bytes: u64,
    lines: u32,
    maximum_bytes: Option<u64>,
    maximum_lines: Option<u32>,
}

impl LogRate {
    fn new(request: &LogRequest) -> Self {
        Self {
            started: tokio::time::Instant::now(),
            bytes: 0,
            lines: 0,
            // A read that ends is bounded by its tail, not by time: every
            // line of it arrives at once.
            maximum_bytes: request
                .limits
                .maximum_bytes_per_second
                .filter(|_| request.follow),
            maximum_lines: request
                .limits
                .maximum_lines_per_second
                .filter(|_| request.follow),
        }
    }

    fn consume(&mut self, bytes: usize) -> Result<(), MmError> {
        if self.started.elapsed() >= Duration::from_secs(1) {
            self.started = tokio::time::Instant::now();
            self.bytes = 0;
            self.lines = 0;
        }
        self.bytes = self.bytes.saturating_add(bytes as u64);
        self.lines = self.lines.saturating_add(1);
        if self
            .maximum_bytes
            .is_some_and(|maximum| self.bytes > maximum)
            || self
                .maximum_lines
                .is_some_and(|maximum| self.lines > maximum)
        {
            return Err(MmError::RuntimeResponse {
                message: RATE_BOUND.to_owned(),
            });
        }
        Ok(())
    }
}

/// One line of a container's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub source: LogSource,
    pub timestamp: Option<String>,
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

/// Turns the bytes of the engine's log answer into lines.
///
/// Without a terminal the answer is a sequence of frames: one byte that
/// says standard output or standard error, three unused, a length, and one
/// message. A message is a line that begins with its timestamp; a line
/// longer than the engine's own buffer (16 KiB) arrives as several messages
/// of which only the last ends in a newline, each with a timestamp of its
/// own. With a terminal the answer is the output as it was written.
pub struct LogDemux {
    tty: bool,
    maximum: usize,
    buffered: Vec<u8>,
    stdout: Partial,
    stderr: Partial,
}

#[derive(Default)]
struct Partial {
    timestamp: Option<String>,
    bytes: Vec<u8>,
    truncated: bool,
    /// Whether a line has begun and not ended.
    open: bool,
}

impl LogDemux {
    #[must_use]
    pub fn new(tty: bool, maximum_line_bytes: usize) -> Self {
        Self {
            tty,
            maximum: maximum_line_bytes,
            buffered: Vec::new(),
            stdout: Partial::default(),
            stderr: Partial::default(),
        }
    }

    /// Takes the next bytes of the answer and returns the lines they end.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Line> {
        let mut lines = Vec::new();
        if self.tty {
            self.stdout
                .raw(bytes, self.maximum, LogSource::Stdout, &mut lines);
            return lines;
        }
        self.buffered.extend_from_slice(bytes);
        let mut start = 0;
        while let Some(header) = self.buffered.get(start..start + 8) {
            let size = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
            let Some(message) = self.buffered.get(start + 8..start + 8 + size) else {
                break;
            };
            let (partial, source) = if header[0] == 2 {
                (&mut self.stderr, LogSource::Stderr)
            } else {
                (&mut self.stdout, LogSource::Stdout)
            };
            partial.message(message, self.maximum, source, &mut lines);
            start += 8 + size;
        }
        self.buffered.drain(..start);
        lines
    }

    /// Takes the next bytes the engine's `logs --timestamps` command wrote
    /// on `source`: lines, each beginning with its timestamp.
    pub fn push_text(&mut self, source: LogSource, bytes: &[u8]) -> Vec<Line> {
        let mut lines = Vec::new();
        let partial = if source == LogSource::Stderr {
            &mut self.stderr
        } else {
            &mut self.stdout
        };
        partial.text(bytes, self.maximum, source, &mut lines);
        lines
    }

    /// The lines that were begun and never ended.
    pub fn finish(&mut self) -> Vec<Line> {
        let mut lines = Vec::new();
        self.stdout.end(LogSource::Stdout, &mut lines);
        self.stderr.end(LogSource::Stderr, &mut lines);
        lines
    }
}

impl Partial {
    /// One framed message: a timestamp, then a line or a piece of one.
    fn message(
        &mut self,
        message: &[u8],
        maximum: usize,
        source: LogSource,
        lines: &mut Vec<Line>,
    ) {
        let (timestamp, text) = split_timestamp(message);
        // The piece that continues a line keeps the line's first timestamp.
        if !self.open {
            self.timestamp = timestamp.clone();
        }
        for byte in text {
            if *byte == b'\n' {
                self.end(source, lines);
                // A message with more than one line: the rest shares its
                // timestamp.
                self.timestamp.clone_from(&timestamp);
            } else {
                self.take(*byte, maximum);
            }
        }
        if !self.open {
            self.timestamp = None;
        }
    }

    /// Output of a terminal: lines as they were written, each beginning
    /// with its timestamp.
    fn raw(&mut self, bytes: &[u8], maximum: usize, source: LogSource, lines: &mut Vec<Line>) {
        for byte in bytes {
            if *byte == b'\n' {
                let (timestamp, text) = split_timestamp(&self.bytes);
                let text = text.to_vec();
                self.timestamp = timestamp;
                self.bytes = text;
                self.end(source, lines);
            } else {
                // Room for the timestamp the line begins with.
                self.take(*byte, maximum.saturating_add(40));
            }
        }
    }

    /// Output of the engine's `logs --timestamps` command: lines, each
    /// beginning with its timestamp, cut to `maximum` bytes after it.
    fn text(&mut self, bytes: &[u8], maximum: usize, source: LogSource, lines: &mut Vec<Line>) {
        for byte in bytes {
            if *byte == b'\n' {
                let (timestamp, text) = split_timestamp(&self.bytes);
                let mut text = match timestamp.as_deref() {
                    Some(leading) => command::without_piece_stamps(text, leading),
                    None => text.to_vec(),
                };
                if text.len() > maximum {
                    text.truncate(maximum);
                    self.truncated = true;
                }
                self.timestamp = timestamp;
                self.bytes = text;
                self.end(source, lines);
            } else {
                // Room for the timestamps of the line and of its pieces.
                self.take(*byte, maximum.saturating_mul(2).saturating_add(64));
            }
        }
    }

    fn take(&mut self, byte: u8, maximum: usize) {
        self.open = true;
        if self.bytes.len() < maximum {
            self.bytes.push(byte);
        } else {
            self.truncated = true;
        }
    }

    fn end(&mut self, source: LogSource, lines: &mut Vec<Line>) {
        if !self.open && self.bytes.is_empty() && self.timestamp.is_none() {
            return;
        }
        if self.bytes.last() == Some(&b'\r') {
            self.bytes.pop();
        }
        lines.push(Line {
            source,
            timestamp: self.timestamp.take(),
            bytes: std::mem::take(&mut self.bytes),
            truncated: std::mem::take(&mut self.truncated),
        });
        self.open = false;
    }
}

/// The timestamp a message begins with, and the rest of it.
fn split_timestamp(message: &[u8]) -> (Option<String>, &[u8]) {
    let Some(space) = message.iter().take(40).position(|byte| *byte == b' ') else {
        return (None, message);
    };
    match std::str::from_utf8(&message[..space]) {
        Ok(candidate) if log_instant(candidate).is_some() => {
            (Some(candidate.to_owned()), &message[space + 1..])
        }
        _ => (None, message),
    }
}
