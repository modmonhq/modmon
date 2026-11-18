use std::sync::Arc;

use mm::adapters::config::ConfigLoader;
use mm::app::OnboardingService;
use mm::domain::onboarding::{
    ConfigMutationAction, ConfigMutationRequest, ConfigResourceKind, GuidedJourney, WorkflowEffect,
    WorkflowEvent, WorkflowRequest, WorkflowState, WorkflowStep,
};
use tempfile::TempDir;

mod temp_dirs;

const SENTINEL: &str = "TOP_SECRET_ONBOARDING_SENTINEL";

fn service() -> OnboardingService {
    OnboardingService::new(Arc::new(ConfigLoader))
}

fn initialized_project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    std::fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: nginx\n    ports: [\"8080:80\"]\n",
    )?;
    ConfigLoader.initialize(project.path(), Some("guided-test"))?;
    let path = project.path().join(".config/mm.yaml");
    let source = std::fs::read_to_string(&path)?;
    std::fs::write(
        &path,
        format!("# unrelated operator comment\n{source}# trailing operator comment\n"),
    )?;
    Ok(project)
}

fn module_request(action: ConfigMutationAction) -> ConfigMutationRequest {
    ConfigMutationRequest {
        action,
        resource: ConfigResourceKind::Module,
        name: "api".to_owned(),
        parent: None,
        definition: (action != ConfigMutationAction::Remove).then(|| {
            serde_json::json!({
                "path": "api",
                "services": {}
            })
        }),
    }
}

#[test]
fn reviewed_mutation_is_minimal_comment_preserving_and_stale_safe()
-> Result<(), Box<dyn std::error::Error>> {
    let project = initialized_project()?;
    let service = service();
    let request = module_request(ConfigMutationAction::Add);
    let plan = service.plan_mutation(project.path(), request.clone())?;
    assert!(plan.canonical_diff.contains("+  api:"));
    assert_eq!(plan.affected_files.len(), 1);
    assert_eq!(plan.generated_resources, ["module:api"]);
    assert_eq!(plan.plan_hash.len(), 64);

    let before = std::fs::read_to_string(project.path().join(".config/mm.yaml"))?;
    assert!(!before.contains("api:"));
    let result = service.apply_mutation(project.path(), request.clone(), &plan.plan_hash)?;
    assert_eq!(result.plan_hash, plan.plan_hash);
    let after = std::fs::read_to_string(project.path().join(".config/mm.yaml"))?;
    assert!(after.starts_with("# unrelated operator comment\n"));
    assert!(after.ends_with("# trailing operator comment\n"));
    assert!(after.contains("  api:\n    path: api"));

    let edit = ConfigMutationRequest {
        action: ConfigMutationAction::Edit,
        definition: Some(serde_json::json!({"path": "backend", "services": {}})),
        ..request
    };
    let stale = service.plan_mutation(project.path(), edit.clone())?;
    std::fs::write(
        project.path().join(".config/mm.yaml"),
        after.replace("# trailing", "# concurrent\n# trailing"),
    )?;
    let error = match service.apply_mutation(project.path(), edit, &stale.plan_hash) {
        Ok(_) => return Err("a stale reviewed edit succeeded".into()),
        Err(error) => error,
    };
    assert!(error.to_string().contains("plan changed"));
    Ok(())
}

#[test]
fn cancellation_back_resume_error_and_retry_are_pure_state_transitions()
-> Result<(), Box<dyn std::error::Error>> {
    let project = initialized_project()?;
    let service = service();
    let request = WorkflowRequest {
        journey: GuidedJourney::ProjectEditing,
        root: project.path().to_path_buf(),
        environment: Some("local".to_owned()),
        target: Some("local".to_owned()),
        mutation: Some(module_request(ConfigMutationAction::Add)),
    };
    let before = std::fs::read(project.path().join(".config/mm.yaml"))?;
    let mut state = WorkflowState::default();
    assert!(matches!(
        service.transition(&mut state, WorkflowEvent::Start(request.clone()))?,
        WorkflowEffect::Discover(_)
    ));
    let review = service.plan_workflow(request)?;
    service.transition(&mut state, WorkflowEvent::Discovered(review))?;
    assert_eq!(state.step, WorkflowStep::Configure);
    service.transition(&mut state, WorkflowEvent::Review)?;
    service.transition(&mut state, WorkflowEvent::Back)?;
    assert_eq!(state.step, WorkflowStep::Configure);
    service.transition(&mut state, WorkflowEvent::Cancel)?;
    assert_eq!(state.step, WorkflowStep::Cancelled);
    service.transition(&mut state, WorkflowEvent::Resume)?;
    assert_eq!(state.step, WorkflowStep::Review);
    service.transition(&mut state, WorkflowEvent::Failed("injected".to_owned()))?;
    assert_eq!(state.step, WorkflowStep::Error);
    assert!(matches!(
        service.transition(&mut state, WorkflowEvent::Retry)?,
        WorkflowEffect::Discover(_)
    ));
    assert_eq!(
        before,
        std::fs::read(project.path().join(".config/mm.yaml"))?,
        "state transitions and cancellation must not mutate configuration"
    );
    Ok(())
}

#[test]
fn tui_and_headless_requests_produce_the_same_plan_hash_and_no_secret_state()
-> Result<(), Box<dyn std::error::Error>> {
    let project = initialized_project()?;
    std::fs::create_dir(project.path().join("api"))?;
    std::fs::write(project.path().join("api/package.json"), "{}\n")?;
    std::fs::write(
        project.path().join("api/.env.example"),
        format!("PASSWORD={SENTINEL}\n"),
    )?;
    let service = service();
    let request = WorkflowRequest {
        journey: GuidedJourney::NewProject,
        root: project.path().to_path_buf(),
        environment: None,
        target: None,
        mutation: None,
    };
    let headless = service.plan_workflow(request.clone())?;
    let mut state = WorkflowState::default();
    let effect = service.transition(&mut state, WorkflowEvent::Start(request))?;
    let WorkflowEffect::Discover(tui_request) = effect else {
        return Err("TUI did not submit a typed discovery effect".into());
    };
    let tui = service.plan_workflow(tui_request)?;
    assert_eq!(headless.plan_hash, tui.plan_hash);
    let serialized = serde_json::to_string(&(&state, &tui))?;
    let debug = format!("{state:?} {tui:?}");
    assert!(!serialized.contains(SENTINEL));
    assert!(!debug.contains(SENTINEL));
    assert!(
        tui.proposals
            .iter()
            .any(|proposal| proposal.subject == "environment-template:api")
    );
    Ok(())
}

#[test]
fn secret_declaration_rejects_plaintext_before_preview() -> Result<(), Box<dyn std::error::Error>> {
    let project = initialized_project()?;
    let request = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Secret,
        name: "database.password".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({
            "provider": "age://local/database-password",
            "value": SENTINEL
        })),
    };
    let debug = format!("{request:?}");
    let error = match service().plan_mutation(project.path(), request) {
        Ok(_) => return Err("a plaintext secret declaration was accepted".into()),
        Err(error) => error,
    };
    assert!(error.to_string().contains("dedicated hidden-input effect"));
    assert!(!debug.contains(SENTINEL));
    assert!(!error.to_string().contains(SENTINEL));
    Ok(())
}

#[test]
fn discovery_reports_compose_services_builds_ports_templates_and_mount_reasons()
-> Result<(), Box<dyn std::error::Error>> {
    let project = temp_dirs::tempdir()?;
    std::fs::create_dir(project.path().join("web"))?;
    std::fs::write(project.path().join("web/Dockerfile"), "FROM scratch\n")?;
    std::fs::write(project.path().join("web/.env.example"), "PUBLIC_URL=\n")?;
    std::fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    build: web\n    ports: [\"8080:80\"]\n    volumes: [\"./web:/app\"]\n    env_file: web/.env\n",
    )?;
    let review = service().plan_workflow(WorkflowRequest {
        journey: GuidedJourney::NewProject,
        root: project.path().to_path_buf(),
        environment: None,
        target: None,
        mutation: None,
    })?;
    let text = serde_json::to_string(&review)?;
    for expected in [
        "compose-file",
        "service:web",
        "module:web",
        "environment-template:web",
        "development-mount:web",
        "Compose build declaration",
        "HTTP port",
    ] {
        assert!(
            text.contains(expected),
            "missing discovery evidence {expected}"
        );
    }
    Ok(())
}

#[test]
fn guided_edits_and_discovery_fail_closed_for_incomplete_and_malformed_shapes()
-> Result<(), Box<dyn std::error::Error>> {
    let project = initialized_project()?;
    let onboarding = service();
    let missing_definition = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Module,
        name: "missing-definition".to_owned(),
        parent: None,
        definition: None,
    };
    assert!(
        onboarding
            .plan_mutation(project.path(), missing_definition)
            .is_err()
    );
    let missing_parent = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Service,
        name: "web".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({"stack": "app"})),
    };
    assert!(
        onboarding
            .plan_mutation(project.path(), missing_parent)
            .is_err()
    );

    let add = module_request(ConfigMutationAction::Add);
    let plan = onboarding.plan_mutation(project.path(), add.clone())?;
    onboarding.apply_mutation(project.path(), add.clone(), &plan.plan_hash)?;
    assert!(onboarding.plan_mutation(project.path(), add).is_err());
    let edit_without_definition = ConfigMutationRequest {
        action: ConfigMutationAction::Edit,
        resource: ConfigResourceKind::Module,
        name: "api".to_owned(),
        parent: None,
        definition: None,
    };
    assert!(
        onboarding
            .plan_mutation(project.path(), edit_without_definition)
            .is_err()
    );

    let malformed = temp_dirs::tempdir()?;
    std::fs::write(
        malformed.path().join("compose.yaml"),
        "services:\n  7: {}\n",
    )?;
    let review = onboarding.plan_workflow(WorkflowRequest {
        journey: GuidedJourney::NewProject,
        root: malformed.path().to_path_buf(),
        environment: None,
        target: None,
        mutation: None,
    })?;
    assert!(
        review
            .proposals
            .iter()
            .all(|proposal| !proposal.subject.starts_with("service:"))
    );
    assert!(
        onboarding
            .plan_workflow(WorkflowRequest {
                journey: GuidedJourney::NewProject,
                root: malformed.path().join("missing"),
                environment: None,
                target: None,
                mutation: None,
            })
            .is_err()
    );
    Ok(())
}
