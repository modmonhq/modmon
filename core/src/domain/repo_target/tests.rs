use super::*;

const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w";

fn target() -> RepoTargetConfig {
    RepoTargetConfig {
        host: Some("prod-eu.example.test".to_owned()),
        port: 2222,
        host_key: Some(HOST_KEY.to_owned()),
        daemon_fingerprint: Some("a".repeat(64)),
        nodes: Vec::new(),
        cluster: RepoClusterConfig::default(),
        setup: None,
        delivery: Default::default(),
        providers: std::collections::BTreeMap::new(),
        members: Removed,
        swarm: Removed,
    }
}

#[test]
fn repo_facts_become_a_relay_target() -> Result<(), String> {
    let project = ProjectId(uuid::Uuid::from_u128(4));
    let named = target().named_target(project, "prod-eu")?;
    let TargetTransport::Ssh {
        user,
        port,
        host_key_fingerprint,
        daemon_fingerprint,
        ..
    } = &named.definition.transport
    else {
        return Err("not an SSH target".to_owned());
    };
    assert_eq!(user, RELAY_ACCOUNT);
    assert_eq!(*port, 2222);
    assert_eq!(
        host_key_fingerprint,
        "SHA256:0ilAVEYiaM4+PCd17vMiY2bh4bv814hVSbQKrN45DMU"
    );
    assert_eq!(daemon_fingerprint.as_deref(), Some("a".repeat(64).as_str()));
    assert_eq!(
        target().named_target(project, "prod-eu")?.definition.id,
        named.definition.id,
        "the same project and name keep one identity"
    );
    Ok(())
}

#[test]
fn invalid_facts_are_rejected() {
    let mut bad_key = target();
    bad_key.host_key = Some("ssh-ed25519 not-base64!".to_owned());
    assert!(bad_key.validate("prod").is_err());
    let mut bad_daemon = target();
    bad_daemon.daemon_fingerprint = Some("short".to_owned());
    assert!(bad_daemon.validate("prod").is_err());
    let mut bad_host = target();
    bad_host.host = Some("two words".to_owned());
    assert!(bad_host.validate("prod").is_err());
    assert!(target().validate("Bad Name").is_err());
}

#[test]
fn a_flat_target_is_one_endpoint_and_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let target = target();
    assert!(!target.is_cluster());
    let pin = target.pin("prod-eu")?;
    assert_eq!(pin.host_key.as_deref(), Some(HOST_KEY));
    assert!(pin.nodes.is_empty());
    assert!(pin.covers(&pin));

    let project = ProjectId(uuid::Uuid::from_u128(4));
    let named = target.named_target(project, "prod-eu")?;
    crate::domain::target::validate_target_definition(&named.definition)?;
    assert!(named.definition.nodes.is_empty());
    assert!(!named.definition.is_cluster());
    assert_eq!(named.definition.default_runtime, None);
    assert!(matches!(
        &named.definition.transport,
        crate::domain::target::TargetTransport::Ssh { host, port: 2222, .. }
            if host == "prod-eu.example.test"
    ));

    let yaml = serde_yaml_ng::to_string(&target)?;
    for removed in ["members", "swarm", "cluster", "nodes", "setup"] {
        assert!(!yaml.contains(removed), "{removed} in {yaml}");
    }
    assert_eq!(serde_yaml_ng::from_str::<RepoTargetConfig>(&yaml)?, target);

    let default_port: RepoTargetConfig = serde_yaml_ng::from_str(&format!(
        "host: a.example.test\nhost-key: {HOST_KEY}\ndaemon-fingerprint: {}\n",
        "a".repeat(64)
    ))?;
    assert_eq!(default_port.port, 22);
    default_port.validate("prod")?;
    assert!(
        serde_yaml_ng::from_str::<RepoTargetConfig>("host: a.example.test\nuser: root\n").is_err(),
        "a target always connects as the relay account"
    );
    Ok(())
}
