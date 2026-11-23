//! Networks that span cluster nodes: how addresses are given out and
//! merged, the firewall rules and routes a node holds, and what a plan
//! says it changes.

use std::net::Ipv4Addr;

use mm::domain::cluster_network::forward_hook;
use mm::domain::cluster_network::{
    ChangeSet, HostRange, HostRangeKind, NetworkAddresses, NetworkDrift, NetworkDriftKind,
    NetworkNeed, NetworkReport, NetworkTable, NetworkUse, NodeAddresses, NodeNetworkReport,
    NodeNetworkStatus, NodeNetworks, ObservedNetwork, PlannedNode, SealedNetworks, bridge_name,
    environment_network, network_key, plan_networks, plan_networks_retiring, range_conflict,
    space_problem, with_routes,
};
use mm::domain::mesh::{MeshPeer, MeshState};
use mm::domain::repo_target::Ipv4Cidr;
use mm::domain::runtime::RuntimeKind;
use mm::domain::workspace::ProjectId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn cidr(text: &str) -> Result<Ipv4Cidr, String> {
    text.parse()
}

fn settings() -> Result<SealedNetworks, String> {
    Ok(SealedNetworks {
        space: cidr("10.64.0.0/12")?,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table: NetworkTable::default(),
        retired: Vec::new(),
    })
}

fn need<'a>(key: &'a str, node: &'a str) -> NetworkNeed<'a> {
    NetworkNeed {
        key,
        node,
        network: "mm-engine-name",
        internal: false,
    }
}

fn held(slice: &str) -> Result<NodeAddresses, String> {
    Ok(NodeAddresses {
        slice: cidr(slice)?,
        network: "mm-engine-name".to_owned(),
    })
}

fn uses(name: &str, observed: Option<(&str, bool)>) -> NetworkUse {
    NetworkUse {
        name: name.to_owned(),
        key: format!("mm-{name}"),
        network: format!("mm-0123456789ab-{name}"),
        internal: false,
        services: vec!["api".to_owned(), "web".to_owned()],
        observed: observed.map(|(addresses, current)| ObservedNetwork {
            addresses: addresses.to_owned(),
            current,
        }),
    }
}

/// Two networks over three nodes, as a first deployment gives them out.
fn table() -> Result<NetworkTable, String> {
    let space = cidr("10.64.0.0/12")?;
    let mut table = NetworkTable::default();
    for (network, node) in [
        ("mm-backend", "app-1"),
        ("mm-backend", "app-2"),
        ("mm-frontend", "app-2"),
        ("mm-backend", "db-1"),
    ] {
        table.assign(space, &need(network, node))?;
    }
    Ok(table)
}

#[test]
fn a_new_pair_takes_the_lowest_free_block_and_slice() -> TestResult {
    let table = table()?;
    let backend = &table.networks["mm-backend"];
    assert_eq!(backend.block, cidr("10.64.0.0/20")?);
    assert_eq!(backend.nodes["app-1"].slice, cidr("10.64.0.0/24")?);
    assert_eq!(backend.nodes["app-2"].slice, cidr("10.64.1.0/24")?);
    assert_eq!(backend.nodes["db-1"].slice, cidr("10.64.2.0/24")?);
    let frontend = &table.networks["mm-frontend"];
    assert_eq!(frontend.block, cidr("10.64.16.0/20")?);
    // A network one node uses still takes its slice from the cluster space.
    assert_eq!(frontend.nodes["app-2"].slice, cidr("10.64.16.0/24")?);
    table.validate()?;
    assert_eq!(
        table.slices_of("app-2"),
        vec![cidr("10.64.1.0/24")?, cidr("10.64.16.0/24")?]
    );
    Ok(())
}

#[test]
fn an_address_never_changes_once_given() -> TestResult {
    let space = cidr("10.64.0.0/12")?;
    let mut table = table()?;
    let before = table.clone();
    // Asking again gives nothing new, and a node that left keeps its slice
    // reserved: the next node gets the one after it.
    assert!(!table.assign(space, &need("mm-backend", "app-2"))?);
    assert_eq!(table, before);
    table
        .networks
        .get_mut("mm-backend")
        .ok_or("no backend")?
        .nodes
        .remove("app-2");
    assert!(table.assign(space, &need("mm-backend", "app-4"))?);
    assert_eq!(
        table.networks["mm-backend"].nodes["app-4"].slice,
        cidr("10.64.1.0/24")?
    );
    assert!(before.is_kept_by(&before));
    assert!(!before.is_kept_by(&table), "app-2 lost its slice");
    Ok(())
}

#[test]
fn another_address_space_serves_new_networks_and_old_ones_keep_theirs() -> TestResult {
    let mut table = table()?;
    let moved = cidr("10.112.0.0/12")?;
    assert!(table.assign(moved, &need("mm-jobs", "app-1"))?);
    assert_eq!(table.networks["mm-jobs"].block, cidr("10.112.0.0/20")?);
    assert_eq!(
        table.networks["mm-backend"].nodes["app-1"].slice,
        cidr("10.64.0.0/24")?
    );
    // The firewall covers the configured space and the blocks outside it.
    let node = NodeNetworks {
        node: "app-1".to_owned(),
        space: moved,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table,
    };
    assert_eq!(
        node.spaces(),
        vec![moved, cidr("10.64.0.0/20")?, cidr("10.64.16.0/20")?]
    );
    Ok(())
}

#[test]
fn a_full_space_says_how_to_get_more_room() -> TestResult {
    let space = cidr("10.91.0.0/19")?;
    let mut table = NetworkTable::default();
    table.assign(space, &need("mm-a", "app-1"))?;
    table.assign(space, &need("mm-b", "app-1"))?;
    let Err(message) = table.assign(space, &need("mm-c", "app-1")) else {
        return Err("a third network fitted two blocks".into());
    };
    assert!(
        message.contains("cluster.networks 10.91.0.0/19 has no room for network 'mm-c'"),
        "{message}"
    );
    assert!(message.contains("holds 2 network(s)"), "{message}");
    assert!(
        message.contains("Set `cluster.networks` under the target to another private range"),
        "{message}"
    );
    assert!(message.contains("such as 10.64.0.0/12"), "{message}");
    // A space that already is in the default is not offered the default.
    let mut small = NetworkTable::default();
    small.assign(cidr("10.64.0.0/20")?, &need("mm-a", "app-1"))?;
    let Err(message) = small.assign(cidr("10.64.0.0/20")?, &need("mm-b", "app-1")) else {
        return Err("a /20 held two networks".into());
    };
    assert!(message.contains("holds 1 network(s)"), "{message}");
    assert!(message.contains("such as 10.32.0.0/12"), "{message}");
    Ok(())
}

#[test]
fn a_space_too_small_for_one_network_says_what_fits() -> TestResult {
    let Err(message) = space_problem(cidr("10.91.0.0/22")?) else {
        return Err("a /22 held a /20".into());
    };
    assert_eq!(
        message,
        "cluster.networks 10.91.0.0/22 is too small to hold a network: every network takes a /20 (16 nodes, each with room for 253 containers). Use a /20 or larger: a /16 holds 16 networks, and the default 10.64.0.0/12 holds 256"
    );
    assert!(space_problem(cidr("10.91.0.0/21")?).is_err());
    assert!(space_problem(cidr("10.91.0.0/20")?).is_ok());
    assert!(space_problem(cidr("10.64.0.0/12")?).is_ok());
    Ok(())
}

#[test]
fn the_default_space_holds_256_networks_of_16_nodes() -> TestResult {
    let space = mm::domain::repo_target::DEFAULT_CLUSTER_NETWORKS;
    assert_eq!(space, cidr("10.64.0.0/12")?);
    let mut table = NetworkTable::default();
    for index in 0..256 {
        table.assign(space, &need(&format!("mm-{index}"), "app-1"))?;
    }
    assert_eq!(table.networks["mm-255"].block, cidr("10.79.240.0/20")?);
    assert!(table.assign(space, &need("mm-256", "app-1")).is_err());
    // The last node of a network holds the last /24 of its block.
    for index in 2..=16 {
        table.assign(space, &need("mm-255", &format!("app-{index}")))?;
    }
    let last = &table.networks["mm-255"].nodes["app-16"];
    assert_eq!(last.slice, cidr("10.79.255.0/24")?);
    table.validate()?;
    Ok(())
}

#[test]
fn a_network_spans_sixteen_nodes_at_most() -> TestResult {
    let space = cidr("10.64.0.0/12")?;
    let mut table = NetworkTable::default();
    for index in 0..16 {
        table.assign(space, &need("mm-a", &format!("node-{index}")))?;
    }
    let Err(message) = table.assign(space, &need("mm-a", "node-16")) else {
        return Err("a seventeenth node got a slice".into());
    };
    assert!(
        message.contains("already spans 16 nodes") && message.contains("node 'node-16'"),
        "{message}"
    );
    Ok(())
}

#[test]
fn the_tables_of_all_nodes_merge_into_one() -> TestResult {
    let whole = table()?;
    // Each node knows the networks it has seen; together they know all.
    let mut first = whole.clone();
    first.networks.remove("mm-frontend");
    let mut second = whole.clone();
    second
        .networks
        .get_mut("mm-backend")
        .ok_or("no backend")?
        .nodes
        .remove("db-1");
    let merged = NetworkTable::merge([("app-1", &first), ("app-2", &second)])?;
    assert_eq!(merged, whole);
    Ok(())
}

#[test]
fn nodes_that_disagree_are_named() -> TestResult {
    let whole = table()?;
    let mut other = whole.clone();
    other
        .networks
        .get_mut("mm-backend")
        .ok_or("no backend")?
        .nodes
        .insert("app-1".to_owned(), held("10.64.5.0/24")?);
    let Err(message) = NetworkTable::merge([("app-1", &whole), ("db-1", &other)]) else {
        return Err("two slices for one node merged".into());
    };
    assert!(
        message.contains("nodes 'app-1' and 'db-1' disagree about the addresses of node 'app-1' on network 'mm-backend'"),
        "{message}"
    );
    // Two networks that were given the same block on separate nodes.
    let mut clash = NetworkTable::default();
    clash.assign(cidr("10.64.0.0/12")?, &need("mm-other", "db-1"))?;
    let Err(message) = NetworkTable::merge([("app-1", &whole), ("db-1", &clash)]) else {
        return Err("two networks on one block merged".into());
    };
    assert!(message.contains("both hold 10.64.0.0/20"), "{message}");
    Ok(())
}

#[test]
fn a_table_with_foreign_sizes_is_refused() -> TestResult {
    let mut table = table()?;
    table
        .networks
        .get_mut("mm-backend")
        .ok_or("no backend")?
        .nodes
        .insert("app-9".to_owned(), held("10.64.99.0/24")?);
    let Err(message) = table.validate() else {
        return Err("a slice outside its block validated".into());
    };
    assert!(
        message.contains(
            "gives node 'app-9' 10.64.99.0/24, which is not a part of its block 10.64.0.0/20"
        ),
        "{message}"
    );
    // Two nodes on addresses that overlap.
    let mut table = self::table()?;
    table
        .networks
        .get_mut("mm-backend")
        .ok_or("no backend")?
        .nodes
        .insert("app-9".to_owned(), held("10.64.1.128/25")?);
    let Err(message) = table.validate() else {
        return Err("two nodes on one range validated".into());
    };
    assert!(message.contains("to two nodes"), "{message}");
    Ok(())
}

/// The table of an earlier mm: a /22 per network and a /26 per node, out
/// of 10.91.0.0/16.
fn earlier_table() -> Result<NetworkTable, String> {
    let network = |block: &str, slices: &[(&str, &str)]| -> Result<NetworkAddresses, String> {
        Ok(NetworkAddresses {
            block: cidr(block)?,
            internal: false,
            nodes: slices
                .iter()
                .map(|(node, slice)| Ok(((*node).to_owned(), held(slice)?)))
                .collect::<Result<_, String>>()?,
        })
    };
    Ok(NetworkTable {
        networks: [
            (
                "mm-backend".to_owned(),
                network(
                    "10.91.0.0/22",
                    &[("app-1", "10.91.0.0/26"), ("app-2", "10.91.0.64/26")],
                )?,
            ),
            (
                "mm-frontend".to_owned(),
                network("10.91.4.0/22", &[("app-2", "10.91.4.0/26")])?,
            ),
            (
                "other-project".to_owned(),
                network("10.91.8.0/22", &[("app-1", "10.91.8.0/26")])?,
            ),
        ]
        .into(),
    })
}

fn earlier_node(node: &str) -> Result<NodeNetworks, String> {
    Ok(NodeNetworks {
        node: node.to_owned(),
        space: cidr("10.91.0.0/16")?,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table: earlier_table()?,
    })
}

#[test]
fn a_table_in_an_earlier_mms_sizes_still_reads() -> TestResult {
    let earlier = earlier_table()?;
    earlier.validate()?;
    assert_eq!(earlier.outdated().count(), 3);
    assert_eq!(table()?.outdated().count(), 0);
    // The node keeps routing and guarding what it holds until a plan
    // replaces it.
    let node = earlier_node("app-1")?;
    assert_eq!(
        node.route_lines(),
        ["10.91.0.64/26 to app-2", "10.91.4.0/26 to app-2"]
    );
    assert_eq!(
        node.local_network("mm-backend")
            .ok_or("no backend")?
            .gateway,
        Ipv4Addr::new(10, 91, 0, 1)
    );
    // It is not one a plan keeps: the engine's network of that name is
    // one to create again, whether the node read it before or after it
    // stored the plan's addresses. A node's plan must say the same both
    // times, or the apply would find another plan than the reviewed one.
    assert!(node.kept_network("mm-backend").is_none());
    assert!(node.kept_network("mm-nosuch").is_none());
    assert_eq!(
        self::node("app-2")?.kept_network("mm-backend"),
        self::node("app-2")?.local_network("mm-backend")
    );
    assert!(self::node("app-2")?.kept_network("mm-backend").is_some());
    Ok(())
}

#[test]
fn a_plan_gives_a_network_in_earlier_sizes_new_addresses() -> TestResult {
    let earlier = earlier_table()?;
    // The user kept `cluster.networks: 10.91.0.0/16`, so the new blocks
    // come from the space the old ones are in.
    let settings = SealedNetworks {
        space: cidr("10.91.0.0/16")?,
        ..settings()?
    };
    let known = vec![
        ("app-1".to_owned(), earlier.clone()),
        ("app-2".to_owned(), earlier.clone()),
    ];
    let plan = plan_networks(
        &settings,
        &known,
        &[
            planned(
                "app-1",
                Some(earlier_node("app-1")?),
                vec![uses(
                    "backend",
                    Some(("10.91.0.0/22, range 10.91.0.0/26", true)),
                )],
            ),
            planned(
                "app-2",
                Some(earlier_node("app-2")?),
                vec![
                    uses("backend", Some(("10.91.0.0/22, range 10.91.0.64/26", true))),
                    uses("frontend", Some(("10.91.4.0/22, range 10.91.4.0/26", true))),
                ],
            ),
        ],
    )?
    .ok_or("no network plan")?;
    let table = &plan.sealed.table;
    // Clear of every old block: the engines hold those until each network
    // is created again, and the other project still uses its own.
    assert_eq!(table.networks["mm-backend"].block, cidr("10.91.16.0/20")?);
    assert_eq!(table.networks["mm-frontend"].block, cidr("10.91.32.0/20")?);
    assert_eq!(
        table.networks["mm-backend"].nodes["app-2"].slice,
        cidr("10.91.17.0/24")?
    );
    assert_eq!(
        table.networks["other-project"],
        earlier.networks["other-project"]
    );
    assert_eq!(
        plan.nodes[0].networks[0].summary(),
        "backend: created again on 10.91.16.0/24 of 10.91.16.0/20 (was 10.91.0.0/22, range 10.91.0.0/26); restarts api, web"
    );
    assert_eq!(
        plan.nodes[0].routes.removed,
        ["10.91.0.64/26 to app-2", "10.91.4.0/26 to app-2"]
    );
    assert_eq!(
        plan.nodes[0].routes.added,
        ["10.91.17.0/24 to app-2", "10.91.32.0/24 to app-2"]
    );
    // A node may store it although its own addresses change, and a plan
    // made the old way may not undo it.
    assert!(earlier.is_kept_by(table));
    assert!(!table.is_kept_by(&earlier));
    plan.sealed.validate()?;

    // app-2 was left out of that plan and still holds the earlier table:
    // the next plan goes by the node that has today's sizes.
    let merged = NetworkTable::merge([("app-2", &earlier), ("app-1", table)])?;
    assert_eq!(&merged, table);
    let merged = NetworkTable::merge([("app-1", table), ("app-2", &earlier)])?;
    assert_eq!(&merged, table);
    Ok(())
}

fn range(text: &str, kind: HostRangeKind, interface: &str) -> Result<HostRange, String> {
    Ok(HostRange {
        range: cidr(text)?,
        kind,
        interface: Some(interface.to_owned()),
    })
}

#[test]
fn a_range_a_node_already_uses_is_named_with_the_key_to_set() -> TestResult {
    let held = vec![
        (
            "app-1".to_owned(),
            vec![range("203.0.113.0/24", HostRangeKind::Route, "eth0")?],
        ),
        (
            "app-2".to_owned(),
            vec![
                range("203.0.113.0/24", HostRangeKind::Route, "eth0")?,
                range("10.70.0.5/32", HostRangeKind::Address, "eth1")?,
                range("10.70.0.0/20", HostRangeKind::Route, "eth1")?,
                range("10.32.0.0/16", HostRangeKind::Route, "eth2")?,
            ],
        ),
    ];
    let mesh = Some(cidr("10.90.0.0/24")?);
    let networks = Some(cidr("10.64.0.0/12")?);
    assert_eq!(
        range_conflict("production", "app-1", &held, mesh, networks),
        None
    );
    assert_eq!(
        range_conflict("production", "app-2", &held, mesh, networks).as_deref(),
        Some(
            "node 'app-2' already has the route 10.70.0.0/20 through eth1, which overlaps cluster.networks 10.64.0.0/12 (the addresses of networks that span the nodes). Set `cluster.networks` under targets.production.cluster to a private range no node uses, such as 10.112.0.0/12, then plan again"
        )
    );
    // The mesh subnet, with an address alone.
    let held = vec![(
        "db-1".to_owned(),
        vec![range("10.90.0.7/32", HostRangeKind::Address, "tun0")?],
    )];
    assert_eq!(
        range_conflict("production", "db-1", &held, mesh, networks).as_deref(),
        Some(
            "node 'db-1' already has the address 10.90.0.7 on tun0, which overlaps cluster.mesh.subnet 10.90.0.0/24 (the nodes' mesh addresses). Set `cluster.mesh.subnet` under targets.production.cluster to a private range no node uses, such as 10.92.0.0/24, then plan again"
        )
    );
    // A node nobody read is no finding.
    assert_eq!(
        range_conflict("production", "app-9", &held, mesh, networks),
        None
    );
    Ok(())
}

fn node(name: &str) -> Result<NodeNetworks, String> {
    Ok(SealedNetworks {
        table: table()?,
        ..settings()?
    }
    .on_node(name))
}

#[test]
fn a_bridge_name_fits_an_interface_name_and_is_stable() {
    let name = bridge_name("mm-0123456789ab-0123456789ab-production-backend");
    assert_eq!(name.len(), 15);
    assert!(name.starts_with("mmn-"));
    assert_eq!(
        name,
        bridge_name("mm-0123456789ab-0123456789ab-production-backend")
    );
    assert_ne!(
        name,
        bridge_name("mm-0123456789ab-0123456789ab-production-frontend")
    );
}

#[test]
fn a_node_holds_a_bridge_per_network_it_is_on() -> TestResult {
    let node = node("app-2")?;
    let local = node.local();
    assert_eq!(local.len(), 2);
    let backend = node.local_network("mm-backend").ok_or("no backend")?;
    assert_eq!(backend.block, cidr("10.64.0.0/20")?);
    assert_eq!(backend.slice, cidr("10.64.1.0/24")?);
    assert_eq!(backend.gateway, Ipv4Addr::new(10, 64, 1, 1));
    assert_eq!(backend.bridge, bridge_name("mm-backend"));
    assert_eq!(backend.network, "mm-engine-name");
    assert_eq!(
        (backend.mtu, backend.mesh_interface.as_str()),
        (1420, "mm0")
    );
    // db-1 is not on frontend.
    assert!(self::node("db-1")?.local_network("mm-frontend").is_none());
    Ok(())
}

#[test]
fn a_slice_keeps_its_first_and_last_address_from_containers() -> TestResult {
    let space = cidr("10.64.0.0/12")?;
    let mut table = NetworkTable::default();
    for index in 1..=16 {
        table.assign(space, &need("mm-backend", &format!("app-{index}")))?;
    }
    let on = |node: &str| {
        SealedNetworks {
            space,
            mesh_interface: "mm0".to_owned(),
            mtu: 1420,
            table: table.clone(),
            retired: Vec::new(),
        }
        .on_node(node)
        .local_network("mm-backend")
        .ok_or_else(|| format!("{node} holds no part of backend"))
    };
    // A slice in the middle: .0 and .255 are kept back, .1 is the node's,
    // and 253 are left for containers.
    let middle = on("app-2")?;
    assert_eq!(
        middle.reserved(),
        [
            ("first", Ipv4Addr::new(10, 64, 1, 0)),
            ("last", Ipv4Addr::new(10, 64, 1, 255))
        ]
    );
    assert_eq!(
        middle
            .slice
            .hosts()
            .filter(|address| *address != middle.gateway)
            .count(),
        253
    );
    // The block's own first and last address are the engine's to keep.
    assert_eq!(
        on("app-1")?.reserved(),
        [("last", Ipv4Addr::new(10, 64, 0, 255))]
    );
    assert_eq!(
        on("app-16")?.reserved(),
        [("first", Ipv4Addr::new(10, 64, 15, 0))]
    );
    Ok(())
}

#[test]
fn the_other_nodes_slices_are_routed_to_them() -> TestResult {
    let node = node("app-1")?;
    assert_eq!(
        node.route_lines(),
        [
            "10.64.1.0/24 to app-2",
            "10.64.16.0/24 to app-2",
            "10.64.2.0/24 to db-1"
        ]
    );
    let state = MeshState {
        interface: "mm0".to_owned(),
        address: Ipv4Addr::new(10, 90, 0, 1),
        prefix: 24,
        port: 51820,
        mtu: 1420,
        peers: vec![
            MeshPeer {
                name: "app-2".to_owned(),
                public_key: "a".repeat(43) + "=",
                endpoint: "203.0.113.12:51820".to_owned(),
                allowed_ips: vec![cidr("10.90.0.2/32")?],
            },
            MeshPeer {
                name: "db-1".to_owned(),
                public_key: "b".repeat(43) + "=",
                endpoint: "203.0.113.13:51820".to_owned(),
                allowed_ips: vec![cidr("10.90.0.3/32")?],
            },
        ],
    };
    let routed = with_routes(&state, Some(&node));
    assert_eq!(
        routed.peers[0].allowed_ips,
        [
            cidr("10.90.0.2/32")?,
            cidr("10.64.1.0/24")?,
            cidr("10.64.16.0/24")?
        ]
    );
    assert_eq!(
        routed.peers[1].allowed_ips,
        [cidr("10.90.0.3/32")?, cidr("10.64.2.0/24")?]
    );
    // Applying twice adds nothing twice, and no table means no change.
    assert_eq!(with_routes(&state, None), state);
    let again = with_routes(&routed, Some(&node));
    assert_eq!(again, routed);
    Ok(())
}

#[test]
fn the_rules_accept_one_network_over_the_mesh_and_drop_the_rest() -> TestResult {
    let mut node = node("app-2")?;
    node.table
        .networks
        .get_mut("mm-frontend")
        .ok_or("no frontend")?
        .internal = true;
    let firewall = node.firewall();
    let backend = bridge_name("mm-backend");
    let frontend = bridge_name("mm-frontend");
    assert_eq!(
        firewall.filter,
        [
            format!("-A MM-CLUSTER -s 10.64.16.0/20 -d 10.64.16.0/20 -i mm0 -o {frontend} -m comment --comment \"mm network mm-frontend\" -j ACCEPT"),
            format!("-A MM-CLUSTER -s 10.64.16.0/20 -d 10.64.16.0/20 -i {frontend} -o mm0 -m comment --comment \"mm network mm-frontend\" -j ACCEPT"),
            "-A MM-CLUSTER -m conntrack --ctstate RELATED,ESTABLISHED -m comment --comment \"mm replies\" -j RETURN".to_owned(),
            "-A MM-CLUSTER -m conntrack --ctstate DNAT -m comment --comment \"mm published ports\" -j RETURN".to_owned(),
            format!("-A MM-CLUSTER -s 10.64.0.0/20 -d 10.64.0.0/20 -i mm0 -o {backend} -m comment --comment \"mm network mm-backend\" -j ACCEPT"),
            format!("-A MM-CLUSTER -s 10.64.0.0/20 -d 10.64.0.0/20 -i {backend} -o mm0 -m comment --comment \"mm network mm-backend\" -j ACCEPT"),
            format!("-A MM-CLUSTER -i {backend} -o {backend} -m comment --comment \"mm network mm-backend\" -j RETURN"),
            format!("-A MM-CLUSTER -i {frontend} -o {frontend} -m comment --comment \"mm network mm-frontend\" -j RETURN"),
            format!("-A MM-CLUSTER -i {frontend} -m comment --comment \"mm internal network mm-frontend\" -j DROP"),
            "-A MM-CLUSTER -d 10.64.0.0/12 -m comment --comment \"mm cluster networks\" -j DROP".to_owned(),
            "-A MM-CLUSTER -s 10.64.0.0/12 -o mm0 -m comment --comment \"mm cluster networks\" -j DROP".to_owned(),
        ]
    );
    // Only the network with a way out is given the node's address for it;
    // inside the network and over the mesh a container keeps its own.
    assert_eq!(
        firewall.nat,
        [
            "-A MM-CLUSTER-NAT -s 10.64.1.0/24 -d 10.64.0.0/20 -m comment --comment \"mm network mm-backend\" -j RETURN",
            "-A MM-CLUSTER-NAT -s 10.64.1.0/24 -o mm0 -m comment --comment \"mm network mm-backend\" -j RETURN",
            "-A MM-CLUSTER-NAT -s 10.64.1.0/24 -m comment --comment \"mm network mm-backend\" -j MASQUERADE",
        ]
    );
    let input = firewall.restore_input(forward_hook(RuntimeKind::DockerCompose));
    assert!(
        input.starts_with("*filter\n:MM-CLUSTER - [0:0]\n-A MM-CLUSTER "),
        "{input}"
    );
    // On a Podman host the same rules live in the mangle table, which every
    // forwarded packet passes before netavark's filter rules.
    let podman = firewall.restore_input(forward_hook(RuntimeKind::PodmanCompose));
    assert_eq!(podman, input.replacen("*filter\n", "*mangle\n", 1));
    assert!(
        input.contains("COMMIT\n*nat\n:MM-CLUSTER-NAT - [0:0]\n-A MM-CLUSTER-NAT "),
        "{input}"
    );
    assert!(input.ends_with("-j MASQUERADE\nCOMMIT\n"), "{input}");
    Ok(())
}

#[test]
fn a_node_without_networks_still_guards_the_cluster_space() -> TestResult {
    let firewall = settings()?.on_node("app-1").firewall();
    assert_eq!(firewall.filter.len(), 4);
    assert!(firewall.nat.is_empty());
    Ok(())
}

fn planned(name: &str, stored: Option<NodeNetworks>, uses: Vec<NetworkUse>) -> PlannedNode {
    PlannedNode {
        node: name.to_owned(),
        stored,
        uses,
        drift: Vec::new(),
    }
}

#[test]
fn a_first_plan_gives_addresses_and_shows_routes_and_rules() -> TestResult {
    let plan = plan_networks(
        &settings()?,
        &[],
        &[
            planned("app-1", None, vec![uses("backend", None)]),
            planned("app-2", None, vec![uses("backend", None)]),
            planned("db-1", None, Vec::new()),
        ],
    )?
    .ok_or("no network plan")?;
    assert_eq!(plan.sealed.table.networks["mm-backend"].nodes.len(), 2);
    let first = &plan.nodes[0];
    assert_eq!(
        first.networks[0].summary(),
        "backend: gets 10.64.0.0/24 of 10.64.0.0/20"
    );
    assert!(first.networks[0].assigned && first.networks[0].replaces.is_none());
    assert_eq!(first.routes.added, ["10.64.1.0/24 to app-2"]);
    assert_eq!(first.firewall.added.len(), 10);
    assert!(first.firewall.removed.is_empty());
    // A node that runs nothing on a network still learns the routes and
    // guards the space.
    let third = &plan.nodes[2];
    assert!(third.networks.is_empty());
    assert_eq!(
        third.routes.added,
        ["10.64.0.0/24 to app-1", "10.64.1.0/24 to app-2"]
    );
    assert_eq!(third.firewall.added.len(), 4);
    assert!(plan.changes());
    Ok(())
}

#[test]
fn planning_again_changes_nothing() -> TestResult {
    let first = plan_networks(
        &settings()?,
        &[],
        &[
            planned("app-1", None, vec![uses("backend", None)]),
            planned("app-2", None, vec![uses("backend", None)]),
        ],
    )?
    .ok_or("no network plan")?;
    let stored = |node: &str| Some(first.sealed.on_node(node));
    let known = vec![("app-1".to_owned(), first.sealed.table.clone())];
    let again = plan_networks(
        &settings()?,
        &known,
        &[
            planned(
                "app-1",
                stored("app-1"),
                vec![uses(
                    "backend",
                    Some(("10.64.0.0/20, range 10.64.0.0/24", true)),
                )],
            ),
            planned(
                "app-2",
                stored("app-2"),
                vec![uses(
                    "backend",
                    Some(("10.64.0.0/20, range 10.64.1.0/24", true)),
                )],
            ),
        ],
    )?
    .ok_or("no network plan")?;
    assert_eq!(again.sealed, first.sealed);
    assert!(!again.changes());
    assert_eq!(
        again.nodes[1].networks[0].summary(),
        "backend: keeps 10.64.1.0/24 of 10.64.0.0/20"
    );
    Ok(())
}

#[test]
fn a_host_local_network_is_created_again_and_its_services_restart() -> TestResult {
    let plan = plan_networks(
        &settings()?,
        &[],
        &[planned(
            "app-1",
            None,
            vec![uses("backend", Some(("172.18.0.0/16", false)))],
        )],
    )?
    .ok_or("no network plan")?;
    let network = &plan.nodes[0].networks[0];
    assert_eq!(network.replaces.as_deref(), Some("172.18.0.0/16"));
    assert_eq!(network.restarts, ["api", "web"]);
    assert_eq!(
        network.summary(),
        "backend: created again on 10.64.0.0/24 of 10.64.0.0/20 (was 172.18.0.0/16); restarts api, web"
    );
    Ok(())
}

#[test]
fn a_network_that_drifted_is_created_again_on_the_addresses_it_has() -> TestResult {
    let first = plan_networks(
        &settings()?,
        &[],
        &[planned("app-1", None, vec![uses("backend", None)])],
    )?
    .ok_or("no network plan")?;
    let mut node = planned(
        "app-1",
        Some(first.sealed.on_node("app-1")),
        vec![uses("backend", Some(("172.30.0.0/16", false)))],
    );
    node.drift = vec!["missing rule `-A MM-CLUSTER …`".to_owned()];
    let plan = plan_networks(
        &settings()?,
        &[("app-1".to_owned(), first.sealed.table.clone())],
        &[node],
    )?
    .ok_or("no network plan")?;
    let network = &plan.nodes[0].networks[0];
    assert!(!network.assigned);
    assert_eq!(network.slice, cidr("10.64.0.0/24")?);
    assert_eq!(network.replaces.as_deref(), Some("172.30.0.0/16"));
    assert_eq!(plan.nodes[0].repairs.len(), 1);
    assert!(plan.changes());
    Ok(())
}

#[test]
fn a_node_left_out_of_the_plan_keeps_its_addresses() -> TestResult {
    // db-1 is left out and does not answer; app-1 remembers its slice.
    let known = vec![("app-1".to_owned(), table()?)];
    let plan = plan_networks(
        &settings()?,
        &known,
        &[planned(
            "app-1",
            Some(node("app-1")?),
            vec![uses("frontend", None)],
        )],
    )?
    .ok_or("no network plan")?;
    assert_eq!(
        plan.sealed.table.networks["mm-backend"].nodes["db-1"].slice,
        cidr("10.64.2.0/24")?
    );
    assert_eq!(
        plan.sealed.table.networks["mm-frontend"].nodes["app-1"].slice,
        cidr("10.64.17.0/24")?
    );
    assert_eq!(plan.nodes[0].routes, ChangeSet::default());
    assert_eq!(
        plan.nodes[0].firewall.added.len(),
        6,
        "{:?}",
        plan.nodes[0].firewall
    );
    Ok(())
}

#[test]
fn no_network_and_no_table_means_no_network_section() -> TestResult {
    assert!(plan_networks(&settings()?, &[], &[planned("app-1", None, Vec::new())])?.is_none());
    Ok(())
}

#[test]
fn drift_is_one_finding_per_node_with_what_to_run() {
    let report = NetworkReport::new(
        "production".to_owned(),
        "production",
        vec![
            NodeNetworkReport {
                name: "app-1".to_owned(),
                reachable: true,
                problem: None,
                status: Some(NodeNetworkStatus::default()),
            },
            NodeNetworkReport {
                name: "app-2".to_owned(),
                reachable: true,
                problem: None,
                status: Some(NodeNetworkStatus {
                    node: Some("app-2".to_owned()),
                    stored: None,
                    host_ranges: Vec::new(),
                    drift: vec![
                        NetworkDrift::new(
                            NetworkDriftKind::Firewall,
                            "the chain MM-CLUSTER is missing",
                        ),
                        NetworkDrift::new(
                            NetworkDriftKind::Route,
                            "10.64.0.0/24 is not routed to app-1 through the mesh",
                        ),
                    ],
                }),
            },
        ],
    );
    assert_eq!(report.problems.len(), 1);
    let problem = &report.problems[0];
    assert_eq!(problem.code, "MM_NETWORK_DRIFT");
    assert!(
        problem.message.contains("node 'app-2'")
            && problem
                .message
                .contains("the chain MM-CLUSTER is missing; 10.64.0.0/24 is not routed"),
        "{}",
        problem.message
    );
    let help = problem.help.as_deref().unwrap_or_default();
    assert!(
        help.contains("`mm deploy plan -e production`")
            && help.contains("systemctl restart mm-network"),
        "{help}"
    );
}

#[test]
fn the_table_is_stored_as_kebab_case_json() -> TestResult {
    let node = node("app-1")?;
    let json = serde_json::to_value(&node)?;
    assert_eq!(json["mesh-interface"], "mm0");
    assert_eq!(
        json["table"]["networks"]["mm-backend"]["nodes"]["app-1"]["slice"],
        "10.64.0.0/24"
    );
    let read: NodeNetworks = serde_json::from_value(json)?;
    assert_eq!(read, node);
    Ok(())
}

#[test]
fn a_network_belongs_to_the_longest_environment_its_name_starts_with() {
    let project = ProjectId(uuid::Uuid::from_u128(7));
    let environments = ["staging".to_owned(), "staging-eu".to_owned()];
    let key = |environment: &str, name: &str| network_key(project, environment, name);
    assert_eq!(
        environment_network(
            &key("staging", "backend"),
            project,
            "staging",
            &environments
        ),
        Some("backend".to_owned())
    );
    // `staging-eu`'s backend is not `staging`'s network `eu-backend`.
    assert_eq!(
        environment_network(
            &key("staging-eu", "backend"),
            project,
            "staging",
            &environments
        ),
        None
    );
    assert_eq!(
        environment_network(
            &key("staging-eu", "backend"),
            project,
            "staging-eu",
            &environments
        ),
        Some("backend".to_owned())
    );
    // Keys carry the first twelve characters of the project's id.
    let other = ProjectId(uuid::Uuid::from_u128(8 << 100));
    assert_eq!(
        environment_network(
            &network_key(other, "staging", "backend"),
            project,
            "staging",
            &environments
        ),
        None
    );
}

#[test]
fn a_network_no_service_uses_any_more_leaves_once_the_rollout_finished() -> TestResult {
    let project = ProjectId(uuid::Uuid::from_u128(7));
    let environments = ["production".to_owned(), "production-eu".to_owned()];
    let key = |environment: &str, name: &str| network_key(project, environment, name);
    let use_of = |environment: &str, name: &str| NetworkUse {
        key: key(environment, name),
        ..uses(name, None)
    };
    // production ran backend and sealed on both nodes; production-eu runs
    // its backend on app-1, and another project runs one on app-2.
    let first = plan_networks(
        &settings()?,
        &[],
        &[
            planned(
                "app-1",
                None,
                vec![
                    use_of("production", "backend"),
                    use_of("production", "sealed"),
                    use_of("production-eu", "backend"),
                ],
            ),
            planned(
                "app-2",
                None,
                vec![
                    use_of("production", "backend"),
                    use_of("production", "sealed"),
                ],
            ),
        ],
    )?
    .ok_or("no network plan")?;
    let known = vec![("app-1".to_owned(), first.sealed.table.clone())];
    let ours = |found: &str| environment_network(found, project, "production", &environments);
    let stored = |node: &str| Some(first.sealed.on_node(node));
    let next = |nodes: &[PlannedNode]| {
        plan_networks_retiring(&settings()?, &known, nodes, &ours)?
            .ok_or_else(|| "no network plan".to_owned())
    };

    // Now production declares backend only.
    let plan = next(&[
        planned(
            "app-1",
            stored("app-1"),
            vec![use_of("production", "backend")],
        ),
        planned(
            "app-2",
            stored("app-2"),
            vec![use_of("production", "backend")],
        ),
    ])?;
    assert_eq!(plan.sealed.retired, [key("production", "sealed")]);
    assert!(
        !plan
            .sealed
            .table
            .networks
            .contains_key(&key("production", "sealed"))
    );
    assert!(
        plan.sealed
            .table
            .networks
            .contains_key(&key("production-eu", "backend")),
        "another environment's network stays"
    );
    assert_eq!(plan.nodes[0].removed, ["sealed"]);
    assert!(
        plan.nodes[0]
            .firewall
            .removed
            .iter()
            .any(|rule| rule.contains(&key("production", "sealed"))),
        "{:?}",
        plan.nodes[0].firewall
    );
    assert!(plan.changes());

    // Until its part of the rollout finished, a node keeps what the plan
    // retires: the old containers are on it.
    let at_start = plan
        .sealed
        .on_node_keeping("app-1", stored("app-1").as_ref());
    assert!(
        at_start
            .table
            .networks
            .contains_key(&key("production", "sealed"))
    );
    assert!(
        stored("app-1")
            .ok_or("nothing stored")?
            .table
            .is_kept_by(&at_start.table),
        "the step that stores the plan's table refuses nothing"
    );

    // A node outside the plan that holds a part of it keeps it everywhere.
    let partial = next(&[planned(
        "app-1",
        stored("app-1"),
        vec![use_of("production", "backend")],
    )])?;
    assert!(partial.sealed.retired.is_empty());

    // A plan of production-eu takes nothing of production's.
    let theirs = |found: &str| environment_network(found, project, "production-eu", &environments);
    let eu = plan_networks_retiring(
        &settings()?,
        &known,
        &[
            planned(
                "app-1",
                stored("app-1"),
                vec![use_of("production-eu", "backend")],
            ),
            planned("app-2", stored("app-2"), Vec::new()),
        ],
        &theirs,
    )?
    .ok_or("no network plan")?;
    assert!(eu.sealed.retired.is_empty(), "{:?}", eu.sealed.retired);
    Ok(())
}

#[test]
fn a_host_refuses_a_project_whose_network_key_another_project_has() -> TestResult {
    use mm::domain::cluster_network::project_key;
    use mm::domain::host_policy::{HostPolicy, ProjectEnrollment};

    let live: ProjectId = ProjectId("019c527b-6242-7000-8000-000000001d17".parse()?);
    let twin: ProjectId = ProjectId("019c527b-6242-7000-8000-000000001df0".parse()?);
    let other: ProjectId = ProjectId("019c527b-6243-7000-8000-000000001d17".parse()?);
    assert_eq!(project_key(live), "019c527b6242");
    assert_eq!(project_key(live), project_key(twin));
    assert_eq!(
        network_key(live, "production", "backend"),
        "019c527b6242-production-backend",
        "the names running hosts hold do not change"
    );
    let enrollment = |project_id: ProjectId, name: &str| ProjectEnrollment {
        project_id,
        name: name.to_owned(),
        origin: String::new(),
        origin_host_key: None,
        delivery: mm::domain::delivery::Delivery::Push,
        environments: std::collections::BTreeSet::from(["production".to_owned()]),
        break_glass: vec![mm::domain::host_policy::BreakGlassKey {
            name: "ops".to_owned(),
            key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"
                .to_owned(),
        }],
    };
    let mut policy = HostPolicy::empty();
    policy.enroll(&enrollment(live, "shop"))?;
    let before = policy.clone();
    let refused = policy
        .enroll(&enrollment(twin, "blog"))
        .err()
        .unwrap_or_default();
    for part in [
        "project 'blog' (019c527b-6242-7000-8000-000000001df0)",
        "project 'shop' (019c527b-6242-7000-8000-000000001d17)",
        "019c527b6242",
        "set `project.id` in its mm.yaml",
        "mm target setup plan",
    ] {
        assert!(refused.contains(part), "{part}: {refused}");
    }
    assert_eq!(policy, before, "a refused enrollment changes nothing");
    policy.enroll(&enrollment(live, "shop"))?;
    policy.enroll(&enrollment(other, "blog"))?;
    assert_eq!(policy.projects.len(), 2);
    Ok(())
}
