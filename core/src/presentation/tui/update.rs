use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::domain::onboarding::ConfigMutationAction;
use crate::presentation::tui::control::StackOperation;
use crate::presentation::tui::editor::{self, EditorKind, EditorOpen};
use crate::presentation::tui::model::{
    Confirm, EnvironmentEdit, JobsPane, Model, Notice, ServiceRow, Tab, VariablesPane, source_label,
};
use crate::presentation::tui::ops::ServiceTarget;
use crate::presentation::tui::panel::{self, ServiceAction};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Key(KeyEvent),
    Resize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    ConfigureMount,
    DisableMount,
    StartWorkflow(crate::domain::onboarding::GuidedJourney),
    AdvanceWorkflow,
    BackWorkflow,
    CancelWorkflow,
    ResumeWorkflow,
    Provider(Box<super::provider::ProviderAction>),
    Gate(super::gate::GateAction),
    RunJob(String),
    RunSchedule(String),
    ToggleSchedule(String, bool),
    CancelJob(uuid::Uuid),
    RetryJob(uuid::Uuid),
    ReconcileSchedules,
    TickScheduler,
    Stack(StackOperation),
    SetOverride {
        key: String,
        value: String,
    },
    UnsetOverride(String),
    EditSecret(String),
    /// Read a secret's value hidden and stage it on a remote environment's hosts.
    EditRemoteSecret(String),
    /// Stage a change on a remote environment's hosts.
    Stage(super::host_values::StageRequest),
    Refresh,
    /// Follow a service's logs in the open log view.
    FollowLogs {
        session: u64,
        target: ServiceTarget,
    },
    /// Run a command in a service's container and stream its output.
    RunCommand {
        session: u64,
        target: ServiceTarget,
        argv: Vec<String>,
    },
    /// Stop the log or command stream of a closed view.
    CloseStream,
    Restart(ServiceTarget),
    /// Check the status of other environments.
    CheckEnvironments(Vec<String>),
    SwitchEnvironment(String),
    CancelSwitch,
    /// Open the configuration form for a resource.
    OpenEditor(EditorOpen),
    /// Plan the form's change and show it for review.
    EditorPlan,
    /// Apply exactly the reviewed change.
    EditorApply,
    /// Run the fix for the setup step at this position in the plain terminal.
    RunSetup(usize),
    /// Turn an optional unit on or off for this machine.
    DevelopmentUnit {
        unit: String,
        enable: bool,
    },
}

pub fn update(model: &mut Model, message: Message) -> Action {
    let Message::Key(key) = message else {
        model.clamp_selection();
        return Action::None;
    };
    if key.kind != KeyEventKind::Press {
        return Action::None;
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        model.should_quit = true;
        model.interrupted = true;
        return Action::None;
    }
    let dismissing = model.notice.take().is_some() && key.code == KeyCode::Esc;
    if model.confirm.is_some() {
        return confirm_key(model, key.code);
    }
    if model.environments.switching.is_some() {
        return switching_key(model, key.code);
    }
    if model.editor.is_some() {
        return editor::key(model, key);
    }
    if let Some(action) = panel::typing_key(model, key) {
        return action;
    }
    if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
        model.notice = Some(Notice::info("Refreshing status..."));
        return Action::Refresh;
    }
    if model.tab == Tab::Variables
        && let Some(action) = super::env_input::typing_key(model, key)
    {
        return action;
    }
    if let Some(action) = overlay_key(model, key, dismissing) {
        return action;
    }
    if model.tab == Tab::Providers
        && let Some(action) = super::provider::update(&mut model.providers, key)
    {
        return Action::Provider(Box::new(action));
    }
    if model.tab == Tab::Gates
        && let Some(action) = super::gate::update(&mut model.gates, key)
    {
        return Action::Gate(action);
    }
    if let Some(action) = tab_action(model, key.code) {
        return action;
    }

    let previous = model.tab;
    match key.code {
        // Leaving would stop a deployment half way, with its nodes locked.
        KeyCode::Esc | KeyCode::Char('q')
            if !dismissing && matches!(model.busy, Some(StackOperation::Start(_))) =>
        {
            model.notice = Some(Notice::info(
                "A deployment is being applied; leaving now leaves it unfinished. Wait for it to end, or press ctrl-c to leave anyway.",
            ));
        }
        KeyCode::Esc | KeyCode::Char('q') => model.should_quit = !dismissing,
        KeyCode::Char('?') => model.help = true,
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => select_tab(model, model.tab.next()),
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
            select_tab(model, model.tab.previous());
        }
        KeyCode::Char(']') => select_tab(model, model.tab.next_group()),
        KeyCode::Char('[') => select_tab(model, model.tab.previous_group()),
        KeyCode::Down | KeyCode::Char('j') => move_selection(model, 1),
        KeyCode::Up | KeyCode::Char('k') => move_selection(model, -1),
        KeyCode::PageDown => move_selection(model, 10),
        KeyCode::PageUp => move_selection(model, -10),
        KeyCode::Home => model.scroll = 0,
        KeyCode::End => model.scroll = model.selectable_rows().saturating_sub(1),
        KeyCode::Char('w') => {
            select_tab(model, Tab::Guided);
            return Action::StartWorkflow(
                crate::domain::onboarding::GuidedJourney::ExistingProject,
            );
        }
        KeyCode::Char(digit) => {
            if let Some(tab) = Tab::from_digit(digit) {
                select_tab(model, tab);
            }
        }
        _ => {}
    }
    if model.tab == Tab::Environments && previous != Tab::Environments {
        return check_environments(model, false);
    }
    Action::None
}

/// Keys for whatever lies over the view and takes every key: the help, a
/// review, the action menu, or a log or command.
fn overlay_key(model: &mut Model, key: KeyEvent, dismissing: bool) -> Option<Action> {
    if model.help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc => model.help = false,
            KeyCode::Char('q') => model.should_quit = true,
            _ => {}
        }
        return Some(Action::None);
    }
    if model.review.as_ref().is_some_and(|review| review.shown) {
        // `esc` on a message only dismisses it, here too.
        return Some(if dismissing {
            Action::None
        } else {
            super::review::key(model, key)
        });
    }
    if model.menu.is_some() {
        return Some(panel::menu_key(model, key.code));
    }
    if model.panel.is_some() {
        return Some(panel::panel_key(model, key.code));
    }
    None
}

/// While an environment loads, only `esc` does anything: it cancels the load.
fn switching_key(model: &mut Model, code: KeyCode) -> Action {
    if code != KeyCode::Esc {
        return Action::None;
    }
    model.environments.switching = None;
    if !std::mem::take(&mut model.environments.reloading) {
        model.notice = Some(Notice::info("Switch cancelled."));
    }
    Action::CancelSwitch
}

/// Checks the other environments; `force` repeats checks that are still fresh.
fn check_environments(model: &mut Model, force: bool) -> Action {
    match super::environments::begin_checks(model, force) {
        environments if environments.is_empty() => Action::None,
        environments => Action::CheckEnvironments(environments),
    }
}

/// `a` adds, `e` edits, and `D` removes the selected resource, the same in every view.
fn edit_keys(code: KeyCode, kind: EditorKind, selected: Option<String>) -> Option<Action> {
    let action = match code {
        KeyCode::Char('a') => {
            return Some(Action::OpenEditor(EditorOpen {
                kind,
                action: ConfigMutationAction::Add,
                name: None,
                parent: None,
            }));
        }
        KeyCode::Char('e') => ConfigMutationAction::Edit,
        KeyCode::Char('D') => ConfigMutationAction::Remove,
        _ => return None,
    };
    Some(selected.map_or(Action::None, |name| {
        Action::OpenEditor(EditorOpen {
            kind,
            action,
            name: Some(name),
            parent: None,
        })
    }))
}

fn access_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    if code == KeyCode::Char('j') {
        if let Some(member) = model.access.team.iter().find(|member| member.you) {
            model.notice = Some(Notice::info(format!(
                "This device is already in the users file as {}.",
                member.name
            )));
            return Some(Action::None);
        }
        return Some(Action::OpenEditor(EditorOpen {
            kind: EditorKind::Join,
            action: ConfigMutationAction::Add,
            name: None,
            parent: None,
        }));
    }
    let selected = model
        .access
        .team
        .get(model.scroll)
        .map(|member| member.name.clone());
    edit_keys(code, EditorKind::Teammate, selected)
}

/// Opens the configuration value of the selected variable's key for this environment.
fn edit_config_value(model: &mut Model, action: ConfigMutationAction) -> Action {
    let Some(row) = model.selected_environment_row() else {
        return Action::None;
    };
    let variable = row.variable;
    let Some(key) = variable.global_key.clone() else {
        let location = &variable.location;
        model.notice = Some(Notice::info(format!(
            "{} has no global key, so it can only be changed in its template ({}:{}).",
            variable.name,
            location.path.display(),
            location.line
        )));
        return Action::None;
    };
    Action::OpenEditor(EditorOpen {
        kind: EditorKind::Binding,
        action,
        name: Some(key),
        parent: Some(model.snapshot.environment.clone()),
    })
}

fn environments_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    let selected = model
        .environments
        .entries
        .get(model.scroll)
        .map(|entry| entry.name.clone());
    if let Some(action) = edit_keys(code, EditorKind::Environment, selected) {
        return Some(action);
    }
    match code {
        KeyCode::Char('r') => Some(check_environments(model, true)),
        KeyCode::Enter => {
            let entry = model.environments.entries.get(model.scroll)?.clone();
            if entry.name == model.snapshot.environment {
                model.notice = Some(Notice::info(format!("You are already on {}.", entry.name)));
                return Some(Action::None);
            }
            // An environment that is not set up opens on what it needs.
            if let Some(busy) = &model.busy {
                model.notice = Some(Notice::info(format!(
                    "{} is still running; wait for it to finish.",
                    busy.progress(model.remote().is_some())
                )));
                return Some(Action::None);
            }
            model.environments.switching = Some(entry.name.clone());
            Some(Action::SwitchEnvironment(entry.name))
        }
        _ => None,
    }
}

fn services_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    let action = match code {
        KeyCode::Enter => {
            model.menu = panel::menu_for_selection(model);
            return Some(Action::None);
        }
        KeyCode::Char(' ') => {
            toggle_service_fold(model);
            return Some(Action::None);
        }
        KeyCode::Char('o') => return Some(toggle_optional(model)),
        KeyCode::Char('l') => ServiceAction::Logs,
        KeyCode::Char('x') => ServiceAction::Run,
        KeyCode::Char('r') => ServiceAction::Restart,
        KeyCode::Char('v') => ServiceAction::Variables,
        _ => return None,
    };
    let target = model.selected_service_target()?;
    Some(panel::start(model, action, target))
}

/// Turns the selected optional unit on, or the optional unit of the selected
/// service off.
fn toggle_optional(model: &mut Model) -> Action {
    match model.selected_optional() {
        Some((unit, on)) => Action::DevelopmentUnit {
            unit: unit.name.clone(),
            enable: !on,
        },
        None => {
            model.notice = Some(Notice::info(
                "Only a unit with `development: optional` turns on and off here.",
            ));
            Action::None
        }
    }
}

/// Folds or unfolds the containers of the selected service; from a container
/// row the selection moves up to its service.
fn toggle_service_fold(model: &mut Model) {
    let Some(row) = model.selected_service_row() else {
        return;
    };
    let service = row.service();
    if !super::service_nodes::foldable(service) {
        return;
    }
    let key = (service.stack.clone(), service.name.clone());
    let parent = model.service_rows().iter().position(|candidate| {
        matches!(candidate, ServiceRow::Service { service, .. }
            if service.stack == key.0 && service.name == key.1)
    });
    if !model.collapsed.remove(&key) {
        model.collapsed.insert(key);
    }
    if let Some(parent) = parent {
        model.scroll = parent;
    }
}

fn confirm_key(model: &mut Model, code: KeyCode) -> Action {
    match code {
        KeyCode::Enter | KeyCode::Char('y') => model
            .confirm
            .take()
            .map_or(Action::None, |confirm| confirm.action),
        KeyCode::Esc | KeyCode::Char('n' | 'q') => {
            model.confirm = None;
            Action::None
        }
        _ => Action::None,
    }
}

/// Start and stop are offered on the views that show what they change.
fn stack_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    let operation = match code {
        KeyCode::Char('u') => StackOperation::Review,
        KeyCode::Char('d') => StackOperation::Stop,
        KeyCode::Char('b') if model.remote().is_some() => StackOperation::Rollback,
        _ => return None,
    };
    // While a plan is applied, `u` shows where it is.
    if operation == StackOperation::Review && super::review::show(model) {
        return Some(Action::None);
    }
    if operation == StackOperation::Review && super::setup::blocks_start(model) {
        return Some(Action::None);
    }
    if let Some(busy) = &model.busy {
        model.notice = Some(Notice::info(format!(
            "{} is still running; wait for it to finish.",
            busy.progress(model.remote().is_some())
        )));
        return Some(Action::None);
    }
    if operation == StackOperation::Stop {
        if let Some(remote) = model.remote() {
            model.notice = Some(Notice::info(format!(
                "Stopping is not available on remote targets; stop the stack on '{remote}' itself."
            )));
            return Some(Action::None);
        }
        model.confirm = Some(stop_confirmation(model));
        return Some(Action::None);
    }
    Some(Action::Stack(operation))
}

fn stop_confirmation(model: &Model) -> Confirm {
    let mut stacks = model
        .snapshot
        .stacks
        .iter()
        .map(|stack| stack.name.as_str())
        .collect::<Vec<_>>();
    stacks.dedup();
    let stacks = if stacks.is_empty() {
        "every configured stack".to_owned()
    } else {
        stacks.join(", ")
    };
    Confirm {
        title: "Stop the stack?".to_owned(),
        lines: vec![
            format!(
                "Removes the containers of {stacks} on target '{}'.",
                model.snapshot.target
            ),
            "Volumes, routes, overrides, and secrets are kept; press u to start again.".to_owned(),
        ],
        accept: "stop",
        destructive: true,
        action: Action::Stack(StackOperation::Stop),
    }
}

fn environment_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    if let Some(action) = super::variables::pane_key(model, code) {
        return Some(action);
    }
    match code {
        KeyCode::Char('/') => {
            model.env.searching = true;
            model.env.pane = VariablesPane::Variables;
            Some(Action::None)
        }
        KeyCode::Char('p') => {
            model.refilter(|env| env.problems_only = !env.problems_only);
            Some(Action::None)
        }
        KeyCode::Esc if !model.env.filter.is_empty() || model.env.problems_only => {
            model.refilter(|env| {
                env.filter.clear();
                env.problems_only = false;
            });
            Some(Action::None)
        }
        code if model.host_values().is_some() => super::host_values::key_action(model, code),
        // A remote environment has no local overrides: its values live in the configuration.
        KeyCode::Enter | KeyCode::Char('e') if model.remote().is_some() => {
            Some(edit_config_value(model, ConfigMutationAction::Edit))
        }
        KeyCode::Enter | KeyCode::Char('e') => Some(edit_selected_variable(model)),
        KeyCode::Char('c') => Some(edit_config_value(model, ConfigMutationAction::Edit)),
        KeyCode::Char('D') => Some(edit_config_value(model, ConfigMutationAction::Remove)),
        KeyCode::Char('x') => Some(remove_selected_override(model)),
        _ => None,
    }
}

/// Remote environments read values from the deployed commit and the host's
/// secret store, so a local override would change nothing there.
fn remote_values_notice(model: &mut Model) -> bool {
    let Some(remote) = model.remote() else {
        return false;
    };
    model.notice = Some(Notice::info(format!(
        "Local overrides do not reach '{remote}'. Press enter to set the value in the configuration."
    )));
    true
}

fn edit_selected_variable(model: &mut Model) -> Action {
    if remote_values_notice(model) {
        return Action::None;
    }
    let Some(row) = model.selected_environment_row() else {
        return Action::None;
    };
    let variable = row.variable;
    let Some(key) = variable.global_key.clone() else {
        let location = &variable.location;
        model.notice = Some(Notice::info(format!(
            "{} has no global key, so it can only be changed in its template ({}:{}).",
            variable.name,
            location.path.display(),
            location.line
        )));
        return Action::None;
    };
    if variable.sensitive {
        return Action::EditSecret(key);
    }
    let value = variable.value.clone().unwrap_or_default();
    let usages = model
        .all_environment_rows()
        .iter()
        .filter(|row| row.variable.global_key.as_deref() == Some(key.as_str()))
        .count();
    model.env.edit = Some(EnvironmentEdit { key, value, usages });
    Action::None
}

fn remove_selected_override(model: &mut Model) -> Action {
    if remote_values_notice(model) {
        return Action::None;
    }
    let Some(row) = model.selected_environment_row() else {
        return Action::None;
    };
    let variable = row.variable;
    let Some(key) = variable
        .global_key
        .clone()
        .filter(|_| variable.source == crate::domain::environment::ValueSource::SqliteOverride)
    else {
        model.notice = Some(Notice::info(format!(
            "{} comes from {}, not a local override; nothing to remove.",
            variable.name,
            source_label(variable.source)
        )));
        return Action::None;
    };
    model.confirm = Some(Confirm {
        title: format!("Remove the local override for {key}?"),
        lines: vec![
            "The value falls back to config, the template default, or unset.".to_owned(),
            "Running containers keep the old value until the stack restarts.".to_owned(),
        ],
        accept: "remove",
        destructive: true,
        action: Action::UnsetOverride(key),
    });
    Action::None
}

fn tab_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    if matches!(model.tab, Tab::Overview | Tab::Services | Tab::Variables)
        && let Some(action) = stack_action(model, code)
    {
        return Some(action);
    }
    match model.tab {
        Tab::Overview if code == KeyCode::Enter => super::setup::run_selected(model),
        Tab::Environments => environments_action(model, code),
        Tab::Access => access_action(model, code),
        Tab::Services => services_action(model, code),
        Tab::Variables => environment_action(model, code),
        Tab::Jobs => job_action(model, code),
        Tab::Guided => guided_action(code),
        Tab::Mounts => match code {
            KeyCode::Char('c') => Some(Action::ConfigureMount),
            KeyCode::Char('d') => Some(Action::DisableMount),
            _ => None,
        },
        _ => None,
    }
}

fn job_action(model: &mut Model, code: KeyCode) -> Option<Action> {
    let pane = model.jobs_pane;
    let edited = match pane {
        JobsPane::Jobs => edit_keys(
            code,
            EditorKind::Job,
            model.jobs.jobs.keys().nth(model.scroll).cloned(),
        ),
        JobsPane::Schedules => edit_keys(
            code,
            EditorKind::Schedule,
            model
                .schedules
                .get(model.scroll)
                .map(|schedule| schedule.name.clone()),
        ),
        JobsPane::Runs => None,
    };
    if edited.is_some() {
        return edited;
    }
    match code {
        KeyCode::Right | KeyCode::Char('l') => {
            model.jobs_pane = pane.next();
            model.scroll = 0;
            Some(Action::None)
        }
        KeyCode::Left | KeyCode::Char('h') => {
            model.jobs_pane = pane.previous();
            model.scroll = 0;
            Some(Action::None)
        }
        KeyCode::Char('r') if pane == JobsPane::Jobs => model
            .jobs
            .jobs
            .keys()
            .nth(model.scroll)
            .cloned()
            .map(Action::RunJob),
        KeyCode::Char(' ') if pane == JobsPane::Schedules => model
            .schedules
            .get(model.scroll)
            .map(|schedule| Action::ToggleSchedule(schedule.name.clone(), !schedule.enabled)),
        KeyCode::Char('n') if pane == JobsPane::Schedules => model
            .schedules
            .get(model.scroll)
            .map(|schedule| Action::RunSchedule(schedule.name.clone())),
        KeyCode::Char('c') if pane == JobsPane::Runs => model
            .job_runs
            .get(model.scroll)
            .map(|run| Action::CancelJob(run.id)),
        KeyCode::Char('y') if pane == JobsPane::Runs => model
            .job_runs
            .get(model.scroll)
            .map(|run| Action::RetryJob(run.id)),
        KeyCode::Char('x') => Some(Action::ReconcileSchedules),
        KeyCode::Char('t') => Some(Action::TickScheduler),
        _ => None,
    }
}

fn guided_action(code: KeyCode) -> Option<Action> {
    use crate::domain::onboarding::GuidedJourney;
    match code {
        KeyCode::Esc => Some(Action::CancelWorkflow),
        KeyCode::Char('g') => Some(Action::StartWorkflow(GuidedJourney::GlobalSetup)),
        KeyCode::Char('i') => Some(Action::StartWorkflow(GuidedJourney::NewProject)),
        KeyCode::Char('p') => Some(Action::StartWorkflow(GuidedJourney::ExistingProject)),
        KeyCode::Char('n') => Some(Action::StartWorkflow(GuidedJourney::NodeSetup)),
        KeyCode::Char('e') => Some(Action::StartWorkflow(GuidedJourney::ProjectEditing)),
        KeyCode::Enter => Some(Action::AdvanceWorkflow),
        KeyCode::Char('b') => Some(Action::BackWorkflow),
        KeyCode::Char('r') => Some(Action::ResumeWorkflow),
        _ => None,
    }
}

fn move_selection(model: &mut Model, delta: isize) {
    model.scroll = model.scroll.saturating_add_signed(delta);
    model.clamp_selection();
}

fn select_tab(model: &mut Model, tab: Tab) {
    model.tab = tab;
    model.scroll = 0;
}

#[cfg(test)]
#[path = "update/tests.rs"]
mod tests;
