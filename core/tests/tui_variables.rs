//! The terminal UI's Variables view, driven in-process: a list of services
//! filters the table, search looks across every service with the chosen
//! one's matches first, and every action reaches the variable it was
//! pressed on, from the filtered view and from a search.

use mm::domain::environment::EnvironmentStatusSnapshot;
use mm::domain::workspace::WorkspaceSnapshot;
use mm::presentation::tui::driver::ScriptedUi;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn snapshot() -> Result<WorkspaceSnapshot, serde_json::Error> {
    let service = |name: &str| {
        serde_json::json!({
            "stack": "app", "name": name, "module": "shop", "state": "running",
            "desired_count": 1, "running_count": 1, "healthy_count": 1, "containers": []
        })
    };
    serde_json::from_value(serde_json::json!({
        "workspace": {
            "id": "00000000-0000-0000-0000-000000000009",
            "project_id": "00000000-0000-0000-0000-000000000007",
            "root": "/work/shop", "config_path": "/work/shop/.config/mm.yaml"
        },
        "project_name": "shop", "target": "local", "environment": "development",
        "config_hash": "a", "target_source": null, "runtime_kind": "docker-compose",
        "runtime_source": null,
        "runtime": { "kind": "docker-compose", "available": true, "engine_version": "Docker version 29.0.0",
                     "compose_provider": null, "rootless": null, "socket_path": null,
                     "capabilities": [], "diagnostics": [] },
        "validation": { "diagnostics": [] },
        "stacks": [{
            "name": "app", "compose_project_name": "shop-development-app", "artifact_hash": "b",
            "rendered_config_hash": "c",
            "services": [service("api"), service("worker"), service("web")]
        }],
        "ingress": [], "recent_operations": [], "nodes": [], "names": null, "deployment": null
    }))
}

fn variables() -> Result<EnvironmentStatusSnapshot, serde_json::Error> {
    let variable =
        |name: &str, source: &str, status: &str, value: Option<&str>, sensitive: bool| {
            serde_json::json!({
                "name": name, "global_key": name, "required": true, "has_default": false,
                "sensitive": sensitive, "source": source, "status": status, "value": value,
                "location": { "path": "/work/shop/app.env", "line": 1, "column": 1 }
            })
        };
    serde_json::from_value(serde_json::json!({
        "project_name": "shop", "environment": "development",
        "config_hash": "config-hash", "input_hash": "input-hash", "diagnostics": [],
        "modules": [
            { "module": "backend", "template_path": "/work/shop/app.env", "services": [
                { "stack": "app", "service": "api", "variables": [
                    variable("DATABASE_URL", "config_value", "resolved", Some("postgres://db/shop"), false),
                    variable("API_SECRET", "secret", "resolved", Some("hidden-value"), true),
                    variable("SENTRY_DSN", "missing", "unset", None, false),
                ]},
                { "stack": "app", "service": "worker", "variables": [
                    variable("DATABASE_URL", "config_value", "resolved", Some("postgres://db/shop"), false),
                    variable("QUEUE_URL", "config_value", "resolved", Some("redis://queue"), false),
                    variable("SENTRY_URL", "missing", "unset", None, false),
                ]}
            ]},
            { "module": "frontend", "template_path": "/work/shop/web.env", "services": [
                { "stack": "app", "service": "web", "variables": [
                    variable("PUBLIC_API_URL", "config_value", "resolved", Some("https://api.shop.test"), false),
                    variable("SESSION_KEY", "secret", "resolved", Some("hidden-session"), true),
                ]},
                { "stack": "app", "service": "notifications-dispatcher", "variables": [
                    variable("SMTP_HOST", "template_default", "defaulted", Some("mailpit"), false),
                ]}
            ]}
        ]
    }))
}

fn variables_view() -> Result<ScriptedUi, serde_json::Error> {
    let mut ui = ScriptedUi::new(snapshot()?, None);
    ui.show_variables(variables()?);
    ui.press("5");
    Ok(ui)
}

fn has(screen: &str, markers: &[&str]) {
    for marker in markers {
        assert!(screen.contains(marker), "missing {marker:?} in:\n{screen}");
    }
}

fn lacks(screen: &str, markers: &[&str]) {
    for marker in markers {
        assert!(
            !screen.contains(marker),
            "unexpected {marker:?} in:\n{screen}"
        );
    }
}

fn before(screen: &str, first: &str, second: &str) {
    let first_at = screen.find(first);
    let second_at = screen.find(second);
    assert!(
        first_at.is_some() && second_at.is_some() && first_at < second_at,
        "{first:?} should come before {second:?} in:\n{screen}"
    );
}

fn typed(ui: &mut ScriptedUi, text: &str) {
    for character in text.chars() {
        let key = character.to_string();
        assert_eq!(ui.press(&key), "None");
    }
}

#[test]
fn the_service_list_filters_the_table_to_the_chosen_service() -> TestResult {
    let mut ui = variables_view()?;
    let screen = ui.screen(120, 40)?;
    has(
        &screen,
        &[
            "SERVICES",
            "api",
            "worker",
            "web",
            "DATABASE_URL",
            "API_SECRET",
            "SENTRY_DSN",
            "9 variables",
            "2 problems",
            "h services",
        ],
    );
    lacks(&screen, &["QUEUE_URL", "PUBLIC_API_URL"]);

    assert_eq!(ui.press("h"), "None");
    has(&ui.screen(120, 40)?, &["j/k service", "l variables"]);
    assert_eq!(ui.press("j"), "None");
    let screen = ui.screen(120, 40)?;
    has(&screen, &["QUEUE_URL", "SENTRY_URL", "DATABASE_URL"]);
    lacks(&screen, &["API_SECRET", "SENTRY_DSN"]);
    assert_eq!(ui.press("j"), "None");
    assert_eq!(ui.press("enter"), "None");
    assert_eq!(ui.press("j"), "None");
    let screen = ui.screen(120, 40)?;
    has(&screen, &["PUBLIC_API_URL", "SESSION_KEY · web"]);
    assert_eq!(ui.press("enter"), "EditSecret(\"SESSION_KEY\")");
    Ok(())
}

#[test]
fn search_looks_at_every_service_and_lists_the_chosen_one_first() -> TestResult {
    let mut ui = variables_view()?;
    ui.press("h");
    ui.press("j");
    ui.press("/");
    typed(&mut ui, "sentry");
    let screen = ui.screen(120, 40)?;
    has(&screen, &["2 of 9 variables", "SENTRY_URL · worker"]);
    before(&screen, "SENTRY_URL", "SENTRY_DSN");
    lacks(&screen, &["QUEUE_URL", "PUBLIC_API_URL"]);

    ui.press("esc");
    ui.press("h");
    ui.press("k");
    ui.press("/");
    typed(&mut ui, "sentry");
    before(&ui.screen(120, 40)?, "SENTRY_DSN", "SENTRY_URL");

    ui.press("esc");
    ui.press("/");
    typed(&mut ui, "hidden");
    has(&ui.screen(120, 40)?, &["No variables match"]);

    ui.press("esc");
    ui.press("p");
    let screen = ui.screen(120, 40)?;
    has(&screen, &["problems only", "SENTRY_DSN", "SENTRY_URL"]);
    lacks(&screen, &["DATABASE_URL", "QUEUE_URL"]);
    Ok(())
}

#[test]
fn actions_from_a_search_reach_the_variable_of_the_service_it_belongs_to() -> TestResult {
    let mut ui = variables_view()?;
    ui.press("/");
    typed(&mut ui, "queue");
    assert_eq!(ui.press("enter"), "None");
    assert_eq!(ui.press("enter"), "None");
    has(&ui.screen(120, 40)?, &["Set override", "redis://queue"]);
    ui.press("2");
    assert_eq!(
        ui.press("enter"),
        "SetOverride { key: \"QUEUE_URL\", value: \"redis://queue2\" }"
    );
    assert!(ui.press("c").starts_with(
        "OpenEditor(EditorOpen { kind: Binding, action: Edit, name: Some(\"QUEUE_URL\")"
    ));

    ui.press("esc");
    ui.press("/");
    typed(&mut ui, "session");
    ui.press("enter");
    assert_eq!(ui.press("enter"), "EditSecret(\"SESSION_KEY\")");
    Ok(())
}

#[test]
fn leaving_a_search_keeps_the_selected_variable_in_its_own_service() -> TestResult {
    let mut ui = variables_view()?;
    ui.press("/");
    typed(&mut ui, "queue");
    ui.press("esc");
    let screen = ui.screen(120, 40)?;
    has(
        &screen,
        &["QUEUE_URL · worker", "SENTRY_URL", "search all services"],
    );
    lacks(&screen, &["API_SECRET"]);
    assert_eq!(
        ui.press("enter"),
        "None",
        "enter opens the override of the kept selection"
    );
    has(&ui.screen(120, 40)?, &["redis://queue"]);
    Ok(())
}

#[test]
fn the_services_view_opens_the_variables_of_its_selected_service() -> TestResult {
    let mut ui = ScriptedUi::new(snapshot()?, None);
    ui.show_variables(variables()?);
    ui.press("3");
    ui.press("j");
    assert_eq!(ui.press("v"), "None");
    let screen = ui.screen(120, 40)?;
    has(&screen, &["QUEUE_URL"]);
    lacks(&screen, &["API_SECRET"]);
    Ok(())
}

#[test]
fn narrow_terminals_shorten_service_names_and_never_wrap_them() -> TestResult {
    let mut ui = variables_view()?;
    has(
        &ui.screen(170, 48)?,
        &["notifications-dispatcher", "SOURCE"],
    );
    for (width, height) in [(120, 40), (80, 24), (60, 20)] {
        let screen = ui.screen(width, height)?;
        has(&screen, &["notifi", "…", "VARIABLE", "DATABASE_URL"]);
        lacks(&screen, &["notifications-dispatcher"]);
        assert_eq!(screen.contains("SOURCE"), width >= 120, "{width}x{height}");
        assert!(
            !screen
                .lines()
                .any(|line| line.trim_start().starts_with("dispatcher")),
            "a service name wrapped at {width}x{height}:\n{screen}"
        );
    }
    ui.press("/");
    typed(&mut ui, "url");
    let screen = ui.screen(80, 24)?;
    has(&screen, &["api", "worker", "QUEUE_URL"]);
    Ok(())
}
