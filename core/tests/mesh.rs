//! The mesh between a cluster target's nodes: peer sets, the plan a setup
//! shows, key rotation, and what a status says is wrong.

use std::net::Ipv4Addr;

use base64::Engine;
use mm::domain::diagnostic::Severity;
use mm::domain::mesh::*;
use mm::domain::repo_target::{Ipv4Cidr, WireguardMesh};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A 44-character key whose bytes are all `byte`.
fn key(byte: u8) -> String {
    base64::engine::general_purpose::STANDARD.encode([byte; 32])
}

fn member(name: &str, last: u8, public_key: Option<String>) -> MeshMember {
    MeshMember {
        name: name.to_owned(),
        address: Ipv4Addr::new(10, 90, 0, last),
        endpoint: format!("203.0.113.{}", 10 + last),
        public_key,
        routed: Vec::new(),
    }
}

/// Three nodes that all have a key.
fn members() -> Vec<MeshMember> {
    vec![
        member("app-1", 1, Some(key(1))),
        member("app-2", 2, Some(key(2))),
        member("db-1", 3, Some(key(3))),
    ]
}

fn held(members: &[MeshMember], node: &str) -> Vec<MeshPeer> {
    peer_set(51820, members, node)
        .iter()
        .filter_map(PlannedPeer::resolved)
        .collect()
}

fn actions(plans: &[MeshPeerPlan]) -> Vec<(&str, PeerAction)> {
    plans
        .iter()
        .map(|plan| (plan.name.as_str(), plan.action))
        .collect()
}

#[test]
fn a_node_peers_with_every_other_node_at_its_endpoint_and_mesh_address() {
    let peers = peer_set(51820, &members(), "app-2");
    assert_eq!(
        peers
            .iter()
            .map(|peer| (
                peer.name.as_str(),
                peer.endpoint.as_str(),
                peer.allowed_ips
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ))
            .collect::<Vec<_>>(),
        [
            ("app-1", "203.0.113.11:51820", "10.90.0.1/32".to_owned()),
            ("db-1", "203.0.113.13:51820", "10.90.0.3/32".to_owned()),
        ],
        "never itself, in the target's order"
    );
    assert_eq!(peers[0].public_key, Some(key(1)));
}

#[test]
fn prefixes_routed_to_a_node_join_its_allowed_addresses_on_every_peer() -> TestResult {
    let mut members = members();
    // Given out of order and twice: the peer set is the same either way.
    members[2].routed = vec![
        "10.91.7.0/24".parse()?,
        "10.91.2.0/24".parse()?,
        "10.91.7.0/24".parse()?,
    ];
    for node in ["app-1", "app-2"] {
        let peers = peer_set(51820, &members, node);
        let database = peers
            .iter()
            .find(|peer| peer.name == "db-1")
            .ok_or("no db-1")?;
        assert_eq!(
            database
                .allowed_ips
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["10.90.0.3/32", "10.91.2.0/24", "10.91.7.0/24"],
            "{node}"
        );
    }
    // The node itself routes nothing extra to the others.
    assert!(
        peer_set(51820, &members, "db-1")
            .iter()
            .all(|peer| peer.allowed_ips.len() == 1)
    );
    Ok(())
}

#[test]
fn endpoints_carry_the_mesh_port_and_bracket_ipv6() {
    assert_eq!(endpoint_with_port("node.test", 51900), "node.test:51900");
    assert_eq!(
        endpoint_with_port("2001:db8::7", 51820),
        "[2001:db8::7]:51820"
    );
    assert_eq!(endpoint_host("node.test:51900"), "node.test");
    assert_eq!(endpoint_host("[2001:db8::7]:51820"), "2001:db8::7");
}

#[test]
fn a_node_without_a_mesh_adds_every_peer() {
    let plans = plan_peers(None, &peer_set(51820, &members(), "app-1"));
    assert_eq!(
        actions(&plans),
        [("app-2", PeerAction::Add), ("db-1", PeerAction::Add)]
    );
    assert!(plans.iter().all(|plan| plan.previous.is_none()));
    assert!(
        plans[0]
            .summary()
            .starts_with("+ app-2: 10.90.0.2/32 at 203.0.113.12:51820")
    );
}

#[test]
fn an_unchanged_peer_set_plans_no_change() {
    let members = members();
    let current = held(&members, "app-1");
    let plans = plan_peers(Some(&current), &peer_set(51820, &members, "app-1"));
    assert_eq!(
        actions(&plans),
        [("app-2", PeerAction::Keep), ("db-1", PeerAction::Keep)]
    );
}

#[test]
fn adding_and_removing_a_node_changes_one_entry_on_every_other_node() {
    let before = members();
    let mut after = members();
    after.push(member("app-4", 4, None));
    for node in ["app-1", "app-2", "db-1"] {
        let current = held(&before, node);
        let plans = plan_peers(Some(&current), &peer_set(51820, &after, node));
        let added = plans
            .iter()
            .filter(|plan| plan.action != PeerAction::Keep)
            .collect::<Vec<_>>();
        assert_eq!(added.len(), 1, "{node}");
        assert_eq!(
            (added[0].name.as_str(), added[0].action),
            ("app-4", PeerAction::Add)
        );
        assert!(
            added[0].summary().contains("made on app-4 during apply"),
            "a new node's key is not known yet: {}",
            added[0].summary()
        );
    }
    // Removing db-1: the two that stay drop it and keep each other.
    let after = &before[..2];
    for node in ["app-1", "app-2"] {
        let current = held(&before, node);
        let plans = plan_peers(Some(&current), &peer_set(51820, after, node));
        let (kept, removed): (Vec<_>, Vec<_>) = plans
            .iter()
            .partition(|plan| plan.action == PeerAction::Keep);
        assert_eq!(kept.len(), 1, "{node}");
        assert_eq!(actions(&plans)[1], ("db-1", PeerAction::Remove), "{node}");
        assert_eq!(removed[0].summary(), "- db-1: removed");
        assert_eq!(
            removed[0]
                .previous
                .as_ref()
                .map(|peer| peer.public_key.clone()),
            Some(key(3))
        );
    }
}

#[test]
fn a_changed_peer_says_what_changes() -> TestResult {
    let before = members();
    let current = held(&before, "app-1");
    let mut after = members();
    after[1].public_key = Some(key(9));
    after[2].endpoint = "198.51.100.3".to_owned();
    after[2].routed = vec!["10.91.3.0/24".parse()?];
    let plans = plan_peers(Some(&current), &peer_set(51820, &after, "app-1"));
    assert_eq!(
        actions(&plans),
        [("app-2", PeerAction::Change), ("db-1", PeerAction::Change)]
    );
    let rekeyed = plans[0].summary();
    assert!(rekeyed.starts_with("~ app-2: key "), "{rekeyed}");
    assert!(
        !rekeyed.contains("endpoint") && !rekeyed.contains("allowed"),
        "{rekeyed}"
    );
    let moved = plans[1].summary();
    assert!(
        moved.contains("endpoint 203.0.113.13:51820 -> 198.51.100.3:51820"),
        "{moved}"
    );
    assert!(
        moved.contains("allowed 10.90.0.3/32 -> 10.90.0.3/32,10.91.3.0/24"),
        "{moved}"
    );
    assert!(!moved.contains("key"), "{moved}");

    // A peer whose node could not be read: its key is confirmed at apply.
    after[1].public_key = None;
    let plans = plan_peers(Some(&current), &peer_set(51820, &after, "app-1"));
    assert_eq!(plans[0].action, PeerAction::Change);
    assert!(
        plans[0]
            .summary()
            .contains("the key app-2 reports during apply"),
        "{}",
        plans[0].summary()
    );
    Ok(())
}

#[test]
fn a_left_out_node_keeps_the_entry_each_node_already_holds() {
    let members = members();
    let current = held(&members, "app-1");
    // db-1 is left out and its key cannot be read now.
    let mut unread = members.clone();
    unread[2].public_key = None;
    let desired = desired_peers(51820, &unread, "app-1", &["db-1"], &current);
    assert_eq!(desired.len(), 2);
    assert_eq!(desired[1].public_key, Some(key(3)), "kept as it is");
    assert_eq!(
        actions(&plan_peers(Some(&current), &desired)),
        [("app-2", PeerAction::Keep), ("db-1", PeerAction::Keep)]
    );
    // A node that never had the left-out node as a peer gets no entry.
    let desired = desired_peers(51820, &unread, "app-1", &["db-1"], &current[..1]);
    assert_eq!(
        desired
            .iter()
            .map(|peer| peer.name.as_str())
            .collect::<Vec<_>>(),
        ["app-2"]
    );
}

fn default_mesh() -> WireguardMesh {
    WireguardMesh::default()
}

#[test]
fn a_node_plan_starts_from_what_the_node_holds() -> TestResult {
    let mesh = default_mesh();
    let members = members();
    // Not read (a new SSH host key): everything is new.
    let unread = plan_node(&mesh, &members, &members[0], &[], None);
    assert_eq!(unread.origin, MeshNodeOrigin::New);
    assert_eq!(unread.endpoint, "203.0.113.11:51820");
    assert!(unread.changes());
    assert_eq!(
        actions(&unread.peers),
        [("app-2", PeerAction::Add), ("db-1", PeerAction::Add)]
    );

    // In place: nothing changes.
    let facts = MeshFacts {
        public_key: Some(key(1)),
        state: Some(MeshState::new(
            &mesh,
            members[0].address,
            held(&members, "app-1"),
        )),
        ..MeshFacts::default()
    };
    let current = plan_node(&mesh, &members, &members[0], &[], Some(&facts));
    assert_eq!(current.origin, MeshNodeOrigin::Current);
    assert!(!current.changes(), "{current:?}");

    // Another MTU and port in the target: the interface changes, no peer does.
    let tuned = WireguardMesh { mtu: 1380, ..mesh };
    let changed = plan_node(&tuned, &members, &members[0], &[], Some(&facts));
    assert_eq!(changed.interface_changes, ["mtu 1420 -> 1380"]);
    assert!(changed.changes());
    assert!(
        changed
            .peers
            .iter()
            .all(|peer| peer.action == PeerAction::Keep)
    );
    Ok(())
}

#[test]
fn a_host_with_the_earlier_wg0_is_planned_as_a_replacement_that_keeps_its_peers() {
    let mesh = default_mesh();
    let members = members();
    // The earlier configuration recorded no peer names.
    let mut peers = held(&members, "app-1");
    for peer in &mut peers {
        peer.name.clear();
    }
    let mut state = MeshState::new(&mesh, members[0].address, peers);
    state.interface = LEGACY_MESH_INTERFACE.to_owned();
    let facts = MeshFacts {
        public_key: Some(key(1)),
        legacy_interface: Some(LEGACY_MESH_INTERFACE.to_owned()),
        state: Some(state),
        ..MeshFacts::default()
    };
    let plan = plan_node(&mesh, &members, &members[0], &[], Some(&facts));
    assert_eq!(plan.origin, MeshNodeOrigin::Legacy);
    assert_eq!(plan.interface_changes, ["interface wg0 -> mm0"]);
    assert!(plan.changes());
    assert_eq!(
        actions(&plan.peers),
        [("app-2", PeerAction::Keep), ("db-1", PeerAction::Keep)],
        "peers are named by their mesh address and stay as they are"
    );

    // Before the new mmd is installed only the key is known.
    let facts = MeshFacts {
        public_key: Some(key(1)),
        legacy_interface: Some(LEGACY_MESH_INTERFACE.to_owned()),
        ..MeshFacts::default()
    };
    let plan = plan_node(&mesh, &members, &members[0], &[], Some(&facts));
    assert_eq!(plan.origin, MeshNodeOrigin::Legacy);
    assert_eq!(
        actions(&plan.peers),
        [("app-2", PeerAction::Add), ("db-1", PeerAction::Add)]
    );
}

#[test]
fn a_peer_no_node_matches_is_named_by_its_address() -> TestResult {
    let mut peers = vec![MeshPeer {
        name: String::new(),
        public_key: key(7),
        endpoint: "198.51.100.9:51820".to_owned(),
        allowed_ips: vec!["10.90.0.9/32".parse()?],
    }];
    name_peers(&mut peers, &members());
    assert_eq!(peers[0].name, "10.90.0.9");
    Ok(())
}

#[test]
fn a_state_mmd_must_not_apply_is_refused() -> TestResult {
    let mesh = default_mesh();
    let members = members();
    let valid = MeshState::new(&mesh, members[0].address, held(&members, "app-1"));
    valid.validate()?;
    assert_eq!(valid.subnet().to_string(), "10.90.0.0/24");
    assert_eq!(valid.interface, "mm0");

    type Change = (&'static str, fn(&mut MeshState));
    let changes: [Change; 8] = [
        ("another interface", |state| {
            state.interface = "eth0".to_owned()
        }),
        ("a short key", |state| {
            state.peers[0].public_key = "AAAA".to_owned()
        }),
        ("a key that is not base64", |state| {
            state.peers[0].public_key = "!".repeat(44);
        }),
        ("an endpoint without a port", |state| {
            state.peers[0].endpoint = "203.0.113.12".to_owned();
        }),
        ("an option as an endpoint", |state| {
            state.peers[0].endpoint = "-oProxyCommand=x:1".to_owned();
        }),
        ("the same key twice", |state| {
            state.peers[1].public_key = state.peers[0].public_key.clone();
        }),
        ("the node's own address", |state| {
            state.peers[0].allowed_ips = vec![Ipv4Cidr {
                address: state.address,
                prefix: 32,
            }];
        }),
        ("an MTU no link carries", |state| state.mtu = 100),
    ];
    for (what, change) in changes {
        let mut state = valid.clone();
        change(&mut state);
        assert!(state.validate().is_err(), "{what} was accepted");
    }
    Ok(())
}

#[test]
fn neither_the_state_nor_the_facts_have_a_field_for_a_private_key() -> TestResult {
    let mesh = default_mesh();
    let members = members();
    let state = MeshState::new(&mesh, members[0].address, held(&members, "app-1"));
    let facts = MeshFacts {
        public_key: Some(key(1)),
        pending_public_key: Some(key(8)),
        legacy_interface: None,
        state: Some(state.clone()),
        host_ranges: Vec::new(),
        release: None,
    };
    for json in [
        serde_json::to_string(&state)?,
        serde_json::to_string(&facts)?,
    ] {
        assert!(!json.to_ascii_lowercase().contains("private"), "{json}");
    }
    // A state that carries one is not a state: unknown fields are refused.
    let mut with_key = serde_json::to_value(&state)?;
    with_key["private-key"] = serde_json::Value::String(key(5));
    assert!(serde_json::from_value::<MeshState>(with_key).is_err());
    Ok(())
}

#[test]
fn a_rotation_plan_hash_covers_the_keys_it_replaces() -> TestResult {
    let mut plan = MeshRotationPlan {
        environment: "production".to_owned(),
        target: "production".to_owned(),
        node: "app-2".to_owned(),
        address: Ipv4Addr::new(10, 90, 0, 2),
        current_public_key: key(2),
        pending_public_key: None,
        peers: vec![
            MeshRotationPeer {
                node: "app-1".to_owned(),
                holds: Some(key(2)),
            },
            MeshRotationPeer {
                node: "db-1".to_owned(),
                holds: Some(key(2)),
            },
        ],
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash()?;
    assert_eq!(
        plan.apply_command(),
        format!(
            "mm --environment production target mesh rotate app-2 --plan {}",
            plan.plan_hash
        )
    );
    let mut rotated_elsewhere = plan.clone();
    rotated_elsewhere.current_public_key = key(6);
    assert_ne!(rotated_elsewhere.canonical_hash()?, plan.plan_hash);
    let mut interrupted = plan.clone();
    interrupted.pending_public_key = Some(key(7));
    assert_ne!(interrupted.canonical_hash()?, plan.plan_hash);
    Ok(())
}

fn peer_status(name: &str, byte: u8, last: u8, age: Option<u64>) -> MeshPeerStatus {
    MeshPeerStatus {
        name: Some(name.to_owned()),
        public_key: key(byte),
        endpoint: Some(format!("203.0.113.{}:51820", 10 + last)),
        configured_endpoint: Some(format!("203.0.113.{}:51820", 10 + last)),
        allowed_ips: vec![format!("10.90.0.{last}/32")],
        handshake_age_seconds: age,
        received_bytes: 1,
        sent_bytes: 1,
        on_interface: true,
    }
}

/// `name` (key `byte`, address `.last`) with the given peers.
fn node_report(name: &str, byte: u8, last: u8, peers: Vec<MeshPeerStatus>) -> MeshNodeReport {
    MeshNodeReport {
        name: name.to_owned(),
        host: format!("203.0.113.{}", 10 + last),
        reachable: true,
        problem: None,
        status: Some(MeshNodeStatus {
            node: Some(name.to_owned()),
            interface: Some(MESH_INTERFACE.to_owned()),
            up: true,
            address: Some(Ipv4Addr::new(10, 90, 0, last)),
            port: Some(51820),
            mtu: Some(1420),
            public_key: Some(key(byte)),
            peers,
            guard_drift: Vec::new(),
        }),
    }
}

/// Three nodes that all heard from each other `age` seconds ago.
fn healthy(age: u64) -> Vec<MeshNodeReport> {
    vec![
        node_report(
            "app-1",
            1,
            1,
            vec![
                peer_status("app-2", 2, 2, Some(age)),
                peer_status("db-1", 3, 3, Some(age)),
            ],
        ),
        node_report(
            "app-2",
            2,
            2,
            vec![
                peer_status("app-1", 1, 1, Some(age)),
                peer_status("db-1", 3, 3, Some(age)),
            ],
        ),
        node_report(
            "db-1",
            3,
            3,
            vec![
                peer_status("app-1", 1, 1, Some(age)),
                peer_status("app-2", 2, 2, Some(age)),
            ],
        ),
    ]
}

fn peer_of(nodes: &mut [MeshNodeReport], node: usize, peer: usize) -> Option<&mut MeshPeerStatus> {
    nodes[node].status.as_mut()?.peers.get_mut(peer)
}

#[test]
fn a_mesh_where_every_pair_shook_hands_recently_has_no_problem() {
    let report = MeshReport::new("production".to_owned(), healthy(40));
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert!(report.healthy());
    assert_eq!(report.nodes[0].summary().as_deref(), Some("mesh 2/2 peers"));
    assert!(
        MeshReport::new(
            "production".to_owned(),
            healthy(MESH_HANDSHAKE_STALE_SECONDS)
        )
        .healthy()
    );
}

#[test]
fn a_pair_without_a_handshake_names_the_port_and_both_addresses() -> TestResult {
    let mut nodes = healthy(30);
    // app-1 and db-1 never heard from each other; app-2 hears both.
    peer_of(&mut nodes, 0, 1)
        .ok_or("no peer")?
        .handshake_age_seconds = None;
    peer_of(&mut nodes, 2, 0)
        .ok_or("no peer")?
        .handshake_age_seconds = Some(4000);
    let report = MeshReport::new("production".to_owned(), nodes);
    assert!(!report.healthy());
    assert_eq!(
        report.problems.len(),
        1,
        "once per pair: {:?}",
        report.problems
    );
    let problem = &report.problems[0];
    assert_eq!(problem.code, "MM_MESH_NO_HANDSHAKE");
    assert_eq!(problem.severity, Severity::Error);
    assert!(
        problem
            .message
            .contains("'app-1' and 'db-1' have no WireGuard handshake"),
        "{}",
        problem.message
    );
    assert!(
        problem.message.contains("app-1 last heard from it never"),
        "{}",
        problem.message
    );
    assert!(
        problem
            .message
            .contains("db-1 last heard from it 66 min ago"),
        "{}",
        problem.message
    );
    let help = problem.help.as_deref().unwrap_or_default();
    assert!(
        help.contains("UDP port 51820 must be open between 203.0.113.11 (app-1) and 203.0.113.13 (db-1) in both directions"),
        "{help}"
    );
    assert_eq!(report.nodes[0].summary().as_deref(), Some("mesh 1/2 peers"));
    Ok(())
}

#[test]
fn missing_stale_and_mismatched_entries_say_what_to_run() -> TestResult {
    // app-1 has no entry for db-1 at all.
    let mut nodes = healthy(30);
    nodes[0].status.as_mut().ok_or("no status")?.peers.pop();
    let problems = mesh_diagnostics("production", &nodes);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].code, "MM_MESH_PEER_MISSING");
    assert!(
        problems[0]
            .message
            .contains("node 'app-1' has no peer entry for 'db-1'")
    );
    assert!(
        problems[0]
            .help
            .as_deref()
            .unwrap_or_default()
            .contains("mm target setup plan")
    );

    // app-1 holds another key for app-2 than app-2 uses.
    let mut nodes = healthy(30);
    peer_of(&mut nodes, 0, 0).ok_or("no peer")?.public_key = key(9);
    let problems = mesh_diagnostics("production", &nodes);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].code, "MM_MESH_KEY_MISMATCH");

    // The state lists a peer the interface does not have.
    let mut nodes = healthy(30);
    peer_of(&mut nodes, 1, 1).ok_or("no peer")?.on_interface = false;
    let problems = mesh_diagnostics("production", &nodes);
    assert_eq!(problems[0].code, "MM_MESH_DRIFT");
    assert!(
        problems[0]
            .help
            .as_deref()
            .unwrap_or_default()
            .contains("systemctl restart mm-mesh")
    );

    // A peer that is no node of the target any more is only a warning.
    let mut nodes = healthy(30);
    nodes[0]
        .status
        .as_mut()
        .ok_or("no status")?
        .peers
        .push(peer_status("app-9", 9, 9, Some(5)));
    let problems = mesh_diagnostics("production", &nodes);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(
        (problems[0].code.as_str(), problems[0].severity),
        ("MM_MESH_EXTRA_PEER", Severity::Warning)
    );
    Ok(())
}

#[test]
fn nodes_without_an_interface_or_without_an_answer_are_reported_as_such() -> TestResult {
    let mut nodes = healthy(30);
    nodes[1].status.as_mut().ok_or("no status")?.up = false;
    nodes[2].status = Some(MeshNodeStatus::default());
    let problems = mesh_diagnostics("production", &nodes);
    let codes = problems
        .iter()
        .map(|problem| problem.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"MM_MESH_DOWN"), "{codes:?}");
    assert!(codes.contains(&"MM_MESH_NOT_SET_UP"), "{codes:?}");
    assert_eq!(nodes[1].summary().as_deref(), Some("mesh down"));
    assert_eq!(nodes[2].summary(), None);

    // A node whose mmd did not answer has no status: `mm target nodes`
    // explains it, and the mesh report is not healthy.
    let mut nodes = healthy(30);
    nodes[2].status = None;
    nodes[2].reachable = false;
    nodes[2].problem = Some("did not answer".to_owned());
    let report = MeshReport::new("production".to_owned(), nodes);
    assert!(!report.healthy());
    assert!(report.problems.is_empty(), "{:?}", report.problems);

    let existing = MeshReport::on_existing_interface("production".to_owned(), "eth1".to_owned());
    assert!(existing.healthy() && existing.nodes.is_empty());
    assert_eq!(existing.existing_interface.as_deref(), Some("eth1"));
    Ok(())
}

#[test]
fn a_node_is_reached_at_its_endpoint_when_it_declares_one() -> TestResult {
    use mm::domain::repo_target::RepoTargetConfig;

    let parse = |nodes: &str, cluster: &str| {
        serde_yaml_ng::from_str::<RepoTargetConfig>(&format!("nodes:\n{nodes}{cluster}"))
    };
    let target = parse(
        "  - {name: app-1, host: 203.0.113.11, endpoint: 10.0.0.11}\n  - {name: app-2, host: node-2.example.test}\n",
        "",
    )?;
    target.validate("production")?;
    let members = target.mesh_members()?;
    assert_eq!(
        members
            .iter()
            .map(|member| (
                member.name.as_str(),
                member.address.to_string(),
                member.endpoint.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("app-1", "10.90.0.1".to_owned(), "10.0.0.11"),
            ("app-2", "10.90.0.2".to_owned(), "node-2.example.test"),
        ],
        "the endpoint, else the host mm connects to"
    );
    assert!(members.iter().all(|member| member.public_key.is_none()));
    assert_eq!(
        peer_set(51820, &members, "app-2")[0].endpoint,
        "10.0.0.11:51820"
    );
    // The key is not written back unless it is set.
    assert!(!serde_json::to_string(&target.nodes[1])?.contains("endpoint"));

    // The port is the mesh's, never the endpoint's.
    let with_port = parse(
        "  - {name: app-1, host: 203.0.113.11, endpoint: '10.0.0.11:51820'}\n",
        "",
    )?;
    let error = with_port
        .validate("production")
        .err()
        .ok_or("an endpoint with a port was accepted")?;
    assert!(
        error.contains("without a port; the port is cluster.mesh.port"),
        "{error}"
    );

    let ipv6 = parse(
        "  - {name: app-1, host: 203.0.113.11, endpoint: '2001:db8::11'}\n",
        "",
    )?;
    ipv6.validate("production")?;
    assert_eq!(
        endpoint_with_port(&ipv6.mesh_members()?[0].endpoint, 51820),
        "[2001:db8::11]:51820"
    );

    // An existing private interface has no WireGuard endpoints.
    let existing = parse(
        "  - {name: app-1, host: 203.0.113.11, endpoint: 10.0.0.11, private-address: 10.0.0.11}\n",
        "cluster:\n  mesh: {interface: eth1}\n",
    )?;
    let error = existing
        .validate("production")
        .err()
        .ok_or("an endpoint without a mesh was accepted")?;
    assert!(
        error.contains("mm creates no WireGuard mesh, so remove endpoint"),
        "{error}"
    );
    Ok(())
}

#[test]
fn setup_refuses_to_downgrade_a_node_and_says_what_it_replaces() -> TestResult {
    use mm::domain::cluster_setup::{HostRelease, ReleaseConsent, release_notices};

    let release = |version: &str, protocol: u16| HostRelease {
        version: version.to_owned(),
        protocol,
        build: None,
    };
    let this = release("0.2.0", 16);
    assert!(release("0.2.0", 17).is_newer_than(&this));
    assert!(release("0.3.0", 16).is_newer_than(&this));
    assert!(!release("0.9.0", 15).is_newer_than(&this));
    assert!(!this.is_newer_than(&this));

    let mesh = default_mesh();
    let members = members();
    let facts: MeshFacts = serde_json::from_str(
        r#"{"public-key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","release":{"version":"0.2.0","protocol":17},"later":"a field a later mmd adds"}"#,
    )?;
    let planned = plan_node(&mesh, &members, &members[0], &[], Some(&facts));
    assert_eq!(planned.release, Some(release("0.2.0", 17)));
    assert!(
        release_notices(
            &[("app-2", Some(&release("0.1.0", 15)))],
            &this,
            ReleaseConsent {
                downgrade: true,
                upgrade: false,
            },
        )
        .is_err_and(|refusal| refusal.contains("--upgrade")),
        "an older release is replaced only when asked"
    );

    let newer = release("0.2.0", 17);
    let older = release("0.1.0", 15);
    let nodes = [
        ("app-1", Some(&newer)),
        ("app-2", Some(&older)),
        ("app-3", None),
        ("app-4", Some(&this)),
    ];
    let refused = release_notices(&nodes, &this, ReleaseConsent::default())
        .err()
        .unwrap_or_default();
    for part in [
        "app-1 runs mm 0.2.0 (remote protocol 17), newer than this mm 0.2.0 (remote protocol 16)",
        "Install mm 0.2.0 or later on this machine",
        "--downgrade",
    ] {
        assert!(refused.contains(part), "{part}: {refused}");
    }
    let notices = release_notices(
        &nodes,
        &this,
        ReleaseConsent {
            downgrade: true,
            upgrade: true,
        },
    )?;
    assert_eq!(
        notices,
        [
            "app-1: downgrades mm and mmd from 0.2.0 (remote protocol 17) to 0.2.0 (remote protocol 16) (--downgrade)",
            "app-2: replaces mm and mmd 0.1.0 (remote protocol 15) with 0.2.0 (remote protocol 16)",
            "app-3: installs mm and mmd 0.2.0 (remote protocol 16), in place of the earlier release the node may run",
            "app-4 runs this release of mm, 0.2.0 (remote protocol 16); setup installs its host binaries again",
        ]
    );
    Ok(())
}

#[test]
fn setup_refuses_another_build_of_the_same_release_unless_asked() -> TestResult {
    use mm::domain::cluster_setup::{HostRelease, ReleaseConsent, release_notices};

    let build = |digest: Option<&str>| HostRelease {
        version: "0.2.0".to_owned(),
        protocol: 17,
        build: digest.map(str::to_owned),
    };
    let this = build(Some(&"a".repeat(64)));
    let theirs = build(Some(&"b".repeat(64)));
    let same = build(Some(&"a".repeat(64)));
    let unknown = build(None);
    let nodes = [
        ("app-1", Some(&theirs)),
        ("app-2", Some(&same)),
        ("app-3", Some(&unknown)),
    ];
    let refused = release_notices(&nodes, &this, ReleaseConsent::default())
        .err()
        .ok_or("another build went through")?;
    for part in [
        "app-1 runs mm 0.2.0 (remote protocol 17, build bbbbbbbbbbbb)",
        "--upgrade",
        "deploy.mm-version",
    ] {
        assert!(refused.contains(part), "{part}: {refused}");
    }
    assert!(!refused.contains("app-2"), "{refused}");
    let notices = release_notices(
        &nodes,
        &this,
        ReleaseConsent {
            downgrade: false,
            upgrade: true,
        },
    )?;
    assert_eq!(
        notices[0],
        "app-1: replaces mm and mmd 0.2.0 (remote protocol 17, build bbbbbbbbbbbb) with 0.2.0 (remote protocol 17, build aaaaaaaaaaaa)"
    );
    assert!(notices[1].starts_with("app-2 runs this release of mm"));
    assert!(notices[2].starts_with("app-3 runs this release of mm"));
    Ok(())
}
