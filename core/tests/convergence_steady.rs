//! A service without a health check is healthy only once its container has
//! stayed up a moment: a command that exits at once was still running when
//! the engine said it started, and a deployment of it used to succeed
//! (BUGS.md, "A service that exits at once did not fail a deployment").

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm::adapters::runtime::DockerComposeRuntime;
use mm::app::ports::{CommandOutput, CommandSpec, ProcessRunner, RuntimeDriver};
use mm::domain::error::MmError;
use mm::domain::runtime::{
    ConvergenceCancellation, ConvergencePolicy, ConvergenceState, RuntimeTarget,
};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;
use zeroize::Zeroizing;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Answers each engine command with the next output, in order.
struct Answers(Mutex<VecDeque<String>>);

#[async_trait::async_trait]
impl ProcessRunner for Answers {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let stdout = self
            .0
            .lock()
            .map_err(|_| MmError::State {
                message: "answers lock was poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "no answer is left".to_owned(),
            })?;
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-conformance")
}

fn target() -> RuntimeTarget {
    RuntimeTarget {
        root: root(),
        project_directory: root(),
        target: Some(NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(Uuid::from_u128(7))),
        )),
        project_id: ProjectId(Uuid::from_u128(8)),
        environment: "steady".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "mm-conformance-app".to_owned(),
        artifact_hash: "fixture-hash".to_owned(),
        compose_files: vec![root().join("app.compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

/// `docker compose ps` with one container of `api` in `state`, as Compose
/// prints it: no start time, and the status text.
fn api(state: &str, status: &str, exit_code: i32) -> String {
    format!(
        r#"[{{"ID":"abc","Name":"mm-conformance-app-api-1","Image":"busybox:1.36.1","State":"{state}","Status":"{status}","ExitCode":{exit_code},"Labels":{{"com.docker.compose.project":"mm-conformance-app","com.docker.compose.service":"api"}}}}]"#
    )
}

/// Each look renders the project and lists its containers.
fn looks(containers: &[String]) -> Result<Arc<Answers>, std::io::Error> {
    let rendered = std::fs::read_to_string(root().join("app.compose.yaml"))?;
    let mut answers = VecDeque::new();
    for listed in containers {
        answers.push_back(rendered.clone());
        answers.push_back(listed.clone());
    }
    Ok(Arc::new(Answers(Mutex::new(answers))))
}

fn policy() -> ConvergencePolicy {
    ConvergencePolicy {
        timeout: Duration::from_secs(30),
        initial_backoff: Duration::from_millis(5),
        maximum_backoff: Duration::from_millis(5),
    }
}

#[tokio::test]
async fn a_container_that_exits_just_after_it_started_fails_the_wait() -> TestResult {
    let answers = looks(&[
        api("running", "Up Less than a second", 0),
        api("exited", "Exited (1) 1 second ago", 1),
    ])?;
    let outcome = DockerComposeRuntime::new(answers)
        .wait_for_services(
            &target(),
            &["api".to_owned()],
            policy(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Failed);
    assert_eq!(outcome.service.as_deref(), Some("api"));
    assert_eq!(
        outcome.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_EXITED")
    );
    assert_eq!(
        outcome.message.as_deref(),
        Some("its container exited with code 1")
    );
    Ok(())
}

#[tokio::test]
async fn a_container_without_a_health_check_is_healthy_once_it_stayed_up() -> TestResult {
    let answers = looks(&[
        api("running", "Up 1 second", 0),
        api("running", "Up 4 seconds", 0),
        api("running", "Up 6 seconds", 0),
    ])?;
    let outcome = DockerComposeRuntime::new(answers.clone())
        .wait_for_services(
            &target(),
            &["api".to_owned()],
            policy(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Succeeded);
    assert_eq!(outcome.polls, 3, "it waited until the container was up 5 s");

    // One that has run for minutes is healthy at the first look.
    let answers = looks(&[api("running", "Up 3 minutes", 0)])?;
    let outcome = DockerComposeRuntime::new(answers)
        .wait_for_services(
            &target(),
            &["api".to_owned()],
            policy(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Succeeded);
    assert_eq!(outcome.polls, 1);
    Ok(())
}

/// A container the engine keeps restarting (a cluster node gives a service
/// `restart: unless-stopped`) is up for a moment between its exits. Those
/// moments do not make it healthy, and do not reset the count of looks that
/// found it restarting: the wait fails within a few looks, not after the
/// whole window or a name gate (BUGS.md, "A worker that exits at once is not
/// seen as exited on a cluster either").
#[tokio::test]
async fn a_container_in_a_restart_loop_fails_the_wait_within_a_few_looks() -> TestResult {
    let up = || api("running", "Up Less than a second", 0);
    // Compose's list says 0 in the exit code field and the code in the
    // status of a container the engine restarts.
    let restarting = || api("restarting", "Restarting (1) Less than a second ago", 0);
    let answers = looks(&[
        up(),
        restarting(),
        up(),
        restarting(),
        api("running", "Up 1 second", 0),
        restarting(),
    ])?;
    let outcome = DockerComposeRuntime::new(answers)
        .wait_for_services(
            &target(),
            &["api".to_owned()],
            policy(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Failed);
    assert_eq!(outcome.polls, 6);
    assert_eq!(outcome.service.as_deref(), Some("api"));
    assert_eq!(
        outcome.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_RESTARTING")
    );
    assert_eq!(
        outcome.message.as_deref(),
        Some("its container keeps exiting with code 1 and the engine restarts it")
    );
    Ok(())
}

/// `podman ps` as a Podman node lists `api`: Podman never shows a container
/// it restarts as `restarting`; it counts the restarts and starts the
/// container again at once, so a look finds it up for a moment, or exited.
fn podman_api(state: &str, status: &str, restarts: u64) -> String {
    format!(
        r#"[{{"Id":"abc","Names":["mm-conformance-app-api-1"],"Image":"docker.io/library/busybox:1.36.1","State":"{state}","Status":"{status}","ExitCode":0,"Restarts":{restarts},"StartedAt":1700000000,"Labels":{{"com.docker.compose.project":"mm-conformance-app","com.docker.compose.service":"api"}}}}]"#
    )
}

/// A container in a restart loop on a Podman node (15) fails the wait
/// with `MM_RUNTIME_RESTARTING` within a few looks, as on Docker, from the
/// restart count that grows between looks.
#[tokio::test]
async fn a_restart_loop_on_podman_fails_the_wait_within_a_few_looks() -> TestResult {
    let answers = looks(&[
        podman_api("running", "Up Less than a second", 0),
        podman_api("running", "Up Less than a second", 2),
        podman_api("running", "Up 1 second", 3),
        podman_api("running", "Up Less than a second", 5),
    ])?;
    let outcome = DockerComposeRuntime::new(answers)
        .wait_for_services(
            &target(),
            &["api".to_owned()],
            policy(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Failed);
    assert_eq!(outcome.polls, 4);
    assert_eq!(
        outcome.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_RESTARTING")
    );
    assert_eq!(
        outcome.message.as_deref(),
        Some("the engine keeps restarting its container")
    );

    // A container Podman started once and that stays up is healthy.
    let answers = looks(&[
        podman_api("running", "Up 2 seconds", 1),
        podman_api("running", "Up 6 seconds", 1),
    ])?;
    let outcome = DockerComposeRuntime::new(answers)
        .wait_for_services(
            &target(),
            &["api".to_owned()],
            policy(),
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Succeeded);
    Ok(())
}
