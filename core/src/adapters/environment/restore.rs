//! The environment of a plan put back onto the stack files of an earlier
//! deployment.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::error::MmError;

/// The `overlays` under `root` (a bundle's directory), each limited to
/// `services`.
///
/// A rollback puts back the stack files of the last deployment with the
/// environment of the plan at hand, whose values are resolved from the
/// configuration now. When that plan's stack has a service the earlier
/// files do not define, Compose refuses the whole stack, since the service
/// would have no image. An overlay that names such a service is copied
/// beside itself without it; an overlay that names none, and a file outside
/// `root`, is returned as it is.
pub fn limited_overlays(
    root: &Path,
    overlays: &[PathBuf],
    services: &BTreeSet<String>,
) -> Result<Vec<PathBuf>, MmError> {
    overlays
        .iter()
        .map(|overlay| limited(root, overlay, services))
        .collect()
}

fn limited(root: &Path, overlay: &Path, services: &BTreeSet<String>) -> Result<PathBuf, MmError> {
    let Some(name) = overlay
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|_| overlay.parent() == Some(root))
    else {
        return Ok(overlay.to_path_buf());
    };
    let text = fs::read_to_string(overlay).map_err(|source| MmError::Io {
        context: format!("reading the overlay {}", overlay.display()),
        source,
    })?;
    let mut value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&text).map_err(|error| MmError::Environment {
            message: format!("cannot read the overlay {}: {error}", overlay.display()),
        })?;
    let Some(named) = value
        .get_mut("services")
        .and_then(serde_yaml_ng::Value::as_mapping_mut)
    else {
        return Ok(overlay.to_path_buf());
    };
    let before = named.len();
    named.retain(|service, _| {
        service
            .as_str()
            .is_some_and(|service| services.contains(service))
    });
    if named.len() == before {
        return Ok(overlay.to_path_buf());
    }
    let bytes = serde_yaml_ng::to_string(&value).map_err(|error| MmError::Environment {
        message: format!("cannot write the limited overlay of {name}: {error}"),
    })?;
    let limited = root.join(format!("restore.{name}"));
    match fs::remove_file(&limited) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            return Err(MmError::Io {
                context: format!("replacing {}", limited.display()),
                source,
            });
        }
        _ => {}
    }
    super::artifacts::write_new_private_file(&limited, bytes.as_bytes())?;
    Ok(limited)
}
