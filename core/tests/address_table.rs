//! The cluster's address table as `mm target nodes` and the MCP read
//! `nodes` show it: built from what every node stored, in declared order.

use mm::domain::cluster_network::{
    AddressTable, HostRange, HostRangeKind, NetworkNeed, NetworkTable, NodeNetworkReport,
    NodeNetworkStatus, NodeNetworks, environment_network, network_key,
};
use mm::domain::discovery::{NameReport, ResolvedName};
use mm::domain::repo_target::Ipv4Cidr;
use mm::domain::workspace::ProjectId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SPACE: &str = "10.64.0.0/12";
/// Declared in this order, which is not the order of the names.
const NODES: [&str; 3] = ["web", "app", "db"];

fn cidr(text: &str) -> Result<Ipv4Cidr, String> {
    text.parse()
}

fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(7))
}

/// What every network of the selected environment's name starts with.
fn prefix() -> String {
    network_key(project(), "production", "")
}

fn key(name: &str) -> String {
    network_key(project(), "production", name)
}

/// A table that gives each `(network, node)` pair the lowest free
/// addresses of `space`, in order.
fn table_in(space: &str, pairs: &[(&str, &str)]) -> Result<NetworkTable, String> {
    let space = cidr(space)?;
    let mut table = NetworkTable::default();
    for &(network, node) in pairs {
        table.assign(
            space,
            &NetworkNeed {
                key: network,
                node,
                network: &format!("{node}-{network}"),
                internal: false,
            },
        )?;
    }
    Ok(table)
}

/// `backend` on all three nodes, `frontend` on `web` alone.
fn deployed() -> Result<NetworkTable, String> {
    let (backend, frontend) = (key("backend"), key("frontend"));
    table_in(
        SPACE,
        &[
            (backend.as_str(), "web"),
            (backend.as_str(), "app"),
            (backend.as_str(), "db"),
            (frontend.as_str(), "web"),
        ],
    )
}

fn address(range: &str, interface: &str) -> Result<HostRange, String> {
    Ok(HostRange {
        range: cidr(range)?,
        kind: HostRangeKind::Address,
        interface: Some(interface.to_owned()),
    })
}

fn route(range: &str, interface: &str) -> Result<HostRange, String> {
    Ok(HostRange {
        range: cidr(range)?,
        kind: HostRangeKind::Route,
        interface: Some(interface.to_owned()),
    })
}

fn answered(
    node: &str,
    table: Option<&NetworkTable>,
    host_ranges: Vec<HostRange>,
) -> Result<NodeNetworkReport, String> {
    let stored = match table {
        Some(table) => Some(NodeNetworks {
            node: node.to_owned(),
            space: cidr(SPACE)?,
            mesh_interface: "mm0".to_owned(),
            mtu: 1420,
            table: table.clone(),
        }),
        None => None,
    };
    Ok(NodeNetworkReport {
        name: node.to_owned(),
        reachable: true,
        problem: None,
        status: Some(NodeNetworkStatus {
            node: Some(node.to_owned()),
            stored,
            drift: Vec::new(),
            host_ranges,
        }),
    })
}

fn silent(node: &str) -> NodeNetworkReport {
    NodeNetworkReport {
        name: node.to_owned(),
        reachable: false,
        problem: Some("does not answer on port 22. Check that the host is up".to_owned()),
        status: None,
    }
}

/// Every node answered and holds `table`.
fn all_hold(table: &NetworkTable) -> Result<Vec<NodeNetworkReport>, String> {
    NODES
        .iter()
        .map(|node| answered(node, Some(table), Vec::new()))
        .collect()
}

fn table_of(nodes: &[NodeNetworkReport]) -> Result<AddressTable, String> {
    let prefix = prefix();
    Ok(AddressTable::from_reports(
        Some(cidr(SPACE)?),
        &|key| key.strip_prefix(&prefix).map(str::to_owned),
        nodes,
    ))
}

#[test]
fn each_network_has_its_block_and_each_node_its_slice_in_declared_order() -> TestResult {
    let table = table_of(&all_hold(&deployed()?)?)?;
    assert_eq!(table.space, Some(cidr(SPACE)?));
    assert_eq!(table.node_names(), NODES);
    assert!(table.differing.is_empty());
    assert!(table.held_by_others.is_empty());
    assert!(!table.is_empty());

    let names = table
        .networks
        .iter()
        .map(|network| network.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["backend", "frontend"]);

    let backend = &table.networks[0];
    assert_eq!(backend.key, key("backend"));
    assert_eq!(backend.block, cidr("10.64.0.0/20")?);
    assert!(!backend.internal);
    let holders = backend
        .nodes
        .iter()
        .map(|held| (held.node.as_str(), held.slice.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(
        holders,
        [
            ("web", "10.64.0.0/24".to_owned()),
            ("app", "10.64.1.0/24".to_owned()),
            ("db", "10.64.2.0/24".to_owned()),
        ]
    );
    // The name the network has on that node's engine comes along.
    assert_eq!(backend.nodes[1].network, format!("app-{}", key("backend")));
    Ok(())
}

#[test]
fn a_node_without_a_slice_of_a_network_is_not_among_its_holders() -> TestResult {
    let table = table_of(&all_hold(&deployed()?)?)?;
    let frontend = &table.networks[1];
    assert_eq!(frontend.block, cidr("10.64.16.0/20")?);
    assert_eq!(frontend.slice_of("web"), Some(cidr("10.64.16.0/24")?));
    assert_eq!(frontend.slice_of("app"), None);
    assert_eq!(frontend.slice_of("db"), None);
    assert_eq!(frontend.nodes.len(), 1);
    // It still has its column: the columns are the target's nodes.
    assert_eq!(table.node_names(), NODES);
    Ok(())
}

#[test]
fn a_node_that_never_held_a_network_still_lists_what_it_uses() -> TestResult {
    let deployed = deployed()?;
    let nodes = vec![
        answered(
            "web",
            Some(&deployed),
            vec![
                address("104.204.69.133/32", "eth0")?,
                route("10.106.0.0/20", "eth1")?,
            ],
        )?,
        answered("app", Some(&deployed), Vec::new())?,
        // Joined the target after the last deployment: nothing stored.
        answered("db", None, vec![route("172.17.0.0/16", "docker0")?])?,
    ];
    let table = table_of(&nodes)?;
    // What the others stored still says which slice it has.
    assert_eq!(
        table.networks[0].slice_of("db"),
        Some(cidr("10.64.2.0/24")?)
    );
    let held = table
        .host_ranges
        .iter()
        .map(|held| (held.node.as_str(), held.answered, held.ranges.len()))
        .collect::<Vec<_>>();
    assert_eq!(held, [("web", true, 2), ("app", true, 0), ("db", true, 1)]);
    assert_eq!(
        table.host_ranges[0].ranges[1],
        route("10.106.0.0/20", "eth1")?
    );
    Ok(())
}

#[test]
fn a_silent_node_keeps_its_slices_and_its_ranges_are_unknown() -> TestResult {
    let deployed = deployed()?;
    let nodes = vec![
        answered(
            "web",
            Some(&deployed),
            vec![address("104.204.69.133/32", "eth0")?],
        )?,
        answered("app", Some(&deployed), Vec::new())?,
        silent("db"),
    ];
    let table = table_of(&nodes)?;
    assert!(table.differing.is_empty());
    assert_eq!(table.node_names(), NODES);
    assert_eq!(
        table.networks[0].slice_of("db"),
        Some(cidr("10.64.2.0/24")?)
    );
    let db = &table.host_ranges[2];
    assert_eq!(db.node, "db");
    assert!(!db.answered);
    assert!(db.ranges.is_empty());
    assert_eq!(
        db.problem.as_deref(),
        Some("does not answer on port 22. Check that the host is up")
    );
    assert!(table.host_ranges[0].answered);
    assert_eq!(table.host_ranges[0].problem, None);
    Ok(())
}

#[test]
fn no_node_answering_leaves_the_space_and_one_row_per_node() -> TestResult {
    let nodes = NODES.iter().copied().map(silent).collect::<Vec<_>>();
    let table = table_of(&nodes)?;
    assert!(table.networks.is_empty());
    assert!(!table.is_empty());
    assert_eq!(table.node_names(), NODES);
    assert!(table.host_ranges.iter().all(|held| !held.answered));
    Ok(())
}

#[test]
fn two_nodes_that_disagree_about_a_network_are_named_and_the_rest_still_shows() -> TestResult {
    let deployed = deployed()?;
    // `app` got its addresses for `backend` from a deployment that did not
    // see the others.
    let backend = key("backend");
    let apart = table_in("10.80.0.0/12", &[(backend.as_str(), "app")])?;
    let nodes = vec![
        answered("web", Some(&deployed), Vec::new())?,
        answered("app", Some(&apart), Vec::new())?,
        answered("db", Some(&deployed), Vec::new())?,
    ];
    let table = table_of(&nodes)?;
    assert_eq!(table.differing.len(), 1, "{:?}", table.differing);
    let said = &table.differing[0];
    for part in [
        "'web'",
        "'app'",
        backend.as_str(),
        "10.64.0.0/20",
        "10.80.0.0/20",
    ] {
        assert!(said.contains(part), "{said} does not name {part}");
    }
    // The row is what the first node that holds the network has.
    assert_eq!(table.networks[0].name, "backend");
    assert_eq!(table.networks[0].block, cidr("10.64.0.0/20")?);
    assert_eq!(
        table.networks[0].slice_of("app"),
        Some(cidr("10.64.1.0/24")?)
    );
    // The network they agree about is untouched.
    assert_eq!(table.networks[1].name, "frontend");
    assert_eq!(table.networks[1].block, cidr("10.64.16.0/20")?);
    Ok(())
}

#[test]
fn two_nodes_that_disagree_about_a_slice_are_named() -> TestResult {
    let backend = key("backend");
    let backend = backend.as_str();
    let first = table_in(SPACE, &[(backend, "web"), (backend, "app")])?;
    // Here `app` has the slice `web` has in the other table.
    let second = table_in(SPACE, &[(backend, "app"), (backend, "web")])?;
    let nodes = vec![
        answered("web", Some(&first), Vec::new())?,
        answered("app", Some(&second), Vec::new())?,
    ];
    let table = table_of(&nodes)?;
    assert_eq!(table.differing.len(), 1, "{:?}", table.differing);
    assert!(table.differing[0].contains("disagree about the addresses of node"));
    assert_eq!(
        table.networks[0].slice_of("web"),
        Some(cidr("10.64.0.0/24")?)
    );
    Ok(())
}

#[test]
fn two_networks_on_the_same_addresses_are_reported() -> TestResult {
    // Two deployments that did not see each other gave the first block to
    // different networks.
    let (backend, frontend) = (key("backend"), key("frontend"));
    let nodes = vec![
        answered(
            "web",
            Some(&table_in(SPACE, &[(backend.as_str(), "web")])?),
            Vec::new(),
        )?,
        answered(
            "app",
            Some(&table_in(SPACE, &[(frontend.as_str(), "app")])?),
            Vec::new(),
        )?,
    ];
    let table = table_of(&nodes)?;
    assert_eq!(table.networks.len(), 2);
    assert_eq!(table.differing.len(), 1, "{:?}", table.differing);
    assert!(table.differing[0].contains("both hold 10.64.0.0/20"));
    Ok(())
}

#[test]
fn networks_of_other_projects_and_environments_are_blocks_without_names() -> TestResult {
    let other_project = network_key(
        ProjectId(uuid::Uuid::from_u128(u128::MAX)),
        "production",
        "queue",
    );
    let other_environment = network_key(project(), "staging", "backend");
    let (backend, frontend) = (key("backend"), key("frontend"));
    let shared = table_in(
        SPACE,
        &[
            (backend.as_str(), "web"),
            (other_project.as_str(), "web"),
            (other_environment.as_str(), "app"),
            (frontend.as_str(), "web"),
        ],
    )?;
    let table = table_of(&all_hold(&shared)?)?;
    let rows = table
        .networks
        .iter()
        .map(|network| (network.name.as_str(), network.block.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            ("backend", "10.64.0.0/20".to_owned()),
            ("frontend", "10.64.48.0/20".to_owned()),
        ]
    );
    assert_eq!(
        table.held_by_others,
        [cidr("10.64.16.0/20")?, cidr("10.64.32.0/20")?]
    );
    assert!(table.differing.is_empty());
    Ok(())
}

#[test]
fn a_node_that_left_the_target_keeps_its_column_after_the_declared_ones() -> TestResult {
    let backend = key("backend");
    let stored = table_in(
        SPACE,
        &[(backend.as_str(), "gone"), (backend.as_str(), "web")],
    )?;
    let nodes = vec![
        answered("web", Some(&stored), Vec::new())?,
        answered("app", Some(&stored), Vec::new())?,
    ];
    let table = table_of(&nodes)?;
    assert_eq!(table.node_names(), ["web", "app", "gone"]);
    let holders = table.networks[0]
        .nodes
        .iter()
        .map(|held| held.node.as_str())
        .collect::<Vec<_>>();
    assert_eq!(holders, ["web", "gone"]);
    Ok(())
}

#[test]
fn a_target_whose_networks_stay_local_has_an_empty_table() {
    assert!(AddressTable::default().is_empty());
    assert!(AddressTable::from_reports(None, &|_| None, &[]).is_empty());
}

#[test]
fn the_table_serializes_under_stable_kebab_case_keys() -> TestResult {
    let nodes = vec![
        answered(
            "web",
            Some(&table_in(SPACE, &[(key("backend").as_str(), "web")])?),
            vec![address("104.204.69.133/32", "eth0")?],
        )?,
        silent("app"),
    ];
    let table = table_of(&nodes)?;
    let value = serde_json::to_value(&table)?;
    assert_eq!(
        value,
        serde_json::json!({
            "space": "10.64.0.0/12",
            "networks": [{
                "name": "backend",
                "key": key("backend"),
                "block": "10.64.0.0/20",
                "internal": false,
                "nodes": [{
                    "node": "web",
                    "slice": "10.64.0.0/24",
                    "network": format!("web-{}", key("backend")),
                }],
            }],
            "held-by-others": [],
            "host-ranges": [
                {
                    "node": "web",
                    "answered": true,
                    "ranges": [{
                        "range": "104.204.69.133/32",
                        "kind": "address",
                        "interface": "eth0",
                    }],
                },
                {
                    "node": "app",
                    "answered": false,
                    "problem": "does not answer on port 22. Check that the host is up",
                    "ranges": [],
                },
            ],
            "differing": [],
        })
    );
    assert_eq!(serde_json::from_value::<AddressTable>(value)?, table);
    Ok(())
}

/// Two environments whose names start alike on the same nodes: each one's
/// table, and its names, are its own (BUGS.md, "Two environments whose
/// names start alike on the same hosts see each other's names").
#[test]
fn an_environment_whose_name_starts_like_another_sees_only_its_own_networks() -> TestResult {
    let environments = ["staging".to_owned(), "staging-eu".to_owned()];
    let of = |environment: &str, name: &str| network_key(project(), environment, name);
    let (staging, eu) = (of("staging", "backend"), of("staging-eu", "backend"));
    let held = table_in(SPACE, &[(staging.as_str(), "web"), (eu.as_str(), "web")])?;
    let ours = |key: &str| environment_network(key, project(), "staging", &environments);
    let table = AddressTable::from_reports(Some(cidr(SPACE)?), &ours, &all_hold(&held)?);
    let names = table
        .networks
        .iter()
        .map(|network| network.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["backend"],
        "not staging-eu's backend as `eu-backend`"
    );
    assert_eq!(table.held_by_others.len(), 1);

    // The names a node reports by the prefix of staging's keys.
    let name = |network: &str| ResolvedName {
        name: "web".to_owned(),
        network: network.to_owned(),
        addresses: Vec::new(),
        lookups: Vec::new(),
        fail_open: false,
    };
    let report = NameReport {
        networks: vec!["backend".to_owned(), "eu-backend".to_owned()],
        names: vec![name("backend"), name("eu-backend")],
        problems: Vec::new(),
    }
    .only_networks(|network| ours(&of("staging", network)).is_some());
    assert_eq!(report.networks, ["backend"]);
    assert_eq!(report.names, [name("backend")]);
    Ok(())
}
