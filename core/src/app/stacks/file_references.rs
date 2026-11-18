//! Relative file references in a Compose file, made absolute.
//!
//! mm deploys copies of the project's Compose files from its own artifact
//! directory. Docker Compose resolves relative paths against
//! `--project-directory`, but `docker stack deploy` has no such option and
//! resolves them against each file's directory, which is mm's copy. Writing
//! them as absolute paths under the project directory makes both resolve the
//! same files.

use std::path::Path;

use serde_yaml_ng::Value;

use crate::domain::error::MmError;

/// The file with every relative bind source, `env_file`, and `configs` or
/// `secrets` `file:` made absolute under `directory`. Unchanged files keep
/// their bytes.
pub(super) fn absolute_file_references(
    path: &Path,
    bytes: Vec<u8>,
    directory: &Path,
) -> Result<Vec<u8>, MmError> {
    let error = |message: String| MmError::Environment {
        message: format!("Compose file '{}': {message}", path.display()),
    };
    let mut model: Value =
        serde_yaml_ng::from_slice(&bytes).map_err(|cause| error(cause.to_string()))?;
    let mut changed = false;
    if let Some(services) = model.get_mut("services").and_then(Value::as_mapping_mut) {
        for (_, service) in services.iter_mut() {
            if let Some(volumes) = service.get_mut("volumes").and_then(Value::as_sequence_mut) {
                for volume in volumes {
                    changed |= absolute_volume(volume, directory);
                }
            }
            if let Some(env_file) = service.get_mut("env_file") {
                changed |= absolute_env_files(env_file, directory);
            }
        }
    }
    for section in ["configs", "secrets"] {
        if let Some(entries) = model.get_mut(section).and_then(Value::as_mapping_mut) {
            for (_, entry) in entries.iter_mut() {
                if let Some(file) = entry.get_mut("file") {
                    changed |= absolute_path(file, directory);
                }
            }
        }
    }
    if !changed {
        return Ok(bytes);
    }
    serde_yaml_ng::to_string(&model)
        .map(String::into_bytes)
        .map_err(|cause| error(cause.to_string()))
}

fn is_relative(path: &str) -> bool {
    path == "." || path == ".." || path.starts_with("./") || path.starts_with("../")
}

fn joined(directory: &Path, relative: &str) -> String {
    let joined = directory.join(relative);
    let mut normalized = std::path::PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized.to_string_lossy().into_owned()
}

fn absolute_path(value: &mut Value, directory: &Path) -> bool {
    match value.as_str() {
        Some(path) if is_relative(path) => {
            *value = Value::String(joined(directory, path));
            true
        }
        _ => false,
    }
}

/// `./src:/dst[:mode]`, or `{type: bind, source: ./src, …}`.
fn absolute_volume(volume: &mut Value, directory: &Path) -> bool {
    if let Some(short) = volume.as_str() {
        let Some((source, rest)) = short.split_once(':') else {
            return false;
        };
        if !is_relative(source) {
            return false;
        }
        *volume = Value::String(format!("{}:{rest}", joined(directory, source)));
        return true;
    }
    let is_bind = volume.get("type").and_then(Value::as_str) == Some("bind");
    match volume.get_mut("source") {
        Some(source) if is_bind => absolute_path(source, directory),
        _ => false,
    }
}

/// `env_file: ./a.env`, a list of paths, or a list of `{path: …}`.
fn absolute_env_files(value: &mut Value, directory: &Path) -> bool {
    if let Some(entries) = value.as_sequence_mut() {
        let mut changed = false;
        for entry in entries {
            changed |= match entry.get_mut("path") {
                Some(path) => absolute_path(path, directory),
                None => absolute_path(entry, directory),
            };
        }
        return changed;
    }
    absolute_path(value, directory)
}

#[cfg(test)]
#[path = "file_references_tests.rs"]
mod tests;
