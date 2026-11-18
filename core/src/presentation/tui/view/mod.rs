mod access;
use access::*;
mod chrome;
use chrome::*;
mod cluster;
use cluster::*;
mod editor;
use editor::*;
mod environment;
use environment::*;
mod environments;
use environments::*;
mod gates;
use gates::*;
mod host_values;
use host_values::*;
mod guided;
use guided::*;
mod ingress;
use ingress::*;
mod loading;
use loading::*;
mod log_lines;
use log_lines::*;
mod mounts;
use mounts::*;
mod optional;
use optional::*;
mod overview;
use overview::*;
mod panels;
use panels::*;
mod providers;
use providers::*;
mod review;
use review::*;
mod services;
use services::*;
mod setup;
use setup::*;
mod theme;
use theme::*;
mod tls;
use tls::*;
mod widgets;
pub use theme::Tone;
pub(crate) use theme::{ColorDepth, detect_depth};
pub(crate) use widgets::SPINNER;
use widgets::*;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap};

use crate::domain::diagnostic::Severity;
use crate::domain::environment::{ResolutionStatus, ValueSource};
use crate::domain::runtime::{RuntimeKind, ServiceState};
use crate::domain::service_stack::ServiceComponentKind;
use crate::presentation::output::terminal_safe;
use crate::presentation::tui::model::{LiveState, Model, Tab};

/// Below this width the sidebar collapses into a one-line tab strip.
const SIDEBAR_BREAKPOINT: u16 = 100;
const SIDEBAR_WIDTH: u16 = 26;

pub(super) fn render(frame: &mut Frame<'_>, model: &Model) {
    if frame.area().width < 40 || frame.area().height < 10 {
        render_small_terminal(frame);
        return;
    }
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_header(frame, header, model);
    if let Some(review) = model.review.as_ref().filter(|review| review.shown) {
        render_deploy_review(frame, body, model, review);
    } else if let Some(panel) = &model.panel {
        // Output needs the width: the panel covers the sidebar too.
        render_panel(frame, body, model, panel);
    } else {
        render_view(frame, body, model);
    }
    render_footer(frame, footer, model);
    if let Some(edit) = model
        .env
        .edit
        .as_ref()
        .filter(|_| model.tab == Tab::Variables && model.panel.is_none())
    {
        render_environment_edit(frame, edit, model.remote().is_some());
    }
    if let Some(prompt) = &model.run_prompt {
        render_run_prompt(frame, prompt);
    }
    if let Some(environment) = &model.environments.switching {
        render_switching(frame, model, environment);
    }
    if let Some(editor) = &model.editor {
        render_editor(frame, editor);
    }
    if let Some(confirm) = &model.confirm {
        render_confirm(frame, confirm);
    } else if model.help {
        render_help(frame, model);
    }
}

fn render_view(frame: &mut Frame<'_>, body: Rect, model: &Model) {
    let content = if body.width >= SIDEBAR_BREAKPOINT {
        let [sidebar, _, content, _] = Layout::horizontal([
            Constraint::Length(SIDEBAR_WIDTH),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(body);
        render_sidebar(frame, sidebar, model);
        content
    } else {
        let [strip, content] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(body);
        render_tab_strip(frame, strip, model);
        content
    };
    if let Some(part) = waits(model) {
        render_reading(frame, content, model, part);
        return;
    }
    match model.tab {
        Tab::Overview => render_overview(frame, content, model),
        Tab::Environments => render_environments(frame, content, model),
        Tab::Services => render_services(frame, content, model),
        Tab::Ingress => render_ingress(frame, content, model),
        Tab::Variables => render_environment(frame, content, model),
        Tab::Mounts => render_mounts(frame, content, model),
        Tab::Diagnostics => render_diagnostics(frame, content, model),
        Tab::Tls => render_tls(frame, content, model),
        Tab::Access => render_access(frame, content, model),
        Tab::Gates => render_gates(frame, content, &model.gates, model.tick),
        Tab::Providers => render_providers(frame, content, &model.providers, model.tick),
        Tab::Jobs => render_jobs(frame, content, model),
        Tab::Guided => render_guided(frame, content, model),
    }
}

pub fn render_secret_prompt(
    frame: &mut Frame<'_>,
    model: &Model,
    prompt: &str,
    character_count: usize,
) {
    render(frame, model);
    render_secret_prompt_overlay(frame, prompt, character_count);
}

pub(super) fn render_secret_prompt_overlay(
    frame: &mut Frame<'_>,
    prompt: &str,
    character_count: usize,
) {
    let theme = theme();
    let area = centered_rect(64, 9, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(terminal_safe(prompt), theme.strong()),
            Line::styled("Value is never stored.", theme.muted()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("•".repeat(character_count.min(24)), theme.accent()),
                Span::styled("█", theme.accent()),
                Span::styled(format!("  {character_count} chars"), theme.muted()),
            ]),
            Line::raw(""),
            Line::from(hints(&[("enter", "submit"), ("esc", "cancel")])),
        ])
        .block(card("Hidden input", true))
        .wrap(Wrap { trim: false }),
        area,
    );
}

pub(super) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

/// Renders a selectable table with its scrollbar, keeping the selection in view.
/// Returns the index of the first row shown.
pub(super) fn render_table(
    frame: &mut Frame<'_>,
    area: Rect,
    table: Table<'_>,
    rows: usize,
    selected: Option<usize>,
) -> usize {
    let mut state = TableState::default().with_selected(selected.filter(|_| rows > 0));
    frame.render_stateful_widget(selectable_table(table), area, &mut state);
    scrollbar(frame, area, rows, state.offset());
    state.offset()
}

const fn runtime_label(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::PodmanCompose => "podman compose",
        RuntimeKind::DockerCompose => "docker compose",
    }
}

const fn component_label(kind: ServiceComponentKind) -> &'static str {
    match kind {
        ServiceComponentKind::LocalIngress => "ingress",
        ServiceComponentKind::MmControlApi => "control api",
        ServiceComponentKind::Mailpit => "mailpit",
        ServiceComponentKind::Cap => "cap",
        ServiceComponentKind::CapValkey => "cap valkey",
    }
}

const fn service_tone(state: ServiceState) -> Tone {
    match state {
        ServiceState::Running | ServiceState::JobOnly => Tone::Ok,
        ServiceState::Degraded => Tone::Warn,
        ServiceState::Stopped => Tone::Bad,
        ServiceState::Missing | ServiceState::Unknown => Tone::Idle,
    }
}

const fn severity_tone(severity: Severity) -> Tone {
    match severity {
        Severity::Info => Tone::Info,
        Severity::Warning => Tone::Warn,
        Severity::Error => Tone::Bad,
    }
}

const fn environment_source(source: ValueSource) -> &'static str {
    crate::presentation::tui::model::source_label(source)
}

const fn environment_status(status: ResolutionStatus) -> &'static str {
    crate::presentation::tui::model::status_label(status)
}

const fn environment_tone(status: ResolutionStatus) -> Tone {
    match status {
        ResolutionStatus::Resolved => Tone::Ok,
        ResolutionStatus::Defaulted => Tone::Info,
        ResolutionStatus::NotChecked => Tone::Idle,
        ResolutionStatus::Unset
        | ResolutionStatus::ProviderUnavailable
        | ResolutionStatus::AccessDenied
        | ResolutionStatus::Invalid => Tone::Bad,
    }
}

#[cfg(test)]
mod tests;
