// Split configuration files. Any top-level key except `schema-version` may move
// from `.config/mm.yaml` to `.config/mm/<key>.yaml`, which holds only that key's
// value. The loader assembles every source into one document before the
// optional local override layer applies.

/// Directory, relative to the project root, that holds split top-level keys.
pub const SPLIT_CONFIG_RELATIVE_DIRECTORY: &str = ".config/mm";
const CONFIG_SOURCE_BYTES: usize = 1024 * 1024;
const CONFIG_TOTAL_BYTES: usize = 4 * 1024 * 1024;

macro_rules! splittable_config_keys {
    ($($key:literal => $ty:ty),* $(,)?) => {
        /// Keys with a type check, in schema order. A test compares this with
        /// the domain's `SPLITTABLE_CONFIG_KEYS` and the generated schema.
        #[cfg(test)]
        const CHECKED_CONFIG_KEYS: &[&str] = &[$($key),*];

        /// Deserializes one split file as its key's type so errors keep the
        /// file's own line and column.
        fn check_split_source(key: &str, source: &str) -> Result<(), String> {
            match key {
                $($key => serde_yaml_ng::from_str::<$ty>(source)
                    .map(drop)
                    .map_err(|error| error.to_string()),)*
                other => Err(format!("'{other}' cannot be split into its own file")),
            }
        }

        /// Deserializes one top-level value of the anchor file as its key's type.
        fn check_anchor_value<'de, A: serde::de::MapAccess<'de>>(
            key: &str,
            map: &mut A,
        ) -> Result<(), A::Error> {
            match key {
                "schema-version" => {
                    let version = map.next_value::<u32>()?;
                    if version != CONFIG_SCHEMA_VERSION {
                        return Err(serde::de::Error::custom(format!(
                            "unsupported schema-version {version}; expected schema-version: {CONFIG_SCHEMA_VERSION}"
                        )));
                    }
                }
                $($key => {
                    map.next_value::<$ty>()?;
                })*
                _ => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
            Ok(())
        }
    };
}

splittable_config_keys! {
    "project" => crate::domain::config::ProjectConfig,
    "default-environment" => Option<String>,
    "target" => String,
    "runtime" => crate::domain::config::RuntimeConfig,
    "build" => crate::domain::config::BuildConfig,
    "networks" => std::collections::BTreeMap<String, crate::domain::config::NetworkConfig>,
    "stacks" => std::collections::BTreeMap<String, crate::domain::config::StackConfig>,
    "modules" => std::collections::BTreeMap<String, crate::domain::config::ModuleConfig>,
    "providers" => std::collections::BTreeMap<
        crate::domain::secret_backend::ProviderInstanceName,
        crate::domain::secret_backend::ProviderInstanceConfig,
    >,
    "secrets" => std::collections::BTreeMap<String, crate::domain::config::SecretDeclaration>,
    "bindings" => std::collections::BTreeMap<String, crate::domain::config::EnvironmentBinding>,
    "environment-delivery" => crate::domain::environment_delivery::EnvironmentDeliveryPolicy,
    "environments" => std::collections::BTreeMap<String, crate::domain::config::EnvironmentConfig>,
    "ingress" => crate::domain::config::ProjectIngressConfig,
    "development-mounts" => crate::domain::development_mount::DevelopmentMountPolicy,
    "development-mail" => crate::domain::development_mail::DevelopmentMailPolicy,
    "jobs" => std::collections::BTreeMap<String, crate::domain::config::JobConfig>,
    "schedules" => std::collections::BTreeMap<String, crate::domain::config::ScheduleConfig>,
    "gates" => std::collections::BTreeMap<String, crate::domain::config::GateConfig>,
    "users" => std::collections::BTreeMap<String, crate::domain::users::UserConfig>,
    "agents" => crate::domain::agents::AgentsConfig,
    "targets" => std::collections::BTreeMap<String, crate::domain::repo_target::RepoTargetConfig>,
    "registries" => std::collections::BTreeMap<String, crate::domain::config::RegistryConfig>,
    "hardening" => Option<crate::domain::hardening::Hardening>,
    "release-inputs" => std::collections::BTreeMap<String, crate::domain::release_facts::ReleaseInputConfig>,
}

/// Type-checks each top-level value of an anchor that has split siblings. The
/// anchor alone may miss required keys, so `MmConfig` cannot check it directly.
struct AnchorCheck;

impl<'de> serde::Deserialize<'de> for AnchorCheck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AnchorVisitor;

        impl<'de> serde::de::Visitor<'de> for AnchorVisitor {
            type Value = AnchorCheck;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a configuration mapping")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                while let Some(key) = map.next_key::<String>()? {
                    check_anchor_value(&key, &mut map)?;
                }
                Ok(AnchorCheck)
            }
        }

        deserializer.deserialize_map(AnchorVisitor)
    }
}

/// Path of the split file for a top-level key.
#[must_use]
pub fn split_config_path(anchor: &Path, key: &str) -> PathBuf {
    anchor.with_file_name("mm").join(format!("{key}.yaml"))
}

/// The anchor `.config/mm.yaml` that owns `path`, which is either the anchor
/// itself or one of its split files.
pub(super) fn anchor_for_source(path: &Path) -> PathBuf {
    let parent = path.parent();
    let in_split_directory = parent.and_then(Path::file_name).is_some_and(|name| name == "mm")
        && parent
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|name| name == ".config");
    match parent.and_then(Path::parent) {
        Some(config) if in_split_directory => config.join("mm.yaml"),
        _ => path.to_path_buf(),
    }
}

/// A replacement for one source while previewing an edit: `Some` text stands
/// in for the file on disk, and `None` means the file is absent.
pub(super) type SourceChange = (PathBuf, Option<String>);

#[derive(Debug)]
pub(super) struct ParsedConfiguration {
    pub(super) config: MmConfig,
    pub(super) source_hash: String,
    pub(super) sources: std::collections::BTreeMap<String, PathBuf>,
    pub(super) unknown_keys: Vec<UnknownConfigKey>,
}

#[derive(Debug)]
struct SplitSource {
    key: &'static str,
    path: PathBuf,
    text: String,
}

/// Parses the anchor with the given text and every other source from disk.
pub(super) fn parse_configuration(
    path: &Path,
    source: &str,
) -> Result<(MmConfig, String), MmError> {
    let parsed = parse_configuration_with(path, &[(path.to_path_buf(), Some(source.to_owned()))])?;
    Ok((parsed.config, parsed.source_hash))
}

/// Reads every source for `anchor`, applies `changes`, and assembles them.
pub(super) fn parse_configuration_with(
    anchor: &Path,
    changes: &[SourceChange],
) -> Result<ParsedConfiguration, MmError> {
    let anchor_source = match changes.iter().find(|(path, _)| path == anchor) {
        Some((_, Some(text))) => text.clone(),
        Some((_, None)) => {
            return Err(MmError::State {
                message: format!("{CONFIG_RELATIVE_PATH} cannot be removed"),
            });
        }
        None => read_config_text(anchor)?.ok_or_else(|| MmError::ConfigRead {
            path: anchor.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "configuration disappeared while it was being read",
            ),
        })?,
    };
    let override_path = anchor.with_file_name("mm.overrides.yaml");
    let (mut split, unknown_files) = read_split_sources(anchor)?;
    for (path, change) in changes {
        if path == anchor || *path == override_path {
            continue;
        }
        let key = split_key_for_path(anchor, path)?;
        split.retain(|source| source.key != key);
        if let Some(text) = change {
            split.push(SplitSource {
                key,
                path: path.clone(),
                text: text.clone(),
            });
        }
    }
    split.sort_by(|left, right| left.key.cmp(right.key));
    ensure_no_symlink_components(&override_path)?;
    let overrides = match changes.iter().find(|(path, _)| *path == override_path) {
        Some((_, change)) => change.clone().map(String::into_bytes),
        None => read_regular_file(&override_path, CONFIG_SOURCE_BYTES)?,
    };
    let total = anchor_source.len()
        + split.iter().map(|source| source.text.len()).sum::<usize>()
        + overrides.as_ref().map_or(0, Vec::len);
    if total > CONFIG_TOTAL_BYTES {
        return Err(MmError::ConfigParse {
            path: anchor.to_path_buf(),
            message: format!(
                "configuration sources total {total} bytes; the limit is {CONFIG_TOTAL_BYTES}"
            ),
        });
    }
    let mut parsed = assemble_configuration(anchor, &anchor_source, &split, overrides.as_deref())?;
    parsed
        .unknown_keys
        .extend(unknown_files.iter().map(|key| unknown_split_file(key)));
    Ok(parsed)
}

fn read_config_text(path: &Path) -> Result<Option<String>, MmError> {
    ensure_no_symlink_components(path)?;
    let Some(bytes) = read_regular_file(path, CONFIG_SOURCE_BYTES)? else {
        return Ok(None);
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| MmError::ConfigParse {
            path: path.to_path_buf(),
            message: "configuration must be valid UTF-8".to_owned(),
        })
}

fn split_key_for_path(anchor: &Path, path: &Path) -> Result<&'static str, MmError> {
    SPLITTABLE_CONFIG_KEYS
        .iter()
        .copied()
        .find(|key| split_config_path(anchor, key) == path)
        .ok_or_else(|| MmError::State {
            message: format!("{} is not a configuration source", path.display()),
        })
}

fn unexpected_split_file(path: &Path, reason: &str) -> MmError {
    MmError::ConfigParse {
        path: path.to_path_buf(),
        message: format!(
            "{reason}; files in {SPLIT_CONFIG_RELATIVE_DIRECTORY}/ must be named <key>.yaml for one of: {}",
            SPLITTABLE_CONFIG_KEYS.join(", ")
        ),
    }
}

/// The split files of `anchor`, and the names of those that hold a key this
/// release does not read.
fn read_split_sources(anchor: &Path) -> Result<(Vec<SplitSource>, Vec<String>), MmError> {
    let directory = anchor.with_file_name("mm");
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), Vec::new()));
        }
        Err(source) => {
            return Err(MmError::ConfigRead {
                path: directory,
                source,
            });
        }
    };
    if !metadata.is_dir() {
        return Err(MmError::ConfigParse {
            path: directory,
            message: format!(
                "{SPLIT_CONFIG_RELATIVE_DIRECTORY} must be a directory of split configuration files, not a file or symlink"
            ),
        });
    }
    ensure_no_symlink_components(&directory)?;
    let entries = fs::read_dir(&directory).map_err(|source| MmError::ConfigRead {
        path: directory.clone(),
        source,
    })?;
    let mut sources = Vec::new();
    let mut unknown = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| MmError::ConfigRead {
            path: directory.clone(),
            source,
        })?;
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            return Err(unexpected_split_file(&path, "file name is not valid UTF-8"));
        };
        // Editor swap files, `.DS_Store`, backups, and atomic-write temporaries.
        if name.starts_with('.') || name.ends_with('~') {
            continue;
        }
        let file_type = entry.file_type().map_err(|source| MmError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        if file_type.is_symlink() {
            return Err(unexpected_split_file(&path, "split configuration files cannot be symlinks"));
        }
        if !file_type.is_file() {
            return Err(unexpected_split_file(&path, "unexpected entry"));
        }
        let Some(key) = name.strip_suffix(".yaml") else {
            let reason = if name.ends_with(".yml") {
                "use the .yaml extension"
            } else {
                "unexpected file"
            };
            return Err(unexpected_split_file(&path, reason));
        };
        if key == "schema-version" {
            return Err(unexpected_split_file(
                &path,
                "schema-version must stay in .config/mm.yaml",
            ));
        }
        let Some(key) = SPLITTABLE_CONFIG_KEYS.iter().copied().find(|known| *known == key) else {
            unknown.push(key.to_owned());
            continue;
        };
        let text = read_config_text(&path)?.ok_or_else(|| MmError::ConfigRead {
            path: path.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "split configuration file disappeared while it was being read",
            ),
        })?;
        sources.push(SplitSource { key, path, text });
    }
    sources.sort_by(|left, right| left.key.cmp(right.key));
    unknown.sort();
    Ok((sources, unknown))
}

pub(super) fn has_yaml_content(source: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#') && line != "---" && line != "..."
    })
}

fn assemble_configuration(
    anchor: &Path,
    anchor_source: &str,
    split: &[SplitSource],
    overrides: Option<&[u8]>,
) -> Result<ParsedConfiguration, MmError> {
    let mut sources = std::collections::BTreeMap::new();
    if split.is_empty() {
        let keys = top_level_keys(anchor, anchor_source)?;
        refuse_local_development(anchor, &keys)?;
        let (config, source_hash, unknown_keys) =
            parse_single_file(anchor, anchor_source, overrides)?;
        for key in keys {
            sources.insert(key, anchor.to_path_buf());
        }
        return Ok(ParsedConfiguration {
            config,
            source_hash,
            sources,
            unknown_keys,
        });
    }

    let mut assembled: Value =
        serde_yaml_ng::from_str(anchor_source).map_err(|error| MmError::ConfigParse {
            path: anchor.to_path_buf(),
            message: error.to_string(),
        })?;
    refuse_local_development(anchor, &top_level_keys(anchor, anchor_source)?)?;
    serde_yaml_ng::from_str::<AnchorCheck>(anchor_source).map_err(|error| {
        MmError::ConfigParse {
            path: anchor.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    let root = assembled.as_mapping_mut().ok_or_else(|| MmError::ConfigParse {
        path: anchor.to_path_buf(),
        message: "configuration root must be a mapping".to_owned(),
    })?;
    for key in root.keys().filter_map(Value::as_str) {
        sources.insert(key.to_owned(), anchor.to_path_buf());
    }
    for source in split {
        if root.contains_key(source.key) {
            return Err(MmError::ConfigParse {
                path: source.path.clone(),
                message: format!(
                    "'{}' is set in both {CONFIG_RELATIVE_PATH} and {SPLIT_CONFIG_RELATIVE_DIRECTORY}/{}.yaml; keep one",
                    source.key, source.key
                ),
            });
        }
        if !has_yaml_content(&source.text) {
            return Err(MmError::ConfigParse {
                path: source.path.clone(),
                message: "split configuration file is empty; write {} for an empty mapping or delete the file".to_owned(),
            });
        }
        let value: Value =
            serde_yaml_ng::from_str(&source.text).map_err(|error| MmError::ConfigParse {
                path: source.path.clone(),
                message: error.to_string(),
            })?;
        check_split_source(source.key, &source.text).map_err(|message| {
            MmError::ConfigParse {
                path: source.path.clone(),
                message,
            }
        })?;
        root.insert(Value::String(source.key.to_owned()), value);
        sources.insert(source.key.to_owned(), source.path.clone());
    }

    let (config, unknown_keys) = read_assembled(anchor, assembled, split, overrides)?;
    Ok(ParsedConfiguration {
        config,
        source_hash: split_sources_hash(anchor, anchor_source, split, overrides),
        sources,
        unknown_keys,
    })
}

/// Applies the local overrides to an assembled configuration and reads it.
fn read_assembled(
    anchor: &Path,
    mut assembled: Value,
    split: &[SplitSource],
    overrides: Option<&[u8]>,
) -> Result<(MmConfig, Vec<UnknownConfigKey>), MmError> {
    let override_path = anchor.with_file_name("mm.overrides.yaml");
    let mut local = crate::domain::development_units::LocalDevelopment::default();
    let mut overlay_source = None;
    let mut unknown_development = Vec::new();
    if let Some(bytes) = overrides {
        let (overlay, development, unknown) = parse_overlay(&override_path, bytes)?;
        merge_configuration(&mut assembled, overlay.clone());
        overlay_source = Some(overlay);
        local = development;
        unknown_development = unknown;
    }
    let (mut config, ignored) =
        deserialize_tracked::<MmConfig, _>(assembled).map_err(|(error, ignored)| {
            let error = with_unknown_keys(error.to_string(), &ignored);
            if overrides.is_some() {
                MmError::ConfigParse {
                    path: override_path.clone(),
                    message: format!(
                        "invalid configuration after applying local overrides: {error}"
                    ),
                }
            } else {
                MmError::ConfigParse {
                    path: anchor.to_path_buf(),
                    message: format!(
                        "invalid configuration after combining {SPLIT_CONFIG_RELATIVE_DIRECTORY}/: {error}"
                    ),
                }
            }
        })?;
    config.development.enabled = local.enabled;
    let mut unknown_keys = unknown_keys_in(&ignored, split, overlay_source.as_ref());
    unknown_keys.extend(unknown_development);
    Ok((config, unknown_keys))
}

/// The fingerprint of an anchor, its split files and the local overrides.
fn split_sources_hash(
    anchor: &Path,
    anchor_source: &str,
    split: &[SplitSource],
    overrides: Option<&[u8]>,
) -> String {
    let root = anchor
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(""));
    let mut inputs: Vec<(String, &[u8])> = split
        .iter()
        .map(|source| {
            let relative = source
                .path
                .strip_prefix(root)
                .unwrap_or(&source.path)
                .to_string_lossy()
                .into_owned();
            (relative, source.text.as_bytes())
        })
        .collect();
    inputs.push((CONFIG_RELATIVE_PATH.to_owned(), anchor_source.as_bytes()));
    if let Some(bytes) = overrides {
        inputs.push((OVERRIDES_RELATIVE_PATH.to_owned(), bytes));
    }
    inputs.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mm-config-sources-v1\0");
    for (relative, bytes) in inputs {
        hasher.update(&(relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    hasher.finalize().to_hex().to_string()
}

fn top_level_keys(path: &Path, source: &str) -> Result<Vec<String>, MmError> {
    let value: Value = serde_yaml_ng::from_str(source).map_err(|error| MmError::ConfigParse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(value
        .as_mapping()
        .map(|mapping| {
            mapping
                .keys()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default())
}

/// Lists split sources that git would ignore. A remote environment loads
/// configuration from a commit, so these would silently vanish there.
pub(super) fn ignored_split_sources(document: &ConfigDocument) -> Vec<PathBuf> {
    let split = document
        .split_sources()
        .map(|(_, path)| path.to_path_buf())
        .collect::<Vec<_>>();
    if split.is_empty() {
        return Vec::new();
    }
    git_ignored(document, &document.path.with_file_name("mm"), split).unwrap_or_default()
}

/// The `paths` in `directory` that git would ignore; `None` outside a
/// repository.
pub(super) fn git_ignored(
    document: &ConfigDocument,
    directory: &Path,
    paths: Vec<PathBuf>,
) -> Option<Vec<PathBuf>> {
    let repository = document
        .root
        .ancestors()
        .find(|directory| directory.join(".git").exists())?;
    // Highest precedence first: the closest .gitignore wins, then
    // .git/info/exclude, then the user's global excludes file.
    let mut sources = Vec::new();
    for directory in directory.ancestors() {
        sources.push((directory.to_path_buf(), directory.join(".gitignore")));
        if directory == repository {
            break;
        }
    }
    sources.push((repository.to_path_buf(), repository.join(".git/info/exclude")));
    if let Some(global) = ignore::gitignore::gitconfig_excludes_path() {
        sources.push((repository.to_path_buf(), global));
    }
    let matchers = sources
        .into_iter()
        .filter(|(_, file)| file.is_file())
        .filter_map(|(base, file)| {
            let mut builder = ignore::gitignore::GitignoreBuilder::new(base);
            builder.add(file);
            builder.build().ok()
        })
        .collect::<Vec<_>>();
    let ignored = paths
        .into_iter()
        .filter(|path| {
            matchers.iter().find_map(|matcher| {
                let matched = matcher.matched_path_or_any_parents(path, false);
                if matched.is_ignore() {
                    Some(true)
                } else if matched.is_whitelist() {
                    Some(false)
                } else {
                    None
                }
            }) == Some(true)
        })
        .collect();
    Some(ignored)
}

#[cfg(test)]
#[path = "sources/tests.rs"]
mod source_tests;
