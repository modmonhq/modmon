use super::*;
use base64::Engine as _;
use sha2::Digest as _;

fn ssh_definition() -> TargetDefinition {
    let mut blob = Vec::new();
    for field in [&b"ssh-ed25519"[..], &[7_u8; 32][..]] {
        blob.extend_from_slice(&(field.len() as u32).to_be_bytes());
        blob.extend_from_slice(field);
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(&blob);
    let fingerprint = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(sha2::Sha256::digest(&blob))
    );
    TargetDefinition {
        id: TargetId(Uuid::from_u128(1)),
        transport: TargetTransport::Ssh {
            host: "host.example.test".to_owned(),
            port: 22,
            user: "mm-relay".to_owned(),
            host_key_fingerprint: fingerprint,
            host_key: Some(format!("ssh-ed25519 {encoded}")),
            identity_file: Some(PathBuf::from("/var/lib/mm/identity")),
            remote_root: Some(PathBuf::from("/srv/mm/project")),
            daemon_fingerprint: Some("d".repeat(64)),
        },
        bootstrap: TargetBootstrap::Ansible,
        ingress: Some(TargetIngress::Traefik {
            image: DEFAULT_TRAEFIK_IMAGE.to_owned(),
            acme_email: Some("operator@example.test".to_owned()),
            acme_ca_server: None,
            acme_ca_certificate: None,
        }),
        default_runtime: Some(crate::domain::runtime::RuntimeKind::DockerCompose),
        available_runtimes: vec![crate::domain::runtime::RuntimeKind::DockerCompose],
        runtime: Some(TargetRuntimeEndpoint {
            socket: Some(PathBuf::from("/run/docker.sock")),
            endpoint: Some("unix:///run/docker.sock".to_owned()),
            executable: Some(PathBuf::from("docker")),
        }),
        nodes: Vec::new(),
    }
}

#[test]
fn target_fingerprints_bind_transport_ingress_runtime_and_optional_paths() -> Result<(), String> {
    let local = TargetDefinition::local(TargetId(Uuid::from_u128(1)));
    assert!(local.supports_runtime(crate::domain::runtime::RuntimeKind::PodmanCompose));
    let local_hash = local.canonical_fingerprint();
    assert_eq!(local_hash.len(), 64);

    let ssh = ssh_definition();
    validate_target_definition(&ssh)?;
    assert!(!ssh.supports_runtime(crate::domain::runtime::RuntimeKind::PodmanCompose));
    let ssh_hash = ssh.canonical_fingerprint();
    assert_ne!(local_hash, ssh_hash);
    let mut changed = ssh.clone();
    let runtime = changed.runtime.as_mut();
    assert!(runtime.is_some());
    if let Some(runtime) = runtime {
        runtime.socket = None;
    }
    assert_ne!(changed.canonical_fingerprint(), ssh_hash);
    if let Some(runtime) = changed.runtime.as_mut() {
        runtime.endpoint = None;
        runtime.executable = None;
    }
    assert_ne!(changed.canonical_fingerprint(), ssh_hash);
    Ok(())
}

#[test]
fn target_definition_rejects_each_unsafe_ssh_ingress_and_runtime_field() {
    let mutations: [fn(&mut TargetDefinition); 16] = [
        |value| {
            if let TargetTransport::Ssh { port, .. } = &mut value.transport {
                *port = 0
            }
        },
        |value| {
            if let TargetTransport::Ssh { host, .. } = &mut value.transport {
                *host = "-bad".to_owned()
            }
        },
        |value| {
            if let TargetTransport::Ssh { user, .. } = &mut value.transport {
                *user = "bad user".to_owned()
            }
        },
        |value| {
            if let TargetTransport::Ssh {
                host_key_fingerprint,
                ..
            } = &mut value.transport
            {
                *host_key_fingerprint = "bad".to_owned()
            }
        },
        |value| {
            if let TargetTransport::Ssh { host_key, .. } = &mut value.transport {
                *host_key = Some("ssh-dss AAAA".to_owned())
            }
        },
        |value| {
            if let TargetTransport::Ssh { identity_file, .. } = &mut value.transport {
                *identity_file = Some(PathBuf::from("relative"))
            }
        },
        |value| {
            if let TargetTransport::Ssh { remote_root, .. } = &mut value.transport {
                *remote_root = Some(PathBuf::from("relative"))
            }
        },
        |value| {
            if let TargetTransport::Ssh {
                daemon_fingerprint, ..
            } = &mut value.transport
            {
                *daemon_fingerprint = Some("A".repeat(64))
            }
        },
        |value| {
            if let Some(TargetIngress::Traefik { image, .. }) = &mut value.ingress {
                *image = "traefik:latest".to_owned()
            }
        },
        |value| {
            if let Some(TargetIngress::Traefik { acme_email, .. }) = &mut value.ingress {
                *acme_email = Some("bad@@example".to_owned())
            }
        },
        |value| value.available_runtimes = vec![crate::domain::runtime::RuntimeKind::PodmanCompose],
        |value| {
            if let Some(runtime) = value.runtime.as_mut() {
                runtime.socket = Some(PathBuf::from("relative"));
            }
        },
        |value| {
            if let Some(runtime) = value.runtime.as_mut() {
                runtime.endpoint = Some(String::new());
            }
        },
        |value| {
            if let Some(runtime) = value.runtime.as_mut() {
                runtime.endpoint = Some("bad\nendpoint".to_owned());
            }
        },
        |value| {
            if let Some(runtime) = value.runtime.as_mut() {
                runtime.executable = Some(PathBuf::new());
            }
        },
        |value| {
            if let TargetTransport::Ssh { host, .. } = &mut value.transport {
                *host = "bad..host".to_owned()
            }
        },
    ];
    for mutate in mutations {
        let mut candidate = ssh_definition();
        mutate(&mut candidate);
        assert!(validate_target_definition(&candidate).is_err());
    }
}

#[test]
fn target_catalog_names_ids_and_capability_labels_are_exact() {
    for name in ["local", "target_1", "1-remote"] {
        assert!(validate_target_name(name).is_ok(), "{name}");
    }
    for name in ["", "Upper", "-target", "target space", &"x".repeat(64)] {
        assert!(validate_target_name(name).is_err(), "{name}");
    }
    let id = TargetId(Uuid::from_u128(1));
    let mut catalog = TargetCatalog {
        targets: BTreeMap::from([
            ("local".to_owned(), TargetDefinition::local(id)),
            ("duplicate".to_owned(), TargetDefinition::local(id)),
        ]),
    };
    assert!(validate_target_catalog(&catalog).is_err());
    catalog.targets.remove("duplicate");
    assert!(validate_target_catalog(&catalog).is_ok());
    catalog.targets.insert(
        "Bad".to_owned(),
        TargetDefinition::local(TargetId(Uuid::from_u128(2))),
    );
    assert!(validate_target_catalog(&catalog).is_err());

    let labels = [
        BootstrapCapability::AnsiblePlaybook,
        BootstrapCapability::SshTransport,
        BootstrapCapability::Python3,
        BootstrapCapability::Docker,
        BootstrapCapability::Podman,
        BootstrapCapability::ComposeProvider,
        BootstrapCapability::Haproxy,
        BootstrapCapability::RemoteAccess,
    ]
    .map(BootstrapCapability::label);
    assert_eq!(labels.iter().collect::<BTreeSet<_>>().len(), labels.len());
    assert_eq!(BootstrapStep::ProvisionHost.label(), "provision-host");
    assert_eq!(BootstrapStep::EnrollRuntime.label(), "enroll-runtime");
    assert_eq!(BootstrapStep::InstallAccess.label(), "install-access");
}

fn bootstrap_plan() -> Result<TargetBootstrapPlan, String> {
    let definition = TargetDefinition::local(TargetId(Uuid::from_u128(7)));
    let target = NamedTarget::new("local".to_owned(), definition);
    let mut plan = TargetBootstrapPlan {
        project_id: crate::domain::workspace::ProjectId(Uuid::from_u128(8)),
        project_name: "project".to_owned(),
        config_hash: "a".repeat(64),
        environment: "development".to_owned(),
        target,
        selected_runtime: crate::domain::runtime::RuntimeKind::DockerCompose,
        rootless: false,
        input_files: vec![
            BootstrapInputFile {
                path: PathBuf::from(".config/ansible/ansible.cfg"),
                digest: "b".repeat(64),
            },
            BootstrapInputFile {
                path: PathBuf::from(".config/ansible/bootstrap.yml"),
                digest: "c".repeat(64),
            },
        ],
        required_capabilities: vec![
            BootstrapCapability::AnsiblePlaybook,
            BootstrapCapability::Python3,
            BootstrapCapability::Docker,
            BootstrapCapability::ComposeProvider,
            BootstrapCapability::Haproxy,
        ],
        steps: vec![BootstrapStep::ProvisionHost, BootstrapStep::EnrollRuntime],
        access: None,
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.canonical_hash()?;
    Ok(plan)
}

#[test]
fn bootstrap_plan_rejects_each_noncanonical_manifest_and_runtime_mode() -> Result<(), String> {
    let plan = bootstrap_plan()?;
    plan.validate()?;

    let mut stale = plan.clone();
    stale.plan_hash = "stale".to_owned();
    assert!(stale.validate().is_err());

    let mutations: [fn(&mut TargetBootstrapPlan); 9] = [
        |value| value.project_name.clear(),
        |value| value.config_hash = "A".repeat(64),
        |value| value.target.fingerprint = "stale".to_owned(),
        |value| value.input_files = vec![value.input_files[0].clone(); 257],
        |value| value.input_files[0].path = PathBuf::from("../ansible.cfg"),
        |value| value.input_files.swap(0, 1),
        |value| value.input_files.pop().map_or((), drop),
        |value| value.required_capabilities.pop().map_or((), drop),
        |value| value.steps.reverse(),
    ];
    for mutate in mutations {
        let mut candidate = plan.clone();
        mutate(&mut candidate);
        assert!(candidate.canonical_hash().is_err());
    }

    let mut docker_rootless = plan.clone();
    docker_rootless.rootless = true;
    assert!(docker_rootless.canonical_hash().is_err());

    let mut podman = plan;
    podman.selected_runtime = crate::domain::runtime::RuntimeKind::PodmanCompose;
    podman.rootless = true;
    podman.required_capabilities[2] = BootstrapCapability::Podman;
    assert!(podman.canonical_hash().is_ok());
    podman.rootless = false;
    assert!(podman.canonical_hash().is_err());
    Ok(())
}

const OPS_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w ops@offline";
const GITHUB_HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

fn access_plan() -> Result<TargetBootstrapPlan, String> {
    let mut plan = bootstrap_plan()?;
    let requirements = bootstrap_requirements(
        &plan.target.definition.transport,
        plan.selected_runtime,
        true,
        true,
    )?;
    plan.required_capabilities = requirements.capabilities;
    plan.steps = requirements.steps;
    plan.access = Some(BootstrapAccess {
        origin: "git@github.com:acme/shop.git".to_owned(),
        origin_host_key: Some(GITHUB_HOST_KEY.to_owned()),
        break_glass: vec![crate::domain::host_policy::BreakGlassKey {
            name: "ops".to_owned(),
            key: OPS_KEY.to_owned(),
        }],
        binaries: PathBuf::from("/opt/mm release"),
        mm_digest: "d".repeat(64),
        mmd_digest: "e".repeat(64),
        upgrade: false,
        downgrade: false,
        notices: Vec::new(),
    });
    plan.plan_hash = plan.canonical_hash()?;
    Ok(plan)
}

#[test]
fn bootstrap_access_binds_every_reviewed_input() -> Result<(), String> {
    let plan = access_plan()?;
    plan.validate()?;
    assert_ne!(plan.plan_hash, bootstrap_plan()?.plan_hash);
    assert_eq!(plan.steps.last(), Some(&BootstrapStep::InstallAccess));
    assert_eq!(
        plan.required_capabilities.last(),
        Some(&BootstrapCapability::RemoteAccess)
    );
    let mutations: [fn(&mut BootstrapAccess); 5] = [
        |access| access.origin = "ssh://git@github.com/acme/shop.git".to_owned(),
        |access| access.break_glass[0].name = "recovery".to_owned(),
        |access| access.binaries = PathBuf::from("/opt/other"),
        |access| access.mm_digest = "f".repeat(64),
        |access| access.mmd_digest = "f".repeat(64),
    ];
    for mutate in mutations {
        let mut candidate = plan.clone();
        if let Some(access) = candidate.access.as_mut() {
            mutate(access);
        }
        assert_ne!(candidate.canonical_hash()?, plan.plan_hash);
    }
    // The access step and its capability travel together with the access.
    let mut missing_step = plan.clone();
    missing_step.steps.pop();
    assert!(missing_step.canonical_hash().is_err());
    let mut missing_access = plan.clone();
    missing_access.access = None;
    assert!(missing_access.canonical_hash().is_err());
    Ok(())
}

#[test]
fn bootstrap_access_rejects_unsafe_inputs() -> Result<(), String> {
    let plan = access_plan()?;
    let mutations: [fn(&mut BootstrapAccess); 8] = [
        |access| access.origin_host_key = None,
        |access| access.origin = "http://example.com/shop.git".to_owned(),
        |access| access.break_glass.clear(),
        |access| access.break_glass.push(access.break_glass[0].clone()),
        |access| access.break_glass[0].key = format!("{OPS_KEY}-{{{{ x }}}}"),
        |access| access.binaries = PathBuf::from("relative"),
        |access| access.mm_digest = "D".repeat(64),
        |access| access.mmd_digest = "e".repeat(63),
    ];
    for mutate in mutations {
        let mut candidate = plan.clone();
        if let Some(access) = candidate.access.as_mut() {
            mutate(access);
        }
        assert!(candidate.canonical_hash().is_err());
    }
    // mmd runs as root, so the access layer goes with rootful Podman.
    let podman = bootstrap_requirements(
        &TargetTransport::Local,
        crate::domain::runtime::RuntimeKind::PodmanCompose,
        false,
        true,
    )?;
    assert!(!podman.rootless);
    assert!(podman.capabilities.contains(&BootstrapCapability::Podman));
    Ok(())
}

#[test]
fn bootstrap_access_apply_command_repeats_the_flags_shell_quoted() -> Result<(), String> {
    let plan = access_plan()?;
    let command = plan.apply_command();
    assert!(command.starts_with(&format!(
        "mm --environment development --target local --runtime docker-compose target setup apply --plan {}",
        plan.plan_hash
    )));
    assert!(command.contains(" --origin git@github.com:acme/shop.git "));
    assert!(command.contains(&format!(" --origin-host-key '{GITHUB_HOST_KEY}' ")));
    assert!(command.contains(&format!(" --break-glass 'ops={OPS_KEY}' ")));
    assert!(command.ends_with(" --host-binaries '/opt/mm release'"));
    let mut quoted = plan;
    if let Some(access) = quoted.access.as_mut() {
        access.binaries = PathBuf::from("/opt/it's");
    }
    assert!(
        quoted
            .apply_command()
            .ends_with(r#" --host-binaries '/opt/it'\''s'"#)
    );
    Ok(())
}

#[test]
fn bootstrap_access_reports_are_checked() {
    let report = BootstrapAccessReport {
        version: "0.1.0".to_owned(),
        host_key: GITHUB_HOST_KEY.to_owned(),
        daemon_fingerprint: "a".repeat(64),
        deploy_key: OPS_KEY.to_owned(),
    };
    assert!(report.validate().is_ok());
    let mutations: [fn(&mut BootstrapAccessReport); 5] = [
        |report| report.version = "0.1.0; rm".to_owned(),
        |report| report.host_key = format!("{GITHUB_HOST_KEY} root@host"),
        |report| report.host_key = "ssh-ed25519 !!!".to_owned(),
        |report| report.daemon_fingerprint = "A".repeat(64),
        |report| report.deploy_key = "ssh-rsa AAAAB3NzaC1yc2E".to_owned(),
    ];
    for mutate in mutations {
        let mut candidate = report.clone();
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
}

#[test]
fn bootstrap_paths_catalog_limits_and_ssh_defaults_are_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = TargetCatalog::default();
    for index in 0..257_u128 {
        catalog.targets.insert(
            format!("t{index}"),
            TargetDefinition::local(TargetId(Uuid::from_u128(index + 1))),
        );
    }
    assert!(validate_target_catalog(&catalog).is_err());

    let transport: TargetTransport = serde_json::from_value(serde_json::json!({
        "kind": "ssh",
        "host": "127.0.0.1",
        "user": "operator",
        "host_key_fingerprint": format!("SHA256:{}", "a".repeat(43))
    }))?;
    assert!(matches!(transport, TargetTransport::Ssh { port: 22, .. }));

    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let mut plan = bootstrap_plan()?;
        plan.input_files[0].path = PathBuf::from(OsString::from_vec(vec![b'.', 0xff]));
        assert!(plan.canonical_hash().is_err());
    }
    Ok(())
}

fn wire_blob(fields: &[&[u8]]) -> Vec<u8> {
    let mut blob = Vec::new();
    for field in fields {
        blob.extend_from_slice(&(field.len() as u32).to_be_bytes());
        blob.extend_from_slice(field);
    }
    blob
}

#[test]
fn ssh_wire_keys_cover_supported_algorithms_truncation_and_mismatch() {
    let point = [&[4_u8][..], &[0_u8; 64][..]].concat();
    let ecdsa = wire_blob(&[b"ecdsa-sha2-nistp256", b"nistp256", &point]);
    assert!(validate_ssh_key_blob("ecdsa-sha2-nistp256", &ecdsa).is_ok());
    let rsa = wire_blob(&[b"ssh-rsa", &[1, 0, 1], &[7_u8; 128]]);
    assert!(validate_ssh_key_blob("ssh-rsa", &rsa).is_ok());
    assert!(validate_ssh_key_blob("unknown", &wire_blob(&[b"unknown"])).is_err());
    assert!(validate_ssh_key_blob("ssh-ed25519", &wire_blob(&[b"ssh-ed25519", &[0; 31]])).is_err());
    assert!(take_ssh_wire_string(&mut &[0_u8; 3][..]).is_err());
    assert!(take_ssh_wire_string(&mut &[0, 0, 0, 4, 1][..]).is_err());

    assert!(validate_ssh_host_key(&"x".repeat(16 * 1024 + 1), "irrelevant").is_err());
    assert!(validate_ssh_host_key("ssh-ed25519 !!!", "irrelevant").is_err());
    assert!(validate_ssh_host_key("ssh-ed25519 ", "irrelevant").is_err());
    let definition = ssh_definition();
    let host_key = match definition.transport {
        TargetTransport::Ssh { host_key, .. } => host_key,
        TargetTransport::Local => None,
    };
    assert!(host_key.is_some());
    if let Some(host_key) = host_key {
        assert!(validate_ssh_host_key(&host_key, &format!("SHA256:{}", "z".repeat(43))).is_err());
    }
}

#[test]
fn target_network_and_ingress_lexical_boundaries_are_explicit() {
    let mut definition = ssh_definition();
    if let TargetTransport::Ssh {
        host,
        host_key_fingerprint,
        host_key,
        ..
    } = &mut definition.transport
    {
        *host = "127.0.0.1".to_owned();
        *host_key_fingerprint = format!("SHA256:{}", "a".repeat(42));
        *host_key = None;
    }
    assert!(validate_target_definition(&definition).is_err());

    if let TargetTransport::Ssh {
        host_key_fingerprint,
        ..
    } = &mut definition.transport
    {
        *host_key_fingerprint = format!("SHA256:{}", "a".repeat(43));
    }
    assert!(validate_target_definition(&definition).is_ok());

    if let Some(TargetIngress::Traefik { image, .. }) = &mut definition.ingress {
        *image = format!("bad repo@sha256:{}", "a".repeat(64));
    }
    assert!(validate_target_definition(&definition).is_err());
    if let Some(TargetIngress::Traefik { image, .. }) = &mut definition.ingress {
        *image = format!("bad@repo@sha256:{}", "a".repeat(64));
    }
    assert!(validate_target_definition(&definition).is_err());
    if let Some(TargetIngress::Traefik { image, .. }) = &mut definition.ingress {
        *image = format!("BadRepo@sha256:{}", "a".repeat(64));
    }
    assert!(validate_target_definition(&definition).is_ok());
}

#[test]
fn acme_ca_settings_are_validated_and_only_hashed_when_set() {
    let base = ssh_definition();
    let fingerprint = base.canonical_fingerprint();
    let mut configured = base.clone();
    if let Some(TargetIngress::Traefik {
        acme_ca_server,
        acme_ca_certificate,
        ..
    }) = configured.ingress.as_mut()
    {
        *acme_ca_server = Some("https://acme-staging-v02.api.letsencrypt.org/directory".to_owned());
        *acme_ca_certificate =
            Some("-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n".to_owned());
    }
    assert!(validate_target_definition(&configured).is_ok());
    assert_ne!(configured.canonical_fingerprint(), fingerprint);
    assert_eq!(base.canonical_fingerprint(), fingerprint);
    assert!(validate_acme_ca_server("http://insecure.example/dir").is_err());
    assert!(validate_acme_ca_server("https://").is_err());
    assert!(
        validate_acme_ca_certificate(
            "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----\n-----BEGIN PRIVATE KEY-----"
        )
        .is_err()
    );
    assert!(validate_acme_ca_certificate("not pem").is_err());
}
