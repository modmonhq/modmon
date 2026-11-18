use super::*;
use crate::domain::gate::GatePolicyMode;
use crate::presentation::tui::gate::{GateModel, GateScreen};

pub(super) fn render_gates(frame: &mut Frame<'_>, area: Rect, model: &GateModel, tick: u64) {
    let panel_height = match model.screen {
        GateScreen::List => 0,
        GateScreen::Review { .. } => 7,
        _ => 5,
    };
    let [list, panel] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(panel_height)]).areas(area);
    render_policies(frame, list, model);
    render_panel(frame, panel, model, tick);
}

/// The policy table, or an empty state that says how to add the first one.
fn render_policies(frame: &mut Frame<'_>, list: Rect, model: &GateModel) {
    let theme = theme();
    let block = with_meta(
        card("Access gates", matches!(model.screen, GateScreen::List)),
        vec![Span::styled(
            match model.policies.len() {
                1 => "1 policy".to_owned(),
                count => format!("{count} policies"),
            },
            theme.muted(),
        )],
    );
    if model.policies.is_empty() {
        empty_state(
            frame,
            list,
            block,
            Tone::Idle,
            "No access policies",
            "Prepare services with `mm dev services apply --gate`, then run `mm access add`.",
        );
    } else {
        let rows = model.policies.iter().map(|policy| {
            Row::new([
                Cell::from(Span::styled(
                    terminal_safe(policy.name.as_str()),
                    theme.strong(),
                )),
                Cell::from(Span::styled(
                    match policy.mode {
                        GatePolicyMode::Any => "any driver",
                        GatePolicyMode::All => "all drivers",
                    },
                    theme.soft(),
                )),
                Cell::from(Span::styled(
                    format!("{}d", policy.session_ttl_seconds / 86_400),
                    theme.soft(),
                )),
                Cell::from(Span::styled(
                    format!("rev {}", policy.revision),
                    theme.muted(),
                )),
            ])
        });
        render_table(
            frame,
            list,
            Table::new(
                rows,
                [
                    Constraint::Min(16),
                    Constraint::Length(12),
                    Constraint::Length(8),
                    Constraint::Length(8),
                ],
            )
            .header(table_header(&["POLICY", "MODE", "SESSION", "REVISION"]))
            .block(block),
            model.policies.len(),
            Some(model.selected),
        );
    }
}

/// The add-user flow under the table: username, review, then the outcome.
fn render_panel(frame: &mut Frame<'_>, panel: Rect, model: &GateModel, tick: u64) {
    let theme = theme();
    let policy = model
        .policies
        .get(model.selected)
        .map_or_else(String::new, |policy| terminal_safe(policy.name.as_str()));
    match &model.screen {
        GateScreen::List => {}
        GateScreen::Username(username) => frame.render_widget(
            Paragraph::new(vec![
                field("Username", username, true, 10),
                Line::styled(
                    "  The password is requested after you review the plan.",
                    theme.muted(),
                ),
            ])
            .block(card(format!("Add user to {policy}"), true)),
            panel,
        ),
        GateScreen::Review { username, plan, .. } => frame.render_widget(
            Paragraph::new(vec![
                kv_text("User", username, 11),
                kv(
                    "Plan hash",
                    vec![Span::styled(plan.plan_hash.clone(), theme.key())],
                    11,
                ),
                Line::raw(""),
                Line::from(hints(&[
                    ("enter", "enter password and apply"),
                    ("esc", "cancel"),
                ])),
            ])
            .block(card("Review", true).border_style(theme.tone(Tone::Warn))),
            panel,
        ),
        GateScreen::Applying => result_panel(
            frame,
            panel,
            "Applying",
            Tone::Info,
            vec![Span::styled(
                format!("{} Applying reviewed gate user plan…", spinner(tick)),
                theme.tone(Tone::Info),
            )],
            "",
            &[],
        ),
        GateScreen::Complete(message) => result_panel(
            frame,
            panel,
            "Done",
            Tone::Ok,
            badge(Tone::Ok, terminal_safe(message)),
            "",
            &[],
        ),
        GateScreen::Error(message) => result_panel(
            frame,
            panel,
            "Failed",
            Tone::Bad,
            badge(Tone::Bad, terminal_safe(message)),
            "",
            &[],
        ),
        GateScreen::Cancelled => result_panel(
            frame,
            panel,
            "Cancelled",
            Tone::Idle,
            badge(
                Tone::Idle,
                "Gate user change cancelled; no secret was kept.",
            ),
            "",
            &[],
        ),
    }
}
