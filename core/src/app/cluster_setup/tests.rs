use super::*;

#[test]
fn keys_compare_by_type_and_material_only() {
    assert!(same_key(
        "ssh-ed25519 AAAA host-1",
        "ssh-ed25519 AAAA other comment"
    ));
    assert!(!same_key("ssh-ed25519 AAAA", "ssh-ed25519 BBBB"));
    assert!(!same_key("ssh-ed25519", "ssh-ed25519"));
}

#[test]
fn references_print_as_provider_urls() -> Result<(), String> {
    let reference = SecretReference::new(
        crate::domain::secret_contract::StorageProviderId::new("age")?,
        "local/app-key".to_owned(),
    )?;
    assert_eq!(reference_text(&reference), "age://local/app-key");
    Ok(())
}

mod steps {
    use std::net::Ipv4Addr;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use zeroize::Zeroizing;

    use super::super::{ClusterSetupDependencies, ClusterSetupService};
    use crate::app::ports::{
        ClusterNodeCommand, ClusterNodeConnection, ClusterNodeShell, ClusterScript,
        SecretMaterialResolver, SecretMetadataInspector, TargetBootstrapExecutor,
    };
    use crate::domain::cluster_setup::{
        ClusterDeployKeyPlan, ClusterNodePlan, ClusterSetupPlan, ClusterSetupStep, HostKeyOrigin,
    };
    use crate::domain::error::MmError;
    use crate::domain::mesh::{
        MeshFacts, MeshMember, MeshNodeOrigin, MeshPlan, MeshState, PeerAction, plan_node,
    };
    use crate::domain::repo_target::{Ipv4Cidr, MeshConfig, RepoClusterConfig, RepoTargetSetup};
    use crate::domain::runtime::RuntimeKind;
    use crate::domain::secret::{
        SecretBundle, SecretMetadata, SecretReference, SecretResolution, SecretScope,
    };
    use crate::domain::target::{
        BootstrapAccessReport, BootstrapCapability, BootstrapExecution, BootstrapInputFile,
        TargetBootstrapPlan,
    };
    use crate::domain::workspace::ProjectId;

    const HOST_KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";
    const MMD: &str = "/usr/libexec/mmd";
    const SETUP: &str = "
origin: git@github.com:example/app.git
origin-host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
break-glass:
  - {name: ops-recovery, key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w ops}
";

    fn unused(what: &str) -> MmError {
        MmError::State {
            message: format!("{what} is not part of these steps"),
        }
    }

    /// One command a step ran on a node, with what it wrote to standard input.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        node: String,
        command: ClusterNodeCommand,
        stdin: Option<String>,
    }

    /// Answers as a set of nodes whose `mmd` holds `facts`; a node without
    /// an entry has no mesh yet.
    #[derive(Default)]
    struct RecordingShell {
        calls: Mutex<Vec<Call>>,
        facts: Mutex<std::collections::BTreeMap<String, MeshFacts>>,
        /// Standard error of a reach check that fails on this node.
        unreachable: Mutex<Option<(String, String)>>,
        /// What a node says about the shared storage it serves, by node.
        serving: Mutex<std::collections::BTreeMap<String, &'static str>>,
    }

    impl RecordingShell {
        fn calls(&self) -> Result<Vec<Call>, MmError> {
            self.calls
                .lock()
                .map(|calls| calls.clone())
                .map_err(|_| unused("a poisoned lock"))
        }

        fn forget_calls(&self) -> Result<(), MmError> {
            self.calls
                .lock()
                .map(|mut calls| calls.clear())
                .map_err(|_| unused("a poisoned lock"))
        }

        fn hold(&self, node: &str, facts: MeshFacts) -> Result<(), MmError> {
            self.facts
                .lock()
                .map(|mut held| drop(held.insert(node.to_owned(), facts)))
                .map_err(|_| unused("a poisoned lock"))
        }

        fn serves(&self, node: &str, says: &'static str) -> Result<(), MmError> {
            self.serving
                .lock()
                .map(|mut serving| {
                    serving.insert(node.to_owned(), says);
                })
                .map_err(|_| unused("a poisoned lock"))
        }

        fn facts_of(&self, node: &str) -> Result<MeshFacts, MmError> {
            self.facts
                .lock()
                .map(|held| held.get(node).cloned().unwrap_or_default())
                .map_err(|_| unused("a poisoned lock"))
        }
    }

    /// The key a node makes when it has none: 32 bytes of its name's length
    /// and last character, which is distinct for every node used here.
    fn key_of(node: &str) -> String {
        use base64::Engine;
        let length = u8::try_from(node.len()).unwrap_or(0);
        let byte = node.bytes().last().unwrap_or(0) ^ length.wrapping_mul(16);
        base64::engine::general_purpose::STANDARD.encode([byte; 32])
    }

    #[async_trait]
    impl ClusterNodeShell for RecordingShell {
        async fn scan_host_key(&self, _host: &str, _port: u16) -> Result<String, MmError> {
            Ok(HOST_KEY.to_owned())
        }

        async fn run(
            &self,
            node: &ClusterNodeConnection,
            command: &ClusterNodeCommand,
            stdin: Option<Zeroizing<String>>,
        ) -> Result<Zeroizing<String>, MmError> {
            self.calls
                .lock()
                .map_err(|_| unused("a poisoned lock"))?
                .push(Call {
                    node: node.name.clone(),
                    command: command.clone(),
                    stdin: stdin.map(|stdin| stdin.to_string()),
                });
            let words = match command {
                ClusterNodeCommand::Program(argv) => argv.join(" "),
                ClusterNodeCommand::Script { .. } => String::new(),
            };
            let json = |value: serde_json::Value| value.to_string();
            Ok(Zeroizing::new(match command {
                ClusterNodeCommand::Script {
                    script: ClusterScript::MeshFacts,
                    ..
                } => serde_json::to_string(&self.facts_of(&node.name)?)
                    .map_err(|_| unused("unserializable facts"))?,
                ClusterNodeCommand::Script {
                    script: ClusterScript::Mesh,
                    ..
                } => {
                    json(serde_json::json!({"changed": true, "actions": ["stored the mesh state"]}))
                }
                ClusterNodeCommand::Script {
                    script: ClusterScript::Reach,
                    ..
                } => {
                    let failing = self
                        .unreachable
                        .lock()
                        .map_err(|_| unused("a poisoned lock"))?
                        .clone();
                    match failing {
                        Some((failing, stderr)) if failing == node.name => {
                            return Err(MmError::ProcessFailed {
                                program: format!("ssh {}", node.name),
                                code: Some(1),
                                stderr,
                            });
                        }
                        _ => String::new(),
                    }
                }
                ClusterNodeCommand::Script {
                    script: ClusterScript::StorageFacts,
                    ..
                } => self
                    .serving
                    .lock()
                    .map_err(|_| unused("a poisoned lock"))?
                    .get(&node.name)
                    .copied()
                    .unwrap_or("serves=no files=no")
                    .to_owned(),
                _ if words.ends_with("mesh key") => {
                    let held = self.facts_of(&node.name)?;
                    json(serde_json::json!({
                        "public-key": held.public_key.unwrap_or_else(|| key_of(&node.name)),
                        "host-ranges": held.host_ranges,
                    }))
                }
                _ if words.ends_with("mesh rotate --stage")
                    || words.ends_with("mesh rotate --activate") =>
                {
                    json(
                        serde_json::json!({"public-key": key_of(&format!("{}-rotated", node.name))}),
                    )
                }
                _ if words.contains("--print-identity") => format!("{}\n", "d".repeat(64)),
                _ => String::new(),
            }))
        }

        async fn public_key_of(&self, _private_key: &[u8]) -> Result<String, MmError> {
            Err(unused("deriving a public key"))
        }

        fn seal_secret_bundle(
            &self,
            _recipient: &str,
            _bundle: &SecretBundle,
        ) -> Result<Zeroizing<String>, MmError> {
            Err(unused("sealing secrets"))
        }

        async fn generate_deploy_key(
            &self,
            _comment: &str,
        ) -> Result<(Zeroizing<Vec<u8>>, String), MmError> {
            Err(unused("generating a deploy key"))
        }
    }

    struct NoSecrets;

    #[async_trait]
    impl SecretMetadataInspector for NoSecrets {
        async fn inspect(
            &self,
            _scope: &SecretScope,
            _reference: &SecretReference,
        ) -> Result<SecretMetadata, MmError> {
            Err(unused("inspecting secrets"))
        }
    }

    #[async_trait]
    impl SecretMaterialResolver for NoSecrets {
        async fn resolve(
            &self,
            _scope: &SecretScope,
            _reference: &SecretReference,
        ) -> Result<SecretResolution, MmError> {
            Err(unused("resolving secrets"))
        }
    }

    /// Records each node's bootstrap plan and reports the node's reviewed key.
    #[derive(Default)]
    struct RecordingBootstrap {
        plans: Mutex<Vec<TargetBootstrapPlan>>,
    }

    #[async_trait]
    impl TargetBootstrapExecutor for RecordingBootstrap {
        async fn execute(
            &self,
            _project_root: &Path,
            plan: &TargetBootstrapPlan,
        ) -> Result<BootstrapExecution, MmError> {
            self.plans
                .lock()
                .map_err(|_| unused("a poisoned lock"))?
                .push(plan.clone());
            Ok(BootstrapExecution {
                capabilities: plan.required_capabilities.clone(),
                access: Some(BootstrapAccessReport {
                    version: "1.2.3".to_owned(),
                    host_key: HOST_KEY.to_owned(),
                    daemon_fingerprint: "d".repeat(64),
                    deploy_key: "ssh-ed25519 BBBB".to_owned(),
                }),
            })
        }
    }

    struct Fixture {
        shell: Arc<RecordingShell>,
        bootstrap: Arc<RecordingBootstrap>,
        service: ClusterSetupService,
    }

    fn fixture() -> Fixture {
        let shell = Arc::new(RecordingShell::default());
        let bootstrap = Arc::new(RecordingBootstrap::default());
        let service = ClusterSetupService::new(ClusterSetupDependencies {
            documents: crate::test_project_documents(),
            inspector: Arc::new(NoSecrets),
            resolver: Arc::new(NoSecrets),
            shell: shell.clone(),
            bootstrap: bootstrap.clone(),
            identity_file: PathBuf::from("/var/lib/operator/id_ed25519"),
        });
        Fixture {
            shell,
            bootstrap,
            service,
        }
    }

    /// Three nodes on `cluster`, with the addresses mm would assign or the
    /// ones given.
    fn plan(
        cluster: &str,
        addresses: [Ipv4Addr; 3],
    ) -> Result<ClusterSetupPlan, Box<dyn std::error::Error>> {
        let cluster: RepoClusterConfig = serde_yaml_ng::from_str(cluster)?;
        let setup: RepoTargetSetup = serde_yaml_ng::from_str(SETUP)?;
        let nodes = ["app-1", "app-2", "db-1"]
            .into_iter()
            .zip(addresses)
            .zip(11_u8..)
            .map(|((name, private_address), host)| ClusterNodePlan {
                name: name.to_owned(),
                host: format!("203.0.113.{host}"),
                port: 22,
                private_address,
                host_key: HOST_KEY.to_owned(),
                host_key_fingerprint: "SHA256:0ilAVEYiaM4+PCd17vMiY2bh4bv814hVSbQKrN45DMU"
                    .to_owned(),
                host_key_origin: HostKeyOrigin::FirstUse,
            })
            .collect::<Vec<_>>();
        let storage = cluster
            .storage
            .clone()
            .unwrap_or_else(|| "app-1".to_owned());
        let mut plan = ClusterSetupPlan {
            project_id: ProjectId(uuid::Uuid::from_u128(1)),
            project_name: "app".to_owned(),
            config_hash: "c".repeat(64),
            environment: "production".to_owned(),
            target: "production".to_owned(),
            skipped: Vec::new(),
            mesh: mesh_plan(&cluster, &nodes, &[]),
            nodes,
            cluster,
            storage,
            runtime: RuntimeKind::DockerCompose,
            setup,
            input_files: [
                (".config/ansible/ansible.cfg", "b"),
                (".config/ansible/bootstrap.yml", "c"),
            ]
            .map(|(path, digest)| BootstrapInputFile {
                path: PathBuf::from(path),
                digest: digest.repeat(64),
            })
            .to_vec(),
            binaries: PathBuf::from("/opt/mm"),
            mm_digest: "a".repeat(64),
            mmd_digest: "b".repeat(64),
            architecture: "x86_64".to_owned(),
            secrets: Vec::new(),
            secret_store: None,
            unset_secrets: Vec::new(),
            notices: Vec::new(),
            deploy_key: ClusterDeployKeyPlan {
                secret: "deploy".to_owned(),
                revision: "r2".to_owned(),
                public_key: "ssh-ed25519 BBBB".to_owned(),
            },
            access_commit: "e".repeat(40),
            steps: ClusterSetupStep::ALL.to_vec(),
            plan_hash: String::new(),
        };
        plan.plan_hash = plan.canonical_hash()?;
        Ok(plan)
    }

    fn members_of(nodes: &[ClusterNodePlan]) -> Vec<MeshMember> {
        nodes
            .iter()
            .map(|node| MeshMember {
                name: node.name.clone(),
                address: node.private_address,
                endpoint: node.host.clone(),
                public_key: None,
                routed: Vec::new(),
            })
            .collect()
    }

    /// The mesh section planning writes for nodes that could not be read:
    /// everything is new. `None` for an existing private interface.
    fn mesh_plan(
        cluster: &RepoClusterConfig,
        nodes: &[ClusterNodePlan],
        left_out: &[&str],
    ) -> Option<MeshPlan> {
        let MeshConfig::Wireguard(mesh) = &cluster.mesh else {
            return None;
        };
        let members = members_of(nodes);
        Some(MeshPlan {
            interface: "mm0".to_owned(),
            subnet: mesh.subnet,
            port: mesh.port,
            mtu: mesh.mtu,
            nodes: members
                .iter()
                .filter(|member| !left_out.contains(&member.name.as_str()))
                .map(|member| plan_node(mesh, &members, member, left_out, None))
                .collect(),
        })
    }

    /// What a node that runs `mm0` with `peers` reports.
    fn facts_with(node: &ClusterNodePlan, plan: &ClusterSetupPlan, peers: &[&str]) -> MeshFacts {
        let MeshConfig::Wireguard(mesh) = &plan.cluster.mesh else {
            return MeshFacts::default();
        };
        let mut members = members_of(&plan.nodes);
        members.extend(plan.skipped.iter().map(|skipped| MeshMember {
            name: skipped.name.clone(),
            address: skipped.private_address,
            endpoint: format!("203.0.113.{}", 11 + skipped.position),
            public_key: None,
            routed: Vec::new(),
        }));
        for member in &mut members {
            member.public_key = Some(key_of(&member.name));
        }
        MeshFacts {
            public_key: Some(key_of(&node.name)),
            state: Some(MeshState::new(
                mesh,
                node.private_address,
                crate::domain::mesh::peer_set(mesh.port, &members, &node.name)
                    .iter()
                    .filter(|peer| peers.contains(&peer.name.as_str()))
                    .filter_map(crate::domain::mesh::PlannedPeer::resolved)
                    .collect(),
            )),
            ..MeshFacts::default()
        }
    }

    /// The state each node was handed on standard input, by node.
    fn applied_states(calls: &[Call]) -> Result<Vec<(String, MeshState)>, serde_json::Error> {
        scripts(calls, ClusterScript::Mesh)
            .into_iter()
            .map(|(node, _, stdin)| {
                Ok((
                    node.to_owned(),
                    serde_json::from_str(stdin.unwrap_or_default())?,
                ))
            })
            .collect()
    }

    fn mesh_addresses() -> [Ipv4Addr; 3] {
        [1, 2, 3].map(|last| Ipv4Addr::new(10, 90, 0, last))
    }

    fn scripts(calls: &[Call], wanted: ClusterScript) -> Vec<(&str, Vec<&str>, Option<&str>)> {
        calls
            .iter()
            .filter_map(|call| match &call.command {
                ClusterNodeCommand::Script { script, arguments } if *script == wanted => Some((
                    call.node.as_str(),
                    arguments.iter().map(String::as_str).collect(),
                    call.stdin.as_deref(),
                )),
                _ => None,
            })
            .collect()
    }

    fn programs(calls: &[Call]) -> Vec<(&str, Vec<&str>)> {
        calls
            .iter()
            .filter_map(|call| match &call.command {
                ClusterNodeCommand::Program(argv) => Some((
                    call.node.as_str(),
                    argv.iter().map(String::as_str).collect(),
                )),
                ClusterNodeCommand::Script { .. } => None,
            })
            .collect()
    }

    /// Nothing a step runs or sends names Docker Swarm.
    fn assert_no_swarm(calls: &[Call]) {
        for call in calls {
            let words = match &call.command {
                ClusterNodeCommand::Program(argv) => argv.join(" "),
                ClusterNodeCommand::Script { arguments, .. } => arguments.join(" "),
            };
            let text = format!("{words} {}", call.stdin.as_deref().unwrap_or_default());
            assert!(!text.to_ascii_lowercase().contains("swarm"), "{call:?}");
            assert!(!text.contains("--require-shared"), "{call:?}");
        }
    }

    #[tokio::test]
    async fn prepare_names_the_nodes_and_leaves_the_mesh_to_mmd()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = plan("{}", mesh_addresses())?;
        fixture
            .service
            .prepare_nodes(&plan, &crate::app::ports::NoProgress)
            .await?;
        let calls = fixture.shell.calls()?;
        assert_eq!(
            scripts(&calls, ClusterScript::Prepare),
            [
                (
                    "app-1",
                    vec!["app-1", "mm0", "10.90.0.1", "yes", "docker-compose"],
                    None
                ),
                (
                    "app-2",
                    vec!["app-2", "mm0", "10.90.0.2", "yes", "docker-compose"],
                    None
                ),
                (
                    "db-1",
                    vec!["db-1", "mm0", "10.90.0.3", "yes", "docker-compose"],
                    None
                ),
            ]
        );
        assert_eq!(calls.len(), 3, "the mesh waits for the mmd that owns it");
        assert_no_swarm(&calls);
        Ok(())
    }

    #[tokio::test]
    async fn connecting_hands_every_node_its_peers_and_checks_every_link()
    -> Result<(), Box<dyn std::error::Error>> {
        for (cluster, mtu, port) in [
            ("{}", 1420, 51820),
            ("mesh: {mtu: 1380, port: 51900}", 1380, 51900),
        ] {
            let fixture = fixture();
            let plan = plan(cluster, mesh_addresses())?;
            fixture.service.connect_nodes(&plan).await?;
            let calls = fixture.shell.calls()?;

            // First every node makes or reports its key, on the node.
            let key = vec![MMD, "mesh", "key"];
            assert_eq!(
                programs(&calls),
                [
                    ("app-1", key.clone()),
                    ("app-2", key.clone()),
                    ("db-1", key)
                ]
            );
            // Then each one gets its interface and every other node as a peer.
            let states = applied_states(&calls)?;
            assert_eq!(
                states
                    .iter()
                    .map(|(node, _)| node.as_str())
                    .collect::<Vec<_>>(),
                ["app-1", "app-2", "db-1"]
            );
            for (node, state) in &states {
                state.validate()?;
                assert_eq!(state.interface, "mm0");
                assert_eq!(
                    (state.prefix, state.port, state.mtu),
                    (24, port, mtu),
                    "{node}"
                );
            }
            let (_, first) = &states[0];
            assert_eq!(first.address, Ipv4Addr::new(10, 90, 0, 1));
            assert_eq!(
                first
                    .peers
                    .iter()
                    .map(|peer| (
                        peer.name.as_str(),
                        peer.public_key.clone(),
                        peer.endpoint.clone(),
                        peer.allowed_ips[0].to_string()
                    ))
                    .collect::<Vec<_>>(),
                [
                    (
                        "app-2",
                        key_of("app-2"),
                        format!("203.0.113.12:{port}"),
                        "10.90.0.2/32".to_owned()
                    ),
                    (
                        "db-1",
                        key_of("db-1"),
                        format!("203.0.113.13:{port}"),
                        "10.90.0.3/32".to_owned()
                    ),
                ]
            );
            assert!(
                scripts(&calls, ClusterScript::Mesh)
                    .iter()
                    .all(|(_, arguments, _)| arguments.is_empty()),
                "nothing about the mesh travels in an argument list"
            );
            // MTU PEER...: one packet at the mesh MTU to every other node.
            let mtu = mtu.to_string();
            assert_eq!(
                scripts(&calls, ClusterScript::Reach),
                [
                    ("app-1", vec![mtu.as_str(), "10.90.0.2", "10.90.0.3"], None),
                    ("app-2", vec![mtu.as_str(), "10.90.0.1", "10.90.0.3"], None),
                    ("db-1", vec![mtu.as_str(), "10.90.0.1", "10.90.0.2"], None),
                ]
            );
            assert_eq!(calls.len(), 9, "key, mesh, and reach once per node");
            assert_no_swarm(&calls);
        }
        Ok(())
    }

    #[tokio::test]
    async fn no_private_key_is_ever_asked_for_or_sent() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = plan("{}", mesh_addresses())?;
        fixture
            .service
            .prepare_nodes(&plan, &crate::app::ports::NoProgress)
            .await?;
        fixture.service.connect_nodes(&plan).await?;
        let json = serde_json::to_string(&plan)?.to_ascii_lowercase();
        assert!(
            !json.contains("private-key") && !json.contains("privatekey"),
            "{json}"
        );
        for call in fixture.shell.calls()? {
            let text = format!("{:?} {}", call.command, call.stdin.unwrap_or_default())
                .to_ascii_lowercase();
            for forbidden in [
                "private-key",
                "privatekey",
                "private.key",
                "genkey",
                "wg0.key",
            ] {
                assert!(!text.contains(forbidden), "{forbidden} in {text}");
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn an_existing_interface_gets_no_wireguard_mesh() -> Result<(), Box<dyn std::error::Error>>
    {
        let fixture = fixture();
        let addresses = [2, 3, 4].map(|last| Ipv4Addr::new(192, 168, 7, last));
        let plan = plan("mesh: {interface: eth1}", addresses)?;
        assert!(plan.mesh.is_none());
        fixture
            .service
            .prepare_nodes(&plan, &crate::app::ports::NoProgress)
            .await?;
        fixture.service.connect_nodes(&plan).await?;
        let calls = fixture.shell.calls()?;
        assert_eq!(
            scripts(&calls, ClusterScript::Prepare),
            [
                (
                    "app-1",
                    vec!["app-1", "eth1", "192.168.7.2", "no", "docker-compose"],
                    None
                ),
                (
                    "app-2",
                    vec!["app-2", "eth1", "192.168.7.3", "no", "docker-compose"],
                    None
                ),
                (
                    "db-1",
                    vec!["db-1", "eth1", "192.168.7.4", "no", "docker-compose"],
                    None
                ),
            ]
        );
        assert!(scripts(&calls, ClusterScript::Mesh).is_empty());
        assert!(programs(&calls).is_empty(), "no node is asked for a key");
        // The links are still checked, without the MTU-sized packet.
        assert_eq!(
            scripts(&calls, ClusterScript::Reach)[0],
            ("app-1", vec!["0", "192.168.7.3", "192.168.7.4"], None)
        );
        assert_no_swarm(&calls);
        Ok(())
    }

    #[tokio::test]
    async fn the_plan_reads_recorded_nodes_and_shows_what_changes_on_each()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let target: crate::domain::repo_target::RepoTargetConfig = serde_yaml_ng::from_str(
            "nodes:\n  - {name: app-1, host: 203.0.113.11}\n  - {name: app-2, host: 203.0.113.12}\n  - {name: db-1, host: 203.0.113.13, endpoint: 10.0.0.3}\n",
        )?;
        let mut reviewed = plan("{}", mesh_addresses())?;
        // app-1 and app-2 run the mesh with each other; db-1 is new.
        for index in [0, 1] {
            reviewed.nodes[index].host_key_origin = HostKeyOrigin::Recorded;
            let other = if index == 0 { "app-2" } else { "app-1" };
            let facts = facts_with(&reviewed.nodes[index], &reviewed, &[other]);
            fixture.shell.hold(&reviewed.nodes[index].name, facts)?;
        }
        let mesh = fixture
            .service
            .plan_mesh(
                "production",
                &target,
                "root",
                (reviewed.nodes.as_slice(), &[][..]),
                &crate::app::ports::NoProgress,
            )
            .await?
            .ok_or("no mesh section")?;
        let calls = fixture.shell.calls()?;
        assert_eq!(
            scripts(&calls, ClusterScript::MeshFacts)
                .iter()
                .map(|(node, ..)| *node)
                .collect::<Vec<_>>(),
            ["app-1", "app-2"],
            "a node with a new host key is not logged in to before the review"
        );
        assert_eq!(calls.len(), 2, "planning only reads");

        assert_eq!(
            (mesh.interface.as_str(), mesh.port, mesh.mtu),
            ("mm0", 51820, 1420)
        );
        assert!(mesh.changes());
        let [first, second, third] = mesh.nodes.as_slice() else {
            return Err("three node sections".into());
        };
        assert_eq!(first.origin, MeshNodeOrigin::Current);
        assert_eq!(first.public_key, Some(key_of("app-1")));
        assert_eq!(
            first
                .peers
                .iter()
                .map(|peer| (peer.name.as_str(), peer.action, peer.public_key.is_some()))
                .collect::<Vec<_>>(),
            [
                ("app-2", PeerAction::Keep, true),
                ("db-1", PeerAction::Add, false)
            ],
            "the link between the two existing nodes is not touched"
        );
        assert_eq!(
            first.peers[1].endpoint, "10.0.0.3:51820",
            "db-1 declares an endpoint"
        );
        assert_eq!(second.peers[0].action, PeerAction::Keep);
        assert_eq!(third.origin, MeshNodeOrigin::New);
        assert_eq!(third.public_key, None, "made on the node during apply");
        assert_eq!(third.endpoint, "10.0.0.3:51820");
        assert!(
            third
                .peers
                .iter()
                .all(|peer| peer.action == PeerAction::Add)
        );

        // Nothing differs once db-1 is in: the next plan changes nothing.
        for index in [0, 1, 2] {
            reviewed.nodes[index].host_key_origin = HostKeyOrigin::Recorded;
            let others = ["app-1", "app-2", "db-1"];
            let mut facts = facts_with(&reviewed.nodes[index], &reviewed, &others);
            if let Some(state) = &mut facts.state {
                for peer in state.peers.iter_mut().filter(|peer| peer.name == "db-1") {
                    peer.endpoint = "10.0.0.3:51820".to_owned();
                }
            }
            fixture.shell.hold(&reviewed.nodes[index].name, facts)?;
        }
        let settled = fixture
            .service
            .plan_mesh(
                "production",
                &target,
                "root",
                (reviewed.nodes.as_slice(), &[][..]),
                &crate::app::ports::NoProgress,
            )
            .await?
            .ok_or("no mesh section")?;
        assert!(!settled.changes(), "{settled:?}");

        // A node that left the target is dropped by the others, with a
        // notice that says how to clean it.
        let smaller: crate::domain::repo_target::RepoTargetConfig = serde_yaml_ng::from_str(
            "nodes:\n  - {name: app-1, host: 203.0.113.11}\n  - {name: app-2, host: 203.0.113.12}\n",
        )?;
        let shrunk = fixture
            .service
            .plan_mesh(
                "production",
                &smaller,
                "root",
                (&reviewed.nodes[..2], &[][..]),
                &crate::app::ports::NoProgress,
            )
            .await?
            .ok_or("no mesh section")?;
        assert_eq!(shrunk.nodes.len(), 2);
        for node in &shrunk.nodes {
            assert_eq!(
                node.peers
                    .iter()
                    .map(|peer| peer.action)
                    .collect::<Vec<_>>(),
                [PeerAction::Keep, PeerAction::Remove],
                "{}",
                node.node
            );
        }
        let notices = super::super::mesh::departed_notices(&shrunk);
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(
            notices[0].contains("db-1 is not a node of the target any more"),
            "{}",
            notices[0]
        );
        assert!(
            notices[0].contains("sudo /usr/libexec/mmd mesh leave"),
            "{}",
            notices[0]
        );
        Ok(())
    }

    fn provider_network() -> Vec<crate::domain::cluster_network::HostRange> {
        use crate::domain::cluster_network::{HostRange, HostRangeKind};
        vec![
            HostRange {
                range: Ipv4Cidr {
                    address: Ipv4Addr::new(10, 70, 0, 0),
                    prefix: 20,
                },
                kind: HostRangeKind::Route,
                interface: Some("eth1".to_owned()),
            },
            HostRange {
                range: Ipv4Cidr {
                    address: Ipv4Addr::new(10, 70, 0, 5),
                    prefix: 32,
                },
                kind: HostRangeKind::Address,
                interface: Some("eth1".to_owned()),
            },
        ]
    }

    #[tokio::test]
    async fn the_plan_refuses_a_space_a_recorded_node_already_uses()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let target: crate::domain::repo_target::RepoTargetConfig = serde_yaml_ng::from_str(
            "nodes:\n  - {name: app-1, host: 203.0.113.11}\n  - {name: app-2, host: 203.0.113.12}\n  - {name: db-1, host: 203.0.113.13}\n",
        )?;
        let mut reviewed = plan("{}", mesh_addresses())?;
        for index in [0, 1] {
            reviewed.nodes[index].host_key_origin = HostKeyOrigin::Recorded;
        }
        let mut facts = facts_with(&reviewed.nodes[1], &reviewed, &["app-1"]);
        facts.host_ranges = provider_network();
        fixture.shell.hold("app-2", facts)?;
        let error = fixture
            .service
            .plan_mesh(
                "production",
                &target,
                "root",
                (reviewed.nodes.as_slice(), &[][..]),
                &crate::app::ports::NoProgress,
            )
            .await
            .err()
            .ok_or("a space the node uses was planned")?;
        assert_eq!(
            error.to_string(),
            "target error: node 'app-2' already has the route 10.70.0.0/20 through eth1, which overlaps cluster.networks 10.64.0.0/12 (the addresses of networks that span the nodes). Set `cluster.networks` under targets.production.cluster to a private range no node uses, such as 10.32.0.0/12, then plan again"
        );
        // With the space moved the same node plans.
        let mut moved = target.clone();
        moved.cluster.networks = Ipv4Cidr {
            address: Ipv4Addr::new(10, 32, 0, 0),
            prefix: 12,
        };
        fixture
            .service
            .plan_mesh(
                "production",
                &moved,
                "root",
                (reviewed.nodes.as_slice(), &[][..]),
                &crate::app::ports::NoProgress,
            )
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn a_new_node_that_uses_the_mesh_subnet_stops_the_apply_before_the_mesh()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = plan("{}", mesh_addresses())?;
        // db-1 had a new host key at plan time, so it is read only now.
        fixture.shell.hold(
            "db-1",
            MeshFacts {
                host_ranges: vec![crate::domain::cluster_network::HostRange {
                    range: Ipv4Cidr {
                        address: Ipv4Addr::new(10, 90, 0, 0),
                        prefix: 16,
                    },
                    kind: crate::domain::cluster_network::HostRangeKind::Route,
                    interface: Some("vpn0".to_owned()),
                }],
                ..MeshFacts::default()
            },
        )?;
        let error = fixture
            .service
            .connect_nodes(&plan)
            .await
            .err()
            .ok_or("the mesh came up on a subnet a node uses")?;
        let message = error.to_string();
        assert!(
            message.contains("node 'db-1' already has the route 10.90.0.0/16 through vpn0, which overlaps cluster.mesh.subnet 10.90.0.0/24")
                && message.contains("Set `cluster.mesh.subnet` under targets.production.cluster")
                && message.contains("such as 10.92.0.0/24"),
            "{message}"
        );
        let calls = fixture.shell.calls()?;
        assert!(
            scripts(&calls, ClusterScript::Mesh).is_empty(),
            "no node's mesh changed"
        );
        assert_eq!(calls.len(), 3, "only the keys were read");
        Ok(())
    }

    #[tokio::test]
    async fn a_key_that_changed_after_the_review_stops_the_apply()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let mut plan = plan("{}", mesh_addresses())?;
        if let Some(mesh) = &mut plan.mesh {
            mesh.nodes[1].public_key = Some(key_of("somebody-else"));
        }
        let error = fixture
            .service
            .connect_nodes(&plan)
            .await
            .err()
            .ok_or("another key was accepted")?;
        assert!(
            error
                .to_string()
                .contains("app-2's WireGuard key changed after the plan was reviewed"),
            "{error}"
        );
        assert!(
            scripts(&fixture.shell.calls()?, ClusterScript::Mesh).is_empty(),
            "no node's mesh is changed"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_link_that_does_not_come_up_says_which_port_between_which_hosts()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = plan("{}", mesh_addresses())?;
        *fixture.shell.unreachable.lock().map_err(|_| "poisoned")? =
            Some(("app-2".to_owned(), "mm-unreachable: 10.90.0.3\n".to_owned()));
        let error = fixture
            .service
            .connect_nodes(&plan)
            .await
            .err()
            .ok_or("an unreachable peer passed")?
            .to_string();
        for expected in [
            "node app-2 does not reach db-1 (10.90.0.3) over the mesh",
            "UDP port 51820 must be open between 203.0.113.12 and 203.0.113.13 in both directions",
            "mm target mesh status",
        ] {
            assert!(error.contains(expected), "{expected}: {error}");
        }

        *fixture.shell.unreachable.lock().map_err(|_| "poisoned")? =
            Some(("app-1".to_owned(), "mm-mtu: 10.90.0.2\n".to_owned()));
        let error = fixture
            .service
            .connect_nodes(&plan)
            .await
            .err()
            .ok_or("a path that drops large packets passed")?
            .to_string();
        for expected in [
            "node app-1 reaches app-2 (10.90.0.2) over the mesh, but a packet of 1420 bytes does not arrive",
            "between 203.0.113.11 and 203.0.113.12 carries less than the 1480 bytes",
            "Set cluster.mesh.mtu under targets.production",
        ] {
            assert!(error.contains(expected), "{expected}: {error}");
        }

        // Anything else the check prints is passed on as it is.
        *fixture.shell.unreachable.lock().map_err(|_| "poisoned")? =
            Some(("app-1".to_owned(), "ping: not found\n".to_owned()));
        let error = fixture.service.connect_nodes(&plan).await.err();
        assert!(
            matches!(error, Some(MmError::ProcessFailed { .. })),
            "{error:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn storage_is_served_by_one_node_on_its_private_address_only()
    -> Result<(), Box<dyn std::error::Error>> {
        // STORAGE_ROOT LISTEN_ADDRESS CLIENT...: the first node by default.
        let fixture = fixture();
        fixture
            .service
            .serve_storage(&plan("{}", mesh_addresses())?)
            .await?;
        let calls = fixture.shell.calls()?;
        assert_eq!(
            scripts(&calls, ClusterScript::Storage),
            [(
                "app-1",
                vec![
                    "/var/lib/mm/storage",
                    "10.90.0.1",
                    "10.90.0.1",
                    "10.90.0.2",
                    "10.90.0.3"
                ],
                None
            )]
        );
        // The other nodes say whether they served before, every node loads
        // the rules that keep containers away from the storage port, and
        // each other node proves it can mount.
        assert_eq!(
            scripts(&calls, ClusterScript::StorageFacts),
            [
                ("app-2", vec!["/var/lib/mm/storage"], None),
                ("db-1", vec!["/var/lib/mm/storage"], None)
            ]
        );
        assert_eq!(
            programs(&calls),
            ["app-1", "app-2", "db-1"].map(|node| (node, vec![MMD, "storage", "up"]))
        );
        assert_eq!(
            scripts(&calls, ClusterScript::StorageReach),
            [
                ("app-2", vec!["10.90.0.1"], None),
                ("db-1", vec!["10.90.0.1"], None)
            ]
        );
        assert!(scripts(&calls, ClusterScript::StorageWithdraw).is_empty());
        assert_eq!(calls.len(), 8);
        let position = |script: ClusterScript| {
            calls.iter().position(|call| {
                matches!(&call.command, ClusterNodeCommand::Script { script: ran, .. } if *ran == script)
            })
        };
        assert!(
            position(ClusterScript::StorageFacts) < position(ClusterScript::Storage)
                && position(ClusterScript::Storage) < position(ClusterScript::StorageReach),
            "ask first, serve, then prove"
        );
        assert_no_swarm(&calls);

        let chosen = storage_calls("db-1").await?;
        assert_eq!(chosen.len(), 8);
        assert_eq!(
            scripts(&chosen, ClusterScript::Storage),
            [(
                "db-1",
                vec![
                    "/var/lib/mm/storage",
                    "10.90.0.3",
                    "10.90.0.1",
                    "10.90.0.2",
                    "10.90.0.3"
                ],
                None
            )]
        );

        let refused = self::fixture();
        let mut unknown = plan("{}", mesh_addresses())?;
        unknown.storage = "app-9".to_owned();
        assert!(refused.service.serve_storage(&unknown).await.is_err());
        assert!(refused.shell.calls()?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn a_storage_node_that_keeps_files_is_never_taken_out_of_service()
    -> Result<(), Box<dyn std::error::Error>> {
        // app-1 served until now and keeps files; the plan names db-1.
        let fixture = fixture();
        fixture.shell.serves("app-1", "serves=yes files=yes")?;
        let moved = plan("storage: db-1", mesh_addresses())?;
        let Err(error) = fixture.service.serve_storage(&moved).await else {
            return Err("a storage node with files keeps serving".into());
        };
        let text = error.to_string();
        for part in [
            "cluster.storage now names db-1",
            "app-1 holds shared volumes with files in them",
            "rsync -aHAX --numeric-ids",
            "Nothing was changed",
        ] {
            assert!(text.contains(part), "{part}: {text}");
        }
        let calls = fixture.shell.calls()?;
        assert!(
            calls.iter().all(|call| matches!(
                &call.command,
                ClusterNodeCommand::Script {
                    script: ClusterScript::StorageFacts,
                    ..
                }
            )),
            "the nodes were only asked: {calls:?}"
        );

        // One that keeps no file stops serving, and db-1 takes over.
        let fixture = self::fixture();
        fixture.shell.serves("app-1", "serves=yes files=no")?;
        fixture.service.serve_storage(&moved).await?;
        let calls = fixture.shell.calls()?;
        assert_eq!(
            scripts(&calls, ClusterScript::StorageWithdraw),
            [("app-1", Vec::<&str>::new(), None)]
        );
        assert_eq!(scripts(&calls, ClusterScript::Storage)[0].0, "db-1");
        let withdrawn = calls.iter().position(|call| {
            matches!(
                &call.command,
                ClusterNodeCommand::Script {
                    script: ClusterScript::StorageWithdraw,
                    ..
                }
            )
        });
        let served = calls.iter().position(|call| {
            matches!(
                &call.command,
                ClusterNodeCommand::Script {
                    script: ClusterScript::Storage,
                    ..
                }
            )
        });
        assert!(
            withdrawn < served,
            "the old node stops before the new one serves"
        );

        // The storage node itself is never asked to hand over to itself.
        let fixture = self::fixture();
        fixture.shell.serves("app-1", "serves=yes files=yes")?;
        fixture
            .service
            .serve_storage(&plan("{}", mesh_addresses())?)
            .await?;
        Ok(())
    }

    /// What serving storage runs when `cluster.storage` names `node`.
    async fn storage_calls(node: &str) -> Result<Vec<Call>, Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = plan(&format!("storage: {node}"), mesh_addresses())?;
        fixture.service.serve_storage(&plan).await?;
        Ok(fixture.shell.calls()?)
    }

    #[tokio::test]
    async fn access_is_synced_once_on_every_node() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = plan("{}", mesh_addresses())?;
        fixture.service.sync_access(&plan).await?;
        let calls = fixture.shell.calls()?;
        let project = plan.project_id.0.to_string();
        let commit = "e".repeat(40);
        let sync = vec![
            MMD,
            "access",
            "sync",
            "--project",
            project.as_str(),
            "--environment",
            "production",
            "--commit",
            commit.as_str(),
        ];
        assert_eq!(
            programs(&calls),
            [
                ("app-1", sync.clone()),
                ("app-2", sync.clone()),
                ("db-1", sync)
            ]
        );
        assert!(calls.iter().all(|call| call.stdin.is_none()));
        assert_no_swarm(&calls);
        Ok(())
    }

    #[tokio::test]
    async fn every_node_is_provisioned_with_the_plans_runtime_and_then_verified()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = plan("{}", mesh_addresses())?;
        let records = fixture
            .service
            .provision_nodes(
                &plan,
                Path::new("/srv/project"),
                &crate::app::ports::NoProgress,
            )
            .await?;
        assert_eq!(
            records
                .iter()
                .map(|record| (record.name.as_str(), record.private_address))
                .collect::<Vec<_>>(),
            [
                ("app-1", Ipv4Addr::new(10, 90, 0, 1)),
                ("app-2", Ipv4Addr::new(10, 90, 0, 2)),
                ("db-1", Ipv4Addr::new(10, 90, 0, 3))
            ]
        );
        assert!(records.iter().all(|record| record.host_key == HOST_KEY
            && record.daemon_fingerprint == "d".repeat(64)
            && record.version == "1.2.3"));
        let bootstraps = fixture
            .bootstrap
            .plans
            .lock()
            .map_err(|_| "poisoned")?
            .clone();
        assert_eq!(bootstraps.len(), 3);
        for bootstrap in &bootstraps {
            bootstrap.validate()?;
            assert_eq!(bootstrap.selected_runtime, RuntimeKind::DockerCompose);
            assert!(!bootstrap.rootless);
            assert!(bootstrap.access.is_some());
            assert!(bootstrap.target.definition.ingress.is_none());
            assert!(
                bootstrap
                    .required_capabilities
                    .contains(&BootstrapCapability::Docker)
                    && !bootstrap
                        .required_capabilities
                        .contains(&BootstrapCapability::Haproxy)
            );
        }
        assert!(
            fixture.shell.calls()?.is_empty(),
            "provisioning runs through the bootstrap executor"
        );

        fixture.service.verify(&plan, &records).await?;
        let calls = fixture.shell.calls()?;
        assert_eq!(
            programs(&calls),
            [
                ("app-1", vec![MMD, "--print-identity"]),
                ("app-2", vec![MMD, "--print-identity"]),
                ("db-1", vec![MMD, "--print-identity"])
            ]
        );
        assert_no_swarm(&calls);

        let mut other_identity = records;
        other_identity[1].daemon_fingerprint = "f".repeat(64);
        assert!(matches!(
            fixture.service.verify(&plan, &other_identity).await,
            Err(MmError::Security { .. })
        ));
        Ok(())
    }

    /// The minimal fixture project with the three-node target, each node's
    /// keys recorded unless it is in `unrecorded`.
    fn rotation_project(root: &Path, unrecorded: &[&str]) -> std::io::Result<()> {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
        std::fs::create_dir_all(root.join(".config"))?;
        let mut config = std::fs::read_to_string(source.join(".config/mm.yaml"))?
            .replace("target: local\n", "target: production\n");
        config.push_str("targets:\n  production:\n    nodes:\n");
        for (name, host) in ["app-1", "app-2", "db-1"].into_iter().zip(11..) {
            config.push_str(&format!(
                "      - name: {name}\n        host: 203.0.113.{host}\n"
            ));
            if !unrecorded.contains(&name) {
                config.push_str(&format!(
                    "        host-key: {HOST_KEY}\n        daemon-fingerprint: {}\n",
                    "d".repeat(64)
                ));
            }
        }
        std::fs::write(root.join(".config/mm.yaml"), config)
    }

    /// Every node runs `mm0` with both others as peers.
    fn meshed(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
        let plan = plan("{}", mesh_addresses())?;
        for node in &plan.nodes {
            let facts = facts_with(node, &plan, &["app-1", "app-2", "db-1"]);
            fixture.shell.hold(&node.name, facts)?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn rotating_a_key_changes_the_node_and_its_one_entry_on_each_peer()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::app::MeshRotationRequest;

        let directory = crate::test_support::tempdir()?;
        rotation_project(directory.path(), &[])?;
        let fixture = fixture();
        meshed(&fixture)?;
        let request = MeshRotationRequest {
            start: directory.path().to_path_buf(),
            environment: None,
            node: "app-2".to_owned(),
        };
        let planned = fixture.service.plan_mesh_rotation(&request).await?;
        assert_eq!(
            (
                planned.node.as_str(),
                planned.target.as_str(),
                planned.address
            ),
            ("app-2", "production", Ipv4Addr::new(10, 90, 0, 2))
        );
        assert_eq!(planned.current_public_key, key_of("app-2"));
        assert_eq!(planned.pending_public_key, None);
        assert_eq!(
            planned
                .peers
                .iter()
                .map(|peer| (peer.node.as_str(), peer.holds.clone()))
                .collect::<Vec<_>>(),
            [
                ("app-1", Some(key_of("app-2"))),
                ("db-1", Some(key_of("app-2")))
            ]
        );
        assert_eq!(planned.plan_hash, planned.canonical_hash()?);
        let reads = fixture.shell.calls()?;
        assert!(
            reads.iter().all(|call| matches!(
                call.command,
                ClusterNodeCommand::Script {
                    script: ClusterScript::MeshFacts,
                    ..
                }
            )),
            "planning only reads: {reads:?}"
        );

        // Another hash than the reviewed one changes nothing.
        fixture.shell.forget_calls()?;
        assert!(matches!(
            fixture
                .service
                .apply_mesh_rotation(&request, "0".repeat(64).as_str())
                .await,
            Err(MmError::State { .. })
        ));
        assert!(programs(&fixture.shell.calls()?).is_empty());
        assert!(scripts(&fixture.shell.calls()?, ClusterScript::Mesh).is_empty());

        fixture.shell.forget_calls()?;
        let result = fixture
            .service
            .apply_mesh_rotation(&request, &planned.plan_hash)
            .await?;
        let new_key = key_of("app-2-rotated");
        assert_eq!(result.public_key, new_key);
        assert_eq!(result.previous_public_key, key_of("app-2"));
        assert_eq!(result.updated_peers, ["app-1", "db-1"]);
        let calls = fixture.shell.calls()?;
        // The node stages its new key, the peers switch, the node activates.
        assert_eq!(
            programs(&calls),
            [
                ("app-2", vec![MMD, "mesh", "rotate", "--stage"]),
                ("app-2", vec![MMD, "mesh", "rotate", "--activate"]),
            ]
        );
        let states = applied_states(&calls)?;
        assert_eq!(
            states
                .iter()
                .map(|(node, _)| node.as_str())
                .collect::<Vec<_>>(),
            ["app-1", "db-1"],
            "the rotated node's own peers are not touched"
        );
        let before = plan("{}", mesh_addresses())?;
        for (node, state) in &states {
            let held = facts_with(
                before
                    .nodes
                    .iter()
                    .find(|held| held.name == *node)
                    .ok_or("node")?,
                &before,
                &["app-1", "app-2", "db-1"],
            )
            .state
            .ok_or("state")?;
            assert_eq!(state.peers.len(), held.peers.len(), "{node}");
            for (now, was) in state.peers.iter().zip(&held.peers) {
                if now.name == "app-2" {
                    assert_eq!(now.public_key, new_key, "{node}");
                    assert_eq!(
                        (&now.endpoint, &now.allowed_ips),
                        (&was.endpoint, &was.allowed_ips)
                    );
                } else {
                    assert_eq!(now, was, "{node}: the entry for {} is as it was", now.name);
                }
            }
        }
        let order = calls
            .iter()
            .filter_map(|call| match &call.command {
                ClusterNodeCommand::Program(argv) => argv.last().cloned(),
                ClusterNodeCommand::Script {
                    script: ClusterScript::Mesh,
                    ..
                } => Some(format!("peer {}", call.node)),
                ClusterNodeCommand::Script {
                    script: ClusterScript::Reach,
                    ..
                } => Some("reach".to_owned()),
                ClusterNodeCommand::Script { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            ["--stage", "peer app-1", "peer db-1", "--activate", "reach"]
        );
        assert_eq!(
            scripts(&calls, ClusterScript::Reach),
            [("app-2", vec!["0", "10.90.0.1", "10.90.0.3"], None)]
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_rotation_needs_a_known_node_and_every_node_set_up()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::app::MeshRotationRequest;

        let directory = crate::test_support::tempdir()?;
        rotation_project(directory.path(), &[])?;
        let fixture = fixture();
        meshed(&fixture)?;
        let request = |node: &str| MeshRotationRequest {
            start: directory.path().to_path_buf(),
            environment: None,
            node: node.to_owned(),
        };
        let unknown = fixture
            .service
            .plan_mesh_rotation(&request("app-9"))
            .await
            .err()
            .ok_or("an unknown node was accepted")?
            .to_string();
        assert!(
            unknown.contains("has no node 'app-9'; its nodes are app-1, app-2, db-1"),
            "{unknown}"
        );

        // db-1 has no mesh yet: a rotation would cut it off.
        fixture.shell.hold("db-1", MeshFacts::default())?;
        let not_meshed = fixture
            .service
            .plan_mesh_rotation(&request("app-1"))
            .await
            .err()
            .ok_or("a node without a mesh was accepted")?
            .to_string();
        assert!(
            not_meshed.contains("node db-1 does not run the mesh interface mm0 yet"),
            "{not_meshed}"
        );
        assert!(not_meshed.contains("mm target setup plan"), "{not_meshed}");

        let unrecorded = crate::test_support::tempdir()?;
        rotation_project(unrecorded.path(), &["app-2"])?;
        let error = fixture
            .service
            .plan_mesh_rotation(&MeshRotationRequest {
                start: unrecorded.path().to_path_buf(),
                environment: None,
                node: "app-1".to_owned(),
            })
            .await
            .err()
            .ok_or("an unverified node was contacted")?
            .to_string();
        assert!(
            error.contains("node app-2 has no recorded SSH host key"),
            "{error}"
        );
        Ok(())
    }

    /// The three-node plan with `db-1` left out by `--nodes app-1,app-2`.
    fn subset_plan(cluster: &str) -> Result<ClusterSetupPlan, Box<dyn std::error::Error>> {
        let mut plan = plan(cluster, mesh_addresses())?;
        plan.mesh = mesh_plan(&plan.cluster, &plan.nodes, &["db-1"]);
        let left_out = plan.nodes.remove(2);
        plan.skipped = vec![crate::domain::cluster_setup::ClusterSkippedNode {
            name: left_out.name,
            position: 2,
            private_address: left_out.private_address,
        }];
        plan.plan_hash = plan.canonical_hash()?;
        Ok(plan)
    }

    #[tokio::test]
    async fn a_subset_setup_never_contacts_the_node_it_leaves_out()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let plan = subset_plan("{}")?;
        // app-1 already has db-1 as a peer; app-2 never had.
        let facts = facts_with(&plan.nodes[0], &plan, &["app-2", "db-1"]);
        fixture.shell.hold("app-1", facts)?;
        fixture
            .service
            .prepare_nodes(&plan, &crate::app::ports::NoProgress)
            .await?;
        fixture.service.connect_nodes(&plan).await?;
        fixture.service.serve_storage(&plan).await?;
        fixture.service.sync_access(&plan).await?;
        let calls = fixture.shell.calls()?;
        assert!(
            calls.iter().all(|call| call.node != "db-1"),
            "nothing runs on the node that was left out"
        );
        // A node keeps the entry it holds for db-1, whose key cannot be
        // read now, and gets the other selected node as a peer.
        let states = applied_states(&calls)?;
        assert_eq!(
            states
                .iter()
                .map(|(node, state)| (
                    node.as_str(),
                    state
                        .peers
                        .iter()
                        .map(|peer| (peer.name.as_str(), peer.public_key.clone()))
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>(),
            [
                (
                    "app-1",
                    vec![("app-2", key_of("app-2")), ("db-1", key_of("db-1"))]
                ),
                ("app-2", vec![("app-1", key_of("app-1"))]),
            ]
        );
        assert_eq!(
            scripts(&calls, ClusterScript::Reach),
            [
                ("app-1", vec!["1420", "10.90.0.2"], None),
                ("app-2", vec!["1420", "10.90.0.1"], None)
            ],
            "only the nodes set up now are checked"
        );
        // The storage node is set up, and still exports to every node.
        assert_eq!(
            scripts(&calls, ClusterScript::Storage),
            [(
                "app-1",
                vec![
                    "/var/lib/mm/storage",
                    "10.90.0.1",
                    "10.90.0.1",
                    "10.90.0.2",
                    "10.90.0.3"
                ],
                None
            )]
        );
        assert_eq!(
            programs(&calls)
                .iter()
                .filter(|(_, argv)| argv.contains(&"access"))
                .map(|(node, _)| *node)
                .collect::<Vec<_>>(),
            ["app-1", "app-2"],
            "access is synced on the selected nodes only"
        );
        Ok(())
    }

    #[tokio::test]
    async fn only_the_selected_nodes_are_scanned() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = fixture();
        let target: crate::domain::repo_target::RepoTargetConfig = serde_yaml_ng::from_str(
            "nodes:\n  - {name: app-1, host: 203.0.113.11}\n  - {name: app-2, host: 203.0.113.12}\n  - {name: db-1, host: 203.0.113.13}\n",
        )?;
        let only = ["db-1".to_owned(), "app-1".to_owned()];
        let (nodes, skipped) = fixture.service.scan_nodes(&target, false, &only).await?;
        assert_eq!(
            nodes
                .iter()
                .map(|node| (node.name.as_str(), node.private_address.to_string()))
                .collect::<Vec<_>>(),
            [
                ("app-1", "10.90.0.1".to_owned()),
                ("db-1", "10.90.0.3".to_owned())
            ],
            "a node keeps its address whether or not the others are set up"
        );
        assert_eq!(
            skipped
                .iter()
                .map(|node| (
                    node.name.as_str(),
                    node.position,
                    node.private_address.to_string()
                ))
                .collect::<Vec<_>>(),
            [("app-2", 1, "10.90.0.2".to_owned())]
        );
        let (all, none) = fixture.service.scan_nodes(&target, false, &[]).await?;
        assert_eq!((all.len(), none.len()), (3, 0));

        let unknown = fixture
            .service
            .scan_nodes(&target, false, &["app-9".to_owned()])
            .await
            .err()
            .ok_or("an unknown node was accepted")?
            .to_string();
        assert!(unknown.contains("--nodes names 'app-9'"), "{unknown}");
        assert!(unknown.contains("app-1, app-2, db-1"), "{unknown}");
        Ok(())
    }

    #[test]
    fn steps_that_need_a_left_out_node_limit_themselves_and_say_so()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::super::subset;
        let target: crate::domain::repo_target::RepoTargetConfig = serde_yaml_ng::from_str(
            "nodes:\n  - {name: app-1, host: 203.0.113.11}\n  - {name: app-2, host: 203.0.113.12}\n  - {name: db-1, host: 203.0.113.13}\ncluster: {storage: db-1}\n",
        )?;
        let mut notices = Vec::new();
        assert_eq!(
            subset::steps(&target, "db-1", &[], &mut notices),
            ClusterSetupStep::ALL
        );
        assert!(notices.is_empty());

        let skipped = subset_plan("{}")?.skipped;
        let steps = subset::steps(&target, "db-1", &skipped, &mut notices);
        assert!(
            !steps.contains(&ClusterSetupStep::ServeStorage),
            "the storage node is left out, so storage is not touched"
        );
        assert_eq!(steps.len(), ClusterSetupStep::ALL.len() - 1);
        let text = notices.join("\n");
        for expected in [
            "left out: db-1",
            "it keeps the mm version and configuration it has now",
            "keep the WireGuard peers they already have for db-1",
            "db-1 serves shared volumes and is left out",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }

        // Another node serves storage: the step runs.
        let mut notices = Vec::new();
        let steps = subset::steps(&target, "app-1", &skipped, &mut notices);
        assert_eq!(steps, ClusterSetupStep::ALL);

        // An unreachable node is named with the way to go on without it.
        let down =
            subset::unreachable(&target, &target.nodes[2], &unused("ssh-keyscan")).to_string();
        assert!(down.contains("node 'db-1' (203.0.113.13:22)"), "{down}");
        assert!(down.contains("add `--nodes app-1,app-2`"), "{down}");
        let silent = MmError::ProcessFailed {
            program: "ssh-keyscan".to_owned(),
            code: Some(1),
            stderr: String::new(),
        };
        let down = subset::unreachable(&target, &target.nodes[2], &silent).to_string();
        assert!(
            down.contains("did not present an SSH host key (nothing answered the key scan)"),
            "{down}"
        );
        let foreign = subset::foreign_host_key(&target, &target.nodes[2]).to_string();
        assert!(foreign.contains("--trust-new-host-keys"), "{foreign}");
        assert!(foreign.contains("add `--nodes app-1,app-2`"), "{foreign}");
        Ok(())
    }
}
