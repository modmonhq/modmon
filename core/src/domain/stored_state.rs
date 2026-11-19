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
