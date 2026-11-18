//! `mm service status`: how many containers an autoscaled service runs and
//! wants, and the last change with its reason.

use crate::domain::autoscale::ServiceScale;
use crate::domain::rollout::utc_time;
use crate::domain::workspace::WorkspaceSnapshot;

use super::terminal_safe;

/// The lines for every autoscaled service of `snapshot`.
pub(crate) fn scale_lines(snapshot: &WorkspaceSnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    for stack in &snapshot.stacks {
        for service in &stack.services {
            if let Some(scale) = &service.scale {
                lines.extend(service_lines(&service.name, scale));
            }
        }
    }
    lines
}

fn service_lines(name: &str, scale: &ServiceScale) -> Vec<String> {
    let name = terminal_safe(name);
    let mut lines = vec![format!(
        "autoscale {name}: {} running, {} wanted ({}..{}){}",
        scale.running,
        scale.wanted,
        scale.min,
        scale.max,
        scale
            .decider
            .as_deref()
            .map_or_else(String::new, |node| format!(
                ", decided on {}",
                terminal_safe(node)
            ))
    )];
    if let Some(change) = &scale.last_change {
        lines.push(format!(
            "  last change: {} -> {} at {}: {}",
            change.from,
            change.to,
            utc_time(change.at_unix_ms),
            terminal_safe(&change.reason)
        ));
    }
    if let Some(reading) = &scale.reading {
        let average = reading.average.map_or_else(
            || "no measurement".to_owned(),
            |average| average.to_string(),
        );
        lines.push(format!(
            "  {} over the last {} s: {average}",
            reading.metric.as_str(),
            reading.window_seconds
        ));
    }
    if let Some(holding) = &scale.holding {
        lines.push(format!("  now: {}", terminal_safe(holding)));
    }
    lines
}
