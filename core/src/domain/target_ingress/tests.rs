use super::{
    CompiledTargetIngressArtifact, MacosIngressHost, TargetIngressArtifactManifest,
    TargetIngressStep, TargetIngressSupportFileManifest, ingress_loopback_ports,
    ingress_network_subnet, ingress_proxy_ports, ingress_verify_port, ipv4_subnets_overlap,
};
use crate::domain::target::TargetId;
use uuid::Uuid;

fn manifest() -> TargetIngressArtifactManifest {
    TargetIngressArtifactManifest {
        target_id: TargetId(Uuid::from_u128(7)),
        target_name: "local".to_owned(),
        target_fingerprint: "a".repeat(64),
        provider: "traefik".to_owned(),
        image: "traefik@sha256:abc".to_owned(),
        acme_email: Some("operator@example.test".to_owned()),
        compose_project_name: "mm-ingress".to_owned(),
        network_name: "mm-ingress".to_owned(),
        acme_volume_name: "acme".to_owned(),
        dynamic_config_volume_name: "dynamic".to_owned(),
        dynamic_config_directory: "/dynamic".to_owned(),
        acme_storage_path: "/acme.json".to_owned(),
        acme_file_mode: "0600".to_owned(),
        public_http_port: 80,
        public_https_port: 443,
        proxy_http_port: 20000,
        proxy_https_port: 20001,
        proxy_verify_port: 20002,
        certificate_resolver: "mm-http".to_owned(),
        target_compose_path: "compose.yaml".to_owned(),
        compose_digest: "b".repeat(64),
        support_files: vec![TargetIngressSupportFileManifest {
            name: "config.yaml".to_owned(),
            destination: "/config.yaml".to_owned(),
            mode: "0600".to_owned(),
            digest: "c".repeat(64),
        }],
        steps: vec![TargetIngressStep::ApplyIngressStack],
        macos_host: None,
        artifact_hash: String::new(),
    }
}

#[test]
fn ports_hashes_and_debug_are_deterministic_and_content_sensitive() {
    let target = TargetId(Uuid::from_u128(7));
    let all = ingress_loopback_ports(target);
    assert_eq!(ingress_proxy_ports(target), (all.0, all.1));
    assert_eq!(ingress_verify_port(target), all.2);
    assert_eq!(all.1, all.0 + 1);
    assert_eq!(all.2, all.0 + 2);

    let with_email = manifest();
    let first = with_email.canonical_hash();
    let mut without_email = with_email.clone();
    without_email.acme_email = None;
    assert_ne!(first, without_email.canonical_hash());

    let artifact = CompiledTargetIngressArtifact {
        manifest: with_email,
        compose: b"private compose".to_vec(),
        support_files: Vec::new(),
    };
    let debug = format!("{artifact:?}");
    assert!(debug.contains("compose_bytes: 15"));
    assert!(debug.contains("support_file_count: 0"));
    assert!(!debug.contains("private compose"));
}

#[test]
fn macos_host_profile_is_hashed_only_when_present() -> Result<(), serde_json::Error> {
    let linux = manifest();
    let linux_hash = linux.canonical_hash();
    let json = serde_json::to_value(&linux)?;
    assert!(json.get("macos_host").is_none());

    let (network_subnet, network_gateway) = ingress_network_subnet(linux.target_id);
    let host = MacosIngressHost {
        haproxy_executable: "/opt/homebrew/bin/haproxy".to_owned(),
        docker_executable: "/usr/local/bin/docker".to_owned(),
        docker_host: "unix:///srv/dev/.docker/run/docker.sock".to_owned(),
        home: "/srv/dev".to_owned(),
        network_subnet,
        network_gateway,
    };
    let mut macos = linux.clone();
    macos.macos_host = Some(host.clone());
    assert_ne!(macos.canonical_hash(), linux_hash);
    let mut moved = macos.clone();
    if let Some(moved_host) = &mut moved.macos_host {
        moved_host.docker_host = "unix:///var/run/docker.sock".to_owned();
    }
    assert_ne!(moved.canonical_hash(), macos.canonical_hash());
    assert!(host.trusted_proxy_peer().ends_with(".1/32"));
    Ok(())
}

#[test]
fn ingress_subnets_are_deterministic_private_slash_24s() {
    let target = TargetId(Uuid::from_u128(7));
    let (subnet, gateway) = ingress_network_subnet(target);
    assert_eq!(
        ingress_network_subnet(target),
        (subnet.clone(), gateway.clone())
    );
    assert!(subnet.starts_with("10.252.") && subnet.ends_with(".0/24"));
    assert_eq!(
        gateway.rsplit_once('.').map(|(prefix, _)| prefix),
        subnet.rsplit_once('.').map(|(prefix, _)| prefix)
    );
}

#[test]
fn ipv4_subnet_overlap_handles_containment_adjacency_and_invalid_input() {
    assert_eq!(
        ipv4_subnets_overlap("10.252.7.0/24", "10.252.7.128/25"),
        Some(true)
    );
    assert_eq!(
        ipv4_subnets_overlap("10.0.0.0/8", "10.252.7.0/24"),
        Some(true)
    );
    assert_eq!(
        ipv4_subnets_overlap("10.252.7.0/24", "10.252.8.0/24"),
        Some(false)
    );
    assert_eq!(
        ipv4_subnets_overlap("0.0.0.0/0", "192.168.1.0/24"),
        Some(true)
    );
    assert_eq!(ipv4_subnets_overlap("10.252.7.0/24", "fd00::/64"), None);
    assert_eq!(ipv4_subnets_overlap("10.252.7.0/33", "10.252.7.0/24"), None);
}
