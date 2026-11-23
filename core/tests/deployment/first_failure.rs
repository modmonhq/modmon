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
    assert_eq!(*mutation_status, mm::MutationStatus::Kept);
    assert!(error.kept());
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
async fn the_next_plan_removes_a_kept_stack_the_configuration_no_longer_has() -> TestResult {
    let fixture = Fixture::new(false)?;
    let failing = fixture.service.plan(&fixture.selection).await?;
    let kept = failing
        .stacks
        .first()
        .map(|stack| stack.compose_project_name.clone())
        .ok_or("the plan has no stack")?;
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
    assert!(error.kept(), "{error}");

    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(
        "deployment-test-{environment}-{stack}",
        "replacement-{environment}-{stack}",
    );
    fs::write(config_path, config)?;
    fixture.runtime.clear()?;
    let next = fixture.service.plan(&fixture.selection).await?;
    assert_eq!(
        next.removed_stacks
            .iter()
            .map(|stack| stack.compose_project_name.as_str())
            .collect::<Vec<_>>(),
        [kept.as_str()]
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &next.plan_hash)
        .await?;
    let calls = fixture.runtime.calls()?;
    assert!(calls.contains(&"remove:app".to_owned()), "{calls:?}");
    let after = fixture.service.plan(&fixture.selection).await?;
    assert!(after.removed_stacks.is_empty());
    Ok(())
}

fn rename_projects(fixture: &Fixture, from: &str, to: &str) -> TestResult {
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?.replace(from, to);
    fs::write(config_path, config)?;
    Ok(())
}

async fn fail_first(fixture: &Fixture) -> TestResult {
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .runtime
        .fail_convergence
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("an unhealthy first deployment succeeded")?;
    assert!(error.kept(), "{error}");
    Ok(())
}

#[tokio::test]
async fn every_project_a_renamed_stack_kept_is_removed_once() -> TestResult {
    let fixture = Fixture::new(false)?;
    fail_first(&fixture).await?;
    rename_projects(
        &fixture,
        "deployment-test-{environment}-{stack}",
        "second-{environment}-{stack}",
    )?;
    fail_first(&fixture).await?;
    rename_projects(
        &fixture,
        "second-{environment}-{stack}",
        "third-{environment}-{stack}",
    )?;
    fixture.runtime.clear()?;
    let next = fixture.service.plan(&fixture.selection).await?;
    let mut removed = next
        .removed_stacks
        .iter()
        .map(|stack| (stack.stack.as_str(), stack.compose_project_name.as_str()))
        .collect::<Vec<_>>();
    removed.sort_unstable();
    assert_eq!(removed.len(), 2, "{removed:?}");
    assert!(
        removed.iter().all(|(stack, _)| *stack == "app"),
        "{removed:?}"
    );
    assert_ne!(removed[0].1, removed[1].1);
    fixture
        .service
        .apply_reviewed(&fixture.selection, &next.plan_hash)
        .await?;
    let calls = fixture.runtime.calls()?;
    assert_eq!(
        calls.iter().filter(|call| *call == "remove:app").count(),
        2,
        "{calls:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_first_deployment_that_was_removed_is_not_removed_again() -> TestResult {
    let fixture = Fixture::new(true)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    let error = fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await
        .err()
        .ok_or("a deployment whose routes failed succeeded")?;
    assert!(!error.kept(), "{error}");
    rename_projects(
        &fixture,
        "deployment-test-{environment}-{stack}",
        "replacement-{environment}-{stack}",
    )?;
    let next = fixture.service.plan(&fixture.selection).await?;
    assert!(next.removed_stacks.is_empty(), "{:?}", next.removed_stacks);
    Ok(())
}

#[tokio::test]
async fn recovering_an_interrupted_first_deployment_keeps_only_what_the_next_plan_has() -> TestResult
{
    let fixture = Fixture::new(false)?;
    let config_path = fixture.project.join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "runtime:\n  driver: podman-compose\n",
            "runtime:\n  driver: podman-compose\nnetworks:\n  internal: {}\n",
        )
        .replace(
            "    files: [compose.yaml]\n",
            "    files: [compose.yaml]\n    networks: [internal]\n",
        )
        .replace(
            "  app:\n    path: app\n",
            "  app:\n    path: app\n    networks: [internal]\n",
        );
    fs::write(config_path, config)?;
    let next = fixture.service.plan(&fixture.selection).await?;
    let internal = next
        .networks
        .first()
        .map(|network| network.physical_name.clone())
        .ok_or("the plan has no network")?;
    let mut interrupted_plan = next.clone();
    let mut orphan = interrupted_plan
        .networks
        .first()
        .cloned()
        .ok_or("the plan has no network")?;
    orphan.logical_name = "orphan".to_owned();
    orphan.physical_name = "deployment-test-orphan".to_owned();
    for member in &mut orphan.members {
        member.stack = "gone".to_owned();
    }
    interrupted_plan.networks.push(orphan);
    interrupted_plan.plan_hash = interrupted_plan.canonical_hash()?;
    let workspace = fixture.workspace()?;
    let interrupted = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id: interrupted,
        workspace_id: workspace.id,
        kind: OperationKind::Deploy,
        environment: "local".to_owned(),
        requested_at_unix_ms: 900,
        plan_hash: Some(interrupted_plan.plan_hash.clone()),
    })?;
    fixture
        .state
        .save_deployment_plan(interrupted, &interrupted_plan)?;

    fixture
        .service
        .apply_reviewed(&fixture.selection, &next.plan_hash)
        .await?;
    let calls = fixture.runtime.calls()?;
    let removals = calls
        .iter()
        .filter(|call| call.starts_with("remove"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        removals,
        ["remove-network:deployment-test-orphan"],
        "{calls:?}"
    );
    assert!(!removals.contains(&format!("remove-network:{internal}")));
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
