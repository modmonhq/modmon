use crossterm::event::{KeyEventKind, KeyEventState};

use super::*;
use crate::domain::onboarding::GuidedJourney;
use crate::presentation::tui::control::StackOperation;
use crate::presentation::tui::fixtures::{model, schedule};
use crate::presentation::tui::provider::ProviderAction;

#[test]
fn ignores_non_key_and_non_press_events_and_interrupts_on_control_c() {
    let mut model = model();
    assert_eq!(update(&mut model, Message::Resize), Action::None);
    let release = KeyEvent {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    };
    assert_eq!(update(&mut model, Message::Key(release)), Action::None);
    assert!(!model.should_quit);

    assert_eq!(
        update(
            &mut model,
            Message::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        ),
        Action::None
    );
    assert!(model.should_quit);
    assert!(model.interrupted);
}

#[test]
fn workflow_mount_and_quit_keys_emit_explicit_actions() {
    let mut model = model();
    assert_eq!(
        press(&mut model, KeyCode::Char('w')),
        Action::StartWorkflow(GuidedJourney::ExistingProject)
    );
    assert_eq!(model.tab, Tab::Guided);
    for (code, expected) in [
        (
            KeyCode::Char('g'),
            Action::StartWorkflow(GuidedJourney::GlobalSetup),
        ),
        (
            KeyCode::Char('i'),
            Action::StartWorkflow(GuidedJourney::NewProject),
        ),
        (
            KeyCode::Char('p'),
            Action::StartWorkflow(GuidedJourney::ExistingProject),
        ),
        (
            KeyCode::Char('n'),
            Action::StartWorkflow(GuidedJourney::NodeSetup),
        ),
        (
            KeyCode::Char('e'),
            Action::StartWorkflow(GuidedJourney::ProjectEditing),
        ),
        (KeyCode::Enter, Action::AdvanceWorkflow),
        (KeyCode::Char('b'), Action::BackWorkflow),
        (KeyCode::Char('r'), Action::ResumeWorkflow),
        (KeyCode::Esc, Action::CancelWorkflow),
    ] {
        assert_eq!(press(&mut model, code), expected);
    }

    model.tab = Tab::Mounts;
    assert_eq!(
        press(&mut model, KeyCode::Char('c')),
        Action::ConfigureMount
    );
    assert_eq!(press(&mut model, KeyCode::Char('d')), Action::DisableMount);
    model.tab = Tab::Overview;
    assert_eq!(press(&mut model, KeyCode::F(1)), Action::None);
    assert!(!model.should_quit);
    assert_eq!(press(&mut model, KeyCode::Esc), Action::None);
    assert!(model.should_quit);
}

#[test]
fn provider_and_gate_tabs_delegate_before_global_shortcuts() {
    let mut model = model();
    model.tab = Tab::Providers;
    assert_eq!(
        press(&mut model, KeyCode::Char('x')),
        Action::Provider(Box::new(ProviderAction::None))
    );
    model.tab = Tab::Gates;
    assert_eq!(
        press(&mut model, KeyCode::Char('a')),
        Action::Gate(crate::presentation::tui::gate::GateAction::None)
    );
}

#[test]
fn navigation_shortcuts_reset_scroll_and_follow_sidebar_order() {
    let mut model = model();
    model.scroll = 7;
    press(&mut model, KeyCode::Tab);
    assert_eq!(model.tab, Tab::Environments);
    assert_eq!(model.scroll, 0);
    press(&mut model, KeyCode::Right);
    assert_eq!(model.tab, Tab::Services);
    press(&mut model, KeyCode::Char('l'));
    assert_eq!(
        model.tab,
        Tab::Ingress,
        "with no service to show logs for, l moves on"
    );
    for code in [KeyCode::BackTab, KeyCode::Left, KeyCode::Char('h')] {
        press(&mut model, code);
    }
    assert_eq!(model.tab, Tab::Overview);

    for (character, expected) in [
        ('1', Tab::Overview),
        ('2', Tab::Environments),
        ('3', Tab::Services),
        ('4', Tab::Ingress),
        ('5', Tab::Variables),
        ('6', Tab::Mounts),
        ('7', Tab::Diagnostics),
        ('8', Tab::Tls),
        ('9', Tab::Access),
        ('0', Tab::Gates),
    ] {
        model.tab = Tab::Overview;
        model.scroll = 4;
        press(&mut model, KeyCode::Char(character));
        assert_eq!(model.tab, expected);
        assert_eq!(model.scroll, 0);
    }

    model.tab = Tab::Services;
    press(&mut model, KeyCode::Char(']'));
    assert_eq!(model.tab, Tab::Diagnostics);
    press(&mut model, KeyCode::Char('['));
    assert_eq!(model.tab, Tab::Overview);
}

#[test]
fn help_overlay_captures_keys_until_closed() {
    let mut model = model();
    press(&mut model, KeyCode::Char('?'));
    assert!(model.help);
    press(&mut model, KeyCode::Char('2'));
    assert_eq!(model.tab, Tab::Overview);
    press(&mut model, KeyCode::Esc);
    assert!(!model.help);
    assert!(!model.should_quit);
}

#[test]
fn selection_is_bounded_by_the_current_view() {
    let mut model = model();
    press(&mut model, KeyCode::Down);
    press(&mut model, KeyCode::PageDown);
    assert_eq!(model.scroll, 0);

    model.tab = Tab::Mounts;
    press(&mut model, KeyCode::PageDown);
    assert_eq!(model.scroll, 0);

    model.tab = Tab::Jobs;
    model.jobs_pane = JobsPane::Schedules;
    model.schedules.push(schedule("nightly-two", false));
    press(&mut model, KeyCode::PageDown);
    assert_eq!(model.scroll, 1);
    press(&mut model, KeyCode::Char('j'));
    assert_eq!(model.scroll, 1);
    press(&mut model, KeyCode::Char('k'));
    assert_eq!(model.scroll, 0);
    press(&mut model, KeyCode::End);
    assert_eq!(model.scroll, 1);
    press(&mut model, KeyCode::Home);
    assert_eq!(model.scroll, 0);
}

#[test]
fn job_keys_target_the_selected_item_in_the_focused_pane() {
    let mut model = model();
    model.tab = Tab::Jobs;
    let run_id = model.job_runs[0].id;
    assert_eq!(
        press(&mut model, KeyCode::Char('r')),
        Action::RunJob("cleanup".to_owned())
    );
    assert_eq!(press(&mut model, KeyCode::Char(' ')), Action::None);

    press(&mut model, KeyCode::Char('l'));
    assert_eq!(model.jobs_pane, JobsPane::Schedules);
    assert_eq!(model.tab, Tab::Jobs);
    assert_eq!(
        press(&mut model, KeyCode::Char(' ')),
        Action::ToggleSchedule("nightly".to_owned(), false)
    );
    assert_eq!(
        press(&mut model, KeyCode::Char('n')),
        Action::RunSchedule("nightly".to_owned())
    );

    press(&mut model, KeyCode::Right);
    assert_eq!(model.jobs_pane, JobsPane::Runs);
    assert_eq!(
        press(&mut model, KeyCode::Char('c')),
        Action::CancelJob(run_id)
    );
    assert_eq!(
        press(&mut model, KeyCode::Char('y')),
        Action::RetryJob(run_id)
    );
    assert_eq!(
        press(&mut model, KeyCode::Char('x')),
        Action::ReconcileSchedules
    );
    assert_eq!(press(&mut model, KeyCode::Char('t')), Action::TickScheduler);

    model.scroll = 8;
    for character in ['c', 'y'] {
        assert_eq!(press(&mut model, KeyCode::Char(character)), Action::None);
    }
    press(&mut model, KeyCode::Char('h'));
    assert_eq!(model.jobs_pane, JobsPane::Schedules);
    assert_eq!(model.scroll, 0);
}

#[test]
fn stack_keys_review_start_and_confirm_stop_on_workspace_views() -> Result<(), serde_json::Error> {
    let mut model = crate::presentation::tui::fixtures::rich_model()?;
    for tab in [Tab::Overview, Tab::Services, Tab::Variables] {
        model.tab = tab;
        assert_eq!(
            press(&mut model, KeyCode::Char('u')),
            Action::Stack(StackOperation::Review)
        );
        assert_eq!(press(&mut model, KeyCode::Char('d')), Action::None);
        let confirm = model.confirm.as_ref().map(|confirm| confirm.destructive);
        assert_eq!(confirm, Some(true), "{tab:?} did not ask before stopping");
        assert_eq!(press(&mut model, KeyCode::Char('2')), Action::None);
        assert_eq!(model.tab, tab, "the dialog must capture keys");
        assert_eq!(
            press(&mut model, KeyCode::Enter),
            Action::Stack(StackOperation::Stop)
        );
        assert!(model.confirm.is_none());
    }

    press(&mut model, KeyCode::Char('d'));
    assert_eq!(press(&mut model, KeyCode::Esc), Action::None);
    assert!(model.confirm.is_none());
    assert!(!model.should_quit);

    model.busy = Some(StackOperation::Stop);
    assert_eq!(press(&mut model, KeyCode::Char('u')), Action::None);
    assert!(model.notice.is_some());
    press(&mut model, KeyCode::Char('j'));
    assert!(model.notice.is_none(), "any key dismisses a notice");

    model.busy = None;
    model.tab = Tab::Mounts;
    assert_eq!(press(&mut model, KeyCode::Char('d')), Action::DisableMount);
    Ok(())
}

#[test]
fn environment_search_filters_rows_and_esc_clears_before_quitting() -> Result<(), serde_json::Error>
{
    let mut model = crate::presentation::tui::fixtures::rich_model()?;
    model.tab = Tab::Variables;
    assert_eq!(model.selectable_rows(), 3);

    press(&mut model, KeyCode::Char('/'));
    for character in "sentry".chars() {
        assert_eq!(press(&mut model, KeyCode::Char(character)), Action::None);
    }
    assert_eq!(model.env.filter, "sentry");
    assert_eq!(model.selectable_rows(), 1);
    assert_eq!(model.tab, Tab::Variables, "typing must not switch views");
    press(&mut model, KeyCode::Enter);
    assert!(!model.env.searching);
    assert_eq!(model.selectable_rows(), 1);

    assert_eq!(press(&mut model, KeyCode::Esc), Action::None);
    assert!(!model.should_quit);
    assert!(model.env.filter.is_empty());
    assert_eq!(model.selectable_rows(), 3);

    press(&mut model, KeyCode::Char('p'));
    assert_eq!(model.selectable_rows(), 1);
    press(&mut model, KeyCode::Char('/'));
    press(&mut model, KeyCode::Char('x'));
    assert_eq!(model.selectable_rows(), 0);
    press(&mut model, KeyCode::Esc);
    press(&mut model, KeyCode::Esc);
    assert_eq!(model.selectable_rows(), 3);
    assert!(!model.env.problems_only);

    press(&mut model, KeyCode::Char('/'));
    for character in "resolved db".chars() {
        press(&mut model, KeyCode::Char(character));
    }
    assert_eq!(model.selectable_rows(), 1, "terms combine and match values");
    model.env.filter = "hidden".to_owned();
    assert_eq!(
        model.selectable_rows(),
        0,
        "sensitive values are not searchable"
    );
    Ok(())
}

#[test]
fn environment_edit_targets_global_keys_and_routes_secrets_to_hidden_input()
-> Result<(), serde_json::Error> {
    let mut model = crate::presentation::tui::fixtures::rich_model()?;
    model.tab = Tab::Variables;
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(model.env.edit.is_none());
    assert!(
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("/project/app.env:1"))
    );

    let environment = model
        .environment
        .as_mut()
        .map(|environment| &mut environment.modules[0].services[0].variables);
    if let Some(variables) = environment {
        variables[0].global_key = Some("DATABASE_URL".to_owned());
        variables[1].global_key = Some("API_SECRET".to_owned());
    }
    assert_eq!(press(&mut model, KeyCode::Char('e')), Action::None);
    assert_eq!(
        model
            .env
            .edit
            .as_ref()
            .map(|edit| (edit.value.as_str(), edit.usages)),
        Some(("postgres://db/app", 1))
    );
    press(&mut model, KeyCode::Char('q'));
    assert!(!model.should_quit, "q is text while editing");
    update(
        &mut model,
        Message::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
    );
    for character in "sqlite://".chars() {
        press(&mut model, KeyCode::Char(character));
    }
    assert_eq!(
        press(&mut model, KeyCode::Enter),
        Action::SetOverride {
            key: "DATABASE_URL".to_owned(),
            value: "sqlite://".to_owned(),
        }
    );
    assert!(model.env.edit.is_none());

    press(&mut model, KeyCode::Char('e'));
    assert_eq!(press(&mut model, KeyCode::Esc), Action::None);
    assert!(model.env.edit.is_none());
    assert!(!model.should_quit);

    press(&mut model, KeyCode::Char('x'));
    assert!(
        model.confirm.is_none(),
        "config values have no override to remove"
    );

    press(&mut model, KeyCode::Down);
    assert_eq!(
        press(&mut model, KeyCode::Enter),
        Action::EditSecret("API_SECRET".to_owned())
    );
    Ok(())
}

#[test]
fn removing_an_override_asks_first() -> Result<(), serde_json::Error> {
    let mut model = crate::presentation::tui::fixtures::rich_model()?;
    model.tab = Tab::Variables;
    if let Some(environment) = model.environment.as_mut() {
        let variable = &mut environment.modules[0].services[0].variables[0];
        variable.global_key = Some("DATABASE_URL".to_owned());
        variable.source = crate::domain::environment::ValueSource::SqliteOverride;
    }
    assert_eq!(press(&mut model, KeyCode::Char('x')), Action::None);
    assert_eq!(
        press(&mut model, KeyCode::Char('y')),
        Action::UnsetOverride("DATABASE_URL".to_owned())
    );
    Ok(())
}

#[test]
fn service_rows_nest_containers_and_actions_target_the_selected_row()
-> Result<(), serde_json::Error> {
    use crate::presentation::tui::fixtures::container_id;
    use crate::presentation::tui::panel::Panel;

    let mut model = crate::presentation::tui::fixtures::rich_model()?;
    model.tab = Tab::Services;
    assert_eq!(model.selectable_rows(), 12, "4 services and 8 replicas");

    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    let menu = model.menu.clone();
    assert_eq!(
        menu.as_ref().map(|menu| menu.target.label.as_str()),
        Some("web")
    );
    assert_eq!(menu.map(|menu| menu.items.len()), Some(4));
    assert_eq!(press(&mut model, KeyCode::Char('2')), Action::None);
    assert_eq!(model.tab, Tab::Services, "the menu captures keys");
    press(&mut model, KeyCode::Esc);
    assert!(model.menu.is_none());
    assert!(!model.should_quit);

    let web = ServiceTarget {
        stack: "app".to_owned(),
        service: "web".to_owned(),
        container: None,
        label: "web".to_owned(),
        node: None,
    };
    press(&mut model, KeyCode::Enter);
    assert_eq!(
        press(&mut model, KeyCode::Char('l')),
        Action::FollowLogs {
            session: 1,
            target: web.clone()
        },
        "a menu key runs its action"
    );
    assert!(matches!(model.panel, Some(Panel::Logs(_))));
    assert_eq!(press(&mut model, KeyCode::Char('2')), Action::None);
    assert_eq!(model.tab, Tab::Services, "the panel captures keys");
    assert_eq!(press(&mut model, KeyCode::Char('q')), Action::CloseStream);
    assert!(model.panel.is_none());
    assert!(!model.should_quit);

    // A command runs in one container: the first running one of a service.
    press(&mut model, KeyCode::Char('x'));
    let first = ServiceTarget {
        container: Some(container_id("web", 0)),
        label: "fixture-web-1".to_owned(),
        ..web.clone()
    };
    assert_eq!(
        model.run_prompt.as_ref().map(|prompt| &prompt.target),
        Some(&first)
    );
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(
        model
            .run_prompt
            .as_ref()
            .is_some_and(|prompt| prompt.error.is_some()),
        "an empty command is refused"
    );
    for character in "ls -la 'my dir'".chars() {
        press(&mut model, KeyCode::Char(character));
    }
    assert_eq!(
        press(&mut model, KeyCode::Enter),
        Action::RunCommand {
            session: 2,
            target: first.clone(),
            argv: vec!["ls".to_owned(), "-la".to_owned(), "my dir".to_owned()],
        }
    );
    assert!(matches!(model.panel, Some(Panel::Run(_))));
    assert_eq!(press(&mut model, KeyCode::Esc), Action::CloseStream);

    // The second replica's own row targets that container, and history recalls.
    model.scroll = 2;
    press(&mut model, KeyCode::Char('x'));
    press(&mut model, KeyCode::Up);
    assert_eq!(
        model
            .run_prompt
            .as_ref()
            .map(|prompt| (prompt.input.as_str(), prompt.target.label.as_str())),
        Some(("ls -la 'my dir'", "fixture-web-2"))
    );
    press(&mut model, KeyCode::Esc);
    assert!(model.run_prompt.is_none());

    model.scroll = 0;
    assert_eq!(press(&mut model, KeyCode::Char('r')), Action::None);
    assert_eq!(
        model.confirm.as_ref().map(|confirm| confirm.destructive),
        Some(true)
    );
    assert_eq!(press(&mut model, KeyCode::Enter), Action::Restart(web));

    press(&mut model, KeyCode::Char(' '));
    assert_eq!(model.selectable_rows(), 10, "web is folded");
    model.scroll = 0;
    press(&mut model, KeyCode::Char(' '));
    assert_eq!(model.selectable_rows(), 12);
    model.scroll = 2;
    press(&mut model, KeyCode::Char(' '));
    assert_eq!(
        model.scroll, 0,
        "folding from a replica selects its service"
    );

    press(&mut model, KeyCode::Char('v'));
    assert_eq!(model.tab, Tab::Variables);
    assert!(model.env.filter.is_empty());
    assert!(
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text == "web declares no variables.")
    );
    Ok(())
}

#[test]
fn remote_environments_explain_what_they_cannot_do() -> Result<(), serde_json::Error> {
    let mut model = crate::presentation::tui::fixtures::remote_model()?;
    model.tab = Tab::Services;
    let noticed = |model: &Model, text: &str| {
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains(text))
    };

    assert_eq!(press(&mut model, KeyCode::Char('r')), Action::None);
    assert!(model.confirm.is_some(), "a remote restart should ask first");
    model.confirm = None;

    assert_eq!(press(&mut model, KeyCode::Char('d')), Action::None);
    assert!(model.confirm.is_none(), "stop must not ask and then fail");
    assert!(noticed(&model, "prod-eu"));

    // The host grants exec on api only: web is refused, api is prompted.
    assert_eq!(press(&mut model, KeyCode::Char('x')), Action::None);
    assert!(model.run_prompt.is_none());
    assert!(noticed(&model, "only on api"));
    model.scroll = 3;
    press(&mut model, KeyCode::Char('x'));
    assert!(model.run_prompt.is_some());
    press(&mut model, KeyCode::Esc);

    press(&mut model, KeyCode::Enter);
    let disabled = model.menu.as_ref().map(|menu| {
        menu.items
            .iter()
            .map(|item| item.disabled.is_some())
            .collect::<Vec<_>>()
    });
    assert_eq!(disabled, Some(vec![false, false, false, false]));
    press(&mut model, KeyCode::Esc);
    assert!(matches!(
        press(&mut model, KeyCode::Char('l')),
        Action::FollowLogs { .. }
    ));
    press(&mut model, KeyCode::Esc);

    model.tab = Tab::Variables;
    model.scroll = 0;
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(noticed(&model, "no global key"));
    if let Some(environment) = model.environment.as_mut() {
        environment.modules[0].services[0].variables[0].global_key =
            Some("DATABASE_URL".to_owned());
    }
    assert_eq!(
        press(&mut model, KeyCode::Enter),
        Action::OpenEditor(EditorOpen {
            kind: EditorKind::Binding,
            action: ConfigMutationAction::Edit,
            name: Some("DATABASE_URL".to_owned()),
            parent: Some("production".to_owned()),
        }),
        "a remote value is set in the configuration, not as a local override"
    );
    assert!(model.env.edit.is_none());
    assert_eq!(press(&mut model, KeyCode::Char('x')), Action::None);
    assert!(noticed(&model, "do not reach 'prod-eu'"));
    assert_eq!(
        press(&mut model, KeyCode::Char('u')),
        Action::Stack(StackOperation::Review),
        "u still reviews a deploy"
    );
    Ok(())
}

#[test]
fn environments_are_checked_on_open_and_switched_with_enter() -> Result<(), serde_json::Error> {
    let mut model = crate::presentation::tui::fixtures::rich_model()?;
    assert_eq!(
        press(&mut model, KeyCode::Char('2')),
        Action::CheckEnvironments(vec!["staging".to_owned(), "production".to_owned()]),
        "the current and the unresolvable environment are not checked"
    );
    assert_eq!(model.tab, Tab::Environments);
    assert_eq!(press(&mut model, KeyCode::Char('r')), Action::None);

    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("already on development"))
    );
    // An environment that is not set up opens too, on what it needs.
    press(&mut model, KeyCode::End);
    let unresolved = model.environments.entries[model.scroll].clone();
    assert!(unresolved.problem.is_some());
    assert_eq!(
        press(&mut model, KeyCode::Enter),
        Action::SwitchEnvironment(unresolved.name)
    );
    assert_eq!(press(&mut model, KeyCode::Esc), Action::CancelSwitch);

    model.scroll = 1;
    assert_eq!(
        press(&mut model, KeyCode::Enter),
        Action::SwitchEnvironment("staging".to_owned())
    );
    assert_eq!(press(&mut model, KeyCode::Char('q')), Action::None);
    assert!(!model.should_quit, "a switch in progress captures keys");
    assert_eq!(press(&mut model, KeyCode::Esc), Action::CancelSwitch);
    assert!(model.environments.switching.is_none());
    Ok(())
}

#[test]
fn add_edit_and_remove_keys_open_the_editor_for_the_selected_resource()
-> Result<(), serde_json::Error> {
    let open = |kind, action, name: Option<&str>| {
        Action::OpenEditor(EditorOpen {
            kind,
            action,
            name: name.map(str::to_owned),
            parent: None,
        })
    };
    let mut model = crate::presentation::tui::fixtures::remote_model()?;

    model.tab = Tab::Environments;
    model.scroll = 1;
    for (key, expected) in [
        (
            'a',
            open(EditorKind::Environment, ConfigMutationAction::Add, None),
        ),
        (
            'e',
            open(
                EditorKind::Environment,
                ConfigMutationAction::Edit,
                Some("staging"),
            ),
        ),
        (
            'D',
            open(
                EditorKind::Environment,
                ConfigMutationAction::Remove,
                Some("staging"),
            ),
        ),
    ] {
        assert_eq!(press(&mut model, KeyCode::Char(key)), expected);
    }

    model.tab = Tab::Access;
    model.scroll = 0;
    assert_eq!(
        press(&mut model, KeyCode::Char('e')),
        Action::None,
        "nobody is selected in an empty team"
    );
    assert_eq!(
        press(&mut model, KeyCode::Char('j')),
        open(EditorKind::Join, ConfigMutationAction::Add, None)
    );
    model.access.team = crate::presentation::tui::fixtures::team();
    assert_eq!(model.selectable_rows(), 3);
    model.scroll = 1;
    assert_eq!(
        press(&mut model, KeyCode::Char('a')),
        open(EditorKind::Teammate, ConfigMutationAction::Add, None)
    );
    assert_eq!(
        press(&mut model, KeyCode::Char('D')),
        open(
            EditorKind::Teammate,
            ConfigMutationAction::Remove,
            Some("bob")
        )
    );
    assert_eq!(press(&mut model, KeyCode::Char('j')), Action::None);
    assert!(
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("already in the users file as alice"))
    );

    model.tab = Tab::Jobs;
    model.scroll = 0;
    assert_eq!(
        press(&mut model, KeyCode::Char('e')),
        open(EditorKind::Job, ConfigMutationAction::Edit, Some("cleanup"))
    );
    press(&mut model, KeyCode::Char('l'));
    assert_eq!(
        press(&mut model, KeyCode::Char('D')),
        open(
            EditorKind::Schedule,
            ConfigMutationAction::Remove,
            Some("nightly")
        )
    );
    press(&mut model, KeyCode::Char('l'));
    assert_eq!(
        press(&mut model, KeyCode::Char('a')),
        Action::None,
        "runs are history, not configuration"
    );
    Ok(())
}

#[test]
fn the_editor_captures_keys_through_form_review_and_done() -> Result<(), String> {
    use crate::presentation::tui::editor::{Editor, EditorContext, EditorStage};

    let mut model = model();
    let context = EditorContext::from_model(&model);
    model.editor = Some(Editor::new(
        EditorOpen {
            kind: EditorKind::Environment,
            action: ConfigMutationAction::Add,
            name: None,
            parent: None,
        },
        None,
        &context,
    ));
    for character in "qa2".chars() {
        assert_eq!(press(&mut model, KeyCode::Char(character)), Action::None);
    }
    assert!(!model.should_quit);
    assert_eq!(model.tab, Tab::Overview);
    let value = |model: &Model, index: usize| {
        model
            .editor
            .as_ref()
            .map(|editor| editor.fields[index].value.clone())
    };
    assert_eq!(value(&model, 0).as_deref(), Some("qa2"));

    press(&mut model, KeyCode::Tab);
    press(&mut model, KeyCode::Tab);
    assert_eq!(value(&model, 2).as_deref(), Some("project default"));
    press(&mut model, KeyCode::Char('x'));
    assert_eq!(
        value(&model, 2).as_deref(),
        Some("project default"),
        "a choice ignores typing"
    );
    press(&mut model, KeyCode::Right);
    assert_eq!(value(&model, 2).as_deref(), Some("podman-compose"));
    press(&mut model, KeyCode::Left);
    press(&mut model, KeyCode::Left);
    assert_eq!(value(&model, 2).as_deref(), Some("docker-compose"));
    press(&mut model, KeyCode::BackTab);
    assert_eq!(model.editor.as_ref().map(|editor| editor.focus), Some(1));
    assert_eq!(press(&mut model, KeyCode::Enter), Action::EditorPlan);

    let request = model
        .editor
        .as_ref()
        .ok_or("the editor closed")?
        .request()?;
    if let Some(editor) = model.editor.as_mut() {
        editor.stage = EditorStage::Review {
            plan: Box::new(crate::domain::onboarding::ConfigMutationPlan {
                request,
                affected_files: vec![".config/mm.yaml".into()],
                canonical_diff: "+  qa2:\n".to_owned(),
                generated_resources: Vec::new(),
                generated_routes: Vec::new(),
                privilege_prompts: Vec::new(),
                source_hash: "source".to_owned(),
                result_hash: "result".to_owned(),
                plan_hash: "plan".to_owned(),
            }),
            scroll: 0,
        };
    }
    assert_eq!(press(&mut model, KeyCode::Char('2')), Action::None);
    assert_eq!(press(&mut model, KeyCode::Char('b')), Action::None);
    assert!(matches!(
        model.editor.as_ref().map(|editor| &editor.stage),
        Some(EditorStage::Form)
    ));
    if let Some(editor) = model.editor.as_mut() {
        editor.stage = EditorStage::Done {
            headline: "Added environment 'qa2'.".to_owned(),
            steps: vec!["Commit it.".to_owned()],
        };
    }
    assert_eq!(press(&mut model, KeyCode::Char('x')), Action::None);
    assert!(model.editor.is_some());
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(model.editor.is_none());

    model.editor = Some(Editor::new(
        EditorOpen {
            kind: EditorKind::Job,
            action: ConfigMutationAction::Add,
            name: None,
            parent: None,
        },
        None,
        &context,
    ));
    assert_eq!(press(&mut model, KeyCode::Esc), Action::None);
    assert!(model.editor.is_none());
    assert!(!model.should_quit);
    Ok(())
}

#[test]
fn setup_steps_are_selected_and_run_from_the_overview() {
    use crate::presentation::tui::fixtures::setup_view;

    let mut model = model();
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(model.notice.is_none(), "nothing to run without setup steps");

    model.setup = setup_view();
    assert_eq!(press(&mut model, KeyCode::Enter), Action::RunSetup(0));
    press(&mut model, KeyCode::Char('j'));
    press(&mut model, KeyCode::Char('j'));
    assert_eq!(model.scroll, 1, "selection stops at the last step");
    assert_eq!(press(&mut model, KeyCode::Enter), Action::RunSetup(1));

    model.setup.steps[1].ready = false;
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(notice_text(&model).contains("Do those first"));

    model.setup.steps[1].ready = true;
    model.setup.steps[1].issue = None;
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(notice_text(&model).contains("mm dev services plan"));

    model.busy = Some(StackOperation::Stop);
    model.scroll = 0;
    assert_eq!(press(&mut model, KeyCode::Enter), Action::None);
    assert!(notice_text(&model).contains("still running"));
}

#[test]
fn start_points_at_unfinished_setup_on_this_machine_only() -> Result<(), serde_json::Error> {
    use crate::presentation::tui::fixtures::{remote_model, setup_view};

    let mut model = model();
    model.setup = setup_view();
    assert_eq!(press(&mut model, KeyCode::Char('u')), Action::None);
    assert!(notice_text(&model).contains("2 setup steps are left on the Overview"));
    // Leaving for another environment is never held back by setup.
    press(&mut model, KeyCode::Char('2'));
    assert_eq!(model.tab, Tab::Environments);

    let mut remote = remote_model()?;
    remote.tab = Tab::Overview;
    remote.setup = setup_view();
    assert_eq!(
        press(&mut remote, KeyCode::Char('u')),
        Action::Stack(StackOperation::Review)
    );
    Ok(())
}

#[test]
fn cancelling_a_reload_is_quiet_and_clears_it() {
    let mut model = model();
    model.environments.switching = Some("development".to_owned());
    model.environments.reloading = true;
    assert_eq!(press(&mut model, KeyCode::Esc), Action::CancelSwitch);
    assert!(model.notice.is_none());
    assert!(!model.environments.reloading);
    assert!(model.environments.switching.is_none());
}

fn notice_text(model: &Model) -> &str {
    model
        .notice
        .as_ref()
        .map_or("", |notice| notice.text.as_str())
}

fn press(model: &mut Model, code: KeyCode) -> Action {
    update(model, Message::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}
