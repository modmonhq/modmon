//! The mesh between a cluster target's nodes: its status, the peer changes
//! of a setup plan, and key rotation plans.

use super::*;
use crate::domain::mesh::{
    MeshNodeOrigin, MeshPlan, MeshReport, MeshRotationPlan, MeshRotationResult,
};

fn short_key(key: &str) -> String {
    format!("{}…", key.chars().take(12).collect::<String>())
}

fn bytes(count: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = count;
    let mut unit = 0;
    while value >= 10 * 1024 && unit + 1 < UNITS.len() {
        value /= 1024;
        unit += 1;
    }
    format!("{value} {}", UNITS[unit])
}

fn handshake(age: Option<u64>) -> String {
    match age {
        None => "never".to_owned(),
        Some(seconds) if seconds < 120 => format!("{seconds} s ago"),
        Some(seconds) if seconds < 7200 => format!("{} min ago", seconds / 60),
        Some(seconds) => format!("{} h ago", seconds / 3600),
    }
}

/// One node of the status: its interface, then a row per peer.
fn write_mesh_node(
    writer: &mut impl Write,
    node: &crate::domain::mesh::MeshNodeReport,
) -> Result<(), MmError> {
    let Some(status) = &node.status else {
        return writeln!(
            writer,
            "\n  {}  no answer: {}",
            terminal_safe(&node.name),
            terminal_safe(node.problem.as_deref().unwrap_or("unknown"))
        )
        .map_err(output_error);
    };
    let Some(interface) = &status.interface else {
        return writeln!(
            writer,
            "\n  {}  no mesh interface",
            terminal_safe(&node.name)
        )
        .map_err(output_error);
    };
    writeln!(
        writer,
        "\n  {}  {} {}  {}  UDP {}  mtu {}  key {}",
        terminal_safe(&node.name),
        terminal_safe(interface),
        if status.up { "up" } else { "DOWN" },
        status
            .address
            .map_or_else(|| "-".to_owned(), |address| address.to_string()),
        status
            .port
            .map_or_else(|| "-".to_owned(), |port| port.to_string()),
        status
            .mtu
            .map_or_else(|| "-".to_owned(), |mtu| mtu.to_string()),
        terminal_safe(
            &status
                .public_key
                .as_deref()
                .map_or_else(|| "-".to_owned(), short_key)
        ),
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "    {:<16} {:<26} {:<12} {:>10} {:>10}",
        "PEER", "ENDPOINT", "HANDSHAKE", "RECEIVED", "SENT"
    )
    .map_err(output_error)?;
    for peer in &status.peers {
        writeln!(
            writer,
            "    {:<16} {:<26} {:<12} {:>10} {:>10}",
            terminal_safe(
                &peer
                    .name
                    .clone()
                    .unwrap_or_else(|| short_key(&peer.public_key))
            ),
            terminal_safe(
                peer.endpoint
                    .as_deref()
                    .or(peer.configured_endpoint.as_deref())
                    .unwrap_or("-")
            ),
            if peer.on_interface {
                handshake(peer.handshake_age_seconds)
            } else {
                "not applied".to_owned()
            },
            bytes(peer.received_bytes),
            bytes(peer.sent_bytes),
        )
        .map_err(output_error)?;
    }
    Ok(())
}

/// `mm target mesh status`: every node's interface, each peer's endpoint,
/// last handshake, and traffic, then what is wrong and what to do.
pub(crate) fn write_mesh_report(report: &MeshReport, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if let Some(interface) = &report.existing_interface {
        return writeln!(
            writer,
            "Target {} uses the private interface {} the nodes already share; mm manages no WireGuard mesh for it.",
            terminal_safe(&report.target),
            terminal_safe(interface)
        )
        .map_err(output_error);
    }
    writeln!(
        writer,
        "Target {}: mesh between {} node(s)",
        terminal_safe(&report.target),
        report.nodes.len()
    )
    .map_err(output_error)?;
    for node in &report.nodes {
        write_mesh_node(&mut writer, node)?;
    }
    if report.problems.is_empty() {
        if report.healthy() {
            writeln!(
                writer,
                "\nEvery node has a recent handshake with every other."
            )
            .map_err(output_error)?;
        }
        return Ok(());
    }
    writeln!(writer, "\nProblems:").map_err(output_error)?;
    write_diagnostics(&mut writer, &report.problems)
}

/// The mesh section of a cluster setup plan: every node's interface and the
/// change to each of its peers.
pub(crate) fn mesh_plan_text(plan: &MeshPlan) -> String {
    let mut text = format!(
        "\nMesh: WireGuard {}, {} mtu {}. UDP port {} must be reachable between the nodes.\n",
        plan.interface, plan.subnet, plan.mtu, plan.port
    );
    if !plan.changes() {
        text.push_str("  No change: every node already holds its peers.\n");
    }
    for node in &plan.nodes {
        let key = node.public_key.as_deref().map_or_else(
            || "its key is made on the node during apply".to_owned(),
            |key| format!("key {}", short_key(key)),
        );
        let origin = match node.origin {
            MeshNodeOrigin::New => " (new on this node)",
            MeshNodeOrigin::Legacy => " (replaces wg0 from an earlier mm; the node keeps its key)",
            MeshNodeOrigin::Current => "",
        };
        text.push_str(&format!(
            "  {:<20} {}  at {}  {key}{origin}\n",
            node.node, node.address, node.endpoint
        ));
        for change in &node.interface_changes {
            text.push_str(&format!("      ~ {change}\n"));
        }
        for peer in &node.peers {
            text.push_str(&format!("      {}\n", peer.summary()));
        }
    }
    text
}

/// `mm target mesh rotate NODE`: what the rotation replaces, and the
/// command that applies exactly this plan.
pub(crate) fn write_mesh_rotation_plan(
    plan: &MeshRotationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let mut text = format!(
        "Mesh key rotation plan {}\n  environment: {}\n  target:      {}\n  node:        {} ({})\n  current key: {}\n",
        plan.plan_hash,
        plan.environment,
        plan.target,
        plan.node,
        plan.address,
        short_key(&plan.current_public_key),
    );
    match &plan.pending_public_key {
        Some(pending) => text.push_str(&format!(
            "  new key:     {} (made by an earlier rotation that did not finish)\n",
            short_key(pending)
        )),
        None => text.push_str(&format!(
            "  new key:     made on {} during apply; the private key never leaves it\n",
            plan.node
        )),
    }
    text.push_str("\nPeer entries replaced:\n");
    for peer in &plan.peers {
        let change = match (&peer.holds, &plan.pending_public_key) {
            (Some(held), Some(pending)) if held == pending => {
                "already holds the new key".to_owned()
            }
            (Some(held), _) => format!("~ {}: key {} -> the new key", plan.node, short_key(held)),
            (None, _) => format!("+ {}: the new key", plan.node),
        };
        text.push_str(&format!("  {:<20} {change}\n", peer.node));
    }
    text.push_str(&format!(
        "\nNothing else changes: the other nodes keep their keys and their sessions with each other.\n{}'s links drop for a few seconds while its peers switch.\n\nApply with:\n  {} --yes\n",
        plan.node,
        plan.apply_command()
    ));
    let stdout = crate::presentation::record::stdout();
    write_terminal_safe_multiline(&mut stdout.lock(), &text)
}

pub(crate) fn write_mesh_rotation_result(
    result: &MeshRotationResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let text = format!(
        "Rotated the mesh key of {}: {} -> {}\nPeers updated: {}\nCheck the links with:\n  mm --environment {} target mesh status\n",
        result.node,
        short_key(&result.previous_public_key),
        short_key(&result.public_key),
        if result.updated_peers.is_empty() {
            "none".to_owned()
        } else {
            result.updated_peers.join(", ")
        },
        result.environment,
    );
    let stdout = crate::presentation::record::stdout();
    write_terminal_safe_multiline(&mut stdout.lock(), &text)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;
    use crate::domain::mesh::{MeshMember, MeshState, PlannedPeer, peer_set, plan_node};
    use crate::domain::repo_target::WireguardMesh;

    fn key(byte: u8) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([byte; 32])
    }

    fn members() -> Vec<MeshMember> {
        ["app-1", "app-2", "app-4"]
            .into_iter()
            .zip([1_u8, 2, 4])
            .map(|(name, last)| MeshMember {
                name: name.to_owned(),
                address: Ipv4Addr::new(10, 90, 0, last),
                endpoint: format!("203.0.113.{}", 10 + last),
                // app-4 is new: it has no key yet.
                public_key: (last != 4).then(|| key(last)),
                routed: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn the_setup_plan_shows_each_nodes_interface_and_peer_changes() {
        let mesh = WireguardMesh::default();
        let members = members();
        let held = peer_set(mesh.port, &members[..2], "app-1")
            .iter()
            .filter_map(PlannedPeer::resolved)
            .collect();
        let facts = crate::domain::mesh::MeshFacts {
            public_key: Some(key(1)),
            state: Some(MeshState::new(&mesh, members[0].address, held)),
            ..crate::domain::mesh::MeshFacts::default()
        };
        let plan = MeshPlan {
            interface: "mm0".to_owned(),
            subnet: mesh.subnet,
            port: mesh.port,
            mtu: mesh.mtu,
            nodes: vec![
                plan_node(&mesh, &members, &members[0], &[], Some(&facts)),
                plan_node(&mesh, &members, &members[2], &[], None),
            ],
        };
        let text = mesh_plan_text(&plan);
        for expected in [
            "Mesh: WireGuard mm0, 10.90.0.0/24 mtu 1420. UDP port 51820 must be reachable between the nodes.",
            "  app-1                10.90.0.1  at 203.0.113.11:51820  key AQEBAQEBAQEB…\n",
            "      = app-2: unchanged\n",
            "      + app-4: 10.90.0.4/32 at 203.0.113.14:51820, key made on app-4 during apply\n",
            "  app-4                10.90.0.4  at 203.0.113.14:51820  its key is made on the node during apply (new on this node)\n",
            "      + app-1: 10.90.0.1/32 at 203.0.113.11:51820, key AQEBAQEBAQEB…\n",
        ] {
            assert!(text.contains(expected), "{expected}\n---\n{text}");
        }
        assert!(!text.contains("No change"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("private"), "{text}");
    }

    #[test]
    fn traffic_and_handshakes_are_short_to_read() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(9304), "9304 B");
        assert_eq!(bytes(10 * 1024), "10 KiB");
        assert_eq!(bytes(5 * 1024 * 1024 * 1024), "5120 MiB");
        assert_eq!(bytes(50 * 1024 * 1024 * 1024), "50 GiB");
        assert_eq!(handshake(None), "never");
        assert_eq!(handshake(Some(86)), "86 s ago");
        assert_eq!(handshake(Some(600)), "10 min ago");
        assert_eq!(handshake(Some(3 * 3600 + 5)), "3 h ago");
    }
}
