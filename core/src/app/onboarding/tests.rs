use super::*;
use crate::domain::onboarding::{ProjectDiscovery, WorkflowReview};

fn mutation(
    action: ConfigMutationAction,
    resource: ConfigResourceKind,
    definition: Option<serde_json::Value>,
) -> ConfigMutationRequest {
    ConfigMutationRequest {
        action,
        resource,
        name: "example".to_owned(),
        parent: None,
        definition,
    }
}

fn request() -> WorkflowRequest {
    WorkflowRequest {
        journey: GuidedJourney::ProjectEditing,
        root: std::path::PathBuf::from("/workspace"),
        environment: Some("local".to_owned()),
        target: Some("local".to_owned()),
        mutation: None,
    }
}

fn review() -> WorkflowReview {
    WorkflowReview {
        journey: GuidedJourney::ProjectEditing,
        title: "Edit project configuration".to_owned(),
        proposals: Vec::new(),
        affected_files: Vec::new(),
        canonical_diff: String::new(),
        generated_resources: Vec::new(),
        generated_routes: Vec::new(),
        privilege_prompts: Vec::new(),
        next_actions: Vec::new(),
        plan_hash: "a".repeat(64),
    }
}

#[test]
fn mutation_validation_rejects_ambiguous_shapes_and_nested_plaintext() {
    for name in ["", "   ", "bad name", "bad\nname"] {
        let mut request = mutation(
            ConfigMutationAction::Add,
            ConfigResourceKind::Module,
            Some(serde_json::json!({})),
        );
        request.name = name.to_owned();
        assert!(matches!(
            validate_mutation_request(&request),
            Err(MmError::ConfigParse { .. })
        ));
    }

    for action in [ConfigMutationAction::Add, ConfigMutationAction::Edit] {
        let request = mutation(action, ConfigResourceKind::Module, None);
        assert!(matches!(
            validate_mutation_request(&request),
            Err(MmError::Environment { .. })
        ));
    }
    let remove = mutation(
        ConfigMutationAction::Remove,
        ConfigResourceKind::Module,
        Some(serde_json::json!({})),
    );
    assert!(matches!(
        validate_mutation_request(&remove),
        Err(MmError::Environment { .. })
    ));

    for definition in [
        serde_json::json!({"password": "sentinel"}),
        serde_json::json!({"nested": [{"TOKEN": "sentinel"}]}),
        serde_json::json!([{"material": "sentinel"}]),
    ] {
        let request = mutation(
            ConfigMutationAction::Add,
            ConfigResourceKind::Secret,
            Some(definition),
        );
        assert!(matches!(
            validate_mutation_request(&request),
            Err(MmError::Security { .. })
        ));
    }

    assert!(
        validate_mutation_request(&mutation(
            ConfigMutationAction::Add,
            ConfigResourceKind::Secret,
            Some(serde_json::json!({"provider": "age", "generate": {"length": 32}})),
        ))
        .is_ok()
    );
    assert!(
        validate_mutation_request(&mutation(
            ConfigMutationAction::Remove,
            ConfigResourceKind::Secret,
            None,
        ))
        .is_ok()
    );
}

#[test]
fn canonical_diffs_and_journey_guidance_are_complete() {
    assert_eq!(canonical_diff("same\n", "same\n"), "(no changes)\n");
    let changed = canonical_diff("first\nold\nlast\n", "first\nnew\nlast\n");
    assert!(changed.contains("@@ -2,1 +2,1 @@"));
    assert!(changed.contains("-old\n+new\n"));
    let added = canonical_diff("", "new\n");
    assert!(added.contains("+new\n"));

    for journey in [
        GuidedJourney::GlobalSetup,
        GuidedJourney::NewProject,
        GuidedJourney::ExistingProject,
        GuidedJourney::NodeSetup,
        GuidedJourney::ProjectEditing,
    ] {
        assert!(!next_actions(journey).is_empty());
        assert_eq!(
            privilege_prompts(journey).is_empty(),
            !matches!(
                journey,
                GuidedJourney::GlobalSetup | GuidedJourney::NodeSetup
            )
        );
    }
}

#[test]
fn workflow_transition_accepts_each_legal_edge_and_rejects_missing_review_state()
-> Result<(), MmError> {
    let mut invalid_request = request();
    invalid_request.mutation = Some(mutation(
        ConfigMutationAction::Add,
        ConfigResourceKind::Module,
        None,
    ));
    assert!(
        transition(
            &mut WorkflowState::default(),
            WorkflowEvent::Start(invalid_request)
        )
        .is_err()
    );

    let request = request();
    let review = review();
    let mut state = WorkflowState::default();
    assert_eq!(
        transition(&mut state, WorkflowEvent::Start(request.clone()))?,
        WorkflowEffect::Discover(request.clone())
    );
    assert_eq!(state.step, WorkflowStep::Discover);
    assert_eq!(
        transition(&mut state, WorkflowEvent::Discovered(review.clone()))?,
        WorkflowEffect::None
    );
    assert_eq!(state.step, WorkflowStep::Configure);
    transition(&mut state, WorkflowEvent::Review)?;
    let effect = transition(&mut state, WorkflowEvent::Apply)?;
    assert_eq!(
        effect,
        WorkflowEffect::ApplyReviewed {
            request: request.clone(),
            plan_hash: review.plan_hash.clone(),
        }
    );
    assert_eq!(state.step, WorkflowStep::Applying);
    transition(&mut state, WorkflowEvent::Applied("complete".to_owned()))?;
    assert_eq!(state.step, WorkflowStep::Complete);
    assert_eq!(state.message.as_deref(), Some("complete"));

    let mut state = WorkflowState {
        step: WorkflowStep::Discover,
        ..WorkflowState::default()
    };
    transition(&mut state, WorkflowEvent::Configure)?;
    assert_eq!(state.step, WorkflowStep::Configure);

    let mut missing_request = WorkflowState {
        step: WorkflowStep::Review,
        review: Some(review.clone()),
        ..WorkflowState::default()
    };
    assert!(transition(&mut missing_request, WorkflowEvent::Apply).is_err());
    let mut missing_review = WorkflowState {
        step: WorkflowStep::Review,
        request: Some(request.clone()),
        ..WorkflowState::default()
    };
    assert!(transition(&mut missing_review, WorkflowEvent::Apply).is_err());

    let back_cases = [
        (WorkflowStep::Configure, WorkflowStep::Discover),
        (WorkflowStep::Review, WorkflowStep::Configure),
        (WorkflowStep::Error, WorkflowStep::Review),
        (WorkflowStep::ChooseJourney, WorkflowStep::ChooseJourney),
        (WorkflowStep::Discover, WorkflowStep::ChooseJourney),
        (WorkflowStep::Applying, WorkflowStep::ChooseJourney),
        (WorkflowStep::Complete, WorkflowStep::ChooseJourney),
        (WorkflowStep::Cancelled, WorkflowStep::ChooseJourney),
    ];
    for (step, expected) in back_cases {
        let mut state = WorkflowState {
            step,
            message: Some("clear me".to_owned()),
            ..WorkflowState::default()
        };
        transition(&mut state, WorkflowEvent::Back)?;
        assert_eq!(state.step, expected);
        assert_eq!(state.message, None);
    }

    Ok(())
}

#[test]
fn workflow_cancel_resume_failure_retry_and_invalid_events_are_explicit() -> Result<(), MmError> {
    let mut state = WorkflowState::default();
    transition(&mut state, WorkflowEvent::Cancel)?;
    assert_eq!(state.step, WorkflowStep::Cancelled);
    assert!(
        state
            .message
            .as_deref()
            .is_some_and(|value| value.contains("no changes"))
    );
    transition(&mut state, WorkflowEvent::Resume)?;
    assert_eq!(state.step, WorkflowStep::Discover);

    state.step = WorkflowStep::Cancelled;
    state.review = Some(review());
    transition(&mut state, WorkflowEvent::Resume)?;
    assert_eq!(state.step, WorkflowStep::Review);

    transition(&mut state, WorkflowEvent::Failed("injected".to_owned()))?;
    assert_eq!(state.step, WorkflowStep::Error);
    assert_eq!(state.message.as_deref(), Some("injected"));
    assert!(transition(&mut state, WorkflowEvent::Retry).is_err());
    state.request = Some(request());
    assert!(matches!(
        transition(&mut state, WorkflowEvent::Retry)?,
        WorkflowEffect::Discover(_)
    ));
    assert_eq!(state.message, None);

    assert!(transition(&mut WorkflowState::default(), WorkflowEvent::Review).is_err());
    Ok(())
}

#[test]
fn workflow_hashes_bind_discovery_mutation_and_privilege_inputs() -> Result<(), MmError> {
    let request = request();
    let discovery = ProjectDiscovery {
        root: request.root.clone(),
        proposals: Vec::new(),
    };
    let baseline = workflow_plan_hash(&request, &discovery, None, &[])?;
    assert_ne!(
        baseline,
        workflow_plan_hash(&request, &discovery, Some("mutation"), &[])?
    );
    assert_ne!(
        baseline,
        workflow_plan_hash(&request, &discovery, None, &["privilege".to_owned()])?
    );

    let mutation = mutation(
        ConfigMutationAction::Add,
        ConfigResourceKind::Module,
        Some(serde_json::json!({})),
    );
    assert_ne!(
        mutation_plan_hash(&mutation, "source-a", "result")?,
        mutation_plan_hash(&mutation, "source-b", "result")?
    );
    Ok(())
}
