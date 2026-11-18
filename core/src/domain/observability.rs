use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::runtime::{RuntimeKind, ServiceIdentity};
use crate::domain::secret::SecretReference;

/// Recent lines a log request asks for unless told otherwise.
pub const DEFAULT_LOG_TAIL: u32 = 100;
/// Seconds a command may run unless told otherwise.
pub const DEFAULT_EXEC_TIMEOUT_SECONDS: u64 = 30;
pub const DEFAULT_MAX_LOG_LINE_BYTES: usize = 64 * 1024;
pub const DEFAULT_LOG_BUFFER_RECORDS: usize = 128;
pub const MAX_LOG_BUFFER_RECORDS: usize = 1_024;
pub const MAX_LOG_LINE_BYTES: usize = 1024 * 1024;
pub const MAX_LOG_BUFFER_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_EXEC_ARGV: usize = 256;
pub const MAX_EXEC_ARGUMENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogOutputPolicy {
    Both,
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogMode {
    Redacted,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LogLimits {
    pub maximum_line_bytes: usize,
    pub buffered_records: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_bytes_per_second: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_lines_per_second: Option<u32>,
}

impl Default for LogLimits {
    fn default() -> Self {
        Self {
            maximum_line_bytes: DEFAULT_MAX_LOG_LINE_BYTES,
            buffered_records: DEFAULT_LOG_BUFFER_RECORDS,
            maximum_bytes_per_second: Some(1024 * 1024),
            maximum_lines_per_second: Some(10_000),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LogRequest {
    /// The service whose containers are read; `None` reads every service of
    /// the environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<ServiceSelector>,
    /// On a cluster target: only these nodes. Empty reads every node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
    pub tail: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_unix_ms: Option<i64>,
    pub follow: bool,
    pub timestamps: bool,
    pub output: LogOutputPolicy,
    pub mode: LogMode,
    pub limits: LogLimits,
    pub approval: ApprovalContext,
}

impl LogRequest {
    /// The request every interface starts from: the last lines of both
    /// streams of one service, redacted, without following. Callers change
    /// what they offer.
    #[must_use]
    pub fn new(selector: ServiceSelector, approval: ApprovalContext) -> Self {
        Self {
            selector: Some(selector),
            ..Self::every_service(approval)
        }
    }

    /// The same for every service of the environment.
    #[must_use]
    pub fn every_service(approval: ApprovalContext) -> Self {
        Self {
            selector: None,
            nodes: Vec::new(),
            tail: DEFAULT_LOG_TAIL,
            since_unix_ms: None,
            until_unix_ms: None,
            follow: false,
            timestamps: false,
            output: LogOutputPolicy::Both,
            mode: LogMode::Redacted,
            limits: LogLimits::default(),
            approval,
        }
    }

    /// The one service the request names, when it names one.
    #[must_use]
    pub fn service(&self) -> Option<&str> {
        self.selector
            .as_ref()
            .map(|selector| selector.service.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    Stdout,
    Stderr,
    /// Not a line a container wrote: what mm says about the read itself,
    /// such as a node that did not answer.
    Runtime,
}

/// One line of a service's output. A read of several containers returns
/// them as one stream in the order the engines recorded the lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LogRecord {
    pub identity: ServiceIdentity,
    pub sequence: u64,
    pub source: LogSource,
    /// The cluster node the line comes from; absent on a single host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The container that wrote the line, by the engine's name for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// When the engine recorded the line (RFC 3339, the node's clock).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub message: String,
    pub lossy_utf8: bool,
    pub truncated: bool,
    pub reconnect: u32,
}

impl LogRecord {
    /// A line mm adds to a merged read about one node: why it has no lines
    /// from there.
    #[must_use]
    pub fn about_node(identity: ServiceIdentity, node: &str, message: String) -> Self {
        Self {
            identity,
            sequence: 0,
            source: LogSource::Runtime,
            node: Some(node.to_owned()),
            container: None,
            timestamp: None,
            message,
            lossy_utf8: false,
            truncated: false,
            reconnect: 0,
        }
    }

    /// How a reader names the container: `web-1` for the first container of
    /// service `web`, as Compose itself prints it, or the engine's name when
    /// it does not follow that pattern.
    #[must_use]
    pub fn container_label(&self) -> Option<&str> {
        let container = self.container.as_deref()?;
        let service = self.identity.service.as_str();
        if service.is_empty() {
            return Some(container);
        }
        // `<project>-<service>-<number>`, or with underscores in old projects.
        let label = container
            .rmatch_indices(service)
            .map(|(start, _)| &container[start..])
            .find(|label| {
                label[service.len()..]
                    .strip_prefix(['-', '_'])
                    .is_some_and(|number| {
                        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
                    })
            });
        Some(label.unwrap_or(container))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecStdinMode {
    Closed,
    Inherit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecTtyMode {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecEnvironmentReference {
    pub name: String,
    pub reference: SecretReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalContext {
    pub principal: String,
    pub source: String,
    pub approved_high_risk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecRequest {
    pub selector: ServiceSelector,
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workdir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<ExecEnvironmentReference>,
    pub tty: ExecTtyMode,
    pub stdin: ExecStdinMode,
    #[schemars(with = "u64")]
    pub timeout: Duration,
    pub approval: ApprovalContext,
    pub shell_wrapper: bool,
}

impl ExecRequest {
    /// The request every interface starts from: an exact argument vector
    /// with no TTY, no stdin, and no extra environment.
    #[must_use]
    pub fn new(selector: ServiceSelector, argv: Vec<String>, approval: ApprovalContext) -> Self {
        Self {
            selector,
            argv,
            user: None,
            workdir: None,
            environment: Vec::new(),
            tty: ExecTtyMode::Disabled,
            stdin: ExecStdinMode::Closed,
            timeout: Duration::from_secs(DEFAULT_EXEC_TIMEOUT_SECONDS),
            approval,
            shell_wrapper: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecOutputSource {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecEvent {
    Selected {
        runtime: RuntimeKind,
        identity: ServiceIdentity,
        container_id: String,
    },
    Output {
        source: ExecOutputSource,
        value: String,
        lossy_utf8: bool,
    },
    Finished(ExecOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecOutcome {
    pub runtime: RuntimeKind,
    pub identity: ServiceIdentity,
    pub container_id: String,
    pub exit_code: i32,
    pub started_at_unix_ms: i64,
    pub finished_at_unix_ms: i64,
}

/// Restart one service, or one of its containers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestartRequest {
    pub selector: ServiceSelector,
    pub approval: ApprovalContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RestartOutcome {
    pub runtime: RuntimeKind,
    pub identity: ServiceIdentity,
    /// The containers that were restarted.
    pub containers: Vec<String>,
    /// The service was restarted one node at a time, each healthy again
    /// before the next.
    pub rolling: bool,
    /// The nodes of a cluster target the service was restarted on, in
    /// the order they were; empty on one host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperation {
    Status,
    Logs,
    RawLogs,
    Exec,
    Shell,
    Restart,
    JobRead,
    JobRun,
    JobCancel,
    ScheduleManage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub principal: String,
    pub source: String,
    pub operation: RuntimeOperation,
    pub project_id: crate::domain::workspace::ProjectId,
    pub environment: String,
    pub selector: Option<ServiceSelector>,
    pub approved_high_risk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecAuditRecord {
    pub id: uuid::Uuid,
    pub principal: String,
    pub runtime: RuntimeKind,
    pub project_id: crate::domain::workspace::ProjectId,
    pub environment: String,
    pub stack: String,
    pub module: Option<String>,
    pub service: String,
    pub container_id: Option<String>,
    pub executable: String,
    pub argv_hash: String,
    pub started_at_unix_ms: i64,
    pub finished_at_unix_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OperationCancellation(Arc<AtomicBool>);

impl OperationCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub struct ResolvedExecRequest {
    pub request: ExecRequest,
    pub environment: crate::domain::runtime::RuntimeEnvironment,
}

impl std::fmt::Debug for ResolvedExecRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedExecRequest")
            .field("request", &self.request)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

pub fn validate_selector(selector: &ServiceSelector) -> Result<(), String> {
    for (kind, value) in std::iter::once(("service", selector.service.as_str()))
        .chain(selector.stack.as_deref().map(|value| ("stack", value)))
        .chain(selector.module.as_deref().map(|value| ("module", value)))
        .chain(
            selector
                .container
                .as_deref()
                .map(|value| ("container", value)),
        )
    {
        if value.is_empty()
            || value.len() > 255
            || value.chars().any(char::is_control)
            || value.starts_with('-')
        {
            return Err(format!("{kind} selector is invalid"));
        }
    }
    Ok(())
}

/// The text that stands in for a secret value in output.
pub const REDACTED: &str = "[REDACTED]";

/// A value at least this long is replaced wherever it appears.
const REDACT_ANYWHERE_LENGTH: usize = 8;

/// A value shorter than this is never replaced.
const REDACT_MINIMUM_LENGTH: usize = 4;

/// Words that ordinary output is full of. Replacing them would hide nothing
/// and would tell every reader what the secret is.
const ORDINARY_WORDS: [&str; 10] = [
    "null",
    "none",
    "nil",
    "true",
    "false",
    "undefined",
    "yes",
    "no",
    "on",
    "off",
];

/// Replaces secret values in container output.
///
/// A long value is replaced wherever it appears. A short one would also match
/// inside unrelated text (`null` in `/dev/null`), so it is replaced only where
/// it stands alone: not as part of a longer word, path, or name. Values under
/// four characters and ordinary words such as `null` or `true` are left as
/// they are, because output cannot be told apart from them.
#[must_use]
pub fn redact_output<S: AsRef<str>>(message: &str, secrets: &[S]) -> String {
    let mut output = message.to_owned();
    for secret in secrets {
        let secret = secret.as_ref();
        let length = secret.chars().count();
        if length < REDACT_MINIMUM_LENGTH
            || ORDINARY_WORDS
                .iter()
                .any(|word| word.eq_ignore_ascii_case(secret))
        {
            continue;
        }
        output = if length >= REDACT_ANYWHERE_LENGTH {
            output.replace(secret, REDACTED)
        } else {
            redact_standalone(&output, secret)
        };
    }
    output
}

/// Replaces `secret` where the text around it does not continue a word,
/// path, or name.
fn redact_standalone(text: &str, secret: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut copied = 0;
    for (start, _) in text.match_indices(secret) {
        if start < copied {
            continue;
        }
        let end = start + secret.len();
        let mut before = text[..start].chars().rev();
        let mut after = text[end..].chars();
        if joins(before.next(), before.next()) || joins(after.next(), after.next()) {
            continue;
        }
        output.push_str(&text[copied..start]);
        output.push_str(REDACTED);
        copied = end;
    }
    output.push_str(&text[copied..]);
    output
}

/// Whether the character next to a match makes it part of something longer.
/// A dot does only when more of a name follows it, so the end of a sentence
/// still counts as standing alone.
fn joins(adjacent: Option<char>, beyond: Option<char>) -> bool {
    match adjacent {
        Some(character) if character.is_alphanumeric() => true,
        Some('/' | '\\' | '-' | '_') => true,
        Some('.') => beyond.is_some_and(char::is_alphanumeric),
        _ => false,
    }
}

pub fn validate_log_request(request: &LogRequest) -> Result<(), String> {
    if let Some(selector) = &request.selector {
        validate_selector(selector)?;
    }
    if request
        .nodes
        .iter()
        .any(|node| node.is_empty() || node.len() > 255 || node.chars().any(char::is_control))
    {
        return Err("node selector is invalid".to_owned());
    }
    if request.tail > 1_000_000 {
        return Err("log tail exceeds 1000000 records".to_owned());
    }
    if request
        .since_unix_ms
        .zip(request.until_unix_ms)
        .is_some_and(|(since, until)| since > until)
    {
        return Err("log since timestamp is after until timestamp".to_owned());
    }
    if request.limits.maximum_line_bytes == 0
        || request.limits.maximum_line_bytes > MAX_LOG_LINE_BYTES
        || request.limits.buffered_records == 0
        || request.limits.buffered_records > MAX_LOG_BUFFER_RECORDS
        || request
            .limits
            .maximum_line_bytes
            .saturating_mul(request.limits.buffered_records)
            > MAX_LOG_BUFFER_BYTES
        || request.limits.maximum_bytes_per_second == Some(0)
        || request.limits.maximum_lines_per_second == Some(0)
    {
        return Err("log bounds are outside the supported range".to_owned());
    }
    Ok(())
}

pub fn validate_exec_request(request: &ExecRequest) -> Result<(), String> {
    validate_selector(&request.selector)?;
    if request.argv.is_empty() || request.argv.len() > MAX_EXEC_ARGV {
        return Err("exec argv must contain between 1 and 256 values".to_owned());
    }
    if request.argv.iter().any(|value| {
        value.is_empty() || value.as_bytes().contains(&0) || value.len() > MAX_EXEC_ARGUMENT_BYTES
    }) {
        return Err("exec argv contains an empty, NUL, or oversized value".to_owned());
    }
    if request.timeout.is_zero() || request.timeout > Duration::from_secs(24 * 60 * 60) {
        return Err("exec timeout must be between one nanosecond and 24 hours".to_owned());
    }
    if request.environment.iter().any(|value| {
        value.name.is_empty()
            || value.name.len() > 255
            || !value
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || value.name.as_bytes()[0].is_ascii_digit()
    }) {
        return Err("exec environment name is invalid".to_owned());
    }
    let names = request
        .environment
        .iter()
        .map(|value| value.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if names.len() != request.environment.len() {
        return Err("exec environment names must be unique".to_owned());
    }
    Ok(())
}

#[must_use]
pub fn argv_hash(argv: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in argv {
        hasher.update(&(value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

pub fn environment_values(
    values: BTreeMap<String, String>,
) -> crate::domain::runtime::RuntimeEnvironment {
    crate::domain::runtime::RuntimeEnvironment::from_sensitive_values(values)
}

#[path = "observability/merge.rs"]
mod merge;
pub use merge::{LogInstant, LogMerge, log_instant, merged_tail};

#[cfg(test)]
mod tests;
