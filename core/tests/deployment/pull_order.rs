use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn position(calls: &[String], call: &str) -> Result<usize, String> {
    calls
        .iter()
        .position(|made| made == call)
        .ok_or_else(|| format!("no {call} in {calls:?}"))
}

#[tokio::test]
async fn a_stack_pulls_its_images_before_compose_applies_it() -> TestResult {
    let fixture = Fixture::new(false)?;
    let plan = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    let calls = fixture.runtime.calls()?;
    assert!(
        position(&calls, "pull:app")? < position(&calls, "apply:app")?,
        "{calls:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_stack_pulls_its_images_before_an_old_container_leaves() -> TestResult {
    use mm::app::StackStep;
    let fixture = Fixture::on_node("app-1")?;
    let leaving = Arc::new(RecordingLeaving(Arc::clone(&fixture.runtime)));
    let service = fixture
        .service
        .with_names(Arc::new(WebOnAppTwo))
        .with_node_host(leaving, Arc::new(NoProxy));
    let selection = &fixture.selection;
    on_backend(&fixture.project)?;
    let first = service.plan(selection).await?;
    service.begin_rollout(selection, &first.plan_hash).await?;
    service
        .roll_stack(selection, &first.plan_hash, "app", StackStep::Apply)
        .await?;
    service.finish_rollout(selection, &first.plan_hash).await?;
    write_cluster_project(
        &fixture.project,
        "      web: {stack: app}\n      worker: {stack: app}",
        "services:\n  web: {image: example/web:2}\n  worker: {image: example/worker}\n",
    )?;
    on_backend(&fixture.project)?;
    let second = service.plan(selection).await?;
    service.begin_rollout(selection, &second.plan_hash).await?;
    *fixture.runtime.outdated.lock().map_err(|_| "poisoned")? =
        vec![("web".to_owned(), "web-1-id".to_owned())];
    fixture.runtime.clear()?;
    service
        .roll_stack(selection, &second.plan_hash, "app", StackStep::Apply)
        .await?;
    let calls = fixture.runtime.calls()?;
    let pull = position(&calls, "pull:app")?;
    let leave = calls
        .iter()
        .position(|call| call.starts_with("leave:web-1-id"))
        .ok_or_else(|| format!("web did not leave: {calls:?}"))?;
    assert!(pull < leave, "{calls:?}");
    assert!(pull < position(&calls, "up:app")?, "{calls:?}");
    Ok(())
}
