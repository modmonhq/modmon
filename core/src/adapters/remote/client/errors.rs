//! What a host's `mmd` reported, as the error it was on the host.
//!
//! The daemon sends its error's code and message. The client gives them
//! back as they are, so a refusal reads the same over SSH as it would on
//! the host, and only a refusal the host made as security policy says so.

use crate::domain::error::MmError;
use crate::domain::remote::RemoteError;

/// Every code a daemon error can carry that has no variant of its own
/// here; `MmError::code` hands out `&'static str`.
const CODES: [&str; 40] = [
    "MM_APPLY_STOPPED",
    "MM_BUILD_REQUIRED",
    "MM_CONFIG_EXISTS",
    "MM_CONFIG_INVALID",
    "MM_CONFIG_NOT_FOUND",
    "MM_CONFIG_PARSE",
    "MM_CONFIG_READ",
    "MM_CONVERGENCE_TIMEOUT",
    "MM_DEPLOY_RUNNING",
    "MM_ENVIRONMENT_REQUIRED",
    "MM_HOST_BUSY",
    "MM_IO",
    "MM_NODE_MISMATCH",
    "MM_NOT_DEPLOYED",
    "MM_PROCESS_FAILED",
    "MM_PROCESS_SPAWN",
    "MM_PROCESS_TIMEOUT",
    "MM_REMOTE_DEADLINE",
    "MM_REMOTE_IDENTITY",
    "MM_REMOTE_UPGRADE_REQUIRED",
    "MM_ROLLED_BACK",
    "MM_RUNTIME_CONTAINER_FAILED",
    "MM_RUNTIME_EXITED",
    "MM_RUNTIME_PAUSED",
    "MM_RUNTIME_REQUIRED",
    "MM_RUNTIME_RESTARTING",
    "MM_RUNTIME_UNHEALTHY",
    "MM_RUNTIME_UNSUPPORTED",
    "MM_SECRET_INCOMPATIBLE",
    "MM_SECRET_STORE_UNAVAILABLE",
    "MM_SERVICES_REQUIRED",
    "MM_STATE_LEGACY",
    "MM_STORAGE_IN_USE",
    "MM_STORAGE_NOT_SERVED",
    "MM_STORAGE_UNREACHABLE",
    "MM_TARGET_MISSING",
    "MM_TARGET_OPERATION_IN_PROGRESS",
    "MM_TLS_SETUP_REQUIRED",
    "MM_TUNNEL_NOT_CONNECTED",
    "MM_TUNNEL_TOKEN_MISSING",
];

/// The code for an error whose own code this client does not know.
const UNKNOWN_CODE: &str = "MM_REMOTE";

pub(super) fn server_error(error: RemoteError) -> MmError {
    let RemoteError { code, message, .. } = error;
    // The daemon rendered its error with the variant's own prefix; taking
    // it off keeps the rebuilt error from saying it twice.
    let body = |prefix: &str| message.strip_prefix(prefix).unwrap_or(&message).to_owned();
    match code.as_str() {
        "MM_SECURITY" => MmError::Security {
            message: body("security policy rejected the operation: "),
        },
        "MM_TARGET" => MmError::Target {
            message: body("target error: "),
        },
        "MM_ENVIRONMENT" => MmError::Environment {
            message: body("environment error: "),
        },
        "MM_STATE" => MmError::State {
            message: body("state store error: "),
        },
        "MM_PLACEMENT" => MmError::Placement {
            message: body("placement: "),
        },
        "MM_BUILD" => MmError::Build {
            message: body("build error: "),
        },
        "MM_SECRET" => MmError::Secret {
            message: body("secret provider error: "),
        },
        "MM_RUNTIME_RESPONSE" => MmError::RuntimeResponse {
            message: body("runtime response was invalid: "),
        },
        "MM_TERMINAL" => MmError::Terminal {
            message: body("terminal error: "),
        },
        "MM_TIMEOUT" => MmError::Timeout { message: body("") },
        "MM_CANCELLED" => MmError::Cancelled { message: body("") },
        "MM_RELEASE_INPUT" => MmError::ReleaseInput { message: body("") },
        crate::domain::error::NOT_DEPLOYED_CODE => MmError::Remote {
            code: crate::domain::error::NOT_DEPLOYED_CODE,
            message: body(""),
        }
        .in_own_words(),
        other => match CODES.iter().find(|known| **known == other) {
            Some(known) => MmError::Remote {
                code: known,
                message: body(""),
            },
            None => MmError::Remote {
                code: UNKNOWN_CODE,
                message: format!("{other}: {}", body("")),
            },
        },
    }
}

#[cfg(test)]
#[path = "tests/errors.rs"]
mod tests;
