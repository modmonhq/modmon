//! Reads the release a single host's `mmd` reports with the script cluster
//! setup reads each node's mesh facts with, over OpenSSH with the target's
//! reviewed host key and the account setup logs in with.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::ansible_bootstrap::pinned_known_hosts;
use super::cluster_shell::{private_tempdir, remote_command, require_success, write_private};
use crate::app::ports::{
    ClusterNodeCommand, ClusterScript, CommandSpec, HostReleaseReader, ProcessRunner,
};
use crate::domain::cluster_setup::HostRelease;
use crate::domain::error::MmError;
use crate::domain::mesh::MeshFacts;
use crate::domain::target::TargetTransport;

const READ_TIMEOUT: Duration = Duration::from_secs(60);

pub struct SshHostReleaseReader {
    process: Arc<dyn ProcessRunner>,
    ssh_auth_sock: Option<PathBuf>,
}

impl std::fmt::Debug for SshHostReleaseReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SshHostReleaseReader")
            .finish_non_exhaustive()
    }
}

impl SshHostReleaseReader {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self {
            process,
            ssh_auth_sock: std::env::var_os("SSH_AUTH_SOCK")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        }
    }
}

#[async_trait]
impl HostReleaseReader for SshHostReleaseReader {
    async fn host_release(
        &self,
        transport: &TargetTransport,
    ) -> Result<Option<HostRelease>, MmError> {
        let TargetTransport::Ssh {
            host,
            port,
            user,
            host_key_fingerprint,
            identity_file,
            ..
        } = transport
        else {
            return Ok(None);
        };
        let directory = private_tempdir()?;
        let known_hosts = directory.path().join("known_hosts");
        let lines = pinned_known_hosts(
            self.process.as_ref(),
            "ssh-keyscan",
            host,
            *port,
            host_key_fingerprint,
        )
        .await?;
        write_private(&known_hosts, lines.as_bytes())?;
        let mut specification = CommandSpec::new("ssh")
            .args(["-F", "/dev/null", "-o"])
            .arg(format!("UserKnownHostsFile={}", known_hosts.display()))
            .args_append([
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                "BatchMode=yes",
                "-o",
                "PasswordAuthentication=no",
                "-o",
                "KbdInteractiveAuthentication=no",
                "-o",
                "ConnectTimeout=20",
            ]);
        if let Some(identity_file) = identity_file {
            specification = specification
                .args_append(["-o", "IdentitiesOnly=yes", "-i"])
                .arg(identity_file.to_string_lossy());
        }
        if let Some(socket) = &self.ssh_auth_sock {
            specification = specification.environment("SSH_AUTH_SOCK", socket.to_string_lossy());
        }
        let output = self
            .process
            .run(
                specification
                    .arg("-p")
                    .arg(port.to_string())
                    .arg("-l")
                    .arg(user.clone())
                    .arg(host.clone())
                    .arg("--")
                    .arg(remote_command(
                        user,
                        &ClusterNodeCommand::Script {
                            script: ClusterScript::MeshFacts,
                            arguments: Vec::new(),
                        },
                    ))
                    .environment("LC_ALL", "C")
                    .timeout(READ_TIMEOUT),
            )
            .await?;
        require_success(&format!("ssh {host}"), &output)?;
        let facts: MeshFacts =
            serde_json::from_str(output.stdout.trim()).map_err(|error| MmError::Target {
                message: format!(
                    "{host} did not say which release of mm its mmd is ({error}); run `mm target setup plan` again, or check `/usr/libexec/mmd mesh show` on the host"
                ),
            })?;
        Ok(facts.release)
    }
}
