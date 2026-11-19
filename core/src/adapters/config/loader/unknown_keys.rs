use crate::app::unknown_keys::{Ignored, deserialize_tracked};
use crate::domain::config::UnknownConfigKey;
use crate::domain::config::unknown_keys::{KeySegment, dotted, suggestion};

fn configuration_schema() -> &'static serde_json::Value {
    static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::to_value(ConfigLoader.schema()).unwrap_or_default())
}

fn unknown_key(file: &str, path: &[KeySegment]) -> UnknownConfigKey {
    UnknownConfigKey {
        file: file.to_owned(),
        key: dotted(path),
        suggestion: suggestion(configuration_schema(), path),
    }
}

/// `message` with the unknown keys read before a failure.
fn with_unknown_keys(message: String, ignored: &Ignored) -> String {
    let mut message = message;
    for path in ignored.iter().take(3) {
        let key = unknown_key("", path);
        message.push_str(&format!("; unknown key `{}`", key.key));
        if let Some(suggestion) = &key.suggestion {
            message.push_str(&format!(" (did you mean `{suggestion}`?)"));
        }
    }
    message
}

/// Names each ignored key with the file the user wrote it in: the local
/// overrides when they set it, else the split file of its top-level key,
/// else `.config/mm.yaml`.
fn unknown_keys_in(
    ignored: &Ignored,
    split: &[SplitSource],
    overlay: Option<&Value>,
) -> Vec<UnknownConfigKey> {
    ignored
        .iter()
        .map(|path| {
            let file = match path.first() {
                _ if overlay.is_some_and(|overlay| sets(overlay, path)) => {
                    OVERRIDES_RELATIVE_PATH.to_owned()
                }
                Some(KeySegment::Key(key)) if split.iter().any(|source| source.key == key.as_str()) => {
                    format!("{SPLIT_CONFIG_RELATIVE_DIRECTORY}/{key}.yaml")
                }
                _ => CONFIG_RELATIVE_PATH.to_owned(),
            };
            unknown_key(&file, path)
        })
        .collect()
}

/// A split file whose name is not a key this release reads.
fn unknown_split_file(key: &str) -> UnknownConfigKey {
    unknown_key(
        &format!("{SPLIT_CONFIG_RELATIVE_DIRECTORY}/{key}.yaml"),
        &[KeySegment::Key(key.to_owned())],
    )
}

/// The keys `development:` of the local overrides ignored.
fn unknown_development_keys(ignored: &Ignored) -> Vec<UnknownConfigKey> {
    ignored
        .iter()
        .map(|path| {
            let mut full = vec![KeySegment::Key(LOCAL_DEVELOPMENT_KEY.to_owned())];
            full.extend(path.iter().cloned());
            unknown_key(OVERRIDES_RELATIVE_PATH, &full)
        })
        .collect()
}

fn sets(value: &Value, path: &[KeySegment]) -> bool {
    let Some((first, rest)) = path.split_first() else {
        return true;
    };
    let child = match (first, value) {
        (_, Value::Tagged(tagged)) => return sets(&tagged.value, path),
        (KeySegment::Key(key), Value::Mapping(mapping)) => mapping
            .iter()
            .find(|(name, _)| key_text(name).as_deref() == Some(key.as_str()))
            .map(|(_, child)| child),
        (KeySegment::Index(index), Value::Sequence(items)) => items.get(*index),
        _ => None,
    };
    child.is_some_and(|child| sets(child, rest))
}

fn key_text(key: &Value) -> Option<String> {
    match key {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}
