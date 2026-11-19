//! The release a deployment runs: its commit, its name, the images it was
//! built into, and the values the deployer supplies with the plan.
//! Bindings, env templates and config-file templates read them as `MM.`
//! and `RELEASE.` keys. They are part of the plan and recorded with the
//! deployment, so a rollback runs with the facts of the release it returns
//! to. None of them is ever secret.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::{EnvironmentBinding, MmConfig};
use crate::domain::config_resolve::effective_binding;

pub const FACT_PREFIX: &str = "MM.";
pub const INPUT_PREFIX: &str = "RELEASE.";
pub const COMMIT: &str = "MM.COMMIT";
pub const COMMIT_SHORT: &str = "MM.COMMIT_SHORT";
pub const RELEASE: &str = "MM.RELEASE";
pub const IMAGE: &str = "MM.IMAGE.";
pub const IMAGE_DIGEST: &str = "MM.IMAGE_DIGEST.";

const SHORT_COMMIT: usize = 12;
const MAX_NAME_BYTES: usize = 128;
const MAX_INPUT_BYTES: usize = 4096;

/// One value under `release-inputs:` that `mm deploy plan` takes with
/// `--release-input NAME=VALUE`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ReleaseInputConfig {
    /// A regular expression the whole value has to match, such as
    /// `^[A-Za-z0-9._-]+$`. Without one, any value of printable text is
    /// accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

impl ReleaseInputConfig {
    /// The pattern as one that has to match the whole value.
    pub fn matcher(&self) -> Result<Option<regex::Regex>, String> {
        self.pattern
            .as_deref()
            .map(|pattern| {
                regex::Regex::new(&format!("^(?:{pattern})$"))
                    .map_err(|error| format!("'{pattern}' is not a regular expression: {error}"))
            })
            .transpose()
    }
}

/// What a client asks a plan to deploy besides its commit and build.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRequest {
    /// The release's name (`--release`); `sha-<commit>` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The declared release inputs, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
}

impl ReleaseRequest {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.inputs.is_empty()
    }

    /// The options that ask for this request again, each with a leading
    /// space: ` --release v1 --release-input BUILD=7`.
    #[must_use]
    pub fn arguments(&self) -> String {
        let quoted = |value: &str| {
            if !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-/:@=+,".contains(&byte))
            {
                value.to_owned()
            } else {
                format!("'{}'", value.replace('\'', "'\\''"))
            }
        };
        let mut arguments = String::new();
        if let Some(name) = &self.name {
            arguments.push_str(&format!(" --release {}", quoted(name)));
        }
        for (key, value) in &self.inputs {
            arguments.push_str(&format!(
                " --release-input {}",
                quoted(&format!("{key}={value}"))
            ));
        }
        arguments
    }

    /// The request `--release NAME` and `--release-input NAME=VALUE ...`
    /// make. A name given twice is refused.
    pub fn from_arguments(name: Option<String>, inputs: &[String]) -> Result<Self, String> {
        let mut parsed = BTreeMap::new();
        for input in inputs {
            let Some((key, value)) = input.split_once('=') else {
                return Err(format!(
                    "--release-input '{input}' has no value; write it as NAME=VALUE, such as --release-input BUILD_ID=4757.2.3"
                ));
            };
            if parsed.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(format!(
                    "--release-input {key} is given twice; give each input once"
                ));
            }
        }
        Ok(Self {
            name,
            inputs: parsed,
        })
    }
}

/// The release a deployment plan runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseFacts {
    /// The full commit SHA; absent for a checkout that is not in git.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    /// Each module's image, as `repository@sha256:...`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, String>,
}

/// What a release key resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseValue {
    Known(String),
    /// Nothing is being deployed, so there is no release to read it from:
    /// `mm config validate`, `mm env status`, `mm dev`.
    Pending,
    /// The release has no such value; the text says why.
    Missing(String),
}

impl ReleaseFacts {
    #[must_use]
    pub fn new(commit: Option<String>, request: ReleaseRequest) -> Self {
        Self {
            commit,
            name: request.name,
            inputs: request.inputs,
            images: BTreeMap::new(),
        }
    }

    /// The request that plans this release again.
    #[must_use]
    pub fn request(&self) -> ReleaseRequest {
        ReleaseRequest {
            name: self.name.clone(),
            inputs: self.inputs.clone(),
        }
    }

    #[must_use]
    pub fn with_images(mut self, images: BTreeMap<String, String>) -> Self {
        self.images = images;
        self
    }

    /// `MM.RELEASE`: the name given, or `sha-` and the short commit.
    #[must_use]
    pub fn release_name(&self) -> Option<String> {
        self.name.clone().or_else(|| {
            self.commit
                .as_deref()
                .map(|commit| format!("sha-{}", short(commit)))
        })
    }

    #[must_use]
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        let mut field = |value: &str| {
            hasher.update(&(value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        };
        field("release-facts-v1");
        field(self.commit.as_deref().unwrap_or_default());
        field(self.name.as_deref().unwrap_or_default());
        for (kind, values) in [("input", &self.inputs), ("image", &self.images)] {
            for (key, value) in values {
                field(kind);
                field(key);
                field(value);
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    /// Whether two deployments ran the same name and inputs.
    #[must_use]
    pub fn same_request(left: Option<&Self>, right: Option<&Self>) -> bool {
        left.map(Self::request).unwrap_or_default() == right.map(Self::request).unwrap_or_default()
    }

    /// Checks the name and inputs against what `release-inputs:` declares:
    /// nothing undeclared given, each value matching its pattern. Which
    /// inputs have to be given depends on what the environment's templates
    /// read. The error is the sentence the deployer reads.
    pub fn check(&self, declared: &BTreeMap<String, ReleaseInputConfig>) -> Result<(), String> {
        if let Some(name) = &self.name {
            check_name(name)?;
        }
        if let Some(undeclared) = self.inputs.keys().find(|key| !declared.contains_key(*key)) {
            return Err(if declared.is_empty() {
                format!(
                    "release input {undeclared} is not declared, and this commit declares none. Declare it under `release-inputs:` in .config/mm.yaml (`{undeclared}: {{}}`), or leave out --release-input {undeclared}"
                )
            } else {
                format!(
                    "release input {undeclared} is not declared; this commit declares {}. Declare it under `release-inputs:`, or leave out --release-input {undeclared}",
                    declared.keys().cloned().collect::<Vec<_>>().join(", ")
                )
            });
        }
        for (key, value) in &self.inputs {
            if let Some(config) = declared.get(key) {
                check_input(key, value, config)?;
            }
        }
        Ok(())
    }

    /// The value of a release key, or why this release has none.
    fn value(&self, key: &str) -> Result<String, String> {
        if let Some(name) = key.strip_prefix(INPUT_PREFIX) {
            return self
                .inputs
                .get(name)
                .cloned()
                .ok_or_else(|| format!("the plan was not given --release-input {name}=VALUE"));
        }
        if let Some((digest_only, module)) = image_key(key) {
            let image = self.images.get(module).ok_or_else(|| {
                format!(
                    "module '{module}' has no image in this deployment. Its image comes from the build the plan deploys: build the module, or plan with --receipt FILE from `mm build receipt`"
                )
            })?;
            return Ok(if digest_only {
                image
                    .rsplit_once('@')
                    .map_or(image.as_str(), |(_, digest)| digest)
                    .to_owned()
            } else {
                image.clone()
            });
        }
        let no_commit =
            || "this checkout is not a git repository, so the deployment has no commit".to_owned();
        match key {
            COMMIT => self.commit.clone().ok_or_else(no_commit),
            COMMIT_SHORT => self
                .commit
                .as_deref()
                .map(|commit| short(commit).to_owned())
                .ok_or_else(no_commit),
            RELEASE => self.release_name().ok_or_else(no_commit),
            _ => Err(format!("'{key}' is no fact mm knows about a release")),
        }
    }
}

fn short(commit: &str) -> &str {
    commit.get(..SHORT_COMMIT).unwrap_or(commit)
}

fn check_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "release name '{}' has to start with a letter or digit and use only letters, digits, '.', '_' and '-' (at most {MAX_NAME_BYTES}), such as v26.6.0",
            name.escape_debug()
        ))
    }
}

fn check_input(key: &str, value: &str, config: &ReleaseInputConfig) -> Result<(), String> {
    if value.chars().any(char::is_control) || value.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "release input {key} has to be one line of printable text, at most {MAX_INPUT_BYTES} bytes"
        ));
    }
    let matcher = config
        .matcher()
        .map_err(|problem| format!("release-inputs.{key}.pattern: {problem}"))?;
    match (matcher, &config.pattern) {
        (Some(matcher), Some(pattern)) if !matcher.is_match(value) => Err(format!(
            "release input {key} is '{value}', which does not match its pattern '{pattern}' from `release-inputs:`"
        )),
        _ => Ok(()),
    }
}

/// `MM.IMAGE.<module>` as `(false, module)`, `MM.IMAGE_DIGEST.<module>` as
/// `(true, module)`.
fn image_key(key: &str) -> Option<(bool, &str)> {
    let (digest_only, module) = match key.strip_prefix(IMAGE_DIGEST) {
        Some(module) => (true, module),
        None => (false, key.strip_prefix(IMAGE)?),
    };
    let valid = !module.is_empty()
        && module
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    valid.then_some((digest_only, module))
}

/// Whether `key` is spelled as an image fact: its last part is a module
/// name, which need not be upper case.
#[must_use]
pub fn is_image_key(key: &str) -> bool {
    image_key(key).is_some()
}

/// Whether `key` names a fact of the release: `MM.COMMIT`,
/// `MM.COMMIT_SHORT`, `MM.RELEASE`, `MM.IMAGE.<module>`,
/// `MM.IMAGE_DIGEST.<module>`, or `RELEASE.<NAME>` for an input
/// `release-inputs:` declares.
#[must_use]
pub fn is_release_key(config: &MmConfig, key: &str) -> bool {
    matches!(key, COMMIT | COMMIT_SHORT | RELEASE)
        || image_key(key).is_some_and(|(_, module)| config.modules.contains_key(module))
        || key
            .strip_prefix(INPUT_PREFIX)
            .is_some_and(|name| config.release_inputs.contains_key(name))
}

/// The release key `key` reads in `environment`: itself when it is a
/// release fact, or the one a binding refers to with `value-ref`; `None`
/// for every other key.
#[must_use]
pub fn release_source<'a>(
    config: &'a MmConfig,
    environment: &str,
    key: &'a str,
) -> Option<&'a str> {
    if is_release_key(config, key) {
        return Some(key);
    }
    match effective_binding(config, environment, key)? {
        EnvironmentBinding::Reference(reference) => Some(reference.value_ref.as_str()),
        _ => None,
    }
}

/// What `key` resolves to in `environment` when it is a release fact, or
/// a binding that refers to one with `value-ref`; `None` for every other
/// key.
#[must_use]
pub fn release_value(config: &MmConfig, environment: &str, key: &str) -> Option<ReleaseValue> {
    let source = release_source(config, environment, key)?;
    Some(match &config.release {
        None => ReleaseValue::Pending,
        Some(release) => match release.value(source) {
            Ok(value) => ReleaseValue::Known(value),
            Err(reason) => ReleaseValue::Missing(reason),
        },
    })
}
