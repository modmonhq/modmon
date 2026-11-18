use super::*;
use crate::domain::onboarding::{DiscoveryConfidence, WorkflowState, WorkflowStep};

const STEPS: [&str; 6] = ["Choose", "Discover", "Configure", "Review", "Apply", "Done"];

/// Position in [`STEPS`], or `None` for the off-path error and cancelled states.
pub(super) const fn step_position(step: WorkflowStep) -> Option<usize> {
    match step {
        WorkflowStep::ChooseJourney => Some(0),
        WorkflowStep::Discover => Some(1),
        WorkflowStep::Configure => Some(2),
        WorkflowStep::Review => Some(3),
        WorkflowStep::Applying => Some(4),
        WorkflowStep::Complete => Some(5),
        WorkflowStep::Error | WorkflowStep::Cancelled => None,
    }
}

pub(super) fn render_guided(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    render_guided_state(frame, area, &model.workflow, model.scroll);
}

pub(super) fn render_guided_state(
    frame: &mut Frame<'_>,
    area: Rect,
    workflow: &WorkflowState,
    scroll: usize,
) {
    let theme = theme();
    let mut lines = vec![match step_position(workflow.step) {
        Some(position) => stepper(
            &STEPS,
            position,
            if workflow.step == WorkflowStep::Complete {
                Tone::Ok
            } else {
                Tone::Info
            },
        ),
        None if workflow.step == WorkflowStep::Error => Line::from(badge(Tone::Bad, "Error")),
        None => Line::from(badge(Tone::Idle, "Cancelled")),
    }];
    lines.push(Line::raw(""));
    if let Some(message) = &workflow.message {
        let tone = match workflow.step {
            WorkflowStep::Error => Tone::Bad,
            WorkflowStep::Complete => Tone::Ok,
            WorkflowStep::Cancelled => Tone::Idle,
            _ => Tone::Info,
        };
        lines.push(Line::styled(terminal_safe(message), theme.tone(tone)));
        lines.push(Line::raw(""));
    }
    match &workflow.review {
        Some(review) => push_review(&mut lines, review, scroll),
        None if workflow.step == WorkflowStep::ChooseJourney => {
            lines.push(section("Start a journey"));
            for (key, label) in [
                ("g", "Set up this machine"),
                ("i", "Create a new project"),
                ("p", "Adopt an existing project"),
                ("n", "Set up a remote node"),
                ("e", "Edit this project"),
            ] {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {key}  "), theme.key()),
                    Span::styled(label, theme.text()),
                ]));
            }
        }
        None => {}
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(card("Guided setup", true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The reviewed plan: discovery proposals, privilege prompts, and next steps.
fn push_review<'a>(
    lines: &mut Vec<Line<'a>>,
    review: &'a crate::domain::onboarding::WorkflowReview,
    scroll: usize,
) {
    let theme = theme();
    lines.push(Line::styled(terminal_safe(&review.title), theme.strong()));
    lines.push(kv(
        "Plan hash",
        vec![Span::styled(review.plan_hash.clone(), theme.key())],
        11,
    ));
    for proposal in review.proposals.iter().skip(scroll) {
        let (confidence, tone) = match proposal.confidence {
            DiscoveryConfidence::High => ("high", Tone::Ok),
            DiscoveryConfidence::Medium => ("medium", Tone::Info),
            DiscoveryConfidence::Low => ("low", Tone::Warn),
        };
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(terminal_safe(&proposal.subject), theme.strong()),
            Span::styled(" = ", theme.muted()),
            Span::styled(terminal_safe(&proposal.proposed_value), theme.accent()),
            Span::styled(format!("   {confidence}"), theme.tone(tone)),
        ]));
        for reason in &proposal.reasons {
            lines.push(Line::styled(
                format!("  because {}", terminal_safe(reason)),
                theme.muted(),
            ));
        }
    }
    if !review.privilege_prompts.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Needs elevated privileges"));
        for privilege in &review.privilege_prompts {
            lines.push(Line::from(badge(Tone::Bad, terminal_safe(privilege))));
        }
    }
    if !review.next_actions.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Next"));
        for action in &review.next_actions {
            lines.push(Line::from(vec![
                Span::styled("  → ", theme.accent()),
                Span::styled(terminal_safe(action), theme.text()),
            ]));
        }
    }
}

pub(super) fn render_small_terminal(frame: &mut Frame<'_>) {
    frame.render_widget(
        Paragraph::new("Terminal too small for mm. Resize to at least 40x10, or use `mm init`, `mm secret provider --help`, and `--output json` for a linear/screen-reader-friendly view.")
            .wrap(Wrap { trim: true }),
        frame.area(),
    );
}
