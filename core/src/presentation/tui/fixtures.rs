//! Shared TUI test fixtures.

use std::collections::BTreeMap;

use crate::domain::config::{DurationSpec, JobConfig, RetryPolicyConfig};
use crate::domain::diagnostic::ValidationReport;
use crate::domain::runtime::{RuntimeKind, RuntimeProbe};
use crate::domain::scheduler::{
    DesiredDefinitions, JobRun, JobRunState, JobTrigger, ScheduleRecord,
};
use crate::domain::service_stack::ServiceStackStatus;
use crate::domain::workspace::{ProjectId, WorkspaceId, WorkspaceRecord, WorkspaceSnapshot};
use crate::presentation::tui::model::{Model, ModelInput};
use crate::presentation::tui::provider::SecretBackendTuiContext;

pub(super) fn model() -> Model {
    let project_id = ProjectId(uuid::Uuid::from_u128(1));
    let mut jobs = BTreeMap::new();
    jobs.insert(
        "cleanup".to_owned(),
        JobConfig {
            module: "app".to_owned(),
            service: "worker".to_owned(),
            command: vec!["cleanup".to_owned()],
            environments: None,
            run: None,
            timeout: DurationSpec("30s".to_owned()),
            retry: RetryPolicyConfig::default(),
            output_limit_bytes: 1024,
            history_limit: 10,
            placement: None,
            user: None,
            placed_elsewhere: None,
            development: None,
            off_locally: None,
        },
    );
    Model::new(ModelInput {
        snapshot: snapshot(project_id),
        environment: None,
        tls: None,
        installation_services: ServiceStackStatus {
            installed: false,
            stack_version: None,
            runtime: None,
            target_id: None,
            route: "https://mm.test".to_owned(),
            internal_smtp: "mailpit:1025".to_owned(),
            host_smtp_port: None,
            components: Vec::new(),
            diagnostics: Vec::new(),
        },
        mounts: Vec::new(),
        backends: SecretBackendTuiContext {
            root: "/project".into(),
            target: "local".to_owned(),
            config_hash: "a".repeat(64),
            project_id,
            environment: "development".to_owned(),
            external_connections: Vec::new(),
            managed_connections: Vec::new(),
            drivers: Vec::new(),
            instances: BTreeMap::new(),
            rotatable_secrets: Vec::new(),
        },
        gates: Vec::new(),
        jobs: DesiredDefinitions {
            jobs,
            schedules: BTreeMap::new(),
        },
        schedules: vec![schedule("nightly", true)],
        job_runs: vec![running_job(project_id)],
        access: crate::presentation::tui::model::AccessView::default(),
        environments: Vec::new(),
        setup: crate::presentation::tui::setup::SetupView::default(),
    })
}

fn snapshot(project_id: ProjectId) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        workspace: WorkspaceRecord {
            id: WorkspaceId(uuid::Uuid::from_u128(2)),
            project_id,
            root: "/project".into(),
            config_path: "/project/.config/mm.yaml".into(),
        },
        project_name: "fixture".to_owned(),
        target: "local".to_owned(),
        environment: "development".to_owned(),
        config_hash: "config-hash".to_owned(),
        target_source: None,
        runtime_kind: Some(RuntimeKind::DockerCompose),
        runtime_source: Some("fixture".to_owned()),
        runtime: RuntimeProbe {
            kind: RuntimeKind::DockerCompose,
            available: false,
            engine_version: None,
            compose_provider: None,
            rootless: None,
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: Vec::new(),
        },
        validation: ValidationReport::default(),
        stacks: Vec::new(),
        ingress: Vec::new(),
        recent_operations: Vec::new(),
        nodes: Vec::new(),
        names: None,
        deployment: None,
        drift: Vec::new(),
        network_drift: Vec::new(),
        optional: Vec::new(),
        tunnel: None,
    }
}

fn running_job(project_id: ProjectId) -> JobRun {
    JobRun {
        id: uuid::Uuid::from_u128(3),
        project_id,
        environment: "development".to_owned(),
        job: "cleanup".to_owned(),
        schedule: Some("nightly".to_owned()),
        trigger: JobTrigger::Schedule,
        state: JobRunState::Running,
        attempt: 1,
        maximum_attempts: 1,
        fence_token: 1,
        runtime_name: "fixture".to_owned(),
        runtime_id: Some("container".to_owned()),
        requested_at_unix_ms: 1,
        scheduled_for_unix_ms: Some(1),
        started_at_unix_ms: Some(1),
        finished_at_unix_ms: None,
        next_attempt_at_unix_ms: None,
        exit_code: None,
        error_code: None,
        output: String::new(),
        output_truncated: false,
        cancellation_requested: false,
        principal: "developer".to_owned(),
        source: "tui".to_owned(),
        node: None,
    }
}

pub(super) fn schedule(name: &str, enabled: bool) -> ScheduleRecord {
    ScheduleRecord {
        name: name.to_owned(),
        project_id: ProjectId(uuid::Uuid::from_u128(1)),
        environment: "development".to_owned(),
        job: "cleanup".to_owned(),
        enabled,
        retired: false,
        next_due_at_unix_ms: Some(1),
        definition_hash: "definition-hash".to_owned(),
        generation: 1,
    }
}

/// A populated workspace for render tests: mixed service health, routes, environment
/// problems, TLS, and diagnostics of every severity.
pub(super) fn rich_model() -> Result<Model, serde_json::Error> {
    use crate::domain::diagnostic::Diagnostic;

    let mut model = model();
    // One container per desired replica: healthy first, then unhealthy, then exited.
    let service =
        |name: &str, module: &str, state: &str, desired: u32, running: u32, healthy: u32| {
            let containers = (0..desired)
                .map(|index| {
                    let (state, status, health) = if index < healthy {
                        ("running", "Up 2 hours (healthy)", Some("healthy"))
                    } else if index < running {
                        ("running", "Up 4 minutes (unhealthy)", Some("unhealthy"))
                    } else {
                        ("exited", "Exited (137) 3 minutes ago", None)
                    };
                    serde_json::json!({
                        "id": container_id(name, index),
                        "name": format!("fixture-{name}-{}", index + 1), "service": name,
                        "image": format!("acme/{name}:latest"), "state": state,
                        "status": status, "health": health, "labels": {},
                        "restart_count": if state == "exited" { 7 } else { 0 }
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "stack": "app", "name": name, "module": module, "state": state,
                "desired_count": desired, "running_count": running, "healthy_count": healthy,
                "image": format!("acme/{name}:latest"), "revision": "9f2c1e7ab4d03c55e1",
                "restart_count": if state == "degraded" { 7 } else { 0 },
                "containers": containers
            })
        };
    model.snapshot.stacks = serde_json::from_value(serde_json::json!([{
        "name": "app", "compose_project_name": "fixture-app", "artifact_hash": "a",
        "rendered_config_hash": "b",
        "services": [
            service("web", "frontend", "running", 2, 2, 2),
            service("api", "backend", "running", 3, 3, 3),
            service("worker", "backend", "degraded", 3, 2, 1),
            service("migrate", "backend", "stopped", 1, 0, 0),
        ]
    }]))?;
    model.snapshot.ingress = serde_json::from_value(serde_json::json!([
        { "module": "frontend", "stack": "app", "service": "web", "url": "https://web.fixture.mm.test",
          "selected_port": 3000, "tls": true, "provenance": "automatic_convention" },
        { "module": "backend", "stack": "app", "service": "api", "url": "https://api.fixture.mm.test",
          "selected_port": 8080, "tls": true, "provenance": "explicit_config",
          "warning": "port 8080 is also published on the host" },
        { "module": "backend", "stack": "app", "service": "worker", "reason": "no HTTP port declared" }
    ]))?;
    model.snapshot.runtime.available = true;
    model.snapshot.runtime.engine_version = Some("28.1.1".to_owned());
    model.snapshot.validation.push(
        Diagnostic::warning(
            "runtime.service.restarting",
            "worker is restarting repeatedly",
        )
        .with_help("Run mm logs worker to see why it exits."),
    );
    model.snapshot.validation.push(
        Diagnostic::error("runtime.container.exited", "migrate exited with code 137")
            .with_help("Check memory limits for the migrate service."),
    );
    model.snapshot.validation.push(Diagnostic::info(
        "config.defaulted",
        "Environment delivery uses the development default",
    ));
    add_local_parts(&mut model)?;
    model.environments =
        crate::presentation::tui::environments::EnvironmentsView::new(environments());
    Ok(model)
}

/// What the rich workspace reads on this machine: the shared services,
/// local HTTPS, and the variables.
fn add_local_parts(model: &mut Model) -> Result<(), serde_json::Error> {
    model.installation_services = serde_json::from_value(serde_json::json!({
        "installed": true, "runtime": "docker-compose", "route": "https://mm.test",
        "internal_smtp": "mailpit:1025", "host_smtp_port": 1025, "diagnostics": [],
        "components": [
            { "kind": "local_ingress", "version": "3.4", "running": true, "healthy": true },
            { "kind": "mm_control_api", "version": "1.0", "running": true, "healthy": true },
            { "kind": "mailpit", "version": "1.21", "running": true, "healthy": true },
            { "kind": "cap", "version": "0.1", "running": false, "healthy": false }
        ]
    }))?;
    model.tls = Some(serde_json::from_value(serde_json::json!({
        "installed": true, "fingerprint": "sha256:aa", "generation": 3,
        "issued_hosts": ["api.fixture.mm.test", "web.fixture.mm.test"],
        "key_permissions_valid": true, "trust_disabled": false, "rotation_pending": false,
        "trust": { "supported": true, "installed": true, "platform": "linux",
                   "scope": "current_user", "mismatch": false, "needs_privilege": false }
    }))?);
    let variable =
        |name: &str, source: &str, status: &str, value: Option<&str>, sensitive: bool| {
            serde_json::json!({
                "name": name, "global_key": null, "required": true, "has_default": false,
                "sensitive": sensitive, "source": source, "status": status, "value": value,
                "location": { "path": "/project/app.env", "line": 1, "column": 1 }
            })
        };
    model.environment = Some(serde_json::from_value(serde_json::json!({
        "project_name": "fixture", "environment": "development",
        "config_hash": "config-hash", "input_hash": "input-hash", "diagnostics": [],
        "modules": [{
            "module": "backend", "template_path": "/project/app.env",
            "services": [{
                "stack": "app", "service": "api",
                "variables": [
                    variable("DATABASE_URL", "config_value", "resolved", Some("postgres://db/app"), false),
                    variable("API_SECRET", "secret", "resolved", Some("hidden-value"), true),
                    variable("SENTRY_DSN", "missing", "unset", None, false),
                ]
            }]
        }]
    }))?);
    Ok(())
}

/// A 12-digit hexadecimal container ID that is stable per service and replica.
pub(super) fn container_id(service: &str, index: u32) -> String {
    let name = service
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{:0<11.11}{}", name, index + 1)
}

/// A local development environment, two remote ones, and one that cannot be resolved.
pub(super) fn environments() -> Vec<crate::presentation::tui::environments::EnvironmentEntry> {
    use crate::presentation::tui::environments::EnvironmentEntry;
    vec![
        EnvironmentEntry {
            name: "development".to_owned(),
            default: true,
            target: "local".to_owned(),
            runtime: Some(RuntimeKind::DockerCompose),
            ..EnvironmentEntry::default()
        },
        EnvironmentEntry {
            name: "staging".to_owned(),
            target: "staging-eu".to_owned(),
            remote: Some("deploy@staging.example.test:22".to_owned()),
            runtime: Some(RuntimeKind::DockerCompose),
            deploy_from: vec!["main".to_owned()],
            ..EnvironmentEntry::default()
        },
        EnvironmentEntry {
            name: "production".to_owned(),
            target: "prod-eu".to_owned(),
            remote: Some("deploy@prod.example.test:22".to_owned()),
            runtime: Some(RuntimeKind::DockerCompose),
            deploy_from: vec!["release".to_owned()],
            ..EnvironmentEntry::default()
        },
        EnvironmentEntry {
            name: "preview".to_owned(),
            target: String::new(),
            problem: Some("target 'preview-host' is not registered on this machine".to_owned()),
            ..EnvironmentEntry::default()
        },
    ]
}

/// A team of two people and a CI identity; alice is this device.
pub(super) fn team() -> Vec<crate::presentation::tui::model::TeamMemberView> {
    use crate::presentation::tui::model::TeamMemberView;
    vec![
        TeamMemberView {
            name: "alice".to_owned(),
            access: Some("deploy +exec[api]".to_owned()),
            keys: 2,
            machine: false,
            you: true,
        },
        TeamMemberView {
            name: "bob".to_owned(),
            access: Some("view".to_owned()),
            keys: 1,
            machine: false,
            you: false,
        },
        TeamMemberView {
            name: "ci".to_owned(),
            access: Some("deploy".to_owned()),
            keys: 1,
            machine: true,
            you: false,
        },
    ]
}

/// The rich workspace seen as the remote `production` environment, with the
/// host's view of the operator and the other environments already checked.
pub(super) fn remote_model() -> Result<Model, serde_json::Error> {
    use crate::domain::host_policy::{AccessLevel, EnvironmentAccess, ServiceSelection};
    use crate::domain::runtime::ServiceState;
    use crate::presentation::tui::environments::{EnvironmentProbe, ProbeState, ServiceLine};

    let mut model = rich_model()?;
    model.snapshot.environment = "production".to_owned();
    model.snapshot.target = "prod-eu".to_owned();
    model.snapshot.runtime.kind = RuntimeKind::DockerCompose;
    model.access = crate::presentation::tui::model::AccessView {
        remote_target: Some("prod-eu".to_owned()),
        device_key: Some("SHA256:device".to_owned()),
        identity: Some(crate::domain::remote::RemoteIdentity {
            principal: "alice".to_owned(),
            principal_id: crate::domain::access::PrincipalId(uuid::Uuid::from_u128(1)),
            key_fingerprint: "SHA256:device".to_owned(),
            break_glass: false,
            environment: "production".to_owned(),
            access: Some(EnvironmentAccess {
                level: AccessLevel::Deploy,
                exec: ServiceSelection::Only(["api".to_owned()].into()),
                forward: ServiceSelection::All(false),
                secrets: false,
            }),
            access_commit: Some("a".repeat(40)),
            deployed_commit: Some("9f2c1e7ab4d03c55e1".to_owned()),
        }),
        ..crate::presentation::tui::model::AccessView::default()
    };
    let at = std::time::Instant::now();
    model.environments.probes.insert(
        "development".to_owned(),
        ProbeState::Ready {
            probe: EnvironmentProbe {
                services: ["web", "api"]
                    .map(|name| ServiceLine {
                        name: name.to_owned(),
                        state: ServiceState::Running,
                        text: "running 1/1".to_owned(),
                    })
                    .into(),
                routes: 2,
                ..EnvironmentProbe::default()
            },
            at,
        },
    );
    model.environments.probes.insert(
        "staging".to_owned(),
        ProbeState::Failed {
            message: "ssh: connect to host staging.example.test port 22: Connection timed out"
                .to_owned(),
            at,
        },
    );
    Ok(model)
}

/// The remote environment as a cluster of three nodes. `web` is placed on
/// every node and has no current container on app-3, which says so as drift;
/// `api` runs as one container on app-1.
pub(super) fn cluster_model() -> Result<Model, serde_json::Error> {
    let container = |service: &str, node: &str| {
        serde_json::json!({
            "id": format!("{service}-{node}"), "name": format!("fixture-app-{service}-1"),
            "service": service, "image": format!("acme/{service}:latest"), "state": "running",
            "status": "Up 2 hours (healthy)", "health": "healthy", "labels": {}, "node": node
        })
    };
    let share = |node: &str, state: &str, running: u32| {
        serde_json::json!({
            "node": node, "state": state, "desired_count": 1,
            "running_count": running, "healthy_count": running
        })
    };
    let mut model = remote_model()?;
    let mut snapshot = model.snapshot.clone();
    snapshot.stacks = serde_json::from_value(serde_json::json!([{
        "name": "app", "compose_project_name": "fixture-app", "artifact_hash": "a",
        "rendered_config_hash": "b",
        "services": [
            { "stack": "app", "name": "web", "module": "frontend", "state": "degraded",
              "desired_count": 3, "running_count": 2, "healthy_count": 2,
              "nodes": [share("app-1", "running", 1), share("app-2", "running", 1),
                        share("app-3", "missing", 0)],
              "containers": [
                  container("web", "app-1"), container("web", "app-2"),
                  // The container a newer one replaced: listed by the engine, not here.
                  { "id": "web-earlier", "name": "fixture-app-web-run-1f2e", "service": "web",
                    "image": "acme/web:latest", "state": "exited",
                    "status": "Exited (0) 2 days ago", "health": null, "labels": {},
                    "node": "app-3", "history": true }
              ] },
            { "stack": "app", "name": "api", "module": "backend", "state": "running",
              "desired_count": 1, "running_count": 1, "healthy_count": 1,
              "nodes": [share("app-1", "running", 1)],
              "containers": [container("api", "app-1")] }
        ]
    }]))?;
    let deployment = serde_json::json!({
        "commit": "3f2c9a1e5b7d40c8a1e2", "deployed_at_unix_ms": 1_794_633_060_000_i64,
        "deployed_by": "alice", "plan_hash": "8a41c0de91f2a7b3"
    });
    snapshot.nodes = serde_json::from_value(serde_json::json!([
        { "name": "app-1", "reachable": true, "deployment": deployment },
        { "name": "app-2", "reachable": true, "deployment": deployment },
        { "name": "app-3", "reachable": true, "deployment": deployment,
          "drift": ["service 'web' is deployed here and has no container"] }
    ]))?;
    snapshot.deployment = Some(serde_json::from_value(deployment)?);
    if let Some(identity) = model.access.identity.as_mut() {
        identity.deployed_commit = Some("3f2c9a1e5b7d40c8a1e2".to_owned());
    }
    // The findings about the nodes are made when a status is shown.
    model.show_status(snapshot);
    model.environments.probes.insert(
        "production".to_owned(),
        crate::presentation::tui::environments::ProbeState::Ready {
            probe: crate::presentation::tui::environments::EnvironmentProbe {
                nodes: checked_nodes()?,
                ..crate::presentation::tui::environments::EnvironmentProbe::default()
            },
            at: std::time::Instant::now(),
        },
    );
    Ok(model)
}

/// The cluster's nodes as a check of them reports each: `mm target nodes`
/// and `mm target mesh status`, put into words by the same code as live.
fn checked_nodes() -> Result<Vec<crate::presentation::tui::NodeLine>, serde_json::Error> {
    use crate::domain::mesh::{MeshNodeReport, MeshNodeStatus, MeshPeerStatus};

    let peer = || MeshPeerStatus {
        name: None,
        public_key: "key".to_owned(),
        endpoint: None,
        configured_endpoint: None,
        allowed_ips: Vec::new(),
        handshake_age_seconds: Some(40),
        received_bytes: 0,
        sent_bytes: 0,
        on_interface: true,
    };
    ["app-1", "app-2", "app-3"]
        .into_iter()
        .map(|name| {
            let report: crate::domain::remote::NodeReport =
                serde_json::from_value(serde_json::json!({
                    "name": name, "host": "203.0.113.11", "port": 22, "state": "ready",
                    "reachable": true, "mmd_version": "0.1.0",
                    "deployed": {
                        "commit": "3f2c9a1e5b7d40c8a1e2",
                        "deployed_at_unix_ms": 1_794_633_060_000_i64,
                        "deployed_by": "alice", "runs_services": true
                    }
                }))?;
            let mesh = MeshNodeReport {
                name: name.to_owned(),
                host: report.host.clone(),
                reachable: true,
                problem: None,
                status: Some(MeshNodeStatus {
                    interface: Some("mm0".to_owned()),
                    up: true,
                    peers: vec![peer(), peer()],
                    ..MeshNodeStatus::default()
                }),
            };
            Ok(crate::presentation::tui::NodeLine::from(&report).with_mesh(Some(&mesh)))
        })
        .collect()
}

/// A local environment that still needs a build and the shared services, as
/// the setup card lists them.
pub(super) fn setup_view() -> crate::presentation::tui::setup::SetupView {
    use crate::domain::readiness::SetupIssue;
    use crate::presentation::tui::setup::{SetupStep, SetupView};

    let step = |issue: SetupIssue, command: &str| SetupStep {
        title: issue.title(),
        detail: issue.summary(),
        command: Some(command.to_owned()),
        code: Some(issue.diagnostic_code()),
        ready: true,
        issue: Some(issue),
    };
    SetupView {
        steps: vec![
            step(
                SetupIssue::MissingImages {
                    services: ["api/api", "docs/docs", "web/web", "web/queue"]
                        .map(str::to_owned)
                        .to_vec(),
                },
                "mm build --publish",
            ),
            step(
                SetupIssue::MissingDevelopmentServices {
                    network: "mm-services-dev-412092c800fc43085768".to_owned(),
                },
                "mm dev services plan",
            ),
        ],
        unavailable: None,
    }
}
