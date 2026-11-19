use super::*;

fn plan() -> Result<ClusterSetupPlan, Box<dyn std::error::Error>> {
    let cluster: RepoClusterConfig = serde_yaml_ng::from_str("mesh: {subnet: 10.77.0.0/24}\n")?;
    let setup: RepoTargetSetup =
        serde_yaml_ng::from_str("origin: git@github.com:example/app.git\n")?;
    let mut plan = ClusterSetupPlan {
        project_id: ProjectId(uuid::Uuid::from_u128(1)),
        project_name: "app".to_owned(),
        config_hash: "c".repeat(64),
        environment: "integration".to_owned(),
        target: "integration".to_owned(),
        nodes: vec![ClusterNodePlan {
            name: "app-1".to_owned(),
            host: "203.0.113.1".to_owned(),
            port: 22,
            private_address: Ipv4Addr::new(10, 77, 0, 1),
            host_key: "ssh-ed25519 AAAA".to_owned(),
            host_key_fingerprint: "SHA256:x".to_owned(),
            host_key_origin: HostKeyOrigin::FirstUse,
            cpu: None,
        }],
        skipped: Vec::new(),
        cluster,
        mesh: None,
        storage: "app-1".to_owned(),
        runtime: RuntimeKind::DockerCompose,
        setup,
        input_files: Vec::new(),
        binaries: vec![crate::domain::target::HostBinaries {
            architecture: "x86_64".to_owned(),
            directory: PathBuf::from("/opt/mm"),
            mm_digest: "a".repeat(64),
            mmd_digest: "b".repeat(64),
            source: "/opt/mm".to_owned(),
        }],
        host_binaries: None,
        secrets: vec![ClusterSecretPlan {
            name: "app.key".to_owned(),
            reference: "age://local/app-key".to_owned(),
            revision: "r1".to_owned(),
        }],
        secret_store: None,
        unset_secrets: Vec::new(),
        notices: Vec::new(),
        deploy_key: ClusterDeployKeyPlan {
            secret: "deploy".to_owned(),
            revision: "r2".to_owned(),
            public_key: "ssh-ed25519 BBBB".to_owned(),
        },
        access_commit: "d".repeat(40),
        steps: ClusterSetupStep::ALL.to_vec(),
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash()?;
    Ok(plan)
}

#[test]
fn the_hash_binds_keys_revisions_and_the_commit() -> Result<(), Box<dyn std::error::Error>> {
    let reviewed = plan()?;
    reviewed.validate()?;
    assert!(reviewed.apply_command().ends_with(&reviewed.plan_hash));
    let mut given = reviewed.clone();
    given.host_binaries = Some(PathBuf::from("/opt/mm"));
    assert!(given.apply_command().ends_with(" --host-binaries /opt/mm"));
    let changes: [fn(&mut ClusterSetupPlan); 9] = [
        |plan| plan.binaries[0].mmd_digest = "c".repeat(64),
        |plan| plan.binaries[0].source = "release v9.9.9 for x86_64".to_owned(),
        |plan| plan.nodes[0].cpu = Some("aarch64".to_owned()),
        |plan| plan.nodes[0].host_key = "ssh-ed25519 CCCC".to_owned(),
        |plan| plan.nodes[0].private_address = Ipv4Addr::new(10, 77, 0, 2),
        |plan| plan.runtime = RuntimeKind::PodmanCompose,
        |plan| plan.secrets[0].revision = "r9".to_owned(),
        |plan| plan.deploy_key.revision = "r9".to_owned(),
        |plan| plan.access_commit = "e".repeat(40),
    ];
    for change in changes {
        let mut changed = reviewed.clone();
        change(&mut changed);
        assert!(changed.validate().is_err());
    }
    let mut empty = reviewed;
    empty.nodes.clear();
    assert!(empty.validate().is_err());
    Ok(())
}

#[test]
fn steps_have_stable_labels() {
    let labels = ClusterSetupStep::ALL.map(ClusterSetupStep::label);
    assert_eq!(labels[0], "prepare-nodes");
    assert_eq!(
        labels,
        [
            "prepare-nodes",
            "provision-nodes",
            "connect-nodes",
            "serve-storage",
            "push-secrets",
            "sync-access",
            "verify"
        ]
    );
}

#[test]
fn a_subset_plan_is_another_plan_and_keeps_every_node_address_in_order()
-> Result<(), Box<dyn std::error::Error>> {
    let full = plan()?;
    assert_eq!(full.private_addresses(), [Ipv4Addr::new(10, 77, 0, 1)]);
    assert!(!full.apply_command().contains("--nodes"));

    // app-1 is set up; app-0 before it and app-2 after it are left out.
    let mut subset = full.clone();
    subset.skipped = vec![
        ClusterSkippedNode {
            name: "app-0".to_owned(),
            position: 0,
            private_address: Ipv4Addr::new(10, 77, 0, 9),
        },
        ClusterSkippedNode {
            name: "app-2".to_owned(),
            position: 2,
            private_address: Ipv4Addr::new(10, 77, 0, 2),
        },
    ];
    assert!(
        subset.validate().is_err(),
        "leaving a node out changes the hash"
    );
    subset.plan_hash = subset.canonical_hash()?;
    subset.validate()?;
    assert_ne!(subset.plan_hash, full.plan_hash);
    assert_eq!(
        subset.private_addresses(),
        [
            Ipv4Addr::new(10, 77, 0, 9),
            Ipv4Addr::new(10, 77, 0, 1),
            Ipv4Addr::new(10, 77, 0, 2)
        ],
        "the storage export lists every node, in declared order"
    );
    assert!(
        subset.apply_command().ends_with(" --nodes app-1"),
        "apply needs the same nodes as the plan: {}",
        subset.apply_command()
    );
    // A plan made before subsets existed has no key for them.
    let json = serde_json::to_value(&full)?;
    assert!(json.get("skipped").is_none());
    Ok(())
}
