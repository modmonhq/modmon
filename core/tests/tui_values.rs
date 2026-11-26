//! The Variables view of a remote environment: what each host holds
//! and has staged, by key, never a value, and the keys that stage a change.

use mm::domain::staged::{
    HostValue, NodeValues, RemoteValues, StagedRevision, ValueKind, ValueOrigin, ValueRevision,
};
use mm::domain::workspace::WorkspaceSnapshot;
use mm::presentation::tui::driver::ScriptedUi;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn snapshot() -> Result<WorkspaceSnapshot, serde_json::Error> {
    serde_json::from_value(serde_json::json!({
        "workspace": {
            "id": "00000000-0000-0000-0000-000000000009",
            "project_id": "00000000-0000-0000-0000-000000000007",
            "root": "/work/shop", "config_path": "/work/shop/.config/mm.yaml"
        },
        "project_name": "shop", "target": "cluster", "environment": "uat",
        "config_hash": "a", "target_source": null, "runtime_kind": "docker-compose",
        "runtime_source": null,
        "runtime": { "kind": "docker-compose", "available": true, "engine_version": "Docker version 29.0.0",
                     "compose_provider": null, "rootless": null, "socket_path": null,
                     "capabilities": [], "diagnostics": [] },
        "validation": { "diagnostics": [] },
        "stacks": [], "ingress": [], "recent_operations": [], "nodes": [],
        "names": null, "deployment": null
    }))
}

fn value(key: &str, kind: ValueKind, staged: Option<&str>) -> HostValue {
    HostValue {
        key: key.to_owned(),
        kind,
        origin: Some(ValueOrigin::Staged),
        provider: None,
        services: vec!["web".to_owned()],
        required: true,
        active: Some(ValueRevision {
            revision: "3f9a1c2e7b4d".to_owned(),
            changed_by: Some("alice".to_owned()),
            changed_at_unix_ms: Some(1),
        }),
        staged: staged.map(|revision| StagedRevision {
            revision: revision.to_owned(),
            removal: false,
            staged_by: "bob".to_owned(),
            staged_at_unix_ms: 2,
        }),
        defaulted: false,
    }
}

fn values() -> RemoteValues {
    let node = |name: &str| NodeValues {
        node: Some(name.to_owned()),
        commit: "c".repeat(40),
        values: vec![
            value("web.app-key", ValueKind::Secret, Some("5f1c9e2a0000")),
            value("APP.FLAGS", ValueKind::Variable, None),
        ],
    };
    RemoteValues {
        target: "cluster".to_owned(),
        environment: "uat".to_owned(),
        nodes: vec![node("app-1"), node("app-2")],
        unreachable: Vec::new(),
    }
}

#[test]
fn the_view_lists_what_the_hosts_hold_and_what_is_staged() -> TestResult {
    let mut ui = ScriptedUi::new(snapshot()?, Some("cluster"));
    ui.host_values(values());
    ui.press("5");
    let screen = ui.screen(140, 40)?;
    for part in [
        "web.app-key",
        "APP.FLAGS",
        "3f9a1c2e",
        "5f1c9e2a by bob",
        "1 staged",
        "on the hosts",
        "app-1",
        "app-2",
    ] {
        assert!(screen.contains(part), "{part} missing:\n{screen}");
    }
    assert!(!screen.contains("Variables are unavailable"), "{screen}");
    Ok(())
}

#[test]
fn enter_reads_a_secret_hidden_x_stages_a_removal_and_shift_x_discards() -> TestResult {
    let mut ui = ScriptedUi::new(snapshot()?, Some("cluster"));
    ui.host_values(values());
    ui.press("5");
    ui.press("/");
    for key in "app-key".chars() {
        ui.press(&key.to_string());
    }
    ui.press("enter");
    assert_eq!(ui.press("enter"), "EditRemoteSecret(\"web.app-key\")");
    assert_eq!(ui.press("X"), "Stage(Discard(Secret, \"web.app-key\"))");
    assert_eq!(ui.press("x"), "None");
    let screen = ui.screen(140, 40)?;
    assert!(
        screen.contains("Stage the removal of web.app-key"),
        "{screen}"
    );
    Ok(())
}

#[test]
fn a_secret_its_templates_default_reads_as_default() -> TestResult {
    let mut values = values();
    for node in &mut values.nodes {
        let secret = &mut node.values[0];
        secret.origin = None;
        secret.active = None;
        secret.staged = None;
        secret.required = false;
        secret.defaulted = true;
    }
    let card = mm::presentation::output::remote_values_card(&values, None)
        .lines(mm::presentation::inline::Palette::plain(), 140)
        .iter()
        .map(mm::presentation::inline::plain)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(card.contains("default: nothing live"), "{card}");
    assert!(!card.contains("unset"), "{card}");

    let mut ui = ScriptedUi::new(snapshot()?, Some("cluster"));
    ui.host_values(values);
    ui.press("5");
    let screen = ui.screen(140, 40)?;
    assert!(screen.contains("default"), "{screen}");
    Ok(())
}
