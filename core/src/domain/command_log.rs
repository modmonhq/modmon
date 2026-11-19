//! The record of one `mm` command run in a project on this machine.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::progress::ProgressEvent;

/// The most bytes of output one record keeps; the rest is cut.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
/// The most progress events one record keeps.
pub const MAX_EVENTS: usize = 2_000;
/// What stands in for a value that is never recorded.
pub const REDACTED: &str = "[REDACTED]";

/// How a command ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CommandOutcome {
    Succeeded,
    Failed,
    Interrupted,
}

/// Who ran a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum CommandPrincipal {
    /// A person at this machine's terminal, by login name.
    User { name: String },
    /// An agent working through `mm mcp`.
    Agent { name: String },
}

/// One progress event and when it came, in milliseconds from the start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct RecordedEvent {
    pub at_ms: u64,
    pub event: ProgressEvent,
}

/// A command as the log keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct CommandRecord {
    /// Sorts by start time: `20261115T201530123Z-4f2a`.
    pub id: String,
    /// The arguments after `mm`, with secret-bearing ones redacted.
    pub arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    pub principal: CommandPrincipal,
    pub started_at_unix_ms: i64,
    pub finished_at_unix_ms: i64,
    pub outcome: CommandOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// What the command printed, as plain text without escape codes.
    pub output: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub output_truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<RecordedEvent>,
    /// The command's JSON result, with `--output json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

/// A record without its output, events and result, for lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct CommandSummary {
    pub id: String,
    pub arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub principal: CommandPrincipal,
    pub started_at_unix_ms: i64,
    pub duration_ms: u64,
    pub outcome: CommandOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Which records a list returns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CommandFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<CommandOutcome>,
    /// Only commands that started at or after this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_unix_ms: Option<i64>,
}

impl CommandRecord {
    #[must_use]
    pub fn summary(&self) -> CommandSummary {
        CommandSummary {
            id: self.id.clone(),
            arguments: self.arguments.clone(),
            environment: self.environment.clone(),
            principal: self.principal.clone(),
            started_at_unix_ms: self.started_at_unix_ms,
            duration_ms: u64::try_from(self.finished_at_unix_ms - self.started_at_unix_ms)
                .unwrap_or(0),
            outcome: self.outcome,
            exit_code: self.exit_code,
            error_code: self.error_code.clone(),
        }
    }

    /// The record as it may be kept: secrets out of its output and result,
    /// and its output and events within their bounds.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.output = redact_text(&self.output);
        if self.output.len() > MAX_OUTPUT_BYTES {
            let mut cut = MAX_OUTPUT_BYTES;
            while !self.output.is_char_boundary(cut) {
                cut -= 1;
            }
            self.output.truncate(cut);
            self.output_truncated = true;
        }
        self.events.truncate(MAX_EVENTS);
        if let Some(result) = self.result.as_mut() {
            scrub_json(result, false);
        }
        self
    }
}

impl CommandFilter {
    #[must_use]
    pub fn admits(&self, summary: &CommandSummary) -> bool {
        self.environment
            .as_ref()
            .is_none_or(|environment| summary.environment.as_ref() == Some(environment))
            && self
                .outcome
                .is_none_or(|outcome| summary.outcome == outcome)
            && self
                .since_unix_ms
                .is_none_or(|since| summary.started_at_unix_ms >= since)
    }
}

fn sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace('-', "_");
    [
        "password",
        "secret",
        "token",
        "private_key",
        "cookie",
        "passphrase",
    ]
    .iter()
    .any(|word| name.contains(word))
        && name != "approval_token"
}

/// The arguments of a command as a record keeps them: a value given to a
/// secret-bearing flag, or as `NAME=value` with such a name, is redacted.
#[must_use]
pub fn redact_arguments(arguments: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(arguments.len());
    let mut hide_next = false;
    for argument in arguments {
        if hide_next {
            redacted.push(REDACTED.to_owned());
            hide_next = false;
            continue;
        }
        if let Some(flag) = argument.strip_prefix("--") {
            match flag.split_once('=') {
                Some((name, _)) if sensitive_name(name) => {
                    redacted.push(format!("--{name}={REDACTED}"));
                }
                None if sensitive_name(flag) && !flag.ends_with("stdin") => {
                    redacted.push(argument.clone());
                    hide_next = true;
                }
                _ => redacted.push(argument.clone()),
            }
            continue;
        }
        match argument.split_once('=') {
            Some((name, _)) if sensitive_name(name) => {
                redacted.push(format!("{name}={REDACTED}"));
            }
            _ => redacted.push(argument.clone()),
        }
    }
    redacted
}

/// `text` with private key blocks, `name=value` pairs of secret-bearing
/// names, the user and password of URLs, `Bearer` and `Basic` credentials,
/// and access tokens of known forms redacted.
#[must_use]
pub fn redact_text(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut in_key = false;
    for line in text.split_inclusive('\n') {
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            in_key = true;
            redacted.push_str(REDACTED);
            redacted.push('\n');
            continue;
        }
        if in_key {
            in_key = !(line.contains("-----END") && line.contains("PRIVATE KEY"));
            continue;
        }
        redacted.push_str(&redact_pairs(line));
    }
    redacted
}

fn redact_pairs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut credential_follows = false;
    for (index, word) in line.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let (body, newline) = word
            .strip_suffix('\n')
            .map_or((word, ""), |body| (body, "\n"));
        match body.split_once('=') {
            _ if credential_follows && !body.is_empty() => out.push_str(REDACTED),
            Some((name, value)) if sensitive_name(name) && !value.trim().is_empty() => {
                out.push_str(name);
                out.push('=');
                out.push_str(REDACTED);
            }
            _ => out.push_str(&redact_token(&redact_userinfo(body))),
        }
        out.push_str(newline);
        if !body.is_empty() {
            credential_follows = matches!(body.to_ascii_lowercase().as_str(), "bearer" | "basic");
        }
    }
    out
}

/// The prefixes of access tokens that services hand out.
const TOKEN_PREFIXES: [&str; 16] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "hvs.",
    "hvb.",
    "hvr.",
    "eyJ",
    "AKIA",
];

/// `word` without the user and password of a URL in it.
fn redact_userinfo(word: &str) -> String {
    let Some(scheme) = word.find("://") else {
        return word.to_owned();
    };
    let authority = scheme + 3;
    let end = word[authority..]
        .find(['/', '?', '#'])
        .map_or(word.len(), |end| authority + end);
    match word[authority..end].rfind('@') {
        Some(at) => format!(
            "{}{REDACTED}{}",
            &word[..authority],
            &word[authority + at..]
        ),
        None => word.to_owned(),
    }
}

/// `word` without an access token in it.
fn redact_token(word: &str) -> String {
    let start = word
        .find(|character: char| character.is_ascii_alphanumeric())
        .unwrap_or(word.len());
    let candidate = &word[start..];
    let end = candidate
        .find(|character: char| {
            !(character.is_ascii_alphanumeric()
                || matches!(character, '_' | '-' | '.' | '/' | '+' | '='))
        })
        .unwrap_or(candidate.len());
    let token = &candidate[..end];
    if TOKEN_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + 8)
    {
        format!("{}{REDACTED}{}", &word[..start], &candidate[end..])
    } else {
        word.to_owned()
    }
}

/// Replaces every string under a secret-bearing key with [`REDACTED`].
pub fn scrub_json(value: &mut serde_json::Value, inherited: bool) {
    match value {
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                scrub_json(value, inherited || sensitive_name(key));
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                scrub_json(value, inherited);
            }
        }
        serde_json::Value::String(value) if inherited => REDACTED.clone_into(value),
        _ => {}
    }
}
