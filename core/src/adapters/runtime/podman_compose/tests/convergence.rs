fn convergence_policy(timeout: std::time::Duration) -> crate::domain::runtime::ConvergencePolicy {
    crate::domain::runtime::ConvergencePolicy {
        timeout,
        initial_backoff: std::time::Duration::ZERO,
        maximum_backoff: std::time::Duration::ZERO,
    }
}

fn compose_config() -> CommandOutput {
    command_output("services:\n  api:\n    image: alpine:3.22\n")
}

fn observed_container() -> CommandOutput {
    command_output(
        &json!([{
            "Id": "abc123",
            "Names": ["demo-local-app-api-1"],
            "Image": "alpine:3.22",
            "State": "running",
            "Status": "Up 10 seconds (starting)",
            "Labels": {
                "com.docker.compose.project": "demo-local-app",
                "com.docker.compose.service": "api"
            }
        }])
        .to_string(),
    )
}

#[tokio::test]
async fn convergence_reports_cancelled_terminal_timeout_and_observation_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from("/workspace");
    let target = deployment_target(&root);

    let cancellation = crate::domain::runtime::ConvergenceCancellation::default();
    cancellation.cancel();
    let runtime = PodmanComposeRuntime::with_executable(
        recording_runner(Vec::<CommandOutput>::new()),
        "podman-test",
    );
    assert!(format!("{runtime:?}").contains("PodmanComposeRuntime"));
    let outcome = crate::adapters::runtime::convergence::wait_for_project(
        &runtime,
        &target,
        convergence_policy(std::time::Duration::from_secs(1)),
        cancellation,
        None,
    )
    .await?;
    assert_eq!(outcome.state, crate::domain::runtime::ConvergenceState::Cancelled);
    assert_eq!(outcome.polls, 0);
    assert!(outcome.observation.services.is_empty());
    let cancellation = crate::domain::runtime::ConvergenceCancellation::default();
    cancellation.cancel();
    let outcome = RuntimeDriver::wait_for_convergence(
        &runtime,
        &target,
        convergence_policy(std::time::Duration::from_secs(1)),
        cancellation,
    )
    .await?;
    assert_eq!(outcome.state, crate::domain::runtime::ConvergenceState::Cancelled);

    for (health, expected) in [
        ("abc123\thealthy\n", crate::domain::runtime::ConvergenceState::Succeeded),
        ("abc123\tunhealthy\n", crate::domain::runtime::ConvergenceState::Failed),
    ] {
        let runtime = PodmanComposeRuntime::with_executable(
            recording_runner([compose_config(), observed_container(), command_output(health)]),
            "podman-test",
        );
        let outcome = crate::adapters::runtime::convergence::wait_for_project(
            &runtime,
            &target,
            convergence_policy(std::time::Duration::ZERO),
            crate::domain::runtime::ConvergenceCancellation::default(),
            None,
        )
        .await?;
        assert_eq!(outcome.state, expected);
        assert_eq!(outcome.polls, 1);
        assert_eq!(
            outcome.diagnostic_code.is_some(),
            expected == crate::domain::runtime::ConvergenceState::Failed
        );
    }

    let runtime = PodmanComposeRuntime::with_executable(
        recording_runner([compose_config(), command_output("[]")]),
        "podman-test",
    );
    let timed_out = crate::adapters::runtime::convergence::wait_for_project(
        &runtime,
        &target,
        convergence_policy(std::time::Duration::ZERO),
        crate::domain::runtime::ConvergenceCancellation::default(),
        None,
    )
    .await?;
    assert_eq!(timed_out.state, crate::domain::runtime::ConvergenceState::TimedOut);
    assert_eq!(timed_out.polls, 1);
    assert!(timed_out.message.is_some());

    let runtime = PodmanComposeRuntime::with_executable(
        recording_runner(Vec::<CommandOutput>::new()),
        "podman-test",
    );
    assert!(
        crate::adapters::runtime::convergence::wait_for_project(
            &runtime,
            &target,
            convergence_policy(std::time::Duration::from_secs(1)),
            crate::domain::runtime::ConvergenceCancellation::default(),
            None,
        )
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn convergence_repolls_accepted_observations_and_detects_mid_poll_cancellation()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from("/workspace");
    let target = deployment_target(&root);
    let runtime = PodmanComposeRuntime::with_executable(
        recording_runner([
            compose_config(),
            command_output("[]"),
            compose_config(),
            observed_container(),
            command_output("abc123\thealthy\n"),
        ]),
        "podman-test",
    );
    let outcome = crate::adapters::runtime::convergence::wait_for_project(
        &runtime,
        &target,
        convergence_policy(std::time::Duration::from_secs(1)),
        crate::domain::runtime::ConvergenceCancellation::default(),
        None,
    )
    .await?;
    assert_eq!(outcome.state, crate::domain::runtime::ConvergenceState::Succeeded);
    assert_eq!(outcome.polls, 2);

    let cancellation = crate::domain::runtime::ConvergenceCancellation::default();
    let runtime = PodmanComposeRuntime::with_executable(
        Arc::new(CancelAfterObservationRunner {
            outputs: Mutex::new([compose_config(), command_output("[]")].into()),
            calls: std::sync::atomic::AtomicUsize::new(0),
            cancellation: cancellation.clone(),
        }),
        "podman-test",
    );
    let outcome = crate::adapters::runtime::convergence::wait_for_project(
        &runtime,
        &target,
        convergence_policy(std::time::Duration::from_secs(1)),
        cancellation,
        None,
    )
    .await?;
    assert_eq!(outcome.state, crate::domain::runtime::ConvergenceState::Cancelled);
    assert_eq!(outcome.polls, 1);
    Ok(())
}

struct CancelAfterObservationRunner {
    outputs: Mutex<VecDeque<CommandOutput>>,
    calls: std::sync::atomic::AtomicUsize,
    cancellation: crate::domain::runtime::ConvergenceCancellation,
}

#[async_trait]
impl ProcessRunner for CancelAfterObservationRunner {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        if self
            .calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 1
        {
            self.cancellation.cancel();
        }
        self.outputs
            .lock()
            .map_err(|_| MmError::State {
                message: "convergence output lock was poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "convergence runner has no output".to_owned(),
            })
    }
}
