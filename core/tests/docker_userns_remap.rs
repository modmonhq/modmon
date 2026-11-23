//! A Docker daemon with `userns-remap` gives its containers other user ids
//! than the host's, and the probe says so.

use std::sync::Arc;

use mm::adapters::runtime::DockerComposeRuntime;
use mm::app::ports::{CommandOutput, CommandSpec, ProcessRunner, RuntimeDriver};
use mm::domain::error::MmError;
use mm::domain::runtime::DOCKER_USERNS_REMAP;
use zeroize::Zeroizing;

/// Answers `docker info` with `security_options`, and every other probe
/// command as an engine that works.
struct Engine {
    security_options: &'static str,
}

#[async_trait::async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let command = specification.redacted_display();
        let stdout = if command.contains(" info ") {
            format!(r#"{{"SecurityOptions":{}}}"#, self.security_options)
        } else if command.contains("compose version") {
            "Docker Compose version v2.40.0\n".to_owned()
        } else {
            "Docker version 28.5.0\n".to_owned()
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
async fn the_docker_probe_says_when_the_daemon_remaps_user_ids() {
    for (security_options, remapped) in [
        (r#"["name=seccomp,profile=builtin","name=userns"]"#, true),
        (r#"["name=seccomp,profile=builtin","name=cgroupns"]"#, false),
        (r#"["name=rootless"]"#, false),
    ] {
        let probe = DockerComposeRuntime::new(Arc::new(Engine { security_options }))
            .probe()
            .await;
        assert!(probe.available, "{probe:?}");
        assert_eq!(
            probe.remaps_user_ids(),
            Some(remapped),
            "{security_options}"
        );
        assert_eq!(
            probe
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DOCKER_USERNS_REMAP),
            remapped
        );
    }
}

#[tokio::test]
async fn a_probe_that_cannot_read_docker_info_does_not_say_whether_ids_are_remapped() {
    let probe = DockerComposeRuntime::new(Arc::new(Engine {
        security_options: "not json",
    }))
    .probe()
    .await;
    assert_eq!(probe.remaps_user_ids(), None, "{probe:?}");
}
