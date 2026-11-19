//! What a route lets through and adds on a cluster target: response
//! headers, an allowlist of requests that answers 404 to every other one,
//! and an explicit priority.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The highest `priority` a route may set.
pub const MAX_PRIORITY: i64 = 1_000_000;

const MAX_RULES: usize = 32;
const MAX_METHODS: usize = 16;
const MAX_PATTERN: usize = 512;
const MAX_HEADERS: usize = 32;
const MAX_HEADER_NAME: usize = 64;
const MAX_HEADER_VALUE: usize = 1024;

/// One kind of request a route lets through: one of `methods` (any when
/// empty) whose path matches `path` (any when absent).
#[derive(
    Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct AllowRule {
    /// HTTP methods in capitals, such as `[GET, HEAD]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    /// A regular expression the request path matches, such as
    /// `^/hooks/[^/]+$`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// What is wrong with a route's headers, allowlist, and priority; empty
/// when nothing is.
#[must_use]
pub fn rule_problems(
    headers: &BTreeMap<String, String>,
    allow: &[AllowRule],
    priority: Option<i64>,
) -> Vec<String> {
    let mut problems = header_problems(headers);
    if allow.len() > MAX_RULES {
        problems.push(format!("allow lists more than {MAX_RULES} rules"));
    }
    for rule in allow {
        problems.extend(allow_problem(rule));
    }
    if let Some(priority) = priority
        && !(1..=MAX_PRIORITY).contains(&priority)
    {
        problems.push(format!(
            "priority {priority} is not between 1 and {MAX_PRIORITY}"
        ));
    }
    problems
}

/// What is wrong with response headers; empty when nothing is.
#[must_use]
pub fn header_problems(headers: &BTreeMap<String, String>) -> Vec<String> {
    let mut problems = Vec::new();
    if headers.len() > MAX_HEADERS {
        problems.push(format!("headers lists more than {MAX_HEADERS} headers"));
    }
    for (name, value) in headers {
        let token = |byte: u8| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_|~".contains(&byte);
        if name.is_empty() || name.len() > MAX_HEADER_NAME || !name.bytes().all(token) {
            problems.push(format!("header '{name}' is not a header name"));
        }
        if value.len() > MAX_HEADER_VALUE || value.chars().any(char::is_control) {
            problems.push(format!(
                "the value of header '{name}' has a control character or is longer than {MAX_HEADER_VALUE} bytes"
            ));
        }
    }
    problems
}

fn allow_problem(rule: &AllowRule) -> Option<String> {
    if rule.methods.is_empty() && rule.path.is_none() {
        return Some("an allow rule names neither methods nor a path".to_owned());
    }
    if rule.methods.len() > MAX_METHODS
        || rule.methods.iter().any(|method| {
            method.is_empty()
                || method.len() > 16
                || !method.bytes().all(|byte| byte.is_ascii_uppercase())
        })
    {
        return Some(format!(
            "allow methods {:?} are not HTTP methods in capitals, such as GET",
            rule.methods
        ));
    }
    let path = rule.path.as_deref()?;
    if path.is_empty()
        || path.len() > MAX_PATTERN
        || path.contains('`')
        || path.chars().any(char::is_control)
    {
        return Some(format!(
            "allow path '{path}' is empty, longer than {MAX_PATTERN} bytes, or has a backtick or control character"
        ));
    }
    regex::Regex::new(path)
        .err()
        .map(|error| format!("allow path '{path}' is not a regular expression: {error}"))
}

/// The requests `allow` lets through, as a Traefik rule.
#[must_use]
pub fn allow_expression(allow: &[AllowRule]) -> String {
    allow
        .iter()
        .map(|rule| {
            let methods = (!rule.methods.is_empty()).then(|| {
                let any = rule
                    .methods
                    .iter()
                    .map(|method| format!("Method(`{method}`)"))
                    .collect::<Vec<_>>()
                    .join(" || ");
                format!("({any})")
            });
            let path = rule
                .path
                .as_ref()
                .map(|path| format!("PathRegexp(`{path}`)"));
            let parts = methods.into_iter().chain(path).collect::<Vec<_>>();
            format!("({})", parts.join(" && "))
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

/// How the plan shows what a route lets through.
#[must_use]
pub fn allow_summary(allow: &[AllowRule]) -> String {
    allow
        .iter()
        .map(|rule| {
            let methods = if rule.methods.is_empty() {
                "any method".to_owned()
            } else {
                rule.methods.join("|")
            };
            match &rule.path {
                Some(path) => format!("{methods} {path}"),
                None => methods,
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
