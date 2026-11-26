#![cfg(target_os = "linux")]

use std::sync::{Arc, Mutex};

use mm::adapters::target::SystemdPublicIngressControl;
use mm::app::ports::{CommandOutput, CommandSpec, ProcessRunner, PublicIngressControl};
use mm::domain::error::MmError;
use mm::domain::host_ingress::{PublicIngressState, PublicIngressStatus};
use zeroize::Zeroizing;

struct Systemd {
    show: &'static str,
    commands: Mutex<Vec<String>>,
}

impl Systemd {
    fn new(show: &'static str) -> Arc<Self> {
        Arc::new(Self {
            show,
            commands: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait::async_trait]
impl ProcessRunner for Systemd {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let command = specification.redacted_display();
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "recorder poisoned".to_owned(),
            })?
            .push(command.clone());
        let stdout = if command.contains(" show ") {
            self.show.to_owned()
        } else {
            String::new()
        };
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

#[tokio::test]
async fn a_disabled_inactive_forwarder_reads_as_kept_stopped() -> Result<(), MmError> {
    for (show, expected) in [
        (
            "LoadState=loaded\nActiveState=inactive\nUnitFileState=disabled\n",
            PublicIngressStatus {
                installed: true,
                running: false,
                failed: false,
                disabled: true,
            },
        ),
        (
            "LoadState=loaded\nActiveState=inactive\nUnitFileState=enabled\n",
            PublicIngressStatus {
                installed: true,
                running: false,
                failed: false,
                disabled: false,
            },
        ),
        (
            "LoadState=loaded\nActiveState=active\nUnitFileState=disabled\n",
            PublicIngressStatus {
                installed: true,
                running: true,
                failed: false,
                disabled: true,
            },
        ),
        (
            "LoadState=not-found\nActiveState=inactive\nUnitFileState=disabled\n",
            PublicIngressStatus::default(),
        ),
    ] {
        let status = SystemdPublicIngressControl::new(Systemd::new(show))
            .inspect()
            .await?;
        assert_eq!(status, expected, "{show}");
        assert_eq!(
            status.kept_stopped(),
            expected.disabled && !expected.running,
            "{show}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn stop_and_start_change_the_boot_state_with_the_running_state() -> Result<(), MmError> {
    let systemd = Systemd::new("");
    let control = SystemdPublicIngressControl::new(systemd.clone());
    control
        .set_state(PublicIngressState::Stopped, false)
        .await?;
    control
        .set_state(PublicIngressState::Running, false)
        .await?;
    let commands = systemd.commands.lock().map_err(|_| MmError::State {
        message: "recorder poisoned".to_owned(),
    })?;
    assert!(
        commands[0].ends_with("systemctl disable --now mm-ingress-forwarder.service"),
        "{commands:?}"
    );
    assert!(
        commands[1].ends_with("systemctl enable --now mm-ingress-forwarder.service"),
        "{commands:?}"
    );
    Ok(())
}
