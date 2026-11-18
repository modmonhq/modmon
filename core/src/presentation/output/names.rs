//! What the service names of a cluster environment resolve to, under the
//! service rows of a status.

use std::io::Write;

use super::{output_error, terminal_safe};
use crate::domain::discovery::{NameAddress, NameReport, ResolvedName};
use crate::domain::error::MmError;

/// The line of a plan that says who answers names on the networks that
/// span the nodes.
pub(crate) fn names_header_line() -> &'static str {
    "  service names: each node answers its own containers, on its address on each network (port 53), with a service's healthy containers on the same node, or else on every node"
}

fn address(address: &NameAddress, health: bool) -> String {
    let mut notes = vec![address.node.clone()];
    if health {
        notes.push(address.health.as_str().to_owned());
    }
    if address.restart_limit {
        notes.push(format!(
            "no longer restarted, {}",
            crate::domain::healing::RESTART_LIMIT
        ));
    }
    if !address.unknown_to.is_empty() {
        notes.push(format!("not known on {}", address.unknown_to.join(", ")));
    }
    format!("{} ({})", address.address, notes.join(", "))
}

/// `a`, `a and b`, `a, b and c`.
fn listed(nodes: &[&str]) -> String {
    match nodes {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// Which nodes answer `<service>` with their own containers, when the
/// nodes do not all give the same answer: `each node resolves to its own`,
/// or `app-2 and db-1 resolve to their own, app-1 to all`.
fn steering(name: &ResolvedName) -> Option<String> {
    let answer = |lookup: &crate::domain::discovery::NodeLookup| {
        let mut addresses = lookup.addresses.clone();
        addresses.sort_unstable();
        addresses
    };
    let first = answer(name.lookups.first()?);
    if name.lookups.iter().all(|lookup| answer(lookup) == first) {
        return None;
    }
    let nodes = |local: bool| {
        name.lookups
            .iter()
            .filter(|lookup| lookup.local == local)
            .map(|lookup| lookup.node.as_str())
            .collect::<Vec<_>>()
    };
    let (own, all) = (nodes(true), nodes(false));
    match (own.as_slice(), all.as_slice()) {
        ([], _) => None,
        (_, []) => Some("each node resolves to its own".to_owned()),
        ([one], _) => Some(format!(
            "{one} resolves to its own, {} to all",
            listed(&all)
        )),
        _ => Some(format!(
            "{} resolve to their own, {} to all",
            listed(&own),
            listed(&all)
        )),
    }
}

/// `10.64.0.2 (app-1), 10.64.1.2 (app-2); left out: 10.64.2.2 (app-3,
/// unhealthy)`: the service's healthy containers on every node, then the
/// ones that are not healthy and so are in no answer, then which nodes
/// answer with their own when the answers differ between nodes. When none
/// is healthy the running ones are answered, and the line says so.
pub(crate) fn resolution(name: &ResolvedName) -> String {
    let open = name.fail_open || name.fails_open();
    let answers = name
        .routed()
        .map(|answer| address(answer, open))
        .collect::<Vec<_>>();
    let left_out = name
        .addresses
        .iter()
        .filter(|candidate| !name.routed().any(|routed| routed == *candidate))
        .map(|candidate| address(candidate, true))
        .collect::<Vec<_>>();
    let mut line = if answers.is_empty() {
        "nothing".to_owned()
    } else {
        answers.join(", ")
    };
    if !left_out.is_empty() {
        line.push_str(&format!("; left out: {}", left_out.join(", ")));
    }
    if let Some(steering) = steering(name) {
        line.push_str(&format!("; {steering}"));
    }
    if open {
        line.push_str(
            "; answered while unhealthy: none of its containers passes its health check, so mm answers the running ones (`mm doctor` names the check)",
        );
    }
    line
}

pub(super) fn write_names(mut writer: impl Write, names: &NameReport) -> Result<(), MmError> {
    if !names.names.is_empty() {
        writeln!(writer, "\n  {:<24} {:<14} RESOLVES TO", "NAME", "NETWORK")
            .map_err(output_error)?;
    }
    for name in &names.names {
        writeln!(
            writer,
            "  {:<24} {:<14} {}",
            terminal_safe(&name.name),
            terminal_safe(&name.network),
            terminal_safe(&resolution(name))
        )
        .map_err(output_error)?;
    }
    for problem in &names.problems {
        writeln!(
            writer,
            "  node {}: {}",
            terminal_safe(&problem.node),
            terminal_safe(&problem.detail)
        )
        .map_err(output_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::discovery::{Health, NodeLookup};

    fn lookup(node: &str, thirds: &[u8], local: bool) -> NodeLookup {
        NodeLookup {
            node: node.to_owned(),
            addresses: thirds
                .iter()
                .map(|third| [10, 64, *third, 2].into())
                .collect(),
            local,
        }
    }

    #[test]
    fn a_name_says_which_nodes_answer_with_their_own_when_they_differ() {
        let mut name = ResolvedName {
            name: "cache".to_owned(),
            network: "backend".to_owned(),
            addresses: vec![
                address_on(0, "app-1", Health::Healthy, &[]),
                address_on(1, "app-2", Health::Healthy, &[]),
                address_on(2, "db-1", Health::Healthy, &[]),
            ],
            lookups: vec![
                lookup("app-1", &[0], true),
                lookup("app-2", &[1], true),
                lookup("db-1", &[2], true),
            ],
            fail_open: false,
        };
        assert_eq!(
            resolution(&name),
            "10.64.0.2 (app-1), 10.64.1.2 (app-2), 10.64.2.2 (db-1); each node resolves to its own"
        );
        // db-1 has none of its own: it answers with the others'.
        name.addresses.truncate(2);
        name.lookups[2] = lookup("db-1", &[0, 1], false);
        assert_eq!(
            resolution(&name),
            "10.64.0.2 (app-1), 10.64.1.2 (app-2); app-1 and app-2 resolve to their own, db-1 to all"
        );
        // One container: every node gives the same answer, and the line
        // stays as short as it was.
        name.addresses.truncate(1);
        name.lookups = vec![
            lookup("app-1", &[0], true),
            lookup("app-2", &[0], false),
            lookup("db-1", &[0], false),
        ];
        assert_eq!(resolution(&name), "10.64.0.2 (app-1)");
        // Two on one node, as while a container is replaced start-first.
        name.addresses = vec![
            address_on(0, "app-1", Health::Healthy, &[]),
            address_on(1, "app-1", Health::Healthy, &[]),
        ];
        name.lookups = vec![
            lookup("app-1", &[0, 1], true),
            lookup("app-2", &[1, 0], false),
        ];
        assert_eq!(resolution(&name), "10.64.0.2 (app-1), 10.64.1.2 (app-1)");
        name.addresses
            .push(address_on(2, "db-1", Health::Healthy, &[]));
        name.lookups = vec![
            lookup("app-1", &[0, 1], true),
            lookup("app-2", &[0, 1, 2], false),
            lookup("db-1", &[2], true),
        ];
        assert!(
            resolution(&name).ends_with("; app-1 and db-1 resolve to their own, app-2 to all"),
            "{}",
            resolution(&name)
        );
        name.lookups.remove(2);
        assert!(
            resolution(&name).ends_with("; app-1 resolves to its own, app-2 to all"),
            "{}",
            resolution(&name)
        );
    }

    fn address_on(third: u8, node: &str, health: Health, unknown_to: &[&str]) -> NameAddress {
        NameAddress {
            address: [10, 64, third, 2].into(),
            node: node.to_owned(),
            health,
            unknown_to: unknown_to.iter().map(|node| (*node).to_owned()).collect(),
            restart_limit: false,
        }
    }

    #[test]
    fn a_name_lists_its_answers_then_what_is_left_out() {
        let mut name = ResolvedName {
            name: "web".to_owned(),
            network: "backend".to_owned(),
            addresses: vec![
                address_on(0, "app-1", Health::Healthy, &[]),
                address_on(1, "app-2", Health::Healthy, &["db-1"]),
                address_on(2, "db-1", Health::Unhealthy, &[]),
            ],
            lookups: Vec::new(),
            fail_open: false,
        };
        assert_eq!(
            resolution(&name),
            "10.64.0.2 (app-1), 10.64.1.2 (app-2, not known on db-1); left out: 10.64.2.2 (db-1, unhealthy)"
        );
        name.addresses.truncate(1);
        assert_eq!(resolution(&name), "10.64.0.2 (app-1)");
        // None healthy: the running ones are answered, and it says so.
        name.addresses = vec![
            address_on(2, "db-1", Health::Unhealthy, &[]),
            address_on(3, "db-2", Health::Stopped, &[]),
        ];
        assert_eq!(
            resolution(&name),
            "10.64.2.2 (db-1, unhealthy); left out: 10.64.3.2 (db-2, stopped); answered while unhealthy: none of its containers passes its health check, so mm answers the running ones (`mm doctor` names the check)"
        );
        name.addresses.remove(0);
        assert_eq!(
            resolution(&name),
            "nothing; left out: 10.64.3.2 (db-2, stopped)"
        );

        let mut written = Vec::new();
        let report = NameReport {
            networks: vec!["backend".to_owned()],
            names: vec![name],
            problems: vec![crate::domain::discovery::NameProblem {
                node: "app-1".to_owned(),
                detail: "never heard from node 'db-1'".to_owned(),
            }],
        };
        assert!(write_names(&mut written, &report).is_ok());
        let written = String::from_utf8_lossy(&written);
        assert!(
            written.contains("NAME") && written.contains("RESOLVES TO"),
            "{written}"
        );
        assert!(
            written.contains("  node app-1: never heard from node 'db-1'"),
            "{written}"
        );
    }
}
