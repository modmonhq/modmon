use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use atomic_write_file::AtomicWriteFile;
use fs4::FileExt;
use schemars::Schema;
use uuid::Uuid;
use yaml_edit::YamlFile;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_owned_by_current_user, ensure_user_controlled_directory,
    ensure_user_controlled_file, read_regular_file,
};
use crate::domain::config::{
    CONFIG_SCHEMA_VERSION, ConfigDocument, EnvironmentConfig, MmConfig, ProjectConfig,
    RuntimeConfig, SPLITTABLE_CONFIG_KEYS, StackConfig,
};
use crate::domain::environment_delivery::{EnvironmentDeliveryMode, EnvironmentDeliveryPolicy};
use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

pub const CONFIG_RELATIVE_PATH: &str = ".config/mm.yaml";
pub const ANSIBLE_CONFIG_RELATIVE_PATH: &str = ".config/ansible/ansible.cfg";
pub const ANSIBLE_PLAYBOOK_RELATIVE_PATH: &str = ".config/ansible/bootstrap.yml";
const ANSIBLE_CONFIG: &str = include_str!("../../../../templates/ansible/ansible.cfg");
const ANSIBLE_PLAYBOOK: &str = include_str!("../../../../templates/ansible/bootstrap.yml");
const INITIALIZING_MARKER: &str = ".mm-initializing";
const INITIALIZING_MARKER_CONTENT: &[u8] = b"mm Ansible starter publication is incomplete\n";

#[derive(Debug)]
pub struct InitializedConfigDocument {
    pub document: ConfigDocument,
    pub starter_files: Vec<PathBuf>,
    created_files: Vec<PathBuf>,
    created_directories: Vec<PathBuf>,
    gitignore: InitializationFileChange,
}

impl InitializedConfigDocument {
    pub fn rollback(&self) -> Result<(), MmError> {
        let ignore_result = rollback_gitignore(&self.gitignore);
        let paths_result = rollback_paths(&self.created_files, &self.created_directories);
        ignore_result.and(paths_result)
    }
}

impl crate::app::ports::ProjectInitialization for InitializedConfigDocument {
    fn document(&self) -> &ConfigDocument {
        &self.document
    }

    fn starter_files(&self) -> &[PathBuf] {
        &self.starter_files
    }

    fn rollback(&self) -> Result<(), MmError> {
        self.rollback()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigLoader;

impl ConfigLoader {
    pub fn discover(&self, start: &Path) -> Result<PathBuf, MmError> {
        let absolute_start = if start.is_absolute() {
            start.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|source| MmError::Io {
                    context: "resolving the current directory".to_owned(),
                    source,
                })?
                .join(start)
        };

        let start_directory = if absolute_start.is_file() {
            absolute_start
                .parent()
                .map_or_else(|| absolute_start.clone(), Path::to_path_buf)
        } else {
            absolute_start.clone()
        };

        for directory in start_directory.ancestors() {
            let candidate = directory.join(CONFIG_RELATIVE_PATH);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        Err(MmError::ConfigNotFound {
            start: absolute_start,
        })
    }

    pub fn load_from(&self, start: &Path) -> Result<ConfigDocument, MmError> {
        let path = self.discover(start)?;
        self.load(&path)
    }

    pub fn load(&self, path: &Path) -> Result<ConfigDocument, MmError> {
        let _phase = crate::app::timing::phase("config.load");
        ensure_no_symlink_components(path)?;
        let path = dunce::canonicalize(path).map_err(|source| MmError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let valid_location = path.file_name().is_some_and(|name| name == "mm.yaml")
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == ".config");
        if !valid_location {
            return Err(MmError::ConfigParse {
                path,
                message: format!("configuration must be located at {CONFIG_RELATIVE_PATH}"),
            });
        }

        let source = read_regular_file(&path, 1024 * 1024)?.ok_or_else(|| MmError::ConfigRead {
            path: path.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "configuration disappeared while it was being read",
            ),
        })?;
        let source = String::from_utf8(source).map_err(|_| MmError::ConfigParse {
            path: path.clone(),
            message: "configuration must be valid UTF-8".to_owned(),
        })?;
        let ParsedConfiguration {
            config,
            source_hash,
            sources,
        } = parse_configuration_with(&path, &[(path.clone(), Some(source))])?;
        if config.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(MmError::ConfigParse {
                path,
                message: format!(
                    "unsupported schema-version {}; expected schema-version: {CONFIG_SCHEMA_VERSION}",
                    config.schema_version
                ),
            });
        }
        let root_candidate =
            path.parent()
                .and_then(Path::parent)
                .ok_or_else(|| MmError::ConfigParse {
                    path: path.clone(),
                    message: format!("configuration must be located at {CONFIG_RELATIVE_PATH}"),
                })?;
        let root = dunce::canonicalize(root_candidate).map_err(|source| MmError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        if root.to_str().is_none() {
            return Err(MmError::ConfigParse {
                path,
                message: "project roots must be valid Unicode paths".to_owned(),
            });
        }

        let mut document = ConfigDocument {
            config,
            root,
            path,
            sources,
            source_hash,
        };
        crate::app::development_units::mark(&mut document);
        Ok(document)
    }

    pub fn initialize(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<InitializedConfigDocument, MmError> {
        self.initialize_with_ignore(root, requested_name, None)
    }

    pub fn initialize_reviewed(
        &self,
        root: &Path,
        requested_name: Option<&str>,
        plan_hash: &str,
    ) -> Result<InitializedConfigDocument, MmError> {
        use crate::app::ports::GuidedConfigEditor;
        let preview = self.preview_initialization(root, requested_name)?;
        let gitignore = preview.gitignore.clone();
        let config_hash = preview.config_hash.clone();
        let plan = crate::app::OnboardingService::initialization_plan(preview)?;
        if plan.plan_hash != plan_hash {
            return Err(MmError::State {
                message: "initialization plan changed; review the new plan".to_owned(),
            });
        }
        self.initialize_with_ignore(root, requested_name, Some((gitignore, config_hash)))
    }

    fn initialize_with_ignore(
        &self,
        root: &Path,
        requested_name: Option<&str>,
        reviewed: Option<(InitializationFileChange, String)>,
    ) -> Result<InitializedConfigDocument, MmError> {
        let root = dunce::canonicalize(root).map_err(|source| MmError::Io {
            context: format!("resolving project root {}", root.display()),
            source,
        })?;
        ensure_user_controlled_directory(&root)?;
        let path = root.join(CONFIG_RELATIVE_PATH);
        ensure_no_symlink_components(&path)?;
        if path.exists() {
            let marker = root.join(".config/ansible").join(INITIALIZING_MARKER);
            if marker.exists() {
                return recover_completed_initialization(self, &root, &path, &marker);
            }
            return Err(MmError::ConfigAlreadyExists { path });
        }
        let (gitignore, reviewed_hash) = match reviewed {
            Some((gitignore, hash)) => (gitignore, Some(hash)),
            None => (preview_gitignore(&root)?, None),
        };

        let config_directory = root.join(".config");
        let config = initial_config(&root, requested_name);
        let source = serde_yaml_ng::to_string(&config).map_err(|error| MmError::ConfigParse {
            path: path.clone(),
            message: error.to_string(),
        })?;

        let mut created_directories = Vec::new();
        if config_directory.exists() {
            ensure_user_controlled_directory(&config_directory)?;
        } else if let Err(source) = create_project_directory(&config_directory) {
            return Err(MmError::Io {
                context: format!("creating {}", config_directory.display()),
                source,
            });
        } else {
            created_directories.push(config_directory.to_path_buf());
            if let Err(error) = ensure_user_controlled_directory(&config_directory) {
                return Err(rollback_or(error, &[], &created_directories));
            }
        }

        let scaffold = match initialize_ansible_scaffold(&root) {
            Ok(scaffold) => scaffold,
            Err(error) => {
                return Err(rollback_or(error, &[], &created_directories));
            }
        };
        let AnsibleScaffold {
            mut starter_files,
            mut created_files,
            created_directories: scaffold_directories,
            marker,
        } = scaffold;
        created_directories.extend(scaffold_directories);
        if let Err(error) = publish_new_project_file(&path, source.as_bytes()) {
            return Err(rollback_or(error, &created_files, &created_directories));
        }
        created_files.push(path.clone());
        let document = match self.load(&path) {
            Ok(document) => document,
            Err(error) => return Err(rollback_or(error, &created_files, &created_directories)),
        };
        if reviewed_hash.is_some_and(|hash| hash != document.source_hash) {
            let error = MmError::State {
                message: "local overrides changed after initialization review".to_owned(),
            };
            return Err(rollback_or(error, &created_files, &created_directories));
        }
        if let Err(error) = apply_gitignore(&gitignore) {
            return Err(rollback_or(error, &created_files, &created_directories));
        }
        if gitignore.changed() {
            starter_files.push(gitignore.path.clone());
        }
        if let Err(error) = complete_initialization(&marker, &root) {
            return Err(rollback_initialized(
                error, &created_files, &created_directories, &gitignore,
            ));
        }

        Ok(InitializedConfigDocument {
            document,
            starter_files,
            created_files,
            created_directories,
            gitignore,
        })
    }

    pub fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<ConfigDocument, MmError> {
        self.edit_locked(start, "default-environment", &mut |document, source, split| {
            if let Some(environment) = environment
                && !document.config.environments.contains_key(environment)
            {
                return Err(MmError::Environment {
                    message: format!("environment '{environment}' is not configured"),
                });
            }
            let edited = if split {
                edit_split_scalar(environment)
            } else {
                edit_default_environment(source, environment)
            };
            edited.map_err(|message| MmError::ConfigParse {
                path: document.source("default-environment").to_path_buf(),
                message,
            })
        })
    }

    pub fn set_environment_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: EnvironmentDeliveryMode,
    ) -> Result<ConfigDocument, MmError> {
        self.edit_locked(start, "environments", &mut |document, source, split| {
            if !document.config.environments.contains_key(environment) {
                return Err(MmError::Environment {
                    message: format!("environment '{environment}' is not configured"),
                });
            }
            edit_environment_delivery(source, environment, mode, split).map_err(|message| {
                MmError::ConfigParse {
                    path: document.source("environments").to_path_buf(),
                    message,
                }
            })
        })
    }

    /// Runs one lossless edit against the file that owns top-level `key`,
    /// under the single configuration lock. `edit` receives the loaded
    /// document, the owning file's text, and whether that file is split out.
    fn edit_locked(
        &self,
        start: &Path,
        key: &str,
        edit: &mut dyn FnMut(&ConfigDocument, &str, bool) -> Result<String, MmError>,
    ) -> Result<ConfigDocument, MmError> {
        let path = self.discover(start)?;
        let lock = lock_configuration(&path)?;
        let document = self.load(&path)?;
        let target = document.source(key).to_path_buf();
        let split = target != document.path;
        let source = fs::read_to_string(&target).map_err(|source| MmError::ConfigRead {
            path: target.clone(),
            source,
        })?;
        let edited = edit(&document, &source, split)?;
        parse_configuration_with(&document.path, &[(target.clone(), Some(edited.clone()))])?;
        let mut file = AtomicWriteFile::options()
            .open(&target)
            .map_err(|source| MmError::Io {
                context: format!("opening {} for atomic update", target.display()),
                source,
            })?;
        file.write_all(edited.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!("writing {}", target.display()),
                source,
            })?;
        let current = fs::read_to_string(&target).map_err(|source| MmError::ConfigRead {
            path: target.clone(),
            source,
        })?;
        if current != source || self.load(&document.path)?.source_hash != document.source_hash {
            return Err(MmError::State {
                message: "configuration changed while it was being edited; retry the command"
                    .to_owned(),
            });
        }
        file.commit().map_err(|source| MmError::Io {
            context: format!("publishing {}", target.display()),
            source,
        })?;
        let updated = self.load(&document.path)?;
        FileExt::unlock(&lock).map_err(|source| MmError::Io {
            context: format!("unlocking configuration {}", document.path.display()),
            source,
        })?;
        Ok(updated)
    }

    pub fn set_development_mount(
        &self,
        start: &Path,
        module: &str,
        mount: Option<&crate::domain::development_mount::DevelopmentMountConfig>,
        expected_source_hash: &str,
    ) -> Result<ConfigDocument, MmError> {
        self.edit_locked(start, "modules", &mut |document, source, split| {
            if document.source_hash != expected_source_hash {
                return Err(MmError::State {
                    message:
                        "development mount plan is stale because configuration changed; review a new plan"
                            .to_owned(),
                });
            }
            if !document.config.modules.contains_key(module) {
                return Err(MmError::Environment {
                    message: format!("module '{module}' is not configured"),
                });
            }
            edit_development_mount(source, module, mount, split).map_err(|message| {
                MmError::ConfigParse {
                    path: document.source("modules").to_path_buf(),
                    message,
                }
            })
        })
    }

    #[must_use]
    pub fn schema(&self) -> Schema {
        let mut schema = schemars::schema_for!(MmConfig);
        schema.insert(
            "$comment".to_owned(),
            serde_json::Value::String(
                "Generated from MmConfig; run ./scripts/generate-artifacts.sh. Do not edit."
                    .to_owned(),
            ),
        );
        schema
    }
}

/// Replaces one configuration source atomically.
pub(super) fn write_config_atomically(path: &Path, text: &str) -> Result<(), MmError> {
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| MmError::Io {
            context: format!("opening {} for atomic update", path.display()),
            source,
        })?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing {}", path.display()),
        source,
    })
}

/// Takes the one lock that serializes every configuration edit, whichever
/// source file it writes.
pub(super) fn lock_configuration(anchor: &Path) -> Result<fs::File, MmError> {
    let lock_path = anchor.with_extension("yaml.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| MmError::Io {
            context: format!("opening configuration lock {}", lock_path.display()),
            source,
        })?;
    crate::adapters::file_lock::exclusive(&lock, &lock_path)?;
    Ok(lock)
}

impl ConfigLoader {
    /// The JSON Schema for one top-level key's value, as written in its split
    /// file `.config/mm/<key>.yaml`. It carries only the definitions it uses.
    #[must_use]
    pub fn key_schema(&self, key: &str) -> Option<Schema> {
        if !SPLITTABLE_CONFIG_KEYS.contains(&key) {
            return None;
        }
        let full = serde_json::to_value(self.schema()).ok()?;
        let property = full.get("properties")?.get(key)?.clone();
        let definitions = full
            .get("$defs")
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut used = serde_json::Map::new();
        let mut pending = Vec::new();
        collect_definition_refs(&property, &mut pending);
        while let Some(name) = pending.pop() {
            if used.contains_key(&name) {
                continue;
            }
            if let Some(definition) = definitions.get(&name) {
                collect_definition_refs(definition, &mut pending);
                used.insert(name, definition.clone());
            }
        }
        let mut schema = serde_json::Map::new();
        if let Some(version) = full.get("$schema") {
            schema.insert("$schema".to_owned(), version.clone());
        }
        schema.insert(
            "title".to_owned(),
            serde_json::Value::String(format!("mm configuration: {key}")),
        );
        schema.insert(
            "$comment".to_owned(),
            serde_json::Value::String(format!(
                "The value of top-level '{key}', for {SPLIT_CONFIG_RELATIVE_DIRECTORY}/{key}.yaml. Generated from MmConfig; run ./scripts/generate-artifacts.sh. Do not edit."
            )),
        );
        match property {
            serde_json::Value::Object(property) => schema.extend(property),
            other => {
                schema.insert("allOf".to_owned(), serde_json::Value::Array(vec![other]));
            }
        }
        if !used.is_empty() {
            schema.insert("$defs".to_owned(), serde_json::Value::Object(used));
        }
        Schema::try_from(serde_json::Value::Object(schema)).ok()
    }
}

fn collect_definition_refs(value: &serde_json::Value, names: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(name) = map
                .get("$ref")
                .and_then(serde_json::Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/$defs/"))
            {
                names.push(name.to_owned());
            }
            for child in map.values() {
                collect_definition_refs(child, names);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_definition_refs(child, names);
            }
        }
        _ => {}
    }
}

impl crate::app::ports::ProjectDocumentStore for ConfigLoader {
    fn discover(&self, start: &Path) -> Result<PathBuf, MmError> {
        Self::discover(self, start)
    }

    fn load_from(&self, start: &Path) -> Result<ConfigDocument, MmError> {
        Self::load_from(self, start)
    }

    fn load(&self, path: &Path) -> Result<ConfigDocument, MmError> {
        Self::load(self, path)
    }

    fn initialize(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<Box<dyn crate::app::ports::ProjectInitialization>, MmError> {
        Ok(Box::new(Self::initialize(self, root, requested_name)?))
    }

    fn schema(&self) -> schemars::Schema {
        Self::schema(self)
    }

    fn initialize_reviewed(
        &self,
        root: &Path,
        requested_name: Option<&str>,
        plan_hash: &str,
    ) -> Result<Box<dyn crate::app::ports::ProjectInitialization>, MmError> {
        Ok(Box::new(Self::initialize_reviewed(
            self, root, requested_name, plan_hash,
        )?))
    }

    fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<ConfigDocument, MmError> {
        Self::set_default_environment(self, start, environment)
    }

    fn set_environment_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: EnvironmentDeliveryMode,
    ) -> Result<ConfigDocument, MmError> {
        Self::set_environment_delivery(self, start, environment, mode)
    }

    fn set_development_mount(
        &self,
        start: &Path,
        module: &str,
        mount: Option<&crate::domain::development_mount::DevelopmentMountConfig>,
        expected_source_hash: &str,
    ) -> Result<ConfigDocument, MmError> {
        Self::set_development_mount(self, start, module, mount, expected_source_hash)
    }

    fn set_development_enabled(
        &self,
        start: &Path,
        enabled: &[String],
    ) -> Result<ConfigDocument, MmError> {
        Self::set_development_enabled(self, start, enabled)
    }

    fn ignored_sources(&self, document: &ConfigDocument) -> Vec<PathBuf> {
        ignored_split_sources(document)
    }
}

#[cfg(test)]
#[path = "document/tests.rs"]
mod tests;
