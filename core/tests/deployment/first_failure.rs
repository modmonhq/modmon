use super::*;

use mm::domain::rollout::{DeploymentStatus, RolloutChanges};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn a_failed_first_deployment_keeps_what_it_started_and_the_next_one_deploys_over_it()
-> TestResult {
    let fixture = Fixture::new(false)?;
    let failing = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);

    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &failing.plan_hash)
        .await
        .err()
        .ok_or("an unhealthy first deployment succeeded")?;

    let MmError::OperationFailure {
        mutation_status,
        cleanup_failures,
        recovery,
        primary,
        ..
    } = &error
    else {
        return Err(format!("unexpected error: {error}").into());
    };
    assert_eq!(*mutation_status, mm::MutationStatus::Applied);
    assert!(cleanup_failures.is_empty());
    assert!(matches!(**primary, MmError::RuntimeResponse { .. }));
    assert_eq!(error.code(), primary.code());
    assert!(
        recovery.contains("nothing was rolled back")
            && recovery.contains("(app)")
            && recovery.contains("`mm service logs SERVICE --environment local`")
            && recovery.contains("`mm deploy plan --environment local`"),
        "{recovery}"
    );
    let calls = fixture.runtime.calls()?;
    assert!(
        !calls.iter().any(|call| call.starts_with("remove")),
        "{calls:?}"
    );
    assert!(
        fixture
            .state
            .running_operations(fixture.workspace()?.id)?
            .is_empty()
    );

    fixture.runtime.clear()?;
    let next = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &next.plan_hash)
        .await?;
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"apply:app".to_owned()), "{calls:?}");
    assert!(
        !calls.iter().any(|call| call.starts_with("remove")),
        "{calls:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_first_deployment_that_fails_after_its_stacks_is_still_removed() -> TestResult {
    let fixture = Fixture::new(true)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("a deployment whose routes failed succeeded")?;
    assert!(
        !matches!(error, MmError::OperationFailure { .. }),
        "{error}"
    );
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"remove:app".to_owned()), "{calls:?}");
    assert!(calls.contains(&"remove:ingress".to_owned()), "{calls:?}");
    Ok(())
}

#[tokio::test]
async fn an_interrupted_first_rollout_keeps_the_stacks_the_next_one_runs() -> TestResult {
    use mm::app::StackStep;
    let fixture = Fixture::on_node("app-1")?;
    let selection = &fixture.selection;
    let service = &fixture.service;
    let first = service.plan(selection).await?;
    service.begin_rollout(selection, &first.plan_hash).await?;
    fixture
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    service
        .roll_stack(selection, &first.plan_hash, "app", StackStep::Apply)
        .await?;

    write_cluster_project(
        &fixture.project,
        "      web: {stack: app}\n      worker: {stack: app}",
        "services:\n  web: {image: example/web:2}\n  worker: {image: example/worker}\n",
    )?;
    fixture.runtime.clear()?;
    let next = service.plan(selection).await?;
    assert_ne!(next.plan_hash, first.plan_hash);
    service.begin_rollout(selection, &next.plan_hash).await?;
    let calls = fixture.runtime.calls()?;
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("remove") || call.starts_with("apply:")),
        "{calls:?}"
    );
    assert!(
        service
            .rollback_rollout(selection, &first.plan_hash, None)
            .await
            .is_ok_and(|undone| !undone),
        "the interrupted rollout is closed"
    );
    Ok(())
}

#[test]
fn begun_lists_each_stack_a_step_reached_once_in_order() {
    let mut changes = RolloutChanges::default();
    assert!(changes.begun().is_empty());
    changes.note("db", true);
    changes.note("db", false);
    changes.note("api", true);
    changes.note("web", false);
    assert_eq!(changes.begun(), ["db", "api", "web"]);
}

#[test]
fn an_environment_is_never_deployed_only_when_every_host_says_so() -> TestResult {
    let status = |rows: serde_json::Value| -> Result<DeploymentStatus, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "target": "cluster",
            "environment": "production",
            "nodes": rows,
        }))
    };
    let record = serde_json::json!({
        "commit": BUILD_COMMIT,
        "deployed_at_unix_ms": 1,
        "deployed_by": "ci",
        "runs_services": true,
    });
    assert!(
        status(serde_json::json!([
            {"node": "app-1", "status": {}},
            {"node": "app-2", "status": {}},
        ]))?
        .never_deployed()
    );
    assert!(
        !status(serde_json::json!([
            {"node": "app-1", "status": {}},
            {"node": "app-2", "status": {"current": record}},
        ]))?
        .never_deployed()
    );
    assert!(
        !status(serde_json::json!([
            {"node": "app-1", "status": {}},
            {"node": "app-2", "problem": "unreachable"},
        ]))?
        .never_deployed()
    );
    assert!(!status(serde_json::json!([]))?.never_deployed());
    Ok(())
}
