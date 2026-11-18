//! `mm service status`: the containers mm restarted because they kept
//! failing their health check, and the ones it stopped restarting.

use crate::domain::healing::{RESTART_LIMIT, RestartEvent, ServiceRestarts};
use crate::domain::rollout::utc_time;
use crate::domain::workspace::WorkspaceSnapshot;

use super::terminal_safe;

/// The lines for every service of `snapshot` that mm restarted.
pub(crate) fn restart_lines(snapshot: &WorkspaceSnapshot) -> Vec<String> {
    snapshot
        .stacks
        .iter()
        .flat_map(|stack| &stack.services)
        .filter_map(|service| {
            service
                .restarts
                .as_ref()
                .map(|restarts| service_lines(&service.name, restarts))
        })
        .flatten()
        .collect()
}

/// `web-1 on app-2 at 2026-11-16 12:03:04 UTC`
pub(crate) fn where_and_when(event: &RestartEvent) -> String {
    format!(
        "{}{} at {}",
        terminal_safe(&event.container),
        event
            .node
            .as_deref()
            .map(|node| format!(" on {}", terminal_safe(node)))
            .unwrap_or_default(),
        utc_time(event.at_unix_ms)
    )
}

/// `restarted web: 2 times by mm, the last web-1 on app-2 at …: unhealthy
/// for 3m`, and a line for each container mm stopped restarting.
pub(crate) fn service_lines(name: &str, restarts: &ServiceRestarts) -> Vec<String> {
    let name = terminal_safe(name);
    let mut lines = Vec::new();
    if let Some(last) = restarts.restarts.last() {
        let count = restarts.restarts.len();
        lines.push(format!(
            "restarted {name}: {count} {} by mm, the last {}: {}",
            if count == 1 { "time" } else { "times" },
            where_and_when(last),
            terminal_safe(&last.reason)
        ));
    }
    for event in &restarts.limited {
        lines.push(format!(
            "restart limit {name}: mm stopped restarting {}: {}; it gets no requests while another container of {name} can take them ({RESTART_LIMIT}). `mm service logs {name}` shows why; fix it and deploy, or run `mm service restart {name}`",
            where_and_when(event),
            terminal_safe(&event.reason)
        ));
    }
    lines
}
