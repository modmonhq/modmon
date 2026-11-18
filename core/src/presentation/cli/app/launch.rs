//! Building the session the terminal UI shows.
//!
//! The first frame comes from `at_once`: what the configuration and this
//! machine's own files say, with nothing asked of an engine or a host. The
//! complete session comes from `open`, which runs after the first frame is
//! up, reads its parts side by side, and reports each one as soon as it has
//! it, so a slow host holds back only its own part.

use super::*;

/// The session for the first frame. It asks no engine and no host: every
/// part that needs one is empty and marked as still being read.
pub(super) fn at_once(
    invocation: &super::super::dispatch::Invocation,
    directory: &environments::EnvironmentDirectory,
) -> Result<tui::LaunchRequest, MmError> {
    let _phase = crate::app::timing::phase("tui.first-session");
    let mut invocation = invocation.clone();
    // The runtime the configuration or a saved preference names. When none
    // does, the readiness report finds one; nothing can act before then.
    if let Ok(runtime) = composition::resolve_runtime(
        &invocation.start,
        invocation.home(),
        invocation.selection.runtime,
        invocation.selection.target.as_deref(),
        invocation.selection.environment.as_deref(),
        false,
        true,
    ) {
        invocation.runtime_kind = runtime;
        invocation.runtime = invocation.services.driver(runtime);
    }
    let invocation = &invocation;
    let document = composition::project_documents().load_from(&invocation.start)?;
    let snapshot = unread_snapshot(&document, invocation)?;
    let reads = Reads::new(invocation, &document);
    let mut request = session(snapshot, invocation, directory, (&document, &reads), true)?.0;
    request.loading = true;
    Ok(request)
}

/// Builds the TUI session for the invocation's environment, reporting each
/// part to `progress` as it is read.
///
/// When the environment cannot be read (this machine is not registered, no
/// runtime is chosen, a host does not answer) and `fallback` is set, the
/// session still opens: its views are empty and the Overview says why. An
/// environment on this machine always falls back, so its setup can be done
/// from inside the TUI.
pub(super) async fn open(
    invocation: &super::super::dispatch::Invocation,
    directory: &environments::EnvironmentDirectory,
    fallback: bool,
    progress: &tui::LoadProgress,
) -> Result<tui::LaunchRequest, MmError> {
    let (invocation, report) = prepared(invocation).await?;
    let invocation = &invocation;
    let remote = report.is_none();
    let steps = report
        .as_ref()
        .map(|report| setup_steps(report, invocation, false))
        .unwrap_or_default();
    progress.loaded(tui::Loaded::Setup(tui::SetupView {
        steps: steps.clone(),
        unavailable: None,
    }));
    let document = composition::project_documents().load_from(&invocation.start)?;
    let reads = Reads::new(invocation, &document);
    let read = read_parts(invocation, (&document, reads.as_ref()), progress).await;
    let (snapshot, unavailable) = match read.snapshot {
        Ok(snapshot) => (snapshot, None),
        Err(error) if fallback || !remote => {
            tracing::debug!(%error, "opening the TUI without a readable environment");
            let inspected;
            let report = match &report {
                Some(report) => report,
                None => {
                    inspected = super::super::setup::inspect(invocation).await?;
                    &inspected
                }
            };
            (
                placeholder_snapshot(invocation, report, &error)?,
                Some(error.to_string()),
            )
        }
        Err(error) => return Err(error),
    };
    let limited = unavailable.is_some();
    // Findings keep the order the parts have on screen: variables first.
    let mut first = Parts {
        limited,
        warnings: Vec::new(),
    };
    // A host runs its deployed commit, which may differ from this checkout:
    // then the variables resolved here would not describe the host. Locally a
    // different hash means the file changed just now; the next refresh has it.
    let environment = first
        .read("Variables", read.variables.map(Some))
        .filter(|environment| environment.config_hash == snapshot.config_hash);
    let (mut request, mut parts) = session(
        snapshot,
        invocation,
        directory,
        (&document, &reads),
        limited,
    )?;
    request.environment = environment;
    request.mounts = read.mounts;
    request.tls = parts.read("Local HTTPS", read.tls.map(Some));
    request.installation_services = parts.read_or(
        "Shared services",
        read.shared_services,
        unknown_service_status,
    );
    request.access.identity = read.identity.identity;
    // The host that just failed to answer has nothing to add about you.
    if !limited && request.access.identity_error.is_none() {
        request.access.identity_error = read.identity.identity_error;
    }
    first.warnings.append(&mut parts.warnings);
    request.warnings = first.warnings;
    request.live = Arc::new(LiveStatus {
        invocation: invocation.clone(),
        reads,
        watch_setup: !steps.is_empty() || limited,
        limited,
    });
    request.setup = tui::SetupView { steps, unavailable };
    Ok(request)
}

/// The invocation with its runtime resolved, and, for an environment on
/// this machine, the readiness report its setup steps come from. A remote
/// environment's readiness is its hosts': this machine's engines, local
/// HTTPS, and shared services are not inspected before its status.
async fn prepared(
    invocation: &super::super::dispatch::Invocation,
) -> Result<
    (
        super::super::dispatch::Invocation,
        Option<crate::domain::workspace::DoctorReport>,
    ),
    MmError,
> {
    let mut invocation = invocation.clone();
    if runs_remotely(&invocation) {
        if let Ok(runtime) = composition::resolve_runtime(
            &invocation.start,
            invocation.home(),
            invocation.selection.runtime,
            invocation.selection.target.as_deref(),
            invocation.selection.environment.as_deref(),
            false,
            true,
        ) {
            invocation.runtime_kind = runtime;
            invocation.runtime = invocation.services.driver(runtime);
        }
        return Ok((invocation, None));
    }
    let phase = crate::app::timing::phase("tui.readiness");
    let report = super::super::setup::inspect(&invocation).await?;
    drop(phase);
    if let Some(runtime) = report.runtime_kind {
        invocation.runtime_kind = runtime;
        invocation.runtime = invocation.services.driver(runtime);
    }
    Ok((invocation, Some(report)))
}

/// What is read from engines and hosts for a session, each part by itself.
struct Read {
    snapshot: Result<crate::domain::workspace::WorkspaceSnapshot, MmError>,
    variables: Result<EnvironmentStatusSnapshot, MmError>,
    mounts: Vec<crate::domain::development_mount::DevelopmentMountPlan>,
    tls: Result<crate::domain::ca::LocalCaStatus, MmError>,
    shared_services: Result<crate::domain::service_stack::ServiceStackStatus, MmError>,
    /// Only what the host said: who you are, or why it did not say.
    identity: tui::AccessView,
}

/// Reads every part at the same time and tells `progress` about each as it
/// arrives. No part waits for another, so the status of a remote
/// environment and who you are there take one wait, not two.
async fn read_parts(
    invocation: &super::super::dispatch::Invocation,
    (document, reads): (&crate::domain::config::ConfigDocument, &Reads),
    progress: &tui::LoadProgress,
) -> Read {
    // The variables describe a host only when it runs this checkout's
    // configuration, so they wait for the status to say which it runs.
    let (runs, running) = tokio::sync::oneshot::channel::<Option<String>>();
    let status = read_status(reads, progress, runs);
    let remote = reads.remote();
    let variables = read_variables(invocation, progress, running);
    let mounts = async {
        let _phase = crate::app::timing::phase("tui.mounts");
        let mounts = match composition::development_mount_manager(
            invocation.home(),
            Arc::clone(&invocation.runtime),
        ) {
            Ok(manager) => mount_plans(document, &manager, invocation).await,
            Err(_) => Vec::new(),
        };
        if progress.is_shown() {
            progress.loaded(tui::Loaded::Mounts(mounts.clone()));
        }
        mounts
    };
    let tls = async {
        let _phase = crate::app::timing::phase("tui.local-https");
        let tls = tls_status(invocation).await;
        progress.loaded(tui::Loaded::Https(tls.as_ref().ok().cloned()));
        tls
    };
    let shared_services = async {
        let _phase = crate::app::timing::phase("tui.shared-services");
        // The shared development services run on this machine; a remote
        // environment does not use them and they are not read for it.
        let services = if remote {
            Ok(unread_service_status())
        } else {
            service_status(invocation).await
        };
        if let Ok(services) = &services
            && progress.is_shown()
        {
            progress.loaded(tui::Loaded::SharedServices(Box::new(services.clone())));
        }
        services
    };
    let identity = async {
        let _phase = crate::app::timing::phase("tui.identity");
        let mut view = tui::AccessView::default();
        identify(invocation, &mut view).await;
        if progress.is_shown() {
            progress.loaded(tui::Loaded::Identity {
                identity: view.identity.clone(),
                error: view.identity_error.clone(),
            });
        }
        view
    };
    let (snapshot, variables, mounts, tls, shared_services, identity) =
        tokio::join!(status, variables, mounts, tls, shared_services, identity);
    Read {
        snapshot,
        variables,
        mounts,
        tls,
        shared_services,
        identity,
    }
}

/// The status, reported node by node on a cluster, and then which
/// configuration the environment runs, for `read_variables`.
async fn read_status(
    reads: &Reads,
    progress: &tui::LoadProgress,
    runs: tokio::sync::oneshot::Sender<Option<String>>,
) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
    let _phase = crate::app::timing::phase("tui.status");
    let so_far = |snapshot| {
        progress.loaded(tui::Loaded::Status {
            snapshot: Box::new(snapshot),
            complete: false,
        });
    };
    let snapshot = reads.status(Some(&so_far)).await;
    if let Ok(snapshot) = &snapshot
        && progress.is_shown()
    {
        progress.loaded(tui::Loaded::Status {
            snapshot: Box::new(snapshot.clone()),
            complete: true,
        });
    }
    let hash = snapshot
        .as_ref()
        .ok()
        .map(|snapshot| snapshot.config_hash.clone());
    if runs.send(hash).is_err() {
        tracing::debug!("nothing waited for the configuration the environment runs");
    }
    snapshot
}

/// The variables as this checkout resolves them. They are shown once
/// `running` says the environment runs this checkout's configuration.
async fn read_variables(
    invocation: &super::super::dispatch::Invocation,
    progress: &tui::LoadProgress,
    running: tokio::sync::oneshot::Receiver<Option<String>>,
) -> Result<EnvironmentStatusSnapshot, MmError> {
    let phase = crate::app::timing::phase("tui.variables");
    let variables = match composition::environment_services(
        &invocation.start,
        invocation.home(),
        Arc::clone(&invocation.clock),
    ) {
        Ok(services) => services.manager.status(&invocation.selection, false).await,
        Err(error) => Err(error),
    };
    drop(phase);
    if progress.is_shown() {
        let hash = running.await.ok().flatten();
        let shown = variables
            .as_ref()
            .ok()
            .filter(|variables| Some(&variables.config_hash) == hash.as_ref())
            .map(|variables| Box::new(variables.clone()));
        progress.loaded(tui::Loaded::Variables(shown));
    }
    variables
}

/// A session around `snapshot`: every manager the TUI acts through, and
/// what this machine's own files say (the team, saved forwards, gates,
/// jobs, secret providers). The parts read from engines and hosts are left
/// empty for the caller to fill. With `limited`, the snapshot is a
/// placeholder and a part that cannot be read is not reported.
fn session(
    snapshot: crate::domain::workspace::WorkspaceSnapshot,
    invocation: &super::super::dispatch::Invocation,
    directory: &environments::EnvironmentDirectory,
    (document, reads): (&crate::domain::config::ConfigDocument, &Arc<Reads>),
    limited: bool,
) -> Result<(tui::LaunchRequest, Parts), MmError> {
    let environment_services = composition::environment_services(
        &invocation.start,
        invocation.home(),
        Arc::clone(&invocation.clock),
    )?;
    let mut parts = Parts {
        limited,
        warnings: Vec::new(),
    };
    let mount_manager =
        composition::development_mount_manager(invocation.home(), Arc::clone(&invocation.runtime))?;
    let (backend_manager, backends) = backend_state(document, &snapshot, invocation)?;
    let gate_services = composition::gate_services(
        &invocation.start,
        invocation.home(),
        Arc::clone(&invocation.clock),
    )?;
    let gate_policies = parts.read(
        "Gates",
        gate_services
            .manager
            .show(document.config.project.id, &snapshot.environment, None),
    );
    let (scheduler, jobs, schedules, job_runs) = scheduler_state(invocation, &mut parts)?;
    let access = access_view(document, &snapshot.environment, invocation);
    let request = tui::LaunchRequest {
        access,
        snapshot,
        environment: None,
        tls: None,
        installation_services: unread_service_status(),
        mounts: Vec::new(),
        mount_manager,
        development_units: composition::development_units(),
        selection: invocation.selection.clone(),
        onboarding: composition::onboarding_service(),
        backends,
        backend_manager,
        secret_manager: environment_services.secrets,
        gate_manager: gate_services.manager,
        gate_policies,
        scheduler,
        jobs,
        schedules,
        job_runs,
        live: Arc::new(LiveStatus {
            invocation: invocation.clone(),
            reads: Arc::clone(reads),
            watch_setup: false,
            limited,
        }),
        setup: tui::SetupView::default(),
        warnings: Vec::new(),
        guide: Arc::new(guide::SetupGuide {
            invocation: invocation.clone(),
        }),
        control: Arc::new(control::WorkspaceControl {
            invocation: invocation.clone(),
        }),
        operations: Arc::new(operations::ServiceOperations {
            reads: Arc::clone(reads),
        }),
        environments: environments::entries(invocation),
        directory: Arc::new(environments::EnvironmentDirectory {
            base: directory.base.clone(),
            launched: directory.launched.clone(),
        }),
        loading: false,
    };
    Ok((request, parts))
}

/// Whether the environment runs on another host, whose readiness is its own.
pub(super) fn runs_remotely(invocation: &super::super::dispatch::Invocation) -> bool {
    composition::remote_transport_context(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    )
    .is_ok_and(|context| context.is_some())
}

/// The setup steps to show: the shared wording, plus the command for each.
pub(super) fn setup_steps(
    report: &crate::domain::workspace::DoctorReport,
    invocation: &super::super::dispatch::Invocation,
    remote: bool,
) -> Vec<tui::SetupStep> {
    if remote {
        return Vec::new();
    }
    report
        .setup_steps()
        .into_iter()
        .map(|step| tui::SetupStep {
            title: step.title,
            detail: step.detail,
            command: step
                .issue
                .as_ref()
                .filter(|issue| {
                    !matches!(issue, crate::domain::readiness::SetupIssue::MissingRuntime)
                })
                .map(|issue| invocation.recovery_context.display_command(issue)),
            code: step
                .issue
                .as_ref()
                .map(crate::domain::readiness::SetupIssue::diagnostic_code),
            ready: step.ready,
            issue: step
                .issue
                .filter(crate::domain::readiness::SetupIssue::is_guided),
        })
        .collect()
}

/// What the configuration alone says about the environment.
fn configured_snapshot(
    document: &crate::domain::config::ConfigDocument,
    environment: String,
    runtime: crate::domain::runtime::RuntimeProbe,
) -> crate::domain::workspace::WorkspaceSnapshot {
    use crate::domain::workspace::{WorkspaceId, WorkspaceRecord, WorkspaceSnapshot};

    WorkspaceSnapshot {
        workspace: WorkspaceRecord {
            // Nothing was registered: the state may be what is missing.
            id: WorkspaceId(uuid::Uuid::nil()),
            project_id: document.config.project.id,
            root: document.root.clone(),
            config_path: document.path.clone(),
        },
        project_name: document.config.project.name.clone(),
        target: document.config.target.clone(),
        environment,
        config_hash: document.source_hash.clone(),
        target_source: None,
        runtime_kind: None,
        runtime_source: None,
        runtime,
        validation: crate::domain::diagnostic::ValidationReport::default(),
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

/// The snapshot of the first frame: nothing was asked yet. On a cluster it
/// lists every node as not answered yet.
fn unread_snapshot(
    document: &crate::domain::config::ConfigDocument,
    invocation: &super::super::dispatch::Invocation,
) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
    let environment = crate::domain::config::resolve_environment(
        document,
        invocation.selection.environment.as_deref(),
    )?;
    let mut snapshot = configured_snapshot(
        document,
        environment,
        crate::domain::runtime::RuntimeProbe {
            kind: invocation.runtime_kind,
            available: false,
            engine_version: None,
            compose_provider: None,
            rootless: None,
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: Vec::new(),
        },
    );
    snapshot.runtime_kind = Some(invocation.runtime_kind);
    if let Ok(Some(remote)) = composition::remote_environment(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    ) {
        snapshot.target = remote.target().to_owned();
        if remote.is_cluster() {
            snapshot.nodes = remote
                .nodes()
                .iter()
                .filter_map(|node| node.name.clone())
                .map(crate::domain::workspace::NodeStatus::pending)
                .collect();
        }
    }
    Ok(snapshot)
}

/// A snapshot for an environment that could not be read: what the
/// configuration and the readiness report say, and nothing from a runtime.
pub(super) fn placeholder_snapshot(
    invocation: &super::super::dispatch::Invocation,
    report: &crate::domain::workspace::DoctorReport,
    reason: &MmError,
) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
    let document = composition::project_documents().load_from(&invocation.start)?;
    let environment = crate::domain::config::resolve_environment(
        &document,
        invocation.selection.environment.as_deref(),
    )?;
    let mut validation = report.validation.clone();
    let message = reason.to_string();
    if !validation
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == message)
    {
        validation.push(crate::domain::diagnostic::Diagnostic::error(
            reason.code(),
            message,
        ));
    }
    let mut snapshot = configured_snapshot(&document, environment, report.runtime.clone());
    if let Some(target) = &report.target {
        snapshot.target.clone_from(target);
    }
    snapshot.target_source.clone_from(&report.target_source);
    snapshot.runtime_kind = report.runtime_kind;
    snapshot.runtime_source.clone_from(&report.runtime_source);
    snapshot.validation = validation;
    Ok(snapshot)
}

/// Reads the parts of a session beside its snapshot. One that cannot be read
/// is left empty and reported as a finding, so a problem in one view never
/// keeps the others from opening.
pub(super) struct Parts {
    /// The snapshot itself could not be read; its reason is already shown.
    limited: bool,
    warnings: Vec<crate::domain::diagnostic::Diagnostic>,
}

impl Parts {
    pub(super) fn read<T: Default>(&mut self, part: &str, result: Result<T, MmError>) -> T {
        self.read_or(part, result, |_| T::default())
    }

    pub(super) fn read_or<T>(
        &mut self,
        part: &str,
        result: Result<T, MmError>,
        fallback: impl FnOnce(&MmError) -> T,
    ) -> T {
        result.unwrap_or_else(|error| {
            tracing::debug!(%error, part, "left a part of the TUI empty");
            // Unfinished setup explains itself as steps on the Overview.
            if !self.limited && !matches!(error, MmError::Setup { .. }) {
                self.warnings
                    .push(crate::domain::diagnostic::Diagnostic::warning(
                        error.code(),
                        format!("{part} could not be read: {error}"),
                    ));
            }
            fallback(&error)
        })
    }
}

/// The shared-services status before it is read.
fn unread_service_status() -> crate::domain::service_stack::ServiceStackStatus {
    crate::domain::service_stack::ServiceStackStatus {
        installed: false,
        stack_version: None,
        runtime: None,
        target_id: None,
        route: String::new(),
        internal_smtp: String::new(),
        host_smtp_port: None,
        components: Vec::new(),
        diagnostics: Vec::new(),
    }
}
