//! Service action state: the action menu, the log viewer, and running a command.

use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::presentation::tui::model::{Confirm, Model, Notice, Tab};
use crate::presentation::tui::ops::ServiceTarget;
use crate::presentation::tui::update::Action;

/// Lines kept per panel; older lines are dropped and counted.
pub(super) const MAXIMUM_LINES: usize = 5_000;
/// Longest command line accepted from the prompt.
const MAXIMUM_COMMAND_CHARS: usize = 4 * 1024;
/// Commands remembered for the up and down keys, per session.
const MAXIMUM_HISTORY: usize = 50;
const PAGE: usize = 15;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogLine {
    pub timestamp: Option<String>,
    pub stderr: bool,
    /// The cluster node the line comes from; `None` on a single host.
    pub node: Option<String>,
    /// The container that wrote the line, by its short name (`web-1`).
    pub container: Option<String>,
    /// What mm says about a node, such as that it did not answer.
    pub about_node: bool,
    pub text: String,
}

impl LogLine {
    /// Whether the line, or the name of its node or container, contains
    /// `needle`, which is lowercase.
    fn contains(&self, needle: &str) -> bool {
        let names = [self.node.as_deref(), self.container.as_deref()];
        let mut parts = names.into_iter().flatten().chain([self.text.as_str()]);
        needle.is_empty() || parts.any(|part| part.to_lowercase().contains(needle))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamState {
    Connecting,
    Streaming,
    Ended,
    Failed(String),
}

/// A full-width view over the services list; it captures keys until closed.
#[derive(Debug, Clone)]
pub enum Panel {
    Logs(LogsPanel),
    Run(RunPanel),
}

#[derive(Debug, Clone)]
pub struct LogsPanel {
    pub session: u64,
    pub target: ServiceTarget,
    pub lines: VecDeque<LogLine>,
    /// Lines dropped from the front once the buffer was full.
    pub dropped: usize,
    pub state: StreamState,
    /// Matching lines between the bottom of the view and the newest line; 0 follows.
    pub scroll_back: usize,
    /// Lines that arrived while scrolled back.
    pub unseen: usize,
    pub wrap: bool,
    pub timestamps: bool,
    pub filter: String,
    /// Keys go to the filter box instead of shortcuts.
    pub filtering: bool,
}

impl LogsPanel {
    #[must_use]
    pub fn new(session: u64, target: ServiceTarget) -> Self {
        Self {
            session,
            target,
            lines: VecDeque::new(),
            dropped: 0,
            state: StreamState::Connecting,
            scroll_back: 0,
            unseen: 0,
            wrap: true,
            timestamps: false,
            filter: String::new(),
            filtering: false,
        }
    }

    fn matches(&self, line: &LogLine) -> bool {
        line.contains(&self.filter.to_lowercase())
    }

    pub fn push(&mut self, mut line: LogLine) {
        line.text = strip_ansi(&line.text);
        if self.scroll_back > 0 && self.matches(&line) {
            self.scroll_back += 1;
            self.unseen += 1;
        }
        if self.lines.len() == MAXIMUM_LINES {
            self.lines.pop_front();
            self.dropped += 1;
        }
        self.lines.push_back(line);
        self.clamp();
    }

    /// The lines that match the filter, oldest first.
    #[must_use]
    pub fn visible(&self) -> Vec<&LogLine> {
        let needle = self.filter.to_lowercase();
        self.lines
            .iter()
            .filter(|line| line.contains(&needle))
            .collect()
    }

    fn clamp(&mut self) {
        self.scroll_back = self.scroll_back.min(self.visible().len().saturating_sub(1));
        if self.scroll_back == 0 {
            self.unseen = 0;
        }
    }

    fn scroll(&mut self, delta: isize) {
        self.scroll_back = self.scroll_back.saturating_add_signed(delta);
        self.clamp();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    pub stderr: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunState {
    Running,
    Exited(i32),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct RunPanel {
    pub session: u64,
    pub target: ServiceTarget,
    /// The command line as typed.
    pub command: String,
    pub argv: Vec<String>,
    pub lines: VecDeque<OutputLine>,
    pub dropped: usize,
    pub state: RunState,
    pub scroll_back: usize,
    /// Output received after the last newline, per stream.
    partial: [String; 2],
}

impl RunPanel {
    #[must_use]
    pub fn new(session: u64, target: ServiceTarget, command: String, argv: Vec<String>) -> Self {
        Self {
            session,
            target,
            command,
            argv,
            lines: VecDeque::new(),
            dropped: 0,
            state: RunState::Running,
            scroll_back: 0,
            partial: [String::new(), String::new()],
        }
    }

    fn push_line(&mut self, stderr: bool, text: &str) {
        if self.lines.len() == MAXIMUM_LINES {
            self.lines.pop_front();
            self.dropped += 1;
        }
        if self.scroll_back > 0 {
            self.scroll_back += 1;
        }
        self.lines.push_back(OutputLine {
            stderr,
            text: strip_ansi(text.trim_end_matches('\r')),
        });
        self.scroll_back = self.scroll_back.min(self.lines.len().saturating_sub(1));
    }

    /// Adds a chunk of output; only complete lines are shown until the command ends.
    pub fn push(&mut self, stderr: bool, chunk: &str) {
        let mut pending = std::mem::take(&mut self.partial[usize::from(stderr)]);
        pending.push_str(chunk);
        let mut rest = pending.as_str();
        while let Some((line, tail)) = rest.split_once('\n') {
            self.push_line(stderr, line);
            rest = tail;
        }
        self.partial[usize::from(stderr)] = rest.to_owned();
    }

    pub fn finish(&mut self, result: Result<i32, String>) {
        for stderr in [false, true] {
            let rest = std::mem::take(&mut self.partial[usize::from(stderr)]);
            if !rest.is_empty() {
                self.push_line(stderr, &rest);
            }
        }
        self.state = match result {
            Ok(code) => RunState::Exited(code),
            Err(error) => RunState::Failed(error),
        };
    }

    fn scroll(&mut self, delta: isize) {
        self.scroll_back = self
            .scroll_back
            .saturating_add_signed(delta)
            .min(self.lines.len().saturating_sub(1));
    }
}

/// The command line being typed for a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPrompt {
    pub target: ServiceTarget,
    pub input: String,
    /// Position in the history while browsing it with the arrow keys.
    pub recalled: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Logs,
    Run,
    Restart,
    Variables,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub key: char,
    pub label: &'static str,
    pub detail: String,
    pub action: ServiceAction,
    /// Why the action is unavailable here.
    pub disabled: Option<String>,
}

/// The action menu for one service or container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceMenu {
    pub target: ServiceTarget,
    pub items: Vec<MenuItem>,
    pub selected: usize,
}

/// Builds the menu for the selected Services row, or `None` when nothing is selected.
pub(super) fn menu_for_selection(model: &Model) -> Option<ServiceMenu> {
    let target = model.selected_service_target()?;
    let replica = target.container.is_some();
    let items = [
        (
            'l',
            "Logs",
            match (&target.node, replica) {
                (_, true) => "follow this container".to_owned(),
                (Some(_), false) => "follow this node".to_owned(),
                (None, false) => "follow every replica".to_owned(),
            },
            ServiceAction::Logs,
        ),
        (
            'x',
            "Run a command",
            "no TTY; output is captured".to_owned(),
            ServiceAction::Run,
        ),
        (
            'r',
            "Restart",
            if replica {
                "this container only".to_owned()
            } else {
                "every container of the service".to_owned()
            },
            ServiceAction::Restart,
        ),
        (
            'v',
            "Variables",
            "what the service reads, and from where".to_owned(),
            ServiceAction::Variables,
        ),
    ]
    .into_iter()
    .map(|(key, label, detail, action)| MenuItem {
        key,
        label,
        detail,
        action,
        disabled: model.action_unavailable(action, &target),
    })
    .collect();
    Some(ServiceMenu {
        target,
        items,
        selected: 0,
    })
}

/// Starts `action` for `target`, or explains why it cannot run here.
pub(super) fn start(model: &mut Model, action: ServiceAction, target: ServiceTarget) -> Action {
    if let Some(reason) = model.action_unavailable(action, &target) {
        model.notice = Some(Notice::info(reason));
        return Action::None;
    }
    match action {
        ServiceAction::Logs => {
            let session = model.next_session();
            model.panel = Some(Panel::Logs(LogsPanel::new(session, target.clone())));
            Action::FollowLogs { session, target }
        }
        ServiceAction::Run => {
            let target = model.exec_target(target);
            model.run_prompt = Some(RunPrompt {
                target,
                input: String::new(),
                recalled: None,
                error: None,
            });
            Action::None
        }
        ServiceAction::Restart => {
            model.confirm = Some(restart_confirmation(model, target));
            Action::None
        }
        ServiceAction::Variables => {
            model.tab = Tab::Variables;
            if model.environment.is_none() {
                model.scroll = 0;
                model.env.filter = target.service;
                model.env.problems_only = false;
            } else if !model.show_service_variables(&target.stack, &target.service) {
                model.notice = Some(Notice::info(format!(
                    "{} declares no variables.",
                    target.service
                )));
            }
            Action::None
        }
    }
}

fn restart_confirmation(model: &Model, target: ServiceTarget) -> Confirm {
    let scope = if target.container.is_some() {
        format!("Restarts the container {}.", target.label)
    } else {
        let containers = model
            .services()
            .iter()
            .find(|service| service.stack == target.stack && service.name == target.service)
            .map_or(0, |service| super::service_nodes::current(service).count());
        match containers {
            0 | 1 => format!("Restarts the container of {}.", target.service),
            count => format!("Restarts all {count} containers of {}.", target.service),
        }
    };
    Confirm {
        title: format!("Restart {}?", target.label),
        lines: vec![
            scope,
            "Requests in flight are dropped.".to_owned(),
            "The image, configuration, and volumes stay as deployed.".to_owned(),
        ],
        accept: "restart",
        destructive: true,
        action: Action::Restart(target),
    }
}

/// Handles keys for a focused text field of a panel; `None` lets shortcuts handle it.
pub(super) fn typing_key(model: &mut Model, key: KeyEvent) -> Option<Action> {
    if model.run_prompt.is_some() {
        return Some(prompt_key(model, key));
    }
    if let Some(Panel::Logs(logs)) = &mut model.panel
        && logs.filtering
    {
        match key.code {
            KeyCode::Esc => {
                logs.filtering = false;
                logs.filter.clear();
            }
            KeyCode::Enter => logs.filtering = false,
            KeyCode::Backspace => {
                logs.filter.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                logs.filter.clear();
            }
            KeyCode::Char(character)
                if !character.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                logs.filter.push(character);
            }
            _ => {}
        }
        logs.scroll_back = 0;
        logs.unseen = 0;
        return Some(Action::None);
    }
    None
}

fn prompt_key(model: &mut Model, key: KeyEvent) -> Action {
    let Some(prompt) = model.run_prompt.as_mut() else {
        return Action::None;
    };
    prompt.error = None;
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => model.run_prompt = None,
        KeyCode::Enter => return submit_prompt(model),
        KeyCode::Backspace => {
            prompt.input.pop();
        }
        KeyCode::Char('u') if control => prompt.input.clear(),
        KeyCode::Up => {
            let next = prompt.recalled.map_or(0, |index| index + 1);
            if let Some(command) = model.run_history.iter().rev().nth(next) {
                prompt.input = command.clone();
                prompt.recalled = Some(next);
            }
        }
        KeyCode::Down => match prompt.recalled {
            Some(0) | None => {
                prompt.recalled = None;
                prompt.input.clear();
            }
            Some(index) => {
                if let Some(command) = model.run_history.iter().rev().nth(index - 1) {
                    prompt.input = command.clone();
                }
                prompt.recalled = Some(index - 1);
            }
        },
        KeyCode::Char(character)
            if !character.is_control()
                && !control
                && prompt.input.chars().count() < MAXIMUM_COMMAND_CHARS =>
        {
            prompt.input.push(character);
        }
        _ => {}
    }
    Action::None
}

fn submit_prompt(model: &mut Model) -> Action {
    let Some(prompt) = model.run_prompt.as_mut() else {
        return Action::None;
    };
    let argv = match split_command(&prompt.input) {
        Ok(argv) if argv.is_empty() => {
            prompt.error = Some("Type a command, for example: ls -la /app".to_owned());
            return Action::None;
        }
        Ok(argv) => argv,
        Err(message) => {
            prompt.error = Some(message);
            return Action::None;
        }
    };
    let Some(prompt) = model.run_prompt.take() else {
        return Action::None;
    };
    let command = prompt.input.trim().to_owned();
    model.run_history.retain(|previous| previous != &command);
    model.run_history.push(command.clone());
    if model.run_history.len() > MAXIMUM_HISTORY {
        model.run_history.remove(0);
    }
    run(model, prompt.target, command, argv)
}

fn run(model: &mut Model, target: ServiceTarget, command: String, argv: Vec<String>) -> Action {
    let session = model.next_session();
    model.panel = Some(Panel::Run(RunPanel::new(
        session,
        target.clone(),
        command,
        argv.clone(),
    )));
    Action::RunCommand {
        session,
        target,
        argv,
    }
}

/// Handles keys while the action menu is open.
pub(super) fn menu_key(model: &mut Model, code: KeyCode) -> Action {
    let Some(menu) = model.menu.as_mut() else {
        return Action::None;
    };
    let chosen = match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Left | KeyCode::Char('h') => {
            model.menu = None;
            return Action::None;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            menu.selected = (menu.selected + 1) % menu.items.len().max(1);
            return Action::None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            menu.selected = (menu.selected + menu.items.len().max(1) - 1) % menu.items.len().max(1);
            return Action::None;
        }
        KeyCode::Enter | KeyCode::Right => menu.items.get(menu.selected),
        KeyCode::Char(key) => menu.items.iter().find(|item| item.key == key),
        _ => None,
    };
    let Some(item) = chosen else {
        return Action::None;
    };
    let (action, target) = (item.action, menu.target.clone());
    model.menu = None;
    start(model, action, target)
}

/// Handles keys while a log or command panel is open.
pub(super) fn panel_key(model: &mut Model, code: KeyCode) -> Action {
    match &mut model.panel {
        Some(Panel::Logs(logs)) => match code {
            KeyCode::Esc | KeyCode::Char('q') => return close(model),
            KeyCode::Char('?') => model.help = true,
            KeyCode::Up | KeyCode::Char('k') => logs.scroll(1),
            KeyCode::Down | KeyCode::Char('j') => logs.scroll(-1),
            KeyCode::PageUp => logs.scroll(PAGE.cast_signed()),
            KeyCode::PageDown => logs.scroll(-PAGE.cast_signed()),
            KeyCode::Home | KeyCode::Char('g') => logs.scroll(isize::MAX),
            KeyCode::End | KeyCode::Char('G' | 'f') => logs.scroll(isize::MIN),
            KeyCode::Char(' ') => logs.scroll(if logs.scroll_back == 0 { 1 } else { isize::MIN }),
            KeyCode::Char('w') => logs.wrap = !logs.wrap,
            KeyCode::Char('t') => logs.timestamps = !logs.timestamps,
            KeyCode::Char('/') => logs.filtering = true,
            KeyCode::Char('c') => {
                logs.lines.clear();
                logs.dropped = 0;
                logs.scroll_back = 0;
                logs.unseen = 0;
            }
            KeyCode::Char('r') if !matches!(logs.state, StreamState::Streaming) => {
                let target = logs.target.clone();
                return start(model, ServiceAction::Logs, target);
            }
            _ => {}
        },
        Some(Panel::Run(run)) => match code {
            KeyCode::Esc | KeyCode::Char('q') => return close(model),
            KeyCode::Char('?') => model.help = true,
            KeyCode::Up | KeyCode::Char('k') => run.scroll(1),
            KeyCode::Down | KeyCode::Char('j') => run.scroll(-1),
            KeyCode::PageUp => run.scroll(PAGE.cast_signed()),
            KeyCode::PageDown => run.scroll(-PAGE.cast_signed()),
            KeyCode::Home | KeyCode::Char('g') => run.scroll(isize::MAX),
            KeyCode::End | KeyCode::Char('G') => run.scroll(isize::MIN),
            KeyCode::Char('r') if run.state != RunState::Running => {
                let (target, command, argv) =
                    (run.target.clone(), run.command.clone(), run.argv.clone());
                return self::run(model, target, command, argv);
            }
            KeyCode::Enter | KeyCode::Char('x') if run.state != RunState::Running => {
                model.run_prompt = Some(RunPrompt {
                    target: run.target.clone(),
                    input: String::new(),
                    recalled: None,
                    error: None,
                });
            }
            _ => {}
        },
        None => {}
    }
    Action::None
}

fn close(model: &mut Model) -> Action {
    model.panel = None;
    Action::CloseStream
}

/// Splits a command line into arguments: whitespace separates, single and double
/// quotes group, and a backslash keeps the next character. No shell runs, so
/// pipes and variables need an explicit `sh -c '...'`.
pub(super) fn split_command(line: &str) -> Result<Vec<String>, String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote = None;
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        match (quote, character) {
            (Some(open), close) if open == close => quote = None,
            (Some('"') | None, '\\') => match characters.next() {
                Some(next) => {
                    current.push(next);
                    started = true;
                }
                None => return Err("The command ends with an unfinished \\ escape.".to_owned()),
            },
            (Some(_), other) => current.push(other),
            (None, '\'' | '"') => {
                quote = Some(character);
                started = true;
            }
            (None, space) if space.is_whitespace() => {
                if started {
                    arguments.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, other) => {
                current.push(other);
                started = true;
            }
        }
    }
    if let Some(open) = quote {
        return Err(format!("The command has an unclosed {open} quote."));
    }
    if started {
        arguments.push(current);
    }
    Ok(arguments)
}

/// Removes terminal escape sequences so colored output reads as plain text.
pub(super) fn strip_ansi(text: &str) -> String {
    if !text.contains('\u{1b}') {
        return text.to_owned();
    }
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            plain.push(character);
            continue;
        }
        match characters.peek() {
            // CSI: parameters and intermediates, then one final byte.
            Some('[') => {
                characters.next();
                for next in characters.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            // OSC: ends with BEL or ESC \.
            Some(']') => {
                characters.next();
                while let Some(next) = characters.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' {
                        characters.next_if_eq(&'\\');
                        break;
                    }
                }
            }
            Some(_) => {
                characters.next();
            }
            None => {}
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> ServiceTarget {
        ServiceTarget {
            stack: "app".to_owned(),
            service: "api".to_owned(),
            container: None,
            label: "api".to_owned(),
            node: None,
        }
    }

    fn line(text: &str) -> LogLine {
        LogLine {
            text: text.to_owned(),
            ..LogLine::default()
        }
    }

    #[test]
    fn commands_split_like_a_shell_without_running_one() {
        assert_eq!(
            split_command("  ls -la   /app "),
            Ok(vec!["ls".to_owned(), "-la".to_owned(), "/app".to_owned()])
        );
        assert_eq!(
            split_command(r#"sh -c 'echo "a b" | wc -c' "" x\ y "q\"r""#),
            Ok(vec![
                "sh".to_owned(),
                "-c".to_owned(),
                r#"echo "a b" | wc -c"#.to_owned(),
                String::new(),
                "x y".to_owned(),
                "q\"r".to_owned(),
            ])
        );
        assert_eq!(split_command("   "), Ok(Vec::new()));
        assert!(split_command("echo 'open").is_err());
        assert!(split_command("echo \\").is_err());
    }

    #[test]
    fn escape_sequences_are_removed_from_output() {
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\u{1b}[1;32mok\u{1b}[0m done"), "ok done");
        assert_eq!(
            strip_ansi("\u{1b}]0;title\u{7}text\u{1b}]8;;x\u{1b}\\!"),
            "text!"
        );
        assert_eq!(strip_ansi("cut\u{1b}"), "cut");
    }

    #[test]
    fn logs_follow_until_scrolled_back_and_count_what_arrives() {
        let mut logs = LogsPanel::new(1, target());
        for index in 0..5 {
            logs.push(line(&format!("line {index}")));
        }
        assert_eq!(logs.scroll_back, 0);
        logs.scroll(2);
        logs.push(line("line 5"));
        assert_eq!(
            (logs.scroll_back, logs.unseen),
            (3, 1),
            "the view stays put"
        );
        logs.scroll(isize::MIN);
        assert_eq!((logs.scroll_back, logs.unseen), (0, 0));

        logs.filter = "LINE 1".to_owned();
        assert_eq!(logs.visible().len(), 1);
        logs.scroll(isize::MAX);
        assert_eq!(logs.scroll_back, 0, "one match cannot scroll");

        logs.filter.clear();
        for index in 0..MAXIMUM_LINES {
            logs.push(line(&format!("bulk {index}")));
        }
        assert_eq!(logs.lines.len(), MAXIMUM_LINES);
        assert_eq!(logs.dropped, 6);
    }

    #[test]
    fn command_output_is_shown_by_line_and_flushed_at_exit() {
        let mut run = RunPanel::new(1, target(), "ls".to_owned(), vec!["ls".to_owned()]);
        run.push(false, "one\ntw");
        run.push(true, "warn\r\n");
        run.push(false, "o\nthree");
        let text = |run: &RunPanel| {
            run.lines
                .iter()
                .map(|line| (line.stderr, line.text.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            text(&run),
            [
                (false, "one".to_owned()),
                (true, "warn".to_owned()),
                (false, "two".to_owned())
            ]
        );
        run.finish(Ok(3));
        assert_eq!(run.lines.len(), 4);
        assert_eq!(run.state, RunState::Exited(3));
    }
}
