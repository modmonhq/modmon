use super::*;
use crate::domain::runtime::{ServiceIdentity, ServiceState};
use crate::domain::workspace::ProjectId;

fn container(service: &str, state: &str, exit_code: Option<i32>) -> ContainerObservation {
    ContainerObservation {
        id: format!("{service}-id"),
        name: format!("demo-{service}-1"),
        service: service.to_owned(),
        image: "image".to_owned(),
        state: state.to_owned(),
        status: String::new(),
        health: None,
        restart_count: None,
        exit_code,
        created_at: None,
        started_at: None,
        finished_at: None,
        revision: None,
        digest: None,
        diagnostic_code: None,
        error: None,
        labels: Default::default(),
        node: None,
        history: false,
    }
}

fn service(
    name: &str,
    state: ServiceState,
    healthy: u32,
    code: Option<&str>,
) -> ServiceObservation {
    ServiceObservation {
        identity: ServiceIdentity {
            project_id: ProjectId(uuid::Uuid::from_u128(80)),
            environment: "production".to_owned(),
            stack: "app".to_owned(),
            module: None,
            service: name.to_owned(),
        },
        desired_count: 1,
        running_count: u32::from(state != ServiceState::Stopped),
        healthy_count: healthy,
        state,
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        revision: None,
        digest: None,
        diagnostic_code: code.map(str::to_owned),
    }
}

fn project(
    services: Vec<ServiceObservation>,
    containers: Vec<ContainerObservation>,
) -> ObservedProject {
    ObservedProject {
        stack: "app".to_owned(),
        services,
        containers,
    }
}

#[test]
fn a_wait_ends_when_every_service_is_healthy_and_names_the_one_that_is_not() {
    let mut watch = ConvergenceWatch::new(None);
    let starting = project(
        vec![
            service("web", ServiceState::Running, 1, None),
            service("worker", ServiceState::Degraded, 0, None),
        ],
        vec![
            container("web", "running", None),
            container("worker", "running", None),
        ],
    );
    let waiting = watch.assess(&starting);
    assert_eq!(waiting.state, ConvergenceState::Converging);
    assert_eq!(waiting.service.as_deref(), Some("worker"));

    let healthy = project(
        vec![
            service("web", ServiceState::Running, 1, None),
            service("worker", ServiceState::Running, 1, None),
        ],
        vec![
            container("web", "running", None),
            container("worker", "running", None),
        ],
    );
    assert_eq!(watch.assess(&healthy).state, ConvergenceState::Succeeded);
    assert_eq!(
        ConvergenceWatch::new(None)
            .assess(&project(Vec::new(), Vec::new()))
            .state,
        ConvergenceState::Accepted
    );
}

#[test]
fn a_wait_for_some_services_ignores_the_others() {
    let observed = project(
        vec![
            service("web", ServiceState::Running, 1, None),
            service("worker", ServiceState::Stopped, 0, None),
        ],
        vec![
            container("web", "running", None),
            container("worker", "exited", Some(1)),
        ],
    );
    let only = ["web".to_owned()];
    assert_eq!(
        ConvergenceWatch::new(Some(&only)).assess(&observed).state,
        ConvergenceState::Succeeded
    );
    assert_eq!(
        ConvergenceWatch::new(None).assess(&observed).state,
        ConvergenceState::Failed
    );
}

#[test]
fn a_service_that_cannot_become_healthy_fails_the_wait_at_once() {
    // A failing health check.
    let unhealthy = project(
        vec![service(
            "web",
            ServiceState::Degraded,
            0,
            Some("MM_RUNTIME_UNHEALTHY"),
        )],
        vec![container("web", "running", None)],
    );
    let failed = ConvergenceWatch::new(None).assess(&unhealthy);
    assert_eq!(failed.state, ConvergenceState::Failed);
    assert_eq!(failed.service.as_deref(), Some("web"));
    assert_eq!(
        failed.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_UNHEALTHY")
    );
    assert_eq!(failed.message.as_deref(), Some("its health check fails"));

    // A container that exited and that nothing restarts.
    let exited = project(
        vec![service("web", ServiceState::Stopped, 0, None)],
        vec![container("web", "exited", Some(3))],
    );
    let failed = ConvergenceWatch::new(None).assess(&exited);
    assert_eq!(failed.diagnostic_code.as_deref(), Some("MM_RUNTIME_EXITED"));
    assert_eq!(
        failed.message.as_deref(),
        Some("its container exited with code 3")
    );
    let done = project(
        vec![service("migrate", ServiceState::Stopped, 0, None)],
        vec![container("migrate", "exited", Some(0))],
    );
    let failed = ConvergenceWatch::new(None).assess(&done);
    assert!(
        failed
            .message
            .as_deref()
            .is_some_and(|message| message.contains("is a job, not a service")),
        "{failed:?}"
    );
}

#[test]
fn a_container_the_engine_keeps_restarting_fails_the_wait_after_three_looks() {
    let restarting = project(
        vec![service("web", ServiceState::Degraded, 0, None)],
        vec![container("web", "restarting", Some(1))],
    );
    let mut watch = ConvergenceWatch::new(None);
    assert_eq!(
        watch.assess(&restarting).state,
        ConvergenceState::Converging
    );
    assert_eq!(
        watch.assess(&restarting).state,
        ConvergenceState::Converging
    );
    let failed = watch.assess(&restarting);
    assert_eq!(failed.state, ConvergenceState::Failed);
    assert_eq!(
        failed.diagnostic_code.as_deref(),
        Some("MM_RUNTIME_RESTARTING")
    );
    assert!(
        failed
            .message
            .as_deref()
            .is_some_and(|message| message.contains("keeps exiting with code 1")),
        "{failed:?}"
    );

    // One restart on the way to healthy is forgotten.
    let mut watch = ConvergenceWatch::new(None);
    watch.assess(&restarting);
    watch.assess(&restarting);
    let healthy = project(
        vec![service("web", ServiceState::Running, 1, None)],
        vec![container("web", "running", None)],
    );
    assert_eq!(watch.assess(&healthy).state, ConvergenceState::Succeeded);
    assert_eq!(
        watch.assess(&restarting).state,
        ConvergenceState::Converging
    );
}
