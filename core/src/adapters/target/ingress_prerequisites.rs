use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::adapters::private_fs::ensure_no_symlink_components;
use crate::app::ports::{CommandSpec, ProcessRunner};
use crate::domain::error::MmError;

/// Bootstrap only the host prerequisites for the fixed ingress integration.
/// DNS remains owned by workstation setup, not by the ingress on/off switch.
pub struct IngressPrerequisites {
    process: Arc<dyn ProcessRunner>,
}

impl IngressPrerequisites {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self { process }
    }

    pub async fn ensure(&self, allow_prompt: bool) -> Result<(), MmError> {
        if cfg!(target_os = "macos") {
            return self.ensure_macos().await;
        }
        if !cfg!(target_os = "linux") {
            return Err(MmError::Target {
                message: "public ingress installation requires Linux with systemd".to_owned(),
            });
        }
        let executable = std::env::current_exe().map_err(|source| MmError::Io {
            context: "locating the ingress helper build".to_owned(),
            source,
        })?;
        let helper = Path::new("/usr/local/libexec/mm/mm-ingress-helper");
        let source = executable.with_file_name("mm-ingress-helper");
        ensure_no_symlink_components(helper)?;
        ensure_no_symlink_components(&source)?;
        if !source.is_file() {
            return Err(MmError::Target {
                message: format!(
                    "mm-ingress-helper is missing beside {}; install mm again from a release (the installer and the release archive put it there), or run ./install.sh --no-setup in a checkout, then start ingress",
                    executable.display()
                ),
            });
        }
        let source = source.to_str().ok_or_else(|| MmError::Target {
            message: "the ingress helper path is not UTF-8".to_owned(),
        })?;
        self.ensure_packages(allow_prompt).await?;
        self.sudo(
            "/usr/bin/install",
            &[
                "-d",
                "-o",
                "root",
                "-g",
                "root",
                "-m",
                "755",
                "/usr/local/libexec/mm",
            ],
            allow_prompt,
        )
        .await?;
        self.sudo(
            "/usr/bin/install",
            &[
                "-o",
                "root",
                "-g",
                "root",
                "-m",
                "755",
                source,
                "/usr/local/libexec/mm/mm-ingress-helper.new",
            ],
            allow_prompt,
        )
        .await?;
        self.sudo(
            "/usr/bin/mv",
            &[
                "-f",
                "/usr/local/libexec/mm/mm-ingress-helper.new",
                "/usr/local/libexec/mm/mm-ingress-helper",
            ],
            allow_prompt,
        )
        .await
    }

    /// macOS needs no root for forwarding: Homebrew's HAProxy, installed as the
    /// current user, and Docker Desktop as the active endpoint.
    async fn ensure_macos(&self) -> Result<(), MmError> {
        use crate::adapters::macos_host::{
            homebrew_executable, homebrew_haproxy, probe_local_ingress_host,
        };
        use crate::domain::target_ingress::IngressHostProfile;

        if homebrew_haproxy().is_none() {
            let brew = homebrew_executable().ok_or_else(|| MmError::Target {
                message: "HAProxy is required; install Homebrew (https://brew.sh), then retry"
                    .to_owned(),
            })?;
            let brew = brew.to_str().ok_or_else(|| MmError::Target {
                message: "the Homebrew path is not UTF-8".to_owned(),
            })?;
            let output = self
                .process
                .run(
                    CommandSpec::new(brew)
                        .args(["install", "haproxy"])
                        .inherit_environment(&["PATH", "HOME", "USER", "LOGNAME", "TERM"])
                        .timeout(Duration::from_secs(900)),
                )
                .await?;
            if !output.success() {
                return Err(MmError::ProcessFailed {
                    program: "brew install haproxy".to_owned(),
                    code: output.code,
                    stderr: output.stderr.trim().to_owned(),
                });
            }
        }
        match probe_local_ingress_host() {
            IngressHostProfile::Unavailable { reason } => Err(MmError::Target { message: reason }),
            IngressHostProfile::LinuxSystemd | IngressHostProfile::MacosLaunchd(_) => Ok(()),
        }
    }

    async fn ensure_packages(&self, allow_prompt: bool) -> Result<(), MmError> {
        let mut packages = Vec::new();
        if !Path::new("/usr/sbin/haproxy").is_file() {
            packages.push("haproxy");
        }
        if Path::new("/sys/fs/selinux/enforce").exists()
            && !Path::new("/usr/bin/semanage").is_file()
            && !Path::new("/usr/sbin/semanage").is_file()
        {
            packages.push("policycoreutils-python-utils");
        }
        if packages.is_empty() {
            return Ok(());
        }
        let (program, mut arguments) = if Path::new("/usr/bin/dnf").is_file() {
            ("/usr/bin/dnf", vec!["install", "-y"])
        } else if Path::new("/usr/bin/apt-get").is_file() {
            ("/usr/bin/apt-get", vec!["install", "-y"])
        } else if Path::new("/usr/bin/pacman").is_file() {
            ("/usr/bin/pacman", vec!["-S", "--needed", "--noconfirm"])
        } else {
            return Err(MmError::Target {
                message: format!(
                    "install these packages, then retry ingress: {}",
                    packages.join(", ")
                ),
            });
        };
        arguments.extend(packages);
        self.sudo(program, &arguments, allow_prompt).await
    }

    async fn sudo(
        &self,
        program: &str,
        arguments: &[&str],
        allow_prompt: bool,
    ) -> Result<(), MmError> {
        let mut command = CommandSpec::new("/usr/bin/sudo");
        if !allow_prompt {
            command = command.arg("-n");
        }
        command = command
            .arg("--")
            .arg(program)
            .args_append(arguments.iter().copied())
            .inherit_environment(&["PATH", "HOME", "USER", "LOGNAME", "TERM"])
            .timeout(Duration::from_secs(300));
        if allow_prompt {
            command = command.inherit_stdin();
        }
        let output = self.process.run(command).await?;
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
}
