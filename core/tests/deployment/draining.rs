use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn a_rollout_step_tells_the_runtime_which_containers_still_drain() -> TestResult {
    use mm::app::StackStep;
    let fixture = Fixture::on_node("app-1")?;
    let leaving = Arc::new(RecordingLeaving(Arc::clone(&fixture.runtime)));
    let service = fixture
        .service
        .with_names(Arc::new(WebOnAppTwo))
        .with_node_host(leaving, Arc::new(NoProxy));
    let selection = &fixture.selection;
    write_cluster_project(
        &fixture.project,
        "      web: {stack: app, update: {order: start-first}}\n      worker: {stack: app}",
        "services:\n  web: {image: example/web:2}\n  worker: {image: example/worker}\n",
    )?;
    on_backend(&fixture.project)?;
    let plan = service.plan(selection).await?;
    assert_eq!(plan.rollout.start_first("app"), ["web"]);
    *fixture.runtime.draining.lock().map_err(|_| "poisoned")? = vec!["web-old-id".to_owned()];
    service.begin_rollout(selection, &plan.plan_hash).await?;
    service
        .roll_stack(selection, &plan.plan_hash, "app", StackStep::Start)
        .await?;
    service
        .roll_stack(selection, &plan.plan_hash, "app", StackStep::Apply)
        .await?;
    let told = fixture
        .runtime
        .told_draining
        .lock()
        .map_err(|_| "poisoned")?
        .clone();
    assert_eq!(told, ["beside:web:web-old-id", "unchanged:web-old-id"]);
    Ok(())
}
