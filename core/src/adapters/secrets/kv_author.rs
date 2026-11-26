//! Who wrote a KV v2 version, kept in the secret's custom metadata with the
//! version it describes.

use std::collections::BTreeMap;

const SET_BY: &str = "mm_set_by";
const SET_VERSION: &str = "mm_set_version";

/// The metadata write that records `set_by` as the writer of `version`.
pub(super) fn author_metadata(version: u64, set_by: &str) -> serde_json::Value {
    let mut custom = serde_json::Map::new();
    custom.insert(SET_BY.to_owned(), set_by.into());
    custom.insert(SET_VERSION.to_owned(), version.to_string().into());
    serde_json::json!({ "custom_metadata": custom })
}

/// The writer recorded for `current`; none when the record is for another
/// version, such as one written outside mm.
pub(super) fn recorded_author(
    custom: Option<&BTreeMap<String, String>>,
    current: u64,
) -> Option<String> {
    let custom = custom?;
    if custom.get(SET_VERSION)? != &current.to_string() {
        return None;
    }
    custom
        .get(SET_BY)
        .filter(|set_by| crate::domain::secret::validate_set_by(set_by).is_ok())
        .cloned()
}
