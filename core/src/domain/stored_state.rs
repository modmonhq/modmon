use std::path::Path;

use crate::domain::error::MmError;

/// Where a stored file lives, which decides what its reader runs to get the
/// release that wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredOn {
    ThisMachine,
    Host,
    /// An mm home, which `mm` keeps on a machine and `mmd` on a host.
    MmHome,
}

impl StoredOn {
    /// What to run to get a release that reads newer state.
    #[must_use]
    pub const fn next_step(self) -> &'static str {
        match self {
            Self::ThisMachine => "update with `mm update`",
            Self::Host => {
                "upgrade this host: run `mm update` on your machine, then `mm target setup plan --upgrade`"
            }
            Self::MmHome => {
                "update with `mm update`, or on a host upgrade its mmd with `mm target setup plan --upgrade`"
            }
        }
    }
}

/// The refusal for state a newer release wrote. `version` names what this
/// release does not know, such as `schema 3`.
#[must_use]
pub fn written_by_newer_mm(path: &Path, version: &str, on: StoredOn) -> MmError {
    MmError::State {
        message: newer_mm_message(path, version, on),
    }
}

#[must_use]
pub fn newer_mm_message(path: &Path, version: &str, on: StoredOn) -> String {
    format!(
        "{} was written by a newer mm ({version}); {}",
        path.display(),
        on.next_step()
    )
}

/// What this release does not know in a JSON parse error: the unknown field
/// or variant a newer release added.
#[must_use]
pub fn unknown_in(error: &serde_json::Error) -> Option<String> {
    let text = error.to_string();
    (text.starts_with("unknown field") || text.starts_with("unknown variant"))
        .then(|| text.split(", expected").next().unwrap_or(&text).to_owned())
}

/// The number under `key` at the top of a JSON document, read before its
/// shape so that a newer release's document is refused for its version.
#[must_use]
pub fn stored_version(json: &[u8], key: &str) -> Option<u32> {
    serde_json::from_slice::<serde_json::Value>(json)
        .ok()?
        .get(key)?
        .as_u64()
        .and_then(|version| u32::try_from(version).ok())
}

/// The refusal for a numbered schema this release does not know. A lower
/// number than `current` is the reader's to migrate before it calls this.
pub fn check_schema(path: &Path, found: u32, current: u32, on: StoredOn) -> Result<(), MmError> {
    if found > current {
        return Err(written_by_newer_mm(path, &format!("schema {found}"), on));
    }
    Ok(())
}

/// The error for a stored `what` in `path` that does not parse. A field or
/// variant this release does not know means a newer release wrote it.
#[must_use]
pub fn unreadable(path: &Path, what: &str, error: &serde_json::Error, on: StoredOn) -> MmError {
    if let Some(unknown) = unknown_in(error) {
        return written_by_newer_mm(path, &format!("{what} with {unknown}"), on);
    }
    MmError::State {
        message: format!("{what} in {} is invalid: {error}", path.display()),
    }
}

/// The keys in `value` that `T` does not read, as paths such as
/// `projects.<id>.added`. A newer release may write them; the reader ignores
/// them and says so.
#[must_use]
pub fn unknown_keys<T: schemars::JsonSchema>(value: &serde_json::Value) -> Vec<String> {
    let schema = schemars::schema_for!(T);
    let root = schema.as_value();
    let mut unknown = Vec::new();
    collect_unknown(root, root, value, "", &mut unknown);
    unknown
}

fn collect_unknown(
    root: &serde_json::Value,
    schema: &serde_json::Value,
    value: &serde_json::Value,
    at: &str,
    unknown: &mut Vec<String>,
) {
    let schema = resolve(root, schema);
    if let Some(choices) = schema
        .get("anyOf")
        .or_else(|| schema.get("oneOf"))
        .and_then(serde_json::Value::as_array)
    {
        let present = choices
            .iter()
            .map(|choice| resolve(root, choice))
            .filter(|choice| choice.get("type").and_then(serde_json::Value::as_str) != Some("null"))
            .collect::<Vec<_>>();
        if let [only] = present.as_slice() {
            collect_unknown(root, only, value, at, unknown);
        }
        return;
    }
    let path = |key: &str| {
        if at.is_empty() {
            key.to_owned()
        } else {
            format!("{at}.{key}")
        }
    };
    match value {
        serde_json::Value::Object(fields) => {
            let values = schema
                .get("additionalProperties")
                .filter(|values| values.is_object());
            match schema
                .get("properties")
                .and_then(serde_json::Value::as_object)
            {
                Some(properties) => {
                    for (key, field) in fields {
                        match properties.get(key) {
                            Some(property) => {
                                collect_unknown(root, property, field, &path(key), unknown);
                            }
                            None if values.is_none() => unknown.push(path(key)),
                            None => {}
                        }
                    }
                }
                None => {
                    if let Some(values) = values {
                        for (key, field) in fields {
                            collect_unknown(root, values, field, &path(key), unknown);
                        }
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            if let Some(item) = schema.get("items") {
                for (index, entry) in items.iter().enumerate() {
                    collect_unknown(root, item, entry, &format!("{at}[{index}]"), unknown);
                }
            }
        }
        _ => {}
    }
}

fn resolve<'a>(
    root: &'a serde_json::Value,
    schema: &'a serde_json::Value,
) -> &'a serde_json::Value {
    schema
        .get("$ref")
        .and_then(serde_json::Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| root.get("$defs")?.get(name))
        .unwrap_or(schema)
}
