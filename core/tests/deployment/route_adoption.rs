use super::*;

use mm::domain::deployment::DeploymentPlan;
use mm::presentation::inline::{Palette, plain};
use mm::presentation::output::{local_plan_card, local_plan_lines};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn foreign_claim(
    own: &IngressRouteClaim,
    (project_id, environment): (ProjectId, &str),
    host: &str,
) -> IngressRouteClaim {
    let mut foreign = own.clone();
    foreign.project_id = project_id;
    foreign.project_name = "foreign".to_owned();
    foreign.environment = environment.to_owned();
    foreign.stack = "foreign".to_owned();
    foreign.route = "foreign".to_owned();
    foreign.router_name = "foreign-router".to_owned();
    foreign.backend_alias = "foreign-backend".to_owned();
    foreign.host = host.to_owned();
    foreign.ownership = mm::domain::ingress::IngressRouteOwnership::Project {
        project_id,
        environment: environment.to_owned(),
        stack: foreign.stack.clone(),
        service: foreign.service.clone(),
    };
    foreign
}

fn other_project() -> ProjectId {
    ProjectId(Uuid::from_u128(0x119c_527b_6242_7b43_bfd4_bd60_e27c_1de3))
}

fn claims_of(routes: &IngressRouteSet, project_id: ProjectId) -> Vec<IngressRouteClaim> {
    routes
        .claims
        .iter()
        .filter(|claim| claim.project_id == project_id && claim.environment == "local")
        .cloned()
        .collect()
}

fn others_than(routes: &IngressRouteSet, project_id: ProjectId) -> Vec<IngressRouteClaim> {
    routes
        .claims
        .iter()
        .filter(|claim| claim.project_id != project_id || claim.environment != "local")
        .cloned()
        .collect()
}

fn no_route_changes(plan: &DeploymentPlan) -> bool {
    plan.route_diff.added.is_empty()
        && plan.route_diff.changed.is_empty()
        && plan.route_diff.removed.is_empty()
}

fn rewrite_own_artifact_hashes(fixture: &Fixture) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(rusqlite::Connection::open(&fixture.state_path)?.execute(
        "UPDATE ingress_route_claims SET artifact_hash = 'rewritten-' || artifact_hash
         WHERE target_id = ?1 AND project_id = ?2 AND environment_name = 'local'",
        [fixture.target_id.0.to_string(), PROJECT_ID.to_owned()],
    )?)
}

/// Opens a deployment of `plan`, commits its routes, and leaves it in
/// `state`, or running.
fn commit_routes_and_stop(
    fixture: &Fixture,
    plan: &DeploymentPlan,
    state: Option<OperationState>,
) -> Result<OperationId, Box<dyn std::error::Error>> {
    let id = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id,
        workspace_id: fixture.workspace()?.id,
        kind: OperationKind::Deploy,
        environment: plan.environment.clone(),
        requested_at_unix_ms: 1_000,
        plan_hash: Some(plan.plan_hash.clone()),
    })?;
    fixture.state.save_deployment_plan(id, plan)?;
    fixture
        .routes
        .replace_owner_claims_cas(IngressClaimsReplacement {
            target_id: plan.target_id,
            project_id: plan.project_id,
            environment: &plan.environment,
            expected_generation: plan.expected_route_generation,
            claims: &plan.desired_route_claims,
            middlewares: &plan.desired_middlewares,
            preserve_gate_attachments: true,
            config_digest: Some(&plan.dynamic_config.manifest.digest),
            now_unix_ms: 1_000,
        })?;
    if let Some(state) = state {
        fixture
            .state
            .finish_operation(id, state, 1_000, Some("MM_RUNTIME_RESPONSE"))?;
    }
    Ok(id)
}

fn operation_state(
    fixture: &Fixture,
    id: OperationId,
) -> Result<(OperationState, Option<String>), Box<dyn std::error::Error>> {
    let operation = fixture
        .state
        .recent_operations(fixture.workspace()?.id, 20)?
        .into_iter()
        .find(|operation| operation.id == id)
        .ok_or("the operation is missing")?;
    Ok((operation.state, operation.error_code))
}

fn write_image(fixture: &Fixture, image: &str) -> Result<(), std::io::Error> {
    fs::write(
        fixture.project.join("compose.yaml"),
        format!("services:\n  web:\n    image: {image}\n    expose: [8080]\n"),
    )
}

#[tokio::test]
async fn a_plan_takes_back_the_environments_routes_the_last_finished_deployment_did_not_record()
-> TestResult {
    let fixture = Fixture::new(false)?;
    let draft = fixture.service.plan(&fixture.selection).await?;
    let other = foreign_claim(
        &draft.desired_route_claims[0],
        (other_project(), "local"),
        "other.example.test",
    );
    fixture.routes.replace_owner_claims(
        fixture.target_id,
        other_project(),
        "local",
        std::slice::from_ref(&other),
        &BTreeMap::new(),
        900,
    )?;
    let first = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &first.plan_hash)
        .await?;
    assert_eq!(rewrite_own_artifact_hashes(&fixture)?, 1);
    let before = fixture.routes.route_set(fixture.target_id)?;

    let plan = fixture.service.plan(&fixture.selection).await?;

    let held = claims_of(&before, first.project_id);
    assert!(held[0].artifact_hash.starts_with("rewritten-"));
    assert_eq!(plan.adopted_route_claims, held);
    assert_eq!(plan.previous_owner_claims, held);
    assert!(plan.route_state_changed);
    assert_eq!(
        plan.expected_previous_plan_hash.as_deref(),
        Some(first.plan_hash.as_str())
    );
    assert_eq!(plan.expected_route_generation, before.generation);
    assert!(before.config_digest.is_some());
    assert_eq!(plan.expected_route_config_digest, before.config_digest);
    assert!(no_route_changes(&plan));
    let line = "takes back 1 route this environment holds that the last finished deployment did not record";
    assert_eq!(plan.adopted_routes_line().as_deref(), Some(line));
    let lines = local_plan_lines(&plan);
    assert!(lines.contains(&format!("  routes: {line}")), "{lines:#?}");
    let card = local_plan_card(&plan, &[])
        .lines(Palette::plain(), 200)
        .iter()
        .map(plain)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(card.contains(&format!("The plan {line}")), "{card}");
    let json = serde_json::to_value(&plan)?;
    assert_eq!(
        json["adopted_route_claims"][0]["artifact_hash"],
        serde_json::json!(held[0].artifact_hash)
    );

    let mut unadopted = plan.clone();
    unadopted.adopted_route_claims.clear();
    assert_ne!(unadopted.canonical_hash()?, plan.plan_hash);
    let mut elsewhere = plan.clone();
    elsewhere.adopted_route_claims[0].host = "elsewhere.example.test".to_owned();
    assert!(elsewhere.canonical_hash().is_err());
    let mut uncommitted = plan.clone();
    uncommitted.route_state_changed = false;
    assert!(uncommitted.canonical_hash().is_err());

    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;

    let after = fixture.routes.route_set(fixture.target_id)?;
    assert_eq!(
        claims_of(&after, plan.project_id),
        plan.desired_route_claims
    );
    assert_eq!(
        others_than(&after, plan.project_id),
        others_than(&before, plan.project_id)
    );
    assert_eq!(others_than(&after, plan.project_id).len(), 1);
    assert_eq!(after.generation, before.generation + 1);
    assert_eq!(
        after.config_digest.as_deref(),
        Some(plan.dynamic_config.manifest.digest.as_str())
    );
    let settled = fixture.service.plan(&fixture.selection).await?;
    assert!(settled.adopted_route_claims.is_empty());
    assert!(!settled.route_state_changed);
    assert!(
        serde_json::to_value(&settled)?
            .get("adopted_route_claims")
            .is_none()
    );
    assert!(
        !local_plan_lines(&settled)
            .iter()
            .any(|line| line.contains("takes back"))
    );
    Ok(())
}

#[tokio::test]
async fn routes_no_deployment_ever_recorded_are_taken_back_by_the_first_one() -> TestResult {
    let fixture = Fixture::new(false)?;
    let draft = fixture.service.plan(&fixture.selection).await?;
    fixture.routes.replace_owner_claims(
        fixture.target_id,
        draft.project_id,
        &draft.environment,
        &draft.desired_route_claims,
        &draft.desired_middlewares,
        900,
    )?;

    let plan = fixture.service.plan(&fixture.selection).await?;

    assert_eq!(plan.adopted_route_claims, draft.desired_route_claims);
    assert_eq!(plan.expected_previous_plan_hash, None);
    assert_eq!(plan.expected_route_config_digest, None);
    assert!(plan.route_state_changed && no_route_changes(&plan));
    assert_eq!(
        plan.adopted_routes_line().as_deref(),
        Some("takes back 1 route this environment holds that no finished deployment recorded")
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    let after = fixture.routes.route_set(fixture.target_id)?;
    assert_eq!(after.claims, plan.desired_route_claims);
    assert_eq!(
        after.config_digest.as_deref(),
        Some(plan.dynamic_config.manifest.digest.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn taking_back_routes_never_takes_a_host_another_owner_holds() -> TestResult {
    let owners = [
        (other_project(), "local"),
        (ProjectId(Uuid::parse_str(PROJECT_ID)?), "staging"),
    ];
    for owner in owners {
        let fixture = Fixture::new(false)?;
        let draft = fixture.service.plan(&fixture.selection).await?;
        fixture.routes.replace_owner_claims(
            fixture.target_id,
            draft.project_id,
            &draft.environment,
            &draft.desired_route_claims,
            &draft.desired_middlewares,
            900,
        )?;
        let foreign = foreign_claim(&draft.desired_route_claims[0], owner, "shared.example.test");
        fixture.routes.replace_owner_claims(
            fixture.target_id,
            owner.0,
            owner.1,
            std::slice::from_ref(&foreign),
            &BTreeMap::new(),
            901,
        )?;
        let config = fixture.project.join(".config/mm.yaml");
        fs::write(
            &config,
            fs::read_to_string(&config)?
                .replace("app: app.example.test", "app: shared.example.test"),
        )?;
        let before = fixture.routes.route_set(fixture.target_id)?;

        let error = fixture
            .service
            .plan(&fixture.selection)
            .await
            .err()
            .ok_or("a plan took a host another owner holds")?;

        assert!(
            matches!(
                &error,
                MmError::Environment { message }
                    if message.contains("conflicts with route")
                        && message.contains("run `mm deploy plan` again")
            ),
            "{error}"
        );
        assert_eq!(others_than(&before, draft.project_id).len(), 1);
        assert_eq!(claims_of(&before, draft.project_id).len(), 1);
        assert_eq!(fixture.routes.route_set(fixture.target_id)?, before);
    }
    Ok(())
}

#[tokio::test]
async fn a_failed_apply_that_committed_its_routes_does_not_strand_the_next_plan() -> TestResult {
    let fixture = Fixture::new(false)?;
    let first = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &first.plan_hash)
        .await?;
    write_image(&fixture, "example/web-v2")?;
    let update = fixture.service.plan(&fixture.selection).await?;
    assert_ne!(update.desired_route_claims, first.desired_route_claims);
    let failed = commit_routes_and_stop(&fixture, &update, Some(OperationState::Failed))?;

    let plan = fixture.service.plan(&fixture.selection).await?;

    assert_eq!(plan.adopted_route_claims, update.desired_route_claims);
    assert_eq!(
        plan.expected_previous_plan_hash.as_deref(),
        Some(first.plan_hash.as_str())
    );
    assert!(plan.route_state_changed && no_route_changes(&plan));
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    assert_eq!(
        fixture.routes.route_set(fixture.target_id)?.claims,
        plan.desired_route_claims
    );
    assert_eq!(operation_state(&fixture, failed)?.0, OperationState::Failed);
    Ok(())
}

#[tokio::test]
async fn an_interrupted_apply_whose_routes_another_owner_changed_since_is_undone_and_replanned()
-> TestResult {
    let fixture = Fixture::new(false)?;
    let first = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &first.plan_hash)
        .await?;
    write_image(&fixture, "example/web-v2")?;
    let update = fixture.service.plan(&fixture.selection).await?;
    let interrupted = commit_routes_and_stop(&fixture, &update, None)?;
    let other = foreign_claim(
        &update.desired_route_claims[0],
        (other_project(), "local"),
        "other.example.test",
    );
    fixture.routes.replace_owner_claims(
        fixture.target_id,
        other_project(),
        "local",
        std::slice::from_ref(&other),
        &BTreeMap::new(),
        1_001,
    )?;
    let before = fixture.routes.route_set(fixture.target_id)?;

    let plan = fixture.service.plan(&fixture.selection).await?;

    assert_eq!(plan.adopted_route_claims, update.desired_route_claims);
    assert_eq!(
        plan.expected_previous_plan_hash.as_deref(),
        Some(first.plan_hash.as_str())
    );
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    assert_eq!(
        operation_state(&fixture, interrupted)?,
        (
            OperationState::Interrupted,
            Some("MM_INTERRUPTED_RECOVERED".to_owned())
        )
    );
    let after = fixture.routes.route_set(fixture.target_id)?;
    assert_eq!(
        claims_of(&after, plan.project_id),
        plan.desired_route_claims
    );
    assert_eq!(
        others_than(&after, plan.project_id),
        others_than(&before, plan.project_id)
    );
    Ok(())
}

#[tokio::test]
async fn a_plan_after_the_routes_were_deleted_by_hand_applies() -> TestResult {
    let fixture = Fixture::new(false)?;
    let first = fixture.service.plan(&fixture.selection).await?;
    fixture
        .service
        .apply_reviewed(&fixture.selection, &first.plan_hash)
        .await?;
    let connection = rusqlite::Connection::open(&fixture.state_path)?;
    connection.execute(
        "DELETE FROM ingress_route_claims WHERE project_id = ?1",
        [PROJECT_ID],
    )?;
    connection.execute(
        "UPDATE ingress_route_generations SET config_digest = NULL",
        [],
    )?;

    let plan = fixture.service.plan(&fixture.selection).await?;

    assert!(plan.adopted_route_claims.is_empty());
    assert_eq!(plan.adopted_routes_line(), None);
    assert_eq!(
        plan.expected_previous_plan_hash.as_deref(),
        Some(first.plan_hash.as_str())
    );
    assert!(plan.route_state_changed);
    fixture
        .service
        .apply_reviewed(&fixture.selection, &plan.plan_hash)
        .await?;
    assert_eq!(
        fixture.routes.route_set(fixture.target_id)?.claims,
        plan.desired_route_claims
    );
    Ok(())
}
