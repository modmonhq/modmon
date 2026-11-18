use super::*;

mod control;
mod environments;
mod guide;
mod launch;
use launch::{Parts, open, placeholder_snapshot, runs_remotely, setup_steps};
mod operations;
mod reads;
use reads::Reads;

pub(super) async fn run(invocation: &super::dispatch::Invocation) -> Result<u8, MmError> {
    if invocation.format == OutputFormat::Json {
        return super::doctor::run(false, invocation).await;
    }
    require_interactive_terminal()?;
    if !initialize_missing_project(invocation)? || !recover_legacy_state(invocation)? {
        return Ok(0);
    }
    // This machine is a place environments can run. Recording that is private,
    // changes no existing target, and is what `mm init` and `mm dev setup` do;
    // asking a new developer to do it by hand would be a step with no choice in it.
    if let Err(error) = composition::ensure_local_target(invocation.home()) {
        tracing::debug!(%error, "could not register this machine as the local target");
    }
    let document = composition::project_documents().load_from(&invocation.start)?;
    let directory = environments::EnvironmentDirectory {
        base: invocation.clone(),
        launched: crate::domain::config::resolve_environment(
            &document,
            invocation.selection.environment.as_deref(),
        )?,
    };
    // The first frame is drawn from the configuration alone. The session is
    // read once it is up, whatever state the environment is in: what is
    // missing is shown there, next to the way to every other environment.
    let launch = launch::at_once(invocation, &directory)?;
    // Leaving the terminal UI is not a failure, whatever it was showing:
    // `mm doctor` and `mm service status` are the commands with a verdict.
    Ok(match tui::run(launch).await? {
        tui::TuiExit::Interrupted => 130,
        tui::TuiExit::Quit => 0,
    })
}

/// mm's own state database is the one thing every environment needs. Offers
/// the reviewed recovery for a pre-baseline database; `false` when declined.
fn recover_legacy_state(invocation: &super::dispatch::Invocation) -> Result<bool, MmError> {
    match composition::inspect_local_state(invocation.home()) {
        Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::LegacyState { .. },
        }) => super::state::recover_interactively(invocation),
        Err(error) => Err(error),
        Ok(()) => Ok(true),
    }
}

fn require_interactive_terminal() -> Result<(), MmError> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(MmError::Terminal {
            message: "the Ratatui interface requires an interactive terminal; use `mm doctor` for a readiness report or `mm service status` for service status"
                .to_owned(),
        });
    }
    Ok(())
}

fn initialize_missing_project(invocation: &super::dispatch::Invocation) -> Result<bool, MmError> {
    let setup = match composition::discover_project(&invocation.start) {
        Ok(_) => return Ok(true),
        Err(MmError::ConfigNotFound { .. }) => prompt_interactive_setup(&invocation.start)?,
        Err(error) => return Err(error),
    };
    let SetupDecision::Initialize { name } = setup else {
        return Ok(false);
    };
    match composition::inspect_local_state(invocation.home()) {
        Err(MmError::Setup {
            issue: crate::domain::readiness::SetupIssue::LegacyState { .. },
        }) => {
            if !super::state::recover_interactively(invocation)? {
                return Ok(false);
            }
        }
        Err(error) => return Err(error),
        Ok(()) => {}
    }
    let onboarding = composition::onboarding_service();
    let plan = onboarding.plan_initialization(&invocation.start, name.as_deref())?;
    output::write_initialization_plan(&plan, invocation.format)?;
    if !confirm(&format!(
        "Apply project initialization plan {}?",
        output::terminal_safe(&plan.plan_hash)
    ))? {
        return Ok(false);
    }
    composition::ensure_local_target(invocation.home())?;
    let application = composition::persistent_application(
        invocation.home(),
        Arc::clone(&invocation.runtime),
        Arc::clone(&invocation.clock),
    )?;
    let initialized =
        application.initialize_reviewed(&invocation.start, name.as_deref(), &plan.plan_hash)?;
    composition::trust_agents(&invocation.start, invocation.home(), None)?;
    output::write_initialized(&initialized, invocation.format)?;
    Ok(true)
}

/// One status read of an environment, as `mm service status` reads it.
async fn workspace_snapshot(
    invocation: &super::dispatch::Invocation,
) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
    composition::service_gateway(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
        &invocation.services,
    )?
    .status(&approval("tui", false))
    .await
}

/// Who you are where, as far as this machine knows: this device's key, the
/// team in the users file, saved forwards, and whether the environment is
/// remote. Each part is best effort so the TUI still opens offline.
fn access_view(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    invocation: &super::dispatch::Invocation,
) -> tui::AccessView {
    let (device_key, team) = team_view(document, environment, invocation);
    let project = document.config.project.id.0.to_string();
    let forwards = match composition::forward_shortcuts(invocation.home())
        .and_then(|store| store.list(&project))
    {
        Ok(shortcuts) => shortcuts
            .into_iter()
            .map(|(name, shortcut)| {
                format!(
                    "{name}  {}:{} -> {}/{}:{}{}",
                    shortcut.bind,
                    shortcut.local_port,
                    shortcut.environment,
                    shortcut.service,
                    shortcut.port,
                    if shortcut.auto { "  auto" } else { "" }
                )
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    let mut view = tui::AccessView {
        device_key,
        team,
        forwards,
        ..tui::AccessView::default()
    };
    match composition::remote_transport_context(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    ) {
        Ok(Some((_, scope))) => view.remote_target = Some(scope.target),
        Ok(None) => {}
        Err(error) => view.identity_error = Some(output::error_line(&error)),
    }
    view
}

/// Asks the host of a remote environment who you are there.
async fn identify(invocation: &super::dispatch::Invocation, view: &mut tui::AccessView) {
    let Ok(Some((transport, scope))) = composition::remote_transport_context(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    ) else {
        return;
    };
    match tokio::time::timeout(std::time::Duration::from_secs(10), transport.whoami(scope)).await {
        Ok(Ok(identity)) => view.identity = Some(identity),
        Ok(Err(error)) => view.identity_error = Some(output::error_line(&error)),
        Err(_) => view.identity_error = Some("the host did not answer in time".to_owned()),
    }
}

/// This device's key fingerprint and the team in the users file, with each
/// member's access in `environment`.
fn team_view(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    invocation: &super::dispatch::Invocation,
) -> (Option<String>, Vec<tui::TeamMemberView>) {
    let device_key = match composition::device_key(invocation.home()) {
        Ok(key) => key.map(|key| key.fingerprint),
        Err(_) => None,
    };
    let team = document
        .config
        .users
        .iter()
        .map(|(name, user)| tui::TeamMemberView {
            name: name.clone(),
            access: user.access_in(environment).map(|access| access.summary()),
            keys: user.keys.len(),
            machine: user.machine,
            you: device_key.as_ref().is_some_and(|fingerprint| {
                user.keys.iter().any(|key| {
                    crate::domain::host_policy::parse_public_key(key.line())
                        .is_ok_and(|parsed| &parsed.fingerprint == fingerprint)
                })
            }),
        })
        .collect();
    (device_key, team)
}

/// Re-reads the status shown in the TUI; failures of optional parts keep their last value.
struct LiveStatus {
    invocation: super::dispatch::Invocation,
    reads: Arc<Reads>,
    /// Setup had something open at launch, so it is read again each time.
    watch_setup: bool,
    /// The session opened without a readable environment.
    limited: bool,
}

impl LiveStatus {
    /// The setup state now, with `unavailable` as the reason nothing was read.
    async fn setup(
        &self,
        unavailable: Option<&MmError>,
    ) -> Result<(tui::SetupView, crate::domain::workspace::DoctorReport), MmError> {
        let invocation = &self.invocation;
        let report = super::setup::inspect(invocation).await?;
        let steps = setup_steps(&report, invocation, runs_remotely(invocation));
        Ok((
            tui::SetupView {
                steps,
                unavailable: unavailable.map(ToString::to_string),
            },
            report,
        ))
    }
}

#[async_trait::async_trait]
impl tui::LiveStatusSource for LiveStatus {
    async fn refresh(
        &self,
        so_far: crate::app::StatusSoFar<'_>,
    ) -> Result<tui::LiveStatus, MmError> {
        let invocation = &self.invocation;
        let snapshot = match self.reads.status(Some(so_far)).await {
            Ok(snapshot) => snapshot,
            // Still not readable: keep showing why, in case the reason changed.
            Err(error) if self.limited => {
                let (setup, report) = self.setup(Some(&error)).await?;
                return Ok(tui::LiveStatus {
                    snapshot: placeholder_snapshot(invocation, &report, &error)?,
                    environment: None,
                    tls: tls_status(invocation).await.ok(),
                    installation_services: None,
                    setup: Some(setup),
                });
            }
            Err(error) => return Err(error),
        };
        let environment = match composition::environment_services(
            &invocation.start,
            invocation.home(),
            Arc::clone(&invocation.clock),
        ) {
            Ok(services) => services
                .manager
                .status(&invocation.selection, false)
                .await
                .ok()
                .filter(|environment| environment.config_hash == snapshot.config_hash),
            Err(_) => None,
        };
        let setup = if self.watch_setup {
            self.setup(None).await.ok().map(|(setup, _)| setup)
        } else {
            None
        };
        // Local HTTPS and the shared development services are this
        // machine's: a remote environment's refresh does not ask for them.
        let (tls, installation_services) = if self.reads.remote() {
            (None, None)
        } else {
            (
                tls_status(invocation).await.ok(),
                service_status(invocation).await.ok(),
            )
        };
        Ok(tui::LiveStatus {
            snapshot,
            environment,
            tls,
            installation_services,
            setup,
        })
    }
}

/// The mount plan of every module that has one, in configuration order.
/// Each plan asks the engine what it can do, so a few are made at a time.
async fn mount_plans(
    document: &crate::domain::config::ConfigDocument,
    manager: &crate::app::DevelopmentMountManager,
    invocation: &super::dispatch::Invocation,
) -> Vec<crate::domain::development_mount::DevelopmentMountPlan> {
    let modules = document.config.modules.keys().collect::<Vec<_>>();
    let mut plans = Vec::new();
    for few in modules.chunks(4) {
        let planned = futures_util::future::join_all(few.iter().map(|module| async move {
            manager
                .plan(
                    &invocation.selection,
                    &crate::app::DevelopmentMountPlanRequest {
                        module: (*module).clone(),
                        ..Default::default()
                    },
                )
                .await
        }))
        .await;
        plans.extend(planned.into_iter().flatten());
    }
    plans
}

fn backend_state(
    document: &crate::domain::config::ConfigDocument,
    snapshot: &crate::domain::workspace::WorkspaceSnapshot,
    invocation: &super::dispatch::Invocation,
) -> Result<
    (
        crate::app::SecretBackendManager,
        tui::SecretBackendTuiContext,
    ),
    MmError,
> {
    let manager = composition::interactive_secret_backend_manager(
        invocation.home(),
        &invocation.services,
        invocation.runtime_kind,
        document.config.providers.clone(),
    )?;
    let context = tui::SecretBackendTuiContext::from_config(
        document.root.clone(),
        document.source_hash.clone(),
        &document.config,
        snapshot.environment.clone(),
        manager.drivers(),
        manager.instances()?,
    );
    Ok((manager, context))
}

type SchedulerState = (
    crate::app::JobSchedulerService,
    crate::domain::scheduler::DesiredDefinitions,
    Vec<crate::domain::scheduler::ScheduleRecord>,
    Vec<crate::domain::scheduler::JobRun>,
);

fn scheduler_state(
    invocation: &super::dispatch::Invocation,
    parts: &mut Parts,
) -> Result<SchedulerState, MmError> {
    let scheduler = composition::job_scheduler_service(
        &invocation.start,
        invocation.home(),
        invocation.runtime_kind,
        &invocation.services,
    )?;
    let approval = approval("tui", false);
    let jobs = parts.read_or(
        "Jobs",
        scheduler.list_jobs(&invocation.selection, &approval),
        |_| crate::domain::scheduler::DesiredDefinitions {
            jobs: std::collections::BTreeMap::new(),
            schedules: std::collections::BTreeMap::new(),
        },
    );
    let schedules = parts.read(
        "Schedules",
        scheduler.list_schedules(&invocation.selection, &approval),
    );
    let runs = parts.read(
        "Job runs",
        scheduler.history(&invocation.selection, None, None, 100, &approval),
    );
    Ok((scheduler, jobs, schedules, runs))
}

async fn tls_status(
    invocation: &super::dispatch::Invocation,
) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
    composition::local_ca_manager(invocation.home(), false, invocation.services.process())?
        .status()
        .await
}

async fn service_status(
    invocation: &super::dispatch::Invocation,
) -> Result<crate::domain::service_stack::ServiceStackStatus, MmError> {
    composition::service_stack_manager(
        invocation.home(),
        invocation.runtime_kind,
        &invocation.services,
    )?
    .status()
    .await
}

/// The shared-services status when it could not be read.
fn unknown_service_status(error: &MmError) -> crate::domain::service_stack::ServiceStackStatus {
    crate::domain::service_stack::ServiceStackStatus {
        installed: false,
        stack_version: None,
        runtime: None,
        target_id: None,
        route: String::new(),
        internal_smtp: String::new(),
        host_smtp_port: None,
        components: Vec::new(),
        diagnostics: vec![crate::domain::diagnostic::Diagnostic::warning(
            error.code(),
            error.to_string(),
        )],
    }
}
