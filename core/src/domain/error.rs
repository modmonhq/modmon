use std::path::PathBuf;

use thiserror::Error;

use crate::domain::runtime::{RuntimeCapability, RuntimeKind};
use crate::domain::secret_contract::SecretWorkloadKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationStatus {
    NotStarted,
    Unknown,
    Applied,
}

impl std::fmt::Display for MutationStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted => formatter.write_str("not started"),
            Self::Unknown => formatter.write_str("unknown"),
            Self::Applied => formatter.write_str("applied"),
        }
    }
}

#[derive(Debug, Error)]
pub enum MmError {
    #[error("{issue}")]
    Setup {
        issue: crate::domain::readiness::SetupIssue,
    },
    #[error("no .config/mm.yaml was found from {start} or its parents")]
    ConfigNotFound { start: PathBuf },

    #[error("could not read configuration at {path}: {source}")]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse configuration at {path}: {message}")]
    ConfigParse { path: PathBuf, message: String },

    #[error("no environment was selected; pass --environment or configure default-environment")]
    EnvironmentRequired,

    #[error("environment error: {message}")]
    Environment { message: String },

    #[error("secret provider error: {message}")]
    Secret { message: String },

    #[error(
        "secret '{logical_secret}' is incompatible: backend '{backend}', delivery '{delivery}', runtime {runtime:?}, workload {workload:?}; {supported}"
    )]
    SecretCompatibility {
        logical_secret: String,
        backend: String,
        delivery: String,
        runtime: RuntimeKind,
        workload: SecretWorkloadKind,
        supported: String,
    },

    #[error("target error: {message}")]
    Target { message: String },

    #[error("build error: {message}")]
    Build { message: String },

    #[error("another operation is already in progress for target {target_id}")]
    TargetOperationInProgress { target_id: uuid::Uuid },

    #[error("security policy rejected the operation: {message}")]
    Security { message: String },

    #[error("authentication failed")]
    AuthenticationFailed,

    #[error("configuration validation failed with {errors} error(s)")]
    Validation { errors: usize },

    #[error("project initialization path already exists at {path}")]
    ConfigAlreadyExists { path: PathBuf },

    #[error("I/O error while {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to start {program}: {source}")]
    ProcessSpawn {
        program: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{program} timed out after {seconds} seconds")]
    ProcessTimeout { program: String, seconds: u64 },

    #[error("{program} failed with exit code {code:?}: {stderr}")]
    ProcessFailed {
        program: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("runtime response was invalid: {message}")]
    RuntimeResponse { message: String },

    /// A remote host, or its `mmd`, that mm could not reach or talk to.
    /// `subject` names it with its address, `help` says what to run next.
    /// `problem` starts with mm's own words for what happened; anything
    /// `ssh` or another program printed follows them in parentheses and
    /// nowhere else (`own_words` leaves it out).
    #[error("{subject} {problem}. {help}")]
    Unreachable {
        subject: String,
        problem: String,
        help: String,
    },

    /// Something did not finish, or stopped reporting progress, within its
    /// time. The message names what, where, and for how long, and says
    /// what to run next. Never a state or security error.
    #[error("{message}")]
    Timeout { message: String },

    /// An operation was stopped before it finished: `mm deploy cancel`,
    /// or its client went away. The message says by whom and what is left.
    #[error("{message}")]
    Cancelled { message: String },

    /// Applying a cluster plan stopped at a node; the message says which
    /// nodes changed and which did not.
    #[error("{message}")]
    ApplyStopped { message: String },

    /// A request addressed to one cluster node reached a host that is not it.
    #[error("this request is for node '{requested}', but the host that answered {}. The address recorded for '{requested}' reaches the wrong host: check `host` for each node under the target's nodes:, then run `mm target setup record`", wrong_node_identity(.actual.as_deref()))]
    WrongNode {
        requested: String,
        actual: Option<String>,
    },

    #[error("runtime {runtime:?} does not support {operation} on target {target}")]
    UnsupportedCapability {
        runtime: RuntimeKind,
        target: String,
        operation: String,
        required: Vec<RuntimeCapability>,
        detected_version: Option<String>,
    },

    #[error("state store error: {message}")]
    State { message: String },

    #[error("placement: {message}")]
    Placement { message: String },

    /// A release name or input the plan was given that the commit does not
    /// accept, or a release fact a template reads that the deployment does
    /// not have.
    #[error("{message}")]
    ReleaseInput { message: String },

    /// What a remote host's `mmd` reported, with the code it gave, when the
    /// error has no variant of its own on this side.
    #[error("{message}")]
    Remote { code: &'static str, message: String },

    #[error("terminal error: {message}")]
    Terminal { message: String },

    /// An error in mm's own words: mm refuses the request as asked, and the
    /// message says why and what to do instead. It is made from the
    /// configuration and the request alone, never from what a provider, an
    /// engine, or a host printed, so an agent gets it as it is
    /// ([`MmError::refusal`]). Its code, exit code, and text are those of
    /// the error it wraps.
    #[error(transparent)]
    InOwnWords(Box<MmError>),

    #[error(
        "operation {operation_id} failed (mutation state: {mutation_status}): {primary}; cleanup failures: {cleanup_failures:?}; recovery: {recovery}"
    )]
    OperationFailure {
        operation_id: String,
        mutation_status: MutationStatus,
        cleanup_failures: Vec<String>,
        recovery: String,
        #[source]
        primary: Box<MmError>,
    },
}

/// Code of [`MmError::WrongNode`] in remote error frames.
pub const NODE_MISMATCH_CODE: &str = "MM_NODE_MISMATCH";

/// Code of a host's refusal to act on an environment it never deployed.
pub const NOT_DEPLOYED_CODE: &str = "MM_NOT_DEPLOYED";

fn wrong_node_identity(actual: Option<&str>) -> String {
    actual.map_or_else(
        || "has no node name (/etc/mm/node.env is missing; `mm target setup` writes it)".to_owned(),
        |actual| format!("is node '{actual}'"),
    )
}

impl MmError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Setup { issue } => issue.code(),
            Self::ConfigNotFound { .. } => "MM_CONFIG_NOT_FOUND",
            Self::ConfigRead { .. } => "MM_CONFIG_READ",
            Self::ConfigParse { .. } => "MM_CONFIG_PARSE",
            Self::EnvironmentRequired => "MM_ENVIRONMENT_REQUIRED",
            Self::Environment { .. } => "MM_ENVIRONMENT",
            Self::Secret { .. } => "MM_SECRET",
            Self::SecretCompatibility { .. } => "MM_SECRET_INCOMPATIBLE",
            Self::Target { .. } => "MM_TARGET",
            Self::Build { .. } => "MM_BUILD",
            Self::TargetOperationInProgress { .. } => "MM_TARGET_OPERATION_IN_PROGRESS",
            Self::Security { .. } => "MM_SECURITY",
            Self::AuthenticationFailed => "MM_SECURITY",
            Self::Validation { .. } => "MM_CONFIG_INVALID",
            Self::ConfigAlreadyExists { .. } => "MM_CONFIG_EXISTS",
            Self::Io { .. } => "MM_IO",
            Self::ProcessSpawn { .. } => "MM_PROCESS_SPAWN",
            Self::ProcessTimeout { .. } => "MM_PROCESS_TIMEOUT",
            Self::ProcessFailed { .. } => "MM_PROCESS_FAILED",
            Self::RuntimeResponse { .. } => "MM_RUNTIME_RESPONSE",
            Self::Unreachable { .. } => "MM_UNREACHABLE",
            Self::Timeout { .. } => "MM_TIMEOUT",
            Self::Cancelled { .. } => "MM_CANCELLED",
            Self::WrongNode { .. } => NODE_MISMATCH_CODE,
            Self::ApplyStopped { .. } => "MM_APPLY_STOPPED",
            Self::UnsupportedCapability { .. } => "MM_RUNTIME_UNSUPPORTED",
            Self::State { .. } => "MM_STATE",
            Self::Placement { .. } => "MM_PLACEMENT",
            Self::ReleaseInput { .. } => "MM_RELEASE_INPUT",
            Self::Remote { code, .. } => code,
            Self::Terminal { .. } => "MM_TERMINAL",
            Self::OperationFailure { primary, .. } => primary.code(),
            Self::InOwnWords(refusal) => refusal.code(),
        }
    }

    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Setup { issue } => issue.exit_code(),
            Self::ConfigNotFound { .. }
            | Self::ConfigRead { .. }
            | Self::ConfigParse { .. }
            | Self::EnvironmentRequired
            | Self::Environment { .. }
            | Self::Target { .. }
            | Self::Build { .. }
            | Self::Validation { .. }
            | Self::Placement { .. }
            | Self::ReleaseInput { .. }
            | Self::ConfigAlreadyExists { .. } => 2,
            Self::Secret { .. }
            | Self::SecretCompatibility { .. }
            | Self::TargetOperationInProgress { .. }
            | Self::Security { .. }
            | Self::AuthenticationFailed => 3,
            Self::ProcessSpawn { .. }
            | Self::ProcessTimeout { .. }
            | Self::ProcessFailed { .. }
            | Self::RuntimeResponse { .. }
            | Self::Unreachable { .. }
            | Self::Timeout { .. }
            | Self::Cancelled { .. }
            | Self::WrongNode { .. }
            | Self::ApplyStopped { .. }
            | Self::Remote { .. }
            | Self::UnsupportedCapability { .. } => 3,
            Self::Io { .. } | Self::State { .. } | Self::Terminal { .. } => 1,
            Self::OperationFailure { primary, .. } => primary.exit_code(),
            Self::InOwnWords(refusal) => refusal.exit_code(),
        }
    }

    /// Marks this error as one in mm's own words ([`MmError::InOwnWords`]):
    /// only for a message made from the configuration and the request.
    #[must_use]
    pub fn in_own_words(self) -> Self {
        match self {
            own @ Self::InOwnWords(_) => own,
            other => Self::InOwnWords(Box::new(other)),
        }
    }

    /// The message of an error in mm's own words, without the prefix of
    /// its kind; `None` for every other error.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        let Self::InOwnWords(refusal) = self else {
            return None;
        };
        Some(match refusal.as_ref() {
            Self::Environment { message }
            | Self::Secret { message }
            | Self::Target { message }
            | Self::Build { message }
            | Self::Security { message }
            | Self::State { message }
            | Self::Placement { message }
            | Self::ReleaseInput { message }
            | Self::Remote { message, .. } => message.clone(),
            other => other.to_string(),
        })
    }

    #[must_use]
    pub fn unsupported_capability(
        runtime: RuntimeKind,
        target: Option<&str>,
        operation: impl Into<String>,
        required: Vec<RuntimeCapability>,
        detected_version: Option<String>,
    ) -> Self {
        Self::UnsupportedCapability {
            runtime,
            target: target.unwrap_or("unspecified").to_owned(),
            operation: operation.into(),
            required,
            detected_version,
        }
    }

    /// For a host that could not be reached: who, and what happened, in
    /// mm's own words, and the next step. The detail in parentheses, which
    /// can quote another program, is left out, so this may be shown where
    /// provider output may not: to an agent over MCP. `None` for every
    /// other error.
    #[must_use]
    pub fn own_words(&self) -> Option<(String, String)> {
        let Self::Unreachable {
            subject,
            problem,
            help,
        } = self
        else {
            return None;
        };
        let own = problem
            .split_once(" (")
            .map_or(problem.as_str(), |(own, _)| own);
        Some((format!("{subject} {own}"), help.clone()))
    }

    #[must_use]
    pub fn operation_failure(
        operation_id: impl Into<String>,
        mutation_status: MutationStatus,
        primary: Self,
        cleanup_failures: Vec<String>,
        recovery: impl Into<String>,
    ) -> Self {
        Self::OperationFailure {
            operation_id: operation_id.into(),
            mutation_status,
            cleanup_failures,
            recovery: recovery.into(),
            primary: Box::new(primary),
        }
    }
}

#[cfg(test)]
mod tests;
