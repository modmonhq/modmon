use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

const EXTENDS_DEPTH: usize = 16;

/// A service whose exec-form (`CMD`) health check has an argument that
/// Podman before 5.8 splits at whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HealthcheckArgumentNote {
    pub stack: String,
    pub service: String,
    /// The Compose file that sets the check's `test`.
    pub file: String,
    /// The interpolation the argument comes from, when only its value could
    /// contain whitespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpolated: Option<String>,
    /// The same check as one shell line, for `CMD-SHELL`.
    pub shell: String,
}

impl HealthcheckArgumentNote {
    #[must_use]
    pub fn line(&self) -> String {
        let Self {
            stack,
            service,
            file,
            ..
        } = self;
        let check = format!(
            "services.{service}.healthcheck.test in {file} (stack '{stack}') is a `CMD` check"
        );
        match &self.interpolated {
            None => format!(
                "{check} with an argument that contains whitespace or is empty; Podman before 5.8 splits such an argument at whitespace, so the check never passes"
            ),
            Some(token) => format!(
                "{check} with an argument from `{token}`, whose value mm does not know; if it contains whitespace or is empty, Podman before 5.8 splits it and the check never passes"
            ),
        }
    }

    #[must_use]
    pub fn help(&self) -> String {
        let shell = serde_json::Value::String(self.shell.clone());
        format!(
            "write `test: [\"CMD-SHELL\", {shell}]` (it runs with /bin/sh in the container), or run Podman 5.8 or later"
        )
    }
}

/// Whether the Podman of `version` ("5.8.0" or "podman version 5.8.0")
/// keeps exec-form health check arguments whole.
#[must_use]
pub fn podman_keeps_arguments(version: &str) -> bool {
    let Some(number) = version
        .split_whitespace()
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))
    else {
        return false;
    };
    let mut parts = number.split(['.', '-', '+']).map(str::parse::<u64>);
    matches!(
        (parts.next(), parts.next()),
        (Some(Ok(major)), Some(Ok(minor))) if (major, minor) >= (5, 8)
    )
}

/// The notes for the services of one stack, from its Compose files in the
/// order Compose reads them, each as `(file name, parsed file)`. Services
/// for which `skip` is true are left out.
#[must_use]
pub fn healthcheck_argument_notes(
    stack: &str,
    files: &[(String, Value)],
    skip: impl Fn(&str) -> bool,
) -> Vec<HealthcheckArgumentNote> {
    let mut checks = Vec::<(String, Mapping, String)>::new();
    for (file, model) in files {
        let Some(services) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for name in services.keys().filter_map(Value::as_str) {
            let Some(own) = file_healthcheck(services, name, 0) else {
                continue;
            };
            match checks.iter_mut().find(|(service, _, _)| service == name) {
                Some((_, check, source)) => {
                    if own.contains_key("test") {
                        source.clone_from(file);
                    }
                    overlay(check, &own);
                }
                None => checks.push((name.to_owned(), own, file.clone())),
            }
        }
    }
    checks
        .into_iter()
        .filter(|(service, _, _)| !skip(service))
        .filter_map(|(service, check, file)| {
            let arguments = exec_arguments(&check)?;
            let interpolated = match arguments.iter().map(|argument| risk(argument)).max()? {
                Risk::None => return None,
                Risk::Literal => None,
                Risk::Interpolated(token) => Some(token),
            };
            Some(HealthcheckArgumentNote {
                stack: stack.to_owned(),
                service,
                file,
                interpolated,
                shell: shell_line(&arguments),
            })
        })
        .collect()
}

/// The `healthcheck` service `name` has in one file, with what it inherits
/// through `extends` from a service of the same file.
fn file_healthcheck(services: &Mapping, name: &str, depth: usize) -> Option<Mapping> {
    let service = services.get(name)?;
    let base = match service.get("extends") {
        Some(Value::String(base)) => Some(base.as_str()),
        Some(Value::Mapping(extends)) if extends.get("file").is_none() => {
            extends.get("service").and_then(Value::as_str)
        }
        _ => None,
    }
    .filter(|base| *base != name && depth < EXTENDS_DEPTH)
    .and_then(|base| file_healthcheck(services, base, depth + 1));
    let own = service.get("healthcheck").and_then(Value::as_mapping);
    match (base, own) {
        (Some(mut base), Some(own)) => {
            overlay(&mut base, own);
            Some(base)
        }
        (base, own) => base.or_else(|| own.cloned()),
    }
}

fn overlay(check: &mut Mapping, own: &Mapping) {
    for (key, value) in own {
        check.insert(key.clone(), value.clone());
    }
}

fn exec_arguments(check: &Mapping) -> Option<Vec<String>> {
    if check.get("disable").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let (kind, arguments) = check.get("test")?.as_sequence()?.split_first()?;
    if kind.as_str() != Some("CMD") {
        return None;
    }
    arguments
        .iter()
        .map(|argument| match argument {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Risk {
    None,
    Interpolated(String),
    Literal,
}

fn risk(argument: &str) -> Risk {
    if argument.is_empty() {
        return Risk::Literal;
    }
    let parts = interpolation_parts(argument);
    let literal_space = parts.iter().any(|part| match part {
        Part::Literal(text) => text.contains(char::is_whitespace),
        Part::Variable { .. } => false,
    });
    if literal_space {
        return Risk::Literal;
    }
    let variables = parts.iter().filter_map(|part| match part {
        Part::Variable { token, fallback } => Some((token, fallback)),
        Part::Literal(_) => None,
    });
    let whole = parts.len() == 1;
    for (token, fallback) in variables {
        if whole || fallback.contains(char::is_whitespace) {
            return Risk::Interpolated(token.clone());
        }
    }
    Risk::None
}

enum Part {
    Literal(String),
    Variable { token: String, fallback: String },
}

/// `argument` cut into the literal text and the `$NAME` and `${...}`
/// interpolations Compose replaces; `$$` is a literal `$`.
fn interpolation_parts(argument: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = argument;
    while let Some(index) = rest.find('$') {
        literal.push_str(&rest[..index]);
        rest = &rest[index..];
        let variable = if let Some(after) = rest.strip_prefix("$$") {
            literal.push('$');
            rest = after;
            continue;
        } else if rest.starts_with("${") {
            braced_length(rest).map(|length| {
                let token = &rest[..length];
                let body = &token[2..length - 1];
                let fallback = ["-", "+"]
                    .iter()
                    .filter_map(|separator| body.split_once(separator))
                    .min_by_key(|(name, _)| name.len())
                    .map_or("", |(_, fallback)| fallback);
                (length, fallback.to_owned())
            })
        } else {
            let name = rest[1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            (name > 0 && !rest[1..].starts_with(|c: char| c.is_ascii_digit()))
                .then(|| (name + 1, String::new()))
        };
        let Some((length, fallback)) = variable else {
            literal.push('$');
            rest = &rest[1..];
            continue;
        };
        if !literal.is_empty() {
            parts.push(Part::Literal(std::mem::take(&mut literal)));
        }
        parts.push(Part::Variable {
            token: rest[..length].to_owned(),
            fallback,
        });
        rest = &rest[length..];
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(Part::Literal(literal));
    }
    parts
}

fn braced_length(text: &str) -> Option<usize> {
    let mut depth = 0_usize;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"${") {
            depth += 1;
            index += 2;
            continue;
        }
        if bytes[index] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(index + 1);
            }
        }
        index += 1;
    }
    None
}

fn shell_line(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| {
            if !argument.is_empty()
                && argument
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
            {
                argument.clone()
            } else {
                format!("'{}'", argument.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
