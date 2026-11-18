//! A launchctl stand-in for adapter tests: tracks loaded labels and records calls.
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use zeroize::Zeroizing;

use super::LAUNCHCTL;
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};
use crate::domain::error::MmError;

/// launchd stand-in: tracks which labels are loaded and records each call.
#[derive(Default)]
pub(crate) struct FakeLaunchd {
    pub(crate) loaded: Mutex<Vec<String>>,
    pub(crate) calls: Mutex<Vec<Vec<String>>>,
    pub(crate) fail_bootstrap: bool,
    pub(crate) print_stdout: Option<&'static str>,
    /// Exit status for any non-launchctl program, such as `haproxy -c`.
    pub(crate) other_status: i32,
    /// Labels reported disabled; bootstrap of these fails as launchd does.
    pub(crate) disabled: Vec<&'static str>,
}

pub(crate) fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

#[async_trait]
impl ProcessRunner for FakeLaunchd {
    async fn run(&self, command: CommandSpec) -> Result<CommandOutput, MmError> {
        let record = command.redacted_record();
        let lock_error = || MmError::State {
            message: "fake launchd lock poisoned".to_owned(),
        };
        if record.program != LAUNCHCTL {
            let mut call = vec![record.program];
            call.extend(record.arguments);
            self.calls.lock().map_err(|_| lock_error())?.push(call);
            return Ok(output(self.other_status, "", "injected program failure"));
        }
        let arguments = record.arguments;
        self.calls
            .lock()
            .map_err(|_| lock_error())?
            .push(arguments.clone());
        let mut loaded = self.loaded.lock().map_err(|_| lock_error())?;
        let label = |service: &str| service.rsplit('/').next().unwrap_or_default().to_owned();
        Ok(match arguments[0].as_str() {
            "print" if loaded.contains(&label(&arguments[1])) => output(
                0,
                self.print_stdout
                    .unwrap_or("gui/501/x = {\n\tstate = running\n\tpid = 7\n}\n"),
                "",
            ),
            "print" => output(113, "", "Could not find service \"x\" in domain"),
            "bootstrap" if self.fail_bootstrap => output(5, "", "Input/output error"),
            "bootstrap"
                if self
                    .disabled
                    .iter()
                    .any(|label| arguments[2].ends_with(&format!("{label}.plist"))) =>
            {
                output(5, "", "Input/output error")
            }
            "bootstrap" => {
                let path = Path::new(&arguments[2]);
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_owned();
                if !path.is_file() || loaded.contains(&name) {
                    output(5, "", "Input/output error")
                } else {
                    loaded.push(name);
                    output(0, "", "")
                }
            }
            "bootout" => {
                let name = label(&arguments[1]);
                if loaded.contains(&name) {
                    loaded.retain(|entry| *entry != name);
                    output(0, "", "")
                } else {
                    output(3, "", "Boot-out failed: 3: No such process")
                }
            }
            "kickstart" => output(0, "", ""),
            "print-disabled" => output(
                0,
                &format!(
                    "disabled services = {{\n{}}}\n",
                    self.disabled
                        .iter()
                        .map(|label| format!("\t\t\"{label}\" => disabled\n"))
                        .collect::<String>()
                ),
                "",
            ),
            other => output(64, "", &format!("unexpected {other}")),
        })
    }
}

impl FakeLaunchd {
    pub(crate) fn verbs(&self) -> Vec<String> {
        self.calls
            .lock()
            .map(|calls| calls.iter().map(|call| call[0].clone()).collect())
            .unwrap_or_default()
    }
}
