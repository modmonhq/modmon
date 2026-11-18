//! The config files of a stack's services, read and rendered.
//!
//! `mm config validate` and the compilation of a stack both resolve a
//! config file with `resolve_config_file`, so what validation accepts is
//! what a host can deploy. The compilation then gives each service a
//! read-only mount of the file's version on the host.

use std::path::{Component, Path, PathBuf};

use super::*;
use crate::domain::config::ServiceConfigFileConfig;
use crate::domain::config_file::{
    BindingProblem, ConfigFileUse, MAX_CONFIG_FILE_BYTES, TemplateProblem, binding_text, render,
    target_problem, version,
};
use crate::domain::stack::CompiledConfigFile;
use crate::domain::user_namespace::ServiceIds;

/// One entry under a service's `config-files`.
pub(crate) struct ConfigFileEntry<'a> {
    pub document: &'a ConfigDocument,
    pub environment: &'a str,
    pub module: &'a str,
    pub service: &'a str,
    pub target: &'a str,
    pub config: &'a ServiceConfigFileConfig,
}

/// A config file with its content as the environment's containers see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedConfigFile {
    pub service: String,
    pub target: String,
    /// The file or template, relative to the project.
    pub source: PathBuf,
    pub template: bool,
    pub bytes: Vec<u8>,
    pub mode: u32,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub version: String,
}

impl ConfigFileEntry<'_> {
    fn key(&self) -> String {
        format!(
            "modules.{}.services.{}.config-files.\"{}\"",
            self.module, self.service, self.target
        )
    }

    /// The source path under the project, and whether it is a template.
    fn source(&self) -> Result<(PathBuf, bool), String> {
        let (relative, template) = match (&self.config.file, &self.config.template) {
            (Some(file), None) => (file, false),
            (None, Some(template)) => (template, true),
            _ => {
                return Err(format!(
                    "{} needs exactly one source: `file: <path>` for a file shipped as it is, or `template: <path>` for one rendered from the environment's bindings",
                    self.key()
                ));
            }
        };
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(format!(
                "{}: '{}' has to be a path inside module '{}', relative to the module's directory and without `..`",
                self.key(),
                relative.display(),
                self.module
            ));
        }
        let module = self
            .document
            .config
            .modules
            .get(self.module)
            .map(|module| module.path.clone())
            .unwrap_or_default();
        Ok((module.join(relative), template))
    }

    fn owner(&self, name: &str, value: Option<&String>) -> Result<Option<u32>, String> {
        let Some(value) = value else {
            return Ok(None);
        };
        match value.parse::<u32>() {
            Ok(id) if value.bytes().all(|byte| byte.is_ascii_digit()) => Ok(Some(id)),
            _ => Err(format!(
                "{}: {name} '{value}' is not a numeric id such as \"101\"; a container's user names are not known on the host",
                self.key()
            )),
        }
    }

    fn rendered(&self, source: &Path, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
        let text = String::from_utf8(bytes).map_err(|_| {
            format!(
                "{}: template '{}' is not UTF-8 text; ship it with `file:` to keep its bytes as they are",
                self.key(),
                source.display()
            )
        })?;
        render(&text, |key| {
            binding_text(&self.document.config, self.environment, key)
        })
        .map(String::into_bytes)
        .map_err(|problem| self.template_problem(source, &problem))
    }

    fn template_problem(&self, source: &Path, problem: &TemplateProblem) -> String {
        let place = format!(
            "{}: '{}' line {}",
            self.key(),
            source.display(),
            problem.line
        );
        let key = problem.key.as_deref().unwrap_or_default();
        match &problem.problem {
            None => format!(
                "{place} has `{{{{` that does not start a placeholder. A placeholder is `{{{{ KEY }}}}` with a key from `bindings:`; a file that has to contain `{{{{` itself is shipped with `file:` instead of `template:`"
            ),
            Some(BindingProblem::Unknown) => format!(
                "{place} uses `{{{{ {key} }}}}`, and environment '{environment}' has no binding '{key}'. Add `{key}: {{value: ...}}` under `bindings:` or `environments.{environment}.bindings:`",
                environment = self.environment
            ),
            Some(BindingProblem::Secret(secret)) => format!(
                "{place} uses `{{{{ {key} }}}}`, which is the secret '{secret}'. A config file is kept on the host as it is and its content names its version, so a template never takes a secret. Mount the secret with `secret-files` on the service (`secret-files: {{{name}: {{secret: {secret}}}}}` puts it at /run/secrets/{name}) and have the configuration read that file",
                name = secret.replace('.', "-")
            ),
            Some(BindingProblem::Unresolved(reason)) => {
                format!("{place} uses `{{{{ {key} }}}}`, which has no value: {reason}")
            }
        }
    }
}

/// Reads one config file and renders it for the environment. The error is
/// the sentence a user reads: what is wrong, where, and what to write.
pub(crate) fn resolve_config_file(
    entry: &ConfigFileEntry<'_>,
) -> Result<ResolvedConfigFile, String> {
    if let Some(problem) = target_problem(entry.target) {
        return Err(format!("{}: the path {problem}", entry.key()));
    }
    let config = entry.config;
    if config.mode == 0 || config.mode & !0o777 != 0 {
        return Err(format!(
            "{}: mode {} is not a file mode. Write it as a YAML integer from 1 to 511, such as 292 for 0444 or 420 for 0644",
            entry.key(),
            config.mode
        ));
    }
    let uid = entry.owner("uid", config.uid.as_ref())?;
    let gid = entry.owner("gid", config.gid.as_ref())?;
    let (source, template) = entry.source()?;
    let path = entry.document.root.join(&source);
    let unreadable = |error: MmError| format!("{}: {error}", entry.key());
    ensure_no_symlink_components(&path).map_err(unreadable)?;
    let bytes = read_regular_file(&path, MAX_CONFIG_FILE_BYTES)
        .map_err(unreadable)?
        .ok_or_else(|| {
            format!(
                "{}: '{}' does not exist. The path is relative to the module's directory, and a remote host reads the file from the commit it deploys, so it has to be committed",
                entry.key(),
                source.display()
            )
        })?;
    let bytes = if template {
        entry.rendered(&source, bytes)?
    } else {
        bytes
    };
    Ok(ResolvedConfigFile {
        service: entry.service.to_owned(),
        target: entry.target.to_owned(),
        version: version(&bytes, config.mode, uid, gid),
        source,
        template,
        bytes,
        mode: config.mode,
        uid,
        gid,
    })
}

/// Every config file entry of the services that are part of `environment`,
/// on this node or another.
pub(crate) fn config_file_entries<'a>(
    document: &'a ConfigDocument,
    environment: &'a str,
) -> impl Iterator<Item = (&'a ServiceConfig, ConfigFileEntry<'a>)> {
    document
        .config
        .modules
        .iter()
        .flat_map(move |(module, config)| {
            config
                .services
                .iter()
                .filter(move |(_, service)| service.is_in_environment(environment))
                .flat_map(move |(service, config)| {
                    config
                        .config_files_in(environment)
                        .map(move |(target, file)| {
                            (
                                config,
                                ConfigFileEntry {
                                    document,
                                    environment,
                                    module,
                                    service,
                                    target,
                                    config: file,
                                },
                            )
                        })
                })
        })
}

/// The config files of one stack's services on this host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct StackConfigFiles {
    files: Vec<(ResolvedConfigFile, PathBuf)>,
}

impl StackConfigFiles {
    /// Each mount as `(service, file on the host, path in the container)`.
    pub(super) fn mounts(&self) -> impl Iterator<Item = (String, String, String)> + '_ {
        self.files.iter().map(|(file, path)| {
            (
                file.service.clone(),
                path.to_string_lossy().into_owned(),
                file.target.clone(),
            )
        })
    }

    /// What the plan says about them.
    pub(super) fn uses(&self, stack: &str) -> Vec<ConfigFileUse> {
        self.files
            .iter()
            .map(|(file, _)| ConfigFileUse {
                stack: stack.to_owned(),
                service: file.service.clone(),
                target: file.target.clone(),
                source: file.source.to_string_lossy().into_owned(),
                template: file.template,
                version: file.version.clone(),
                previous: None,
            })
            .collect()
    }

    /// The versions to put on the host, each once.
    pub(super) fn compiled(self) -> Vec<CompiledConfigFile> {
        let mut seen = BTreeSet::new();
        self.files
            .into_iter()
            .filter(|(file, _)| seen.insert(file.version.clone()))
            .map(|(file, path)| CompiledConfigFile {
                path,
                version: file.version,
                bytes: file.bytes,
                mode: file.mode,
                uid: file.uid,
                gid: file.gid,
            })
            .collect()
    }
}

impl ResolvedConfigFile {
    /// The file as the host keeps it for a service with user ids of its
    /// own: given to the host ids that its owner in the container, root
    /// unless `uid` and `gid` say otherwise, is there.
    fn given_to(mut self, ids: ServiceIds) -> Result<Self, String> {
        let host = |id: Option<u32>, name: &str| {
            let id = id.unwrap_or(0);
            ids.host_id(id).ok_or_else(|| {
                format!(
                    "config file {} of service '{}': {name} {id} is none of the ids its containers have on Podman, 0 to {}",
                    self.target,
                    self.service,
                    crate::domain::user_namespace::SERVICE_IDS - 1
                )
            })
        };
        let (uid, gid) = (host(self.uid, "uid")?, host(self.gid, "gid")?);
        self.uid = Some(uid);
        self.gid = Some(gid);
        self.version = version(&self.bytes, self.mode, self.uid, self.gid);
        Ok(self)
    }
}

/// The config files of the services of `stack` that this host runs, with
/// the place under `root` where each version is kept. A service with user
/// ids of its own gets its files owned by those ids.
pub(super) fn stack_config_files(
    document: &ConfigDocument,
    environment: &str,
    stack: &str,
    services: &[StackServiceManifest],
    root: &Path,
    user_ids: &super::user_namespace::StackUserIds,
) -> Result<StackConfigFiles, MmError> {
    let mut files = Vec::new();
    for (service, entry) in config_file_entries(document, environment) {
        if service.stack != stack
            || !service.is_active(environment)
            || !services
                .iter()
                .any(|managed| managed.service == entry.service)
        {
            continue;
        }
        let file = resolve_config_file(&entry)
            .and_then(|file| match user_ids.get(&file.service) {
                Some(ids) => file.given_to(ids),
                None => Ok(file),
            })
            .map_err(|message| MmError::Environment { message })?;
        let path = root.join(&file.version);
        files.push((file, path));
    }
    Ok(StackConfigFiles { files })
}
