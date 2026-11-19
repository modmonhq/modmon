//! Keys a configuration file sets that this release does not read.
//!
//! A release ignores a key it does not know and says so, so a project that
//! uses a key a newer release added still plans with an older `mm` or `mmd`.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The warning about an unknown key, and the error `mm config validate
/// --strict` makes of it.
pub const UNKNOWN_KEY_CODE: &str = "MM_CONFIG_UNKNOWN_KEY";

/// What to do about an unknown key.
pub const UNKNOWN_KEY_HELP: &str = "fix the key if it is misspelled; if a newer mm reads it, run `mm update` on this machine and on the hosts";

/// A key this release ignores, where the user wrote it.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct UnknownConfigKey {
    /// The file that sets the key, relative to the project root.
    pub file: String,
    /// The key's dotted path from the top of the configuration.
    pub key: String,
    /// A key this release knows in the same place, spelled almost the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl UnknownConfigKey {
    /// The warning, naming who ignores the key: `this mm`, or a host's mmd.
    #[must_use]
    pub fn message(&self, reader: &str) -> String {
        let mut message = format!(
            "unknown key `{}` in {}; {reader} ignores it",
            self.key, self.file
        );
        if let Some(suggestion) = &self.suggestion {
            message.push_str(&format!(" (did you mean `{suggestion}`?)"));
        }
        message
    }
}

impl UnknownConfigKey {
    /// The warning `mm config validate` lists, at the file under `root`.
    #[must_use]
    pub fn diagnostic(&self, root: &std::path::Path) -> crate::domain::diagnostic::Diagnostic {
        crate::domain::diagnostic::Diagnostic::warning(UNKNOWN_KEY_CODE, self.to_string())
            .at_path(root.join(&self.file))
            .with_help(UNKNOWN_KEY_HELP)
    }
}

impl std::fmt::Display for UnknownConfigKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message("this mm"))
    }
}

/// One step from the top of a configuration file to a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySegment {
    Key(String),
    Index(usize),
}

/// `path` as the key is written in messages: `modules.web.build.sufix`,
/// `stacks.app.ingress[0].host`, `registries."ghcr.io".user`.
#[must_use]
pub fn dotted(path: &[KeySegment]) -> String {
    let mut text = String::new();
    for segment in path {
        match segment {
            KeySegment::Index(index) => text.push_str(&format!("[{index}]")),
            KeySegment::Key(key) => {
                if !text.is_empty() {
                    text.push('.');
                }
                let plain = !key.is_empty()
                    && key.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '*')
                    });
                if plain {
                    text.push_str(key);
                } else {
                    text.push_str(&format!("{key:?}"));
                }
            }
        }
    }
    text
}

/// The key `schema` allows beside the last key of `path` that is spelled
/// closest to it, when one is close enough to be a misspelling.
#[must_use]
pub fn suggestion(schema: &Value, path: &[KeySegment]) -> Option<String> {
    let (KeySegment::Key(unknown), parent) = path.split_last()? else {
        return None;
    };
    let mut known = BTreeSet::new();
    known_keys(schema, schema, parent, &mut known, 0);
    let limit = (unknown.chars().count() / 3).max(1);
    known
        .into_iter()
        .map(|candidate| (edit_distance(unknown, &candidate), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min()
        .map(|(_, candidate)| candidate)
}

fn known_keys(
    root: &Value,
    schema: &Value,
    path: &[KeySegment],
    known: &mut BTreeSet<String>,
    depth: usize,
) {
    let Some(object) = schema.as_object().filter(|_| depth < 64) else {
        return;
    };
    if let Some(definition) = object
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| root.get("$defs")?.get(name))
    {
        known_keys(root, definition, path, known, depth + 1);
    }
    for combinator in ["anyOf", "oneOf", "allOf"] {
        for branch in object
            .get(combinator)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            known_keys(root, branch, path, known, depth + 1);
        }
    }
    let properties = object.get("properties").and_then(Value::as_object);
    match path.split_first() {
        None => known.extend(properties.into_iter().flatten().map(|(key, _)| key.clone())),
        Some((KeySegment::Key(key), rest)) => {
            if let Some(property) = properties.and_then(|properties| properties.get(key)) {
                known_keys(root, property, rest, known, depth + 1);
            } else if let Some(values) = object
                .get("additionalProperties")
                .filter(|values| values.is_object())
            {
                known_keys(root, values, rest, known, depth + 1);
            }
        }
        Some((KeySegment::Index(_), rest)) => {
            if let Some(items) = object.get("items") {
                known_keys(root, items, rest, known, depth + 1);
            }
        }
    }
}

/// Edits between `left` and `right`, a swap of two neighbours counting as one.
#[must_use]
pub fn edit_distance(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let width = right.len() + 1;
    let mut rows = vec![0; (left.len() + 1) * width];
    for (index, cell) in rows.iter_mut().enumerate().take(width) {
        *cell = index;
    }
    for row in 1..=left.len() {
        rows[row * width] = row;
        for column in 1..=right.len() {
            let cost = usize::from(left[row - 1] != right[column - 1]);
            let mut best = (rows[(row - 1) * width + column] + 1)
                .min(rows[row * width + column - 1] + 1)
                .min(rows[(row - 1) * width + column - 1] + cost);
            if row > 1
                && column > 1
                && left[row - 1] == right[column - 2]
                && left[row - 2] == right[column - 1]
            {
                best = best.min(rows[(row - 2) * width + column - 2] + 1);
            }
            rows[row * width + column] = best;
        }
    }
    rows[left.len() * width + right.len()]
}
