//! Reading user-written configuration with the keys this release ignores,
//! and telling the user about them.

use std::sync::{Mutex, PoisonError};

use crate::domain::config::UnknownConfigKey;
use crate::domain::config::unknown_keys::KeySegment;

/// The paths, from the top of a file, of the keys one reading ignored.
pub type Ignored = Vec<Vec<KeySegment>>;

/// Hears of the unknown keys of a configuration the process read.
pub type Listener = fn(&[UnknownConfigKey]);

static LISTENER: Mutex<Option<Listener>> = Mutex::new(None);

/// Deserializes `T`, collecting the keys it ignores. A failure keeps the
/// keys ignored before it: a misspelled key often explains a missing one.
pub fn deserialize_tracked<'de, T, D>(deserializer: D) -> Result<(T, Ignored), (D::Error, Ignored)>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    let mut ignored = Vec::new();
    let result = serde_ignored::deserialize(deserializer, |path| {
        let mut segments = Vec::new();
        key_path(&path, &mut segments);
        ignored.push(segments);
    });
    match result {
        Ok(value) => Ok((value, ignored)),
        Err(error) => Err((error, ignored)),
    }
}

fn key_path(path: &serde_ignored::Path<'_>, segments: &mut Vec<KeySegment>) {
    match path {
        serde_ignored::Path::Root => {}
        serde_ignored::Path::Seq { parent, index } => {
            key_path(parent, segments);
            segments.push(KeySegment::Index(*index));
        }
        serde_ignored::Path::Map { parent, key } => {
            key_path(parent, segments);
            segments.push(KeySegment::Key(key.clone()));
        }
        serde_ignored::Path::Some { parent }
        | serde_ignored::Path::NewtypeStruct { parent }
        | serde_ignored::Path::NewtypeVariant { parent } => key_path(parent, segments),
    }
}

/// Has `listener` hear of the unknown keys in every configuration this
/// process reads from now on.
pub fn listen(listener: Listener) {
    *LISTENER.lock().unwrap_or_else(PoisonError::into_inner) = Some(listener);
}

/// Tells the listener, if there is one, of `keys`.
pub fn report(keys: &[UnknownConfigKey]) {
    if keys.is_empty() {
        return;
    }
    let listener = *LISTENER.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(listener) = listener {
        listener(keys);
    }
}
