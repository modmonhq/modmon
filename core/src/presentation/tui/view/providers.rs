use super::*;
use crate::presentation::tui::provider::{
    DataPathForm, ExternalSetupForm, ProviderModel, ProviderReview, ProviderScreen,
};

pub(super) fn render_providers(
    frame: &mut Frame<'_>,
    area: Rect,
    providers: &ProviderModel,
    tick: u64,
) {
    let theme = theme();
    match &providers.screen {
        ProviderScreen::Dashboard => render_provider_dashboard(frame, area, providers),
        ProviderScreen::ExternalSetup(form) => render_external_setup(frame, area, form),
        ProviderScreen::DataPath(form) => render_data_path(frame, area, form),
        ProviderScreen::Review(review) => render_provider_review(frame, area, review),
        ProviderScreen::Applying(message) => result_panel(
            frame,
            area,
            "Secret backend",
            Tone::Info,
            vec![Span::styled(
                format!("{} {}", spinner(tick), terminal_safe(message)),
                theme.tone(Tone::Info),
            )],
            "",
            &[],
        ),
        ProviderScreen::Complete(message) => result_panel(
            frame,
            area,
            "Secret backend",
            Tone::Ok,
            badge(Tone::Ok, "Operation complete"),
            message,
            &[("enter", "back to providers")],
        ),
        ProviderScreen::Error(message) => {
            let keys: &[(&str, &str)] = if providers.last_review.is_some() {
                &[("r", "retry the exact review"), ("b", "back")]
            } else {
                &[("b", "back; fix the reported input first")]
            };
            result_panel(
                frame,
                area,
                "Secret backend",
                Tone::Bad,
                badge(Tone::Bad, "Operation failed"),
                message,
                keys,
            );
        }
        ProviderScreen::Cancelled => result_panel(
            frame,
            area,
            "Secret backend",
            Tone::Idle,
            badge(Tone::Idle, "Operation cancelled"),
            "Cancelled before the next change was made.",
            &[("r", "resume the exact review"), ("b", "back")],
        ),
    }
}

fn render_provider_dashboard(frame: &mut Frame<'_>, area: Rect, providers: &ProviderModel) {
    let managed = managed_provider_lines(providers);
    let managed_height = u16::try_from(managed.len() + 2).unwrap_or(u16::MAX);
    let [top, _, bottom] = Layout::vertical([
        Constraint::Length(managed_height),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(managed)
            .block(card("Managed provider", false))
            .wrap(Wrap { trim: false }),
        top,
    );
    render_connections(frame, bottom, providers);
}

/// Release, provider, operations, and health of the selected managed provider.
fn managed_provider_lines(providers: &ProviderModel) -> Vec<Line<'_>> {
    let theme = theme();
    let width = 12;
    let mut managed = vec![kv(
        "Release",
        vec![
            Span::styled(terminal_safe(&providers.managed_release), theme.text()),
            Span::styled("   ", theme.muted()),
            Span::styled("[ ]", theme.key()),
            Span::styled(" change", theme.muted()),
        ],
        width,
    )];
    if let Some(scope) = providers
        .context
        .managed_connections
        .get(providers.selected_managed)
    {
        managed.push(kv_text(
            "Provider",
            &format!("{} · {}", scope.name, scope.instance.driver_display_name),
            width,
        ));
        managed.push(kv_text(
            "Operations",
            &scope
                .instance
                .operations
                .iter()
                .map(|operation| format!("{operation:?}").to_lowercase())
                .collect::<Vec<_>>()
                .join(", "),
            width,
        ));
    }
    match &providers.managed_status {
        Some(status) => {
            let mut state = badge(
                if status.installed {
                    Tone::Ok
                } else {
                    Tone::Idle
                },
                if status.installed {
                    "installed"
                } else {
                    "not installed"
                },
            );
            if let Some(health) = &status.health {
                state.push(Span::raw("   "));
                state.extend(health_badges(health.healthy, health.sealed));
                state.push(Span::styled(
                    format!("   {}", terminal_safe(&health.version)),
                    theme.muted(),
                ));
            }
            managed.push(kv("Status", state, width));
            managed.push(kv_text(
                "Recovery",
                if status.recovery_available_off_instance {
                    "available off-instance"
                } else {
                    "not available off-instance"
                },
                width,
            ));
            for diagnostic in &status.diagnostics {
                managed.push(Line::from(badge(Tone::Bad, terminal_safe(diagnostic))));
            }
        }
        None => managed.push(kv(
            "Status",
            vec![
                Span::styled("s", theme.key()),
                Span::styled(" check health, seal, and recovery", theme.muted()),
            ],
            width,
        )),
    }
    managed
}

/// External OpenBao and Vault connections plus secrets that can be rotated.
fn render_connections(frame: &mut Frame<'_>, bottom: Rect, providers: &ProviderModel) {
    let theme = theme();
    let actions = Line::from(hints(&[
        ("x", "external setup"),
        ("t", "test"),
        ("p", "managed"),
        ("s", "status"),
        ("v", "rotate"),
        ("g", "migrate"),
    ]));
    let block = card("Connections", true).title_bottom(actions.centered());
    let external = &providers.context.external_connections;
    let secrets = &providers.context.rotatable_secrets;
    if external.is_empty() && secrets.is_empty() {
        empty_state(
            frame,
            bottom,
            block,
            Tone::Idle,
            "No OpenBao or Vault connections",
            "Declare storage metadata in mm.yaml first. Credentials never belong there.",
        );
        return;
    }
    let mut rows = Vec::new();
    for connection in external {
        let status = providers
            .remote_statuses
            .iter()
            .find(|status| status.instance == connection.instance.name);
        let health = status.map_or_else(
            || {
                vec![Span::styled(
                    format!(
                        "{} · not tested",
                        plural(connection.locations.len(), "location")
                    ),
                    theme.muted(),
                )]
            },
            |status| {
                let mut spans = health_badges(status.health.healthy, status.health.sealed);
                spans.push(Span::raw("   "));
                spans.extend(if status.authenticated {
                    badge(Tone::Ok, "authenticated")
                } else {
                    badge(Tone::Bad, "not authenticated")
                });
                spans
            },
        );
        rows.push(Row::new([
            Cell::from(Span::styled("external", theme.muted())),
            Cell::from(Span::styled(
                terminal_safe(&connection.name),
                theme.strong(),
            )),
            Cell::from(Line::from(health)),
        ]));
    }
    for secret in secrets {
        rows.push(Row::new([
            Cell::from(Span::styled("secret", theme.muted())),
            Cell::from(Span::styled(terminal_safe(secret), theme.strong())),
            Cell::from(Span::styled(
                "metadata only · v plans a rotation",
                theme.muted(),
            )),
        ]));
    }
    let count = rows.len();
    render_table(
        frame,
        bottom,
        Table::new(
            rows,
            [
                Constraint::Length(9),
                Constraint::Percentage(35),
                Constraint::Min(20),
            ],
        )
        .header(table_header(&["KIND", "NAME", "STATUS"]))
        .block(block),
        count,
        Some(providers.selected),
    );
}

fn health_badges(healthy: bool, sealed: bool) -> Vec<Span<'static>> {
    let mut spans = if healthy {
        badge(Tone::Ok, "healthy")
    } else {
        badge(Tone::Bad, "unhealthy")
    };
    spans.push(Span::raw("   "));
    spans.extend(if sealed {
        badge(Tone::Warn, "sealed")
    } else {
        badge(Tone::Ok, "unsealed")
    });
    spans
}

fn render_external_setup(frame: &mut Frame<'_>, area: Rect, form: &ExternalSetupForm) {
    let theme = theme();
    let fields = form.displayed_fields();
    let width = fields
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0)
        .max(16)
        + 2;
    let mut lines = vec![
        Line::styled(
            "Public connection metadata only. The developer token is requested after review.",
            theme.muted(),
        ),
        Line::raw(""),
    ];
    for (index, (label, value)) in fields.into_iter().enumerate() {
        lines.push(field(label, value, form.field == index, width));
    }
    let focused = form.field == form.fields.len();
    lines.push(Line::from(vec![
        Span::styled(if focused { "▌ " } else { "  " }, theme.accent()),
        Span::styled(
            format!("{:<width$}", "Bootstrap mounts"),
            if focused {
                theme.strong()
            } else {
                theme.muted()
            },
        ),
        Span::styled(
            if form.allow_bootstrap {
                "[x] yes"
            } else {
                "[ ] no"
            },
            if form.allow_bootstrap {
                theme.accent()
            } else {
                theme.soft()
            },
        ),
    ]));
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                format!("Connect {}", terminal_safe(form.instance.as_str())),
                true,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_data_path(frame: &mut Frame<'_>, area: Rect, form: &DataPathForm) {
    frame.render_widget(
        Paragraph::new(vec![
            kv_text("Provider", form.instance.as_str(), 12),
            kv_text("Operation", form.action.as_str(), 12),
            Line::raw(""),
            field("Path", &form.path, true, 10),
        ])
        .block(card("Managed data operation", true))
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_provider_review(frame: &mut Frame<'_>, area: Rect, review: &ProviderReview) {
    let theme = theme();
    let mut lines = vec![
        Line::styled(terminal_safe(&review.title()), theme.strong()),
        Line::raw(""),
        kv(
            "Plan hash",
            vec![Span::styled(review.plan_hash().to_owned(), theme.key())],
            11,
        ),
        Line::raw(""),
        section("What will change"),
    ];
    for consequence in review.consequences() {
        lines.push(Line::from(vec![
            Span::styled("  → ", theme.tone(Tone::Warn)),
            Span::styled(terminal_safe(&consequence), theme.text()),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "No credential or secret value is held on this screen.",
        theme.muted(),
    ));
    lines.push(Line::raw(""));
    lines.push(Line::from(hints(&[
        ("enter", "apply"),
        ("b", "back"),
        ("c/esc", "cancel"),
    ])));
    frame.render_widget(
        Paragraph::new(lines)
            .block(card("Review", true).border_style(theme.tone(Tone::Warn)))
            .wrap(Wrap { trim: false }),
        area,
    );
}
