//! What a command shows while it waits.

use std::io::{self, Stderr, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::Size;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};

use super::Surface;
use super::style::Palette;
use super::tasks::Tasks;
use crate::app::ports::ProgressReporter;
use crate::domain::progress::{NodeState, ProgressEvent, StepOutcome};
use crate::domain::rollout::{ProgressStep, RolloutProgress};
use crate::presentation::cli::OutputFormat;
use crate::presentation::tui::Tone;

/// How often the viewport is drawn: the spinner's pace.
const TICK: Duration = Duration::from_millis(80);
/// The most rows the viewport takes, whatever the terminal's height.
const MAX_ROWS: usize = 16;
/// The step a deployment's rollout reports its nodes under.
pub const ROLLOUT: &str = "rollout";

/// A painter is drawing: a panic must give the terminal back.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// A panic happened: the painter stops drawing over its message.
static ABANDONED: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK: Once = Once::new();

type Screen = Terminal<CrosstermBackend<Stderr>>;

/// The progress of one command.
pub struct Progress {
    surface: Surface,
    tasks: Arc<Mutex<Tasks>>,
    painter: Option<Painter>,
    stopped: bool,
}

struct Painter {
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Progress {
    /// Progress shown the way this process shows `format`.
    #[must_use]
    pub fn new(format: OutputFormat) -> Self {
        Self::on(Surface::of(format))
    }

    /// Progress on `surface`.
    #[must_use]
    pub fn on(surface: Surface) -> Self {
        let palette = if surface.is_live() {
            Palette::color()
        } else {
            Palette::plain()
        };
        let tasks = Arc::new(Mutex::new(Tasks::new(palette)));
        let painter = surface
            .is_live()
            .then(|| Painter::start(Arc::clone(&tasks)).ok())
            .flatten();
        let surface = match (surface, &painter) {
            (Surface::Live, None) => Surface::Plain,
            (surface, _) => surface,
        };
        Self {
            surface,
            tasks,
            painter,
            stopped: false,
        }
    }

    /// Where this progress shows.
    #[must_use]
    pub const fn surface(&self) -> Surface {
        self.surface
    }

    fn tasks(&self) -> MutexGuard<'_, Tasks> {
        self.tasks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lists a step to come, so the list shows the whole run.
    pub fn plan(&self, step: &str, label: &str) {
        self.tasks().plan(step, label);
    }

    /// A line recorded in order with the finished steps.
    pub fn note(&self, tone: Option<Tone>, text: impl Into<String>) {
        let text = text.into();
        if self.surface != Surface::Quiet {
            crate::presentation::record::line(&text);
        }
        self.tasks().note(tone, text);
        self.flush_plain();
    }

    /// A deployment's step on one node.
    pub fn rollout(&self, progress: &RolloutProgress) {
        match self.surface {
            Surface::Quiet => return,
            Surface::Plain | Surface::Live => {
                let line = format!(
                    "==> {}",
                    crate::presentation::output::terminal_safe(&progress.to_string())
                );
                crate::presentation::record::line(&line);
                if self.surface == Surface::Plain {
                    eprintln!("{line}");
                    return;
                }
            }
        }
        let node = progress.node.as_str();
        let (state, detail) = match &progress.step {
            ProgressStep::Operation(line) => {
                self.tasks().note(Some(Tone::Info), line.clone());
                return;
            }
            ProgressStep::Note(line) => {
                self.tasks().note(None, format!("{node}: {line}"));
                return;
            }
            ProgressStep::Healthy(stack) => (NodeState::Done, format!("{stack} healthy")),
            ProgressStep::RollingBack => (NodeState::Failed, "rolling back".to_owned()),
            step => (NodeState::Working, step.to_string()),
        };
        self.report(ProgressEvent::node(ROLLOUT, node, state, Some(&detail)));
    }

    /// Ends the live view.
    pub fn finish(mut self) {
        self.stop(false);
    }

    fn stop(&mut self, interrupted: bool) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if interrupted {
            self.tasks().interrupt(Instant::now());
        }
        match self.painter.take() {
            Some(painter) => painter.stop(),
            None => self.flush_plain(),
        }
    }

    fn flush_plain(&self) {
        if self.surface != Surface::Plain {
            return;
        }
        let lines = self.tasks().take_plain();
        if lines.is_empty() {
            return;
        }
        let mut stderr = io::stderr().lock();
        for line in lines {
            let _ignored = writeln!(stderr, "{line}");
        }
    }
}

impl ProgressReporter for Progress {
    fn report(&self, event: ProgressEvent) {
        crate::presentation::record::event(&event);
        if self.surface == Surface::Quiet {
            return;
        }
        self.tasks().apply(event, Instant::now());
        self.flush_plain();
    }
}

impl Drop for Progress {
    /// Ending without `finish` records the running steps as interrupted.
    fn drop(&mut self) {
        let running = self.tasks().running();
        self.stop(running);
    }
}

impl Painter {
    fn start(tasks: Arc<Mutex<Tasks>>) -> io::Result<Self> {
        let screen = open(1)?;
        install_panic_hook();
        ACTIVE.store(true, Ordering::SeqCst);
        let (stop, stopped) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("mm-progress".to_owned())
            .spawn(move || paint(screen, &tasks, &stopped));
        match thread {
            Ok(thread) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            Err(error) => {
                ACTIVE.store(false, Ordering::SeqCst);
                Err(error)
            }
        }
    }

    fn stop(mut self) {
        let _ignored = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ignored = thread.join();
        }
        ACTIVE.store(false, Ordering::SeqCst);
    }
}

fn open(height: u16) -> io::Result<Screen> {
    Terminal::with_options(
        CrosstermBackend::new(io::stderr()),
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
}

/// The painter's loop: draw now, then every tick, until told to stop.
fn paint(mut screen: Screen, tasks: &Mutex<Tasks>, stopped: &mpsc::Receiver<()>) {
    let mut tick = 0_u64;
    let mut height = 1_u16;
    loop {
        let wait = if tick == 0 { Duration::ZERO } else { TICK };
        let stopping = !matches!(stopped.recv_timeout(wait), Err(RecvTimeoutError::Timeout));
        if ABANDONED.load(Ordering::SeqCst) {
            return;
        }
        let size = screen.size().unwrap_or(Size {
            width: 80,
            height: 24,
        });
        let width = usize::from(size.width.max(20)).min(super::MAX_WIDTH);
        let rows = usize::from(size.height.saturating_sub(2)).clamp(1, MAX_ROWS);
        let (records, live) = {
            let mut tasks = tasks.lock().unwrap_or_else(PoisonError::into_inner);
            let records = tasks.take_records(width);
            let live = if stopping {
                Vec::new()
            } else {
                tasks.live(Instant::now(), tick, width, rows)
            };
            (records, live)
        };
        if !records.is_empty() {
            let _ignored = record(&mut screen, records);
        }
        if stopping {
            let _ignored = close(&mut screen);
            return;
        }
        let needed = u16::try_from(live.len().max(1)).unwrap_or(1);
        if needed > height && grow(&mut screen, needed).is_ok() {
            height = needed;
        }
        let _ignored = screen.draw(|frame| {
            frame.render_widget(Paragraph::new(live), frame.area());
        });
        tick += 1;
    }
}

/// Writes finished steps above the viewport.
fn record(screen: &mut Screen, lines: Vec<Line<'static>>) -> io::Result<()> {
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    screen.insert_before(height, |buffer| {
        Paragraph::new(lines).render(buffer.area, buffer);
    })
}

/// A taller viewport in place of the current one, from the same row.
fn grow(screen: &mut Screen, height: u16) -> io::Result<()> {
    let top = screen.get_frame().area().as_position();
    screen.clear()?;
    screen.set_cursor_position(top)?;
    *screen = open(height)?;
    Ok(())
}

/// Clears the viewport and leaves the cursor at its top, shown.
fn close(screen: &mut Screen) -> io::Result<()> {
    let top = screen.get_frame().area().as_position();
    screen.clear()?;
    screen.set_cursor_position(top)?;
    screen.show_cursor()?;
    io::stderr().flush()
}

/// Gives the terminal back before a panic message prints.
fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if ACTIVE.load(Ordering::SeqCst) {
                ABANDONED.store(true, Ordering::SeqCst);
                let mut stderr = io::stderr();
                let _ignored = crossterm::execute!(stderr, crossterm::cursor::Show);
                let _ignored = writeln!(stderr);
            }
            previous(info);
        }));
    });
}

/// Reports `work` as one step.
pub async fn step<T, E>(
    progress: &Progress,
    (step, label): (&str, &str),
    work: impl std::future::Future<Output = Result<T, E>>,
    summary: impl FnOnce(&T) -> Option<String>,
) -> Result<T, E> {
    progress.report(ProgressEvent::started(step, label));
    let result = work.await;
    progress.report(match &result {
        Ok(value) => ProgressEvent::finished(step, StepOutcome::Done, summary(value)),
        Err(_) => ProgressEvent::finished(step, StepOutcome::Failed, None),
    });
    result
}
