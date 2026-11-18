//! Runs cluster setup commands on nodes over OpenSSH with a pinned host key
//! and this device's mm key, never `~/.ssh/config` or an agent.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::adapters::keys::ManagedKeys;
use crate::app::ports::{
    ClusterNodeCommand, ClusterNodeConnection, ClusterNodeShell, ClusterScript, CommandOutput,
    CommandSpec, ProcessRunner,
};
use crate::domain::error::MmError;
use crate::domain::key::{PrivateKeyHeader, ed25519_private_key};

const PREPARE: &str = include_str!("cluster_shell/prepare.sh");
const MESH: &str = include_str!("cluster_shell/mesh.sh");
const MESH_FACTS: &str = include_str!("cluster_shell/mesh-facts.sh");
const REACH: &str = include_str!("cluster_shell/reach.sh");
const STORAGE: &str = include_str!("cluster_shell/storage.sh");
const STORAGE_FACTS: &str = include_str!("cluster_shell/storage-facts.sh");
const STORAGE_WITHDRAW: &str = include_str!("cluster_shell/storage-withdraw.sh");
const STORAGE_REACH: &str = include_str!("cluster_shell/storage-reach.sh");
const STORE_FACTS: &str = include_str!("cluster_shell/store-facts.sh");
const STORE_INSTALL: &str = include_str!("cluster_shell/store-install.sh");
/// Package installs on a fresh host can take minutes.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct SshClusterNodeShell {
    process: Arc<dyn ProcessRunner>,
    /// Opens this device's key when mm keeps its passphrase.
    keys: ManagedKeys,
}

impl std::fmt::Debug for SshClusterNodeShell {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SshClusterNodeShell")
            .finish_non_exhaustive()
    }
}

impl SshClusterNodeShell {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>, keys: ManagedKeys) -> Self {
        Self { process, keys }
    }
}

/// `'…'` quoting for a POSIX shell.
#[must_use]
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// The command line the remote login shell runs, raised to root with
/// `sudo -n` when the setup account is not root.
#[must_use]
pub fn remote_command(user: &str, command: &ClusterNodeCommand) -> String {
    let words = match command {
        ClusterNodeCommand::Script { script, arguments } => {
            let body = match script {
                ClusterScript::Prepare => PREPARE,
                ClusterScript::Mesh => MESH,
                ClusterScript::MeshFacts => MESH_FACTS,
                ClusterScript::Reach => REACH,
                ClusterScript::Storage => STORAGE,
                ClusterScript::StorageFacts => STORAGE_FACTS,
                ClusterScript::StorageWithdraw => STORAGE_WITHDRAW,
                ClusterScript::StorageReach => STORAGE_REACH,
                ClusterScript::StoreFacts => STORE_FACTS,
                ClusterScript::StoreInstall => STORE_INSTALL,
            };
            let mut words = vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                body.to_owned(),
                "mm-setup".to_owned(),
            ];
            words.extend(arguments.iter().cloned());
            words
        }
        ClusterNodeCommand::Program(argv) => argv.clone(),
    };
    let quoted = words
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ");
    if user == "root" {
        quoted
    } else {
        format!("sudo -n {quoted}")
    }
}

/// One known_hosts line for the node.
#[must_use]
pub fn known_hosts_line(host: &str, port: u16, host_key: &str) -> String {
    let pattern = if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    };
    format!("{pattern} {}\n", host_key.trim())
}

pub(super) fn private_tempdir() -> Result<tempfile::TempDir, MmError> {
    let directory = tempfile::Builder::new()
        .prefix("mm-cluster-")
        .tempdir()
        .map_err(|source| MmError::Io {
            context: "creating a private directory for cluster setup".to_owned(),
            source,
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).map_err(
            |source| MmError::Io {
                context: "securing the cluster setup directory".to_owned(),
                source,
            },
        )?;
    }
    Ok(directory)
}

pub(super) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|source| MmError::Io {
            context: format!("writing {}", path.display()),
            source,
        })
}

/// The node refused setup's key: which key to authorize there.
fn refused_key(node: &ClusterNodeConnection) -> MmError {
    let public = format!("{}.pub", node.identity_file.display());
    let line = fs::read_to_string(&public)
        .ok()
        .and_then(|text| text.lines().next().map(str::trim).map(str::to_owned))
        .filter(|line| !line.is_empty());
    MmError::Target {
        message: format!(
            "node '{}' ({}@{}) refused the key setup logs in with, {public} (Permission denied (publickey)). Add {} to ~{}/.ssh/authorized_keys on the host and run setup again. After `mm key rotate --finish` this is the new key: a host that lists only the old one refuses it",
            node.name,
            node.user,
            node.host,
            line.map_or_else(
                || "that public key".to_owned(),
                |line| format!("the line `{line}`")
            ),
            node.user
        ),
    }
}

pub(super) fn require_success(program: &str, output: &CommandOutput) -> Result<(), MmError> {
    if output.success() {
        return Ok(());
    }
    Err(MmError::ProcessFailed {
        program: program.to_owned(),
        code: output.code,
        stderr: output
            .stderr
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

#[async_trait]
impl ClusterNodeShell for SshClusterNodeShell {
    async fn scan_host_key(&self, host: &str, port: u16) -> Result<String, MmError> {
        let output = self
            .process
            .run(
                CommandSpec::new("ssh-keyscan")
                    .args(["-T", "10", "-t", "ed25519", "-p"])
                    .arg(port.to_string())
                    .arg(host)
                    .environment("LC_ALL", "C")
                    .timeout(Duration::from_secs(20)),
            )
            .await?;
        require_success("ssh-keyscan", &output)?;
        output
            .stdout
            .lines()
            .filter(|line| !line.starts_with('#'))
            .find_map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                (fields.len() == 3 && fields[1] == "ssh-ed25519")
                    .then(|| format!("{} {}", fields[1], fields[2]))
            })
            .ok_or_else(|| MmError::Target {
                message: format!("{host}:{port} presented no ed25519 SSH host key"),
            })
    }

    async fn run(
        &self,
        node: &ClusterNodeConnection,
        command: &ClusterNodeCommand,
        stdin: Option<Zeroizing<String>>,
    ) -> Result<Zeroizing<String>, MmError> {
        let directory = private_tempdir()?;
        let known_hosts = directory.path().join("known_hosts");
        write_private(
            &known_hosts,
            known_hosts_line(&node.host, node.port, &node.host_key).as_bytes(),
        )?;
        // With a key mm keeps the passphrase of, `ssh` asks for it once
        // and mm's helper answers; in batch mode it would not ask at all.
        let askpass = self.keys.askpass_for(&node.identity_file).await?;
        let mut specification = CommandSpec::new("ssh")
            .args(["-F", "/dev/null", "-o"])
            .arg(format!("UserKnownHostsFile={}", known_hosts.display()))
            .args_append([
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                if askpass.is_some() {
                    "BatchMode=no"
                } else {
                    "BatchMode=yes"
                },
                "-o",
                "NumberOfPasswordPrompts=1",
                "-o",
                "PasswordAuthentication=no",
                "-o",
                "KbdInteractiveAuthentication=no",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "ConnectTimeout=20",
                "-o",
                "ServerAliveInterval=15",
                "-i",
            ])
            .arg(node.identity_file.to_string_lossy())
            .arg("-p")
            .arg(node.port.to_string())
            .arg("-l")
            .arg(node.user.clone())
            .arg(node.host.clone())
            .arg("--")
            .arg(remote_command(&node.user, command))
            .environment("LC_ALL", "C")
            .timeout(COMMAND_TIMEOUT);
        for (name, value) in askpass.iter().flat_map(|askpass| askpass.environment()) {
            specification = specification.environment(name, value);
        }
        if let Some(stdin) = stdin {
            specification = specification.sensitive_stdin(stdin);
        }
        let output = self.process.run(specification).await?;
        drop(askpass);
        if output.code == Some(255) && output.stderr.contains("Permission denied (publickey") {
            return Err(refused_key(node));
        }
        require_success(&format!("ssh {}", node.name), &output)?;
        Ok(output.stdout)
    }

    /// The key never touches a disk here: an OpenSSH key file names its
    /// public half in the clear, and any other format goes to `ssh-keygen`
    /// on standard input.
    async fn public_key_of(&self, private_key: &[u8]) -> Result<String, MmError> {
        let text = std::str::from_utf8(private_key).map_err(|_| MmError::Security {
            message: "the deploy key is not a text private key".to_owned(),
        })?;
        if let Some(header) = PrivateKeyHeader::parse(text) {
            if header.encrypted {
                return Err(MmError::Security {
                    message: "the deploy key is encrypted with a passphrase, and a host cannot use it; store a key without one".to_owned(),
                });
            }
            return Ok(match &header.comment {
                Some(comment) if !comment.is_empty() => {
                    format!("{} {comment}", header.public_line())
                }
                _ => header.public_line(),
            });
        }
        let output = self
            .process
            .run(
                CommandSpec::new("ssh-keygen")
                    .args(["-y", "-P", "", "-f", "/dev/stdin"])
                    .sensitive_stdin(Zeroizing::new(text.to_owned()))
                    .environment("LC_ALL", "C")
                    .timeout(Duration::from_secs(10)),
            )
            .await?;
        require_success("ssh-keygen", &output)?;
        Ok(output.stdout.trim().to_owned())
    }

    fn seal_secret_bundle(
        &self,
        recipient: &str,
        bundle: &crate::domain::secret::SecretBundle,
    ) -> Result<Zeroizing<String>, MmError> {
        let sealed = crate::adapters::secrets::seal_secret_bundle(recipient, bundle)?;
        String::from_utf8(sealed)
            .map(Zeroizing::new)
            .map_err(|_| MmError::Security {
                message: "sealed secret bundle is not ASCII armored".to_owned(),
            })
    }

    /// Made in memory and returned: the private half is never a file on
    /// this machine. It goes to the secret store, and from there to the
    /// hosts.
    async fn generate_deploy_key(
        &self,
        comment: &str,
    ) -> Result<(Zeroizing<Vec<u8>>, String), MmError> {
        use ring::signature::KeyPair as _;

        let random = |error| MmError::Security {
            message: format!("the system's random source failed: {error}"),
        };
        let mut seed = Zeroizing::new([0_u8; 32]);
        getrandom::fill(seed.as_mut_slice()).map_err(random)?;
        let mut check = [0_u8; 4];
        getrandom::fill(&mut check).map_err(random)?;
        let pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(seed.as_slice()).map_err(
            |error| MmError::Security {
                message: format!("generating a deploy key failed: {error}"),
            },
        )?;
        let public: [u8; 32] =
            pair.public_key()
                .as_ref()
                .try_into()
                .map_err(|_| MmError::Security {
                    message: "generating a deploy key gave a public key of the wrong size"
                        .to_owned(),
                })?;
        let (private, public_line) =
            ed25519_private_key(&seed, &public, comment, u32::from_be_bytes(check));
        Ok((Zeroizing::new(private.as_bytes().to_vec()), public_line))
    }
}

#[cfg(test)]
#[path = "cluster_shell/tests.rs"]
mod tests;
