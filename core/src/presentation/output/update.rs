use crate::domain::update_steps::{
    EnvironmentHosts, HostsView, MCP_RESTART, ServiceOutcome, ServiceReport, UpdateSummary,
};

/// What runs which release after `mm update`, and the commands left to run.
pub(crate) fn update_summary_lines(summary: &UpdateSummary) -> Vec<String> {
    let mut rows = vec![("mm on this machine".to_owned(), summary.version.clone())];
    let mut next = Vec::new();
    for service in &summary.services {
        rows.push((service.name.clone(), service_state(service)));
        next.extend(service.next.iter().map(|command| format!("  {command}")));
    }
    for server in summary.mcp_servers.iter().filter(|server| server.older) {
        rows.push((
            format!("mm mcp serve, pid {}", server.pid),
            format!("an older mm; {MCP_RESTART}"),
        ));
    }
    let mut lines = vec![String::new(), "Now running:".to_owned()];
    lines.extend(table(&rows));
    match &summary.hosts {
        HostsView::NoProject { note } => lines.push(note.clone()),
        HostsView::Project { environments } => {
            for environment in environments {
                lines.extend(environment_lines(environment));
            }
            for environment in environments.iter().filter(|hosts| hosts.needs_upgrade()) {
                lines.push(format!("Upgrade the hosts of {}:", environment.environment));
                lines.extend(
                    environment
                        .upgrade
                        .iter()
                        .map(|command| format!("  {command}")),
                );
                lines.extend(environment.note.iter().map(|note| format!("  {note}")));
            }
        }
    }
    if !next.is_empty() {
        lines.push("A local service did not come back on the new mm. Next:".to_owned());
        lines.extend(next);
    }
    lines
}

fn service_state(service: &ServiceReport) -> String {
    let version = service.version.as_deref().unwrap_or("?");
    let program = service.program.as_ref().map_or_else(
        || "another program".to_owned(),
        |path| path.display().to_string(),
    );
    match &service.outcome {
        ServiceOutcome::Restarted => format!("{version}, restarted"),
        ServiceOutcome::Current => version.to_owned(),
        ServiceOutcome::Unhealthy => "restarted, but not running".to_owned(),
        ServiceOutcome::RestartFailed { problem } => format!("not restarted: {problem}"),
        ServiceOutcome::Stopped => "stopped; it starts the new mm".to_owned(),
        ServiceOutcome::NotMm => format!("runs {program}, not mm; left running"),
        ServiceOutcome::OtherMm => format!("runs {program}, which this update did not replace"),
    }
}

fn environment_lines(environment: &EnvironmentHosts) -> Vec<String> {
    let kind = if environment.cluster {
        "cluster"
    } else {
        "host"
    };
    let mut lines = vec![format!(
        "Environment {} ({kind} {}):",
        environment.environment,
        if environment.target.is_empty() {
            "?"
        } else {
            environment.target.as_str()
        }
    )];
    if let Some(problem) = &environment.problem {
        lines.push(format!("  could not list its nodes: {problem}"));
        return lines;
    }
    let rows = environment
        .nodes
        .iter()
        .map(|node| {
            let state = match (&node.version, &node.problem) {
                (Some(version), _) if node.older => format!("mmd {version}, older"),
                (Some(version), _) => format!("mmd {version}"),
                (None, Some(problem)) => format!("did not answer: {problem}"),
                (None, None) => "did not say its release".to_owned(),
            };
            (node.name.clone(), state)
        })
        .collect::<Vec<_>>();
    lines.extend(table(&rows));
    lines
}

fn table(rows: &[(String, String)]) -> Vec<String> {
    let width = rows
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(name, state)| format!("  {name:<width$}  {state}"))
        .collect()
}
