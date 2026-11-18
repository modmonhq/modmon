//! A container that stays paused fails a wait at once instead of after the
//! whole window: only an unpause resumes it. On the VMs a rolling restart
//! waited 120 s for one (LIVE.md, "Findings for the spine").

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
        environment: "paused".to_owned(),
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

/// `docker compose ps` with one container of `api` in `state`.
fn api(state: &str, status: &str) -> String {
    format!(
        r#"[{{"ID":"abc","Name":"mm-conformance-app-api-1","Names":"mm-conformance-app-api-1","Image":"busybox:1.36.1","State":"{state}","Status":"{status}","ExitCode":0,"Labels":{{"com.docker.compose.project":"mm-conformance-app","com.docker.compose.service":"api"}}}}]"#
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

async fn wait(answers: Arc<Answers>) -> Result<mm::domain::runtime::ConvergenceOutcome, MmError> {
    DockerComposeRuntime::new(answers)
        .wait_for_services(
            &target(),
            &["api".to_owned()],
            ConvergencePolicy {
                timeout: Duration::from_secs(120),
                initial_backoff: Duration::from_millis(5),
                maximum_backoff: Duration::from_millis(5),
            },
            ConvergenceCancellation::default(),
        )
        .await
}

#[tokio::test]
async fn a_container_that_stays_paused_fails_the_wait_within_a_few_looks() -> TestResult {
    let paused = || api("paused", "Up 2 minutes (Paused)");
    let outcome = wait(looks(&[paused(), paused(), paused()])?).await?;
    assert_eq!(outcome.state, ConvergenceState::Failed);
    assert_eq!(outcome.polls, 3);
    assert_eq!(outcome.service.as_deref(), Some("api"));
    assert_eq!(
        outcome.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_PAUSED")
    );
    let message = outcome.message.unwrap_or_default();
    assert!(
        message.starts_with("its container mm-conformance-app-api-1 is paused")
            && message.contains("`docker unpause mm-conformance-app-api-1`"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn a_pause_of_a_moment_does_not_fail_the_wait() -> TestResult {
    let outcome = wait(looks(&[
        api("paused", "Up 2 minutes (Paused)"),
        api("running", "Up 2 minutes"),
    ])?)
    .await?;
    assert_eq!(outcome.state, ConvergenceState::Succeeded);
    Ok(())
}
