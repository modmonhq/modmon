use super::*;

pub(super) fn request_hash(value: &impl serde::Serialize) -> String {
    serde_json::to_vec(value).map_or_else(
        |_| blake3::hash(b"invalid-mcp-request").to_hex().to_string(),
        |bytes| blake3::hash(&bytes).to_hex().to_string(),
    )
}

pub(super) fn deployment_hash(value: &serde_json::Value) -> Result<String, MmError> {
    value
        .pointer("/binding/plan_hash")
        .or_else(|| value.get("plan_hash"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| MmError::Security {
            message: "deployment use case returned no canonical plan hash".to_owned(),
        })
}

pub(super) fn direct_plan_hash(value: &serde_json::Value) -> Result<String, MmError> {
    value
        .get("plan_hash")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| MmError::Security {
            message: "application use case returned no canonical plan hash".to_owned(),
        })
}

pub(super) fn to_json(value: impl serde::Serialize) -> Result<serde_json::Value, MmError> {
    serde_json::to_value(value).map_err(|error| MmError::State {
        message: format!("serializing MCP result: {error}"),
    })
}

pub(super) fn validate_arguments(value: &impl serde::Serialize) -> Result<(), MmError> {
    let value = serde_json::to_value(value).map_err(|error| MmError::Environment {
        message: format!("invalid MCP arguments: {error}"),
    })?;
    reject_model_visible_sensitive_fields(&value).map_err(|message| MmError::Security { message })
}

/// An MCP call that ran out of its time: a timeout, never a state error.
/// The configured limit is `request_timeout_ms`.
pub(super) fn timeout_error() -> MmError {
    MmError::Timeout {
        message: "the MCP call did not finish within its time limit".to_owned(),
    }
}

pub(super) fn structured_error(
    code: &str,
    message: &str,
    next_action: &str,
    mutation_state: &str,
    request_id: uuid::Uuid,
) -> CallToolResult {
    CallToolResult::structured_error(serde_json::json!({
        "schema_version": MCP_TOOL_SCHEMA_VERSION,
        "error": {
            "code": code,
            "message": message,
            "retryable": matches!(
                code,
                "MM_STATE" | "MM_IO" | "MM_MCP_RESULT_TOO_LARGE" | "MM_TIMEOUT" | "MM_HOST_BUSY"
            ),
            "mutation_state": mutation_state,
            "next_action": next_action,
            "request_id": request_id,
        }
    }))
}

pub(super) fn audit_unavailable(mutation: bool, request_id: uuid::Uuid) -> CallToolResult {
    structured_error(
        "MM_MCP_AUDIT",
        "The operation result could not be attributed in the MCP audit journal.",
        "Resolve local state permissions before retrying.",
        if mutation { "unknown" } else { "not_started" },
        request_id,
    )
}

/// What an agent is told about a failed call, and what to do next. The
/// text is fixed for each kind of error, so nothing a provider printed
/// reaches the model. Two exceptions are mm's own words: a refusal mm
/// makes from the configuration and the request (`MmError::refusal`),
/// passed as it is, and a host that could not be reached
/// (`MmError::own_words`), the same words the command line prints,
/// without the detail that can quote `ssh`.
pub(super) fn safe_error_message(
    error: &MmError,
) -> (
    std::borrow::Cow<'static, str>,
    std::borrow::Cow<'static, str>,
) {
    if let Some(refusal) = error.refusal() {
        let refusal = if refusal.ends_with('.') {
            refusal
        } else {
            format!("{refusal}.")
        };
        return (
            std::borrow::Cow::Owned(refusal),
            std::borrow::Cow::Borrowed(
                "The message names what to do instead. Do it with these tools where one covers it; an agent cannot run `mm` commands, so tell the user any command it names.",
            ),
        );
    }
    if let Some((problem, help)) = error.own_words() {
        return (
            std::borrow::Cow::Owned(format!("{problem}.")),
            std::borrow::Cow::Owned(format!(
                "{help}. An agent cannot run `mm` commands: tell the user what failed and this next step."
            )),
        );
    }
    let (message, next) = fixed_error_message(error);
    (
        std::borrow::Cow::Borrowed(message),
        std::borrow::Cow::Borrowed(next),
    )
}

fn fixed_error_message(error: &MmError) -> (&'static str, &'static str) {
    match error {
        MmError::Security { .. } | MmError::AuthenticationFailed => (
            "The operation was rejected by authorization or review policy. A remote host applies the agents: block and users file of the commit it deployed, which can be narrower than this checkout.",
            "Call mm_access to see what agents may do. If the action is outside the grant, tell the user what you wanted to run and let them run it.",
        ),
        MmError::State { .. } => (
            "Application state changed or the operation could not be completed safely.",
            "Request a fresh bounded read or plan before retrying.",
        ),
        MmError::Timeout { .. } => (
            "The call did not finish in time: the engine or a host is busy, stuck, or not answering, or an operation passed its deadline.",
            "Call mm_read with kind 'operations' to see what runs and since when. Tell the user what you found; for a remote environment, `mm deploy status` shows it and `mm deploy cancel OPERATION` stops a stuck operation.",
        ),
        MmError::Cancelled { .. } => (
            "The operation was stopped before it finished: someone cancelled it, or its client went away.",
            "Call mm_read with kind 'operations' to see what runs now before trying again.",
        ),
        MmError::Remote {
            code: "MM_HOST_BUSY" | "MM_DEPLOY_RUNNING",
            ..
        } => (
            "Another operation is changing this environment's hosts, so this one did not start and changed nothing.",
            "Call mm_read with kind 'operations' to see who runs what and since when. Try again when it is done; if it is stuck, tell the user that `mm deploy cancel OPERATION` stops it.",
        ),
        MmError::ConfigNotFound { .. }
        | MmError::ConfigRead { .. }
        | MmError::ConfigParse { .. }
        | MmError::Validation { .. }
        | MmError::EnvironmentRequired
        | MmError::Environment { .. } => (
            "The selected project configuration or scope is invalid.",
            "Call mm_access for the configured environments, and mm_read with kinds 'project' and 'health' for diagnostics. A project without configuration needs the user to run `mm init`.",
        ),
        _ => (
            "The typed application use case failed without exposing provider output.",
            "Use bounded health and operation reads; provider details remain redacted.",
        ),
    }
}

pub(super) fn scrub_sensitive(value: &mut serde_json::Value, inherited: bool) {
    match value {
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                let normalized = key.to_ascii_lowercase().replace('-', "_");
                let sensitive = inherited
                    || normalized.contains("password")
                    || normalized.contains("secret")
                    || (normalized.contains("token") && normalized != "approval_token")
                    || normalized.contains("private_key")
                    || normalized.contains("cookie");
                scrub_sensitive(value, sensitive);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                scrub_sensitive(value, inherited);
            }
        }
        serde_json::Value::String(value) if inherited => *value = "[REDACTED]".to_owned(),
        _ => {}
    }
}
