use super::*;

#[test]
fn resource_and_journey_labels_are_complete_and_unique() {
    let resources = [
        ConfigResourceKind::Module,
        ConfigResourceKind::Service,
        ConfigResourceKind::Stack,
        ConfigResourceKind::Build,
        ConfigResourceKind::Environment,
        ConfigResourceKind::Binding,
        ConfigResourceKind::Secret,
        ConfigResourceKind::Ingress,
        ConfigResourceKind::LiveMount,
        ConfigResourceKind::Job,
        ConfigResourceKind::Schedule,
        ConfigResourceKind::Gate,
    ];
    let labels = resources.map(ConfigResourceKind::label);
    assert_eq!(
        labels
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        labels.len()
    );

    let journeys = [
        GuidedJourney::GlobalSetup,
        GuidedJourney::NewProject,
        GuidedJourney::ExistingProject,
        GuidedJourney::NodeSetup,
        GuidedJourney::ProjectEditing,
    ];
    let titles = journeys.map(GuidedJourney::title);
    assert_eq!(
        titles
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        titles.len()
    );
    assert_eq!(WorkflowState::default().step, WorkflowStep::ChooseJourney);
}

#[test]
fn mutation_debug_redacts_secret_definitions_and_classifies_public_config() {
    let definition = serde_json::json!({"value": "sentinel"});
    let mut request = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Secret,
        name: "token".to_owned(),
        parent: None,
        definition: Some(definition),
    };
    let secret_debug = format!("{request:?}");
    assert!(secret_debug.contains("[REDACTED]"));
    assert!(!secret_debug.contains("sentinel"));
    request.resource = ConfigResourceKind::Module;
    let public_debug = format!("{request:?}");
    assert!(public_debug.contains("[PUBLIC CONFIG]"));
    assert!(!public_debug.contains("sentinel"));
}
