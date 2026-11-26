use std::sync::Arc;
use std::time::Duration;

use crate::app::ports::{CommandSpec, ProcessRunner, PublicIngressControl};
use crate::domain::error::MmError;
use crate::domain::host_ingress::{PublicIngressState, PublicIngressStatus};

pub(crate) const FORWARDER_UNIT: &str = "mm-ingress-forwarder.service";
pub(crate) const FORWARDER_SHOW_ARGUMENTS: [&str; 5] = [
    "show",
    "--property=LoadState",
    "--property=ActiveState",
    "--property=UnitFileState",
    FORWARDER_UNIT,
];

pub(crate) fn forwarder_status_from_show(stdout: &str) -> PublicIngressStatus {
    let has = |expected: &str| stdout.lines().any(|line| line.trim_end() == expected);
    let installed = has("LoadState=loaded");
    PublicIngressStatus {
        installed,
        running: has("ActiveState=active"),
        failed: has("ActiveState=failed"),
        disabled: installed && has("UnitFileState=disabled"),
    }
}

/// Fixed system-service operations only. The CLI never elevates itself and
/// cannot substitute an arbitrary unit, executable, or systemctl argument.
pub struct SystemdPublicIngressControl {
    process: Arc<dyn ProcessRunner>,
}

impl SystemdPublicIngressControl {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self { process }
    }
}

#[async_trait::async_trait]
impl PublicIngressControl for SystemdPublicIngressControl {
    async fn inspect(&self) -> Result<PublicIngressStatus, MmError> {
        if !cfg!(target_os = "linux") {
            return Err(MmError::Target {
                message: "global public ingress control requires Linux with systemd".to_owned(),
            });
        }
        let output = self
            .process
            .run(CommandSpec::new("/usr/bin/systemctl").args(FORWARDER_SHOW_ARGUMENTS))
            .await?;
        if !output.success()
            && !output
                .stdout
                .lines()
                .any(|line| line == "LoadState=not-found")
        {
            return Err(MmError::ProcessFailed {
                program: "systemctl".to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            });
        }
        Ok(forwarder_status_from_show(&output.stdout))
    }

    async fn set_state(
        &self,
        state: PublicIngressState,
        allow_prompt: bool,
    ) -> Result<(), MmError> {
        if !cfg!(target_os = "linux") {
            return Err(MmError::Target {
                message: "global public ingress control requires Linux with systemd".to_owned(),
            });
        }
        let mut command = CommandSpec::new("/usr/bin/sudo");
        if !allow_prompt {
            command = command.arg("-n");
        }
        command = command
            .args_append([
                "--",
                "/usr/bin/systemctl",
                match state {
                    PublicIngressState::Running => "enable",
                    PublicIngressState::Stopped => "disable",
                },
                "--now",
                FORWARDER_UNIT,
            ])
            .inherit_environment(&["PATH", "HOME", "USER", "LOGNAME", "TERM"])
            .timeout(Duration::from_secs(120));
        if allow_prompt {
            command = command.inherit_stdin();
        }
        let output = self.process.run(command).await?;
        if output.success() {
            return Ok(());
        }
        Err(MmError::ProcessFailed {
            program: "systemctl".to_owned(),
            code: output.code,
            stderr: format!(
                "{}\nRun `sudo -v` in this terminal if authorization is needed. If the forwarder is not installed, run `./install.sh --setup-only` from the mm checkout.",
                output.stderr.trim()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ports::CommandOutput;
    use std::sync::Mutex;

    struct Recorder(Mutex<Vec<Vec<String>>>);

    #[async_trait::async_trait]
    impl ProcessRunner for Recorder {
        async fn run(&self, command: CommandSpec) -> Result<CommandOutput, MmError> {
            let record = command.redacted_record();
            assert_eq!(record.program, "/usr/bin/sudo");
            self.0
                .lock()
                .map_err(|_| MmError::State {
                    message: "test recorder poisoned".to_owned(),
                })?
                .push(record.arguments);
            Ok(CommandOutput {
                code: Some(0),
                stdout: String::new().into(),
                stderr: String::new().into(),
                truncated: false,
            })
        }
    }

    #[tokio::test]
    async fn commands_are_fixed_and_noninteractive_calls_cannot_prompt()
    -> Result<(), Box<dyn std::error::Error>> {
        if !cfg!(target_os = "linux") {
            return Ok(());
        }
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let control = SystemdPublicIngressControl::new(recorder.clone());
        control
            .set_state(PublicIngressState::Stopped, false)
            .await?;
        control.set_state(PublicIngressState::Running, true).await?;
        let commands = recorder.0.lock().map_err(|_| "recorder poisoned")?;
        assert_eq!(
            commands[0],
            [
                "-n",
                "--",
                "/usr/bin/systemctl",
                "disable",
                "--now",
                "mm-ingress-forwarder.service"
            ]
        );
        assert_eq!(
            commands[1],
            [
                "--",
                "/usr/bin/systemctl",
                "enable",
                "--now",
                "mm-ingress-forwarder.service"
            ]
        );
        Ok(())
    }
}
