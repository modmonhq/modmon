//! The cluster's address table, under the node list of `mm target nodes`.

use crate::domain::cluster_network::{AddressTable, HostRange, NodeHostRanges};
use crate::domain::remote::NodeReport;

/// Rows of cells as indented lines, every column as wide as its widest
/// cell.
fn aligned(rows: &[Vec<String>]) -> Vec<String> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    rows.iter()
        .map(|row| {
            let line = row
                .iter()
                .zip(&widths)
                .map(|(cell, &width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join("  ");
            format!("  {}", line.trim_end())
        })
        .collect()
}

/// One row per network: its block, then each node's slice of it.
fn network_rows(table: &AddressTable) -> Vec<Vec<String>> {
    let nodes = table.node_names();
    let mut header = vec!["NETWORK".to_owned(), "BLOCK".to_owned()];
    header.extend(nodes.iter().map(|node| (*node).to_owned()));
    let mut rows = vec![header];
    for network in &table.networks {
        let mut row = vec![
            if network.internal {
                format!("{} (internal)", network.name)
            } else {
                network.name.clone()
            },
            network.block.to_string(),
        ];
        row.extend(nodes.iter().map(|node| {
            network
                .slice_of(node)
                .map_or_else(|| "-".to_owned(), |slice| slice.to_string())
        }));
        rows.push(row);
    }
    rows
}

/// What one node uses, one range per line. `said_above` is whether the
/// node's row in the node list already says why it does not answer.
fn host_range_cells(held: &NodeHostRanges, said_above: bool) -> Vec<String> {
    if !held.answered {
        return vec![match &held.problem {
            Some(problem) if !said_above => format!("not known: {problem}"),
            _ => "not known: the node did not answer".to_owned(),
        }];
    }
    if held.ranges.is_empty() {
        return vec!["none reported".to_owned()];
    }
    held.ranges.iter().map(HostRange::described).collect()
}

fn host_range_lines(table: &AddressTable, nodes: &[NodeReport]) -> Vec<String> {
    if table.host_ranges.is_empty() {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for held in &table.host_ranges {
        let said_above = nodes
            .iter()
            .any(|node| node.name == held.node && node.problem.is_some());
        for (index, cell) in host_range_cells(held, said_above).into_iter().enumerate() {
            let node = if index == 0 {
                held.node.clone()
            } else {
                String::new()
            };
            rows.push(vec![node, cell]);
        }
    }
    let mut lines = vec![
        String::new(),
        "Addresses the nodes already use (cluster.networks and cluster.mesh.subnet must not overlap them)"
            .to_owned(),
    ];
    lines.extend(aligned(&rows));
    lines
}

/// The lines `mm target nodes` prints after the node rows: the block and
/// slices of each network, then what every node uses outside mm. Nothing
/// for a target whose networks stay local to each host.
pub(crate) fn address_table_lines(table: &AddressTable, nodes: &[NodeReport]) -> Vec<String> {
    if table.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![
        String::new(),
        table.space.map_or_else(
            || "Networks".to_owned(),
            |space| format!("Networks (cluster.networks {space})"),
        ),
    ];
    if table.networks.is_empty() {
        lines.push(
            "  No network of this environment has addresses yet; the first deployment gives them out."
                .to_owned(),
        );
    } else {
        lines.extend(aligned(&network_rows(table)));
    }
    if !table.held_by_others.is_empty() {
        let blocks = table
            .held_by_others
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "  Other projects and environments on these nodes hold {blocks}."
        ));
    }
    if !table.differing.is_empty() {
        lines.push(
            "  The nodes do not agree, so the table shows what the first of them holds:".to_owned(),
        );
        lines.extend(table.differing.iter().map(|line| format!("    {line}")));
        lines.push(
            "  `mm deploy plan` refuses until they agree, and says how to put it right.".to_owned(),
        );
    }
    lines.extend(host_range_lines(table, nodes));
    lines
}
