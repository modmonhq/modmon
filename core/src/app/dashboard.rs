use std::collections::BTreeMap;
use std::sync::Arc;

use crate::app::ports::{
    Clock, DashboardHostSource, InstallationStatusQuery, ProjectDocumentStore, SchedulerRepository,
    StateStore,
};
use crate::app::{RuntimeOperationsService, WorkspaceSelection};
use crate::domain::config::{ConfigDocument, resolve_environment};
use crate::domain::dashboard::{
    DASHBOARD_API_VERSION, DashboardComponent, DashboardDiagnostic, DashboardHealth, DashboardJob,
    DashboardJobRun, DashboardOperation, DashboardProjectSummary, DashboardResource,
    DashboardResourceKind, DashboardResponse, DashboardRoute, DashboardSchedule, DashboardScope,
    DashboardScopeDto, DashboardService, InstallationDashboard, MAX_DASHBOARD_DIAGNOSTICS,
    MAX_DASHBOARD_JOB_RUNS, MAX_DASHBOARD_OPERATIONS, MAX_DASHBOARD_PROJECTS,
    MAX_DASHBOARD_SERVICES, ProjectDashboard,
};
use crate::domain::diagnostic::{Diagnostic, Severity};
use crate::domain::error::MmError;
use crate::domain::local_ingress::project_dashboard_slug;
use crate::domain::ownership::ResourceKind;
use crate::domain::runtime::{ContainerObservation, ServiceState};
use crate::domain::workspace::WorkspaceRecord;

#[derive(Clone)]
pub struct DashboardQueryService {
    operations: RuntimeOperationsService,
    documents: Arc<dyn ProjectDocumentStore>,
    state: Arc<dyn StateStore>,
    scheduler: Arc<dyn SchedulerRepository>,
    installation: Arc<dyn InstallationStatusQuery>,
    clock: Arc<dyn Clock>,
}

#[derive(Clone)]
pub struct RegisteredDashboardHostSource {
    documents: Arc<dyn ProjectDocumentStore>,
    state: Arc<dyn StateStore>,
}

struct RegisteredDashboardProject {
    workspace: WorkspaceRecord,
    document: ConfigDocument,
}

fn registered_projects(
    documents: &dyn ProjectDocumentStore,
    state: &dyn StateStore,
) -> Result<BTreeMap<String, RegisteredDashboardProject>, MmError> {
    let workspaces = state.list_workspaces(MAX_DASHBOARD_PROJECTS + 1)?;
    if workspaces.len() > MAX_DASHBOARD_PROJECTS {
        return Err(MmError::State {
            message: format!(
                "dashboard project response exceeds the {MAX_DASHBOARD_PROJECTS}-project limit"
            ),
        });
    }
    let mut projects = BTreeMap::new();
    for workspace in workspaces {
        let document = documents.load_from(&workspace.root)?;
        if document.config.project.id != workspace.project_id
            || document.path != workspace.config_path
        {
            return Err(MmError::Security {
                message: "a registered dashboard workspace no longer matches its project owner"
                    .to_owned(),
            });
        }
        let slug =
            project_dashboard_slug(&document.config.project.name, document.config.project.id)?;
        if projects
            .insert(
                slug.clone(),
                RegisteredDashboardProject {
                    workspace,
                    document,
                },
            )
            .is_some()
        {
            return Err(MmError::Security {
                message: format!(
                    "multiple registered roots claim dashboard identity '{slug}'; host routing is disabled until the duplicate project UUID is resolved"
                ),
            });
        }
    }
    Ok(projects)
}

impl RegisteredDashboardHostSource {
    #[must_use]
    pub fn new(documents: Arc<dyn ProjectDocumentStore>, state: Arc<dyn StateStore>) -> Self {
        Self { documents, state }
    }
}

impl DashboardHostSource for RegisteredDashboardHostSource {
    fn project_dashboard_hosts(&self) -> Result<std::collections::BTreeSet<String>, MmError> {
        Ok(
            registered_projects(self.documents.as_ref(), self.state.as_ref())?
                .into_keys()
                .map(|slug| format!("mm.{slug}.test"))
                .collect(),
        )
    }
}

impl std::fmt::Debug for DashboardQueryService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DashboardQueryService")
            .finish_non_exhaustive()
    }
}

impl DashboardQueryService {
    #[must_use]
    pub fn new(
        operations: RuntimeOperationsService,
        documents: Arc<dyn ProjectDocumentStore>,
        state: Arc<dyn StateStore>,
        scheduler: Arc<dyn SchedulerRepository>,
        installation: Arc<dyn InstallationStatusQuery>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            operations,
            documents,
            state,
            scheduler,
            installation,
            clock,
        }
    }

    pub fn authorize_scope(&self, scope: &DashboardScope) -> Result<(), MmError> {
        let DashboardScope::Project(slug) = scope else {
            return Ok(());
        };
        if registered_projects(self.documents.as_ref(), self.state.as_ref())?.contains_key(slug) {
            Ok(())
        } else {
            Err(MmError::Security {
                message: "the requested dashboard host is not owned by a registered project"
                    .to_owned(),
            })
        }
    }

    pub async fn query(&self, scope: DashboardScope) -> Result<DashboardResponse, MmError> {
        let installation = self.installation.installation_status().await?;
        let owned = registered_projects(self.documents.as_ref(), self.state.as_ref())?;

        let projects = owned
            .iter()
            .map(|(slug, project)| {
                let environment = resolve_environment(&project.document, None)?;
                Ok(DashboardProjectSummary {
                    project_id: project.workspace.project_id,
                    name: project.document.config.project.name.clone(),
                    slug: slug.clone(),
                    dashboard_url: format!("https://mm.{slug}.test"),
                    environment,
                })
            })
            .collect::<Result<Vec<_>, MmError>>()?;

        let (scope_dto, project) = match scope {
            DashboardScope::Installation => (DashboardScopeDto::Installation, None),
            DashboardScope::Project(slug) => {
                let project = owned.get(&slug).ok_or_else(|| MmError::Security {
                    message: "the requested dashboard host is not owned by a registered project"
                        .to_owned(),
                })?;
                let environment = resolve_environment(&project.document, None)?;
                let snapshot = self
                    .operations
                    .status(
                        &WorkspaceSelection::new(
                            project.workspace.root.clone(),
                            Some(environment.clone()),
                        ),
                        &crate::domain::observability::ApprovalContext {
                            principal: "local-dashboard".to_owned(),
                            source: "dashboard".to_owned(),
                            approved_high_risk: false,
                        },
                    )
                    .await?;
                let delivery = crate::domain::environment_delivery::resolve_environment_delivery(
                    &project.document.config,
                    &environment,
                );
                let definitions = self
                    .scheduler
                    .list_jobs(project.document.config.project.id, &environment)?;
                let schedules = self
                    .scheduler
                    .list_schedules(project.document.config.project.id, &environment)?;
                let runs = self.scheduler.history(
                    project.document.config.project.id,
                    &environment,
                    None,
                    None,
                    MAX_DASHBOARD_JOB_RUNS,
                )?;
                (
                    DashboardScopeDto::Project { slug: slug.clone() },
                    Some(project_dashboard(
                        slug,
                        snapshot,
                        delivery,
                        definitions,
                        schedules,
                        runs,
                    )?),
                )
            }
        };

        Ok(DashboardResponse {
            api_version: DASHBOARD_API_VERSION,
            generated_at_unix_ms: self.clock.now_unix_ms(),
            scope: scope_dto,
            installation: InstallationDashboard {
                installed: installation.installed,
                stack_version: installation.stack_version,
                runtime: installation.runtime,
                route: "https://mm.test".to_owned(),
                components: installation
                    .components
                    .into_iter()
                    .map(|component| DashboardComponent {
                        kind: component.kind,
                        version: component.version,
                        running: component.running,
                        healthy: component.healthy,
                    })
                    .collect(),
                diagnostics: safe_diagnostics(installation.diagnostics),
            },
            projects,
            project,
        })
    }
}

fn project_dashboard(
    slug: String,
    snapshot: crate::domain::workspace::WorkspaceSnapshot,
    environment_delivery: crate::domain::environment_delivery::ResolvedEnvironmentDelivery,
    definitions: crate::domain::scheduler::DesiredDefinitions,
    schedule_records: Vec<crate::domain::scheduler::ScheduleRecord>,
    job_history: Vec<crate::domain::scheduler::JobRun>,
) -> Result<ProjectDashboard, MmError> {
    let services = dashboard_services(&snapshot)?;
    let resources = snapshot
        .stacks
        .iter()
        .flat_map(|stack| &stack.owned_resources)
        .filter_map(|resource| {
            let kind = match resource.kind {
                ResourceKind::Network => DashboardResourceKind::Network,
                ResourceKind::Volume => DashboardResourceKind::Volume,
                _ => return None,
            };
            Some(DashboardResource {
                kind,
                logical_name: resource.logical_name.clone(),
            })
        })
        .collect();
    let mut diagnostics = snapshot.validation.diagnostics;
    diagnostics.extend(snapshot.runtime.diagnostics.clone());
    let optional = snapshot.optional;

    Ok(ProjectDashboard {
        project_id: snapshot.workspace.project_id,
        name: snapshot.project_name,
        slug,
        environment: snapshot.environment,
        target: snapshot.target,
        runtime: snapshot.runtime_kind,
        runtime_available: snapshot.runtime.available,
        config_revision: snapshot.config_hash.chars().take(12).collect(),
        environment_delivery,
        services,
        resources,
        operations: snapshot
            .recent_operations
            .into_iter()
            .take(MAX_DASHBOARD_OPERATIONS)
            .map(|operation| DashboardOperation {
                kind: operation.kind,
                state: operation.state,
                requested_at_unix_ms: operation.requested_at_unix_ms,
                finished_at_unix_ms: operation.finished_at_unix_ms,
                error_code: operation.error_code.filter(|code| safe_code(code)),
            })
            .collect(),
        jobs: definitions
            .jobs
            .into_iter()
            .map(|(name, job)| DashboardJob {
                name,
                module: job.module,
                service: job.service,
                timeout: job.timeout.0,
                maximum_attempts: job.retry.attempts,
            })
            .collect(),
        schedules: schedule_records
            .into_iter()
            .map(|schedule| DashboardSchedule {
                name: schedule.name,
                job: schedule.job,
                enabled: schedule.enabled,
                retired: schedule.retired,
                next_due_at_unix_ms: schedule.next_due_at_unix_ms,
            })
            .collect(),
        job_runs: job_history
            .into_iter()
            .take(MAX_DASHBOARD_JOB_RUNS)
            .map(|run| DashboardJobRun {
                id: run.id,
                job: run.job,
                schedule: run.schedule,
                state: run.state,
                attempt: run.attempt,
                requested_at_unix_ms: run.requested_at_unix_ms,
                finished_at_unix_ms: run.finished_at_unix_ms,
                exit_code: run.exit_code,
                error_code: run.error_code.filter(|code| safe_code(code)),
                output_truncated: run.output_truncated,
            })
            .collect(),
        diagnostics: safe_diagnostics(diagnostics),
        optional,
    })
}

fn dashboard_services(
    snapshot: &crate::domain::workspace::WorkspaceSnapshot,
) -> Result<Vec<DashboardService>, MmError> {
    let mut routes = BTreeMap::<(String, String), Vec<DashboardRoute>>::new();
    for route in snapshot.ingress.iter().filter_map(|route| {
        Some((
            (route.stack.clone(), route.service.clone()),
            DashboardRoute {
                url: safe_route_url(route.url.as_deref()?)?,
                secure: route.tls?,
                provenance: route.provenance?,
            },
        ))
    }) {
        routes.entry(route.0).or_default().push(route.1);
    }

    let mut services = Vec::new();
    for stack in &snapshot.stacks {
        for service in &stack.services {
            let first = service.containers.first();
            services.push(DashboardService {
                name: service.name.clone(),
                app: service.module.clone(),
                stack: stack.name.clone(),
                state: service.state,
                health: service_health(service.state, &service.containers),
                desired_count: service.desired_count,
                running_count: service.running_count,
                healthy_count: service.healthy_count,
                restart_count: service.restart_count,
                exit_code: service.exit_code,
                diagnostic_code: service
                    .diagnostic_code
                    .as_deref()
                    .filter(|code| safe_code(code))
                    .map(str::to_owned),
                image: first.and_then(|container| public_image(&container.image)),
                revision: first.and_then(public_revision),
                routes: routes
                    .remove(&(stack.name.clone(), service.name.clone()))
                    .unwrap_or_default(),
            });
        }
    }
    let service_count = snapshot
        .stacks
        .iter()
        .map(|stack| stack.services.len())
        .sum::<usize>();
    if service_count > MAX_DASHBOARD_SERVICES {
        return Err(MmError::State {
            message: format!(
                "dashboard service response exceeds the {MAX_DASHBOARD_SERVICES}-service limit"
            ),
        });
    }

    Ok(services)
}

fn service_health(state: ServiceState, containers: &[ContainerObservation]) -> DashboardHealth {
    match state {
        ServiceState::Running
            if containers.iter().all(|container| {
                container
                    .health
                    .as_deref()
                    .is_none_or(|health| health.eq_ignore_ascii_case("healthy"))
            }) =>
        {
            DashboardHealth::Healthy
        }
        ServiceState::JobOnly => DashboardHealth::Healthy,
        ServiceState::Running | ServiceState::Degraded => DashboardHealth::Degraded,
        ServiceState::Stopped | ServiceState::Missing => DashboardHealth::Stopped,
        ServiceState::Unknown => DashboardHealth::Unknown,
    }
}

fn public_image(image: &str) -> Option<String> {
    let without_digest = image.rsplit_once("@sha256:").map_or_else(
        || image.rsplit('@').next().unwrap_or(image),
        |(name, _)| name,
    );
    let final_segment = without_digest.rsplit('/').next()?;
    let repository = final_segment.split(':').next()?;
    (!repository.is_empty()
        && repository.len() <= 128
        && repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    .then(|| repository.to_owned())
}

fn public_revision(container: &ContainerObservation) -> Option<String> {
    container
        .digest
        .as_deref()
        .or(container.revision.as_deref())
        .and_then(|value| {
            let value = value.strip_prefix("sha256:").unwrap_or(value);
            (value.len() >= 7
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b'-'))
            .then(|| value.chars().take(12).collect())
        })
}

fn safe_route_url(value: &str) -> Option<String> {
    let (scheme, rest) = value.split_once("://")?;
    if !matches!(scheme, "http" | "https") || rest.contains('@') || rest.len() > 253 {
        return None;
    }
    rest.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'/' | b'_'))
        .then(|| value.to_owned())
}

fn safe_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<DashboardDiagnostic> {
    diagnostics
        .into_iter()
        .take(MAX_DASHBOARD_DIAGNOSTICS)
        .map(|diagnostic| DashboardDiagnostic {
            summary: match diagnostic.severity {
                Severity::Info => "A local configuration detail needs attention.",
                Severity::Warning => "A local service may need attention.",
                Severity::Error => "A local service check failed.",
            }
            .to_owned(),
            suggested_action: suggested_action(&diagnostic.code),
            code: if safe_code(&diagnostic.code) {
                diagnostic.code
            } else {
                "MM_DIAGNOSTIC_REDACTED".to_owned()
            },
            severity: diagnostic.severity,
        })
        .collect()
}

fn safe_code(value: &str) -> bool {
    value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn suggested_action(code: &str) -> Option<String> {
    let command = match code {
        "MM_CONFIG_NOT_FOUND" => "Run `mm init` from the project folder.",
        "MM_SERVICES_NOT_INSTALLED" => "Run `mm dev services plan`, review it, then apply it.",
        "MM_SERVICES_RECOVERY_REQUIRED" => "Review and run `mm dev services repair`.",
        "MM_RUNTIME_CAPABILITY_MISSING" | "MM_RUNTIME_UNSUPPORTED" => {
            "Run `mm doctor` for runtime guidance."
        }
        _ => return Some("Run `mm doctor` for a detailed, secret-safe diagnosis.".to_owned()),
    };
    Some(command.to_owned())
}

#[async_trait::async_trait]
impl InstallationStatusQuery for crate::app::ServiceStackManager {
    async fn installation_status(
        &self,
    ) -> Result<crate::domain::service_stack::ServiceStackStatus, MmError> {
        self.status().await
    }
}

#[cfg(test)]
mod tests;
