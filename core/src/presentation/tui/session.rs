use super::*;

pub(super) struct TerminalSession {
    pub(super) terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    pub(super) fn new() -> Result<Self, MmError> {
        enable_raw_mode().map_err(terminal_error)?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let cleanup = terminal_cleanup_failures([
                disable_raw_mode(),
                execute!(stdout, LeaveAlternateScreen, Show),
            ]);
            return Err(with_terminal_cleanup(error, cleanup));
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut cleanup_stdout = io::stdout();
                let cleanup = terminal_cleanup_failures([
                    disable_raw_mode(),
                    execute!(cleanup_stdout, LeaveAlternateScreen, Show),
                ]);
                return Err(with_terminal_cleanup(error, cleanup));
            }
        };
        Ok(Self { terminal })
    }

    /// Hands the terminal back to line-based output, keeping the session.
    fn suspend(&mut self) -> Result<(), MmError> {
        disable_raw_mode().map_err(terminal_error)?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen, Show).map_err(terminal_error)
    }

    /// Takes the terminal back after `suspend` and redraws from nothing.
    fn resume(&mut self) -> Result<(), MmError> {
        enable_raw_mode().map_err(terminal_error)?;
        execute!(self.terminal.backend_mut(), EnterAlternateScreen, Hide)
            .map_err(terminal_error)?;
        self.terminal.clear().map_err(terminal_error)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        drop(terminal_cleanup_failures([
            disable_raw_mode(),
            execute!(self.terminal.backend_mut(), LeaveAlternateScreen, Show),
            self.terminal.show_cursor(),
        ]));
    }
}

fn terminal_cleanup_failures<const N: usize>(results: [io::Result<()>; N]) -> Vec<String> {
    let failures = results
        .into_iter()
        .filter_map(Result::err)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    for error in &failures {
        tracing::warn!(%error, "terminal restoration failed");
    }
    failures
}

fn with_terminal_cleanup(primary: io::Error, cleanup: Vec<String>) -> MmError {
    let primary = terminal_error(primary);
    if cleanup.is_empty() {
        primary
    } else {
        MmError::operation_failure(
            "terminal-session",
            crate::domain::error::MutationStatus::NotStarted,
            primary,
            cleanup,
            "run `reset` or reopen the terminal before continuing",
        )
    }
}

pub(super) fn terminal_error(error: io::Error) -> MmError {
    MmError::Terminal {
        message: error.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiExit {
    Quit,
    Interrupted,
}

pub struct LaunchRequest {
    pub snapshot: WorkspaceSnapshot,
    pub access: super::model::AccessView,
    pub environment: Option<EnvironmentStatusSnapshot>,
    pub tls: Option<LocalCaStatus>,
    pub installation_services: ServiceStackStatus,
    pub mounts: Vec<crate::domain::development_mount::DevelopmentMountPlan>,
    pub mount_manager: crate::app::DevelopmentMountManager,
    pub development_units: crate::app::DevelopmentUnitService,
    pub selection: crate::app::WorkspaceSelection,
    pub onboarding: crate::app::OnboardingService,
    pub backends: SecretBackendTuiContext,
    pub backend_manager: crate::app::SecretBackendManager,
    pub secret_manager: crate::app::SecretManager,
    pub gate_manager: crate::app::GateManager,
    pub gate_policies: Vec<crate::domain::gate::GatePolicy>,
    pub scheduler: crate::app::JobSchedulerService,
    pub jobs: crate::domain::scheduler::DesiredDefinitions,
    pub schedules: Vec<crate::domain::scheduler::ScheduleRecord>,
    pub job_runs: Vec<crate::domain::scheduler::JobRun>,
    pub live: Arc<dyn LiveStatusSource>,
    pub control: Arc<dyn WorkspaceControl>,
    pub operations: Arc<dyn super::ServiceOperations>,
    /// Every configured environment, for the Environments view.
    pub environments: Vec<super::EnvironmentEntry>,
    pub directory: Arc<dyn super::EnvironmentDirectory>,
    /// What this environment still needs before it can start.
    pub setup: super::SetupView,
    /// What could not be read while opening; those views are empty.
    pub warnings: Vec<crate::domain::diagnostic::Diagnostic>,
    pub guide: Arc<dyn super::SetupGuide>,
    /// The session holds what the configuration says and nothing read from
    /// an engine or a host yet: it is drawn at once, and the complete
    /// session is read in the background and takes its place.
    pub loading: bool,
}

/// What carries over when the TUI switches to another environment, or from
/// the first frame to the complete session.
struct Resume {
    tab: model::Tab,
    notice: Option<model::Notice>,
    run_history: Vec<String>,
    /// Where the user had got to while the session was being read.
    place: Option<(usize, bool)>,
    /// The action key the first frame held for the complete session.
    held: Option<crossterm::event::KeyEvent>,
}

enum Outcome {
    Exit(TuiExit),
    Switch(Box<LaunchRequest>, Resume),
}

pub async fn run(request: LaunchRequest) -> Result<TuiExit, MmError> {
    let termination_requested = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::clone(&termination_requested);
    ctrlc::set_handler(move || signal_flag.store(true, Ordering::SeqCst)).map_err(|error| {
        MmError::Terminal {
            message: format!("could not install terminal signal handler: {error}"),
        }
    })?;
    let mut session = TerminalSession::new()?;
    let mut request = request;
    let mut resume = Resume {
        tab: model::Tab::Overview,
        notice: None,
        run_history: Vec::new(),
        place: None,
        held: None,
    };
    // Each pass shows one environment; switching keeps the terminal session.
    loop {
        match run_environment(&mut session, request, resume, &termination_requested).await? {
            Outcome::Exit(exit) => return Ok(exit),
            Outcome::Switch(next, carried) => {
                request = *next;
                resume = carried;
            }
        }
    }
}

/// The services a session acts through, apart from what its model shows.
struct Acting {
    mount_manager: crate::app::DevelopmentMountManager,
    development_units: crate::app::DevelopmentUnitService,
    selection: crate::app::WorkspaceSelection,
    onboarding: crate::app::OnboardingService,
    backend_manager: crate::app::SecretBackendManager,
    secret_manager: crate::app::SecretManager,
    gate_manager: crate::app::GateManager,
    scheduler: crate::app::JobSchedulerService,
    live: Arc<dyn LiveStatusSource>,
    control: Arc<dyn WorkspaceControl>,
    operations: Arc<dyn super::ServiceOperations>,
    directory: Arc<dyn super::EnvironmentDirectory>,
    guide: Arc<dyn super::SetupGuide>,
    /// How often the status is read again.
    interval: Duration,
}

impl LaunchRequest {
    /// The model that shows this session, continuing where `resume` left
    /// off, and the services the session acts through.
    fn split(self, resume: Resume) -> (Model, Acting) {
        // A remote status is a full inspect on the host. A session that is
        // still being read has nothing to refresh: it is replaced when read.
        let interval = if self.loading {
            Duration::from_secs(24 * 60 * 60)
        } else if self.access.remote_target.is_some() {
            REMOTE_REFRESH_INTERVAL
        } else {
            REFRESH_INTERVAL
        };
        let mut model = Model::new(ModelInput {
            snapshot: self.snapshot,
            environment: self.environment,
            tls: self.tls,
            installation_services: self.installation_services,
            mounts: self.mounts,
            backends: self.backends,
            gates: self.gate_policies,
            jobs: self.jobs,
            schedules: self.schedules,
            job_runs: self.job_runs,
            access: self.access,
            environments: self.environments,
            setup: self.setup,
        });
        model.launch_warnings = self.warnings;
        resume.apply(&mut model, self.loading);
        let acting = Acting {
            mount_manager: self.mount_manager,
            development_units: self.development_units,
            selection: self.selection,
            onboarding: self.onboarding,
            backend_manager: self.backend_manager,
            secret_manager: self.secret_manager,
            gate_manager: self.gate_manager,
            scheduler: self.scheduler,
            live: self.live,
            control: self.control,
            operations: self.operations,
            directory: self.directory,
            guide: self.guide,
            interval,
        };
        (model, acting)
    }
}

async fn run_environment(
    session: &mut TerminalSession,
    request: LaunchRequest,
    resume: Resume,
    termination_requested: &Arc<AtomicBool>,
) -> Result<Outcome, MmError> {
    let (mut model, acting) = request.split(resume);
    let mut refresh = LiveRefresh::start(acting.live, acting.interval);
    let refresh_now = refresh.trigger();
    let (stack, stack_outcomes) = control::StackRunner::new(acting.control);
    let (operations, operation_events) = ops::OperationRunner::new(acting.operations);
    if model.remote().is_some() {
        operations.read_host_values();
    }
    let (environment_runner, environment_events) =
        environments::EnvironmentRunner::new(acting.directory);
    if model.tab == model::Tab::Environments {
        let current = model.snapshot.environment.clone();
        environment_runner.probe(environments::begin_checks(&mut model, false), &current);
    }
    let use_cases = TuiUseCases {
        mounts: &acting.mount_manager,
        development: &acting.development_units,
        selection: &acting.selection,
        onboarding: &acting.onboarding,
        backends: &acting.backend_manager,
        secrets: &acting.secret_manager,
        gates: &acting.gate_manager,
        scheduler: &acting.scheduler,
        stack: &stack,
        operations: &operations,
        environments: &environment_runner,
        guide: acting.guide.as_ref(),
        refresh_now: &refresh_now,
    };
    let next = event_loop(
        session,
        &mut model,
        &use_cases,
        Updates {
            refresh: &mut refresh,
            stack: stack_outcomes,
            operations: operation_events,
            environments: environment_events,
        },
        termination_requested,
    )
    .await?;
    operations.close_stream();
    if let Some(next) = next {
        let resume = Resume::from(&mut model, &next);
        return Ok(Outcome::Switch(next, resume));
    }
    Ok(Outcome::Exit(
        if termination_requested.load(Ordering::SeqCst) || model.interrupted {
            TuiExit::Interrupted
        } else {
            TuiExit::Quit
        },
    ))
}

impl Resume {
    /// What `model` hands on to the session `next`.
    fn from(model: &mut Model, next: &LaunchRequest) -> Self {
        // The complete session of the environment already on screen says
        // nothing about arriving, and keeps the user where they were.
        let read = model.loading.is_first_frame()
            && next.snapshot.environment == model.snapshot.environment;
        let notice = if read {
            model.notice.take()
        } else {
            arrival_notice(next, model.environments.reloading)
        };
        Self {
            tab: model.tab,
            notice,
            run_history: std::mem::take(&mut model.run_history),
            place: read.then_some((model.scroll, model.help)),
            held: model.loading.held.take().filter(|_| read),
        }
    }

    /// Puts a new session's model where the last one left off. `loading`
    /// says the session is the one of the first frame.
    fn apply(self, model: &mut Model, loading: bool) {
        model.tab = self.tab;
        model.notice = self.notice;
        model.run_history = self.run_history;
        if loading {
            model.loading = loading::Loading::everything();
        }
        model.loading.held = self.held;
        if let Some((scroll, help)) = self.place {
            model.scroll = scroll;
            model.help = help;
            model.clamp_selection();
        } else if model.tab == model::Tab::Environments {
            model.scroll = model
                .environments
                .entries
                .iter()
                .position(|entry| entry.name == model.snapshot.environment)
                .unwrap_or(0);
        }
    }
}

/// What to say on arriving in `next`: after a switch, where it runs; after
/// the environment on screen was read again, how its setup stands.
fn arrival_notice(next: &LaunchRequest, reloaded: bool) -> Option<model::Notice> {
    let environment = &next.snapshot.environment;
    if !reloaded {
        let place = match &next.access.remote_target {
            Some(target) => format!("remote · {target}"),
            None => "local".to_owned(),
        };
        return Some(model::Notice::success(format!(
            "Switched to {environment} ({place})."
        )));
    }
    match next.setup.steps.len() {
        0 if next.setup.unavailable.is_none() => {
            Some(model::Notice::success(format!("{environment} is set up.")))
        }
        0 => None,
        1 => Some(model::Notice::info(format!(
            "{environment} has 1 setup step left."
        ))),
        count => Some(model::Notice::info(format!(
            "{environment} has {count} setup steps left."
        ))),
    }
}

/// Reads the environment on screen again, as a switch to itself.
fn reload(model: &mut Model, use_cases: &TuiUseCases<'_>) {
    let current = model.snapshot.environment.clone();
    model.reload_due = false;
    model.environments.switching = Some(current.clone());
    model.environments.reloading = true;
    use_cases.operations.close_stream();
    use_cases.environments.switch(current);
}

/// Leaves the TUI's screen, runs the fix for one setup step in the plain
/// terminal, and comes back to a fresh read of the environment.
async fn run_setup_step(
    session: &mut TerminalSession,
    model: &mut Model,
    use_cases: &TuiUseCases<'_>,
    termination_requested: &AtomicBool,
    index: usize,
) -> Result<(), MmError> {
    let Some(step) = model.setup.steps.get(index).cloned() else {
        return Ok(());
    };
    let Some(issue) = &step.issue else {
        return Ok(());
    };
    session.suspend()?;
    let outcome = async {
        write_step_heading(&step, index, model.setup.steps.len())?;
        let finished = use_cases.guide.run(issue).await;
        if termination_requested.load(Ordering::SeqCst) {
            return Ok(());
        }
        wait_for_key(termination_requested, finished)
    }
    .await;
    // The terminal comes back even when the step or the prompt failed.
    session.resume()?;
    outcome?;
    if termination_requested.load(Ordering::SeqCst) {
        model.interrupted = true;
        model.should_quit = true;
        return Ok(());
    }
    reload(model, use_cases);
    Ok(())
}

fn write_step_heading(step: &super::SetupStep, index: usize, total: usize) -> Result<(), MmError> {
    use std::io::Write as _;

    let mut stdout = io::stdout().lock();
    let safe = crate::presentation::output::terminal_safe;
    writeln!(
        stdout,
        "Step {} of {total}: {}",
        index + 1,
        safe(&step.title)
    )
    .and_then(|()| writeln!(stdout, "{}", safe(&step.detail)))
    .and_then(|()| match &step.command {
        Some(command) => writeln!(stdout, "Same as: {}\n", safe(command)),
        None => writeln!(stdout),
    })
    .and_then(|()| stdout.flush())
    .map_err(terminal_error)
}

/// Holds the step's output on screen until a key is pressed.
fn wait_for_key(termination_requested: &AtomicBool, finished: bool) -> Result<(), MmError> {
    use std::io::Write as _;

    let mut stdout = io::stdout().lock();
    let lead = if finished {
        ""
    } else {
        "The step did not finish. "
    };
    write!(stdout, "\n{lead}Press any key to go back to mm. ")
        .and_then(|()| stdout.flush())
        .map_err(terminal_error)?;
    drop(stdout);
    enable_raw_mode().map_err(terminal_error)?;
    let pressed = loop {
        if termination_requested.load(Ordering::SeqCst) {
            break Ok(());
        }
        match event::poll(Duration::from_millis(250)) {
            Ok(false) => {}
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) if key.kind == crossterm::event::KeyEventKind::Press => {
                    if key.code == crossterm::event::KeyCode::Char('c')
                        && key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        termination_requested.store(true, Ordering::SeqCst);
                    }
                    break Ok(());
                }
                Ok(_) => {}
                Err(error) => break Err(terminal_error(error)),
            },
            Err(error) => break Err(terminal_error(error)),
        }
    };
    disable_raw_mode().map_err(terminal_error)?;
    pressed
}

/// Results that arrive from background tasks between key presses.
struct Updates<'a> {
    refresh: &'a mut LiveRefresh,
    stack: tokio::sync::mpsc::UnboundedReceiver<control::StackOutcome>,
    operations: tokio::sync::mpsc::UnboundedReceiver<ops::OperationEvent>,
    environments: tokio::sync::mpsc::UnboundedReceiver<environments::EnvironmentEvent>,
}

/// How long the loop waits for a key before redrawing; shorter while
/// output streams or parts of the session are arriving.
fn poll_interval(model: &Model) -> Duration {
    if model.panel.is_some() {
        Duration::from_millis(80)
    } else if model.loading.is_active() {
        Duration::from_millis(40)
    } else {
        Duration::from_millis(250)
    }
}

async fn event_loop(
    session: &mut TerminalSession,
    model: &mut Model,
    use_cases: &TuiUseCases<'_>,
    mut updates: Updates<'_>,
    termination_requested: &Arc<AtomicBool>,
) -> Result<Option<Box<LaunchRequest>>, MmError> {
    let mut last_tick = std::time::Instant::now();
    let mut drawn = false;
    let mut reading = false;
    while !model.should_quit && !termination_requested.load(Ordering::SeqCst) {
        while let Some(update) = updates.refresh.try_next() {
            match update {
                // The nodes that answered so far; the others keep what
                // they last said until they answer or are given up on.
                live::LiveUpdate::SoFar(snapshot) if !model.loading.is_active() => {
                    model.show_status(*snapshot);
                    model.clamp_selection();
                }
                live::LiveUpdate::SoFar(_) => {}
                live::LiveUpdate::Done(result) => model.apply_live(*result),
            }
        }
        if model.reload_due && model.environments.switching.is_none() {
            reload(model, use_cases);
        }
        while let Ok(outcome) = updates.stack.try_recv() {
            control::apply_outcome(model, outcome);
            use_cases.refresh_now.notify_one();
        }
        while let Ok(event) = updates.operations.try_recv() {
            let restarted = matches!(event, ops::OperationEvent::Restarted { .. });
            ops::apply_event(model, event);
            if restarted {
                use_cases.refresh_now.notify_one();
            }
        }
        while let Ok(event) = updates.environments.try_recv() {
            let event = match event {
                // The session read after the first frame takes the place
                // of the one on screen. When it cannot be built at all,
                // mm leaves with the reason, as it does for any command.
                environments::EnvironmentEvent::Read {
                    environment,
                    result,
                } if loading::takes_read(model, &environment) => {
                    return result.map(Some);
                }
                event => event,
            };
            if let Some(next) = environments::apply_event(model, event) {
                return Ok(Some(next));
            }
        }
        session
            .terminal
            .draw(|frame| view::render(frame, model))
            .map_err(terminal_error)?;
        if !std::mem::replace(&mut drawn, true) {
            crate::app::timing::mark("first frame");
        }
        // Reading starts once the first frame is up. A switch to another
        // environment takes its place; when that switch is dropped, the
        // session on screen is read again.
        if model.loading.is_first_frame() {
            if model.environments.switching.is_some() {
                reading = false;
            } else if !std::mem::replace(&mut reading, true) {
                use_cases
                    .environments
                    .load(model.snapshot.environment.clone());
            }
        }
        // The key the first frame held runs once this session is up.
        if let Some(key) = loading::release(model) {
            handle_key(session, model, use_cases, termination_requested, key).await?;
        } else if event::poll(poll_interval(model)).map_err(terminal_error)? {
            handle_event(session, model, use_cases, termination_requested).await?;
        }
        // The spinner advances on a steady clock, not once per key or redraw.
        if last_tick.elapsed() >= Duration::from_millis(120) {
            model.tick = model.tick.wrapping_add(1);
            last_tick = std::time::Instant::now();
        }
    }
    Ok(None)
}

async fn handle_event(
    session: &mut TerminalSession,
    model: &mut Model,
    use_cases: &TuiUseCases<'_>,
    termination_requested: &Arc<AtomicBool>,
) -> Result<(), MmError> {
    match event::read().map_err(terminal_error)? {
        // An action waits for the complete session, which takes the place
        // of this one when it is read, after every part has arrived.
        Event::Key(key) if loading::hold(model, key) => {}
        Event::Key(key) => {
            handle_key(session, model, use_cases, termination_requested, key).await?;
        }
        Event::Resize(_, _) => {
            update(model, Message::Resize);
        }
        _ => {}
    }
    Ok(())
}

async fn handle_key(
    session: &mut TerminalSession,
    model: &mut Model,
    use_cases: &TuiUseCases<'_>,
    termination_requested: &Arc<AtomicBool>,
    key: crossterm::event::KeyEvent,
) -> Result<(), MmError> {
    let action = update(model, Message::Key(key));
    if let Action::RunSetup(index) = action {
        return run_setup_step(session, model, use_cases, termination_requested, index).await;
    }
    session
        .terminal
        .draw(|frame| view::render(frame, model))
        .map_err(terminal_error)?;
    if let Some(request) = apply_action(model, action, use_cases).await
        && let Some(material) =
            read_hidden_input(session, model, termination_requested, &request.prompt)?
    {
        if let super::PendingSecretApply::RemoteSecret { name } = request.pending {
            use_cases.operations.stage(crate::app::KeyEdit {
                kind: crate::domain::staged::ValueKind::Secret,
                key: name,
                edit: crate::app::ValueEdit::Set(material.into_zeroizing()),
            });
            return Ok(());
        }
        apply_hidden_secret(
            model,
            request.pending,
            material,
            use_cases.backends,
            use_cases.secrets,
            use_cases.gates,
        )
        .await;
    }
    Ok(())
}

pub(super) fn read_hidden_input(
    session: &mut TerminalSession,
    model: &mut Model,
    termination_requested: &AtomicBool,
    prompt: &str,
) -> Result<Option<crate::domain::secret::SecretMaterial>, MmError> {
    let mut value = Zeroizing::new(String::new());
    loop {
        session
            .terminal
            .draw(|frame| view::render_secret_prompt(frame, model, prompt, value.chars().count()))
            .map_err(terminal_error)?;
        if termination_requested.load(Ordering::SeqCst) {
            model.interrupted = true;
            return Ok(None);
        }
        if !event::poll(Duration::from_millis(250)).map_err(terminal_error)? {
            continue;
        }
        match event::read().map_err(terminal_error)? {
            Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                match key.code {
                    crossterm::event::KeyCode::Enter => {
                        return crate::domain::secret::SecretMaterial::from_zeroizing(value)
                            .map(Some)
                            .map_err(|message| MmError::Security { message });
                    }
                    crossterm::event::KeyCode::Esc => {
                        match model.tab {
                            model::Tab::Providers => {
                                model.providers.screen = provider::ProviderScreen::Cancelled;
                            }
                            model::Tab::Gates => model.gates.screen = gate::GateScreen::Cancelled,
                            _ => {
                                model.notice = Some(model::Notice::info("Secret entry cancelled."));
                            }
                        }
                        return Ok(None);
                    }
                    crossterm::event::KeyCode::Backspace => {
                        value.pop();
                    }
                    crossterm::event::KeyCode::Char('c')
                        if key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        model.interrupted = true;
                        model.should_quit = true;
                        return Ok(None);
                    }
                    crossterm::event::KeyCode::Char(character)
                        if !character.is_control() && value.len() < 1024 * 1024 =>
                    {
                        value.push(character);
                    }
                    _ => {}
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::terminal_cleanup_failures;

    #[test]
    fn terminal_restoration_reports_every_failure() {
        let failures = terminal_cleanup_failures([
            Err(std::io::Error::other("raw mode restore failed")),
            Err(std::io::Error::other("screen restore failed")),
            Ok(()),
        ]);

        assert_eq!(failures.len(), 2);
        assert!(failures[0].contains("raw mode"));
        assert!(failures[1].contains("screen"));
    }
}
