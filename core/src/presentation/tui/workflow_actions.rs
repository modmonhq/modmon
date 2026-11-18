//! The guided setup workflow: its steps, and applying what was reviewed.

use super::*;

pub(super) async fn apply_workflow_action(
    model: &mut Model,
    action: Action,
    onboarding: &crate::app::OnboardingService,
) {
    let Some(event) = workflow_event(model, action) else {
        return;
    };
    let effect = match onboarding.transition(&mut model.workflow, event) {
        Ok(effect) => effect,
        Err(error) => return set_workflow_error(model, error),
    };
    apply_effect(model, effect, onboarding);
}

/// The workflow's answer to a key, or `None` when the key means nothing at
/// this step.
fn workflow_event(
    model: &Model,
    action: Action,
) -> Option<crate::domain::onboarding::WorkflowEvent> {
    use crate::domain::onboarding::{WorkflowEvent, WorkflowRequest, WorkflowStep};

    Some(match action {
        Action::StartWorkflow(journey) => WorkflowEvent::Start(WorkflowRequest {
            journey,
            root: model.snapshot.workspace.root.clone(),
            environment: Some(model.snapshot.environment.clone()),
            target: Some(model.snapshot.target.clone()),
            mutation: None,
        }),
        Action::AdvanceWorkflow => match model.workflow.step {
            WorkflowStep::Configure => WorkflowEvent::Review,
            WorkflowStep::Review => WorkflowEvent::Apply,
            WorkflowStep::ChooseJourney
            | WorkflowStep::Discover
            | WorkflowStep::Applying
            | WorkflowStep::Complete
            | WorkflowStep::Error
            | WorkflowStep::Cancelled => return None,
        },
        Action::BackWorkflow => WorkflowEvent::Back,
        Action::CancelWorkflow => WorkflowEvent::Cancel,
        Action::ResumeWorkflow => match model.workflow.step {
            WorkflowStep::Error => WorkflowEvent::Retry,
            WorkflowStep::Cancelled => WorkflowEvent::Resume,
            _ => return None,
        },
        Action::None
        | Action::ConfigureMount
        | Action::DisableMount
        | Action::Provider(_)
        | Action::Gate(_)
        | Action::RunJob(_)
        | Action::RunSchedule(_)
        | Action::ToggleSchedule(_, _)
        | Action::CancelJob(_)
        | Action::RetryJob(_)
        | Action::ReconcileSchedules
        | Action::TickScheduler
        | Action::Stack(_)
        | Action::SetOverride { .. }
        | Action::UnsetOverride(_)
        | Action::EditSecret(_)
        | Action::EditRemoteSecret(_)
        | Action::Stage(_)
        | Action::Refresh
        | Action::FollowLogs { .. }
        | Action::RunCommand { .. }
        | Action::CloseStream
        | Action::Restart(_)
        | Action::CheckEnvironments(_)
        | Action::SwitchEnvironment(_)
        | Action::CancelSwitch
        | Action::OpenEditor(_)
        | Action::EditorPlan
        | Action::EditorApply
        | Action::DevelopmentUnit { .. }
        | Action::RunSetup(_) => {
            return None;
        }
    })
}

/// Carries out what the workflow asks for after a step.
fn apply_effect(
    model: &mut Model,
    effect: crate::domain::onboarding::WorkflowEffect,
    onboarding: &crate::app::OnboardingService,
) {
    use crate::domain::onboarding::{WorkflowEffect, WorkflowEvent};

    match effect {
        WorkflowEffect::None => {}
        WorkflowEffect::Discover(request) => match onboarding.plan_workflow(request) {
            Ok(review) => {
                if let Err(error) =
                    onboarding.transition(&mut model.workflow, WorkflowEvent::Discovered(review))
                {
                    set_workflow_error(model, error);
                }
            }
            Err(error) => {
                if let Err(transition) = onboarding.transition(
                    &mut model.workflow,
                    WorkflowEvent::Failed(error.to_string()),
                ) {
                    set_workflow_error(model, transition);
                }
            }
        },
        WorkflowEffect::ApplyReviewed { request, plan_hash } => {
            let result = match request.mutation {
                Some(mutation) => onboarding
                    .apply_mutation(&request.root, mutation, &plan_hash)
                    .map(|_| "Reviewed configuration change applied.".to_owned()),
                None => Ok(
                    "Review complete. No mutation was requested; use the displayed typed next action."
                        .to_owned(),
                ),
            };
            let event = match result {
                Ok(message) => WorkflowEvent::Applied(message),
                Err(error) => WorkflowEvent::Failed(error.to_string()),
            };
            if let Err(error) = onboarding.transition(&mut model.workflow, event) {
                set_workflow_error(model, error);
            }
        }
        WorkflowEffect::RequestSecretInput { .. } => {
            set_workflow_error(
                model,
                "Secret input must use the dedicated hidden-input effect; it is never stored in TUI state.",
            );
        }
    }
}

fn set_workflow_error(model: &mut Model, error: impl std::fmt::Display) {
    model.workflow.step = crate::domain::onboarding::WorkflowStep::Error;
    model.workflow.message = Some(error.to_string());
}
