//! Opening on the first frame and filling in.
//!
//! The terminal UI draws from what the configuration says before anything
//! is asked of an engine or a host. Each part of the session is then read
//! in the background and shown when it arrives; a view whose part has not
//! arrived says so instead of looking empty. While parts are still coming
//! in, only keys that look around do anything at once: an action would run
//! against a session that is about to be replaced by the complete one, so
//! it is held, and the complete session runs it when it takes the place of
//! the first frame's.

use std::collections::BTreeSet;
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::domain::ca::LocalCaStatus;
use crate::domain::diagnostic::Diagnostic;
use crate::domain::environment::EnvironmentStatusSnapshot;
use crate::domain::service_stack::ServiceStackStatus;
use crate::domain::workspace::WorkspaceSnapshot;
use crate::presentation::tui::model::{Model, Notice, Tab};

/// What is read after the first frame, one entry per thing a view waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    /// Services, routes, the engine, and the findings of the environment.
    Status,
    Variables,
    Mounts,
    /// Who the host says you are.
    Identity,
    Https,
    SharedServices,
}

impl Part {
    const ALL: [Self; 6] = [
        Self::Status,
        Self::Variables,
        Self::Mounts,
        Self::Identity,
        Self::Https,
        Self::SharedServices,
    ];
}

/// A part of the session that was read.
pub enum Loaded {
    /// What the environment still needs before it can start.
    Setup(super::SetupView),
    /// The status so far. On a cluster it arrives once per node that
    /// answers, each time with the nodes that have not answered yet.
    Status {
        snapshot: Box<WorkspaceSnapshot>,
        /// Every host was asked and answered or failed.
        complete: bool,
    },
    Variables(Option<Box<EnvironmentStatusSnapshot>>),
    Mounts(Vec<crate::domain::development_mount::DevelopmentMountPlan>),
    /// Who the host says you are, or why it did not say.
    Identity {
        identity: Option<crate::domain::remote::RemoteIdentity>,
        error: Option<String>,
    },
    Https(Option<LocalCaStatus>),
    SharedServices(Box<ServiceStackStatus>),
}

/// Where the reader of a session reports each part as it has it.
#[derive(Clone)]
pub struct LoadProgress {
    report: Option<Arc<dyn Fn(Loaded) + Send + Sync>>,
}

impl LoadProgress {
    /// For a session that is shown only once it is complete.
    #[must_use]
    pub fn silent() -> Self {
        Self { report: None }
    }

    pub(super) fn to(report: impl Fn(Loaded) + Send + Sync + 'static) -> Self {
        Self {
            report: Some(Arc::new(report)),
        }
    }

    /// Whether anyone shows the parts; when not, building one is wasted.
    #[must_use]
    pub fn is_shown(&self) -> bool {
        self.report.is_some()
    }

    pub fn loaded(&self, part: Loaded) {
        if let Some(report) = &self.report {
            report(part);
        }
    }
}

/// Which parts of the session on screen have not arrived yet.
#[derive(Debug, Clone, Default)]
pub struct Loading {
    pending: BTreeSet<Part>,
    /// The session on screen is the first frame's, and the complete session
    /// read after it has not taken its place yet. Every part arrives before
    /// that read is done, so this outlives `pending`.
    first_frame: bool,
    /// The action key pressed last while the session was the first frame's,
    /// for the complete session to run ([`hold`], [`release`]).
    pub(super) held: Option<KeyEvent>,
}

impl Loading {
    /// Nothing has been read yet.
    #[must_use]
    pub fn everything() -> Self {
        Self {
            pending: Part::ALL.into_iter().collect(),
            first_frame: true,
            held: None,
        }
    }

    /// Whether the session is still being read.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Whether the session on screen waits to be replaced by the complete
    /// one: its own status is never read again, so it must not stay.
    #[must_use]
    pub fn is_first_frame(&self) -> bool {
        self.first_frame
    }

    #[must_use]
    pub fn is_pending(&self, part: Part) -> bool {
        self.pending.contains(&part)
    }

    fn arrived(&mut self, part: Part) {
        self.pending.remove(&part);
    }

    /// The part the view of `tab` waits for, if it is still on its way.
    #[must_use]
    pub fn waiting(&self, tab: Tab) -> Option<Part> {
        let part = match tab {
            Tab::Overview | Tab::Services | Tab::Ingress | Tab::Diagnostics => Part::Status,
            Tab::Variables => Part::Variables,
            Tab::Mounts => Part::Mounts,
            Tab::Tls => Part::Https,
            Tab::Access => Part::Identity,
            Tab::Environments | Tab::Gates | Tab::Providers | Tab::Jobs | Tab::Guided => {
                return None;
            }
        };
        self.is_pending(part).then_some(part)
    }
}

/// Shows a part that arrived.
pub(super) fn apply(model: &mut Model, part: Loaded) {
    if !model.loading.is_active() {
        return;
    }
    match part {
        Loaded::Setup(setup) => model.setup = setup,
        Loaded::Status { snapshot, complete } => {
            model.show_status(*snapshot);
            if complete {
                model.loading.arrived(Part::Status);
            }
        }
        Loaded::Variables(environment) => {
            model.environment = environment.map(|environment| *environment);
            model.loading.arrived(Part::Variables);
        }
        Loaded::Mounts(mounts) => {
            model.mounts = mounts;
            model.loading.arrived(Part::Mounts);
        }
        Loaded::Identity { identity, error } => {
            model.access.identity = identity;
            if model.access.identity_error.is_none() {
                model.access.identity_error = error;
            }
            model.loading.arrived(Part::Identity);
        }
        Loaded::Https(tls) => {
            model.tls = tls;
            model.loading.arrived(Part::Https);
        }
        Loaded::SharedServices(services) => {
            model.installation_services = *services;
            model.loading.arrived(Part::SharedServices);
        }
    }
    model.clamp_selection();
}

/// Whether `key` only looks around: moving between views and rows, help,
/// and leaving. Those work while the session is still being read; from the
/// Environments view, so does switching to another environment.
#[must_use]
pub(super) fn looks_around(model: &Model, key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return key.code == KeyCode::Char('c');
    }
    if model.help || model.environments.switching.is_some() {
        return true;
    }
    match key.code {
        KeyCode::Esc
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Home
        | KeyCode::End => true,
        KeyCode::Char(character) => {
            matches!(character, 'q' | '?' | 'h' | 'j' | 'k' | 'l' | '[' | ']')
                || Tab::from_digit(character).is_some()
        }
        KeyCode::Enter => model.tab == Tab::Environments,
        _ => false,
    }
}

/// Whether the complete session read for `environment` takes the place of
/// the one on screen: the one on screen is the first frame's, of that
/// environment, and no switch to another one is under way. This holds after
/// every part has arrived too: the parts fill the first frame in, but only
/// the complete session reads its status again.
#[must_use]
pub(super) fn takes_read(model: &Model, environment: &str) -> bool {
    model.loading.is_first_frame()
        && model.environments.switching.is_none()
        && environment == model.snapshot.environment
}

/// Whether `key` waits for the complete session: one that does more than
/// look around, pressed while the session on screen is the first frame's.
/// What it does would happen in a session about to be replaced, and its
/// result, such as a reviewed plan, would go with it; so it is held, and
/// the footer says so. Any other key drops a key held before it: the user went
/// on, and the selection it acted on may have moved.
pub(super) fn hold(model: &mut Model, key: KeyEvent) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }
    model.loading.held = None;
    if !model.loading.is_first_frame() || looks_around(model, key) {
        return false;
    }
    model.loading.held = Some(key);
    model.notice = Some(Notice::info(format!(
        "Still reading {}; this goes ahead as soon as it is here, unless you press another key first.",
        model.snapshot.environment
    )));
    true
}

/// The key held for the complete session, once it is the one on screen.
pub(super) fn release(model: &mut Model) -> Option<KeyEvent> {
    if model.loading.is_first_frame() {
        return None;
    }
    model.loading.held.take()
}

/// A finding for every node that was asked and did not answer, and one for
/// what differs on a host from its last deployment, in the words of
/// `mm service status`. A node that has not answered yet is not one.
pub(super) fn node_findings(snapshot: &WorkspaceSnapshot) -> Vec<Diagnostic> {
    let silent = || snapshot.nodes.iter().filter(|node| !node.reachable);
    let mut findings = silent()
        .filter_map(|node| {
            let problem = node.problem.as_deref()?;
            Some(Diagnostic::error(
                "MM_UNREACHABLE",
                format!("node '{}' {problem}", node.name),
            ))
        })
        .collect::<Vec<_>>();
    // The notes begin with one line for each node that does not answer,
    // which the findings above already say.
    let notes = crate::presentation::output::node_notes(snapshot);
    for note in notes.iter().skip(silent().count()) {
        // What differs, then the sentence that says how to repair it.
        let (differs, repair) = note.rsplit_once(". ").unwrap_or((note.as_str(), ""));
        let finding = Diagnostic::warning("MM_NODE_DRIFT", differs);
        findings.push(if repair.is_empty() {
            finding
        } else {
            finding.with_help(repair)
        });
    }
    findings
}
