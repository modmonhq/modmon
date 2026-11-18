//! What the dashboard's projections keep and drop. The query service is
//! tested over real stores in `tests/units/dashboard_query.rs`.

use super::{
    dashboard_services, project_dashboard, public_image, public_revision, safe_code,
    safe_diagnostics, safe_route_url, service_health, suggested_action,
};
use crate::domain::config::{
    DurationSpec, JobConfig, RetryPolicyConfig, ScheduleConcurrency, ScheduleConfig,
};
use crate::domain::dashboard::{
    DashboardHealth, MAX_DASHBOARD_DIAGNOSTICS, MAX_DASHBOARD_JOB_RUNS, MAX_DASHBOARD_OPERATIONS,
    MAX_DASHBOARD_SERVICES,
};
use crate::domain::diagnostic::{Diagnostic, Severity, ValidationReport};
use crate::domain::environment_delivery::ResolvedEnvironmentDelivery;
use crate::domain::ingress::{IngressRouteProvenance, IngressServiceStatus};
use crate::domain::operation::{OperationId, OperationKind, OperationState, OperationSummary};
use crate::domain::ownership::ResourceKind;
use crate::domain::runtime::{
    ContainerObservation, RuntimeKind, RuntimeProbe, ServiceSnapshot, ServiceState,
};
use crate::domain::scheduler::{
    DesiredDefinitions, JobRun, JobRunState, JobTrigger, ScheduleRecord,
};
use crate::domain::workspace::{
    OwnedResourceSnapshot, ProjectId, StackSnapshot, WorkspaceId, WorkspaceRecord,
    WorkspaceSnapshot,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

const SENTINEL: &str = "mm-dashboard-secret-sentinel-9f3c2a1b";

fn container() -> ContainerObservation {
    ContainerObservation {
        id: "private-id".to_owned(),
        name: "private-name".to_owned(),
        service: "web".to_owned(),
        image: "registry.example/team/web:latest@sha256:abcd".to_owned(),
        state: "running".to_owned(),
        status: "running".to_owned(),
        health: Some("HEALTHY".to_owned()),
        restart_count: Some(2),
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        revision: Some("abcdef0123456789".to_owned()),
        digest: None,
        diagnostic_code: None,
        error: None,
        labels: BTreeMap::new(),
        node: None,
        history: false,
    }
}

fn service() -> ServiceSnapshot {
    ServiceSnapshot {
        stack: "app".to_owned(),
        name: "web".to_owned(),
        module: Some("api".to_owned()),
        nodes: Vec::new(),
        state: ServiceState::Running,
        starting: false,
        desired_count: 1,
        running_count: 1,
        healthy_count: 1,
        image: None,
        revision: None,
        restart_count: Some(2),
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        diagnostic_code: Some("MM_HEALTHY".to_owned()),
        error: None,
        containers: vec![container()],
        scale: None,
        restarts: None,
    }
}

fn snapshot() -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        workspace: WorkspaceRecord {
            id: WorkspaceId(Uuid::from_u128(1)),
            project_id: ProjectId(Uuid::from_u128(2)),
            root: PathBuf::from("/workspace"),
            config_path: PathBuf::from("/workspace/mm.yaml"),
        },
        project_name: "dashboard".to_owned(),
        target: "local".to_owned(),
        environment: "local".to_owned(),
        config_hash: "abcdef0123456789".to_owned(),
        target_source: Some("project".to_owned()),
        runtime_kind: Some(RuntimeKind::DockerCompose),
        runtime_source: Some("project".to_owned()),
        runtime: RuntimeProbe {
            kind: RuntimeKind::DockerCompose,
            available: true,
            engine_version: Some("27".to_owned()),
            compose_provider: None,
            rootless: Some(false),
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: vec![Diagnostic::warning("MM_RUNTIME_WARNING", SENTINEL)],
        },
        validation: ValidationReport {
            diagnostics: vec![Diagnostic::info("MM_CONFIG_DETAIL", SENTINEL)],
        },
        stacks: vec![StackSnapshot {
            name: "app".to_owned(),
            compose_project_name: "dashboard-local-app".to_owned(),
            artifact_hash: "a".repeat(64),
            rendered_config_hash: "b".repeat(64),
            services: vec![service()],
            owned_resources: vec![
                OwnedResourceSnapshot {
                    kind: ResourceKind::Network,
                    logical_name: "frontend".to_owned(),
                },
                OwnedResourceSnapshot {
                    kind: ResourceKind::Volume,
                    logical_name: "data".to_owned(),
                },
                OwnedResourceSnapshot {
                    kind: ResourceKind::Config,
                    logical_name: "private-config".to_owned(),
                },
            ],
        }],
        ingress: vec![
            IngressServiceStatus {
                module: "api".to_owned(),
                stack: "app".to_owned(),
                service: "web".to_owned(),
                service_slug: Some("web".to_owned()),
                url: Some("https://web.dashboard.test/path".to_owned()),
                selected_port: Some(8080),
                tls: Some(true),
                provenance: Some(IngressRouteProvenance::ExplicitConfig),
                certificate_reference: None,
                ownership: None,
                reason: None,
                warning: None,
            },
            IngressServiceStatus {
                module: "api".to_owned(),
                stack: "app".to_owned(),
                service: "web".to_owned(),
                service_slug: None,
                url: Some("https://user:secret@web.dashboard.test".to_owned()),
                selected_port: Some(8080),
                tls: Some(true),
                provenance: Some(IngressRouteProvenance::ExplicitConfig),
                certificate_reference: None,
                ownership: None,
                reason: None,
                warning: None,
            },
        ],
        recent_operations: (0..=MAX_DASHBOARD_OPERATIONS)
            .map(|index| OperationSummary {
                id: OperationId(Uuid::from_u128(index as u128 + 10)),
                kind: OperationKind::Deploy,
                environment: "local".to_owned(),
                state: OperationState::Succeeded,
                requested_at_unix_ms: index as i64,
                finished_at_unix_ms: Some(index as i64 + 1),
                error_code: Some(if index == 0 {
                    "unsafe-code".to_owned()
                } else {
                    "MM_OK".to_owned()
                }),
            })
            .collect(),
        nodes: Vec::new(),
        names: None,
        deployment: None,
        drift: Vec::new(),
        network_drift: Vec::new(),
        optional: Vec::new(),
        tunnel: None,
    }
}

fn definitions() -> DesiredDefinitions {
    DesiredDefinitions {
        jobs: BTreeMap::from([(
            "task".to_owned(),
            JobConfig {
                module: "api".to_owned(),
                service: "web".to_owned(),
                command: vec!["run".to_owned()],
                environments: None,
                run: None,
                timeout: DurationSpec("10m".to_owned()),
                retry: RetryPolicyConfig::default(),
                output_limit_bytes: 1024,
                history_limit: 100,
                placement: None,
                user: None,
                placed_elsewhere: None,
                development: None,
                off_locally: None,
            },
        )]),
        schedules: BTreeMap::from([(
            "every-minute".to_owned(),
            ScheduleConfig {
                job: "task".to_owned(),
                cron: "* * * * *".to_owned(),
                timezone: "UTC".to_owned(),
                enabled: true,
                starts_at: None,
                ends_at: None,
                jitter: DurationSpec("0s".to_owned()),
                concurrency: ScheduleConcurrency::Forbid,
                misfire: Default::default(),
                retry: None,
                timeout: None,
            },
        )]),
    }
}

fn run(index: usize) -> JobRun {
    JobRun {
        id: Uuid::from_u128(index as u128 + 100),
        project_id: ProjectId(Uuid::from_u128(2)),
        environment: "local".to_owned(),
        job: "task".to_owned(),
        schedule: Some("every-minute".to_owned()),
        trigger: JobTrigger::Schedule,
        state: JobRunState::Succeeded,
        attempt: 1,
        maximum_attempts: 1,
        fence_token: 1,
        runtime_name: "private-runtime-name".to_owned(),
        runtime_id: Some("private-runtime-id".to_owned()),
        requested_at_unix_ms: index as i64,
        scheduled_for_unix_ms: Some(index as i64),
        started_at_unix_ms: Some(index as i64),
        finished_at_unix_ms: Some(index as i64 + 1),
        next_attempt_at_unix_ms: None,
        exit_code: Some(0),
        error_code: Some(if index == 0 { "unsafe-code" } else { "MM_OK" }.to_owned()),
        output: SENTINEL.to_owned(),
        output_truncated: false,
        cancellation_requested: false,
        principal: SENTINEL.to_owned(),
        source: SENTINEL.to_owned(),
        node: None,
    }
}

#[test]
fn dashboard_serializers_drop_credentials_raw_diagnostics_and_sensitive_runtime_fields() {
    let mut value = container();
    value.id = SENTINEL.to_owned();
    value.name = SENTINEL.to_owned();
    value.image = format!("user:{SENTINEL}@registry.example/web:latest");
    value.status = SENTINEL.to_owned();
    value.revision = Some(SENTINEL.to_owned());
    value.labels = BTreeMap::from([("token".to_owned(), SENTINEL.to_owned())]);
    assert_eq!(public_image(&value.image).as_deref(), Some("web"));
    assert!(public_revision(&value).is_none());
    assert!(safe_route_url(&format!("https://user:{SENTINEL}@example.test")).is_none());
    let diagnostics = safe_diagnostics(vec![
        Diagnostic::error("MM_RUNTIME_RESPONSE", SENTINEL).with_help(SENTINEL),
    ]);
    let encoded = serde_json::to_string(&diagnostics).unwrap_or_default();
    assert!(!encoded.contains(SENTINEL));
}

#[test]
fn dashboard_public_fields_are_bounded_and_health_is_derived_from_runtime_state() {
    let mut healthy = container();
    assert_eq!(public_image(&healthy.image).as_deref(), Some("web"));
    assert_eq!(public_revision(&healthy).as_deref(), Some("abcdef012345"));
    healthy.digest = Some("sha256:1234567890abcdef".to_owned());
    assert_eq!(public_revision(&healthy).as_deref(), Some("1234567890ab"));

    assert_eq!(
        service_health(ServiceState::Running, &[healthy.clone()]),
        DashboardHealth::Healthy
    );
    healthy.health = Some("unhealthy".to_owned());
    assert_eq!(
        service_health(ServiceState::Running, &[healthy]),
        DashboardHealth::Degraded
    );
    assert_eq!(
        service_health(ServiceState::Degraded, &[]),
        DashboardHealth::Degraded
    );
    assert_eq!(
        service_health(ServiceState::Stopped, &[]),
        DashboardHealth::Stopped
    );
    assert_eq!(
        service_health(ServiceState::Missing, &[]),
        DashboardHealth::Stopped
    );
    assert_eq!(
        service_health(ServiceState::Unknown, &[]),
        DashboardHealth::Unknown
    );

    for (value, expected) in [
        (
            "https://app.example.test/path_1",
            Some("https://app.example.test/path_1"),
        ),
        ("ftp://app.example.test", None),
        ("https://user@app.example.test", None),
        ("not-a-url", None),
    ] {
        assert_eq!(safe_route_url(value).as_deref(), expected);
    }
    assert!(safe_route_url(&format!("https://{}", "a".repeat(254))).is_none());
    assert!(public_image("").is_none());
    assert!(public_image("registry.example/team/invalid$name:latest").is_none());
    assert!(public_image(&format!("{}:latest", "a".repeat(129))).is_none());

    for value in ["MM_OK", "MM_CODE_123"] {
        assert!(safe_code(value));
    }
    for value in ["mm_lower", "MM-DASH", "MM CODE", &"A".repeat(81)] {
        assert!(!safe_code(value));
    }

    let diagnostics = safe_diagnostics(vec![
        Diagnostic::info("MM_CONFIG_NOT_FOUND", "private"),
        Diagnostic::warning("MM_SERVICES_NOT_INSTALLED", "private"),
        Diagnostic::error("MM_SERVICES_RECOVERY_REQUIRED", "private"),
        Diagnostic::error("MM_RUNTIME_UNSUPPORTED", "private"),
        Diagnostic::error("unsafe-code", "private"),
    ]);
    assert_eq!(diagnostics[0].severity, Severity::Info);
    assert_eq!(diagnostics[1].severity, Severity::Warning);
    assert_eq!(diagnostics[2].severity, Severity::Error);
    assert_eq!(diagnostics[4].code, "MM_DIAGNOSTIC_REDACTED");
    assert!(
        diagnostics
            .iter()
            .all(|item| item.suggested_action.is_some())
    );
    assert_eq!(
        suggested_action("MM_RUNTIME_CAPABILITY_MISSING"),
        Some("Run `mm doctor` for runtime guidance.".to_owned())
    );
}

#[test]
fn project_dashboard_projects_only_bounded_public_semantic_state()
-> Result<(), Box<dyn std::error::Error>> {
    let schedule = ScheduleRecord {
        name: "every-minute".to_owned(),
        project_id: ProjectId(Uuid::from_u128(2)),
        environment: "local".to_owned(),
        job: "task".to_owned(),
        enabled: true,
        retired: false,
        next_due_at_unix_ms: Some(1000),
        definition_hash: "private-definition-hash".to_owned(),
        generation: 1,
    };
    let dashboard = project_dashboard(
        "dashboard-slug".to_owned(),
        snapshot(),
        ResolvedEnvironmentDelivery::default(),
        definitions(),
        vec![schedule],
        (0..=MAX_DASHBOARD_JOB_RUNS).map(run).collect(),
    )?;
    assert_eq!(dashboard.services.len(), 1);
    assert_eq!(dashboard.services[0].routes.len(), 1);
    assert_eq!(dashboard.resources.len(), 2);
    assert_eq!(dashboard.operations.len(), MAX_DASHBOARD_OPERATIONS);
    assert_eq!(dashboard.operations[0].error_code, None);
    assert_eq!(dashboard.jobs.len(), 1);
    assert_eq!(dashboard.schedules.len(), 1);
    assert_eq!(dashboard.job_runs.len(), MAX_DASHBOARD_JOB_RUNS);
    assert_eq!(dashboard.job_runs[0].error_code, None);
    assert!(dashboard.diagnostics.len() <= MAX_DASHBOARD_DIAGNOSTICS);
    let encoded = serde_json::to_string(&dashboard)?;
    assert!(!encoded.contains(SENTINEL));
    assert!(!encoded.contains("private-runtime"));
    assert!(!encoded.contains("private-definition-hash"));

    let mut oversized = snapshot();
    oversized.stacks[0].services = vec![service(); MAX_DASHBOARD_SERVICES + 1];
    assert!(dashboard_services(&oversized).is_err());
    Ok(())
}
