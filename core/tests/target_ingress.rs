use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use mm::adapters::config::ConfigLoader;
use mm::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
use mm::app::ports::{TargetIngressArtifactStore, TargetStore};
use mm::app::{TargetIngressManager, TargetManager};
use mm::domain::error::MmError;
use mm::domain::target::{
    DEFAULT_TRAEFIK_IMAGE, NamedTarget, TargetDefinition, TargetId, TargetIngress, TargetTransport,
};
use mm::domain::target_ingress::{
    CompiledTargetIngressArtifact, PreparedTargetIngressArtifact, TRAEFIK_ACME_FILE_MODE,
    TRAEFIK_CERTIFICATE_RESOLVER, ingress_dynamic_config_volume_name, ingress_loopback_ports,
    ingress_network_name, ingress_proxy_ports, ingress_verify_port,
};
use uuid::Uuid;

mod temp_dirs;

const TARGET_ID: &str = "319c527b-6242-7b43-bfd4-bd60e27c1de3";
const TRAEFIK_IMAGE: &str = "docker.io/library/traefik@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct MismatchedTargetStore(NamedTarget);

impl TargetStore for MismatchedTargetStore {
    fn list_targets(&self) -> Result<Vec<NamedTarget>, MmError> {
        unreachable!("ingress planning resolves one named target")
    }

    fn resolve_target(&self, _name: &str) -> Result<NamedTarget, MmError> {
        Ok(self.0.clone())
    }

    fn insert_target(
        &self,
        _name: &str,
        _definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        unreachable!("ingress planning does not insert targets")
    }

    fn ensure_target(
        &self,
        _name: &str,
        _definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        unreachable!("ingress planning does not ensure targets")
    }

    fn remove_target(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
    ) -> Result<NamedTarget, MmError> {
        unreachable!("ingress planning does not remove targets")
    }

    fn update_target_transport(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
        _transport: TargetTransport,
    ) -> Result<NamedTarget, MmError> {
        unreachable!("ingress planning does not update target transport")
    }

    fn update_target_ingress(
        &self,
        _name: &str,
        _expected_fingerprint: &str,
        _ingress: TargetIngress,
    ) -> Result<NamedTarget, MmError> {
        unreachable!("ingress planning does not update target ingress")
    }
}

struct NoArtifactPublication;

impl TargetIngressArtifactStore for NoArtifactPublication {
    fn publish(
        &self,
        _artifact: &CompiledTargetIngressArtifact,
    ) -> Result<PreparedTargetIngressArtifact, MmError> {
        unreachable!("fingerprint rejection happens before publication")
    }
}

#[test]
fn resolved_target_fingerprint_must_match_its_definition() {
    let mut target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::from_u128(1))),
    );
    target.fingerprint = "stale".to_owned();
    let manager = TargetIngressManager::new(
        Arc::new(MismatchedTargetStore(target)),
        Arc::new(NoArtifactPublication),
    );

    assert!(matches!(
        manager.plan("local"),
        Err(MmError::Security { message })
            if message == "resolved target fingerprint does not match its definition"
    ));
}

#[test]
fn configures_only_digest_pinned_traefik_with_reviewed_target_state()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    let target = store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(TARGET_ID)?)),
    )?;
    let manager = TargetManager::new(Arc::new(ConfigLoader), Arc::clone(&store));

    assert!(
        manager
            .configure_traefik(
                "local",
                &target.fingerprint,
                "docker.io/library/traefik:v3".to_owned(),
                Some("ops@example.test".to_owned()),
            )
            .is_err()
    );
    assert!(
        manager
            .configure_traefik(
                "local",
                &target.fingerprint,
                TRAEFIK_IMAGE.to_owned(),
                Some("invalid email".to_owned()),
            )
            .is_err()
    );
    let configured = manager.configure_traefik(
        "local",
        &target.fingerprint,
        TRAEFIK_IMAGE.to_owned(),
        Some("ops@example.test".to_owned()),
    )?;
    assert_eq!(configured.definition.id, target.definition.id);
    assert_ne!(configured.fingerprint, target.fingerprint);
    let source = fs::read_to_string(home.path().join("targets.yaml"))?;
    assert!(source.contains("acme-email: ops@example.test"));
    assert!(!source.contains("acme_email"));
    let ingress = TargetIngressManager::new(
        Arc::clone(&store),
        Arc::new(FilesystemTargetIngressArtifactStore::new(
            home.path().join("ingress"),
        )),
    );
    assert_eq!(format!("{ingress:?}"), "TargetIngressManager { .. }");
    let configured_plan = ingress.plan("local")?;
    assert!(
        fs::read_to_string(configured_plan.artifact.compose_path)?
            .contains("acme.email=ops@example.test")
    );
    assert!(
        manager
            .configure_traefik(
                "local",
                &target.fingerprint,
                TRAEFIK_IMAGE.to_owned(),
                Some("stale@example.test".to_owned()),
            )
            .is_err()
    );
    let without_email = manager.configure_traefik(
        "local",
        &configured.fingerprint,
        TRAEFIK_IMAGE.to_owned(),
        None,
    )?;
    assert_ne!(without_email.fingerprint, configured.fingerprint);
    let source = fs::read_to_string(home.path().join("targets.yaml"))?;
    assert!(!source.contains("acme-email"));
    Ok(())
}

#[test]
fn publishes_one_deterministic_target_owned_ingress_artifact_from_default_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    let mut no_ingress = TargetDefinition::local(TargetId(Uuid::from_u128(17)));
    no_ingress.ingress = None;
    targets.insert_target("no-ingress", no_ingress)?;
    targets.insert_target(
        "production",
        TargetDefinition::local(TargetId(Uuid::parse_str(TARGET_ID)?)),
    )?;
    let artifacts: Arc<dyn TargetIngressArtifactStore> = Arc::new(
        FilesystemTargetIngressArtifactStore::new(home.path().join("ingress")),
    );
    let manager = TargetIngressManager::new(Arc::clone(&targets), Arc::clone(&artifacts));
    assert!(manager.plan("no-ingress").is_err());

    let first = manager.plan("production")?;
    let second = manager.plan("production")?;
    assert_eq!(first, second);
    assert_eq!(first.plan_hash, first.artifact.manifest.artifact_hash);
    assert_eq!(first.artifact.manifest.image, DEFAULT_TRAEFIK_IMAGE);
    assert_eq!(first.artifact.manifest.acme_email, None);
    assert_eq!(
        first.artifact.manifest.network_name,
        ingress_network_name(first.target_id)
    );
    assert_eq!(
        first.artifact.manifest.dynamic_config_volume_name,
        ingress_dynamic_config_volume_name(first.target_id)
    );
    assert_eq!(
        first.artifact.manifest.acme_file_mode,
        TRAEFIK_ACME_FILE_MODE
    );
    assert_eq!(
        first.artifact.manifest.certificate_resolver,
        TRAEFIK_CERTIFICATE_RESOLVER
    );
    assert_eq!(
        first.artifact.manifest.canonical_hash(),
        first.artifact.manifest.artifact_hash
    );
    assert!(
        first.artifact.root.starts_with(
            home.path()
                .join("ingress/319c527b-6242-7b43-bfd4-bd60e27c1de3")
        )
    );

    let compose = fs::read_to_string(&first.artifact.compose_path)?;
    let manifest = fs::read_to_string(&first.artifact.manifest_path)?;
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&compose)?;
    assert_eq!(
        parsed["services"]["traefik"]["image"],
        DEFAULT_TRAEFIK_IMAGE
    );
    assert!(!manifest.contains("acme_email"));
    assert_eq!(parsed["services"]["traefik"]["read_only"], true);
    assert_eq!(parsed["networks"]["mm-ingress"]["external"], true);
    assert_eq!(parsed["volumes"]["mm-traefik-acme"]["external"], true);
    assert_eq!(parsed["volumes"]["mm-traefik-dynamic"]["external"], true);
    assert!(compose.contains("providers.file.directory"));
    assert!(!compose.contains("providers.docker"));
    assert!(!compose.contains("docker.sock"));
    let proxy_ports = ingress_loopback_ports(first.target_id);
    assert_eq!(
        ingress_proxy_ports(first.target_id),
        (proxy_ports.0, proxy_ports.1)
    );
    assert_eq!(ingress_verify_port(first.target_id), proxy_ports.2);
    assert!((20_000..=39_999).contains(&proxy_ports.0));
    assert_eq!((proxy_ports.0 - 20_000) % 3, 0);
    assert_eq!(proxy_ports.1, proxy_ports.0 + 1);
    assert_eq!(proxy_ports.2, proxy_ports.0 + 2);
    assert!(compose.contains(&format!("127.0.0.1:{}:8080", proxy_ports.0)));
    assert!(compose.contains(&format!("127.0.0.1:{}:8443", proxy_ports.1)));
    assert!(compose.contains(&format!("127.0.0.1:{}:8082", proxy_ports.2)));
    assert_eq!(first.artifact.manifest.proxy_http_port, proxy_ports.0);
    assert_eq!(first.artifact.manifest.proxy_https_port, proxy_ports.1);
    assert_eq!(first.artifact.manifest.proxy_verify_port, proxy_ports.2);
    let mut changed_verify_port = first.artifact.manifest.clone();
    changed_verify_port.proxy_verify_port += 1;
    assert_ne!(
        changed_verify_port.canonical_hash(),
        first.artifact.manifest.artifact_hash
    );
    assert_eq!(first.artifact.support_paths.len(), 4);
    let haproxy = fs::read_to_string(first.artifact.root.join("haproxy.cfg"))?;
    assert!(haproxy.contains(&format!("127.0.0.1:{}", proxy_ports.0)));
    assert!(haproxy.contains(&format!("127.0.0.1:{}", proxy_ports.1)));
    assert!(haproxy.contains("send-proxy-v2"));
    let forwarder = fs::read_to_string(first.artifact.root.join("mm-ingress-forwarder.service"))?;
    assert!(forwarder.contains("CAP_NET_BIND_SERVICE"));
    assert!(compose.contains("ping.entrypoint=ping"));
    assert!(compose.contains("entrypoints.ping.address=:8081"));
    assert!(compose.contains("--entrypoints.mm-verify.address=:8082"));
    assert!(compose.contains("proxyprotocol.trustedips"));
    assert!(compose.contains("entrypoints.web.forwardedheaders.trustedips=127.0.0.1/32,::1/128"));
    assert!(
        compose.contains("entrypoints.websecure.forwardedheaders.trustedips=127.0.0.1/32,::1/128")
    );
    assert_eq!(
        first.artifact.manifest.target_compose_path,
        "%h/.local/share/mm/ingress/current/compose.yaml"
    );
    assert!(
        first
            .artifact
            .manifest
            .support_files
            .iter()
            .all(|file| file.mode == "0644" && !file.destination.is_empty())
    );
    assert_eq!(
        fs::read_to_string(first.artifact.root.join("dynamic.yaml"))?,
        mm::domain::traefik_dynamic::EMPTY_DYNAMIC_CONFIG
    );
    assert!(!compose.contains(".acme.email"));
    assert!(compose.contains("acme.httpchallenge.entrypoint=web"));
    assert!(!compose.contains(&home.path().display().to_string()));

    fs::write(first.artifact.root.join("unexpected"), "tampered")?;
    assert!(manager.plan("production").is_err());
    Ok(())
}

#[test]
fn concurrent_first_ingress_plans_publish_one_complete_artifact()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    let target = targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(TARGET_ID)?)),
    )?;
    TargetManager::new(Arc::new(ConfigLoader), Arc::clone(&targets)).configure_traefik(
        "local",
        &target.fingerprint,
        TRAEFIK_IMAGE.to_owned(),
        Some("ops@example.test".to_owned()),
    )?;
    let manager = Arc::new(TargetIngressManager::new(
        targets,
        Arc::new(FilesystemTargetIngressArtifactStore::new(
            home.path().join("ingress"),
        )),
    ));
    let barrier = Arc::new(Barrier::new(2));
    let handles = [(), ()].map(|()| {
        let manager = Arc::clone(&manager);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            manager.plan("local")
        })
    });
    let [first, second] = handles;
    let first = first.join().map_err(|_| "first planner panicked")??;
    let second = second.join().map_err(|_| "second planner panicked")??;
    assert_eq!(first, second);
    assert!(first.artifact.compose_path.is_file());
    assert!(first.artifact.manifest_path.is_file());
    Ok(())
}

#[test]
fn macos_local_profile_trusts_the_fixed_gateway_and_leaves_remote_targets_linux()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::target_ingress::{
        IngressHostProfile, MacosHostFacts, MacosIngressHost, TargetIngressStep,
        ingress_network_subnet,
    };

    let home = temp_dirs::tempdir()?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    let local_id = TargetId(Uuid::parse_str(TARGET_ID)?);
    targets.insert_target("local", TargetDefinition::local(local_id))?;
    let mut remote = TargetDefinition::local(TargetId(Uuid::from_u128(41)));
    remote.transport = TargetTransport::Ssh {
        host: "example.test".to_owned(),
        port: 22,
        user: "deploy".to_owned(),
        host_key_fingerprint: format!("SHA256:{}", "A".repeat(43)),
        host_key: None,
        identity_file: None,
        remote_root: None,
        daemon_fingerprint: None,
    };
    targets.insert_target("remote", remote)?;
    let artifacts: Arc<dyn TargetIngressArtifactStore> = Arc::new(
        FilesystemTargetIngressArtifactStore::new(home.path().join("ingress")),
    );
    let linux =
        TargetIngressManager::new(Arc::clone(&targets), Arc::clone(&artifacts)).plan("local")?;
    let (_, network_gateway) = ingress_network_subnet(local_id);
    let facts = MacosHostFacts {
        haproxy_executable: "/opt/homebrew/bin/haproxy".to_owned(),
        docker_executable: "/usr/local/bin/docker".to_owned(),
        docker_host: "unix:///srv/dev/.docker/run/docker.sock".to_owned(),
        home: "/srv/dev".to_owned(),
    };
    let host = MacosIngressHost::for_target(facts.clone(), local_id);
    let manager = TargetIngressManager::new(Arc::clone(&targets), Arc::clone(&artifacts))
        .with_local_host_profile(IngressHostProfile::MacosLaunchd(facts));

    let macos = manager.plan("local")?;
    assert_ne!(macos.plan_hash, linux.plan_hash);
    assert_eq!(macos.artifact.manifest.macos_host, Some(host));
    assert_eq!(
        macos.artifact.manifest.steps[4..],
        [
            TargetIngressStep::InstallLaunchdAutostart,
            TargetIngressStep::InstallLaunchdForwarder
        ]
    );
    assert_eq!(
        macos.artifact.manifest.target_compose_path,
        "/srv/dev/Library/Application Support/mm/ingress/current/compose.yaml"
    );
    let compose = fs::read_to_string(&macos.artifact.compose_path)?;
    let trusted = format!("{network_gateway}/32");
    for entrypoint in ["web", "websecure"] {
        assert!(compose.contains(&format!(
            "entrypoints.{entrypoint}.proxyprotocol.trustedips={trusted}"
        )));
        assert!(compose.contains(&format!(
            "entrypoints.{entrypoint}.forwardedheaders.trustedips={trusted}"
        )));
    }
    assert!(!compose.contains("127.0.0.1/32"));
    assert!(macos.artifact.root.join("local.mm.ingress.plist").is_file());
    assert!(
        !macos
            .artifact
            .root
            .join("mm-ingress-forwarder.service")
            .exists()
    );
    assert_eq!(manager.plan("local")?, macos);

    let remote = manager.plan("remote")?;
    assert_eq!(remote.artifact.manifest.macos_host, None);
    assert!(
        remote
            .artifact
            .root
            .join("mm-ingress-forwarder.service")
            .is_file()
    );
    Ok(())
}
