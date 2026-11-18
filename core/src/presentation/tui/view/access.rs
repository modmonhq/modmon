use super::*;
use crate::domain::scheduler::JobRunState;
use crate::presentation::tui::model::JobsPane;

pub(super) fn render_access(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let access = &model.access;
    let width = 13;
    let mut lines = vec![
        kv_text("Environment", &model.snapshot.environment, width),
        kv(
            "Runs on",
            match &access.remote_target {
                Some(target) => badge(Tone::Info, format!("remote · {}", terminal_safe(target))),
                None => badge(Tone::Idle, "local"),
            },
            width,
        ),
        kv_text(
            "Your key",
            access
                .device_key
                .as_deref()
                .unwrap_or("none yet — j creates one and requests access"),
            width,
        ),
    ];
    lines.extend(identity_lines(access, width));
    let asking = crate::presentation::tui::loading::Part::Identity;
    if access.remote_target.is_some()
        && access.identity.is_none()
        && model.loading.is_pending(asking)
    {
        lines.push(kv("You are", reading_line(model, asking).spans, width));
    }
    if !access.forwards.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Saved forwards"));
        for forward in &access.forwards {
            lines.push(Line::styled(
                format!("  {}", terminal_safe(forward)),
                theme.text(),
            ));
        }
        lines.push(Line::styled(
            "  `mm forward NAME` starts one; `mm forward up` starts every auto forward.",
            theme.muted(),
        ));
    }
    let summary = u16::try_from(lines.len() + 2)
        .unwrap_or(u16::MAX)
        .min(area.height / 2);
    let [top, team] =
        Layout::vertical([Constraint::Length(summary), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(card("Access", false))
            .wrap(Wrap { trim: false }),
        top,
    );
    render_team(frame, team, model);
}

/// The team in the users file. The selection is who `e` and `D` act on.
fn render_team(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let access = &model.access;
    let listed = access.team.iter().any(|member| member.you);
    let mut meta = vec![Span::styled(
        plural(access.team.len(), "member"),
        theme.muted(),
    )];
    if !listed {
        meta.push(Span::raw("   "));
        meta.extend(hints(&[("j", "request access for this device")]));
    }
    let block = with_meta(card("Team", true), meta);
    if access.team.is_empty() {
        empty_state(
            frame,
            area,
            block,
            Tone::Idle,
            "Nobody is in the users file yet",
            "Press a to add a teammate, or j to request access for this device.",
        );
        return;
    }
    let rows = access.team.iter().map(|member| {
        let mut name = vec![Span::styled(terminal_safe(&member.name), theme.strong())];
        if member.you {
            name.push(Span::styled("  you", theme.accent()));
        }
        Row::new([
            Cell::from(Line::from(name)),
            Cell::from(match &member.access {
                Some(access) => Span::styled(terminal_safe(access), theme.text()),
                None => Span::styled("no access here", theme.muted()),
            }),
            Cell::from(Span::styled(
                format!(
                    "{}{}",
                    plural(member.keys, "key"),
                    if member.machine { " · automation" } else { "" }
                ),
                theme.muted(),
            )),
        ])
    });
    render_table(
        frame,
        area,
        Table::new(
            rows,
            [
                Constraint::Min(16),
                Constraint::Min(22),
                Constraint::Length(20),
            ],
        )
        .header(table_header(&["NAME", "ACCESS HERE", "KEYS"]))
        .block(block),
        access.team.len(),
        Some(model.scroll),
    );
}

/// What the host enforces for you, for a remote environment.
fn identity_lines(
    access: &crate::presentation::tui::model::AccessView,
    width: usize,
) -> Vec<Line<'static>> {
    let theme = theme();
    let mut lines = Vec::new();
    if access.remote_target.is_some() {
        match (&access.identity, &access.identity_error) {
            (Some(identity), _) => {
                let summary = identity
                    .access
                    .as_ref()
                    .map_or_else(|| "no access here".to_owned(), |access| access.summary());
                lines.push(kv(
                    "You are",
                    vec![
                        Span::styled(terminal_safe(&identity.principal), theme.strong()),
                        Span::styled(format!("  {}", terminal_safe(&summary)), theme.text()),
                    ],
                    width,
                ));
                lines.push(kv_text(
                    "Enforcing",
                    &format!(
                        "users file from {}",
                        identity
                            .access_commit
                            .as_deref()
                            .map_or("no commit yet", short_commit)
                    ),
                    width,
                ));
                lines.push(kv_text(
                    "Deployed",
                    identity
                        .deployed_commit
                        .as_deref()
                        .map_or("nothing yet", short_commit),
                    width,
                ));
            }
            (None, Some(error)) => lines.push(Line::from(badge(
                Tone::Warn,
                format!("host: {}", terminal_safe(error)),
            ))),
            (None, None) => {}
        }
    }
    lines
}

pub(super) fn short_commit(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

const fn run_state(state: JobRunState) -> (&'static str, Tone) {
    match state {
        JobRunState::Queued => ("queued", Tone::Info),
        JobRunState::Launching => ("launching", Tone::Info),
        JobRunState::Running => ("running", Tone::Info),
        JobRunState::RetryWait => ("retry wait", Tone::Warn),
        JobRunState::Cancelling => ("cancelling", Tone::Warn),
        JobRunState::Succeeded => ("succeeded", Tone::Ok),
        JobRunState::Failed => ("failed", Tone::Bad),
        JobRunState::TimedOut => ("timed out", Tone::Bad),
        JobRunState::Cancelled => ("cancelled", Tone::Idle),
        JobRunState::Unknown => ("unknown", Tone::Idle),
    }
}

fn next_due(unix_ms: Option<i64>) -> String {
    unix_ms
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map_or_else(
            || "—".to_owned(),
            |time| {
                time.with_timezone(&chrono::Local)
                    .format("%b %d %H:%M")
                    .to_string()
            },
        )
}

pub(super) fn render_jobs(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let [panes, message] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(u16::from(model.scheduler_message.is_some()) * 3),
    ])
    .areas(area);
    let areas = if panes.width >= 90 {
        Layout::horizontal([Constraint::Ratio(1, 3); 3])
            .spacing(1)
            .split(panes)
    } else {
        Layout::vertical([Constraint::Ratio(1, 3); 3]).split(panes)
    };
    render_job_definitions(frame, areas[0], model);
    render_schedules(frame, areas[1], model);
    render_runs(frame, areas[2], model);
    if let Some(text) = &model.scheduler_message {
        frame.render_widget(
            Paragraph::new(Line::from(badge(Tone::Info, terminal_safe(text))))
                .block(card("Scheduler", false))
                .wrap(Wrap { trim: true }),
            message,
        );
    }
}

fn pane_selection(model: &Model, pane: JobsPane) -> Option<usize> {
    (model.jobs_pane == pane).then_some(model.scroll)
}

fn render_job_definitions(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let focused = model.jobs_pane == JobsPane::Jobs;
    let block = card("Jobs", focused);
    if model.jobs.jobs.is_empty() {
        empty_state(frame, area, block, Tone::Idle, "No jobs defined", "");
        return;
    }
    let rows = model.jobs.jobs.iter().map(|(name, job)| {
        Row::new([
            Cell::from(Span::styled(terminal_safe(name), theme.strong())),
            Cell::from(Span::styled(
                format!(
                    "{}/{}",
                    terminal_safe(&job.module),
                    terminal_safe(&job.service)
                ),
                theme.muted(),
            )),
        ])
    });
    render_table(
        frame,
        area,
        Table::new(
            rows,
            [Constraint::Percentage(50), Constraint::Percentage(50)],
        )
        .block(block),
        model.jobs.jobs.len(),
        pane_selection(model, JobsPane::Jobs),
    );
}

fn render_schedules(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let block = card("Schedules", model.jobs_pane == JobsPane::Schedules);
    if model.schedules.is_empty() {
        empty_state(frame, area, block, Tone::Idle, "No schedules", "");
        return;
    }
    let rows = model.schedules.iter().map(|schedule| {
        let state = if schedule.retired {
            badge(Tone::Idle, "retired")
        } else if schedule.enabled {
            badge(Tone::Ok, "on")
        } else {
            badge(Tone::Idle, "off")
        };
        Row::new([
            Cell::from(Line::from(state)),
            Cell::from(Span::styled(terminal_safe(&schedule.name), theme.strong())),
            Cell::from(Span::styled(
                next_due(schedule.next_due_at_unix_ms),
                theme.muted(),
            )),
        ])
    });
    render_table(
        frame,
        area,
        Table::new(
            rows,
            [
                Constraint::Length(9),
                Constraint::Min(8),
                Constraint::Length(13),
            ],
        )
        .block(block),
        model.schedules.len(),
        pane_selection(model, JobsPane::Schedules),
    );
}

fn render_runs(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let block = card("Runs", model.jobs_pane == JobsPane::Runs);
    if model.job_runs.is_empty() {
        empty_state(frame, area, block, Tone::Idle, "No runs yet", "");
        return;
    }
    let rows = model.job_runs.iter().map(|run| {
        let (label, tone) = run_state(run.state);
        let glyph = if matches!(run.state, JobRunState::Running | JobRunState::Launching) {
            spinner(model.tick)
        } else {
            tone.glyph()
        };
        Row::new([
            Cell::from(Line::from(vec![
                Span::styled(glyph, theme.tone(tone)),
                Span::raw(" "),
                Span::styled(label, theme.tone(tone)),
            ])),
            Cell::from(Span::styled(terminal_safe(&run.job), theme.strong())),
            Cell::from(Span::styled(
                format!("{}/{}", run.attempt, run.maximum_attempts),
                theme.muted(),
            )),
        ])
    });
    render_table(
        frame,
        area,
        Table::new(
            rows,
            [
                Constraint::Length(12),
                Constraint::Min(8),
                Constraint::Length(5),
            ],
        )
        .block(block),
        model.job_runs.len(),
        pane_selection(model, JobsPane::Runs),
    );
}
