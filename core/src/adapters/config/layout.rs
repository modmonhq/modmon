//! `mm config split|join`: move one top-level key between `.config/mm.yaml`
//! and `.config/mm/<key>.yaml` without changing what the configuration means.

use std::fs;
use std::path::{Path, PathBuf};

use fs4::FileExt;
use serde_yaml_ng::Value;

use super::ConfigLoader;
use super::guided::{direct_entry, root_mapping};
use super::loader::{
    SPLIT_CONFIG_RELATIVE_DIRECTORY, create_project_directory, lock_configuration,
    parse_configuration_with, publish_new_project_file, split_config_path, write_config_atomically,
};
use crate::adapters::private_fs::ensure_user_controlled_directory;
use crate::domain::config::{ConfigDocument, SPLITTABLE_CONFIG_KEYS};
use crate::domain::error::MmError;
use crate::domain::onboarding::{
    ConfigFileChange, ConfigLayoutAction, ConfigLayoutPreview, ConfigLayoutRequest,
};

pub(super) fn preview_layout(
    loader: &ConfigLoader,
    start: &Path,
    request: &ConfigLayoutRequest,
) -> Result<ConfigLayoutPreview, MmError> {
    let document = loader.load_from(start)?;
    let key = splittable_key(&request.key)?;
    let mut notes = Vec::new();
    let changes = match request.action {
        ConfigLayoutAction::Split => plan_split(&document, key, &mut notes)?,
        ConfigLayoutAction::Join => plan_join(&document, key, &mut notes)?,
    };
    if !preserves_meaning(&document, &changes)? {
        return Err(MmError::State {
            message: format!(
                "moving '{key}' would change the configuration's meaning; no plan was made"
            ),
        });
    }
    Ok(ConfigLayoutPreview {
        anchor_path: document.path,
        source_hash: document.source_hash,
        changes,
        notes,
    })
}

fn plan_split(
    document: &ConfigDocument,
    key: &str,
    notes: &mut Vec<String>,
) -> Result<Vec<ConfigFileChange>, MmError> {
    let anchor = &document.path;
    let split_path = split_config_path(anchor, key);
    if document.source(key) == split_path {
        return Err(MmError::Environment {
            message: format!(
                "'{key}' is already split into {SPLIT_CONFIG_RELATIVE_DIRECTORY}/{key}.yaml"
            ),
        });
    }
    if !document.sources.contains_key(key) {
        return Err(MmError::Environment {
            message: format!("'{key}' is not set in .config/mm.yaml, so there is nothing to split"),
        });
    }
    let anchor_before = read_text(anchor)?;
    let (anchor_after, value) =
        extract_top_level(&anchor_before, key).ok_or_else(|| MmError::ConfigParse {
            path: anchor.clone(),
            message: format!("'{key}' must be a top-level block entry to split it losslessly"),
        })?;
    let changes = layout_changes(
        anchor,
        &anchor_before,
        &anchor_after,
        &split_path,
        None,
        Some(value),
    );
    if preserves_meaning(document, &changes)? {
        return Ok(changes);
    }
    notes.push(format!(
        "comments and formatting inside '{key}' could not be moved as written; the new file is re-rendered"
    ));
    let value = rendered_value(anchor, &anchor_before, key)?;
    Ok(layout_changes(
        anchor,
        &anchor_before,
        &anchor_after,
        &split_path,
        None,
        Some(value),
    ))
}

fn plan_join(
    document: &ConfigDocument,
    key: &str,
    notes: &mut Vec<String>,
) -> Result<Vec<ConfigFileChange>, MmError> {
    let anchor = &document.path;
    let split_path = split_config_path(anchor, key);
    if document.source(key) != split_path {
        return Err(MmError::Environment {
            message: format!(
                "'{key}' is not split out; {SPLIT_CONFIG_RELATIVE_DIRECTORY}/{key}.yaml does not exist"
            ),
        });
    }
    let anchor_before = read_text(anchor)?;
    let split_before = read_text(&split_path)?;
    let anchor_after = append_top_level(&anchor_before, key, &split_before);
    let changes = layout_changes(
        anchor,
        &anchor_before,
        &anchor_after,
        &split_path,
        Some(split_before.clone()),
        None,
    );
    if preserves_meaning(document, &changes)? {
        return Ok(changes);
    }
    notes.push(format!(
        "comments and formatting in {SPLIT_CONFIG_RELATIVE_DIRECTORY}/{key}.yaml could not be moved as written; '{key}' is re-rendered"
    ));
    let value: Value =
        serde_yaml_ng::from_str(&split_before).map_err(|error| MmError::ConfigParse {
            path: split_path.clone(),
            message: error.to_string(),
        })?;
    let anchor_after = append_rendered(&anchor_before, key, &value)?;
    Ok(layout_changes(
        anchor,
        &anchor_before,
        &anchor_after,
        &split_path,
        Some(split_before),
        None,
    ))
}

pub(super) fn commit_layout(
    loader: &ConfigLoader,
    preview: &ConfigLayoutPreview,
    expected_source_hash: &str,
    expected_result_hash: &str,
) -> Result<ConfigDocument, MmError> {
    let anchor = &preview.anchor_path;
    let lock = lock_configuration(anchor)?;
    for change in &preview.changes {
        if read_optional_text(&change.path)? != change.before {
            return Err(stale());
        }
    }
    if loader.load(anchor)?.source_hash != expected_source_hash {
        return Err(stale());
    }
    if preview.result_hash() != expected_result_hash {
        return Err(MmError::State {
            message: "reviewed configuration result does not match its plan".to_owned(),
        });
    }
    let (anchor_change, split_change) = match preview.changes.as_slice() {
        [first, second] if first.path == *anchor => (first, second),
        _ => {
            return Err(MmError::State {
                message: "configuration layout change must touch the anchor and one split file"
                    .to_owned(),
            });
        }
    };
    let anchor_after = anchor_change
        .after
        .as_deref()
        .ok_or_else(|| MmError::State {
            message: ".config/mm.yaml cannot be removed".to_owned(),
        })?;
    let anchor_before = anchor_change.before.as_deref().unwrap_or_default();
    match (&split_change.before, &split_change.after) {
        (None, Some(text)) => commit_split(anchor, anchor_after, &split_change.path, text)?,
        (Some(_), None) => commit_join(anchor, anchor_before, anchor_after, &split_change.path)?,
        _ => {
            return Err(MmError::State {
                message: "configuration layout change must create or remove one split file"
                    .to_owned(),
            });
        }
    }
    let document = loader.load(anchor)?;
    FileExt::unlock(&lock).map_err(|source| MmError::Io {
        context: format!("unlocking configuration {}", anchor.display()),
        source,
    })?;
    Ok(document)
}

/// Creates the split file first. Until the anchor is rewritten the key is
/// declared twice, which fails loudly instead of silently falling back to a
/// default.
fn commit_split(
    anchor: &Path,
    anchor_after: &str,
    split_path: &Path,
    text: &str,
) -> Result<(), MmError> {
    let directory = split_path.parent().ok_or_else(|| MmError::State {
        message: "split configuration file has no directory".to_owned(),
    })?;
    let created_directory = if directory.exists() {
        ensure_user_controlled_directory(directory)?;
        false
    } else {
        create_project_directory(directory).map_err(|source| MmError::Io {
            context: format!("creating {}", directory.display()),
            source,
        })?;
        true
    };
    let remove_created_directory = || {
        if created_directory {
            fs::remove_dir(directory)
        } else {
            Ok(())
        }
    };
    if let Err(error) = publish_new_project_file(split_path, text.as_bytes()) {
        return Err(rollback_error(error, remove_created_directory(), directory));
    }
    if let Err(error) = write_config_atomically(anchor, anchor_after) {
        let cleanup = fs::remove_file(split_path).and_then(|()| remove_created_directory());
        return Err(rollback_error(error, cleanup, split_path));
    }
    Ok(())
}

/// Rewrites the anchor first, then removes the split file. The window in
/// between also declares the key twice.
fn commit_join(
    anchor: &Path,
    anchor_before: &str,
    anchor_after: &str,
    split_path: &Path,
) -> Result<(), MmError> {
    write_config_atomically(anchor, anchor_after)?;
    if let Err(source) = fs::remove_file(split_path) {
        let error = MmError::Io {
            context: format!("removing {}", split_path.display()),
            source,
        };
        return Err(match write_config_atomically(anchor, anchor_before) {
            Ok(()) => error,
            Err(restore) => MmError::State {
                message: format!(
                    "{error}; restoring {} also failed: {restore}",
                    anchor.display()
                ),
            },
        });
    }
    // The last split file leaves an empty `.config/mm/` behind; remove it
    // unless something else appeared there in the meantime.
    if let Some(directory) = split_path.parent()
        && fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_none())
    {
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("removing empty {}", directory.display()),
                    source,
                });
            }
        }
    }
    Ok(())
}

fn stale() -> MmError {
    MmError::State {
        message: "configuration changed after review; no changes were written".to_owned(),
    }
}

fn rollback_error(error: MmError, cleanup: std::io::Result<()>, path: &Path) -> MmError {
    match cleanup {
        Ok(()) => error,
        Err(cleanup) => MmError::State {
            message: format!(
                "{error}; removing {} during rollback also failed: {cleanup}",
                path.display()
            ),
        },
    }
}

fn splittable_key(key: &str) -> Result<&'static str, MmError> {
    SPLITTABLE_CONFIG_KEYS
        .iter()
        .copied()
        .find(|known| *known == key)
        .ok_or_else(|| MmError::Environment {
            message: if key == "schema-version" {
                "schema-version must stay in .config/mm.yaml".to_owned()
            } else {
                format!(
                    "'{key}' is not a top-level configuration key; choose one of: {}",
                    SPLITTABLE_CONFIG_KEYS.join(", ")
                )
            },
        })
}

fn read_text(path: &Path) -> Result<String, MmError> {
    fs::read_to_string(path).map_err(|source| MmError::ConfigRead {
        path: path.to_path_buf(),
        source,
    })
}

fn read_optional_text(path: &Path) -> Result<Option<String>, MmError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(MmError::ConfigRead {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn layout_changes(
    anchor: &Path,
    anchor_before: &str,
    anchor_after: &str,
    split_path: &Path,
    split_before: Option<String>,
    split_after: Option<String>,
) -> Vec<ConfigFileChange> {
    vec![
        ConfigFileChange {
            path: anchor.to_path_buf(),
            before: Some(anchor_before.to_owned()),
            after: Some(anchor_after.to_owned()),
        },
        ConfigFileChange {
            path: split_path.to_path_buf(),
            before: split_before,
            after: split_after,
        },
    ]
}

/// A layout change must leave the assembled configuration exactly as it was.
fn preserves_meaning(
    document: &ConfigDocument,
    changes: &[ConfigFileChange],
) -> Result<bool, MmError> {
    let changes = changes
        .iter()
        .map(|change| (change.path.clone(), change.after.clone()))
        .collect::<Vec<(PathBuf, Option<String>)>>();
    match parse_configuration_with(&document.path, &changes) {
        Ok(parsed) => Ok(parsed.config == document.config),
        Err(MmError::ConfigParse { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Removes top-level `key` from `source` and returns the remaining text and
/// the key's value as the text of a split file, keeping comments and layout.
fn extract_top_level(source: &str, key: &str) -> Option<(String, String)> {
    let root = root_mapping(source);
    let entry = direct_entry(source, root, key)?;
    // Blank lines and column-0 comments after the value introduce the next key.
    let mut lines = source[entry.start..entry.end]
        .split_inclusive('\n')
        .collect::<Vec<_>>();
    while lines.len() > 1
        && lines
            .last()
            .is_some_and(|line| line.trim().is_empty() || line.starts_with('#'))
    {
        lines.pop();
    }
    let end = entry.start + lines.iter().map(|line| line.len()).sum::<usize>();
    // Column-0 comments directly above the key describe it and move with it.
    let mut start = entry.start;
    while let Some(previous) = source[..start].strip_suffix('\n') {
        let line_start = previous.rfind('\n').map_or(0, |offset| offset + 1);
        if previous[line_start..].starts_with('#') {
            start = line_start;
        } else {
            break;
        }
    }
    let leading = &source[start..entry.start];
    let block = &source[entry.start..end];
    let first_end = block.find('\n').map_or(block.len(), |offset| offset + 1);
    let inline = block[..first_end]
        .strip_prefix(key)?
        .strip_prefix(':')?
        .trim();
    let children = dedent(&block[first_end..]);
    let mut value = leading.to_owned();
    if !inline.is_empty() {
        value.push_str(inline);
        value.push('\n');
    }
    value.push_str(&children);
    let mut remaining = String::with_capacity(source.len());
    remaining.push_str(&source[..start]);
    remaining.push_str(&source[end..]);
    Some((remaining, value))
}

fn dedent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    text.split_inclusive('\n')
        .map(|line| {
            let leading = line.len() - line.trim_start_matches(' ').len();
            &line[leading.min(indent)..]
        })
        .collect()
}

fn rendered_value(anchor: &Path, source: &str, key: &str) -> Result<String, MmError> {
    let value: Value = serde_yaml_ng::from_str(source).map_err(|error| MmError::ConfigParse {
        path: anchor.to_path_buf(),
        message: error.to_string(),
    })?;
    let value = value.get(key).cloned().unwrap_or(Value::Null);
    serde_yaml_ng::to_string(&value).map_err(|error| MmError::State {
        message: format!("could not render '{key}': {error}"),
    })
}

/// Appends `key` with the split file's text as its value, keeping comments.
fn append_top_level(anchor: &str, key: &str, split: &str) -> String {
    let mut output = anchor.to_owned();
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    let content = split
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .collect::<Vec<_>>();
    let single_inline = match content.as_slice() {
        [line] => {
            let line = line.trim();
            line.starts_with('{')
                || line.starts_with('[')
                || serde_yaml_ng::from_str::<Value>(line)
                    .is_ok_and(|value| !value.is_mapping() && !value.is_sequence())
        }
        _ => false,
    };
    if single_inline {
        let line = content.first().map_or("", |line| line.trim());
        output.push_str(&format!("{key}: {line}\n"));
        return output;
    }
    output.push_str(key);
    output.push_str(":\n");
    for line in split.split_inclusive('\n') {
        if line.trim().is_empty() {
            output.push_str(line.trim_start_matches([' ', '\t']));
        } else {
            output.push_str("  ");
            output.push_str(line);
        }
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn append_rendered(anchor: &str, key: &str, value: &Value) -> Result<String, MmError> {
    let mut mapping = serde_yaml_ng::Mapping::new();
    mapping.insert(Value::String(key.to_owned()), value.clone());
    let rendered = serde_yaml_ng::to_string(&mapping).map_err(|error| MmError::State {
        message: format!("could not render '{key}': {error}"),
    })?;
    let mut output = anchor.to_owned();
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(&rendered);
    Ok(output)
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
