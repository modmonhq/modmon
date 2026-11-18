//! What a service's containers say about the service: the ones that run
//! now decide, and containers that were replaced are history.

use std::collections::BTreeMap;

use mm::domain::runtime::{
    ContainerObservation, ServiceSnapshot, ServiceState, current_containers, state_across_nodes,
    tally_service,
};

fn container(name: &str, state: &str, created: &str) -> ContainerObservation {
    ContainerObservation {
        id: format!("{name}-id"),
        name: name.to_owned(),
        service: "api-sync".to_owned(),
        image: "registry.example.test/api-sync:1".to_owned(),
        state: state.to_owned(),
        status: if state == "running" {
            "Up 18 hours (healthy)".to_owned()
        } else {
            "Exited (1) 18 hours ago".to_owned()
        },
        health: (state == "running").then(|| "healthy".to_owned()),
        restart_count: None,
        exit_code: (state != "running").then_some(1),
        created_at: Some(created.to_owned()),
        started_at: None,
        finished_at: None,
        revision: None,
        digest: None,
        diagnostic_code: None,
        error: None,
        labels: BTreeMap::new(),
        node: None,
        history: false,
    }
}

fn names(containers: &[&ContainerObservation]) -> Vec<String> {
    containers
        .iter()
        .map(|container| container.name.clone())
        .collect()
}

/// A container that failed and was replaced does not make the
/// service degraded while its replacement runs and is healthy.
#[test]
fn a_replaced_container_that_failed_does_not_degrade_a_healthy_service() {
    let failed = container("api-sync-1", "exited", "2026-11-12T10:00:00Z");
    let mut unhealthy_once = container("api-sync-2", "exited", "2026-11-12T11:00:00Z");
    unhealthy_once.diagnostic_code = Some("MM_RUNTIME_UNHEALTHY".to_owned());
    unhealthy_once.health = Some("unhealthy".to_owned());
    let current = container("api-sync-3", "running", "2026-11-12T12:00:00Z");
    let all = [&failed, &unhealthy_once, &current];

    let tally = tally_service(&all);
    assert_eq!(tally.state, ServiceState::Running);
    assert_eq!(
        (
            tally.desired_count,
            tally.running_count,
            tally.healthy_count
        ),
        (1, 1, 1)
    );
    assert_eq!(tally.diagnostic_code, None);
    assert_eq!(names(&current_containers(&all)), ["api-sync-3"]);
}

#[test]
fn a_container_that_ran_once_for_a_job_never_counts() {
    let service = container("web-1", "running", "2026-11-12T10:00:00Z");
    let mut job = container("web-run-4f2a", "exited", "2026-11-12T11:00:00Z");
    job.labels
        .insert("com.docker.compose.oneoff".to_owned(), "True".to_owned());
    assert!(job.is_one_off());
    let tally = tally_service(&[&service, &job]);
    assert_eq!(tally.state, ServiceState::Running);
    assert_eq!((tally.desired_count, tally.running_count), (1, 1));
    // With nothing but a finished job, the service has no container.
    assert_eq!(tally_service(&[&job]).state, ServiceState::Missing);
}

#[test]
fn the_newest_container_failing_is_the_services_state() {
    // The replacement did not come up: the service is degraded while the
    // old container still runs, and stopped when nothing runs.
    let old = container("web-1", "running", "2026-11-12T10:00:00Z");
    let failed = container("web-2", "exited", "2026-11-12T11:00:00Z");
    let tally = tally_service(&[&old, &failed]);
    assert_eq!(tally.state, ServiceState::Degraded);
    assert_eq!((tally.desired_count, tally.running_count), (2, 1));

    let stopped = container("web-1", "exited", "2026-11-12T10:00:00Z");
    let tally = tally_service(&[&stopped, &failed]);
    assert_eq!(tally.state, ServiceState::Stopped);
    assert_eq!(tally.running_count, 0);
    assert_eq!(tally_service(&[]).state, ServiceState::Missing);
}

#[test]
fn a_running_container_that_is_not_healthy_degrades_the_service() {
    let mut starting = container("web-1", "running", "2026-11-12T10:00:00Z");
    starting.health = Some("starting".to_owned());
    let tally = tally_service(&[&starting]);
    assert_eq!(tally.state, ServiceState::Degraded);
    assert_eq!((tally.running_count, tally.healthy_count), (1, 0));

    let mut unhealthy = container("web-1", "running", "2026-11-12T10:00:00Z");
    unhealthy.health = Some("unhealthy".to_owned());
    unhealthy.diagnostic_code = Some("MM_RUNTIME_UNHEALTHY".to_owned());
    let tally = tally_service(&[&unhealthy]);
    assert_eq!(tally.state, ServiceState::Degraded);
    assert_eq!(
        tally.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_UNHEALTHY")
    );
}

#[test]
fn a_service_on_several_nodes_runs_when_every_node_runs_it() {
    use ServiceState::{Degraded, Missing, Running, Stopped, Unknown};
    assert_eq!(state_across_nodes(&[Running, Running, Running]), Running);
    assert_eq!(state_across_nodes(&[Running, Missing, Running]), Degraded);
    assert_eq!(state_across_nodes(&[Running, Stopped]), Degraded);
    assert_eq!(state_across_nodes(&[Stopped, Stopped]), Stopped);
    assert_eq!(state_across_nodes(&[Missing, Missing]), Missing);
    assert_eq!(state_across_nodes(&[Unknown, Unknown]), Unknown);
    assert_eq!(state_across_nodes(&[Stopped, Missing]), Degraded);
    assert_eq!(state_across_nodes(&[]), Missing);
}

#[test]
fn a_container_waiting_for_shared_storage_says_so() {
    let mut waiting = container("files-1", "created", "2026-11-12T10:00:00Z");
    waiting.error = Some(
        "error while mounting volume '/var/lib/docker/volumes/mm-shared-0123456789ab-production-app-uploads-ab12cd34/_data': connection timed out"
            .to_owned(),
    );
    assert!(waiting.waits_for_shared_storage());
    waiting.error = Some("OCI runtime create failed".to_owned());
    assert!(!waiting.waits_for_shared_storage());
    let mut running = container("files-1", "running", "2026-11-12T10:00:00Z");
    running.error = Some("mm-shared-x: an earlier failure".to_owned());
    assert!(!running.waits_for_shared_storage());
}

/// A container that is neither running nor stopped (paused, between
/// restarts) says nothing by itself; one whose health check had failed
/// makes the service degraded.
#[test]
fn a_paused_container_is_unknown_unless_its_health_check_had_failed() {
    let mut paused = container("web-1", "paused", "2026-11-12T10:00:00Z");
    paused.status = "Up 2 hours (Paused)".to_owned();
    paused.health = None;
    assert_eq!(tally_service(&[&paused]).state, ServiceState::Unknown);
    paused.health = Some("starting".to_owned());
    assert_eq!(tally_service(&[&paused]).state, ServiceState::Unknown);
    paused.health = Some("unhealthy".to_owned());
    let tally = tally_service(&[&paused]);
    assert_eq!(tally.state, ServiceState::Degraded);
    assert_eq!((tally.running_count, tally.healthy_count), (0, 0));
    // A stopped container is stopped whatever its last health was.
    let mut exited = container("web-1", "exited", "2026-11-12T10:00:00Z");
    exited.health = Some("unhealthy".to_owned());
    assert_eq!(tally_service(&[&exited]).state, ServiceState::Stopped);
}

/// A node that did not answer has the state `Unknown`; with the service
/// running elsewhere the service is degraded, not running.
#[test]
fn a_node_nobody_heard_from_keeps_a_service_from_reading_as_running() {
    use ServiceState::{Degraded, Running, Unknown};
    assert_eq!(state_across_nodes(&[Running, Running, Unknown]), Degraded);
    assert_eq!(state_across_nodes(&[Unknown, Unknown]), Unknown);
    let mut starting = container("web-1", "running", "2026-11-12T10:00:00Z");
    assert!(!starting.health_is_starting());
    starting.health = Some("Starting".to_owned());
    assert!(starting.health_is_starting());
}

/// `starting` is written by one function from the service's state, counts
/// and current containers, is always in the JSON, and reads as `false`
/// from JSON that has no such key.
#[test]
fn starting_is_one_stored_fact_that_is_always_serialized() -> Result<(), Box<dyn std::error::Error>>
{
    let mut coming_up = container("web-1", "running", "2026-11-12T10:00:00Z");
    coming_up.health = Some("starting".to_owned());
    let mut replaced = container("web-0", "exited", "2026-11-11T10:00:00Z");
    replaced.history = true;
    let mut service: ServiceSnapshot = serde_json::from_value(serde_json::json!({
        "stack": "app", "name": "web", "module": null, "state": "degraded",
        "desired_count": 1, "running_count": 1, "healthy_count": 0,
        "containers": [coming_up, replaced]
    }))?;
    assert!(!service.starting);
    assert_eq!(serde_json::to_value(&service)?["starting"], false);

    service.note_starting();
    assert!(service.starting);
    assert_eq!(serde_json::to_value(&service)?["starting"], true);

    // A check that failed, a container that does not run, a diagnostic:
    // each is a failure, not a start.
    service.containers[0].health = Some("unhealthy".to_owned());
    service.note_starting();
    assert!(!service.starting);
    service.containers[0].health = Some("starting".to_owned());
    service.running_count = 0;
    service.note_starting();
    assert!(!service.starting);
    service.running_count = 1;
    service.diagnostic_code = Some("runtime.service.restarting".to_owned());
    service.note_starting();
    assert!(!service.starting);
    service.diagnostic_code = None;
    service.state = ServiceState::Running;
    service.note_starting();
    assert!(!service.starting);
    Ok(())
}
