use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::app::unknown_keys::{Ignored, deserialize_tracked};
use crate::domain::config::unknown_keys::{dotted, suggestion};
use crate::domain::config::{ConfigDocument, UnknownConfigKey};
use crate::domain::error::MmError;
use crate::domain::ingress::CanonicalMiddleware;
use crate::domain::path_policy::{ensure_no_symlink_components, read_regular_file};
use crate::domain::stack::StackMiddlewareInput;
use crate::domain::workspace::ProjectId;

pub const MAX_MIDDLEWARE_FILE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedProjectMiddleware {
    pub inputs: Vec<StackMiddlewareInput>,
    pub definitions: BTreeMap<String, CanonicalMiddleware>,
    pub names: Vec<String>,
    /// Keys the middleware files set that this release does not read.
    pub unknown_keys: Vec<UnknownConfigKey>,
}

pub fn load_active_middlewares(
    document: &ConfigDocument,
    environment: &str,
) -> Result<LoadedProjectMiddleware, MmError> {
    let mut inputs = Vec::new();
    let mut definitions = BTreeMap::new();
    let mut names = Vec::new();
    let mut unknown_keys = Vec::new();
    for (bundle_name, bundle) in &document.config.ingress.middlewares {
        if !bundle_is_active(bundle.environments.as_slice(), environment) {
            continue;
        }
        let path = document.root.join(&bundle.file);
        ensure_no_symlink_components(&path)?;
        let bytes = read_regular_file(&path, MAX_MIDDLEWARE_FILE_BYTES)?.ok_or_else(|| {
            MmError::Environment {
                message: format!(
                    "middleware bundle '{bundle_name}' file does not exist: {}",
                    path.display()
                ),
            }
        })?;
        let (parsed, ignored) = read_middleware_file(&path, &bytes)?;
        unknown_keys.extend(middleware_unknown_keys(&bundle.file, &ignored));
        if parsed.is_empty() {
            return Err(MmError::Environment {
                message: format!("middleware bundle '{bundle_name}' defines no middlewares"),
            });
        }
        inputs.push(StackMiddlewareInput {
            bundle: bundle_name.clone(),
            path: bundle.file.clone(),
            digest: blake3::hash(&bytes).to_hex().to_string(),
        });
        for (definition_name, definition) in parsed {
            let namespaced = namespaced_middleware_name(
                document.config.project.id,
                bundle_name,
                &definition_name,
            );
            definitions.insert(namespaced.clone(), definition);
            names.push(namespaced);
        }
    }
    names.sort();
    crate::app::unknown_keys::report(&unknown_keys);
    Ok(LoadedProjectMiddleware {
        inputs,
        definitions,
        names,
        unknown_keys,
    })
}

pub fn namespaced_middleware_name(project_id: ProjectId, bundle: &str, definition: &str) -> String {
    format!("mm-{}-{bundle}-{definition}", project_id.0.as_simple())
}

pub fn parse_middleware_file(
    path: &Path,
    bytes: &[u8],
) -> Result<BTreeMap<String, CanonicalMiddleware>, MmError> {
    read_middleware_file(path, bytes).map(|(definitions, _)| definitions)
}

/// The definitions of a middleware file, and the keys it sets that this
/// release ignores.
pub fn read_middleware_file(
    path: &Path,
    bytes: &[u8],
) -> Result<(BTreeMap<String, CanonicalMiddleware>, Ignored), MmError> {
    let invalid = |error: String| MmError::Environment {
        message: format!(
            "middleware file '{}' is not a bounded http.middlewares document: {error}",
            path.display()
        ),
    };
    let text = std::str::from_utf8(bytes).map_err(|error| invalid(error.to_string()))?;
    let (document, ignored) =
        deserialize_tracked::<MiddlewareFile, _>(serde_yaml_ng::Deserializer::from_str(text))
            .map_err(|(error, _)| invalid(error.to_string()))?;
    let mut definitions = BTreeMap::new();
    for (name, raw) in document.http.middlewares {
        if !is_middleware_name(&name) {
            return Err(MmError::Environment {
                message: format!(
                    "middleware file '{}' has invalid middleware name '{name}'",
                    path.display()
                ),
            });
        }
        let definition = raw.into_canonical(path, &name)?;
        definitions.insert(name, definition);
    }
    Ok((definitions, ignored))
}

/// `ignored` of the middleware file `file`, relative to the project.
pub fn middleware_unknown_keys(file: &Path, ignored: &Ignored) -> Vec<UnknownConfigKey> {
    let schema = serde_json::to_value(schemars::schema_for!(MiddlewareFile)).unwrap_or_default();
    ignored
        .iter()
        .map(|path| UnknownConfigKey {
            file: file.to_string_lossy().into_owned(),
            key: dotted(path),
            suggestion: suggestion(&schema, path),
        })
        .collect()
}

pub fn bundle_is_active(environments: &[String], environment: &str) -> bool {
    environments.is_empty() || environments.iter().any(|name| name == environment)
}

fn is_middleware_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct MiddlewareFile {
    http: MiddlewareHttp,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct MiddlewareHttp {
    middlewares: BTreeMap<String, RawMiddleware>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct RawMiddleware {
    #[serde(rename = "replacePathRegex", default)]
    replace_path_regex: Option<RawReplacePathRegex>,
}

impl RawMiddleware {
    fn into_canonical(self, path: &Path, name: &str) -> Result<CanonicalMiddleware, MmError> {
        match self.replace_path_regex {
            Some(replace_path_regex) => {
                validate_middleware_string(&replace_path_regex.regex, path, name, "regex")?;
                validate_middleware_string(
                    &replace_path_regex.replacement,
                    path,
                    name,
                    "replacement",
                )?;
                Ok(CanonicalMiddleware::ReplacePathRegex {
                    regex: replace_path_regex.regex,
                    replacement: replace_path_regex.replacement,
                })
            }
            None => Err(MmError::Environment {
                message: format!(
                    "middleware file '{}' definition '{name}' has no supported type; only replacePathRegex is allowed",
                    path.display()
                ),
            }),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct RawReplacePathRegex {
    regex: String,
    replacement: String,
}

fn validate_middleware_string(
    value: &str,
    path: &Path,
    name: &str,
    field: &str,
) -> Result<(), MmError> {
    if value.is_empty() || value.chars().any(char::is_control) || value.len() > 4096 {
        return Err(MmError::Environment {
            message: format!(
                "middleware file '{}' definition '{name}' has an invalid {field}",
                path.display()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
