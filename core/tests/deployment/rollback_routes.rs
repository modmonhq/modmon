use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct RefusedSweep;

#[async_trait]
impl mm::app::ports::DeploymentJobs for RefusedSweep {
    async fn run(
        &self,
        _selection: &WorkspaceSelection,
        job: &str,
        _target: RuntimeTarget,
        _plan_hash: &str,
    ) -> Result<mm::domain::scheduler::JobRun, MmError> {
        Err(MmError::State {
            message: format!("job '{job}' is not part of this test"),
        })
    }

    async fn sweep(
        &self,
        _selection: &WorkspaceSelection,
        _projects: &[RuntimeTarget],
    ) -> Result<Vec<String>, MmError> {
        Err(MmError::State {
            message: "the values or secrets of 'local' changed since its last deployment"
                .to_owned(),
        })
    }
}

#[tokio::test]
async fn a_rollback_before_any_stack_does_not_check_the_routes_of_stacks_it_left_alone()
-> TestResult {
    let fixture = Fixture::new(false)?;
    let initial = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &initial.plan_hash)
        .await?;
    fixture.runtime.clear()?;
    fs::write(
        fixture.project.join("compose.yaml"),
        "services:\n  web:\n    image: example/web-v2\n    expose: [8080]\n",
    )?;
    fixture.verifier.fail_route.store(true, Ordering::SeqCst);
    let service = fixture.service.clone().with_jobs(Arc::new(RefusedSweep));
    let update = service.plan(&fixture.selection).await?;

    let result = service
        .apply_reviewed(&fixture.selection, &update.plan_hash)
        .await;

    let Err(MmError::State { message }) = result else {
        return Err(format!("the sweep's own refusal was not the answer: {result:?}").into());
    };
    assert!(
        message.contains("changed since its last deployment"),
        "{message}"
    );
    assert!(
        fixture.verifier.fail_route.load(Ordering::SeqCst),
        "the route of the stack no step changed was checked"
    );
    let calls = fixture.runtime.calls()?;
    assert!(!calls.iter().any(|call| call == "apply:app"), "{calls:?}");
    assert_eq!(fixture.routes.route_set(fixture.target_id)?.generation, 1);
    Ok(())
}
