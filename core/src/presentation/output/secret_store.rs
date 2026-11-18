//! The members of a cluster environment's managed secret store.

use super::*;
use crate::domain::secret_store::{ClusterStoreReport, StoreStatus};

/// The store's members, one per line under a line with the leader.
pub(super) fn store_lines(members: &[(&str, &StoreStatus)]) -> Vec<String> {
    let Some((_, first)) = members.first() else {
        return Vec::new();
    };
    let leader = members.iter().find_map(|(_, store)| store.leader.clone());
    let mut lines = vec![format!(
        "Secret store {}: {} member(s), leader {}",
        first.provider,
        members.len(),
        leader.as_deref().unwrap_or("none")
    )];
    lines.extend(members.iter().map(|(name, store)| member_line(name, store)));
    lines
}

fn member_line(name: &str, store: &StoreStatus) -> String {
    let state = if !store.running {
        "not running"
    } else if !store.initialized {
        "not initialised"
    } else if store.sealed {
        "sealed"
    } else {
        "unsealed"
    };
    let role = match store.peers.iter().find(|peer| peer.name == name) {
        Some(peer) if peer.leader => "leader",
        Some(peer) if peer.voter => "voter",
        Some(_) => "non-voter",
        None if store.peers.is_empty() => "-",
        None => "not in the Raft cluster",
    };
    format!(
        "  {:<16} OpenBao {:<10} {:<16} {role}",
        name,
        store.version.as_deref().unwrap_or("-"),
        state
    )
}

/// `mm secret provider status` of a cluster environment's store.
pub(crate) fn write_cluster_store_report(
    report: &ClusterStoreReport,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Secret store {} of {} on target {}, OpenBao {}: {} member(s), leader {}",
        terminal_safe(&report.provider),
        terminal_safe(&report.environment),
        terminal_safe(&report.target),
        terminal_safe(&report.version),
        report.members.len(),
        terminal_safe(report.leader.as_deref().unwrap_or("none"))
    )
    .map_err(output_error)?;
    for member in &report.members {
        let line = match (&member.status, &member.problem) {
            (Some(status), _) => member_line(&member.node, status),
            (None, problem) => format!(
                "  {:<16} {}",
                member.node,
                problem.as_deref().unwrap_or("no report")
            ),
        };
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    write_diagnostics(&mut writer, &report.diagnostics)
}
