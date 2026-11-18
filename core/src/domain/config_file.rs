//! Files a service ships with: a proxy configuration, an init script.
//!
//! A config file is a file of the project, or a template of the project
//! with the environment's bindings filled in. Every host that runs the
//! service keeps the result under a name made from its content, owner and
//! mode, and the container mounts that file read-only. A changed file is
//! therefore another file on the host and another mount for the service,
//! which is what makes the engine replace the service's container and
//! nothing else; a file that did not change keeps its name from one
//! deployment to the next.
//!
//! Config files are stored as they are and their content names them, so a
//! template never takes a secret: those are `secret-files`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::MmConfig;
use crate::domain::config_resolve::{effective_binding, resolve_cast_url};
use crate::domain::release_facts::{ReleaseValue, is_image_key, release_value};

/// The largest config file mm ships. They are read into memory, hashed and
/// copied on every plan.
pub const MAX_CONFIG_FILE_BYTES: usize = 1024 * 1024;

/// The directory, next to an environment's stack artifacts, that holds
/// its config file versions. A stack's name never starts with a dot.
pub const CONFIG_FILE_DIRECTORY: &str = ".config-files";

/// A config file one service mounts, as a deployment plan lists it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigFileUse {
    pub stack: String,
    pub service: String,
    /// The file's path in the container.
    pub target: String,
    /// The file or template it comes from, relative to the project.
    pub source: String,
    /// Whether `source` is a template rendered from the bindings.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub template: bool,
    /// The digest of the file's content, owner and mode.
    pub version: String,
    /// The version the last finished deployment mounted at this path on
    /// this host. Absent when the service had no such file. It says what
    /// the host runs, so it is not part of what the plan's hash covers:
    /// the stack's artifact already covers `version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
}

/// What a plan does about one config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFileChange {
    /// The service did not mount this file before.
    New,
    /// The content, owner or mode changed: the service's container is
    /// replaced.
    Changed,
    Unchanged,
}

impl ConfigFileUse {
    #[must_use]
    pub fn change(&self) -> ConfigFileChange {
        match &self.previous {
            None => ConfigFileChange::New,
            Some(previous) if *previous == self.version => ConfigFileChange::Unchanged,
            Some(_) => ConfigFileChange::Changed,
        }
    }

    /// The version as a plan prints it.
    #[must_use]
    pub fn short_version(&self) -> &str {
        self.version.get(..12).unwrap_or(&self.version)
    }

    /// Whether `other` is the same mount of the same service.
    #[must_use]
    pub fn same_mount(&self, other: &Self) -> bool {
        self.stack == other.stack && self.service == other.service && self.target == other.target
    }
}

/// The name of one version of a config file: a digest of its content, its
/// mode and its owner, so two services that want the same bytes under
/// different owners get two files on the host.
#[must_use]
pub fn version(bytes: &[u8], mode: u32, uid: Option<u32>, gid: Option<u32>) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mm-config-file-v1\0");
    hasher.update(&mode.to_le_bytes());
    for id in [uid, gid] {
        match id {
            Some(id) => {
                hasher.update(&[1]);
                hasher.update(&id.to_le_bytes());
            }
            None => {
                hasher.update(&[0]);
            }
        }
    }
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

/// Whether `name` is a version's file name.
#[must_use]
pub fn is_version(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Why `target` cannot be a config file's path in a container.
#[must_use]
pub fn target_problem(target: &str) -> Option<&'static str> {
    if !target.starts_with('/') {
        return Some("is not an absolute path in the container, such as /etc/nginx/nginx.conf");
    }
    if target.ends_with('/') {
        return Some("names a directory; a config file is mounted as one file");
    }
    if target
        .split('/')
        .skip(1)
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Some("has an empty, `.` or `..` part");
    }
    // The mount is written as `source:target:ro`.
    if target.contains(':') || target.chars().any(char::is_control) {
        return Some("contains a colon or a control character");
    }
    None
}

/// Why a binding cannot be written into a config file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingProblem {
    Unknown,
    /// The binding is the named secret.
    Secret(String),
    Unresolved(String),
}

/// The text a template gets for binding `key` in `environment`.
pub fn binding_text(
    config: &MmConfig,
    environment: &str,
    key: &str,
) -> Result<String, BindingProblem> {
    match release_value(config, environment, key) {
        Some(ReleaseValue::Known(value)) => return Ok(value),
        Some(ReleaseValue::Pending) => return Ok(String::new()),
        Some(ReleaseValue::Missing(reason)) => return Err(BindingProblem::Unresolved(reason)),
        None => {}
    }
    let binding = effective_binding(config, environment, key).ok_or(BindingProblem::Unknown)?;
    if let Some(secret) = binding.secret_name() {
        return Err(BindingProblem::Secret(secret.to_owned()));
    }
    if let Some(cast) = binding.cast() {
        return resolve_cast_url(config, environment, cast).map_err(BindingProblem::Unresolved);
    }
    binding
        .value()
        .map(ToString::to_string)
        .ok_or(BindingProblem::Unknown)
}

/// A place in a template that is not a placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateProblem {
    pub line: usize,
    pub key: Option<String>,
    pub problem: Option<BindingProblem>,
}

/// `template` with every `{{ KEY }}` replaced by what `value` gives for
/// the key. Nothing else is special: there are no conditions, loops or
/// escapes, and a file that has to contain `{{` itself is shipped as a
/// file, not as a template.
pub fn render(
    template: &str,
    mut value: impl FnMut(&str) -> Result<String, BindingProblem>,
) -> Result<String, TemplateProblem> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    let mut line = 1;
    while let Some(start) = rest.find("{{") {
        let (before, after) = rest.split_at(start);
        line += before.matches('\n').count();
        rendered.push_str(before);
        let body = &after[2..];
        let malformed = TemplateProblem {
            line,
            key: None,
            problem: None,
        };
        let Some(end) = body.find("}}") else {
            return Err(malformed);
        };
        let key = body[..end].trim();
        if !is_binding_key(key) {
            return Err(malformed);
        }
        rendered.push_str(&value(key).map_err(|problem| TemplateProblem {
            line,
            key: Some(key.to_owned()),
            problem: Some(problem),
        })?);
        rest = &body[end + 2..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

/// The binding keys `template` names, each once, in the order `render`
/// asks for them. A malformed placeholder ends the list where `render`
/// stops; `mm config validate` reports it.
pub fn template_keys(template: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    // A malformed placeholder stops `render`; the keys before it still count.
    let _reported_by_validate = render(template, |key| {
        if !keys.iter().any(|known| known == key) {
            keys.push(key.to_owned());
        }
        Ok(String::new())
    });
    keys
}

/// A binding's key, as `bindings:` spells it: `APP.URL`.
fn is_binding_key(key: &str) -> bool {
    is_image_key(key)
        || (!key.is_empty()
            && key.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.chars().all(|character| {
                        character.is_ascii_uppercase()
                            || character.is_ascii_digit()
                            || character == '_'
                    })
            }))
}
