//! Shared volumes: where a volume's files are, what each node mounts, the
//! rules that keep containers away from the storage port, what a plan and a
//! status say, and that nothing but the one delete removes data.

use std::collections::{BTreeMap, VecDeque};
use std::net::Ipv4Addr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::runtime::DockerComposeRuntime;
use mm::adapters::shared_storage::{HostSharedStorage, StorageKeeper, StoragePaths};
use mm::app::ports::{
    CommandOutput, CommandSpec, MeshHost, ProcessRunner, RuntimeDriver, SharedStorageHost,
    StoredVolumes,
};
use mm::app::{SharedStorageNode, StorageScope};
use mm::domain::cluster_network::forward_hook;
use mm::domain::diagnostic::Severity;
use mm::domain::error::MmError;
use mm::domain::mesh::{MeshApplied, MeshFacts, MeshNodeStatus, MeshPeer, MeshState};
use mm::domain::remote::{
    ClusterPlanSeal, ClusterPlanSection, RemotePlanKind, RemoteScope, SealCarried,
};
use mm::domain::repo_target::Ipv4Cidr;
use mm::domain::runtime::RuntimeKind;
use mm::domain::shared_storage::{
    ClusterStoragePlan, MOUNT_OPTIONS, STORAGE_LABEL, SealedStorage, SharedVolumeUse,
    StorageAccess, StorageFirewall, StorageNodeReport, StorageNodeStatus, StorageReport,
    StoredVolume, VolumeDeletePlan, VolumeName, WaitingContainer, mounted_volumes,
    volume_directory, volume_of_directory,
};
use mm::domain::workspace::ProjectId;
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROJECT: &str = "019c527b-6242-7b43-bfd4-bd60e27c1df0";

fn project() -> Result<ProjectId, uuid::Error> {
    Ok(ProjectId(uuid::Uuid::parse_str(PROJECT)?))
}

fn ip(text: &str) -> Result<Ipv4Addr, std::net::AddrParseError> {
    text.parse()
}

fn uploads() -> VolumeName {
    VolumeName::new("app", "uploads")
}

/// The mesh of node `own` in a cluster of three, `10.90.0.1` to `.3`.
fn mesh(own: u8) -> Result<MeshState, Box<dyn std::error::Error>> {
    let peers = (1..=3_u8)
        .filter(|node| *node != own)
        .map(|node| {
            Ok(MeshPeer {
                name: format!("app-{node}"),
                public_key: "k".repeat(43) + "=",
                endpoint: format!("203.0.113.{node}:51820"),
                allowed_ips: vec![
                    format!("10.90.0.{node}/32").parse::<Ipv4Cidr>()?,
                    format!("10.64.{node}.0/24").parse::<Ipv4Cidr>()?,
                ],
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(MeshState {
        interface: "mm0".to_owned(),
        address: Ipv4Addr::new(10, 90, 0, own),
        prefix: 24,
        port: 51820,
        mtu: 1420,
        peers,
    })
}

#[test]
fn a_volume_is_one_directory_per_project_environment_stack_and_name() -> TestResult {
    let directory = volume_directory(project()?, "production", &uploads())?;
    assert_eq!(directory, format!("{PROJECT}/production/app/uploads"));
    for (stack, volume) in [
        ("app", ".."),
        ("app", "a/b"),
        ("", "uploads"),
        ("app", "a b"),
    ] {
        let refused = volume_directory(project()?, "production", &VolumeName::new(stack, volume));
        assert!(refused.is_err(), "{stack}/{volume}");
    }
    assert!(
        volume_directory(project()?, "../etc", &uploads())
            .err()
            .is_some_and(|problem| problem.contains("environment"))
    );

    // Read back from a mount source and from the root of a bind mount.
    for path in [
        format!(":/{directory}"),
        format!("10.90.0.1:/{directory}"),
        format!("/var/lib/mm/storage/{directory}"),
    ] {
        assert_eq!(
            volume_of_directory(project()?, "production", &path),
            Some(uploads())
        );
    }
    for other in [
        format!(":/{PROJECT}/staging/app/uploads"),
        format!(":/{directory}/deeper"),
        format!(":/x{directory}"),
        ":/".to_owned(),
    ] {
        assert_eq!(volume_of_directory(project()?, "production", &other), None);
    }
    Ok(())
}

#[test]
fn the_storage_node_uses_the_directory_and_every_other_node_mounts_it() -> TestResult {
    let local = StorageAccess::Directory {
        node: "app-1".to_owned(),
        root: "/var/lib/mm/storage".into(),
    };
    let here = local.engine_volume(project()?, "production", &uploads())?;
    assert_eq!(
        here.driver_opts,
        BTreeMap::from([
            ("type".to_owned(), "none".to_owned()),
            ("o".to_owned(), "bind".to_owned()),
            (
                "device".to_owned(),
                format!("/var/lib/mm/storage/{PROJECT}/production/app/uploads")
            ),
        ])
    );
    assert_eq!(here.storage_label, "local");

    let over_mesh = |address: &str| -> Result<_, Box<dyn std::error::Error>> {
        Ok(StorageAccess::Mesh {
            node: "app-1".to_owned(),
            address: ip(address)?,
        }
        .engine_volume(project()?, "production", &uploads())?)
    };
    let there = over_mesh("10.90.0.1")?;
    assert_eq!(there.driver_opts["type"], "nfs");
    assert_eq!(
        there.driver_opts["o"],
        format!("addr=10.90.0.1,{MOUNT_OPTIONS}")
    );
    assert_eq!(
        there.driver_opts["device"],
        format!("10.90.0.1:/{PROJECT}/production/app/uploads"),
        "the server is in the source too: Podman mounts with mount(8), whose NFS helper reads it there"
    );
    assert_eq!(there.storage_label, "10.90.0.1");
    for option in ["nfsvers=4.2", "hard", "lookupcache=positive"] {
        assert!(
            MOUNT_OPTIONS.split(',').any(|set| set == option),
            "{option}"
        );
    }
    assert!(
        !MOUNT_OPTIONS.contains("soft"),
        "a soft mount can lose writes"
    );

    // The engine volume is named after what it mounts, so a node never
    // keeps one that points at another storage node.
    assert!(
        there
            .name
            .starts_with("mm-shared-019c527b6242-production-app-uploads-")
    );
    assert_ne!(there.name, here.name);
    assert_ne!(there.name, over_mesh("10.90.0.2")?.name);
    assert_eq!(there.name, over_mesh("10.90.0.1")?.name);

    let unavailable = StorageAccess::Unavailable {
        node: "app-1".to_owned(),
        reason: "run `mm target setup plan`".to_owned(),
    };
    assert_eq!(
        unavailable
            .engine_volume(project()?, "production", &uploads())
            .err()
            .as_deref(),
        Some("run `mm target setup plan`")
    );
    Ok(())
}

const NODE_VIEW_CONFIG: &str = "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1df0
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
    shared-volumes: [uploads]
  jobs:
    files: [jobs.yaml]
modules:
  shop:
    path: .
    services:
      web: {stack: app, placement: all}
environments:
  production: {target: production}
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
";

#[test]
fn a_node_learns_where_it_finds_the_shared_volumes() -> TestResult {
    use mm::domain::config::MmConfig;
    use mm::domain::placement::{NodeScope, view_from_node};

    let view = |config: &str,
                node: &str,
                known: &[(&str, &str)]|
     -> Result<MmConfig, Box<dyn std::error::Error>> {
        let mut config: MmConfig = serde_yaml_ng::from_str(config)?;
        assert!(
            config.stacks["app"].storage.is_none(),
            "never read from a file"
        );
        let mut mesh_addresses = BTreeMap::new();
        for (name, address) in known {
            mesh_addresses.insert((*name).to_owned(), ip(address)?);
        }
        view_from_node(
            &mut config,
            &NodeScope {
                target: "production".to_owned(),
                node: node.to_owned(),
                environment: "production".to_owned(),
                resolver_files: None,
                mesh_addresses,
            },
        )?;
        Ok(config)
    };
    let both = [("app-1", "10.90.0.1"), ("app-2", "10.90.0.2")];

    // The first node keeps the volumes unless `cluster.storage` names one.
    let first = view(NODE_VIEW_CONFIG, "app-1", &both)?;
    assert_eq!(
        first.stacks["app"].storage,
        Some(StorageAccess::Directory {
            node: "app-1".to_owned(),
            root: "/var/lib/mm/storage".into()
        })
    );
    assert!(
        first.stacks["jobs"].storage.is_none(),
        "a stack that shares nothing"
    );
    assert!(!serde_yaml_ng::to_string(&first)?.contains("storage"));
    assert_eq!(
        view(NODE_VIEW_CONFIG, "app-2", &both)?.stacks["app"].storage,
        Some(StorageAccess::Mesh {
            node: "app-1".to_owned(),
            address: ip("10.90.0.1")?
        })
    );
    let chosen = NODE_VIEW_CONFIG.replace(
        "    nodes:\n",
        "    cluster: {storage: app-2}\n    nodes:\n",
    );
    assert!(matches!(
        view(&chosen, "app-2", &both)?.stacks["app"].storage,
        Some(StorageAccess::Directory { .. })
    ));

    // A node whose mesh does not know the storage node says what to run.
    let Some(StorageAccess::Unavailable { reason, .. }) = view(NODE_VIEW_CONFIG, "app-2", &[])?
        .stacks["app"]
        .storage
        .clone()
    else {
        return Err("a node without the storage node's address cannot mount".into());
    };
    assert!(reason.contains("mm target setup plan"), "{reason}");

    // An existing private interface: no mesh of mm's, no shared volumes.
    let own = NODE_VIEW_CONFIG.replace(
        "    nodes:\n      - {name: app-1, host: 203.0.113.11}\n      - {name: app-2, host: 203.0.113.12}\n",
        "    cluster: {mesh: {interface: eth1}}\n    nodes:\n      - {name: app-1, host: 203.0.113.11, private-address: 10.0.0.1}\n      - {name: app-2, host: 203.0.113.12, private-address: 10.0.0.2}\n",
    );
    let Some(StorageAccess::Unavailable { reason, .. }) =
        view(&own, "app-1", &both)?.stacks["app"].storage.clone()
    else {
        return Err("an existing interface serves no shared volumes".into());
    };
    assert!(reason.contains("cluster.mesh.interface"), "{reason}");
    Ok(())
}

#[test]
fn containers_are_kept_away_from_the_storage_port() -> TestResult {
    let firewall = StorageFirewall::of(&mesh(1)?);
    let tail = "-p tcp -m tcp --dport 2049 -m comment --comment \"mm shared storage\"";
    assert_eq!(
        firewall.forward,
        [format!("-A MM-STORAGE -o mm0 {tail} -j DROP")],
        "nothing a container sends reaches a storage port through the mesh"
    );
    assert_eq!(
        firewall.input,
        [
            format!("-A MM-STORAGE-IN -s 10.90.0.1/32 -d 10.90.0.1/32 -i lo {tail} -j RETURN"),
            format!("-A MM-STORAGE-IN -s 10.90.0.2/32 -d 10.90.0.1/32 -i mm0 {tail} -j RETURN"),
            format!("-A MM-STORAGE-IN -s 10.90.0.3/32 -d 10.90.0.1/32 -i mm0 {tail} -j RETURN"),
            format!("-A MM-STORAGE-IN -d 10.90.0.1/32 {tail} -j DROP"),
        ],
        "the node's own mesh address takes storage traffic from the other nodes' mesh addresses only: a container's address (a slice) is none of them"
    );
    let restore = firewall.restore_input(forward_hook(RuntimeKind::DockerCompose));
    assert!(restore.starts_with("*filter\n:MM-STORAGE - [0:0]\n:MM-STORAGE-IN - [0:0]\n"));
    assert!(restore.ends_with("-j DROP\nCOMMIT\n"));
    assert_eq!(restore.lines().count(), 3 + 1 + 4 + 1);

    // On a Podman host the forward chain is in the mangle table, the
    // input chain stays in the filter table.
    let restore = firewall.restore_input(forward_hook(RuntimeKind::PodmanCompose));
    assert_eq!(
        restore,
        format!(
            "*mangle\n:MM-STORAGE - [0:0]\n{}\nCOMMIT\n*filter\n:MM-STORAGE-IN - [0:0]\n{}\nCOMMIT\n",
            firewall.forward.join("\n"),
            firewall.input.join("\n")
        )
    );
    Ok(())
}

fn used(node_is_storage: bool, services: &[&str]) -> Vec<SharedVolumeUse> {
    vec![SharedVolumeUse {
        stack: "app".to_owned(),
        volume: "uploads".to_owned(),
        storage_node: "app-1".to_owned(),
        local: node_is_storage,
        directory: format!("{PROJECT}/production/app/uploads"),
        engine_volume: "mm-shared-x".to_owned(),
        services: services
            .iter()
            .map(|service| (*service).to_owned())
            .collect(),
    }]
}

#[test]
fn a_plan_names_the_node_that_keeps_each_volume_and_the_nodes_that_mount_it() -> TestResult {
    assert_eq!(
        ClusterStoragePlan::of(&[("app-1", &[]), ("app-2", &[])])?,
        None
    );

    let (first, second, third) = (
        used(true, &["web"]),
        used(false, &["web"]),
        used(false, &["web", "worker"]),
    );
    let plan = ClusterStoragePlan::of(&[("app-1", &first), ("app-2", &second), ("app-3", &third)])?
        .ok_or("the plan shares a volume")?;
    assert_eq!(plan.node, "app-1");
    let volume = &plan.volumes[0];
    assert_eq!(
        volume.path,
        format!("/var/lib/mm/storage/{PROJECT}/production/app/uploads")
    );
    assert!(volume.used_on_storage_node);
    assert_eq!(volume.mounted_on, ["app-2", "app-3"]);
    assert_eq!(volume.services["web"], ["app-1", "app-2", "app-3"]);
    assert_eq!(volume.services["worker"], ["app-3"]);
    assert_eq!(
        plan.lines(),
        [format!(
            "app/uploads: kept on app-1 in /var/lib/mm/storage/{PROJECT}/production/app/uploads, used there directly, mounted over the mesh on app-2, app-3 (for web, worker)"
        )]
    );

    // What the nodes keep under the plan's hash.
    let sealed = plan.sealed();
    assert_eq!(
        sealed,
        SealedStorage {
            node: "app-1".to_owned(),
            directories: vec![format!("{PROJECT}/production/app/uploads")],
            mounting: vec!["app-2".to_owned(), "app-3".to_owned()],
        }
    );
    sealed.validate()?;
    for bad in [
        "../../etc",
        "a/b/c",
        "/abs/production/app/uploads",
        "a/b/c/d/e",
    ] {
        let escaped = SealedStorage {
            directories: vec![bad.to_owned()],
            ..sealed.clone()
        };
        assert!(escaped.validate().is_err(), "{bad}");
    }

    // Nodes that name different storage nodes were set up differently.
    let mut elsewhere = used(false, &["web"]);
    elsewhere[0].storage_node = "app-2".to_owned();
    let refused = ClusterStoragePlan::of(&[("app-1", &first), ("app-3", &elsewhere)]);
    assert!(
        refused
            .err()
            .is_some_and(|problem| problem.contains("mm target setup plan")),
    );
    Ok(())
}

#[test]
fn the_plans_hash_covers_its_shared_volumes() -> TestResult {
    let scope = RemoteScope {
        project_id: project()?,
        environment: "production".to_owned(),
        target: "production".to_owned(),
        target_fingerprint: "f".repeat(64),
        runtime: mm::domain::runtime::RuntimeKind::DockerCompose,
        node: None,
    };
    let seal = |storage: Option<SealedStorage>| {
        ClusterPlanSeal::new(
            RemotePlanKind::Deployment,
            &scope,
            &"c".repeat(40),
            vec![ClusterPlanSection {
                node: "app-1".to_owned(),
                plan_hash: "1".repeat(64),
            }],
            Vec::new(),
            SealCarried {
                storage,
                ..SealCarried::default()
            },
        )
    };
    let storage = |directory: &str| SealedStorage {
        node: "app-1".to_owned(),
        directories: vec![directory.to_owned()],
        mounting: Vec::new(),
    };
    let none = seal(None)?;
    let one = seal(Some(storage("p/production/app/uploads")))?;
    let other = seal(Some(storage("p/production/app/assets")))?;
    assert_ne!(none.cluster_plan_hash, one.cluster_plan_hash);
    assert_ne!(one.cluster_plan_hash, other.cluster_plan_hash);
    one.validate()?;
    assert!(seal(Some(storage("../outside"))).is_err());
    // A plan made before shared volumes existed keeps its hash.
    assert!(!serde_json::to_string(&none)?.contains("storage"));
    Ok(())
}

const MOUNTINFO: &str = "\
29 1 254:1 / / rw,relatime shared:1 - ext4 /dev/vda1 rw
258 29 0:80 / /var/lib/docker/volumes/mm-shared-a/_data rw,relatime shared:333 - nfs4 :/019c527b-6242-7b43-bfd4-bd60e27c1df0/production/app/uploads rw,vers=4.2,addr=10.90.0.1
259 29 0:81 / /var/lib/docker/volumes/mm-shared-b/_data rw,relatime shared:334 - nfs4 :/019c527b-6242-7b43-bfd4-bd60e27c1df0/staging/app/uploads rw,vers=4.2,addr=10.90.0.1
260 29 254:1 /var/lib/mm/storage/019c527b-6242-7b43-bfd4-bd60e27c1df0/production/app/assets /var/lib/docker/volumes/mm-shared-c/_data rw,relatime shared:1 - ext4 /dev/vda1 rw
261 29 254:1 /var/lib/docker/volumes/other/_data /var/lib/docker/overlay2/x/merged/data rw - ext4 /dev/vda1 rw
262 29 0:82 / /mnt/else rw - nfs4 fileserver:/exports/production/app/uploads rw
";

#[test]
fn what_a_host_has_mounted_is_read_from_the_kernels_list() -> TestResult {
    assert_eq!(
        mounted_volumes(MOUNTINFO, project()?, "production"),
        [VolumeName::new("app", "assets"), uploads()],
        "a mount over the mesh by its source, the storage node's own use by the root of its bind mount"
    );
    assert_eq!(
        mounted_volumes(MOUNTINFO, project()?, "staging"),
        [uploads()]
    );
    assert!(mounted_volumes("", project()?, "production").is_empty());
    Ok(())
}

fn waiting(state: &str, policy: &str, error: &str, storage: &str) -> WaitingContainer {
    WaitingContainer {
        id: "c1".to_owned(),
        name: "shop-web-1".to_owned(),
        storage: storage.to_owned(),
        state: state.to_owned(),
        error: error.to_owned(),
        restart_policy: policy.to_owned(),
    }
}

const MOUNT_FAILED: &str = "error while mounting volume '/var/lib/docker/volumes/mm-shared-019c527b6242-production-app-uploads-1a2b3c4d/_data': failed to mount local volume: mount :/p/production/app/uploads:/var/lib/docker/volumes/mm-shared-019c527b6242-production-app-uploads-1a2b3c4d/_data, data: addr=10.90.0.1,nfsvers=4.2,hard: connection timed out";

#[test]
fn only_a_container_that_could_not_mount_its_shared_volume_waits_for_storage() -> TestResult {
    let address = Some(ip("10.90.0.1")?);
    assert_eq!(
        waiting("created", "unless-stopped", MOUNT_FAILED, "10.90.0.1").waits_for(),
        address
    );
    assert_eq!(
        waiting("exited", "always", MOUNT_FAILED, "10.90.0.1").waits_for(),
        address
    );
    for (state, policy, error, storage, why) in [
        (
            "exited",
            "unless-stopped",
            "",
            "10.90.0.1",
            "someone stopped it, or it ended by itself",
        ),
        (
            "created",
            "no",
            MOUNT_FAILED,
            "10.90.0.1",
            "its policy says it stays down",
        ),
        ("running", "always", MOUNT_FAILED, "10.90.0.1", "it runs"),
        (
            "exited",
            "always",
            "OCI runtime create failed: exec: not found",
            "10.90.0.1",
            "it failed for another reason",
        ),
        (
            "created",
            "always",
            "error while mounting volume '/var/lib/docker/volumes/data/_data'",
            "10.90.0.1",
            "another volume failed",
        ),
        (
            "created",
            "always",
            MOUNT_FAILED,
            "local",
            "the storage node itself mounts nothing",
        ),
    ] {
        assert_eq!(
            waiting(state, policy, error, storage).waits_for(),
            None,
            "{why}"
        );
    }
    Ok(())
}

fn node_report(name: &str, status: Option<StorageNodeStatus>) -> StorageNodeReport {
    StorageNodeReport {
        name: name.to_owned(),
        answered: status.is_some(),
        problem: status
            .is_none()
            .then(|| "ssh: connection timed out".to_owned()),
        status,
    }
}

fn serving(volumes: Vec<StoredVolume>, mounted: Vec<VolumeName>) -> StorageNodeStatus {
    StorageNodeStatus {
        node: Some("app-1".to_owned()),
        serves: true,
        exported: Some(true),
        address: Some(Ipv4Addr::new(10, 90, 0, 1)),
        reachable: true,
        reach_ms: Some(0),
        mounted,
        volumes,
        free_bytes: Some(20 << 30),
        total_bytes: Some(30 << 30),
        ..StorageNodeStatus::default()
    }
}

fn mounting(node: &str, reachable: bool, mounted: Vec<VolumeName>) -> StorageNodeStatus {
    StorageNodeStatus {
        node: Some(node.to_owned()),
        address: Some(Ipv4Addr::new(10, 90, 0, 1)),
        reachable,
        reach_ms: reachable.then_some(58),
        problem: (!reachable).then(|| "10.90.0.1:2049 did not answer within 3 s".to_owned()),
        mounted,
        ..StorageNodeStatus::default()
    }
}

fn stored(volume: &str, bytes: u64) -> StoredVolume {
    StoredVolume {
        stack: "app".to_owned(),
        volume: volume.to_owned(),
        path: format!("/var/lib/mm/storage/{PROJECT}/production/app/{volume}"),
        bytes: Some(bytes),
        files: Some(3),
        counted: true,
    }
}

fn report(nodes: Vec<StorageNodeReport>, declared: &[VolumeName]) -> StorageReport {
    StorageReport::new(
        (
            "production".to_owned(),
            "production".to_owned(),
            "app-1".to_owned(),
        ),
        declared,
        nodes,
    )
}

#[test]
fn a_status_puts_every_nodes_answer_together() -> TestResult {
    let declared = [uploads(), VolumeName::new("app", "assets")];
    let healthy = report(
        vec![
            node_report(
                "app-1",
                Some(serving(
                    vec![stored("uploads", 4096), stored("old", 1 << 20)],
                    vec![uploads()],
                )),
            ),
            node_report("app-2", Some(mounting("app-2", true, vec![uploads()]))),
            node_report("app-3", Some(mounting("app-3", true, Vec::new()))),
        ],
        &declared,
    );
    assert!(healthy.healthy(), "{:?}", healthy.problems);
    assert_eq!(healthy.address, Some(ip("10.90.0.1")?));
    assert_eq!(healthy.free_bytes, Some(20 << 30));
    let volume = |name: &str| {
        healthy
            .volumes
            .iter()
            .find(|volume| volume.volume == name)
            .ok_or_else(|| format!("no volume {name}"))
    };
    assert_eq!(volume("uploads")?.mounted_on, ["app-1", "app-2"]);
    assert_eq!(volume("uploads")?.bytes, Some(4096));
    assert!(volume("uploads")?.declared && volume("uploads")?.stored);
    assert!(!volume("assets")?.stored, "declared and not created yet");
    // Out of the configuration, and its files are still there.
    assert!(!volume("old")?.declared && volume("old")?.stored);
    assert_eq!(
        healthy
            .undeclared()
            .map(|volume| volume.volume.as_str())
            .collect::<Vec<_>>(),
        ["old"]
    );
    assert_eq!(
        healthy.nodes[0].summary(true).as_deref(),
        Some("keeps shared volumes")
    );
    assert_eq!(
        healthy.nodes[1].summary(true).as_deref(),
        Some("1 shared volume mounted")
    );
    assert_eq!(healthy.nodes[2].summary(true), None);
    Ok(())
}

#[test]
fn a_status_says_what_is_wrong_and_what_to_do() -> TestResult {
    let declared = [uploads()];
    let codes = |report: &StorageReport| {
        report
            .problems
            .iter()
            .map(|problem| (problem.code.clone(), problem.severity))
            .collect::<Vec<_>>()
    };

    let cut_off = report(
        vec![
            node_report(
                "app-1",
                Some(serving(vec![stored("uploads", 1)], Vec::new())),
            ),
            node_report("app-2", Some(mounting("app-2", false, vec![uploads()]))),
            node_report("app-3", None),
        ],
        &declared,
    );
    assert!(!cut_off.healthy());
    assert_eq!(
        codes(&cut_off),
        [
            ("MM_STORAGE_UNREACHABLE".to_owned(), Severity::Error),
            ("MM_STORAGE_NODE_SILENT".to_owned(), Severity::Error),
        ]
    );
    let unreachable = &cut_off.problems[0];
    assert!(
        unreachable
            .message
            .contains("node 'app-2' cannot reach shared storage on app-1")
    );
    assert!(
        unreachable
            .help
            .as_deref()
            .is_some_and(|help| help.contains("mm target mesh status")),
    );
    assert_eq!(
        cut_off.nodes[1].summary(true).as_deref(),
        Some("cannot reach shared storage")
    );

    // A cluster that shares nothing loses nothing while a node cannot
    // reach the storage node: a warning, and the command succeeds.
    let unused = report(
        vec![
            node_report("app-1", Some(serving(Vec::new(), Vec::new()))),
            node_report("app-2", Some(mounting("app-2", false, Vec::new()))),
        ],
        &[],
    );
    assert_eq!(
        codes(&unused),
        [("MM_STORAGE_UNREACHABLE".to_owned(), Severity::Warning)]
    );
    assert!(unused.healthy());
    assert_eq!(unused.nodes[1].summary(false), None);

    // The storage node was never set up to serve.
    let mut never = serving(Vec::new(), Vec::new());
    never.exported = Some(false);
    never.reachable = false;
    let not_served = report(vec![node_report("app-1", Some(never))], &declared);
    assert_eq!(
        codes(&not_served),
        [("MM_STORAGE_NOT_SERVED".to_owned(), Severity::Error)]
    );
    assert!(
        not_served.problems[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("mm target setup plan")),
    );

    // The storage node itself is silent.
    let silent = report(
        vec![
            node_report("app-1", None),
            node_report("app-2", Some(mounting("app-2", false, vec![uploads()]))),
        ],
        &declared,
    );
    assert!(
        silent.problems[0]
            .message
            .contains("keeps the shared volumes")
    );
    assert!(silent.volumes[0].path.is_none() && !silent.volumes[0].stored);
    Ok(())
}

struct Mesh(Option<MeshState>);

fn unsupported<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not part of this test".to_owned(),
    })
}

#[async_trait]
impl MeshHost for Mesh {
    async fn facts(&self) -> Result<MeshFacts, MmError> {
        unsupported()
    }
    fn stored(&self) -> Result<Option<MeshState>, MmError> {
        Ok(self.0.clone())
    }
    async fn ensure_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn apply(&self, _desired: &MeshState) -> Result<MeshApplied, MmError> {
        unsupported()
    }
    async fn up(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
    async fn status(&self) -> Result<MeshNodeStatus, MmError> {
        unsupported()
    }
    async fn stage_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn activate_key(&self) -> Result<String, MmError> {
        unsupported()
    }
    async fn leave(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
}

/// A packet filter that answers from a table of command lines and records
/// every one. A command it does not know fails, as `iptables` does for a
/// chain or a rule that is not there.
#[derive(Default)]
struct Filter {
    answers: Mutex<BTreeMap<String, String>>,
    commands: Mutex<Vec<String>>,
}

impl Filter {
    fn knows(&self, command: &str, stdout: &str) -> Result<(), &'static str> {
        self.answers
            .lock()
            .map_err(|_| "poisoned")?
            .insert(command.to_owned(), stdout.to_owned());
        Ok(())
    }

    fn commands(&self) -> Result<Vec<String>, &'static str> {
        Ok(self.commands.lock().map_err(|_| "poisoned")?.clone())
    }
}

#[async_trait]
impl ProcessRunner for Filter {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let poisoned = || MmError::State {
            message: "the test filter's lock was poisoned".to_owned(),
        };
        let record = specification.redacted_record();
        let command = format!("{} {}", record.program, record.arguments.join(" "));
        self.commands
            .lock()
            .map_err(|_| poisoned())?
            .push(command.clone());
        let changes = ["-N", "-I", "-D", "-F", "-X"]
            .iter()
            .any(|action| record.arguments.iter().any(|argument| argument == action))
            || record.program == "iptables-restore";
        let answer = self
            .answers
            .lock()
            .map_err(|_| poisoned())?
            .get(&command)
            .cloned();
        Ok(CommandOutput {
            code: Some(i32::from(answer.is_none() && !changes)),
            stdout: Zeroizing::new(answer.unwrap_or_default()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

fn host(
    root: &Path,
    filter: Arc<Filter>,
    mesh: Option<MeshState>,
) -> Result<HostSharedStorage, std::io::Error> {
    std::fs::create_dir_all(root.join("storage"))?;
    Ok(
        HostSharedStorage::new(filter, Arc::new(Mesh(mesh))).with_paths(StoragePaths {
            root: root.join("storage"),
            exports: root.join("mm-storage.exports"),
            mountinfo: root.join("mountinfo"),
        }),
    )
}

#[tokio::test]
async fn on_a_podman_host_containers_are_kept_from_storage_in_the_mangle_table() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let filter = Arc::new(Filter::default());
    let storage = host(temporary.path(), filter.clone(), Some(mesh(2)?))?
        .for_engine(RuntimeKind::PodmanCompose);
    assert!(storage.up().await?.changed);
    let commands = filter.commands()?;
    assert_eq!(
        commands
            .iter()
            .filter(|command| !command.contains(" -S ") && !command.contains(" -C "))
            .collect::<Vec<_>>(),
        [
            "iptables-restore --noflush --wait 5",
            "iptables -w -t mangle -I FORWARD 1 -j MM-STORAGE",
            "iptables -w -t filter -I INPUT 1 -j MM-STORAGE-IN",
        ],
        "Podman has no DOCKER-USER: the forward chain is entered from mangle FORWARD, which is never created: {commands:?}"
    );
    assert!(
        commands.contains(&"iptables -w -t mangle -S MM-STORAGE".to_owned()),
        "{commands:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_rules_are_loaded_once_and_left_alone_when_in_place() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let filter = Arc::new(Filter::default());
    let storage = host(temporary.path(), filter.clone(), Some(mesh(2)?))?;

    // Nothing is loaded yet, and the engine has never started.
    let applied = storage.up().await?;
    assert!(applied.changed, "{applied:?}");
    assert!(applied.actions[0].contains("MM-STORAGE and MM-STORAGE-IN"));
    let commands = filter.commands()?;
    assert_eq!(
        commands
            .iter()
            .filter(|command| !command.contains(" -S ") && !command.contains(" -C "))
            .collect::<Vec<_>>(),
        [
            "iptables-restore --noflush --wait 5",
            "iptables -w -t filter -N DOCKER-USER",
            "iptables -w -t filter -I DOCKER-USER 1 -j MM-STORAGE",
            "iptables -w -t filter -I INPUT 1 -j MM-STORAGE-IN",
        ],
        "both chains replaced as a whole, then entered first: {commands:?}"
    );

    // The host holds exactly the rules: nothing is written.
    let wanted = StorageFirewall::of(&mesh(2)?);
    let listed = |chain: &str, rules: &[String]| format!("-N {chain}\n{}\n", rules.join("\n"));
    let in_place = Arc::new(Filter::default());
    in_place.knows(
        "iptables -w -t filter -S MM-STORAGE",
        &listed("MM-STORAGE", &wanted.forward),
    )?;
    in_place.knows(
        "iptables -w -t filter -S MM-STORAGE-IN",
        &listed("MM-STORAGE-IN", &wanted.input),
    )?;
    in_place.knows("iptables -w -t filter -C DOCKER-USER -j MM-STORAGE", "")?;
    in_place.knows("iptables -w -t filter -C INPUT -j MM-STORAGE-IN", "")?;
    let storage = host(temporary.path(), in_place.clone(), Some(mesh(2)?))?;
    assert_eq!(storage.up().await?, MeshApplied::default());
    assert!(
        in_place
            .commands()?
            .iter()
            .all(|command| command.contains(" -S ") || command.contains(" -C ")),
        "{:?}",
        in_place.commands()?
    );

    // A host that is no cluster node has no mesh and gets no rule.
    let alone = Arc::new(Filter::default());
    let storage = host(temporary.path(), alone.clone(), None)?;
    assert_eq!(storage.up().await?, MeshApplied::default());
    assert!(alone.commands()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn directories_are_created_once_and_only_the_delete_removes_one() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let storage = host(temporary.path(), Arc::new(Filter::default()), None)?;
    let root = temporary.path().join("storage");
    let uploads_directory = format!("{PROJECT}/production/app/uploads");
    let assets_directory = format!("{PROJECT}/production/app/assets");
    let both = [uploads_directory.clone(), assets_directory.clone()];

    assert_eq!(storage.create_directories(&both)?, both);
    // Every part is readable and searchable by a container's user.
    for part in [
        PROJECT.to_owned(),
        format!("{PROJECT}/production"),
        uploads_directory.clone(),
    ] {
        let mode = std::fs::metadata(root.join(part))?.permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    // A second deployment, a rollback, a failed apply: what is there stays.
    std::fs::write(root.join(&uploads_directory).join("photo.jpg"), b"12345")?;
    std::fs::create_dir(root.join(&uploads_directory).join("thumbs"))?;
    std::fs::write(root.join(&uploads_directory).join("thumbs/small.jpg"), b"1")?;
    std::fs::set_permissions(
        root.join(&uploads_directory),
        std::fs::Permissions::from_mode(0o770),
    )?;
    assert!(storage.create_directories(&both)?.is_empty());
    assert_eq!(
        std::fs::read(root.join(&uploads_directory).join("photo.jpg"))?,
        b"12345"
    );
    assert_eq!(
        std::fs::metadata(root.join(&uploads_directory))?
            .permissions()
            .mode()
            & 0o777,
        0o770,
        "a directory that exists keeps the mode its service gave it"
    );

    // Nothing outside the root, and nothing through a link.
    for escape in ["../outside", "/abs", "a//b", ""] {
        assert!(
            storage.create_directories(&[escape.to_owned()]).is_err(),
            "{escape}"
        );
    }
    std::os::unix::fs::symlink(temporary.path(), root.join("linked"))?;
    assert!(
        storage
            .create_directories(&["linked/production/app/x".to_owned()])
            .is_err()
    );
    assert!(!temporary.path().join("production").exists());

    // What the storage node keeps, counted.
    let kept = storage.stored(project()?, "production", true).await?;
    assert_eq!(
        kept.volumes
            .iter()
            .map(|volume| volume.volume.as_str())
            .collect::<Vec<_>>(),
        ["assets", "uploads"]
    );
    let counted = &kept.volumes[1];
    assert_eq!(counted.files, Some(2));
    assert!(counted.counted && counted.bytes.is_some());
    assert_eq!(
        counted.path,
        root.join(&uploads_directory).to_string_lossy()
    );
    assert!(kept.total_bytes.is_some_and(|total| total > 0));
    let uncounted = storage.stored(project()?, "production", false).await?;
    assert_eq!(uncounted.volumes[1].bytes, None);
    assert!(
        storage
            .stored(project()?, "staging", true)
            .await?
            .volumes
            .is_empty()
    );

    // The one delete takes the volume and nothing beside it.
    storage.delete(&uploads_directory).await?;
    assert!(!root.join(&uploads_directory).exists());
    assert!(
        root.join(&assets_directory).is_dir(),
        "the other volume stays"
    );
    for escape in ["..", "../storage", "linked"] {
        assert!(storage.delete(escape).await.is_err(), "{escape}");
    }
    assert!(
        temporary
            .path()
            .join("mountinfo")
            .parent()
            .is_some_and(Path::is_dir)
    );
    // With its last volume an environment's directory goes; the root stays.
    storage.delete(&assets_directory).await?;
    assert!(!root.join(PROJECT).exists());
    assert!(root.is_dir());
    Ok(())
}

/// A storage host the test scripts.
#[derive(Default)]
struct Host {
    exported: bool,
    reachable: bool,
    mounted: Vec<VolumeName>,
    stored: Vec<StoredVolume>,
    created: Mutex<Vec<String>>,
    deleted: Mutex<Vec<String>>,
    asked: Mutex<Vec<Ipv4Addr>>,
}

#[async_trait]
impl SharedStorageHost for Host {
    fn exported(&self) -> bool {
        self.exported
    }
    fn create_directories(&self, directories: &[String]) -> Result<Vec<String>, MmError> {
        let mut created = self.created.lock().map_err(|_| MmError::State {
            message: "poisoned".to_owned(),
        })?;
        created.extend(directories.iter().cloned());
        Ok(directories.to_vec())
    }
    async fn reach(&self, address: Ipv4Addr) -> Result<u64, String> {
        self.asked.lock().map_err(|_| "poisoned")?.push(address);
        if self.reachable {
            Ok(1)
        } else {
            Err(format!("{address}:2049 did not answer within 3 s"))
        }
    }
    fn mounted(&self, _project: ProjectId, _environment: &str) -> Result<Vec<VolumeName>, MmError> {
        Ok(self.mounted.clone())
    }
    async fn stored(
        &self,
        _project: ProjectId,
        _environment: &str,
        sizes: bool,
    ) -> Result<StoredVolumes, MmError> {
        let mut volumes = self.stored.clone();
        if !sizes {
            for volume in &mut volumes {
                volume.bytes = None;
                volume.files = None;
            }
        }
        Ok(StoredVolumes {
            volumes,
            free_bytes: Some(1),
            total_bytes: Some(2),
        })
    }
    async fn delete(&self, directory: &str) -> Result<(), MmError> {
        self.deleted
            .lock()
            .map_err(|_| MmError::State {
                message: "poisoned".to_owned(),
            })?
            .push(directory.to_owned());
        Ok(())
    }
    async fn up(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
    async fn leave(&self) -> Result<MeshApplied, MmError> {
        unsupported()
    }
}

fn node(host: Arc<Host>, own: u8) -> Result<SharedStorageNode, Box<dyn std::error::Error>> {
    Ok(SharedStorageNode::new(
        host,
        Arc::new(Mesh(Some(mesh(own)?))),
    ))
}

fn sealed() -> SealedStorage {
    SealedStorage {
        node: "app-1".to_owned(),
        directories: vec![format!("{PROJECT}/production/app/uploads")],
        mounting: vec!["app-2".to_owned()],
    }
}

#[tokio::test]
async fn a_rollout_makes_its_volumes_ready_and_refuses_before_anything_changes() -> TestResult {
    // The storage node creates the plan's directories.
    let serving = Arc::new(Host {
        exported: true,
        ..Host::default()
    });
    assert_eq!(
        node(serving.clone(), 1)?
            .prepare("app-1", &sealed())
            .await?,
        sealed().directories
    );
    assert!(serving.asked.lock().map_err(|_| "poisoned")?.is_empty());

    // One that setup never made a storage node says what to run.
    let unprepared = Arc::new(Host::default());
    let refused = node(unprepared.clone(), 1)?
        .prepare("app-1", &sealed())
        .await;
    let Err(error) = refused else {
        return Err("a node that does not serve creates nothing".into());
    };
    assert_eq!(error.code(), "MM_STORAGE_NOT_SERVED");
    assert!(
        error.to_string().contains("mm target setup plan"),
        "{error}"
    );
    assert!(
        unprepared
            .created
            .lock()
            .map_err(|_| "poisoned")?
            .is_empty()
    );

    // A node that mounts checks that it reaches the storage node, at the
    // storage node's mesh address.
    let reaching = Arc::new(Host {
        reachable: true,
        ..Host::default()
    });
    assert!(
        node(reaching.clone(), 2)?
            .prepare("app-2", &sealed())
            .await?
            .is_empty()
    );
    assert_eq!(
        *reaching.asked.lock().map_err(|_| "poisoned")?,
        [ip("10.90.0.1")?]
    );
    assert!(reaching.created.lock().map_err(|_| "poisoned")?.is_empty());

    let cut_off = Arc::new(Host::default());
    let Err(error) = node(cut_off, 2)?.prepare("app-2", &sealed()).await else {
        return Err("a node that cannot reach storage refuses".into());
    };
    assert_eq!(error.code(), "MM_STORAGE_UNREACHABLE");
    let text = error.to_string();
    assert!(
        text.contains("app-1") && text.contains("mm target mesh status"),
        "{text}"
    );
    assert!(text.contains("nothing was changed"), "{text}");

    // A node the plan gives no shared volume has nothing to check.
    let idle = Arc::new(Host::default());
    assert!(
        node(idle.clone(), 3)?
            .prepare("app-3", &sealed())
            .await?
            .is_empty()
    );
    assert!(idle.asked.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_node_reports_what_it_keeps_mounts_and_reaches() -> TestResult {
    let scope = |node: &'static str| -> Result<StorageScope<'static>, uuid::Error> {
        Ok(StorageScope {
            node: Some(node),
            project: project()?,
            environment: "production",
        })
    };
    let serving = Arc::new(Host {
        exported: true,
        reachable: true,
        mounted: vec![uploads()],
        stored: vec![stored("uploads", 4096)],
        ..Host::default()
    });
    let status = node(serving.clone(), 1)?
        .status(scope("app-1")?, "app-1", true)
        .await?;
    assert!(status.serves && status.reachable);
    assert_eq!(status.exported, Some(true));
    assert_eq!(
        status.address,
        Some(ip("10.90.0.1")?),
        "its own mesh address"
    );
    assert_eq!(status.volumes[0].bytes, Some(4096));
    assert_eq!(status.mounted, [uploads()]);
    assert_eq!((status.free_bytes, status.total_bytes), (Some(1), Some(2)));

    let cut_off = Arc::new(Host {
        mounted: vec![uploads()],
        stored: vec![stored("leftover", 1)],
        ..Host::default()
    });
    let status = node(cut_off, 3)?
        .status(scope("app-3")?, "app-1", true)
        .await?;
    assert!(!status.serves && !status.reachable);
    assert_eq!(status.exported, None);
    assert!(
        status.volumes.is_empty(),
        "only the storage node lists what it keeps"
    );
    assert_eq!(status.address, Some(ip("10.90.0.1")?));
    assert!(
        status
            .problem
            .is_some_and(|problem| problem.contains("did not answer"))
    );

    // A node whose mesh does not know the storage node says so.
    let status = node(Arc::new(Host::default()), 2)?
        .status(scope("app-2")?, "app-9", false)
        .await?;
    assert!(
        status
            .problem
            .is_some_and(|problem| problem.contains("no peer named 'app-9'"))
    );
    Ok(())
}

#[tokio::test]
async fn deleting_needs_the_reviewed_hash_and_a_volume_nobody_uses() -> TestResult {
    let scope = StorageScope {
        node: Some("app-1"),
        project: project()?,
        environment: "production",
    };
    let kept = Arc::new(Host {
        exported: true,
        stored: vec![stored("uploads", 4096)],
        ..Host::default()
    });
    let storage = node(kept.clone(), 1)?;
    let plan = storage.plan_delete(scope, &uploads()).await?;
    assert_eq!((plan.bytes, plan.files, plan.counted), (4096, 3, true));
    assert_eq!(plan.node, "app-1");
    assert_eq!(
        plan.plan_hash,
        VolumeDeletePlan::hash_of(project()?, "production", "app-1", &plan.path)
    );
    assert!(
        kept.deleted.lock().map_err(|_| "poisoned")?.is_empty(),
        "a plan removes nothing"
    );

    // Another hash, another volume's hash: refused.
    let other = VolumeDeletePlan::hash_of(project()?, "staging", "app-1", &plan.path);
    for wrong in [other.as_str(), ""] {
        assert!(storage.delete(scope, &uploads(), wrong).await.is_err());
    }
    assert!(kept.deleted.lock().map_err(|_| "poisoned")?.is_empty());

    let deleted = storage.delete(scope, &uploads(), &plan.plan_hash).await?;
    assert_eq!(deleted.path, plan.path);
    assert_eq!(
        *kept.deleted.lock().map_err(|_| "poisoned")?,
        [format!("{PROJECT}/production/app/uploads")]
    );

    // A volume the node does not keep, a volume in use, a node that does
    // not serve.
    let missing = storage
        .plan_delete(scope, &VolumeName::new("app", "none"))
        .await;
    assert!(
        missing
            .err()
            .is_some_and(|error| error.to_string().contains("mm target storage status"))
    );
    let in_use = Arc::new(Host {
        exported: true,
        mounted: vec![uploads()],
        stored: vec![stored("uploads", 1)],
        ..Host::default()
    });
    let Err(error) = node(in_use, 1)?.plan_delete(scope, &uploads()).await else {
        return Err("a volume a container uses is not deleted".into());
    };
    assert_eq!(error.code(), "MM_STORAGE_IN_USE");
    let Err(error) = node(Arc::new(Host::default()), 2)?
        .plan_delete(scope, &uploads())
        .await
    else {
        return Err("only the storage node deletes".into());
    };
    assert_eq!(error.code(), "MM_STORAGE_NOT_SERVED");
    Ok(())
}

/// An engine that answers each command with the next scripted output.
struct Engine {
    outputs: Mutex<VecDeque<String>>,
    commands: Mutex<Vec<Vec<String>>>,
}

impl Engine {
    fn new(outputs: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            outputs: Mutex::new(outputs.iter().map(|output| (*output).to_owned()).collect()),
            commands: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let poisoned = || MmError::State {
            message: "the test engine's lock was poisoned".to_owned(),
        };
        self.commands
            .lock()
            .map_err(|_| poisoned())?
            .push(specification.redacted_record().arguments);
        let stdout = self
            .outputs
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "the test engine has no more output".to_owned(),
            })?;
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

fn inspected() -> String {
    serde_json::json!([
        {"Id": "aaaa", "Name": "/shop-web-1",
         "State": {"Status": "created", "Error": MOUNT_FAILED},
         "Config": {"Labels": {"io.mm.shared-storage": "10.90.0.1"}},
         "HostConfig": {"RestartPolicy": {"Name": "unless-stopped"}}},
        {"Id": "bbbb", "Name": "/shop-worker-1",
         "State": {"Status": "exited", "Error": ""},
         "Config": {"Labels": {"io.mm.shared-storage": "10.90.0.1"}},
         "HostConfig": {"RestartPolicy": {"Name": "unless-stopped"}}},
    ])
    .to_string()
}

#[tokio::test]
async fn a_container_that_waited_for_storage_is_started_when_storage_answers() -> TestResult {
    // The engine lists them with two commands, and with one when none waits.
    let engine = Engine::new(&["aaaa\nbbbb\n", &inspected(), ""]);
    let runtime = DockerComposeRuntime::new(engine.clone());
    let listed = runtime.waiting_containers(STORAGE_LABEL).await?;
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].waits_for(), Some(ip("10.90.0.1")?));
    assert_eq!(listed[1].waits_for(), None, "it was stopped, not failed");
    assert!(runtime.waiting_containers(STORAGE_LABEL).await?.is_empty());
    let commands = engine.commands.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(
        commands[0],
        [
            "ps",
            "--all",
            "--quiet",
            "--no-trunc",
            "--filter",
            "label=io.mm.shared-storage",
            "--filter",
            "status=created",
            "--filter",
            "status=exited",
        ]
    );
    assert_eq!(
        commands[1],
        ["container", "inspect", "--format", "json", "aaaa", "bbbb"]
    );
    assert_eq!(commands.len(), 3);

    let keeper = |outputs: &[&str],
                  reachable: bool,
                  meshed: bool|
     -> Result<_, Box<dyn std::error::Error>> {
        let engine = Engine::new(outputs);
        let runtime: Arc<dyn RuntimeDriver> = Arc::new(DockerComposeRuntime::new(engine.clone()));
        let host = Arc::new(Host {
            reachable,
            ..Host::default()
        });
        let mesh = Arc::new(Mesh(if meshed { Some(mesh(2)?) } else { None }));
        Ok((StorageKeeper::new(runtime, host, mesh), engine))
    };

    // Storage answers: the one that waited is started, the other is not.
    let (waiting, engine) = keeper(&["aaaa\nbbbb\n", &inspected(), "aaaa\n"], true, true)?;
    assert_eq!(waiting.look().await?, ["shop-web-1"]);
    assert_eq!(
        engine.commands.lock().map_err(|_| "poisoned")?.last(),
        Some(&vec!["start".to_owned(), "aaaa".to_owned()])
    );

    // Storage is still away: nothing is started, and it looks again later.
    let (still_away, engine) = keeper(&["aaaa\nbbbb\n", &inspected()], false, true)?;
    assert!(still_away.look().await?.is_empty());
    assert_eq!(engine.commands.lock().map_err(|_| "poisoned")?.len(), 2);

    // A host that is no cluster node asks its engine nothing.
    let (alone, engine) = keeper(&[], true, false)?;
    assert!(alone.look().await?.is_empty());
    assert!(engine.commands.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

const VALIDATED_CONFIG: &str = "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1df0
  name: shop
default-environment: production
target: local
stacks:
  app:
    files: [compose.yaml]
    shared-volumes: [uploads]
modules:
  shop:
    path: .
    services:
      web: {stack: app, placement: all}
      db: {stack: app, role: database}
environments:
  production: {target: production, bindings: {}}
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
";

const VALIDATED_COMPOSE: &str = "services:
  web:
    image: example/web
    volumes: [\"uploads:/data\"]
  db:
    image: example/db
    volumes: [\"dbdata:/var/lib/db\"]
volumes:
  uploads: {}
  dbdata: {}
";

/// What `mm config validate` says about shared volumes for a project with
/// this configuration and Compose file.
fn shared_volume_findings(
    config: &str,
    compose: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path().join("project");
    std::fs::create_dir_all(root.join(".config"))?;
    std::fs::write(root.join(".config/mm.yaml"), config)?;
    std::fs::write(root.join("compose.yaml"), compose)?;
    let output = temp_dirs::mm()
        .arg("--root")
        .arg(&root)
        .arg("--mm-home")
        .arg(temporary.path().join("home"))
        .args(["--output", "json", "config", "validate"])
        .output()?;
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "mm config validate printed no report ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    Ok(report["validation"]["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == "MM_SHARED_VOLUME")
        .filter_map(|diagnostic| diagnostic["message"].as_str().map(str::to_owned))
        .collect())
}

#[test]
fn validation_refuses_what_cannot_be_shared_and_says_what_to_write() -> TestResult {
    assert_eq!(
        shared_volume_findings(VALIDATED_CONFIG, VALIDATED_COMPOSE)?,
        Vec::<String>::new()
    );

    // Listed, and no Compose file declares it.
    let unknown = VALIDATED_CONFIG.replace("[uploads]", "[uploads, media]");
    let findings = shared_volume_findings(&unknown, VALIDATED_COMPOSE)?;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("lists 'media'") && findings[0].contains("volumes: {media: {}}"));

    // The Compose file says where the volume is stored.
    for (key, line) in [
        ("external", "uploads: {external: true}"),
        ("name", "uploads: {name: shop-uploads}"),
    ] {
        let placed = VALIDATED_COMPOSE.replace("uploads: {}", line);
        let findings = shared_volume_findings(VALIDATED_CONFIG, &placed)?;
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains(&format!("sets `{key}`")),
            "{findings:?}"
        );
    }

    // A database on a shared volume, with why and what to do instead.
    let database = VALIDATED_CONFIG.replace("[uploads]", "[uploads, dbdata]");
    let findings = shared_volume_findings(&database, VALIDATED_COMPOSE)?;
    assert_eq!(findings.len(), 1, "{findings:?}");
    for part in [
        "service 'db' has role database and mounts shared volume 'dbdata'",
        "slower than local disk",
        "locking guarantees",
        "placement: <node>",
        "allow-shared-volumes: true",
    ] {
        assert!(findings[0].contains(part), "{part}: {findings:?}");
    }
    // The service insists.
    let insisted = database.replace(
        "db: {stack: app, role: database}",
        "db: {stack: app, role: database, allow-shared-volumes: true}",
    );
    assert!(shared_volume_findings(&insisted, VALIDATED_COMPOSE)?.is_empty());
    // A queue keeps its data the same way.
    let queue = database.replace("role: database", "role: queue");
    assert!(shared_volume_findings(&queue, VALIDATED_COMPOSE)?[0].contains("role queue"));

    // An existing private interface carries no shared volumes.
    let own_interface = VALIDATED_CONFIG.replace(
        "    nodes:\n      - {name: app-1, host: 203.0.113.11}\n      - {name: app-2, host: 203.0.113.12}\n",
        "    cluster: {mesh: {interface: eth1}}\n    nodes:\n      - {name: app-1, host: 203.0.113.11, private-address: 10.0.0.1}\n      - {name: app-2, host: 203.0.113.12, private-address: 10.0.0.2}\n",
    );
    let findings = shared_volume_findings(&own_interface, VALIDATED_COMPOSE)?;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].contains("cluster.mesh.interface"),
        "{findings:?}"
    );

    // On one host a shared volume is an ordinary volume: nothing to check.
    let one_host = database
        .replace(
            "  production: {target: production, bindings: {}}",
            "  production: {bindings: {}}",
        )
        .replace("placement: all", "role: http");
    assert!(shared_volume_findings(&one_host, VALIDATED_COMPOSE)?.is_empty());
    Ok(())
}
