use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

use crate::app::WorkspaceSelection;
use crate::app::config_validation::validate;
use crate::app::ports::{HostReleaseReader, ProjectDocumentStore, StateStore, TargetStore};
use crate::app::selection::{load_selection_preferences, resolve_selection};
use crate::domain::config::resolve_environment;
use crate::domain::diagnostic::Severity;
use crate::domain::error::MmError;
use crate::domain::host_policy::BreakGlassKey;
use crate::domain::path_policy::{
    ensure_no_symlink_components, ensure_user_controlled_directory, ensure_user_controlled_file,
    read_regular_file,
};
use crate::domain::target::{
    BootstrapAccess, BootstrapInputFile, NamedTarget, TargetBootstrap, TargetBootstrapPlan,
    TargetCatalogSnapshot, TargetDefinition, TargetId, TargetIngress, TargetTransport,
    bootstrap_requirements,
};

const MAX_BOOTSTRAP_INPUT_BYTES: usize = 1024 * 1024;
const MAX_BOOTSTRAP_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_BOOTSTRAP_INPUT_FILES: usize = 256;
const REQUIRED_BOOTSTRAP_INPUTS: [&str; 2] = [
    ".config/ansible/ansible.cfg",
    ".config/ansible/bootstrap.yml",
];
const MAX_HOST_BINARY_BYTES: u64 = 512 * 1024 * 1024;

/// The remote access layer an operator asks `mm target setup` to install.
#[derive(Debug, Clone)]
pub struct BootstrapAccessRequest {
    /// Git URL `mmd` fetches the project from. Empty when mm pushes the
    /// project's files to the host instead.
    pub origin: String,
    /// The git server's SSH host key, for SSH origins.
    pub origin_host_key: Option<String>,
    pub break_glass: Vec<BreakGlassKey>,
    /// Directory with `mm` and `mmd` built for the host.
    pub binaries: PathBuf,
    /// `--upgrade`: install over another build or an older release.
    pub upgrade: bool,
    /// `--downgrade`: install over a newer release.
    pub downgrade: bool,
}

#[derive(Debug, Clone)]
pub struct SshTargetInput {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub host_key_fingerprint: String,
    pub host_key: Option<String>,
    pub identity_file: Option<PathBuf>,
    pub remote_root: Option<PathBuf>,
    pub daemon_fingerprint: Option<String>,
}

impl SshTargetInput {
    fn into_transport(self) -> TargetTransport {
        TargetTransport::Ssh {
            host: self.host,
            port: self.port,
            user: self.user,
            host_key_fingerprint: self.host_key_fingerprint,
            host_key: self.host_key,
            identity_file: self.identity_file,
            remote_root: self.remote_root,
            daemon_fingerprint: self.daemon_fingerprint,
        }
    }
}

#[derive(Clone)]
pub struct TargetManager {
    documents: Arc<dyn ProjectDocumentStore>,
    targets: Arc<dyn TargetStore>,
    releases: Option<Arc<dyn HostReleaseReader>>,
}

impl std::fmt::Debug for TargetManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TargetManager")
            .finish_non_exhaustive()
    }
}

impl TargetManager {
    #[must_use]
    pub fn new(documents: Arc<dyn ProjectDocumentStore>, targets: Arc<dyn TargetStore>) -> Self {
        Self {
            documents,
            targets,
            releases: None,
        }
    }

    /// Reads the release a host's `mmd` reports, so a plan that installs
    /// the remote access layer says what it replaces.
    #[must_use]
    pub fn with_host_releases(mut self, releases: Arc<dyn HostReleaseReader>) -> Self {
        self.releases = Some(releases);
        self
    }

    /// [`Self::bootstrap_plan`], and when it installs `mm` and `mmd`, what
    /// it replaces on the host: an older release or another build only with
    /// `--upgrade` or a `deploy.mm-version` pin of this release, a newer
    /// one only with `--downgrade`, as cluster setup does on each node.
    pub async fn bootstrap_plan_on_host(
        &self,
        selection: &WorkspaceSelection,
        state: Option<&dyn StateStore>,
        access: Option<&BootstrapAccessRequest>,
    ) -> Result<TargetBootstrapPlan, MmError> {
        let mut plan = self.bootstrap_plan(selection, state, access)?;
        let (Some(releases), Some(installed)) = (&self.releases, plan.access.as_mut()) else {
            return Ok(plan);
        };
        let document = self.documents.load_from(&selection.start)?;
        let pin = crate::domain::version_pin::VersionPin::of(&document.config, &plan.environment);
        if let Some(pin) = &pin {
            pin.check_client(env!("CARGO_PKG_VERSION"))
                .map_err(|message| MmError::Environment { message })?;
        }
        let running = releases
            .host_release(&plan.target.definition.transport)
            .await?;
        let this = crate::domain::cluster_setup::HostRelease {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: crate::domain::remote::PROTOCOL_VERSION,
            build: Some(installed.mmd_digest.clone()),
        };
        let consent = crate::domain::cluster_setup::ReleaseConsent {
            downgrade: installed.downgrade,
            upgrade: installed.upgrade || pin.is_some(),
        };
        installed.notices = crate::domain::cluster_setup::release_notices(
            &[(plan.target.name.as_str(), running.as_ref())],
            &this,
            consent,
        )
        .map_err(|message| MmError::Target { message })?;
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Target { message })?;
        Ok(plan)
    }

    pub fn list(&self) -> Result<TargetCatalogSnapshot, MmError> {
        Ok(TargetCatalogSnapshot {
            targets: self.targets.list_targets()?,
        })
    }

    pub fn show(&self, name: &str) -> Result<NamedTarget, MmError> {
        self.targets.resolve_target(name)
    }

    pub fn remove(&self, name: &str, expected_fingerprint: &str) -> Result<NamedTarget, MmError> {
        self.targets.remove_target(name, expected_fingerprint)
    }

    pub fn add_ssh(&self, name: &str, input: SshTargetInput) -> Result<NamedTarget, MmError> {
        self.targets.insert_target(
            name,
            TargetDefinition {
                id: TargetId(Uuid::new_v4()),
                transport: input.into_transport(),
                bootstrap: TargetBootstrap::Ansible,
                ingress: None,
                default_runtime: None,
                available_runtimes: Vec::new(),
                runtime: None,
                nodes: Vec::new(),
            },
        )
    }

    pub fn update_ssh(
        &self,
        name: &str,
        expected_fingerprint: &str,
        input: SshTargetInput,
    ) -> Result<NamedTarget, MmError> {
        self.targets
            .update_target_transport(name, expected_fingerprint, input.into_transport())
    }

    pub fn configure_traefik(
        &self,
        name: &str,
        expected_fingerprint: &str,
        image: String,
        acme_email: Option<String>,
    ) -> Result<NamedTarget, MmError> {
        self.configure_traefik_acme(name, expected_fingerprint, image, acme_email, None, None)
    }

    /// Like `configure_traefik`, with an optional ACME directory and the CA
    /// bundle that signs that directory's HTTPS certificate.
    pub fn configure_traefik_acme(
        &self,
        name: &str,
        expected_fingerprint: &str,
        image: String,
        acme_email: Option<String>,
        acme_ca_server: Option<String>,
        acme_ca_certificate: Option<String>,
    ) -> Result<NamedTarget, MmError> {
        self.targets.update_target_ingress(
            name,
            expected_fingerprint,
            TargetIngress::Traefik {
                image,
                acme_email,
                acme_ca_server,
                acme_ca_certificate,
            },
        )
    }

    pub fn ensure_local(&self) -> Result<NamedTarget, MmError> {
        let target = self
            .targets
            .ensure_target("local", TargetDefinition::local(TargetId(Uuid::new_v4())))?;
        if target.definition.transport != TargetTransport::Local {
            return Err(MmError::Target {
                message: "target 'local' already exists but does not use the local transport"
                    .to_owned(),
            });
        }
        Ok(target)
    }

    /// Plans the bootstrap of the selected target. The target and runtime come
    /// from the same selection as every other command: CLI flags, then private
    /// preferences from `state`, then the environment, then the project.
    /// With `access`, the plan also installs the remote access layer.
    pub fn bootstrap_plan(
        &self,
        selection: &WorkspaceSelection,
        state: Option<&dyn StateStore>,
        access: Option<&BootstrapAccessRequest>,
    ) -> Result<TargetBootstrapPlan, MmError> {
        let config_path = self.documents.discover(&selection.start)?;
        let config_directory = config_path.parent().ok_or_else(|| MmError::Target {
            message: "project configuration has no parent directory".to_owned(),
        })?;
        let root = config_directory.parent().ok_or_else(|| MmError::Target {
            message: "project configuration has no project root".to_owned(),
        })?;
        ensure_user_controlled_directory(root)?;
        ensure_user_controlled_directory(config_directory)?;
        ensure_user_controlled_file(&config_path)?;
        let document = self.documents.load(&config_path)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let validation = validate(&document, &environment);
        let errors = validation
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count();
        if errors > 0 {
            return Err(MmError::invalid(&validation.diagnostics));
        }
        let (environment_pref, project_pref) =
            load_selection_preferences(state, &document, &environment)?;
        let (resolved, target) = resolve_selection(
            &document,
            &environment,
            selection.runtime,
            selection.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            self.targets.as_ref(),
        )?;
        let selected_runtime = resolved.runtime;
        let requirements = bootstrap_requirements(
            &target.definition.transport,
            selected_runtime,
            target.definition.ingress.is_some(),
            access.is_some(),
        )
        .map_err(|message| MmError::Target { message })?;
        let config_directory = document.root.join(".config");
        let ansible_directory = config_directory.join("ansible");
        for directory in [
            document.root.as_path(),
            config_directory.as_path(),
            ansible_directory.as_path(),
        ] {
            ensure_user_controlled_directory(directory)?;
        }
        let input_files = collect_bootstrap_inputs(&document.root)?;
        let access = access.map(reviewed_access).transpose()?;
        let mut plan = TargetBootstrapPlan {
            project_id: document.config.project.id,
            project_name: document.config.project.name,
            config_hash: document.source_hash,
            environment,
            target,
            selected_runtime,
            rootless: requirements.rootless,
            input_files,
            required_capabilities: requirements.capabilities,
            steps: requirements.steps,
            access,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Target { message })?;
        Ok(plan)
    }
}

/// Binds the request to the exact `mm` and `mmd` files setup will install.
fn reviewed_access(request: &BootstrapAccessRequest) -> Result<BootstrapAccess, MmError> {
    let binaries = fs::canonicalize(&request.binaries).map_err(|source| MmError::Io {
        context: format!(
            "resolving the host binaries directory {}",
            request.binaries.display()
        ),
        source,
    })?;
    let access = BootstrapAccess {
        origin: request.origin.clone(),
        origin_host_key: request.origin_host_key.clone(),
        break_glass: request.break_glass.clone(),
        mm_digest: host_binary_digest(&binaries.join("mm"))?,
        mmd_digest: host_binary_digest(&binaries.join("mmd"))?,
        architecture: crate::app::host_binaries::host_binaries_architecture(&binaries)?,
        binaries,
        upgrade: request.upgrade,
        downgrade: request.downgrade,
        notices: Vec::new(),
    };
    access
        .validate()
        .map_err(|message| MmError::Target { message })?;
    Ok(access)
}

/// BLAKE3 of a host binary, read without following a final symlink.
pub fn host_binary_digest(path: &std::path::Path) -> Result<String, MmError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!(
            "reading host binary {}; pass --host-binaries with mm and mmd built for the host",
            path.display()
        ),
        source,
    })?;
    if !metadata.file_type().is_file() {
        return Err(MmError::Target {
            message: format!(
                "host binary '{}' must be a regular file, not a link or directory",
                path.display()
            ),
        });
    }
    if metadata.len() > MAX_HOST_BINARY_BYTES {
        return Err(MmError::Target {
            message: format!(
                "host binary '{}' exceeds {MAX_HOST_BINARY_BYTES} bytes",
                path.display()
            ),
        });
    }
    let file = fs::File::open(path).map_err(|source| MmError::Io {
        context: format!("opening host binary {}", path.display()),
        source,
    })?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(
        &mut std::io::Read::take(file, MAX_HOST_BINARY_BYTES),
        &mut hasher,
    )
    .map_err(|source| MmError::Io {
        context: format!("hashing host binary {}", path.display()),
        source,
    })?;
    Ok(hasher.finalize().to_hex().to_string())
}

pub(crate) fn collect_bootstrap_inputs(
    root: &std::path::Path,
) -> Result<Vec<BootstrapInputFile>, MmError> {
    let files = discover_bootstrap_files(root)?;
    let relative_files = files
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .map(PathBuf::from)
                .map_err(|_| MmError::Security {
                    message: format!(
                        "Ansible bootstrap input '{}' escapes the project root",
                        path.display()
                    ),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let present = relative_files
        .iter()
        .filter_map(|path| path.to_str())
        .collect::<BTreeSet<_>>();
    for required in REQUIRED_BOOTSTRAP_INPUTS {
        if !present.contains(required) {
            return Err(MmError::Target {
                message: format!(
                    "bootstrap input '{required}' is missing; run `mm init` or restore the Ansible starter"
                ),
            });
        }
    }

    let mut total_bytes = 0_usize;
    files
        .into_iter()
        .zip(relative_files)
        .map(|(path, relative)| {
            ensure_user_controlled_file(&path)?;
            let bytes = read_regular_file(&path, MAX_BOOTSTRAP_INPUT_BYTES)?.ok_or_else(|| {
                MmError::Target {
                    message: format!("bootstrap input '{}' disappeared", relative.display()),
                }
            })?;
            total_bytes = checked_bootstrap_total(total_bytes, bytes.len())?;
            if relative.as_path() == std::path::Path::new(".config/ansible/ansible.cfg") {
                reject_external_ansible_config(&path, &bytes)?;
            } else if is_ansible_yaml_input(&relative) {
                reject_transitive_ansible_inputs(&path, &bytes)?;
            }
            Ok(BootstrapInputFile {
                path: relative,
                digest: blake3::hash(&bytes).to_hex().to_string(),
            })
        })
        .collect()
}

fn checked_bootstrap_total(total: usize, next: usize) -> Result<usize, MmError> {
    let total = total.checked_add(next).ok_or_else(|| MmError::Target {
        message: format!("Ansible bootstrap input tree exceeds {MAX_BOOTSTRAP_TOTAL_BYTES} bytes"),
    })?;
    if total > MAX_BOOTSTRAP_TOTAL_BYTES {
        return Err(MmError::Target {
            message: format!(
                "Ansible bootstrap input tree exceeds {MAX_BOOTSTRAP_TOTAL_BYTES} bytes"
            ),
        });
    }
    Ok(total)
}

fn discover_bootstrap_files(root: &std::path::Path) -> Result<Vec<PathBuf>, MmError> {
    let base = root.join(".config/ansible");
    if !base.is_dir() {
        return Err(MmError::Target {
            message: format!(
                "the project has no Ansible starter at {}; host setup runs its bootstrap.yml. Copy .config/ansible from a project made with `mm init`",
                base.display()
            ),
        });
    }
    let mut directories = vec![base.clone()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        ensure_user_controlled_directory(&directory)?;
        let mut entries = fs::read_dir(&directory)
            .map_err(|source| MmError::Io {
                context: format!(
                    "reading Ansible bootstrap directory {}",
                    directory.display()
                ),
                source,
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| MmError::Io {
                context: format!(
                    "reading Ansible bootstrap directory {}",
                    directory.display()
                ),
                source,
            })?;
        entries.sort_by_key(std::fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            ensure_no_symlink_components(&path)?;
            let file_type = entry.file_type().map_err(|source| MmError::Io {
                context: format!("inspecting Ansible bootstrap input {}", path.display()),
                source,
            })?;
            if file_type.is_symlink() {
                return Err(MmError::Security {
                    message: format!(
                        "Ansible bootstrap input '{}' must not be a symlink",
                        path.display()
                    ),
                });
            }
            if file_type.is_dir() {
                directories.push(path);
            } else if file_type.is_file() {
                if path
                    .file_name()
                    .is_some_and(|name| name == ".mm-initializing")
                {
                    return Err(MmError::Target {
                        message: "Ansible starter publication is incomplete; rerun `mm init` before planning"
                            .to_owned(),
                    });
                }
                files.push(path);
                if files.len() > MAX_BOOTSTRAP_INPUT_FILES {
                    return Err(MmError::Target {
                        message: format!(
                            "Ansible bootstrap input tree may contain at most {MAX_BOOTSTRAP_INPUT_FILES} files"
                        ),
                    });
                }
            } else {
                return Err(MmError::Security {
                    message: format!(
                        "Ansible bootstrap input '{}' is not a regular file or directory",
                        path.display()
                    ),
                });
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_ansible_yaml_input(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("json" | "yaml" | "yml")
    ) || path.components().any(|component| {
        let component = component.as_os_str();
        component == "group_vars" || component == "host_vars"
    })
}

fn reject_external_ansible_config(path: &std::path::Path, bytes: &[u8]) -> Result<(), MmError> {
    let source = std::str::from_utf8(bytes).map_err(|_| MmError::Target {
        message: format!("Ansible config '{}' must be valid UTF-8", path.display()),
    })?;
    for (index, line) in source.lines().enumerate() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(';')
            || line.starts_with('[')
        {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| MmError::Target {
            message: format!(
                "Ansible config '{}' line {} must be a key=value setting",
                path.display(),
                index + 1
            ),
        })?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        let valid = match key.as_str() {
            "host_key_checking" => value.eq_ignore_ascii_case("true"),
            "interpreter_python" => value == "auto_silent",
            "retry_files_enabled" => value.eq_ignore_ascii_case("false"),
            "pipelining" => matches!(value.to_ascii_lowercase().as_str(), "true" | "false"),
            _ => false,
        };
        if !valid {
            return Err(MmError::Target {
                message: format!(
                    "Ansible config '{}' setting '{}' is outside the closed bootstrap-plan input model",
                    path.display(),
                    key
                ),
            });
        }
    }
    Ok(())
}

fn reject_transitive_ansible_inputs(path: &std::path::Path, bytes: &[u8]) -> Result<(), MmError> {
    let mut document =
        serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(bytes).map_err(|error| {
            MmError::Target {
                message: format!(
                    "Ansible playbook '{}' is invalid YAML: {error}",
                    path.display()
                ),
            }
        })?;
    document.apply_merge().map_err(|error| MmError::Target {
        message: format!(
            "Ansible playbook '{}' has invalid YAML merge keys: {error}",
            path.display()
        ),
    })?;
    inspect_ansible_value(path, &document)
}

fn inspect_ansible_value(
    path: &std::path::Path,
    value: &serde_yaml_ng::Value,
) -> Result<(), MmError> {
    match value {
        serde_yaml_ng::Value::Mapping(mapping) => {
            inspect_ansible_mapping(path, mapping)?;
        }
        serde_yaml_ng::Value::Sequence(sequence) => {
            for value in sequence {
                inspect_ansible_value(path, value)?;
            }
        }
        serde_yaml_ng::Value::String(value) => {
            let lowercase = value.to_ascii_lowercase();
            let safe_expression = matches!(
                value.trim(),
                "{{ ansible_user }}"
                    | "{{ item.path }}"
                    | "{{ item.mode }}"
                    | "{{ ansible_distribution_release }}"
                    | "{{ {'x86_64':'amd64','aarch64':'arm64'}[ansible_architecture] }}"
                    | "ansible_distribution == 'Debian'"
                    | "ansible_distribution == 'Ubuntu'"
                    | "ansible_distribution in ['Debian', 'Ubuntu']"
            );
            if !safe_expression
                && (value.contains("{{")
                    || value.contains("{%")
                    || value.contains("{#")
                    || lowercase.contains("#jinja2")
                    || lowercase.contains("ansible_")
                    || lowercase.contains("proxycommand")
                    || lowercase.contains("private_key"))
            {
                return Err(MmError::Target {
                    message: format!(
                        "Ansible input '{}' uses Jinja templating, which is outside the reviewed bootstrap plan",
                        path.display()
                    ),
                });
            }
            let compact = value
                .chars()
                .filter(|character| !character.is_ascii_whitespace())
                .collect::<String>();
            if compact.contains("lookup")
                || compact.contains("query")
                || contains_identifier(&compact, "q")
            {
                return Err(MmError::Target {
                    message: format!(
                        "Ansible playbook '{}' uses a lookup that is outside the reviewed bootstrap plan",
                        path.display()
                    ),
                });
            }
        }
        serde_yaml_ng::Value::Tagged(_) => {
            return Err(MmError::Target {
                message: format!(
                    "Ansible input '{}' uses a YAML tag, which is outside the reviewed bootstrap plan",
                    path.display()
                ),
            });
        }
        _ => {}
    }
    Ok(())
}

fn inspect_ansible_mapping(
    path: &std::path::Path,
    mapping: &serde_yaml_ng::Mapping,
) -> Result<(), MmError> {
    for (key, value) in mapping {
        inspect_ansible_value(path, key)?;
        if let Some(key) = key.as_str() {
            let normalized = key.strip_prefix("ansible.builtin.").unwrap_or(key);
            if key.starts_with("with_")
                || normalized.starts_with("ansible_")
                || matches!(
                    normalized,
                    "import_playbook"
                        | "include"
                        | "include_tasks"
                        | "import_tasks"
                        | "include_role"
                        | "import_role"
                        | "include_vars"
                        | "roles"
                        | "vars_files"
                        | "collections"
                        | "action"
                        | "local_action"
                        | "delegate_to"
                        | "connection"
                        | "remote_user"
                        | "strategy"
                        | "vars_prompt"
                        | "template"
                        | "script"
                        | "assemble"
                        | "unarchive"
                        | "src"
                        | "private_key_file"
                        | "proxy_command"
                        | "ssh_args"
                )
                || (key.contains('.') && !key.starts_with("ansible.builtin."))
            {
                return Err(MmError::Target {
                    message: format!(
                        "Ansible playbook '{}' uses '{}', which can load an input outside the reviewed bootstrap plan",
                        path.display(),
                        key
                    ),
                });
            }
        }
        inspect_ansible_value(path, value)?;
    }
    Ok(())
}

fn contains_identifier(value: &str, identifier: &str) -> bool {
    value.match_indices(identifier).any(|(index, _)| {
        let before = value[..index].chars().next_back();
        let after = value[index + identifier.len()..].chars().next();
        let is_identifier = |character: Option<char>| {
            character.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        };
        !is_identifier(before) && !is_identifier(after)
    })
}

#[cfg(test)]
mod tests;
