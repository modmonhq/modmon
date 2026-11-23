use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::adapters::keys::{ManagedKeys, PrivateAgent};
use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_user_controlled_directory, read_regular_file,
};
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner, TargetBootstrapExecutor};
use crate::domain::error::MmError;
use crate::domain::target::{
    BootstrapCapability, BootstrapExecution, TargetBootstrapPlan, TargetTransport,
};

const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_HOST_KEYS: usize = 32;
/// ansible-core 2.14 and later refuse to start without a UTF-8 locale, and
/// macOS has no `C.UTF-8`.
const ANSIBLE_LOCALE: &str = if cfg!(target_os = "macos") {
    "en_US.UTF-8"
} else {
    "C.UTF-8"
};

#[derive(Clone)]
pub struct AnsibleTargetBootstrapExecutor {
    process: Arc<dyn ProcessRunner>,
    staging_root: PathBuf,
    ansible_playbook: String,
    ssh_keyscan: String,
    id: String,
    ssh_auth_sock: Option<PathBuf>,
    /// Opens this device's key when mm keeps its passphrase.
    keys: ManagedKeys,
}

impl std::fmt::Debug for AnsibleTargetBootstrapExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnsibleTargetBootstrapExecutor")
            .field("staging_root", &self.staging_root)
            .field("ansible_playbook", &self.ansible_playbook)
            .field("ssh_keyscan", &self.ssh_keyscan)
            .finish_non_exhaustive()
    }
}

impl AnsibleTargetBootstrapExecutor {
    pub fn new(process: Arc<dyn ProcessRunner>, staging_root: PathBuf) -> Result<Self, MmError> {
        Ok(Self {
            process,
            staging_root,
            ansible_playbook: fixed_executable(&[
                "/usr/bin/ansible-playbook",
                "/usr/local/bin/ansible-playbook",
            ])?,
            ssh_keyscan: fixed_executable(&["/usr/bin/ssh-keyscan"])?,
            id: fixed_executable(&["/usr/bin/id"])?,
            ssh_auth_sock: std::env::var_os("SSH_AUTH_SOCK")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            keys: ManagedKeys::none(),
        })
    }

    /// Lets the executor authenticate with a key mm keeps the passphrase
    /// of: cluster setup connects with this device's key.
    #[must_use]
    pub fn with_managed_keys(mut self, keys: ManagedKeys) -> Self {
        self.keys = keys;
        self
    }

    #[cfg(test)]
    fn with_executables(
        process: Arc<dyn ProcessRunner>,
        staging_root: PathBuf,
        ansible_playbook: impl Into<String>,
        ssh_keyscan: impl Into<String>,
        ssh_auth_sock: Option<PathBuf>,
    ) -> Self {
        Self {
            process,
            staging_root,
            ansible_playbook: ansible_playbook.into(),
            ssh_keyscan: ssh_keyscan.into(),
            id: "id".to_owned(),
            ssh_auth_sock,
            keys: ManagedKeys::none(),
        }
    }

    async fn local_user(&self) -> Result<String, MmError> {
        let output = self
            .process
            .run(
                CommandSpec::new(&self.id)
                    .arg("-un")
                    .environment("LC_ALL", "C")
                    .timeout(Duration::from_secs(5)),
            )
            .await?;
        require_success(&self.id, &output)?;
        let user = output.stdout.trim();
        if user.is_empty()
            || user.len() > 32
            || !user
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(MmError::Security {
                message: "local account name is unsafe for bootstrap".to_owned(),
            });
        }
        Ok(user.to_owned())
    }

    async fn known_hosts(
        &self,
        host: &str,
        port: u16,
        expected_fingerprint: &str,
    ) -> Result<String, MmError> {
        pinned_known_hosts(
            self.process.as_ref(),
            &self.ssh_keyscan,
            host,
            port,
            expected_fingerprint,
        )
        .await
    }

    fn require_ssh_auth_socket(&self) -> Result<String, MmError> {
        let path = self.ssh_auth_sock.as_ref().ok_or_else(|| MmError::Target {
            message:
                "SSH bootstrap requires SSH_AUTH_SOCK to reference a loaded authentication agent"
                    .to_owned(),
        })?;
        if !path.is_absolute() {
            return Err(MmError::Security {
                message: "SSH_AUTH_SOCK must be an absolute path".to_owned(),
            });
        }
        let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
            context: "reading SSH authentication socket metadata".to_owned(),
            source,
        })?;
        if !metadata.file_type().is_socket() {
            return Err(MmError::Security {
                message: "SSH_AUTH_SOCK must reference a Unix socket directly".to_owned(),
            });
        }
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| MmError::Security {
                message: "SSH_AUTH_SOCK is not valid UTF-8".to_owned(),
            })
    }
}

impl AnsibleTargetBootstrapExecutor {
    fn private_staging(&self) -> Result<(tempfile::TempDir, PathBuf), MmError> {
        let staging = tempfile::Builder::new()
            .prefix(".bootstrap-")
            .tempdir_in(&self.staging_root)
            .map_err(|source| MmError::Io {
                context: format!(
                    "creating bootstrap staging directory in {}",
                    self.staging_root.display()
                ),
                source,
            })?;
        let staging_path = fs::canonicalize(staging.path()).map_err(|source| MmError::Io {
            context: "resolving private bootstrap staging directory".to_owned(),
            source,
        })?;
        Ok((staging, staging_path))
    }

    async fn ssh_access(
        &self,
        plan: &TargetBootstrapPlan,
        staging_path: &Path,
    ) -> Result<(SshAccess, Option<PrivateAgent>), MmError> {
        // Ansible starts `ssh` itself, many times. A key mm keeps the
        // passphrase of goes into an agent of mm's own, which holds it for
        // as long as the playbooks run and ends with the bootstrap.
        if let TargetTransport::Ssh {
            host,
            port,
            host_key_fingerprint,
            identity_file,
            ..
        } = &plan.target.definition.transport
        {
            let private_agent = match identity_file {
                Some(identity_file) => self.keys.agent_for(identity_file).await?,
                None => None,
            };
            // A target that names its own key authenticates with that file, so
            // an agent is only passed through when one is usable (for a key
            // with a passphrase). Without a key file the agent is required.
            let auth_sock = if let Some(agent) = &private_agent {
                Some(agent.socket().to_str().map(str::to_owned).ok_or_else(|| {
                    MmError::Security {
                        message: "the private ssh-agent's socket path is not valid UTF-8"
                            .to_owned(),
                    }
                })?)
            } else if identity_file.is_some() {
                self.require_ssh_auth_socket().ok()
            } else {
                Some(self.require_ssh_auth_socket()?)
            };
            let known_hosts = self.known_hosts(host, *port, host_key_fingerprint).await?;
            let path = staging_path.join("known_hosts");
            write_private_file(&path, known_hosts.as_bytes())?;
            let access = SshAccess {
                known_hosts: Some(path),
                auth_sock,
            };
            return Ok((access, private_agent));
        }
        Ok((SshAccess::default(), None))
    }
}

#[derive(Default)]
struct SshAccess {
    known_hosts: Option<PathBuf>,
    auth_sock: Option<String>,
}

#[async_trait]
impl TargetBootstrapExecutor for AnsibleTargetBootstrapExecutor {
    async fn execute(
        &self,
        project_root: &Path,
        plan: &TargetBootstrapPlan,
    ) -> Result<BootstrapExecution, MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        prepare_staging_root(&self.staging_root)?;
        let (_staging, staging_path) = self.private_staging()?;
        snapshot_inputs(project_root, &staging_path, plan)?;

        let (ssh, private_agent) = self.ssh_access(plan, &staging_path).await?;
        let local_user = if matches!(plan.target.definition.transport, TargetTransport::Local) {
            Some(self.local_user().await?)
        } else {
            None
        };
        let inventory = inventory(plan, ssh.known_hosts.as_deref(), local_user.as_deref())?;
        let inventory_path = staging_path.join("inventory.yml");
        write_private_file(&inventory_path, inventory.as_bytes())?;
        let local_temp = staging_path.join("local-temp");
        fs::create_dir(&local_temp).map_err(|source| MmError::Io {
            context: "creating Ansible local temporary directory".to_owned(),
            source,
        })?;
        set_private_directory_permissions(&local_temp)?;
        // Stage the access step before changing the host, so a binary that
        // changed since review stops setup before the first playbook runs.
        let access = plan
            .access
            .as_ref()
            .map(|access| access::stage(&staging_path, plan, access))
            .transpose()?;
        let playbook = staging_path.join(".config/ansible/bootstrap.yml");
        let output = self
            .process
            .run(self.playbook_command(
                &staging_path,
                &inventory_path,
                &local_temp,
                &playbook,
                None,
                ssh.auth_sock.as_deref(),
            ))
            .await?;
        require_playbook_success(&self.ansible_playbook, &output)?;
        let access = match access {
            Some(staged) => {
                let output = self
                    .process
                    .run(self.playbook_command(
                        &staging_path,
                        &inventory_path,
                        &local_temp,
                        &staged.playbook,
                        Some(&staged.variables),
                        ssh.auth_sock.as_deref(),
                    ))
                    .await?;
                require_playbook_success(&self.ansible_playbook, &output)?;
                Some(access::read_report(&staged.report)?)
            }
            None => None,
        };
        drop(private_agent);
        Ok(BootstrapExecution {
            capabilities: plan.required_capabilities.clone(),
            access,
        })
    }
}

impl AnsibleTargetBootstrapExecutor {
    /// One `ansible-playbook` run against the private inventory. The access
    /// step reuses the project's reviewed `ansible.cfg`.
    fn playbook_command(
        &self,
        staging_path: &Path,
        inventory_path: &Path,
        local_temp: &Path,
        playbook: &Path,
        extra_variables: Option<&Path>,
        ssh_auth_sock: Option<&str>,
    ) -> CommandSpec {
        let ansible_config = staging_path.join(".config/ansible/ansible.cfg");
        let mut command = CommandSpec::new(&self.ansible_playbook)
            .args(["--inventory"])
            .arg(inventory_path.to_string_lossy())
            .arg("--limit")
            .arg("mm_target");
        if let Some(variables) = extra_variables {
            command = command
                .arg("--extra-vars")
                .arg(format!("@{}", variables.to_string_lossy()));
        }
        command = command
            .arg(playbook.to_string_lossy())
            .current_dir(staging_path)
            .environment("ANSIBLE_CONFIG", ansible_config.to_string_lossy())
            // Deprecation notices on stderr would bury the task that failed.
            .environment("ANSIBLE_DEPRECATION_WARNINGS", "False")
            .environment("ANSIBLE_HOST_KEY_CHECKING", "True")
            .environment("ANSIBLE_LOCAL_TEMP", local_temp.to_string_lossy())
            .environment("HOME", staging_path.to_string_lossy())
            .environment("LC_ALL", ANSIBLE_LOCALE)
            .environment("PATH", "/usr/local/bin:/usr/bin:/bin")
            .environment("PYTHONNOUSERSITE", "1")
            .timeout(Duration::from_secs(30 * 60));
        if let Some(ssh_auth_sock) = ssh_auth_sock {
            command = command.environment("SSH_AUTH_SOCK", ssh_auth_sock);
        }
        if !control_socket_fits(staging_path) {
            // Ansible's default arguments minus connection sharing. Every
            // task then opens its own connection, so one that fails to
            // connect is tried again instead of ending the run.
            command = command
                .environment("ANSIBLE_SSH_ARGS", "-C")
                .environment("ANSIBLE_SSH_RETRIES", "3");
        }
        command
    }
}

/// Whether ssh can bind Ansible's control socket below `home`. Ansible puts
/// it at `<home>/.ansible/cp/<10 characters>`, ssh appends 17 more while it
/// binds, and a Unix socket path holds about 100 bytes (104 on macOS, 108 on
/// Linux). A longer path fails every task as "unreachable", so such a home
/// connects without connection sharing instead.
fn control_socket_fits(home: &Path) -> bool {
    const SOCKET_PATH_BYTES: usize = 100;
    const BELOW_HOME: usize = "/.ansible/cp/".len() + 10 + 17;
    home.as_os_str().len() + BELOW_HOME <= SOCKET_PATH_BYTES
}

#[derive(Serialize)]
struct Inventory {
    all: InventoryAll,
}

#[derive(Serialize)]
struct InventoryAll {
    children: BTreeMap<String, InventoryGroup>,
}

#[derive(Serialize)]
struct InventoryGroup {
    hosts: BTreeMap<String, BTreeMap<String, serde_yaml_ng::Value>>,
}

fn inventory(
    plan: &TargetBootstrapPlan,
    known_hosts: Option<&Path>,
    local_user: Option<&str>,
) -> Result<String, MmError> {
    let mut variables = BTreeMap::new();
    variables.insert(
        "mm_install_haproxy".to_owned(),
        serde_yaml_ng::Value::Bool(
            plan.required_capabilities
                .contains(&BootstrapCapability::Haproxy),
        ),
    );
    variables.insert(
        "mm_runtime".to_owned(),
        serde_yaml_ng::Value::String(
            match plan.selected_runtime {
                crate::domain::runtime::RuntimeKind::DockerCompose => "docker-compose",
                crate::domain::runtime::RuntimeKind::PodmanCompose => "podman-compose",
            }
            .to_owned(),
        ),
    );
    variables.insert(
        "mm_runtime_rootless".to_owned(),
        serde_yaml_ng::Value::Bool(plan.rootless),
    );
    connection_variables(
        &mut variables,
        &plan.target.definition.transport,
        known_hosts,
        local_user,
    )?;
    let inventory = Inventory {
        all: InventoryAll {
            children: BTreeMap::from([(
                "mm_targets".to_owned(),
                InventoryGroup {
                    hosts: BTreeMap::from([("mm_target".to_owned(), variables)]),
                },
            )]),
        },
    };
    serde_yaml_ng::to_string(&inventory).map_err(|error| MmError::Target {
        message: format!("serializing private Ansible inventory: {error}"),
    })
}

fn connection_variables(
    variables: &mut BTreeMap<String, serde_yaml_ng::Value>,
    transport: &TargetTransport,
    known_hosts: Option<&Path>,
    local_user: Option<&str>,
) -> Result<(), MmError> {
    match transport {
        TargetTransport::Local => {
            variables.insert(
                "ansible_connection".to_owned(),
                serde_yaml_ng::Value::String("local".to_owned()),
            );
            variables.insert(
                "ansible_user".to_owned(),
                serde_yaml_ng::Value::String(
                    local_user
                        .ok_or_else(|| MmError::Security {
                            message: "local bootstrap account is unavailable".to_owned(),
                        })?
                        .to_owned(),
                ),
            );
        }
        TargetTransport::Ssh {
            host,
            port,
            user,
            identity_file,
            ..
        } => {
            let known_hosts =
                known_hosts
                    .and_then(Path::to_str)
                    .ok_or_else(|| MmError::Security {
                        message: "SSH bootstrap requires a valid pinned host-key path".to_owned(),
                    })?;
            variables.insert(
                "ansible_host".to_owned(),
                serde_yaml_ng::Value::String(host.clone()),
            );
            variables.insert(
                "ansible_port".to_owned(),
                serde_yaml_ng::Value::Number((*port).into()),
            );
            variables.insert(
                "ansible_user".to_owned(),
                serde_yaml_ng::Value::String(user.clone()),
            );
            // The target's own key, when it names one, is the only key offered:
            // `-F /dev/null` ignores ~/.ssh/config, so nothing else selects it.
            let identity = match identity_file {
                Some(path) => {
                    let path = path.to_str().ok_or_else(|| MmError::Security {
                        message: "SSH bootstrap requires a valid identity file path".to_owned(),
                    })?;
                    variables.insert(
                        "ansible_ssh_private_key_file".to_owned(),
                        serde_yaml_ng::Value::String(path.to_owned()),
                    );
                    " -o IdentitiesOnly=yes"
                }
                None => "",
            };
            variables.insert(
                "ansible_ssh_common_args".to_owned(),
                serde_yaml_ng::Value::String(
                    format!(
                        "-F /dev/null -o UserKnownHostsFile={} -o GlobalKnownHostsFile=/dev/null -o StrictHostKeyChecking=yes -o BatchMode=yes -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no{identity}",
                        shell_quote(known_hosts)
                    ),
                ),
            );
        }
    }
    Ok(())
}

/// The known_hosts lines of the keys `host` presents whose fingerprint is
/// the reviewed one; refuses a host that presents none of them.
pub(super) async fn pinned_known_hosts(
    process: &dyn ProcessRunner,
    ssh_keyscan: &str,
    host: &str,
    port: u16,
    expected_fingerprint: &str,
) -> Result<String, MmError> {
    let port = port.to_string();
    let output = process
        .run(
            CommandSpec::new(ssh_keyscan)
                .args(["-T", "10", "-p", port.as_str(), host])
                .environment("LC_ALL", "C")
                .timeout(Duration::from_secs(15)),
        )
        .await?;
    require_success(ssh_keyscan, &output)?;
    if output.truncated {
        return Err(MmError::Target {
            message: "SSH host key scan exceeded the output safety limit".to_owned(),
        });
    }
    let mut matched = Vec::new();
    for line in output.stdout.lines().filter(|line| !line.starts_with('#')) {
        if line.is_empty() {
            continue;
        }
        if matched.len() >= MAX_HOST_KEYS || line.len() > 16 * 1024 || !line.is_ascii() {
            return Err(MmError::Security {
                message: "SSH host key scan returned unsafe output".to_owned(),
            });
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || !fields[1].contains("ssh") && !fields[1].starts_with("ecdsa-") {
            continue;
        }
        let key = base64::engine::general_purpose::STANDARD
            .decode(fields[2])
            .map_err(|_| MmError::Security {
                message: "SSH host key scan returned invalid key material".to_owned(),
            })?;
        let digest = Sha256::digest(key);
        let fingerprint = format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
        );
        if fingerprint == expected_fingerprint {
            matched.push(line.to_owned());
        }
    }
    if matched.is_empty() {
        return Err(MmError::Security {
            message: "SSH host key did not match the reviewed target fingerprint".to_owned(),
        });
    }
    matched.push(String::new());
    Ok(matched.join("\n"))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn snapshot_inputs(
    project_root: &Path,
    destination: &Path,
    plan: &TargetBootstrapPlan,
) -> Result<(), MmError> {
    for input in &plan.input_files {
        let source = project_root.join(&input.path);
        ensure_no_symlink_components(&source)?;
        let bytes = read_regular_file(&source, MAX_INPUT_BYTES)?.ok_or_else(|| MmError::State {
            message: "bootstrap input changed after plan review".to_owned(),
        })?;
        if blake3::hash(&bytes).to_hex().as_str() != input.digest {
            return Err(MmError::State {
                message: "bootstrap input changed after plan review".to_owned(),
            });
        }
        let target = destination.join(&input.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| MmError::Io {
                context: format!("creating bootstrap snapshot directory {}", parent.display()),
                source,
            })?;
            set_private_directory_permissions(parent)?;
        }
        write_private_file(&target, &bytes)?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("creating private bootstrap file {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing private bootstrap file {}", path.display()),
            source,
        })
}

/// Creates the private directory setup stages its files in, and every
/// missing directory above it, for the user alone. A parent made with the
/// usual mode (such as `MM_HOME/run`, which cluster setup used to create
/// with 0755) makes the one-host setup's operation lock under it refuse to
/// work.
pub fn prepare_staging_root(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path).map_err(|source| MmError::Io {
        context: format!("creating bootstrap staging root {}", path.display()),
        source,
    })?;
    set_private_directory_permissions(path)?;
    ensure_user_controlled_directory(path)
}

fn set_private_directory_permissions(path: &Path) -> Result<(), MmError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!("setting private permissions on {}", path.display()),
                source,
            }
        })?;
    }
    Ok(())
}

fn fixed_executable(candidates: &[&str]) -> Result<String, MmError> {
    candidates
        .iter()
        .find(|candidate| Path::new(candidate).is_file())
        .map(|candidate| (*candidate).to_owned())
        .ok_or_else(|| MmError::Target {
            message: format!(
                "required executable is unavailable: {}",
                candidates.join(" or ")
            ),
        })
}

/// Ansible reports failed tasks on stdout and warnings on stderr, so the
/// error carries the `fatal:` and `failed:` lines when there are any, or
/// else stderr, or else the end of stdout.
fn require_playbook_success(program: &str, output: &CommandOutput) -> Result<(), MmError> {
    if output.success() {
        return Ok(());
    }
    let lines = output
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let failures = lines
        .iter()
        .filter(|line| line.starts_with("fatal:") || line.starts_with("failed:"))
        .copied()
        .collect::<Vec<_>>();
    if failures.is_empty() && !output.stderr.trim().is_empty() {
        return require_success(program, output);
    }
    let detail = if failures.is_empty() {
        lines[lines.len().saturating_sub(20)..].to_vec()
    } else {
        failures
    };
    Err(MmError::ProcessFailed {
        program: program.to_owned(),
        code: output.code,
        stderr: detail
            .iter()
            .map(|line| line.chars().take(2000).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

fn require_success(program: &str, output: &CommandOutput) -> Result<(), MmError> {
    if output.success() {
        Ok(())
    } else {
        Err(MmError::ProcessFailed {
            program: program.to_owned(),
            code: output.code,
            stderr: output.stderr.trim().to_owned(),
        })
    }
}

#[cfg(test)]
#[path = "ansible_bootstrap/tests.rs"]
mod tests;

#[path = "ansible_bootstrap/access.rs"]
mod access;

#[cfg(test)]
#[path = "ansible_bootstrap/coverage_tests.rs"]
mod coverage_tests;
