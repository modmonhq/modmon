//! The terminal UI driven in-process: keys in, the screen out as text.
//!
//! Tests use it to walk the views of a remote or cluster environment
//! without a terminal: build a session from a status, press keys, hand it
//! what the background tasks would deliver (a reviewed plan, a step of an
//! apply), and read what a terminal of a given size shows. Keys go through
//! the same `update` as a person's and the screen is drawn by the same
//! `render`. `scripts/drive-terminal-ui.py` does the same with the real
//! binary on a pseudo-terminal.

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::domain::environment::EnvironmentStatusSnapshot;
use crate::domain::error::MmError;
use crate::domain::rollout::RolloutProgress;
use crate::domain::workspace::WorkspaceSnapshot;
use crate::presentation::tui::control::{StackOutcome, StartReview, apply_outcome};
use crate::presentation::tui::model::{AccessView, Model, ModelInput};
use crate::presentation::tui::update::{Action, Message, update};

/// One session of the terminal UI without a terminal.
pub struct ScriptedUi {
    model: Model,
}

impl ScriptedUi {
    /// A session that shows `snapshot`, on the remote target `remote` when
    /// one is named.
    #[must_use]
    pub fn new(snapshot: WorkspaceSnapshot, remote: Option<&str>) -> Self {
        let environment = snapshot.environment.clone();
        let backends = super::SecretBackendTuiContext {
            root: snapshot.workspace.root.clone(),
            target: snapshot.target.clone(),
            config_hash: snapshot.config_hash.clone(),
            project_id: snapshot.workspace.project_id,
            environment,
            external_connections: Vec::new(),
            managed_connections: Vec::new(),
            drivers: Vec::new(),
            instances: BTreeMap::new(),
            rotatable_secrets: Vec::new(),
        };
        let model = Model::new(ModelInput {
            snapshot,
            environment: None,
            tls: None,
            installation_services: crate::domain::service_stack::ServiceStackStatus {
                installed: false,
                stack_version: None,
                runtime: None,
                target_id: None,
                route: String::new(),
                internal_smtp: String::new(),
                host_smtp_port: None,
                components: Vec::new(),
                diagnostics: Vec::new(),
            },
            mounts: Vec::new(),
            backends,
            gates: Vec::new(),
            jobs: crate::domain::scheduler::DesiredDefinitions {
                jobs: BTreeMap::new(),
                schedules: BTreeMap::new(),
            },
            schedules: Vec::new(),
            job_runs: Vec::new(),
            access: AccessView {
                remote_target: remote.map(str::to_owned),
                ..AccessView::default()
            },
            environments: Vec::new(),
            setup: super::SetupView::default(),
        });
        Self { model }
    }

    /// Presses one key: a character, or `enter`, `esc`, `tab`, `space`,
    /// `up`, `down`, `left`, `right`, `pgup`, `pgdn`. Returns what the UI
    /// asks the session to do, as its debug text (`None` for nothing).
    pub fn press(&mut self, key: &str) -> String {
        let code = match key {
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "space" => KeyCode::Char(' '),
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "pgup" => KeyCode::PageUp,
            "pgdn" => KeyCode::PageDown,
            other => KeyCode::Char(other.chars().next().unwrap_or(' ')),
        };
        let key = KeyEvent::new(code, KeyModifiers::NONE);
        // As the session does: an action pressed on the first frame waits.
        if super::loading::hold(&mut self.model, key) {
            return format!("{:?}", Action::None);
        }
        self.act(key)
    }

    /// The complete session took the place of the first frame's, and runs
    /// the key the first frame held for it, if any. Returns what that key
    /// asks the session to do, as [`Self::press`] does.
    pub fn complete(&mut self) -> String {
        let held = self.model.loading.held.take();
        self.model.loading = super::loading::Loading::default();
        self.model.loading.held = held;
        match super::loading::release(&mut self.model) {
            Some(key) => self.act(key),
            None => format!("{:?}", Action::None),
        }
    }

    fn act(&mut self, key: KeyEvent) -> String {
        let action = update(&mut self.model, Message::Key(key));
        // What the session does at once with a stack operation: it is busy.
        if let Action::Stack(operation) = &action
            && self.model.busy.is_none()
        {
            self.model.busy = Some(operation.clone());
        }
        format!("{action:?}")
    }

    /// The plan of a review came back.
    pub fn reviewed(&mut self, review: StartReview) {
        apply_outcome(&mut self.model, StackOutcome::Reviewed(Ok(review)));
    }

    /// A node of the apply started a step.
    pub fn progress(&mut self, step: RolloutProgress) {
        apply_outcome(&mut self.model, StackOutcome::Progress(step));
    }

    /// The apply ended, with its summary or its error in the words of
    /// `presentation::output::error_line`.
    pub fn applied(&mut self, result: Result<String, String>) {
        apply_outcome(&mut self.model, StackOutcome::Started(result));
    }

    /// The session of the first frame, drawn before anything was read: its
    /// parts are on their way and the complete session is to take its place.
    #[must_use]
    pub fn opening(snapshot: WorkspaceSnapshot, remote: Option<&str>) -> Self {
        let mut ui = Self::new(snapshot, remote);
        ui.model.loading = super::loading::Loading::everything();
        ui
    }

    /// A part of the first frame's session arrived.
    pub fn loaded(&mut self, part: super::Loaded) {
        super::loading::apply(&mut self.model, part);
    }

    /// Whether the complete session read for `environment` now takes the
    /// place of the one on screen, as the session loop decides. Only the
    /// complete session reads its status again on the refresh interval.
    #[must_use]
    pub fn takes_read(&self, environment: &str) -> bool {
        super::loading::takes_read(&self.model, environment)
    }

    /// A status arrived, as a refresh delivers it.
    pub fn show_status(&mut self, snapshot: WorkspaceSnapshot) {
        self.model.show_status(snapshot);
        self.model.clamp_selection();
    }

    /// The variables of the environment were resolved.
    pub fn show_variables(&mut self, environment: EnvironmentStatusSnapshot) {
        self.model.environment = Some(environment);
        self.model.clamp_selection();
    }

    /// What a remote environment's hosts hold arrived.
    pub fn host_values(&mut self, values: crate::domain::staged::RemoteValues) {
        self.model.remote_values = Some(values);
        self.model.clamp_selection();
    }

    /// The footer's message, if one shows.
    #[must_use]
    pub fn notice(&self) -> Option<String> {
        self.model.notice.as_ref().map(|notice| notice.text.clone())
    }

    /// What a terminal of this size shows, one line per row, trailing
    /// spaces trimmed.
    pub fn screen(&self, width: u16, height: u16) -> Result<String, MmError> {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).map_err(|error| MmError::Terminal {
                message: error.to_string(),
            })?;
        terminal
            .draw(|frame| super::view::render(frame, &self.model))
            .map_err(|error| MmError::Terminal {
                message: error.to_string(),
            })?;
        let buffer = terminal.backend().buffer();
        let mut rows = Vec::new();
        for y in 0..buffer.area.height {
            let row = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            rows.push(row.trim_end().to_owned());
        }
        Ok(rows.join("\n"))
    }
}
