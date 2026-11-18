//! The terminal UI on a remote environment and on a cluster, driven
//! in-process: the deploy review shows the plan and each node while it is
//! applied, a rollback is planned from the views, the nodes of a service are
//! rows to act on, an action pressed while the session is read goes ahead
//! once it is, and what belongs to this machine stays out of the view.

use mm::domain::rollout::{ProgressStep, RolloutProgress};
use mm::domain::service_stack::ServiceStackStatus;
use mm::domain::workspace::WorkspaceSnapshot;
use mm::presentation::tui::driver::ScriptedUi;
use mm::presentation::tui::{Loaded, StartReview};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NODES: [&str; 3] = ["app-1", "app-2", "app-3"];

/// The status of `production` on a cluster of three nodes: `web` on every
/// node, `api` on app-1. `pending` names a node that has not answered yet.
fn cluster(pending: Option<&str>) -> Result<WorkspaceSnapshot, serde_json::Error> {
    let answered = |node: &str| Some(node) != pending;
    let container = |service: &str, node: &str| {
        serde_json::json!({
            "id": format!("{service}-{node}"), "name": format!("shop-production-app-{service}-1"),
            "service": service, "image": "busybox:1.36.1", "state": "running",
            "status": "Up 2 minutes", "health": null, "labels": {}, "node": node
        })
    };
    let share = |node: &str| {
        let up = u32::from(answered(node));
        serde_json::json!({
            "node": node, "state": if up == 1 { "running" } else { "unknown" },
            "desired_count": 1, "running_count": up, "healthy_count": up
        })
    };
    let running = u32::try_from(NODES.iter().filter(|node| answered(node)).count()).unwrap_or(0);
    serde_json::from_value(serde_json::json!({
        "workspace": {
            "id": "00000000-0000-0000-0000-000000000009",
            "project_id": "00000000-0000-0000-0000-000000000007",
            "root": "/work/shop", "config_path": "/work/shop/.config/mm.yaml"
        },
        "project_name": "shop", "target": "cluster", "environment": "production",
        "config_hash": "a", "target_source": null, "runtime_kind": "docker-compose",
        "runtime_source": null,
        "runtime": { "kind": "docker-compose", "available": true, "engine_version": "Docker version 29.0.0",
                     "compose_provider": null, "rootless": null, "socket_path": null,
                     "capabilities": [], "diagnostics": [] },
        "validation": { "diagnostics": [] },
        "stacks": [{
            "name": "app", "compose_project_name": "shop-production-app", "artifact_hash": "b",
            "rendered_config_hash": "c",
            "services": [
                { "stack": "app", "name": "web", "module": "shop",
                  "state": if running == 3 { "running" } else { "degraded" },
                  "desired_count": 3, "running_count": running, "healthy_count": running,
                  "nodes": NODES.iter().map(|node| share(node)).collect::<Vec<_>>(),
                  "containers": NODES.iter().filter(|node| answered(node))
                      .map(|node| container("web", node)).collect::<Vec<_>>() },
                { "stack": "app", "name": "api", "module": "shop", "state": "running",
                  "desired_count": 1, "running_count": 1, "healthy_count": 1,
                  "nodes": [share("app-1")], "containers": [container("api", "app-1")] }
            ]
        }],
        "ingress": [], "recent_operations": [],
        "nodes": NODES.iter().map(|node| serde_json::json!({
            "name": node, "reachable": answered(node), "problem": null,
            "deployment": null, "drift": [], "network_drift": []
        })).collect::<Vec<_>>(),
        "names": null, "deployment": null
    }))
}

fn review(rollback: Option<&str>) -> StartReview {
    StartReview {
        plan_hash: "8a41c0de91f2a7b3".repeat(4),
        target: "cluster".to_owned(),
        stacks: vec!["app".to_owned()],
        routes: 0,
        remote: true,
        nodes: NODES.map(str::to_owned).to_vec(),
        lines: vec![
            "Cluster deployment plan 8a41c0de91f2a7b3".to_owned(),
            "  target:   cluster (app-1, app-2, app-3)".to_owned(),
            "  moves:".to_owned(),
            "    app/worker: starts on app-2, then stops on app-1".to_owned(),
            "  rollout: app on app-1, then app-2, then app-3".to_owned(),
        ],
        rollback: rollback.map(str::to_owned),
    }
}

fn has(screen: &str, markers: &[&str]) {
    for marker in markers {
        assert!(screen.contains(marker), "missing {marker:?} in:\n{screen}");
    }
}

#[test]
fn the_deploy_review_shows_the_plan_and_where_each_node_is_while_it_applies() -> TestResult {
    let mut ui = ScriptedUi::new(cluster(None)?, Some("cluster"));
    assert_eq!(ui.press("3"), "None");
    assert_eq!(ui.press("u"), "Stack(Review)");
    ui.reviewed(review(None));
    has(
        &ui.screen(140, 40)?,
        &[
            "Deploy to production?",
            "app on target 'cluster' (app-1, app-2, app-3)",
            "0 routes · plan 8a41c0de91f2",
            "The plan as `mm deploy plan` prints it:",
            "app/worker: starts on app-2, then stops on app-1",
            "rollout: app on app-1, then app-2, then app-3",
            "enter deploy",
        ],
    );
    assert!(
        ui.press("enter")
            .starts_with("Stack(Start(\"8a41c0de91f2a7b3")
    );
    let step = |node: &str, step| RolloutProgress::new(node, step);
    ui.progress(step("app-1", ProgressStep::Preparing));
    ui.progress(step("app-1", ProgressStep::Stack("app".to_owned())));
    has(
        &ui.screen(140, 40)?,
        &[
            "Deploying to production",
            "app-1  applies app",
            "app-2  waiting",
        ],
    );
    ui.progress(step("app-1", ProgressStep::Healthy("app".to_owned())));
    ui.progress(step("app-2", ProgressStep::Preparing));
    has(
        &ui.screen(140, 40)?,
        &[
            "app-1  waits for its next turn   stacks applied: app",
            "app-2  preparing",
            "app-3  waiting",
        ],
    );
    // esc hides it while the apply goes on; u shows it again.
    assert_eq!(ui.press("esc"), "None");
    has(&ui.screen(140, 40)?, &["Deploying…", "STATE"]);
    assert_eq!(ui.press("u"), "None");
    has(&ui.screen(140, 40)?, &["app-2  preparing"]);
    ui.applied(Ok(
        "Applied 8a41c0de91f2 on 3 node(s) of cluster.".to_owned()
    ));
    has(
        &ui.screen(140, 40)?,
        &[
            "app-1  done",
            "app-2  done",
            "Applied 8a41c0de91f2",
            "enter close",
        ],
    );
    ui.press("enter");
    has(&ui.screen(140, 40)?, &["STATE", "SERVICE"]);
    Ok(())
}

#[test]
fn a_failed_apply_says_why_with_the_code_and_what_each_node_runs() -> TestResult {
    let mut ui = ScriptedUi::new(cluster(None)?, Some("cluster"));
    ui.press("3");
    ui.press("u");
    ui.reviewed(review(None));
    ui.press("enter");
    ui.progress(RolloutProgress::new("app-1", ProgressStep::Preparing));
    ui.progress(RolloutProgress::new("app-1", ProgressStep::RollingBack));
    ui.applied(Err(
        "error[MM_APPLY_STOPPED]: the deployment stopped at node app-1".to_owned(),
    ));
    has(
        &ui.screen(140, 40)?,
        &[
            "app-1  runs what it ran before",
            "app-2  waiting",
            "error[MM_APPLY_STOPPED]: the deployment stopped at node app-1",
        ],
    );
    // The review says it; the footer keeps its keys.
    assert_eq!(ui.notice(), None);
    has(&ui.screen(140, 40)?, &["stopped", "enter close"]);
    Ok(())
}

#[test]
fn b_plans_a_rollback_of_a_remote_environment_and_nothing_on_this_machine() -> TestResult {
    let mut remote = ScriptedUi::new(cluster(None)?, Some("cluster"));
    remote.press("3");
    has(&remote.screen(140, 40)?, &["u deploy", "b roll back"]);
    assert_eq!(remote.press("b"), "Stack(Rollback)");
    remote.reviewed(review(Some(
        "Rollback: undoes the deployment of commit 3f2c9a1e5b7d; returns app-1, app-2, app-3 to commit 41c07de92ab3.",
    )));
    has(
        &remote.screen(140, 40)?,
        &[
            "Roll back production?",
            "Rollback: undoes the deployment of commit 3f2c9a1e5b7d",
            "The plan as `mm deploy rollback` prints it:",
            "enter roll back",
        ],
    );
    assert_eq!(remote.press("esc"), "None");

    let mut local = ScriptedUi::new(cluster(None)?, None);
    local.press("3");
    assert_eq!(local.press("b"), "None");
    Ok(())
}

#[test]
fn the_nodes_of_a_service_on_a_cluster_are_rows_to_act_on() -> TestResult {
    let mut ui = ScriptedUi::new(cluster(None)?, Some("cluster"));
    ui.press("3");
    // web, then its three nodes, then api.
    ui.press("j");
    has(
        &ui.screen(140, 40)?,
        &["Node      app-1", "Status    Up 2 minutes"],
    );
    let logs = ui.press("l");
    assert!(
        logs.contains("FollowLogs") && logs.contains("node: Some(\"app-1\")"),
        "{logs}"
    );
    has(&ui.screen(140, 40)?, &["Logs · web-1  on app-1"]);
    assert_eq!(ui.press("esc"), "CloseStream");
    ui.press("j");
    let menu = ui.press("enter");
    assert_eq!(menu, "None");
    has(&ui.screen(140, 40)?, &["follow this container"]);
    ui.press("esc");
    // A command on a node runs there.
    ui.press("x");
    for key in "true".chars() {
        ui.press(&key.to_string());
    }
    let run = ui.press("enter");
    assert!(
        run.contains("RunCommand") && run.contains("node: Some(\"app-2\")"),
        "{run}"
    );
    Ok(())
}

#[test]
fn a_node_that_has_not_answered_yet_says_so_and_its_services_are_not_whole() -> TestResult {
    let mut ui = ScriptedUi::new(cluster(Some("app-3"))?, Some("cluster"));
    ui.press("3");
    has(
        &ui.screen(140, 40)?,
        &["web  3 nodes", "2/3", "app-3  not answered yet"],
    );
    Ok(())
}

#[test]
fn a_remote_environment_shows_its_nodes_where_this_machine_shows_local_https() -> TestResult {
    let ui = ScriptedUi::new(cluster(None)?, Some("cluster"));
    let overview = ui.screen(140, 40)?;
    has(
        &overview,
        &["Nodes", "3/3", "answering", "u deploy", "b roll back"],
    );
    assert!(!overview.contains("HTTPS"), "{overview}");
    assert!(!overview.contains("Compose"), "{overview}");
    Ok(())
}

/// Every part of the first frame's session, as the reader reports them on
/// a remote environment.
fn every_part(snapshot: &WorkspaceSnapshot) -> Vec<Loaded> {
    vec![
        Loaded::Mounts(Vec::new()),
        Loaded::Https(None),
        Loaded::SharedServices(Box::new(ServiceStackStatus {
            installed: false,
            stack_version: None,
            runtime: None,
            target_id: None,
            route: String::new(),
            internal_smtp: String::new(),
            host_smtp_port: None,
            components: Vec::new(),
            diagnostics: Vec::new(),
        })),
        Loaded::Identity {
            identity: None,
            error: None,
        },
        Loaded::Status {
            snapshot: Box::new(snapshot.clone()),
            complete: true,
        },
        Loaded::Variables(None),
    ]
}

// Found on the test VMs: once every part had arrived, the session of the
// first frame stopped counting as one being read, so the complete session
// read right after it was dropped. The first frame's session never reads
// its status again, so a remote environment was never refreshed: a node
// that had been silent stayed "not answering" after it answered again.
#[test]
fn the_complete_session_takes_the_place_of_the_first_frame_after_every_part_arrived() -> TestResult
{
    let snapshot = cluster(Some("app-3"))?;
    let mut ui = ScriptedUi::opening(snapshot.clone(), Some("cluster"));
    assert!(ui.takes_read("production"), "the first frame is replaced");
    for part in every_part(&snapshot) {
        ui.loaded(part);
    }
    // Every part is on screen, and the session is still the first frame's.
    has(&ui.screen(140, 40)?, &["Nodes", "2/3"]);
    assert!(
        ui.takes_read("production"),
        "the complete session must still take the place of the first frame"
    );
    // A read of another environment never takes its place.
    assert!(!ui.takes_read("staging"));
    // A session that is not the first frame's is never replaced by a read.
    let complete = ScriptedUi::new(snapshot, Some("cluster"));
    assert!(!complete.takes_read("production"));
    Ok(())
}

// Seen twice in the live script on Docker nodes: `u`, typed as soon as
// every node's status was on screen, never brought the deploy review. The
// session on screen was still the first frame's (its parts had all
// arrived; the complete session took its place a moment later), which
// answers an action with a note in the footer and drops it.
#[test]
fn an_action_pressed_before_the_session_is_read_goes_ahead_once_it_is() -> TestResult {
    let snapshot = cluster(None)?;
    let mut ui = ScriptedUi::opening(snapshot.clone(), Some("cluster"));
    for part in every_part(&snapshot) {
        ui.loaded(part);
    }
    has(&ui.screen(140, 40)?, &["Nodes", "3/3"]);
    // Moving between views works at once; the action waits, and says so.
    assert_eq!(ui.press("3"), "None");
    assert_eq!(ui.press("u"), "None");
    let notice = ui.notice().unwrap_or_default();
    assert!(
        notice.starts_with("Still reading production; this goes ahead as soon as it is here"),
        "{notice}"
    );
    // The complete session takes the place of the first frame's, and the
    // action goes ahead there.
    assert_eq!(ui.complete(), "Stack(Review)");
    ui.reviewed(review(None));
    has(&ui.screen(140, 40)?, &["Deploy to production?"]);
    // A key pressed after it drops it: the user went on.
    let mut ui = ScriptedUi::opening(snapshot.clone(), Some("cluster"));
    for part in every_part(&snapshot) {
        ui.loaded(part);
    }
    assert_eq!(ui.press("u"), "None");
    assert_eq!(ui.press("j"), "None");
    assert_eq!(ui.complete(), "None");
    // A session that is not the first frame's acts at once.
    let mut ui = ScriptedUi::new(snapshot, Some("cluster"));
    assert_eq!(ui.press("u"), "Stack(Review)");
    assert_eq!(ui.complete(), "None");
    Ok(())
}

/// The UI's word on a deployment that ran on one remote host names the
/// target the project calls it by, not the host's own `local`.
#[test]
fn a_deployment_on_one_remote_host_is_said_to_start_on_its_target() {
    let result = mm::domain::deployment::DeploymentResult {
        plan_hash: "a".repeat(64),
        target_name: "local".to_owned(),
        target_id: mm::domain::target::TargetId(uuid::Uuid::from_u128(1)),
        environment: "production".to_owned(),
        applied_stacks: vec!["app".to_owned()],
        removed_stacks: Vec::new(),
        route_generation: 1,
        route_count: 0,
        moved_away: Vec::new(),
        jobs: Vec::new(),
    };
    assert_eq!(result.started_on("solo"), "Started app on solo.");
    let routed = mm::domain::deployment::DeploymentResult {
        route_count: 2,
        applied_stacks: Vec::new(),
        ..result
    };
    assert_eq!(
        routed.started_on("solo"),
        "Nothing to start on solo (2 routes)."
    );
}
