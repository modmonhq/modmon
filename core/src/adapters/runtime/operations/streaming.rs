#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JobContainerInspect {
    id: String,
    name: String,
    config: JobContainerConfig,
    state: JobContainerState,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JobContainerConfig {
    labels: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JobContainerState {
    running: bool,
    exit_code: i32,
}

struct OwnedJobContainer {
    id: String,
    running: bool,
    exit_code: i32,
}

impl RuntimeOperationsAdapter {
    fn compose_prefix(&self, target: &RuntimeTarget) -> Result<Vec<String>, MmError> {
        if target.compose_files.is_empty() {
            return Err(MmError::Security {
                message: "refusing a Compose operation without explicit files".to_owned(),
            });
        }
        let mut args = vec!["compose".to_owned()];
        for file in &target.compose_files {
            args.extend(["--file".to_owned(), file.to_string_lossy().into_owned()]);
        }
        args.extend([
            "--project-directory".to_owned(),
            target.project_directory.to_string_lossy().into_owned(),
            "--project-name".to_owned(),
            target.project_name.clone(),
        ]);
        Ok(args)
    }

    async fn owned_job_container(
        &self,
        target: &RuntimeTarget,
        runtime_name: &str,
    ) -> Result<Option<OwnedJobContainer>, MmError> {
        if !valid_job_name(runtime_name) {
            return Err(MmError::Security {
                message: "one-shot runtime name is invalid".to_owned(),
            });
        }
        let output = self
            .command(vec![
                "container".to_owned(),
                "inspect".to_owned(),
                runtime_name.to_owned(),
            ])
            .await?;
        let missing_exit = output.code == Some(1)
            || (self.kind == RuntimeKind::PodmanCompose && output.code == Some(125));
        if missing_exit && output.stderr.to_ascii_lowercase().contains("no such") {
            return Ok(None);
        }
        require_success(&output, self.program(), "inspect one-shot job")?;
        if output.truncated {
            return Err(MmError::RuntimeResponse {
                message: "one-shot inspection output was truncated".to_owned(),
            });
        }
        let mut inspected = serde_json::from_str::<Vec<JobContainerInspect>>(&output.stdout)
            .map_err(|error| MmError::RuntimeResponse {
                message: format!("runtime returned invalid one-shot inspection JSON: {error}"),
            })?;
        let Some(container) = inspected.pop() else {
            return Ok(None);
        };
        if !inspected.is_empty()
            || container.name.trim_start_matches('/') != runtime_name
            || container.config.labels.get("com.docker.compose.project")
                != Some(&target.project_name)
        {
            return Err(MmError::Security {
                message: "refusing a one-shot container without exact project ownership".to_owned(),
            });
        }
        Ok(Some(OwnedJobContainer {
            id: container.id,
            running: container.state.running,
            exit_code: container.state.exit_code,
        }))
    }
}

fn bounded_output(output: CommandOutput, maximum: usize) -> (String, bool) {
    let combined = if output.stderr.is_empty() {
        output.stdout.to_string()
    } else if output.stdout.is_empty() {
        output.stderr.to_string()
    } else {
        format!("{}\n{}", output.stdout.trim_end(), output.stderr.as_str())
    };
    if combined.len() <= maximum {
        return (combined, output.truncated);
    }
    let mut end = maximum;
    while !combined.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    (combined[..end].to_owned(), true)
}

fn valid_job_name(value: &str) -> bool {
    value.starts_with("mm-job-")
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn verify_runtime_id(expected: Option<&str>, actual: &str) -> Result<(), MmError> {
    if expected.is_none_or(|expected| expected == actual || actual.starts_with(expected)) {
        Ok(())
    } else {
        Err(MmError::Security {
            message: "one-shot runtime identity changed ownership".to_owned(),
        })
    }
}

struct ForwardLogsContext {
    streaming: Arc<dyn StreamingProcessRunner>,
    program: String,
    inherited: &'static [&'static str],
    root: std::path::PathBuf,
    args: Vec<String>,
    identity: ServiceIdentity,
    request: LogRequest,
    redactions: Vec<Zeroizing<String>>,
    cancellation: OperationCancellation,
    sender: tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
}

async fn forward_logs(context: ForwardLogsContext) -> Result<(), MmError> {
    let sender = context.sender.clone();
    let mut held = (!context.request.follow).then(|| LastLines::new(context.request.tail));
    let result = stream_logs(context, &mut held).await;
    // What was read before a failure is still sent, ahead of the failure.
    if let Some(held) = held {
        held.send(&sender).await;
    }
    result
}

async fn stream_logs(
    context: ForwardLogsContext,
    held: &mut Option<LastLines>,
) -> Result<(), MmError> {
    let ForwardLogsContext {
        streaming,
        program,
        inherited,
        root,
        args,
        identity,
        request,
        redactions,
        cancellation,
        sender,
    } = context;
    let mut sequence = 0_u64;
    let mut reconnect = 0_u32;
    loop {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let spec = log_stream_spec(&program, inherited, &root, &args, &request);
        let mut process = streaming.spawn(spec, cancellation.clone()).await?;
        let mut lines = LineBuffer::new(request.limits.maximum_line_bytes);
        let mut rate = LogRate::new(&request);
        let mut exit_code = None;
        while let Some(event) = process.recv().await {
            match event {
                ProcessStreamEvent::Output { source, bytes } => {
                    if !include_source(request.output, source) {
                        continue;
                    }
                    for (line, truncated) in lines.push(source, &bytes) {
                        rate.consume(line.len())?;
                        sequence = sequence.saturating_add(1);
                        let record = log_record(LogRecordInput {
                            identity: &identity,
                            sequence,
                            source,
                            line,
                            truncated,
                            reconnect,
                            timestamps: reads_timestamps(&request),
                            redactions: &redactions,
                        });
                        if !deliver(record, held, &sender).await {
                            process.cancel();
                            return Ok(());
                        }
                    }
                }
                ProcessStreamEvent::Exited { code } => {
                    exit_code = code;
                    break;
                }
                ProcessStreamEvent::Failed(error) => return Err(error),
            }
        }
        for (source, line, truncated) in lines.finish() {
            if include_source(request.output, source) {
                rate.consume(line.len())?;
                sequence = sequence.saturating_add(1);
                let record = log_record(LogRecordInput {
                    identity: &identity,
                    sequence,
                    source,
                    line,
                    truncated,
                    reconnect,
                    timestamps: reads_timestamps(&request),
                    redactions: &redactions,
                });
                if !deliver(record, held, &sender).await {
                    return Ok(());
                }
            }
        }
        if cancellation.is_cancelled() {
            return Ok(());
        }
        if !request.follow && exit_code != Some(0) {
            return Err(MmError::RuntimeResponse {
                message: format!("runtime log stream failed with exit code {exit_code:?}"),
            });
        }
        if !request.follow || reconnect >= LOG_RECONNECT_ATTEMPTS {
            return if request.follow && reconnect >= LOG_RECONNECT_ATTEMPTS {
                Err(MmError::RuntimeResponse {
                    message: "runtime log stream ended after bounded reconnect attempts".to_owned(),
                })
            } else {
                Ok(())
            };
        }
        reconnect += 1;
        wait_for_log_reconnect(reconnect).await;
    }
}

/// Holds the record for a read that ends, or sends it on. `false` when the
/// receiver is gone.
async fn deliver(
    record: LogRecord,
    held: &mut Option<LastLines>,
    sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>,
) -> bool {
    match held {
        Some(held) => {
            held.push(record);
            true
        }
        None => sender.send(Ok(record)).await.is_ok(),
    }
}

fn log_stream_spec(
    program: &str,
    inherited: &'static [&'static str],
    root: &std::path::Path,
    args: &[String],
    request: &LogRequest,
) -> StreamCommandSpec {
    StreamCommandSpec::new(program)
        .args(args.to_vec())
        .inherit_environment(inherited)
        .current_dir(root)
        .timeout(log_stream_timeout(request.follow))
        .buffered_events(request.limits.buffered_records)
}

async fn wait_for_log_reconnect(reconnect: u32) {
    tokio::time::sleep(Duration::from_millis(250 * u64::from(reconnect))).await;
}

fn log_stream_timeout(follow: bool) -> Duration {
    if follow {
        Duration::from_secs(24 * 60 * 60)
    } else {
        Duration::from_secs(120)
    }
}

async fn forward_exec(
    mut process: crate::app::ports::ProcessStream,
    sender: &tokio::sync::mpsc::Sender<Result<ExecEvent, MmError>>,
    runtime: RuntimeKind,
    identity: ServiceIdentity,
    container_id: String,
    started_at_unix_ms: i64,
    suppress_output: bool,
) -> Result<(), MmError> {
    while let Some(event) = process.recv().await {
        match event {
            ProcessStreamEvent::Output { source, bytes } => {
                if suppress_output {
                    continue;
                }
                let value = String::from_utf8_lossy(&bytes);
                let event = ExecEvent::Output {
                    source: match source {
                        ProcessOutputSource::Stdout => ExecOutputSource::Stdout,
                        ProcessOutputSource::Stderr => ExecOutputSource::Stderr,
                    },
                    lossy_utf8: matches!(value, std::borrow::Cow::Owned(_)),
                    value: value.into_owned(),
                };
                if sender.send(Ok(event)).await.is_err() {
                    process.cancel();
                    return Ok(());
                }
            }
            ProcessStreamEvent::Exited { code } => {
                let code = code.ok_or_else(|| MmError::RuntimeResponse {
                    message: "remote command ended without an exit code".to_owned(),
                })?;
                sender
                    .send(Ok(ExecEvent::Finished(ExecOutcome {
                        runtime,
                        identity,
                        container_id,
                        exit_code: code,
                        started_at_unix_ms,
                        finished_at_unix_ms: unix_now_ms(),
                    })))
                    .await
                    .map_err(|_| MmError::State {
                        message: "exec consumer disconnected".to_owned(),
                    })?;
                return Ok(());
            }
            ProcessStreamEvent::Failed(error) => return Err(error),
        }
    }
    Ok(())
}

struct LineBuffer {
    maximum: usize,
    stdout: PartialLine,
    stderr: PartialLine,
}

impl LineBuffer {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            stdout: PartialLine::default(),
            stderr: PartialLine::default(),
        }
    }

    fn push(&mut self, source: ProcessOutputSource, bytes: &[u8]) -> Vec<(Vec<u8>, bool)> {
        let line = match source {
            ProcessOutputSource::Stdout => &mut self.stdout,
            ProcessOutputSource::Stderr => &mut self.stderr,
        };
        line.push(bytes, self.maximum)
    }

    fn finish(mut self) -> Vec<(ProcessOutputSource, Vec<u8>, bool)> {
        let mut values = Vec::new();
        if let Some((line, truncated)) = self.stdout.finish() {
            values.push((ProcessOutputSource::Stdout, line, truncated));
        }
        if let Some((line, truncated)) = self.stderr.finish() {
            values.push((ProcessOutputSource::Stderr, line, truncated));
        }
        values
    }
}

#[derive(Default)]
struct PartialLine {
    retained: Vec<u8>,
    truncated: bool,
}

impl PartialLine {
    fn push(&mut self, bytes: &[u8], maximum: usize) -> Vec<(Vec<u8>, bool)> {
        let mut completed = Vec::new();
        for byte in bytes {
            if *byte == b'\n' {
                if self.retained.last() == Some(&b'\r') {
                    self.retained.pop();
                }
                completed.push((std::mem::take(&mut self.retained), self.truncated));
                self.truncated = false;
            } else if self.retained.len() < maximum {
                self.retained.push(*byte);
            } else {
                self.truncated = true;
            }
        }
        completed
    }

    fn finish(&mut self) -> Option<(Vec<u8>, bool)> {
        if self.retained.is_empty() && !self.truncated {
            None
        } else {
            Some((std::mem::take(&mut self.retained), self.truncated))
        }
    }
}

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
            maximum_bytes: request.limits.maximum_bytes_per_second,
            maximum_lines: request.limits.maximum_lines_per_second,
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
                message: "runtime log rate exceeded the configured bound".to_owned(),
            });
        }
        Ok(())
    }
}

struct LogRecordInput<'a> {
    identity: &'a ServiceIdentity,
    sequence: u64,
    source: ProcessOutputSource,
    line: Vec<u8>,
    truncated: bool,
    reconnect: u32,
    timestamps: bool,
    redactions: &'a [Zeroizing<String>],
}

fn log_record(input: LogRecordInput<'_>) -> LogRecord {
    let LogRecordInput {
        identity,
        sequence,
        source,
        line,
        truncated,
        reconnect,
        timestamps,
        redactions,
    } = input;
    let decoded = String::from_utf8_lossy(&line);
    let lossy_utf8 = matches!(decoded, std::borrow::Cow::Owned(_));
    let mut message = Zeroizing::new(decoded.into_owned());
    let compose_value = message
        .split_once(" | ")
        .filter(|(prefix, _)| {
            let prefix = prefix.trim();
            prefix.len() <= 128
                && prefix.contains(&identity.service)
                && prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
        .map(|(_, value)| value.to_owned());
    if let Some(value) = compose_value {
        *message = value;
    }
    let timestamp = if timestamps {
        let parsed = message
            .split_once(' ')
            .filter(|(candidate, _)| timestamp_order(candidate).is_some())
            .map(|(timestamp, rest)| (timestamp.to_owned(), rest.to_owned()));
        parsed.map(|(timestamp, rest)| {
            *message = rest;
            timestamp
        })
    } else {
        None
    };
    let value = crate::domain::observability::redact_output(&message, redactions);
    message.zeroize();
    LogRecord {
        identity: identity.clone(),
        sequence,
        source: match source {
            ProcessOutputSource::Stdout => LogSource::Stdout,
            ProcessOutputSource::Stderr => LogSource::Stderr,
        },
        node: None,
        container: None,
        timestamp,
        message: value,
        lossy_utf8,
        truncated,
        reconnect,
    }
}

fn include_source(policy: LogOutputPolicy, source: ProcessOutputSource) -> bool {
    matches!(
        (policy, source),
        (LogOutputPolicy::Both, _)
            | (LogOutputPolicy::Stdout, ProcessOutputSource::Stdout)
            | (LogOutputPolicy::Stderr, ProcessOutputSource::Stderr)
    )
}

fn append_log_options(args: &mut Vec<String>, request: &LogRequest) {
    args.extend(["--tail".to_owned(), request.tail.to_string()]);
    if let Some(since) = request.since_unix_ms {
        args.extend(["--since".to_owned(), docker_timestamp(since)]);
    }
    if let Some(until) = request.until_unix_ms {
        args.extend(["--until".to_owned(), docker_timestamp(until)]);
    }
    if request.follow {
        args.push("--follow".to_owned());
    }
    if reads_timestamps(request) {
        args.push("--timestamps".to_owned());
    }
}

/// The runtime is always asked for timestamps: they put the lines of
/// several containers, and of several nodes, back in the order they were
/// written. A runtime applies `--tail` to each container, stopped ones
/// included, and returns their lines one container after another.
/// Whether a reader sees the timestamps is the interface's choice.
fn reads_timestamps(_request: &LogRequest) -> bool {
    true
}

/// When a line was written, for ordering: seconds, then the fraction of a
/// second. `None` for a line whose timestamp the runtime did not print.
fn timestamp_order(timestamp: &str) -> Option<(i64, u32)> {
    match time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
    {
        Ok(instant) => Some((instant.unix_timestamp(), instant.nanosecond())),
        // Not a timestamp: the line starts with ordinary text.
        Err(_) => None,
    }
}

/// Keeps the last `tail` records of a read that ends, by when each line was
/// written, without holding more than about twice that many. Such a read is
/// held back until it is complete, then sent in order.
struct LastLines {
    tail: usize,
    /// The order of the record before, for a line printed without a timestamp.
    previous: Option<(i64, u32)>,
    records: Vec<(Option<(i64, u32)>, LogRecord)>,
}

impl LastLines {
    /// How far past `2 * tail` the buffer grows before older records are dropped.
    const SLACK: usize = 1024;

    fn new(tail: u32) -> Self {
        Self {
            tail: usize::try_from(tail).unwrap_or(usize::MAX),
            previous: None,
            records: Vec::new(),
        }
    }

    /// A record without a timestamp of its own stays behind the one it followed.
    fn push(&mut self, record: LogRecord) {
        let order = record
            .timestamp
            .as_deref()
            .and_then(timestamp_order)
            .or(self.previous);
        self.previous = order;
        self.records.push((order, record));
        if self.records.len() > self.tail.saturating_mul(2).saturating_add(Self::SLACK) {
            self.keep_last();
        }
    }

    fn keep_last(&mut self) {
        // Stable: lines of one instant keep the order they arrived in.
        self.records.sort_by_key(|(order, _)| *order);
        let dropped = self.records.len().saturating_sub(self.tail);
        self.records.drain(..dropped);
    }

    /// Sends the kept records; stops quietly when the receiver is gone.
    async fn send(self, sender: &tokio::sync::mpsc::Sender<Result<LogRecord, MmError>>) {
        for record in self.finish() {
            if sender.send(Ok(record)).await.is_err() {
                break;
            }
        }
    }

    /// The kept records, oldest first, numbered from one.
    fn finish(mut self) -> Vec<LogRecord> {
        self.keep_last();
        self.records
            .into_iter()
            .zip(1_u64..)
            .map(|((_, mut record), sequence)| {
                record.sequence = sequence;
                record
            })
            .collect()
    }
}

fn successful_json_ids(
    output: CommandOutput,
    program: &str,
    keys: &[&str],
) -> Result<Vec<String>, MmError> {
    Ok(successful_json_rows(output, program)?
        .iter()
        .filter_map(|record| {
            keys.iter()
                .find_map(|key| record.get(*key).and_then(Value::as_str))
                .map(str::to_owned)
        })
        .collect())
}

/// The rows of an engine's `ps --format json`, one JSON object a line or
/// one array.
fn successful_json_rows(output: CommandOutput, program: &str) -> Result<Vec<Value>, MmError> {
    require_success(&output, program, "select service container")?;
    if output.truncated {
        return Err(MmError::RuntimeResponse {
            message: "runtime container selection output was truncated".to_owned(),
        });
    }
    let trimmed = output.stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let records = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<Value>>(trimmed)
    } else {
        trimmed
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<Vec<Value>, _>>()
    }
    .map_err(|error| MmError::RuntimeResponse {
        message: format!("runtime returned invalid container selection JSON: {error}"),
    })?;
    Ok(records)
}

fn require_success(output: &CommandOutput, program: &str, operation: &str) -> Result<(), MmError> {
    if output.success() {
        Ok(())
    } else {
        Err(MmError::ProcessFailed {
            program: format!("{program} ({operation})"),
            code: output.code,
            stderr: output.stderr.trim().to_owned(),
        })
    }
}

fn valid_container_id(value: &str) -> bool {
    (12..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn docker_timestamp(unix_ms: i64) -> String {
    let seconds = unix_ms.div_euclid(1000);
    let milliseconds = unix_ms.rem_euclid(1000);
    format!("{seconds}.{milliseconds:03}")
}

fn identity(
    target: &RuntimeTarget,
    selector: &crate::domain::observability::ServiceSelector,
) -> ServiceIdentity {
    ServiceIdentity {
        project_id: target.project_id,
        environment: target.environment.clone(),
        stack: target.stack_name.clone(),
        module: selector.module.clone(),
        service: selector.service.clone(),
    }
}

fn unix_now_ms() -> i64 {
    let Ok(duration) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return 0;
    };
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
