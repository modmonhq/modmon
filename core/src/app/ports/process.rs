use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::domain::error::MmError;
use crate::domain::observability::{ExecStdinMode, ExecTtyMode, OperationCancellation};
use crate::domain::runtime::RuntimeEnvironment;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RedactedCommandRecord {
    pub program: String,
    pub arguments: Vec<String>,
    pub environment_keys: Vec<String>,
    pub environment_count: usize,
}

pub struct CommandSpec {
    program: String,
    args: Vec<CommandValue>,
    current_dir: Option<PathBuf>,
    environment: BTreeMap<String, CommandValue>,
    redactions: Vec<Zeroizing<String>>,
    sensitive_stdin: Option<Zeroizing<String>>,
    stdin_file: Option<std::fs::File>,
    inherit_stdin: bool,
    timeout: Duration,
    max_output_bytes: usize,
    internal_stdout: bool,
}

impl CommandSpec {
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            environment: BTreeMap::new(),
            redactions: Vec::new(),
            sensitive_stdin: None,
            stdin_file: None,
            inherit_stdin: false,
            timeout: Duration::from_secs(30),
            max_output_bytes: 4 * 1024 * 1024,
            internal_stdout: false,
        }
    }

    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args
            .into_iter()
            .map(|value| CommandValue::Public(value.into()))
            .collect();
        self
    }

    #[must_use]
    pub fn args_append(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(
            args.into_iter()
                .map(|value| CommandValue::Public(value.into())),
        );
        self
    }

    #[must_use]
    pub fn arg(mut self, value: impl Into<String>) -> Self {
        self.args.push(CommandValue::Public(value.into()));
        self
    }

    #[must_use]
    pub fn sensitive_arg(mut self, value: String) -> Self {
        self.args
            .push(CommandValue::Sensitive(Zeroizing::new(value)));
        self
    }

    #[must_use]
    pub fn redact_value(mut self, value: String) -> Self {
        if !value.is_empty() {
            self.redactions.push(Zeroizing::new(value));
        }
        self
    }

    #[must_use]
    pub fn environment(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment
            .insert(key.into(), CommandValue::Public(value.into()));
        self
    }

    #[must_use]
    pub fn sensitive_environment(mut self, key: impl Into<String>, value: String) -> Self {
        self.environment
            .insert(key.into(), CommandValue::Sensitive(Zeroizing::new(value)));
        self
    }

    #[must_use]
    pub fn interpolation_environment(mut self, environment: &RuntimeEnvironment) -> Self {
        for (key, value) in environment.values() {
            self.environment.insert(
                key.to_owned(),
                CommandValue::Sensitive(Zeroizing::new(value.to_owned())),
            );
        }
        self
    }

    #[must_use]
    pub fn inherit_environment(mut self, keys: &[&str]) -> Self {
        for key in keys {
            let value = std::env::var_os(key).and_then(|value| match value.into_string() {
                Ok(value) => Some(value),
                Err(_) => {
                    tracing::debug!(
                        key,
                        "ignoring inherited environment value that is not UTF-8"
                    );
                    None
                }
            });
            if let Some(value) = value {
                self.environment
                    .insert((*key).to_owned(), CommandValue::Public(value));
            }
        }
        self
    }

    #[must_use]
    pub fn current_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(path.into());
        self
    }

    #[must_use]
    pub const fn inherit_stdin(mut self) -> Self {
        self.inherit_stdin = true;
        self
    }

    /// Pipes sensitive bytes to the child without placing them in argv or its environment.
    #[must_use]
    pub fn sensitive_stdin(mut self, value: Zeroizing<String>) -> Self {
        self.sensitive_stdin = Some(value);
        self.stdin_file = None;
        self.inherit_stdin = false;
        self
    }

    /// Gives the child `file` as its standard input, read from the file's
    /// current offset.
    #[must_use]
    pub fn stdin_file(mut self, file: std::fs::File) -> Self {
        self.stdin_file = Some(file);
        self.sensitive_stdin = None;
        self.inherit_stdin = false;
        self
    }

    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Keeps sensitive values in standard output instead of replacing them
    /// with `[REDACTED]`, for output mm parses itself and never shows, such
    /// as a rendered Compose file: the marker corrupts its YAML. The output
    /// stays zeroized; standard error is still redacted.
    #[must_use]
    pub const fn internal_stdout(mut self) -> Self {
        self.internal_stdout = true;
        self
    }

    pub(crate) const fn keeps_internal_stdout(&self) -> bool {
        self.internal_stdout
    }

    #[must_use]
    pub const fn with_output_limit(mut self, maximum: usize) -> Self {
        self.max_output_bytes = maximum;
        self
    }

    #[must_use]
    pub fn redacted_display(&self) -> String {
        let args = self
            .args
            .iter()
            .map(|argument| argument.redacted())
            .collect::<Vec<_>>()
            .join(" ");
        if args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {args}", self.program)
        }
    }

    #[must_use]
    pub fn redacted_record(&self) -> RedactedCommandRecord {
        RedactedCommandRecord {
            program: self.program.clone(),
            arguments: self
                .args
                .iter()
                .map(|argument| argument.redacted().to_owned())
                .collect(),
            environment_keys: self.environment.keys().cloned().collect(),
            environment_count: self.environment.len(),
        }
    }

    pub(crate) fn program(&self) -> &str {
        &self.program
    }

    pub(crate) fn arguments(&self) -> impl Iterator<Item = &str> {
        self.args.iter().map(CommandValue::expose)
    }

    pub(crate) fn environment_values(&self) -> impl Iterator<Item = (&str, &str)> {
        self.environment
            .iter()
            .map(|(key, value)| (key.as_str(), value.expose()))
    }

    pub(crate) fn sensitive_values(&self) -> impl Iterator<Item = &str> {
        self.args
            .iter()
            .chain(self.environment.values())
            .filter_map(CommandValue::sensitive)
            .chain(self.redactions.iter().map(|value| value.as_str()))
            .chain(self.sensitive_stdin.iter().map(|value| value.as_str()))
    }

    pub(crate) fn current_directory(&self) -> Option<&PathBuf> {
        self.current_dir.as_ref()
    }

    pub(crate) const fn timeout_value(&self) -> Duration {
        self.timeout
    }

    pub(crate) const fn output_limit(&self) -> usize {
        self.max_output_bytes
    }

    pub(crate) const fn should_inherit_stdin(&self) -> bool {
        self.inherit_stdin
    }

    pub(crate) fn take_sensitive_stdin(&mut self) -> Option<Zeroizing<String>> {
        self.sensitive_stdin.take()
    }

    /// The file given with [`CommandSpec::stdin_file`], for the runner to
    /// hand to the child.
    pub fn take_stdin_file(&mut self) -> Option<std::fs::File> {
        self.stdin_file.take()
    }
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandSpec")
            .field("command", &self.redacted_display())
            .field("current_dir", &self.current_dir)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .field("environment_count", &self.environment.len())
            .field("inherit_stdin", &self.inherit_stdin)
            .field(
                "sensitive_stdin",
                &self.sensitive_stdin.as_ref().map(|_| "[REDACTED]"),
            )
            .field("stdin_file", &self.stdin_file.is_some())
            .field("timeout", &self.timeout)
            .field("max_output_bytes", &self.max_output_bytes)
            .finish()
    }
}

enum CommandValue {
    Public(String),
    Sensitive(Zeroizing<String>),
}

impl CommandValue {
    fn expose(&self) -> &str {
        match self {
            Self::Public(value) => value,
            Self::Sensitive(value) => value,
        }
    }

    fn redacted(&self) -> &str {
        match self {
            Self::Public(value) => value,
            Self::Sensitive(_) => "[REDACTED]",
        }
    }

    fn sensitive(&self) -> Option<&str> {
        match self {
            Self::Public(_) => None,
            Self::Sensitive(value) => Some(value),
        }
    }
}

pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: Zeroizing<String>,
    pub stderr: Zeroizing<String>,
    pub truncated: bool,
}

impl std::fmt::Debug for CommandOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandOutput")
            .field("code", &self.code)
            .field("stdout", &"[REDACTED]")
            .field("stderr", &"[REDACTED]")
            .field("truncated", &self.truncated)
            .finish()
    }
}

impl CommandOutput {
    #[must_use]
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

#[async_trait]
pub trait ProcessRunner: Send + Sync {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError>;
}

pub struct StreamCommandSpec {
    program: String,
    args: Vec<String>,
    current_dir: Option<PathBuf>,
    environment: RuntimeEnvironment,
    inherited_environment: Vec<&'static str>,
    stdin: ExecStdinMode,
    tty: ExecTtyMode,
    timeout: Duration,
    buffered_events: usize,
}

impl StreamCommandSpec {
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            environment: RuntimeEnvironment::default(),
            inherited_environment: Vec::new(),
            stdin: ExecStdinMode::Closed,
            tty: ExecTtyMode::Disabled,
            timeout: Duration::from_secs(30),
            buffered_events: 128,
        }
    }

    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn current_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(path.into());
        self
    }

    #[must_use]
    pub fn sensitive_environment(mut self, environment: RuntimeEnvironment) -> Self {
        self.environment = environment;
        self
    }

    #[must_use]
    pub fn inherit_environment(mut self, keys: &'static [&'static str]) -> Self {
        self.inherited_environment = keys.to_vec();
        self
    }

    #[must_use]
    pub const fn stdin(mut self, stdin: ExecStdinMode) -> Self {
        self.stdin = stdin;
        self
    }

    #[must_use]
    pub const fn tty(mut self, tty: ExecTtyMode) -> Self {
        self.tty = tty;
        self
    }

    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub const fn buffered_events(mut self, buffered_events: usize) -> Self {
        self.buffered_events = buffered_events;
        self
    }

    #[must_use]
    pub fn redacted_record(&self) -> RedactedCommandRecord {
        RedactedCommandRecord {
            program: self.program.clone(),
            arguments: self.args.clone(),
            environment_keys: self.environment.keys().map(str::to_owned).collect(),
            environment_count: self.environment.len(),
        }
    }

    pub(crate) fn program(&self) -> &str {
        &self.program
    }

    pub(crate) fn arguments(&self) -> impl Iterator<Item = &str> {
        self.args.iter().map(String::as_str)
    }

    pub(crate) fn current_directory(&self) -> Option<&PathBuf> {
        self.current_dir.as_ref()
    }

    pub(crate) fn environment_values(&self) -> impl Iterator<Item = (&str, &str)> {
        self.environment.values()
    }

    pub(crate) fn inherited_environment(&self) -> &[&str] {
        &self.inherited_environment
    }

    pub(crate) const fn stdin_mode(&self) -> ExecStdinMode {
        self.stdin
    }

    pub(crate) const fn tty_mode(&self) -> ExecTtyMode {
        self.tty
    }

    pub(crate) const fn timeout_value(&self) -> Duration {
        self.timeout
    }

    pub(crate) const fn event_limit(&self) -> usize {
        self.buffered_events
    }
}

impl std::fmt::Debug for StreamCommandSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamCommandSpec")
            .field("program", &self.program)
            .field("arguments", &self.args)
            .field("current_dir", &self.current_dir)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .field("stdin", &self.stdin)
            .field("tty", &self.tty)
            .field("timeout", &self.timeout)
            .field("buffered_events", &self.buffered_events)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputSource {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub enum ProcessStreamEvent {
    Output {
        source: ProcessOutputSource,
        bytes: Vec<u8>,
    },
    Exited {
        code: Option<i32>,
    },
    Failed(MmError),
}

pub struct ProcessStream {
    receiver: tokio::sync::mpsc::Receiver<ProcessStreamEvent>,
    cancellation: OperationCancellation,
    completed: bool,
}

impl ProcessStream {
    #[must_use]
    pub fn new(
        receiver: tokio::sync::mpsc::Receiver<ProcessStreamEvent>,
        cancellation: OperationCancellation,
    ) -> Self {
        Self {
            receiver,
            cancellation,
            completed: false,
        }
    }

    pub async fn recv(&mut self) -> Option<ProcessStreamEvent> {
        let event = self.receiver.recv().await;
        if event.as_ref().is_none_or(|event| {
            matches!(
                event,
                ProcessStreamEvent::Exited { .. } | ProcessStreamEvent::Failed(_)
            )
        }) {
            self.completed = true;
        }
        event
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

impl Drop for ProcessStream {
    fn drop(&mut self) {
        if !self.completed {
            self.cancellation.cancel();
        }
    }
}

#[async_trait]
pub trait StreamingProcessRunner: Send + Sync {
    async fn spawn(
        &self,
        specification: StreamCommandSpec,
        cancellation: OperationCancellation,
    ) -> Result<ProcessStream, MmError>;
}

#[cfg(test)]
mod tests;
