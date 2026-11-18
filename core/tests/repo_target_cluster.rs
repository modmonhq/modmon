//! A cluster target as the repo declares it: its nodes, the defaults of
//! its `cluster:` block, the addresses it gives out, and what validation
//! refuses.

use std::net::Ipv4Addr;

use mm::domain::config::removed::Removed;
use mm::domain::repo_target::*;
use mm::domain::target::TargetTransport;
use mm::domain::workspace::ProjectId;

const KEY_1: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";
const KEY_2: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBPnHBZZYETwo334Dl5Vu8LQSzJiO4gxHeq5zQGE+eAL";

/// The smallest cluster target: three hosts and nothing else.
const NODES: &str = r"
nodes:
  - {name: app-1, host: 203.0.113.11}
  - {name: app-2, host: 203.0.113.12}
  - {name: db-1, host: 203.0.113.13}
";

/// A cluster target that sets every key.
const CLUSTER: &str = r"
nodes:
  - name: app-1
    host: 203.0.113.11
    private-address: 10.77.0.1
  - {name: app-2, host: 203.0.113.12, private-address: 10.77.0.2}
  - {name: app-3, host: 203.0.113.13, private-address: 10.77.0.3}
cluster:
  mesh: {subnet: 10.77.0.0/24, port: 51900, mtu: 1380}
  networks: 10.200.0.0/16
  storage: app-2
  groups: {db: [app-1], web: [app-2, app-3]}
setup:
  origin: git@github.com:example/app.git
  origin-host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
  deploy-key:
    secret: origin.deploy-key
    public-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBPnHBZZYETwo334Dl5Vu8LQSzJiO4gxHeq5zQGE+eAL mm-deploy
  break-glass:
    - {name: ops-recovery, key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w ops}
";

fn parse(yaml: &str) -> Result<RepoTargetConfig, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(yaml)
}

fn recorded() -> Result<RepoTargetConfig, serde_yaml_ng::Error> {
    let mut target = parse(CLUSTER)?;
    for (index, node) in target.nodes.iter_mut().enumerate() {
        node.host_key = Some(if index == 0 { KEY_1 } else { KEY_2 }.to_owned());
        node.daemon_fingerprint = Some(format!("{index}").repeat(64));
    }
    Ok(target)
}

fn address(last: u8) -> Ipv4Addr {
    Ipv4Addr::new(10, 90, 0, last)
}

#[test]
fn a_target_with_only_nodes_is_a_cluster_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let target = parse(NODES)?;
    target.validate("production")?;
    assert!(target.is_cluster());
    assert!(target.cluster.is_default());
    assert_eq!(
        target.cluster.mesh,
        MeshConfig::Wireguard(WireguardMesh {
            subnet: "10.90.0.0/24".parse()?,
            port: 51820,
            mtu: 1420,
        })
    );
    assert_eq!(target.cluster.mesh.interface(), MESH_INTERFACE);
    assert_eq!(MESH_INTERFACE, "mm0");
    assert_eq!(target.cluster.networks.to_string(), "10.64.0.0/12");
    assert_eq!(target.cluster.networks, DEFAULT_CLUSTER_NETWORKS);
    assert_eq!(target.cluster.storage, None);
    assert_eq!(target.storage_node(), Some("app-1"));
    assert!(target.cluster.groups.is_empty());
    assert!(target.setup.is_none());
    assert_eq!(
        target.anchor_node().map(|node| node.name.as_str()),
        Some("app-1")
    );
    assert!(target.nodes.iter().all(|node| node.port == 22
        && node.private_address.is_none()
        && node.host_key.is_none()
        && node.daemon_fingerprint.is_none()));
    assert_eq!(
        target.mesh_addresses()?,
        [address(1), address(2), address(3)]
    );
    Ok(())
}

#[test]
fn a_cluster_that_sets_every_key_parses_and_round_trips() -> Result<(), Box<dyn std::error::Error>>
{
    let target = parse(CLUSTER)?;
    target.validate("integration")?;
    assert_eq!(
        target.cluster.mesh,
        MeshConfig::Wireguard(WireguardMesh {
            subnet: "10.77.0.0/24".parse()?,
            port: 51900,
            mtu: 1380,
        })
    );
    assert_eq!(target.cluster.networks.to_string(), "10.200.0.0/16");
    assert_eq!(target.storage_node(), Some("app-2"));
    assert_eq!(
        target.cluster.groups.get("web"),
        Some(&vec!["app-2".to_owned(), "app-3".to_owned()])
    );
    assert_eq!(target.setup.as_ref().ok_or("no setup")?.user, "root");
    assert_eq!(
        target.mesh_addresses()?,
        [
            Ipv4Addr::new(10, 77, 0, 1),
            Ipv4Addr::new(10, 77, 0, 2),
            Ipv4Addr::new(10, 77, 0, 3)
        ]
    );
    let round_trip = parse(&serde_yaml_ng::to_string(&target)?)?;
    assert_eq!(round_trip, target);
    Ok(())
}

#[test]
fn defaults_are_left_out_of_the_written_config() -> Result<(), Box<dyn std::error::Error>> {
    let mut target = parse(NODES)?;
    let yaml = serde_yaml_ng::to_string(&target)?;
    for absent in ["cluster", "mesh", "setup", "members", "swarm", "port"] {
        assert!(!yaml.contains(absent), "{absent} in {yaml}");
    }
    assert_eq!(parse(&yaml)?, target);

    target.cluster.mesh = MeshConfig::Wireguard(WireguardMesh {
        mtu: 1380,
        ..WireguardMesh::default()
    });
    assert_eq!(
        serde_yaml_ng::to_value(&target.cluster)?,
        serde_yaml_ng::from_str::<serde_yaml_ng::Value>("mesh: {mtu: 1380}")?
    );
    let round_trip = parse(&serde_yaml_ng::to_string(&target)?)?;
    assert_eq!(round_trip, target);
    assert_eq!(
        round_trip.cluster.mesh,
        MeshConfig::Wireguard(WireguardMesh {
            subnet: "10.90.0.0/24".parse()?,
            port: 51820,
            mtu: 1380,
        })
    );

    target.cluster.mesh = MeshConfig::Interface("eth1".to_owned());
    assert_eq!(
        serde_yaml_ng::to_value(&target.cluster)?,
        serde_yaml_ng::from_str::<serde_yaml_ng::Value>("mesh: {interface: eth1}")?
    );
    Ok(())
}

#[test]
fn an_existing_interface_needs_every_node_to_declare_its_address()
-> Result<(), Box<dyn std::error::Error>> {
    let mut target = parse(
        "nodes:\n  - {name: a, host: a.test, private-address: 192.168.1.2}\n  - {name: b, host: b.test}\ncluster: {mesh: {interface: eth1}}\n",
    )?;
    assert_eq!(
        target.cluster.mesh,
        MeshConfig::Interface("eth1".to_owned())
    );
    assert_eq!(target.cluster.mesh.interface(), "eth1");
    let error = target.validate("production").err().unwrap_or_default();
    assert!(
        error.contains("node b needs private-address") && error.contains("eth1"),
        "{error}"
    );
    assert!(target.mesh_addresses().is_err());

    target.nodes[1].private_address = Some(Ipv4Addr::new(192, 168, 1, 3));
    target.validate("production")?;
    assert_eq!(
        target.mesh_addresses()?,
        [Ipv4Addr::new(192, 168, 1, 2), Ipv4Addr::new(192, 168, 1, 3)]
    );
    assert_eq!(parse(&serde_yaml_ng::to_string(&target)?)?, target);

    target.nodes[1].private_address = Some(Ipv4Addr::new(10, 70, 0, 3));
    let error = target.validate("production").err().unwrap_or_default();
    assert!(error.contains("inside cluster.networks"), "{error}");

    target.cluster.mesh = MeshConfig::Interface("not an interface".to_owned());
    let error = target.validate("production").err().unwrap_or_default();
    assert!(error.contains("not a valid interface name"), "{error}");
    Ok(())
}

#[test]
fn a_mesh_is_either_an_interface_or_wireguard_settings() {
    for both in [
        "nodes: [{name: a, host: a.test}]\ncluster: {mesh: {interface: eth1, subnet: 10.1.0.0/24}}\n",
        "nodes: [{name: a, host: a.test}]\ncluster: {mesh: {interface: eth1, mtu: 1400}}\n",
        "nodes: [{name: a, host: a.test}]\ncluster: {mesh: {interface: eth1, port: 51821}}\n",
    ] {
        let error = parse(both)
            .map(drop)
            .map_err(|error| error.to_string())
            .err()
            .unwrap_or_default();
        assert!(error.contains("not both"), "{error}");
    }
    assert!(
        parse("nodes: [{name: a, host: a.test}]\ncluster: {mesh: {wireguard: {}}}\n").is_err(),
        "unknown mesh keys are refused"
    );
}

#[test]
fn undeclared_nodes_take_the_lowest_free_addresses_in_order()
-> Result<(), Box<dyn std::error::Error>> {
    let mut target = parse(NODES)?;
    target.nodes[1].private_address = Some(address(1));
    target.validate("production")?;
    assert_eq!(
        target.mesh_addresses()?,
        [address(2), address(1), address(3)],
        "a declared address is kept and never assigned twice"
    );

    target.nodes[1].private_address = Some(address(200));
    target.validate("production")?;
    assert_eq!(
        target.mesh_addresses()?,
        [address(1), address(200), address(2)]
    );

    // Recording the assignment keeps addresses still when the list changes.
    let mut recorded = parse(NODES)?;
    for (node, address) in recorded.nodes.iter_mut().zip(target.mesh_addresses()?) {
        node.private_address = Some(address);
    }
    recorded.nodes.remove(0);
    recorded.validate("production")?;
    assert_eq!(recorded.mesh_addresses()?, [address(200), address(2)]);
    Ok(())
}

#[test]
fn clusters_of_any_size_are_accepted() -> Result<(), Box<dyn std::error::Error>> {
    for count in [1_u8, 2, 4, 8, 12] {
        let mut target = parse(NODES)?;
        target.nodes = (1..=count)
            .map(|index| RepoTargetNode {
                name: format!("n-{index}"),
                host: format!("n{index}.test"),
                port: 22,
                endpoint: None,
                private_address: None,
                host_key: None,
                daemon_fingerprint: None,
            })
            .collect();
        target.validate("production")?;
        assert_eq!(
            target.mesh_addresses()?,
            (1..=count).map(address).collect::<Vec<_>>(),
            "{count} nodes"
        );
    }
    Ok(())
}

fn rejects_in(yaml: &str, mutate: impl FnOnce(&mut RepoTargetConfig), expected: &str) {
    let mut target = parse(yaml).unwrap_or_else(|_| RepoTargetConfig {
        host: None,
        port: 22,
        host_key: None,
        daemon_fingerprint: None,
        nodes: Vec::new(),
        cluster: RepoClusterConfig::default(),
        setup: None,
        delivery: Default::default(),
        providers: std::collections::BTreeMap::new(),
        members: Removed,
        swarm: Removed,
    });
    assert!(!target.nodes.is_empty(), "fixture parses");
    mutate(&mut target);
    let error = target.validate("integration").err().unwrap_or_default();
    assert!(
        error.contains(expected),
        "expected '{expected}' in '{error}'"
    );
}

fn rejects(mutate: impl FnOnce(&mut RepoTargetConfig), expected: &str) {
    rejects_in(CLUSTER, mutate, expected);
}

fn wireguard(subnet: &str) -> MeshConfig {
    MeshConfig::Wireguard(WireguardMesh {
        subnet: subnet.parse().unwrap_or(DEFAULT_CLUSTER_NETWORKS),
        ..WireguardMesh::default()
    })
}

#[test]
fn invalid_nodes_are_rejected_with_the_reason() {
    rejects(
        |target| target.host = Some("x.test".to_owned()),
        "mixes nodes: with host",
    );
    rejects(
        |target| target.nodes[1].name = "App_2".to_owned(),
        "lowercase hostname",
    );
    rejects(
        |target| target.nodes[1].name = "app-1".to_owned(),
        "repeats node app-1",
    );
    rejects(
        |target| target.nodes[2].name = "all".to_owned(),
        "node name 'all' is taken",
    );
    rejects(
        |target| target.nodes[2].host = "203.0.113.12".to_owned(),
        "repeats 203.0.113.12:22",
    );
    rejects(
        |target| target.nodes[2].host = "two words".to_owned(),
        "node app-3 needs a host name or address",
    );
    rejects(
        |target| target.nodes[1].host_key = Some(KEY_2.to_owned()),
        "node app-2 needs both host-key and daemon-fingerprint",
    );
    rejects(
        |target| target.nodes[1].daemon_fingerprint = Some("1".repeat(64)),
        "node app-2 needs both host-key and daemon-fingerprint",
    );
}

#[test]
fn invalid_private_addresses_are_rejected_with_the_reason() {
    rejects(
        |target| target.nodes[2].private_address = Some(Ipv4Addr::new(10, 77, 0, 2)),
        "node app-3 repeats private-address 10.77.0.2",
    );
    rejects(
        |target| target.nodes[0].private_address = Some(Ipv4Addr::new(8, 8, 8, 8)),
        "not a private address",
    );
    for outside in [
        Ipv4Addr::new(10, 78, 0, 1),
        Ipv4Addr::new(10, 77, 0, 0),
        Ipv4Addr::new(10, 77, 0, 255),
    ] {
        rejects(
            |target| target.nodes[0].private_address = Some(outside),
            "is not a host address of the mesh subnet 10.77.0.0/24",
        );
    }
    rejects_in(
        NODES,
        |target| target.nodes[2].private_address = Some(Ipv4Addr::new(10, 77, 0, 3)),
        "node db-1 private-address 10.77.0.3 is not a host address of the mesh subnet 10.90.0.0/24",
    );
    rejects_in(
        NODES,
        |target| target.cluster.mesh = wireguard("10.90.0.0/30"),
        "mesh subnet 10.90.0.0/30 has no free address for node db-1",
    );
    rejects_in(
        NODES,
        |target| target.cluster.mesh = wireguard("10.90.0.0/31"),
        "cluster.mesh.subnet 10.90.0.0/31 is too small",
    );
}

#[test]
fn invalid_cluster_settings_are_rejected_with_the_reason() {
    rejects(
        |target| {
            target.cluster.networks = Ipv4Cidr {
                address: Ipv4Addr::new(10, 77, 0, 0),
                prefix: 16,
            }
        },
        "cluster.mesh.subnet 10.77.0.0/24 overlaps cluster.networks 10.77.0.0/16",
    );
    rejects_in(
        NODES,
        |target| target.cluster.mesh = wireguard("10.70.4.0/24"),
        "overlaps cluster.networks 10.64.0.0/12",
    );
    rejects(
        |target| {
            target.cluster.networks = Ipv4Cidr {
                address: Ipv4Addr::new(8, 8, 0, 0),
                prefix: 16,
            }
        },
        "cluster.networks 8.8.0.0/16 is not a private range",
    );
    rejects(
        |target| {
            target.cluster.networks = Ipv4Cidr {
                address: Ipv4Addr::new(10, 91, 0, 0),
                prefix: 24,
            }
        },
        "cluster.networks 10.91.0.0/24 is too small to hold a network: every network takes a /20",
    );
    rejects_in(
        NODES,
        |target| target.cluster.mesh = wireguard("8.8.8.0/24"),
        "cluster.mesh.subnet 8.8.8.0/24 is not a private range",
    );
    rejects(
        |target| {
            target.cluster.mesh = MeshConfig::Wireguard(WireguardMesh {
                subnet: Ipv4Cidr {
                    address: Ipv4Addr::new(10, 77, 0, 0),
                    prefix: 24,
                },
                port: 0,
                mtu: 1420,
            });
        },
        "cluster.mesh.port must be between 1 and 65535",
    );
    rejects(
        |target| {
            target.cluster.mesh = MeshConfig::Wireguard(WireguardMesh {
                subnet: Ipv4Cidr {
                    address: Ipv4Addr::new(10, 77, 0, 0),
                    prefix: 24,
                },
                port: 51820,
                mtu: 576,
            });
        },
        "cluster.mesh.mtu 576 must be between 1280 and 9000",
    );
}

#[test]
fn storage_and_groups_must_name_declared_nodes_unambiguously() {
    rejects(
        |target| target.cluster.storage = Some("app-9".to_owned()),
        "cluster.storage names unknown node 'app-9'",
    );
    rejects(
        |target| {
            target
                .cluster
                .groups
                .insert("edge".to_owned(), vec!["app-7".to_owned()]);
        },
        "cluster.groups 'edge' names unknown node 'app-7'",
    );
    rejects(
        |target| {
            target.cluster.groups.insert("edge".to_owned(), Vec::new());
        },
        "cluster.groups 'edge' lists no nodes",
    );
    rejects(
        |target| {
            target
                .cluster
                .groups
                .insert("app-2".to_owned(), vec!["app-1".to_owned()]);
        },
        "cluster.groups name 'app-2' is also a node name",
    );
    rejects(
        |target| {
            target
                .cluster
                .groups
                .insert("all".to_owned(), vec!["app-1".to_owned()]);
        },
        "cluster.groups cannot define 'all'",
    );
    rejects(
        |target| {
            target
                .cluster
                .groups
                .insert("Web".to_owned(), vec!["app-1".to_owned()]);
        },
        "cluster.groups name 'Web' must use lowercase",
    );
}

#[test]
fn invalid_setup_is_rejected_with_the_reason() {
    rejects(
        |target| {
            if let Some(setup) = &mut target.setup
                && let Some(deploy_key) = &mut setup.deploy_key
            {
                deploy_key.public_key = "ssh-ed25519 nope".to_owned();
            }
        },
        "deploy-key public-key",
    );
    rejects(
        |target| {
            if let Some(setup) = &mut target.setup {
                setup.break_glass.push(setup.break_glass[0].clone());
            }
        },
        "break-glass repeats 'ops-recovery'",
    );
    rejects(
        |target| {
            if let Some(setup) = &mut target.setup {
                setup.user = "not a user".to_owned();
            }
        },
        "setup user 'not a user' is not a valid account name",
    );
}

#[test]
fn nodes_need_recorded_keys_before_mm_can_connect() -> Result<(), Box<dyn std::error::Error>> {
    let project = ProjectId(uuid::Uuid::from_u128(9));
    let error = parse(CLUSTER)?
        .named_target(project, "integration")
        .err()
        .unwrap_or_default();
    assert!(
        error.contains("has no node with recorded keys yet"),
        "{error}"
    );
    assert!(error.contains("mm target setup record"), "{error}");
    assert!(
        parse(CLUSTER)?.pin("integration")?.nodes.is_empty(),
        "a node without keys has nothing to pin"
    );

    let target = recorded()?;
    let named = target.named_target(project, "integration")?;
    mm::domain::target::validate_target_definition(&named.definition)?;
    assert!(named.definition.is_cluster());
    let TargetTransport::Ssh { host, .. } = &named.definition.transport else {
        return Err("not SSH".into());
    };
    assert_eq!(host, "203.0.113.11", "the first node stands for the target");
    assert_eq!(
        named
            .definition
            .nodes
            .iter()
            .map(|node| (node.name.as_str(), node.address(), node.is_recorded()))
            .collect::<Vec<_>>(),
        [
            ("app-1", "203.0.113.11:22".to_owned(), true),
            ("app-2", "203.0.113.12:22".to_owned(), true),
            ("app-3", "203.0.113.13:22".to_owned(), true)
        ]
    );
    // The node list is part of the target's identity: reordering it
    // invalidates plans reviewed before.
    let mut reordered = target.clone();
    reordered.nodes.swap(1, 2);
    assert_ne!(
        reordered.named_target(project, "integration")?.fingerprint,
        named.fingerprint
    );

    // A node that is not set up yet is listed, and mm connects to the rest.
    let mut partial = target.clone();
    partial.nodes[0].host_key = None;
    partial.nodes[0].daemon_fingerprint = None;
    let named_partial = partial.named_target(project, "integration")?;
    assert!(!named_partial.definition.nodes[0].is_recorded());
    assert!(named_partial.definition.nodes[0].transport().is_none());
    let TargetTransport::Ssh { host, .. } = &named_partial.definition.transport else {
        return Err("not SSH".into());
    };
    assert_eq!(host, "203.0.113.12");
    Ok(())
}

#[test]
fn a_pin_names_each_node_and_covers_dropping_or_reordering_them()
-> Result<(), Box<dyn std::error::Error>> {
    let target = recorded()?;
    let pin = target.pin("integration")?;
    assert_eq!(pin.host_key, None);
    assert_eq!(
        pin.nodes,
        [
            NodePin {
                name: "app-1".to_owned(),
                host_key: KEY_1.to_owned(),
                daemon_fingerprint: "0".repeat(64),
            },
            NodePin {
                name: "app-2".to_owned(),
                host_key: KEY_2.to_owned(),
                daemon_fingerprint: "1".repeat(64),
            },
            NodePin {
                name: "app-3".to_owned(),
                host_key: KEY_2.to_owned(),
                daemon_fingerprint: "2".repeat(64),
            }
        ]
    );

    // Dropping or reordering nodes adds no identity; a new one needs review.
    let mut fewer = target.clone();
    fewer.cluster.groups.clear();
    fewer.nodes.remove(0);
    assert!(pin.covers(&fewer.pin("integration")?));
    assert!(!fewer.pin("integration")?.covers(&pin));

    let mut half = parse(CLUSTER)?;
    half.nodes[1].host_key = Some(KEY_2.to_owned());
    assert!(
        half.validate("integration").is_err(),
        "keys are recorded in pairs"
    );
    Ok(())
}

#[test]
fn flat_targets_cannot_carry_cluster_settings() -> Result<(), Box<dyn std::error::Error>> {
    let flat = format!(
        "host: a.test\nhost-key: {KEY_1}\ndaemon-fingerprint: {}\n",
        "a".repeat(64)
    );
    parse(&flat)?.validate("prod")?;
    for extra in [
        "cluster: {storage: a}\n",
        "cluster: {mesh: {mtu: 1400}}\n",
        "setup:\n  origin: git@github.com:example/app.git\n",
    ] {
        let error = parse(&format!("{flat}{extra}"))?
            .validate("prod")
            .err()
            .unwrap_or_default();
        assert!(
            error.contains("sets cluster or setup without nodes"),
            "{extra}: {error}"
        );
    }
    let missing = parse("host: a.test\n")?;
    assert!(
        missing
            .validate("prod")
            .err()
            .unwrap_or_default()
            .contains("or a nodes: list")
    );
    Ok(())
}

#[test]
fn removed_keys_fail_with_what_replaces_them() {
    let error_of = |yaml: &str| {
        parse(yaml)
            .map(drop)
            .map_err(|error| error.to_string())
            .err()
            .unwrap_or_default()
    };
    let swarm = error_of(&format!(
        "{NODES}swarm:\n  private-network: {{wireguard: {{subnet: 10.77.0.0/24}}}}\n  storage: app-1\n"
    ));
    assert!(
        swarm.contains("swarm: was removed") && swarm.contains("cluster:"),
        "{swarm}"
    );
    let empty_swarm = error_of(&format!("{NODES}swarm: {{}}\n"));
    assert!(empty_swarm.contains("cluster:"), "{empty_swarm}");

    let members = error_of(&format!(
        "host: a.test\nhost-key: {KEY_1}\ndaemon-fingerprint: {}\nmembers:\n  - host: b.test\n    host-key: {KEY_2}\n    daemon-fingerprint: {}\n",
        "a".repeat(64),
        "b".repeat(64)
    ));
    assert!(
        members.contains("members: was removed") && members.contains("nodes:"),
        "{members}"
    );
    let empty_members = error_of("host: a.test\nmembers: []\n");
    assert!(empty_members.contains("nodes:"), "{empty_members}");

    for error in [swarm, empty_swarm, members, empty_members] {
        assert!(
            !error.contains("unknown field") && !error.contains("unknown variant"),
            "{error}"
        );
    }
    let unknown = error_of(&format!("{NODES}quorum: 3\n"));
    assert!(unknown.contains("unknown field"), "{unknown}");
}

#[test]
fn cidr_parsing_overlap_and_host_addresses() -> Result<(), String> {
    let parse = |value: &str| value.parse::<Ipv4Cidr>();
    assert!(parse("10.77.0.1/24").is_err(), "host bits are refused");
    assert!(parse("10.77.0.0").is_err());
    assert!(parse("10.77.0.0/33").is_err());
    let (wide, narrow, other) = (
        parse("10.0.0.0/8")?,
        parse("10.77.0.0/24")?,
        parse("192.168.0.0/16")?,
    );
    assert!(wide.overlaps(narrow) && narrow.overlaps(wide));
    assert!(!wide.overlaps(other));
    assert!(narrow.contains(Ipv4Addr::new(10, 77, 0, 200)));
    assert!(!narrow.contains(Ipv4Addr::new(10, 77, 1, 1)));
    assert!(parse("0.0.0.0/0").is_ok_and(|all| all.contains(Ipv4Addr::BROADCAST)));
    // The shared address space carriers use for private links counts as
    // private: a mesh may live in it.
    assert!(
        serde_yaml_ng::from_str::<RepoTargetConfig>(
            "nodes:\n  - {name: app-1, host: 203.0.113.11}\ncluster: {mesh: {subnet: 100.64.0.0/24}}\n"
        )
        .is_ok_and(|target| target.validate("production").is_ok())
    );

    assert_eq!(narrow.hosts().count(), 254);
    assert_eq!(narrow.hosts().next(), Some(Ipv4Addr::new(10, 77, 0, 1)));
    assert_eq!(narrow.hosts().last(), Some(Ipv4Addr::new(10, 77, 0, 254)));
    assert_eq!(
        parse("10.77.0.4/30")?.hosts().collect::<Vec<_>>(),
        [Ipv4Addr::new(10, 77, 0, 5), Ipv4Addr::new(10, 77, 0, 6)]
    );
    assert_eq!(parse("10.77.0.4/31")?.hosts().count(), 0);
    assert_eq!(parse("10.77.0.4/32")?.hosts().count(), 0);
    assert!(narrow.is_host(Ipv4Addr::new(10, 77, 0, 1)));
    assert!(narrow.is_host(Ipv4Addr::new(10, 77, 0, 254)));
    assert!(!narrow.is_host(Ipv4Addr::new(10, 77, 0, 0)), "network");
    assert!(!narrow.is_host(Ipv4Addr::new(10, 77, 0, 255)), "broadcast");
    assert!(!narrow.is_host(Ipv4Addr::new(10, 78, 0, 1)), "outside");
    Ok(())
}
