use super::*;

fn context(remote: &[&str]) -> EditorContext {
    EditorContext {
        remote_environments: remote.iter().map(|name| (*name).to_owned()).collect(),
        jobs: vec!["cleanup".to_owned()],
        device_key: None,
        user: None,
    }
}

fn open(kind: EditorKind, action: ConfigMutationAction, name: Option<&str>) -> EditorOpen {
    EditorOpen {
        kind,
        action,
        name: name.map(str::to_owned),
        parent: None,
    }
}

fn set(editor: &mut Editor, id: &str, value: &str) {
    if let Some(field) = editor.fields.iter_mut().find(|field| field.id == id) {
        value.clone_into(&mut field.value);
    }
}

const KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGJ5dGVzLW9mLWEtdGVzdC1rZXktMDEyMzQ1Njc4OWFi alice";

#[test]
fn an_edit_keeps_the_keys_the_form_does_not_show() -> Result<(), String> {
    let base = json!({
        "target": "prod-eu",
        "runtime": { "driver": "docker-compose" },
        "deploy-from": ["main"],
        "bindings": { "LOG_LEVEL": { "value": "info" } },
        "tls": { "certificate": "wildcard" }
    });
    let mut editor = Editor::new(
        open(
            EditorKind::Environment,
            ConfigMutationAction::Edit,
            Some("production"),
        ),
        Some(base),
        &context(&[]),
    );
    assert_eq!(editor.value("deploy-from"), "main");
    assert_eq!(editor.value("runtime"), "docker-compose");
    set(&mut editor, "deploy-from", "main, release/* ,");
    set(&mut editor, "runtime", PROJECT_RUNTIME);
    set(&mut editor, "target", "");
    let request = editor.request()?;
    assert_eq!(request.name, "production");
    assert_eq!(
        request.definition,
        Some(json!({
            "deploy-from": ["main", "release/*"],
            "bindings": { "LOG_LEVEL": { "value": "info" } },
            "tls": { "certificate": "wildcard" }
        }))
    );
    Ok(())
}

#[test]
fn a_new_teammate_needs_a_name_a_key_and_a_level() -> Result<(), String> {
    let mut editor = Editor::new(
        open(EditorKind::Teammate, ConfigMutationAction::Add, None),
        None,
        &context(&["staging", "production"]),
    );
    assert_eq!(
        editor.request(),
        Err("Name is required.".to_owned()),
        "{:?}",
        editor.fields
    );
    set(&mut editor, "name", "Alice");
    set(&mut editor, "key", "ssh-rsa AAAA");
    assert!(editor.request().is_err_and(|error| error.contains("level")));
    set(&mut editor, "access:staging", "deploy");
    assert!(
        editor
            .request()
            .is_err_and(|error| error.contains("ssh-rsa"))
    );
    set(&mut editor, "key", KEY);
    assert!(
        editor
            .request()
            .is_err_and(|error| error.contains("user name 'Alice'"))
    );
    set(&mut editor, "name", "alice");
    assert_eq!(
        editor.request(),
        UserProposal {
            name: "alice".to_owned(),
            key: KEY.to_owned(),
            machine: false,
            access: vec![("staging".to_owned(), AccessLevel::Deploy)],
        }
        .request(),
        "the form builds the proposal `mm team add` builds"
    );
    let request = editor.request()?;
    assert_eq!(request.resource, ConfigResourceKind::User);
    assert_eq!(request.name, "alice");
    assert_eq!(
        request.definition,
        Some(json!({ "keys": [KEY], "access": { "staging": "deploy" } }))
    );
    let steps = editor.next_steps(&request, ".config/mm/users.yaml", None);
    assert!(steps[0].contains(".config/mm/users.yaml"));
    assert!(steps[2].starts_with("alice opens"));
    Ok(())
}

#[test]
fn changing_a_level_keeps_keys_and_add_ons_and_none_removes_the_entry() -> Result<(), String> {
    let base = json!({
        "keys": [KEY],
        "access": {
            "staging": "view",
            "production": { "level": "deploy", "exec": ["api"] }
        }
    });
    let mut editor = Editor::new(
        open(
            EditorKind::Teammate,
            ConfigMutationAction::Edit,
            Some("alice"),
        ),
        Some(base),
        &context(&["staging", "production"]),
    );
    assert!(editor.fields.iter().all(|field| field.id != "key"));
    assert_eq!(editor.value("access:production"), "deploy");
    set(&mut editor, "access:staging", "none");
    set(&mut editor, "access:production", "admin");
    assert_eq!(
        editor.request()?.definition,
        Some(json!({
            "keys": [KEY],
            "access": { "production": { "level": "admin", "exec": ["api"] } }
        }))
    );
    Ok(())
}

#[test]
fn joining_uses_this_devices_key_and_the_operators_name() -> Result<(), String> {
    let mut context = context(&["staging"]);
    context.device_key = Some(KEY.to_owned());
    context.user = Some("bob".to_owned());
    let mut editor = Editor::new(
        open(EditorKind::Join, ConfigMutationAction::Add, None),
        None,
        &context,
    );
    assert_eq!(editor.value("name"), "bob");
    set(&mut editor, "access:staging", "view");
    let request = editor.request()?;
    assert_eq!(request.name, "bob");
    assert_eq!(
        request.definition,
        Some(json!({ "keys": [KEY], "access": { "staging": "view" } }))
    );

    let keyless = Editor::new(
        open(EditorKind::Join, ConfigMutationAction::Add, None),
        None,
        &super::tests::context(&[]),
    );
    assert!(
        keyless
            .fields
            .iter()
            .any(|field| field.label == "Everywhere")
    );
    Ok(())
}

const PROJECT: &str = "schema-version: 1
default-environment: local
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: editor-test
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    services:
      api:
        stack: app
environments:
  local:
    bindings: {}
  # where the team tests
  staging:
    deploy-from: [main] # protected
    bindings:
      LOG_LEVEL: { value: info }
";

#[test]
fn reviewed_edits_are_written_to_the_configuration_and_nothing_else_is()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = crate::test_support::tempdir()?;
    let root = directory.path();
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::create_dir_all(root.join("api"))?;
    std::fs::write(
        root.join("compose.yaml"),
        "services:\n  api:\n    image: nginx:1.27\n",
    )?;
    let config = root.join(".config/mm.yaml");
    std::fs::write(&config, PROJECT)?;
    let onboarding = crate::composition::onboarding_service();
    let mut model = crate::presentation::tui::fixtures::model();
    model.snapshot.workspace.root = root.to_path_buf();

    // The form starts from the environment as it is written, comments aside.
    let base =
        onboarding.current_definition(root, ConfigResourceKind::Environment, "staging", None)?;
    assert_eq!(
        base,
        Some(json!({
            "deploy-from": ["main"],
            "bindings": { "LOG_LEVEL": { "value": "info" } }
        }))
    );
    assert_eq!(
        onboarding.current_definition(root, ConfigResourceKind::User, "alice", None)?,
        None
    );

    let mut editor = Editor::new(
        open(EditorKind::Teammate, ConfigMutationAction::Add, None),
        None,
        &context(&["staging"]),
    );
    set(&mut editor, "name", "alice");
    set(&mut editor, "key", KEY);
    set(&mut editor, "access:staging", "deploy");
    model.editor = Some(editor);
    plan(&mut model, &onboarding);
    let Some(EditorStage::Review { plan: reviewed, .. }) =
        model.editor.as_ref().map(|editor| &editor.stage)
    else {
        return Err(format!("no review: {:?}", model.editor).into());
    };
    assert!(
        reviewed.canonical_diff.contains("+users:"),
        "{}",
        reviewed.canonical_diff
    );
    assert_eq!(
        std::fs::read_to_string(&config)?,
        PROJECT,
        "a plan writes nothing"
    );

    assert!(apply(&mut model, &onboarding));
    let Some(EditorStage::Done { headline, steps }) =
        model.editor.as_ref().map(|editor| &editor.stage)
    else {
        return Err(format!("not done: {:?}", model.editor).into());
    };
    assert_eq!(headline, "Added teammate 'alice' in .config/mm.yaml.");
    assert_eq!(steps.len(), 3);
    let written = std::fs::read_to_string(&config)?;
    assert!(written.contains("alice:"), "{written}");
    assert!(written.contains(KEY), "{written}");
    assert!(
        written.contains("# where the team tests"),
        "comments survive: {written}"
    );

    // A change that cannot be planned stays on the form with the reason.
    let mut editor = Editor::new(
        EditorOpen {
            parent: Some("staging".to_owned()),
            ..open(
                EditorKind::Binding,
                ConfigMutationAction::Edit,
                Some("LOG_LEVEL"),
            )
        },
        onboarding.current_definition(
            root,
            ConfigResourceKind::Binding,
            "LOG_LEVEL",
            Some("staging"),
        )?,
        &context(&[]),
    );
    assert_eq!(editor.value("value"), "info");
    set(&mut editor, "kind", "secret");
    set(&mut editor, "value", "");
    model.editor = Some(editor);
    plan(&mut model, &onboarding);
    assert!(
        model.editor.as_ref().is_some_and(
            |editor| matches!(editor.stage, EditorStage::Form) && editor.error.is_some()
        ),
        "{:?}",
        model.editor
    );

    // A plain value on a local environment is saved with a notice, no checklist.
    if let Some(editor) = model.editor.as_mut() {
        set(editor, "kind", "value");
        set(editor, "value", "debug");
    }
    plan(&mut model, &onboarding);
    assert!(apply(&mut model, &onboarding));
    assert!(model.editor.is_none());
    assert!(model.env.pending_restart);
    assert!(
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("Saved value 'LOG_LEVEL'"))
    );
    let written = std::fs::read_to_string(&config)?;
    assert!(written.contains("debug"), "{written}");
    assert!(
        written.contains("deploy-from: [main] # protected"),
        "{written}"
    );

    // Removing asks for review of the exact lines that go.
    model.editor = Some(Editor::new(
        open(
            EditorKind::Teammate,
            ConfigMutationAction::Remove,
            Some("alice"),
        ),
        onboarding.current_definition(root, ConfigResourceKind::User, "alice", None)?,
        &context(&["staging"]),
    ));
    plan(&mut model, &onboarding);
    assert!(apply(&mut model, &onboarding));
    assert!(!std::fs::read_to_string(&config)?.contains("alice"));
    Ok(())
}

#[test]
fn jobs_schedules_and_values_build_their_definitions() -> Result<(), String> {
    let mut job = Editor::new(
        open(EditorKind::Job, ConfigMutationAction::Add, None),
        None,
        &context(&[]),
    );
    set(&mut job, "name", "migrate");
    set(&mut job, "module", "backend");
    set(&mut job, "service", "api");
    set(&mut job, "command", "bin/rails 'db:migrate' --trace");
    let request = job.request()?;
    assert_eq!(request.name, "migrate");
    assert_eq!(
        request.definition,
        Some(json!({
            "module": "backend",
            "service": "api",
            "command": ["bin/rails", "db:migrate", "--trace"]
        }))
    );
    let existing = Editor::new(
        open(EditorKind::Job, ConfigMutationAction::Edit, Some("say")),
        Some(json!({ "module": "m", "service": "s", "command": ["echo", "two words", "it's"] })),
        &context(&[]),
    );
    assert_eq!(
        split_command(existing.value("command")),
        Ok(vec![
            "echo".to_owned(),
            "two words".to_owned(),
            "it's".to_owned()
        ])
    );

    let mut schedule = Editor::new(
        open(EditorKind::Schedule, ConfigMutationAction::Add, None),
        None,
        &context(&[]),
    );
    assert_eq!(
        schedule.value("job"),
        "cleanup",
        "the only job is preselected"
    );
    set(&mut schedule, "name", "nightly");
    set(&mut schedule, "cron", "0 3 * * *");
    set(&mut schedule, "enabled", NO);
    assert_eq!(
        schedule.request()?.definition,
        Some(json!({ "job": "cleanup", "cron": "0 3 * * *", "enabled": false }))
    );

    let mut value = Editor::new(
        EditorOpen {
            parent: Some("staging".to_owned()),
            ..open(
                EditorKind::Binding,
                ConfigMutationAction::Edit,
                Some("LOG_LEVEL"),
            )
        },
        Some(json!({ "value": "info" })),
        &context(&[]),
    );
    assert_eq!(value.title(), "Set LOG_LEVEL for staging");
    set(&mut value, "value", "debug");
    let request = value.request()?;
    assert_eq!(request.parent.as_deref(), Some("staging"));
    assert_eq!(request.definition, Some(json!({ "value": "debug" })));
    set(&mut value, "kind", "secret");
    set(&mut value, "value", "log.level");
    assert_eq!(
        value.request()?.definition,
        Some(json!({ "secret": "log.level" }))
    );
    Ok(())
}
