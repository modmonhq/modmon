mod access;
mod control;
mod development;
pub mod driver;
mod editor;
mod env_input;
mod environments;
#[cfg(test)]
mod fixtures;
mod gate;
mod host_values;
mod job_actions;
use job_actions::{apply_scheduler_action, apply_scheduler_admin, refresh_scheduler};
mod live;
mod loading;
mod model;
mod ops;
mod panel;
mod provider;
mod review;
mod service_nodes;
mod session;
mod setup;
mod update;
mod variables;
mod view;
mod workflow_actions;
use workflow_actions::apply_workflow_action;

use std::io::{self, Stdout};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use zeroize::Zeroizing;

use crate::domain::ca::LocalCaStatus;
use crate::domain::environment::EnvironmentStatusSnapshot;
use crate::domain::error::MmError;
use crate::domain::service_stack::ServiceStackStatus;
use crate::domain::workspace::WorkspaceSnapshot;
use crate::presentation::tui::model::{Model, ModelInput};
pub(crate) use crate::presentation::tui::provider::SecretBackendTuiContext;
use crate::presentation::tui::update::{Action, Message, update};
pub use control::{ProgressSink, StartReview, WorkspaceControl};
pub use environments::{
    EnvironmentDirectory, EnvironmentEntry, EnvironmentProbe, NodeLine, ServiceLine,
};
pub use live::LiveStatusSource;
use live::{LiveRefresh, REFRESH_INTERVAL, REMOTE_REFRESH_INTERVAL};
pub use loading::{LoadProgress, Loaded};
pub use model::{AccessView, LiveStatus, TeamMemberView};
pub use ops::{ServiceOperations, ServiceTarget};
pub use session::{LaunchRequest, TuiExit, run};
pub use setup::{SetupGuide, SetupStep, SetupView};
pub use view::Tone;
pub(crate) use view::{ColorDepth, SPINNER, detect_depth};

struct TuiUseCases<'a> {
    mounts: &'a crate::app::DevelopmentMountManager,
    development: &'a crate::app::DevelopmentUnitService,
    selection: &'a crate::app::WorkspaceSelection,
    onboarding: &'a crate::app::OnboardingService,
    backends: &'a crate::app::SecretBackendManager,
    secrets: &'a crate::app::SecretManager,
    gates: &'a crate::app::GateManager,
    scheduler: &'a crate::app::JobSchedulerService,
    stack: &'a control::StackRunner,
    operations: &'a ops::OperationRunner,
    environments: &'a environments::EnvironmentRunner,
    guide: &'a dyn SetupGuide,
    refresh_now: &'a tokio::sync::Notify,
}

async fn apply_action(
    model: &mut Model,
    action: Action,
    use_cases: &TuiUseCases<'_>,
) -> Option<SecretInputRequest> {
    if matches!(
        action,
        Action::StartWorkflow(_)
            | Action::AdvanceWorkflow
            | Action::BackWorkflow
            | Action::CancelWorkflow
            | Action::ResumeWorkflow
    ) {
        apply_workflow_action(model, action, use_cases.onboarding).await;
        return None;
    }
    if matches!(action, Action::None) {
        return None;
    }
    let action = match apply_service_action(model, action, use_cases) {
        Some(action) => action,
        None => return None,
    };
    let action = match apply_editor_action(model, action, use_cases).await {
        Some(action) => action,
        None => return None,
    };
    if matches!(
        action,
        Action::Stack(_)
            | Action::SetOverride { .. }
            | Action::UnsetOverride(_)
            | Action::EditSecret(_)
            | Action::EditRemoteSecret(_)
            | Action::Refresh
    ) {
        return control::apply_control_action(model, action, use_cases);
    }
    if matches!(action, Action::ConfigureMount | Action::DisableMount) {
        return apply_mount(model, action, use_cases).await;
    }
    match action {
        Action::Provider(action) => {
            apply_provider_action(model, *action, use_cases.backends, use_cases.secrets).await
        }
        Action::Gate(action) => apply_gate_action(model, action, use_cases.gates),
        Action::ReconcileSchedules | Action::TickScheduler => {
            apply_scheduler_admin(model, action, use_cases).await;
            None
        }
        action => {
            apply_scheduler_action(model, action, use_cases).await;
            None
        }
    }
}

/// Starts log, command, restart, and environment actions in the background.
/// Returns the action when it is not one of these.
fn apply_service_action(
    model: &mut Model,
    action: Action,
    use_cases: &TuiUseCases<'_>,
) -> Option<Action> {
    match action {
        Action::FollowLogs { session, target } => {
            use_cases.operations.follow_logs(session, target);
        }
        Action::RunCommand {
            session,
            target,
            argv,
        } => use_cases.operations.run_command(session, target, argv),
        Action::CloseStream => use_cases.operations.close_stream(),
        Action::Restart(target) => {
            model.restarting.push(target.label.clone());
            use_cases.operations.restart(target);
        }
        Action::Stage(request) => use_cases.operations.stage(request.edit()),
        Action::CheckEnvironments(environments) => use_cases
            .environments
            .probe(environments, &model.snapshot.environment),
        Action::SwitchEnvironment(environment) => {
            use_cases.operations.close_stream();
            use_cases.environments.switch(environment);
        }
        Action::CancelSwitch => use_cases.environments.cancel_switch(),
        Action::DevelopmentUnit { unit, enable } => {
            development::toggle(model, &unit, enable, use_cases);
        }
        action => return Some(action),
    }
    None
}

/// Opens, plans, and applies configuration edits. Returns the action when it
/// is not one of these.
async fn apply_editor_action(
    model: &mut Model,
    action: Action,
    use_cases: &TuiUseCases<'_>,
) -> Option<Action> {
    use crate::domain::onboarding::ConfigMutationAction;
    use editor::{Editor, EditorContext, EditorKind};
    use model::Notice;

    match action {
        Action::OpenEditor(mut open) => {
            let root = model.snapshot.workspace.root.clone();
            let base = match &open.name {
                Some(name) => match use_cases.onboarding.current_definition(
                    &root,
                    open.kind.resource(),
                    name,
                    open.parent.as_deref(),
                ) {
                    Ok(base) => base,
                    Err(error) => {
                        model.notice = Some(Notice::error(
                            crate::presentation::output::error_line(&error),
                        ));
                        return None;
                    }
                },
                None => None,
            };
            let name = open.name.clone().unwrap_or_default();
            match (open.kind, open.action, &base) {
                // Setting a key that has no value yet adds it.
                (EditorKind::Binding, ConfigMutationAction::Edit, None) => {
                    open.action = ConfigMutationAction::Add;
                }
                (EditorKind::Binding, ConfigMutationAction::Remove, None) => {
                    model.notice = Some(Notice::info(format!(
                        "{name} has no value in the configuration for {}; nothing to remove.",
                        model.snapshot.environment
                    )));
                    return None;
                }
                (_, ConfigMutationAction::Edit | ConfigMutationAction::Remove, None) => {
                    model.notice = Some(Notice::info(format!(
                        "{name} is not in the project configuration, so it cannot be changed here."
                    )));
                    return None;
                }
                (EditorKind::Binding, ConfigMutationAction::Edit, Some(base))
                    if base.get("cast").is_some() =>
                {
                    model.notice = Some(Notice::info(format!(
                        "{name} is computed from another key; edit it in the configuration file."
                    )));
                    return None;
                }
                _ => {}
            }
            let mut context = EditorContext::from_model(model);
            if open.kind == EditorKind::Join {
                match use_cases.environments.directory().device_key() {
                    Ok(key) => context.device_key = Some(key),
                    Err(error) => {
                        model.notice = Some(Notice::error(
                            crate::presentation::output::error_line(&error),
                        ));
                        return None;
                    }
                }
                context.user = std::env::var("USER").ok();
            }
            let removing = open.action == ConfigMutationAction::Remove;
            model.editor = Some(Editor::new(open, base, &context));
            if removing {
                editor::plan(model, use_cases.onboarding);
            }
        }
        Action::EditorPlan => editor::plan(model, use_cases.onboarding),
        Action::EditorApply => {
            if editor::apply(model, use_cases.onboarding) {
                // Everything read from the configuration may have changed.
                let directory = use_cases.environments.directory();
                model.environments.entries = directory.environments();
                let (device_key, team) = directory.team(&model.snapshot.environment);
                model.access.device_key = device_key;
                model.access.team = team;
                model.clamp_selection();
                refresh_scheduler(model, use_cases).await;
                use_cases.refresh_now.notify_one();
            }
        }
        action => return Some(action),
    }
    None
}

async fn apply_mount(
    model: &mut Model,
    action: Action,
    use_cases: &TuiUseCases<'_>,
) -> Option<SecretInputRequest> {
    let Some(plan) = model.mounts.get(model.scroll).cloned() else {
        model.mount_message = Some("No development mount proposal is selected.".to_owned());
        return None;
    };
    let request = crate::app::DevelopmentMountPlanRequest {
        module: plan.proposal.module.clone(),
        ..Default::default()
    };
    let result = match action {
        Action::ConfigureMount => {
            use_cases
                .mounts
                .configure_reviewed(use_cases.selection, &request, &plan.plan_hash)
                .await
        }
        Action::DisableMount => {
            use_cases
                .mounts
                .disable_reviewed(use_cases.selection, &request, &plan.plan_hash)
                .await
        }
        Action::None => return None,
        Action::StartWorkflow(_)
        | Action::AdvanceWorkflow
        | Action::BackWorkflow
        | Action::CancelWorkflow
        | Action::ResumeWorkflow
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
        | Action::RunSetup(_) => return None,
    };
    match result {
        Ok(updated) => {
            model.mount_message = Some(format!(
                "Module '{}' mount is now {} in the project config.",
                updated.proposal.module,
                if updated.proposal.configured {
                    "configured"
                } else {
                    "disabled"
                }
            ));
            if let Some(current) = model.mounts.get_mut(model.scroll) {
                *current = updated;
            }
        }
        Err(error) => model.mount_message = Some(error.to_string()),
    }
    None
}

#[derive(Debug, Clone)]
struct SecretInputRequest {
    prompt: String,
    pending: PendingSecretApply,
}

#[derive(Debug, Clone)]
enum PendingSecretApply {
    External {
        request: Box<crate::domain::secret_backend::RemoteSecretBackendSetupRequest>,
        plan_hash: String,
    },
    Rotation {
        name: String,
        plan_hash: String,
    },
    GateUser {
        policy: String,
        username: String,
        plan_hash: String,
    },
    EnvironmentSecret {
        name: String,
    },
    /// Staged on every host of a remote environment.
    RemoteSecret {
        name: String,
    },
}

fn apply_gate_action(
    model: &mut Model,
    action: gate::GateAction,
    gates: &crate::app::GateManager,
) -> Option<SecretInputRequest> {
    use crate::domain::gate::GateMutationAction;

    match action {
        gate::GateAction::None => None,
        gate::GateAction::PlanUser { policy, username } => {
            match gates.plan_user(
                GateMutationAction::AddUser,
                model.snapshot.workspace.project_id,
                &model.snapshot.environment,
                &policy,
                &username,
            ) {
                Ok(plan) => {
                    model.gates.screen = gate::GateScreen::Review {
                        policy,
                        username,
                        plan: Box::new(plan),
                    };
                }
                Err(error) => model.gates.screen = gate::GateScreen::Error(error.to_string()),
            }
            None
        }
        gate::GateAction::ApplyUser {
            policy,
            username,
            plan_hash,
        } => {
            model.gates.screen = gate::GateScreen::Applying;
            Some(SecretInputRequest {
                prompt: format!("Password for gate user {username}"),
                pending: PendingSecretApply::GateUser {
                    policy,
                    username,
                    plan_hash,
                },
            })
        }
    }
}

async fn apply_provider_action(
    model: &mut Model,
    action: provider::ProviderAction,
    backends: &crate::app::SecretBackendManager,
    secrets: &crate::app::SecretManager,
) -> Option<SecretInputRequest> {
    use provider::{ProviderAction, ProviderReview, ProviderScreen};

    if !matches!(
        &action,
        ProviderAction::ApplyReview | ProviderAction::RetryApply | ProviderAction::None
    ) {
        model.providers.last_review = None;
    }
    match &action {
        provider::ProviderAction::TestExternal(request) => {
            apply_external_status(model, backends, request.clone()).await;
            return None;
        }
        provider::ProviderAction::ManagedStatus(provider) => {
            apply_managed_status(model, backends, provider.clone()).await;
            return None;
        }
        _ => {}
    }
    let result =
        match action {
            ProviderAction::PlanExternal(request) => backends
                .plan_remote_setup(request.clone())
                .map(|plan| ProviderReview::ExternalSetup {
                    request,
                    plan: Box::new(plan),
                }),
            ProviderAction::PlanManaged(request) => backends
                .plan_managed(request.clone())
                .map(|plan| ProviderReview::Managed { request, plan }),
            ProviderAction::PlanData(provider, action, path) => {
                let path = if path.is_absolute() {
                    path
                } else {
                    model.providers.context.root.join(path)
                };
                backends
                    .plan_managed_data(
                        &provider,
                        action,
                        &path,
                        &model.providers.context.target,
                        &model.providers.context.config_hash,
                    )
                    .map(ProviderReview::Data)
            }
            ProviderAction::PlanRotation(name) => secrets
                .plan_direct_rotation(
                    &model.providers.context.root,
                    Some(&model.providers.context.environment),
                    &name,
                )
                .await
                .map(ProviderReview::Rotation),
            ProviderAction::ReportError(message) => Err(MmError::State { message }),
            ProviderAction::ApplyReview | ProviderAction::RetryApply => {
                return apply_provider_review(model, backends, secrets).await;
            }
            ProviderAction::TestExternal(_) | ProviderAction::ManagedStatus(_) => return None,
            ProviderAction::None => return None,
        };
    match result {
        Ok(review) => {
            model.providers.last_review = Some(review.clone());
            model.providers.screen = ProviderScreen::Review(Box::new(review));
        }
        Err(error) => provider_error(model, error),
    }
    None
}

async fn apply_external_status(
    model: &mut Model,
    backends: &crate::app::SecretBackendManager,
    request: crate::domain::secret_backend::RemoteSecretBackendStatusRequest,
) {
    model.providers.screen = provider::ProviderScreen::Applying(
        "Testing TLS, health, AppRole login, and exact-scope metadata access...".to_owned(),
    );
    match backends.remote_status(&request).await {
        Ok(status) => {
            model
                .providers
                .remote_statuses
                .retain(|current| current.instance != status.instance);
            let healthy = status.health.healthy && status.authenticated;
            let connection = status.instance.to_string();
            model.providers.remote_statuses.push(status);
            model.providers.screen = provider::ProviderScreen::Complete(format!(
                "Connection '{connection}' test completed; healthy and authenticated: {healthy}."
            ));
        }
        Err(error) => provider_error(model, error),
    }
}

async fn apply_managed_status(
    model: &mut Model,
    backends: &crate::app::SecretBackendManager,
    provider: crate::domain::secret_backend::ProviderInstanceName,
) {
    model.providers.screen = provider::ProviderScreen::Applying(
        "Refreshing managed health/seal diagnostics...".to_owned(),
    );
    match backends.managed_status(&provider).await {
        Ok(status) => {
            let installed = status.installed;
            let diagnostic_count = status.diagnostics.len();
            model.providers.managed_status = Some(status);
            model.providers.screen = provider::ProviderScreen::Complete(format!(
                "Managed status refreshed; installed: {installed}; diagnostics: {diagnostic_count}."
            ));
        }
        Err(error) => provider_error(model, error),
    }
}

async fn apply_provider_review(
    model: &mut Model,
    backends: &crate::app::SecretBackendManager,
    secrets: &crate::app::SecretManager,
) -> Option<SecretInputRequest> {
    use provider::{ProviderReview, ProviderScreen};

    let review = match &model.providers.screen {
        ProviderScreen::Review(review) => Some(review.as_ref().clone()),
        ProviderScreen::Applying(_) | ProviderScreen::Error(_) | ProviderScreen::Cancelled => {
            model.providers.last_review.clone()
        }
        _ => None,
    };
    let Some(review) = review else {
        model.providers.screen =
            ProviderScreen::Error("No reviewed provider plan is available.".to_owned());
        return None;
    };
    model.providers.last_review = Some(review.clone());
    model.providers.screen = ProviderScreen::Applying(format!("Applying {}...", review.title()));
    match review {
        ProviderReview::ExternalSetup { request, plan } => Some(SecretInputRequest {
            prompt: format!("Provider '{}' developer bootstrap token", request.instance),
            pending: PendingSecretApply::External {
                request: Box::new(request),
                plan_hash: plan.plan_hash.clone(),
            },
        }),
        ProviderReview::Managed { request, plan } => {
            match backends.apply_managed(request, &plan.plan_hash).await {
                Ok(result) => {
                    model.providers.screen = ProviderScreen::Complete(format!(
                        "Managed provider '{}' {} completed for plan {}.",
                        result.instance,
                        result.action.as_str(),
                        result.plan_hash
                    ));
                }
                Err(error) => provider_error(model, error),
            }
            None
        }
        ProviderReview::Data(plan) => {
            match backends.apply_managed_data(&plan).await {
                Ok(result) => {
                    model.providers.screen = ProviderScreen::Complete(format!(
                        "Managed provider '{}' {} completed; encrypted snapshot digest {}.",
                        result.instance,
                        result.action.as_str(),
                        result.snapshot_digest
                    ));
                }
                Err(error) => provider_error(model, error),
            }
            None
        }
        ProviderReview::Rotation(plan) if plan.requires_hidden_input => {
            let prompt = match secrets.rotation_prompt(&model.providers.context.root, &plan.name) {
                Ok(Some(prompt)) => prompt,
                Ok(None) => "Secret replacement".to_owned(),
                Err(error) => {
                    provider_error(model, error);
                    return None;
                }
            };
            Some(SecretInputRequest {
                prompt,
                pending: PendingSecretApply::Rotation {
                    name: plan.name,
                    plan_hash: plan.plan_hash,
                },
            })
        }
        ProviderReview::Rotation(plan) => {
            match secrets
                .apply_direct_rotation(
                    &model.providers.context.root,
                    Some(&model.providers.context.environment),
                    &plan.name,
                    &plan.plan_hash,
                    None,
                )
                .await
            {
                Ok(result) => {
                    model.providers.screen = ProviderScreen::Complete(format!(
                        "Secret '{}' rotated to opaque revision {}.",
                        result.name, result.revision
                    ));
                }
                Err(error) => provider_error(model, error),
            }
            None
        }
    }
}

async fn apply_hidden_secret(
    model: &mut Model,
    pending: PendingSecretApply,
    material: crate::domain::secret::SecretMaterial,
    backends: &crate::app::SecretBackendManager,
    secrets: &crate::app::SecretManager,
    gates: &crate::app::GateManager,
) {
    use provider::ProviderScreen;

    match pending {
        PendingSecretApply::External { request, plan_hash } => {
            match backends
                .apply_remote_setup(*request, &plan_hash, material)
                .await
            {
                Ok(status) => {
                    model
                        .providers
                        .remote_statuses
                        .retain(|current| current.instance != status.instance);
                    let connection = status.instance.to_string();
                    let label = status.driver_display_name.clone();
                    model.providers.remote_statuses.push(status);
                    model.providers.screen = ProviderScreen::Complete(format!(
                        "External {} connection '{connection}' configured and tested.",
                        label
                    ));
                }
                Err(error) => provider_error(model, error),
            }
        }
        PendingSecretApply::Rotation { name, plan_hash } => {
            match secrets
                .apply_direct_rotation(
                    &model.providers.context.root,
                    Some(&model.providers.context.environment),
                    &name,
                    &plan_hash,
                    Some(material),
                )
                .await
            {
                Ok(result) => {
                    model.providers.screen = ProviderScreen::Complete(format!(
                        "Secret '{}' rotated to opaque revision {}.",
                        result.name, result.revision
                    ));
                }
                Err(error) => provider_error(model, error),
            }
        }
        PendingSecretApply::GateUser {
            policy,
            username,
            plan_hash,
        } => {
            match gates
                .apply_user_reviewed(crate::app::ApplyGateUserRequest {
                    project_id: model.snapshot.workspace.project_id,
                    environment: &model.snapshot.environment,
                    policy_name: &policy,
                    username: &username,
                    reviewed_plan_hash: &plan_hash,
                    mutation: crate::app::GateUserMutation::Add(material),
                })
                .await
            {
                Ok(user) => {
                    model.gates.screen = gate::GateScreen::Complete(format!(
                        "Application user '{}' added to policy '{}'.",
                        user.display_username, policy
                    ));
                }
                Err(error) => model.gates.screen = gate::GateScreen::Error(error.to_string()),
            }
        }
        PendingSecretApply::RemoteSecret { .. } => {}
        PendingSecretApply::EnvironmentSecret { name } => {
            let result = secrets
                .set(
                    &model.snapshot.workspace.root,
                    Some(&model.snapshot.environment),
                    &name,
                    material,
                )
                .await;
            control::secret_saved(model, &name, result);
        }
    }
}

fn provider_error(model: &mut Model, error: MmError) {
    model.providers.screen = provider::ProviderScreen::Error(error.to_string());
}
