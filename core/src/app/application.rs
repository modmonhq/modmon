use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::StackManager;
use crate::app::config_validation::validate;
use crate::app::ports::{
    Clock, EnvironmentSourceDiscovery, ProjectDocumentStore, RuntimeDriver, StateStore,
};
use crate::domain::config::{ConfigDocument, resolve_environment};
use crate::domain::diagnostic::{Diagnostic, ValidationReport};
use crate::domain::environment_delivery::resolve_environment_delivery;
use crate::domain::error::MmError;
use crate::domain::runtime::{
    ContainerObservation, ObservedProject, RenderedProject, RuntimeCapability, RuntimeProbe,
    ServiceSnapshot, ServiceState,
};
use crate::domain::workspace::{
    DoctorReport, InitializedProject, ValidationSnapshot, WorkspaceRegistration, WorkspaceSnapshot,
};

mod snapshot;
use snapshot::*;
mod stack_status;
pub use stack_status::StatusRead;

#[derive(Debug, Clone)]
pub struct WorkspaceSelection {
    pub start: PathBuf,
    pub environment: Option<String>,
    pub runtime: Option<crate::domain::runtime::RuntimeKind>,
    pub target: Option<String>,
    /// Set by `mmd` on a cluster node: the node this request is for. The
    /// services it selects read the configuration as that node sees it.
    pub node: Option<crate::domain::placement::NodeScope>,
    /// Set by `mmd`: what its host policy says about OCI runtimes. Plans
    /// made from this selection keep to it.
    pub host_runtimes: Option<Arc<crate::domain::hardening::HostRuntimes>>,
    /// Set by `mmd` for an environment on rootful Podman: where each
    /// service gets user ids of its own that mm chooses and knows.
    pub user_ids: Option<Arc<dyn crate::app::ports::ServiceUserIds>>,
    /// Set by `mmd`: per-environment variables come from the values set on
    /// this host, not from this machine's overrides.
    pub host_values: bool,
    /// The release a deployment plans: what `MM.` and `RELEASE.` keys
    /// read. Unset when nothing is deployed, such as for `mm dev`.
    pub release: Option<Arc<crate::domain::release_facts::ReleaseFacts>>,
    /// `mm dev up --with`: optional units turned on for this run only.
    pub development_with: Vec<String>,
}

/// How a request reads the project's configuration: as one cluster node
/// sees it, and under the host's runtime policy. Both are unset on the
/// machine `mm` runs on.
#[derive(Debug, Clone, Copy, Default)]
pub struct DocumentScope<'a> {
    pub node: Option<&'a crate::domain::placement::NodeScope>,
    pub host_runtimes: Option<&'a crate::domain::hardening::HostRuntimes>,
    pub user_ids: Option<&'a Arc<dyn crate::app::ports::ServiceUserIds>>,
    pub development_with: &'a [String],
}

impl WorkspaceSelection {
    #[must_use]
    pub fn new(start: impl Into<PathBuf>, environment: Option<String>) -> Self {
        Self {
            start: start.into(),
            environment,
            runtime: None,
            target: None,
            node: None,
            host_runtimes: None,
            user_ids: None,
            host_values: false,
            release: None,
            development_with: Vec::new(),
        }
    }

    /// This selection for the release `release`.
    #[must_use]
    pub fn with_release(
        &self,
        release: Option<crate::domain::release_facts::ReleaseFacts>,
    ) -> Self {
        let mut selection = self.clone();
        selection.release = release.map(Arc::new);
        selection
    }

    /// How this selection's requests read the configuration.
    #[must_use]
    pub fn document_scope(&self) -> DocumentScope<'_> {
        DocumentScope {
            node: self.node.as_ref(),
            host_runtimes: self.host_runtimes.as_deref(),
            user_ids: self.user_ids.as_ref(),
            development_with: &self.development_with,
        }
    }
}

#[derive(Clone)]
pub struct Application {
    documents: Arc<dyn ProjectDocumentStore>,
    environment_sources: Arc<dyn EnvironmentSourceDiscovery>,
    runtime: Arc<dyn RuntimeDriver>,
    state: Option<Arc<dyn StateStore>>,
    stacks: Option<StackManager>,
    clock: Arc<dyn Clock>,
}

#[derive(Clone)]
pub struct ApplicationDependencies {
    pub documents: Arc<dyn ProjectDocumentStore>,
    pub environment_sources: Arc<dyn EnvironmentSourceDiscovery>,
    pub runtime: Arc<dyn RuntimeDriver>,
    pub state: Arc<dyn StateStore>,
    pub stacks: StackManager,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Application {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Application")
            .field("runtime_kind", &self.runtime.kind())
            .finish_non_exhaustive()
    }
}

impl Application {
    #[must_use]
    pub fn new(dependencies: ApplicationDependencies) -> Self {
        Self {
            documents: dependencies.documents,
            environment_sources: dependencies.environment_sources,
            runtime: dependencies.runtime,
            state: Some(dependencies.state),
            stacks: Some(dependencies.stacks),
            clock: dependencies.clock,
        }
    }

    #[must_use]
    pub fn read_only(
        documents: Arc<dyn ProjectDocumentStore>,
        environment_sources: Arc<dyn EnvironmentSourceDiscovery>,
        runtime: Arc<dyn RuntimeDriver>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            documents,
            environment_sources,
            runtime,
            state: None,
            stacks: None,
            clock,
        }
    }

    #[must_use]
    pub fn runtime_inspection(
        runtime: Arc<dyn RuntimeDriver>,
        clock: Arc<dyn Clock>,
        documents: Arc<dyn ProjectDocumentStore>,
        environment_sources: Arc<dyn EnvironmentSourceDiscovery>,
        stacks: StackManager,
    ) -> Self {
        Self {
            documents,
            environment_sources,
            runtime,
            state: None,
            stacks: Some(stacks),
            clock,
        }
    }

    pub fn initialize(
        &self,
        root: &Path,
        name: Option<&str>,
    ) -> Result<InitializedProject, MmError> {
        let state = self.state()?;
        let initialized = self.documents.initialize(root, name)?;
        self.register_initialized(state, initialized.as_ref())
    }

    pub fn initialize_reviewed(
        &self,
        root: &Path,
        name: Option<&str>,
        plan_hash: &str,
    ) -> Result<InitializedProject, MmError> {
        let state = self.state()?;
        let initialized = self.documents.initialize_reviewed(root, name, plan_hash)?;
        self.register_initialized(state, initialized.as_ref())
    }

    fn register_initialized(
        &self,
        state: &dyn crate::app::ports::StateStore,
        initialized: &dyn crate::app::ports::ProjectInitialization,
    ) -> Result<InitializedProject, MmError> {
        let document = initialized.document();
        let workspace = match state.register_workspace(
            &WorkspaceRegistration {
                project_id: document.config.project.id,
                root: document.root.clone(),
                config_path: document.path.clone(),
            },
            self.clock.now_unix_ms(),
        ) {
            Ok(workspace) => workspace,
            Err(error) => {
                if let Err(cleanup) = initialized.rollback() {
                    return Err(MmError::State {
                        message: format!(
                            "{error}; project initialization cleanup also failed: {cleanup}"
                        ),
                    });
                }
                return Err(error);
            }
        };

        Ok(InitializedProject {
            project_name: document.config.project.name.clone(),
            root: workspace.root,
            config_path: workspace.config_path,
            project_id: workspace.project_id,
            starter_files: initialized.starter_files().to_vec(),
            mount_proposals: crate::app::development_mount::discovery_proposals(document),
        })
    }

    pub fn schema(&self) -> schemars::Schema {
        self.documents.schema()
    }

    pub fn validate(&self, selection: &WorkspaceSelection) -> Result<ValidationSnapshot, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let mut validation = validate(&document, &environment);
        self.add_environment_diagnostics(&document, &environment, &mut validation);
        for path in self.documents.ignored_sources(&document) {
            validation.push(
                Diagnostic::warning(
                    "MM_CONFIG_SOURCE_IGNORED",
                    "git ignores this split configuration file, so it will not be committed and remote environments will not see it",
                )
                .at_path(&path)
                .with_help("remove the ignore rule, or move the key back with `mm config join KEY`"),
            );
        }
        let (target, target_source, runtime_kind, runtime_source) = if self.stacks.is_some() {
            match self.resolve_workspace_selection(&document, selection, &environment) {
                Ok(context) => {
                    self.validate_runtime_kind(&context.selection, &mut validation, &document.path);
                    (
                        context.target.name,
                        Some(context.selection.target_source.as_str().to_owned()),
                        Some(context.selection.runtime),
                        Some(context.selection.runtime_source.as_str().to_owned()),
                    )
                }
                Err(error) => {
                    validation.push(
                        Diagnostic::error("MM_SELECTION_UNRESOLVED", error.to_string())
                            .at_path(&document.path)
                            .with_help(
                                "pass --runtime/--target or configure a project, environment, or target default; mm will not guess",
                            ),
                    );
                    (String::new(), None, None, None)
                }
            }
        } else {
            (
                document.config.target.clone(),
                None,
                selection.runtime,
                None,
            )
        };

        let environment_delivery =
            Some(resolve_environment_delivery(&document.config, &environment));
        Ok(ValidationSnapshot {
            project_name: document.config.project.name,
            root: document.root,
            config_path: document.path,
            target,
            environment: environment.clone(),
            config_hash: document.source_hash,
            target_source,
            runtime_kind,
            runtime_source,
            environment_delivery,
            validation,
        })
    }

    pub async fn doctor(&self, selection: &WorkspaceSelection) -> Result<DoctorReport, MmError> {
        let document = match self.documents.load_from(&selection.start) {
            Ok(document) => Some(document),
            Err(MmError::ConfigNotFound { .. }) => None,
            Err(error) => return Err(error),
        };

        let Some(document) = document else {
            return Ok(self.host_doctor_report().await);
        };

        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let mut validation = validate(&document, &environment);
        self.add_environment_diagnostics(&document, &environment, &mut validation);
        let context = self.resolve_workspace_selection(&document, selection, &environment)?;
        self.validate_runtime_kind(&context.selection, &mut validation, &document.path);
        self.validate_target(
            &document,
            Some(context.target.name.as_str()),
            &mut validation,
        )?;
        let mut runtime = if target_blocks_runtime(&validation) {
            skipped_target_runtime(self.runtime.kind())
        } else {
            self.runtime.probe().await
        };

        let can_render = runtime
            .capabilities
            .contains(&RuntimeCapability::RenderCompose);
        if validation.is_valid() && runtime.available && can_render {
            if runtime
                .capabilities
                .contains(&RuntimeCapability::RenderCompose)
                && !require_capabilities(&mut runtime, &[RuntimeCapability::RenderCompose])
            {
                // keep diagnostics from require_capabilities
            }
            for target in
                self.stack_manager()?
                    .runtime_targets(&document, &environment, &context.target)?
            {
                if let Err(error) = self.runtime.render(&target).await {
                    runtime.diagnostics.push(
                        Diagnostic::error(
                            "MM_COMPOSE_RENDER_FAILED",
                            format!(
                                "stack '{}' could not be rendered: {error}",
                                target.stack_name
                            ),
                        )
                        .at_path(document.source("stacks"))
                        .with_help(
                            "run the provider's Compose config command to inspect the error",
                        ),
                    );
                }
            }
        }

        let ingress = if validation.is_valid() {
            self.stack_manager()?.compile(selection)?.ingress
        } else {
            Vec::new()
        };
        let environment_delivery =
            Some(resolve_environment_delivery(&document.config, &environment));
        Ok(DoctorReport {
            readiness: None,
            project_name: Some(document.config.project.name),
            root: Some(document.root),
            target: Some(context.target.name.clone()),
            environment: Some(environment.clone()),
            target_source: Some(context.selection.target_source.as_str().to_owned()),
            runtime_kind: Some(context.selection.runtime),
            runtime_source: Some(context.selection.runtime_source.as_str().to_owned()),
            environment_delivery,
            project_selected: true,
            runtime,
            validation,
            ingress,
            update: None,
        })
    }

    async fn host_doctor_report(&self) -> DoctorReport {
        let runtime = self.runtime.probe().await;
        let mut validation = ValidationReport::default();
        validation.push(
            Diagnostic::warning(
                "MM_CONFIG_NOT_FOUND",
                "no mm project was found; runtime checks were still completed",
            )
            .with_help("run `mm init` from a project root"),
        );
        validation.push(
            Diagnostic::info(
                "MM_SELECTION_HOST_ONLY",
                "no project selection occurred; host runtime inspection only",
            )
            .with_help("create or enter a project before using project operations"),
        );
        DoctorReport {
            readiness: None,
            project_name: None,
            root: None,
            target: None,
            environment: None,
            target_source: None,
            runtime_kind: Some(self.runtime.kind()),
            runtime_source: None,
            environment_delivery: None,
            project_selected: false,
            runtime,
            validation,
            ingress: Vec::new(),
            update: None,
        }
    }

    pub async fn status(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<WorkspaceSnapshot, MmError> {
        self.status_reading(selection, StatusRead::Rendered).await
    }

    /// The status, asking as much of the engine as `read` says.
    pub async fn status_reading(
        &self,
        selection: &WorkspaceSelection,
        read: StatusRead,
    ) -> Result<WorkspaceSnapshot, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let mut validation = validate(&document, &environment);
        if validation.is_valid() {
            match super::stacks::inspect_deployment_images(
                &document,
                &environment,
                self.state.as_deref(),
            ) {
                Ok(_) => {}
                Err(MmError::Setup {
                    issue: issue @ crate::domain::readiness::SetupIssue::MissingImages { .. },
                }) => {
                    validation.push(
                        Diagnostic::error(issue.diagnostic_code(), issue.to_string())
                            .with_help(issue.help()),
                    );
                }
                Err(error) => return Err(error),
            }
        }
        self.add_environment_diagnostics(&document, &environment, &mut validation);
        let context = self.resolve_workspace_selection(&document, selection, &environment)?;
        self.validate_runtime_kind(&context.selection, &mut validation, &document.path);
        self.validate_target(
            &document,
            Some(context.target.name.as_str()),
            &mut validation,
        )?;
        let ingress = if validation.is_valid() {
            match self.stack_manager()?.compile(selection) {
                Ok(compiled) => compiled.ingress,
                Err(MmError::Setup {
                    issue: issue @ crate::domain::readiness::SetupIssue::MissingImages { .. },
                }) => {
                    validation.push(
                        Diagnostic::error(issue.diagnostic_code(), issue.to_string())
                            .with_help(issue.help()),
                    );
                    Vec::new()
                }
                Err(error) => return Err(error),
            }
        } else {
            Vec::new()
        };
        let (runtime, stacks) = self
            .runtime_and_stacks(
                &document,
                &environment,
                &context.target,
                (&mut validation, read),
            )
            .await?;

        let drift = if read == StatusRead::Observed {
            stack_status::service_drift(&document, &environment, &stacks)
        } else {
            Vec::new()
        };
        let workspace = self.state()?.register_workspace(
            &WorkspaceRegistration {
                project_id: document.config.project.id,
                root: document.root.clone(),
                config_path: document.path.clone(),
            },
            self.clock.now_unix_ms(),
        )?;
        let recent_operations = self.state()?.recent_operations(workspace.id, 20)?;

        let optional = super::development_units::optional_in(&document.config, &environment);
        Ok(WorkspaceSnapshot {
            workspace,
            project_name: document.config.project.name,
            target: context.target.name.clone(),
            environment,
            config_hash: document.source_hash,
            target_source: Some(context.selection.target_source.as_str().to_owned()),
            runtime_kind: Some(context.selection.runtime),
            runtime_source: Some(context.selection.runtime_source.as_str().to_owned()),
            runtime,
            validation,
            stacks,
            ingress,
            recent_operations,
            nodes: Vec::new(),
            names: None,
            deployment: None,
            drift,
            network_drift: Vec::new(),
            optional,
            tunnel: None,
        })
    }

    pub(crate) fn runtime_operation_context(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<
        (
            ConfigDocument,
            String,
            Vec<crate::domain::runtime::RuntimeTarget>,
        ),
        MmError,
    > {
        let (document, context) = self.operation_context(selection)?;
        let environment = context.selection.environment.clone();
        let targets =
            self.stack_manager()?
                .runtime_targets(&document, &environment, &context.target)?;
        Ok((document, environment, targets))
    }

    /// Resolve authorization scope without requiring deployable artifacts.
    pub(crate) fn operation_context(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<(ConfigDocument, crate::app::ResolvedOperationContext), MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let context = self.resolve_workspace_selection(&document, selection, &environment)?;
        if context.selection.runtime != self.runtime.kind() {
            return Err(MmError::Security {
                message: "resolved runtime does not match the runtime operation adapter".to_owned(),
            });
        }
        Ok((document, context))
    }

    /// As `require_runtime_capabilities`, for an operation that only reads
    /// from the engine: it asks whether the engine answers and no more.
    pub(crate) async fn require_read_capabilities(
        &self,
        operation: &str,
        required: &[RuntimeCapability],
    ) -> Result<(), MmError> {
        let probe = self.runtime.status_probe().await;
        probe.require_capabilities(operation, required, None)
    }

    pub(crate) async fn require_runtime_capabilities(
        &self,
        operation: &str,
        required: &[RuntimeCapability],
    ) -> Result<(), MmError> {
        let probe = self.runtime.probe().await;
        probe.require_capabilities(operation, required, None)
    }

    pub(crate) fn runtime_kind(&self) -> crate::domain::runtime::RuntimeKind {
        self.runtime.kind()
    }

    pub(crate) fn environment_index(
        &self,
        document: &ConfigDocument,
        environment: &str,
    ) -> crate::domain::environment::EnvironmentIndex {
        self.environment_sources.index(document, environment)
    }

    /// Waits, at most `timeout`, until the containers of `services` in
    /// `target` run and pass their health checks, as a deployment does.
    pub(crate) async fn wait_for_services(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        services: &[String],
        timeout: std::time::Duration,
    ) -> Result<crate::domain::runtime::ConvergenceOutcome, MmError> {
        let policy = crate::domain::runtime::ConvergencePolicy {
            timeout,
            ..crate::domain::runtime::ConvergencePolicy::default()
        };
        self.runtime
            .wait_for_services(
                target,
                services,
                policy,
                crate::domain::runtime::ConvergenceCancellation::default(),
            )
            .await
    }

    fn resolve_workspace_selection(
        &self,
        document: &ConfigDocument,
        selection: &WorkspaceSelection,
        environment: &str,
    ) -> Result<crate::app::ResolvedOperationContext, MmError> {
        let (environment_pref, project_pref) = match &self.state {
            Some(state) => {
                crate::app::load_selection_preferences(Some(state.as_ref()), document, environment)?
            }
            None => (None, None),
        };
        crate::app::resolve_operation_context(
            document,
            environment,
            selection.runtime,
            selection.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            self.stack_manager()?.targets(),
        )
    }

    fn validate_runtime_kind(
        &self,
        resolved: &crate::app::ResolvedSelection,
        report: &mut ValidationReport,
        path: &std::path::Path,
    ) {
        if resolved.runtime != self.runtime.kind() {
            report.push(
                Diagnostic::error(
                    "MM_RUNTIME_DRIVER_UNAVAILABLE",
                    format!(
                        "resolved runtime {:?} from {} but this command is using {:?}",
                        resolved.runtime,
                        resolved.runtime_source.as_str(),
                        self.runtime.kind()
                    ),
                )
                .at_path(path),
            );
        }
    }

    fn validate_target(
        &self,
        document: &ConfigDocument,
        target_name: Option<&str>,
        report: &mut ValidationReport,
    ) -> Result<(), MmError> {
        if report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MM_TARGET_NAME_INVALID")
        {
            return Ok(());
        }
        let target_name = target_name.unwrap_or(document.config.target.as_str());
        match self.stack_manager()?.resolve_target(target_name) {
            Ok(target)
                if !matches!(
                    target.definition.transport,
                    crate::domain::target::TargetTransport::Local
                ) =>
            {
                report.push(
                    Diagnostic::error(
                        "MM_RUNTIME_TARGET_TRANSPORT_UNSUPPORTED",
                        format!(
                            "target '{}' uses SSH, but the Podman Compose runtime currently inspects only local targets",
                            target.name
                        ),
                    )
                    .at_path(&document.path)
                    .with_help("use `mm target bootstrap plan`; remote runtime inspection is not implemented"),
                );
            }
            Ok(_) => {}
            Err(error) => {
                report.push(
                    Diagnostic::error("MM_TARGET_UNAVAILABLE", error.to_string())
                        .at_path(&document.path)
                        .with_help("run `mm target list` and configure the target under MM_HOME"),
                );
            }
        }
        Ok(())
    }

    fn state(&self) -> Result<&dyn StateStore, MmError> {
        self.state.as_deref().ok_or_else(|| MmError::State {
            message: "this application instance has no persistent state store".to_owned(),
        })
    }

    fn stack_manager(&self) -> Result<&StackManager, MmError> {
        self.stacks.as_ref().ok_or_else(|| MmError::State {
            message: "this application instance has no stack artifact store".to_owned(),
        })
    }

    fn validate_service_mapping(
        &self,
        document: &ConfigDocument,
        environment: &str,
        stack_name: &str,
        rendered: &RenderedProject,
        report: &mut ValidationReport,
    ) {
        let rendered_services = rendered.service_names.iter().collect::<BTreeSet<_>>();
        for (module_name, module) in &document.config.modules {
            for (service, service_config) in &module.services {
                if service_config.stack != stack_name || !service_config.is_active(environment) {
                    continue;
                }
                if !rendered_services.contains(service) {
                    report.push(
                        Diagnostic::error(
                            "MM_SERVICE_NOT_IN_COMPOSE",
                            format!(
                                "module '{module_name}' references service '{service}' in stack '{stack_name}', but it is absent from rendered Compose configuration"
                            ),
                        )
                        .at_path(document.source("modules")),
                    );
                }
            }
        }
    }

    fn add_environment_diagnostics(
        &self,
        document: &ConfigDocument,
        environment: &str,
        validation: &mut ValidationReport,
    ) {
        if document.config.environments.contains_key(environment) {
            validation.diagnostics.extend(
                self.environment_sources
                    .index(document, environment)
                    .diagnostics,
            );
        }
    }
}
