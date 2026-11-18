use std::sync::Arc;

use crate::app::ports::{
    ProjectDocumentStore, ReadinessProbe, RuntimeDriver, StateInspector, StateStore, TargetStore,
};
use crate::app::{WorkspaceSelection, load_selection_preferences};
use crate::domain::config::{ConfigDocument, resolve_environment};
use crate::domain::diagnostic::{Diagnostic, Severity, ValidationReport};
use crate::domain::error::MmError;
use crate::domain::readiness::{
    CheckStatus, ReadinessCheck, ReadinessReport, RuntimeCandidate, SetupIssue,
};
use crate::domain::runtime::{RuntimeKind, RuntimeProbe};
use crate::domain::target::{NamedTarget, TargetTransport};
use crate::domain::workspace::DoctorReport;

mod context;
mod development_services;
mod report;
mod tls;
pub use development_services::DevelopmentServicesCheck;
use report::InspectionReport;
pub use tls::LocalTlsCheck;

/// Read-only prerequisite orchestration. All entry points consume the same
/// typed report; optional probes extend it without changing the presenters.
pub struct ReadinessService {
    documents: Arc<dyn ProjectDocumentStore>,
    state: Arc<dyn StateInspector>,
    targets: Arc<dyn TargetStore>,
    runtimes: Vec<Arc<dyn RuntimeDriver>>,
    probes: Vec<Arc<dyn ReadinessProbe>>,
}

impl ReadinessService {
    pub fn new(
        documents: Arc<dyn ProjectDocumentStore>,
        state: Arc<dyn StateInspector>,
        targets: Arc<dyn TargetStore>,
        runtimes: Vec<Arc<dyn RuntimeDriver>>,
    ) -> Self {
        Self {
            documents,
            state,
            targets,
            runtimes,
            probes: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_probe(mut self, probe: Arc<dyn ReadinessProbe>) -> Self {
        self.probes.push(probe);
        self
    }

    pub async fn inspect(&self, selection: &WorkspaceSelection) -> DoctorReport {
        let mut report = InspectionReport::new(selection.runtime);
        let state = report.record("state", self.state.inspect());
        let catalog = report.record("targets", self.targets.list_targets());
        match self.documents.load_from(&selection.start) {
            Ok(document) => {
                self.inspect_project(selection, &document, state, catalog, &mut report)
                    .await
            }
            Err(MmError::ConfigNotFound { .. }) => {
                report.doctor.validation.push(
                    Diagnostic::warning(
                        "MM_CONFIG_NOT_FOUND",
                        "no mm project was found; inspecting this host",
                    )
                    .with_help("run `mm init` from a project root"),
                );
                self.probe_selected(
                    selection.runtime.unwrap_or(RuntimeKind::PodmanCompose),
                    &mut report,
                )
                .await;
            }
            Err(error) => {
                report.record::<()>("configuration", Err(error));
            }
        }
        for probe in &self.probes {
            if !probe.dependencies().iter().all(|name| report.passed(name)) {
                report.blocked(probe.name(), probe.dependencies());
                continue;
            }
            if let Some(diagnostics) = report.record(probe.name(), probe.inspect(selection).await) {
                report.diagnostics(probe.name(), diagnostics);
            }
        }
        report.finish()
    }

    async fn inspect_project(
        &self,
        selection: &WorkspaceSelection,
        document: &ConfigDocument,
        state: Option<Option<Arc<dyn StateStore>>>,
        catalog: Option<Vec<NamedTarget>>,
        report: &mut InspectionReport,
    ) {
        report.doctor.project_name = Some(document.config.project.name.clone());
        report.doctor.root = Some(document.root.clone());
        let Some(environment) = report.record(
            "environment",
            resolve_environment(document, selection.environment.as_deref()),
        ) else {
            report.blocked("selection", &["environment"]);
            report.blocked("images", &["environment"]);
            return;
        };
        report.doctor.environment = Some(environment.clone());
        report.doctor.environment_delivery = Some(
            crate::domain::environment_delivery::resolve_environment_delivery(
                &document.config,
                &environment,
            ),
        );
        let validation = super::config_validation::validate(document, &environment);
        let valid = validation.is_valid();
        report.diagnostics("configuration", validation.diagnostics);
        let Some(state) = state else {
            report.blocked("images", &["state"]);
            report.blocked("selection", &["state"]);
            // Configured names can still be inspected, but cannot be called the
            // effective selection until private preferences are available.
            if catalog.as_ref().is_some_and(|targets| {
                !targets
                    .iter()
                    .any(|target| target.name == document.config.target)
            }) {
                report.record(
                    "configured target",
                    super::selection::resolve_named_target(
                        document,
                        &document.config.target,
                        self.targets.as_ref(),
                    ),
                );
            }
            return;
        };
        if valid {
            report.record(
                "images",
                super::stacks::inspect_deployment_images(document, &environment, state.as_deref()),
            );
        } else {
            report.blocked("images", &["configuration"]);
        }
        if catalog.is_none() {
            report.blocked("selection", &["targets"]);
            return;
        }
        self.inspect_selection(selection, document, &environment, state.as_deref(), report)
            .await;
    }

    async fn inspect_selection(
        &self,
        selection: &WorkspaceSelection,
        document: &ConfigDocument,
        environment: &str,
        state: Option<&dyn StateStore>,
        report: &mut InspectionReport,
    ) {
        let Some((environment_pref, project_pref)) = report.record(
            "preferences",
            load_selection_preferences(state, document, environment),
        ) else {
            report.blocked("selection", &["preferences"]);
            return;
        };
        let Some((name, source)) = report.record(
            "target selection",
            super::selection::resolve_target_name(
                document,
                environment,
                selection.target.as_deref(),
                environment_pref.as_ref(),
                project_pref.as_ref(),
            ),
        ) else {
            return;
        };
        report.doctor.target = Some(name.clone());
        report.doctor.target_source = Some(source.as_str().to_owned());
        let Some(target) = report.record(
            "target",
            super::selection::resolve_named_target(document, &name, self.targets.as_ref()),
        ) else {
            report.blocked("runtime", &["target"]);
            return;
        };
        let resolved = report.record(
            "selection",
            super::selection::resolve_runtime_and_target(
                document,
                environment,
                selection.runtime,
                selection.target.as_deref(),
                environment_pref.as_ref(),
                project_pref.as_ref(),
                &target,
            ),
        );
        let Some(resolved) = resolved else {
            report.blocked("runtime", &["selection"]);
            if report.has_issue(|issue| matches!(issue, SetupIssue::MissingRuntime)) {
                self.probe_candidates(&target, report).await;
            }
            return;
        };
        report.doctor.runtime_kind = Some(resolved.runtime);
        report.doctor.runtime_source = Some(resolved.runtime_source.as_str().to_owned());
        report.doctor.project_selected = true;
        if target.definition.transport == TargetTransport::Local {
            self.probe_selected(resolved.runtime, report).await;
        } else {
            report.doctor.runtime = RuntimeProbe::on_remote_hosts(resolved.runtime);
        }
        if report.passed("configuration") && report.passed("images") {
            if let Some(ingress) = report.record(
                "stacks",
                super::stacks::inspect_document(document, environment, &target, state),
            ) {
                report.doctor.ingress = ingress;
            }
        } else {
            report.blocked("stacks", &["configuration", "images"]);
        }
    }

    async fn probe_candidates(&self, target: &NamedTarget, report: &mut InspectionReport) {
        if target.definition.transport != TargetTransport::Local {
            return;
        }
        for runtime in &self.runtimes {
            if target.definition.available_runtimes.is_empty()
                || target
                    .definition
                    .available_runtimes
                    .contains(&runtime.kind())
            {
                report.readiness.candidates.push(RuntimeCandidate {
                    runtime: runtime.kind(),
                    probe: runtime.probe().await,
                });
            }
        }
    }

    async fn probe_selected(&self, kind: RuntimeKind, report: &mut InspectionReport) {
        let Some(driver) = self.runtimes.iter().find(|driver| driver.kind() == kind) else {
            report.blocked("runtime", &["runtime driver"]);
            return;
        };
        report.doctor.runtime = driver.probe().await;
        let healthy = report.doctor.runtime.available
            && !report
                .doctor
                .runtime
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error);
        report.check(
            "runtime",
            if healthy {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            },
            if healthy {
                "selected runtime is available"
            } else {
                "selected runtime is unavailable; see runtime diagnostics"
            },
        );
    }
}
