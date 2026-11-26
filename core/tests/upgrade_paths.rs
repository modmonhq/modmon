//! Which installations `mm target setup` upgrades in place, which it refuses,
//! and what its plan says for each, as docs/how-to/upgrade.md states them.

use std::net::Ipv4Addr;

use base64::Engine;
use mm::domain::cluster_setup::{HostRelease, ReleaseConsent, release_notices};
use mm::domain::mesh::{MeshFacts, MeshMember, MeshNodeOrigin, PeerAction, plan_node};
use mm::domain::remote::{Behind, PROTOCOL_VERSION, ProtocolRange, negotiate, protocol_refusal};
use mm::domain::repo_target::WireguardMesh;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const THIS: &str = env!("CARGO_PKG_VERSION");
const MESH_FACTS: &str = include_str!("../src/adapters/target/cluster_shell/mesh-facts.sh");

fn key(byte: u8) -> String {
    base64::engine::general_purpose::STANDARD.encode([byte; 32])
}

fn members() -> Vec<MeshMember> {
    [("app-1", 1), ("app-2", 2)]
        .into_iter()
        .map(|(name, last)| MeshMember {
            name: name.to_owned(),
            address: Ipv4Addr::new(10, 90, 0, last),
            endpoint: format!("203.0.113.{}", 10 + last),
            public_key: Some(key(last)),
            routed: Vec::new(),
        })
        .collect()
}

fn release(version: &str, protocol: u16, build: Option<char>) -> HostRelease {
    HostRelease {
        version: version.to_owned(),
        protocol,
        build: build.map(|digit| digit.to_string().repeat(64)),
    }
}

fn this() -> HostRelease {
    release(THIS, PROTOCOL_VERSION, Some('a'))
}

const ASKED: ReleaseConsent = ReleaseConsent {
    downgrade: false,
    upgrade: true,
};

fn mesh_show(release: &str) -> String {
    format!(
        r#"{{"public-key":"{}","state":{{"interface":"mm0","address":"10.90.0.1","prefix":24,"port":51820,"mtu":1420,"peers":[{{"name":"app-2","public-key":"{}","endpoint":"203.0.113.12:51820","allowed-ips":["10.90.0.2/32"]}}]}}{release}}}"#,
        key(1),
        key(2)
    )
}

#[test]
fn a_node_whose_mmd_reports_no_release_is_replaced_without_a_flag() -> TestResult {
    assert!(
        MESH_FACTS.contains("printf '{}\\n'"),
        "a node without mmd prints no facts"
    );
    assert!(
        MESH_FACTS.contains(r#"printf '{"public-key":"%s","legacy-interface":"wg0"}\n'"#),
        "a node with the wg-quick mesh of an earlier mm prints its key"
    );
    let legacy = format!(r#"{{"public-key":"{}","legacy-interface":"wg0"}}"#, key(1));
    let members = members();
    for (era, printed, origin) in [
        ("no mmd", "{}".to_owned(), MeshNodeOrigin::New),
        (
            "an mmd without mesh commands",
            legacy,
            MeshNodeOrigin::Legacy,
        ),
        (
            "an mmd from before nodes reported their release",
            mesh_show(""),
            MeshNodeOrigin::Current,
        ),
    ] {
        let facts: MeshFacts = serde_json::from_str(&printed)?;
        let planned = plan_node(
            &WireguardMesh::default(),
            &members,
            &members[0],
            &[],
            Some(&facts),
        );
        assert_eq!(planned.origin, origin, "{era}");
        assert_eq!(planned.release, None, "{era}");
        assert_eq!(
            release_notices(
                &[("app-1", planned.release.as_ref())],
                &this(),
                ReleaseConsent::default()
            )?,
            [format!(
                "app-1: installs mm and mmd {}, in place of the earlier release the node may run",
                this()
            )],
            "{era}"
        );
        if origin == MeshNodeOrigin::Current {
            assert_eq!(
                planned
                    .peers
                    .iter()
                    .map(|peer| peer.action)
                    .collect::<Vec<_>>(),
                [PeerAction::Keep],
                "the mesh a node holds is kept"
            );
        }
    }
    Ok(())
}

#[test]
fn a_node_on_an_earlier_protocol_is_replaced_only_when_asked() -> TestResult {
    let members = members();
    for (era, reported, shown) in [
        (
            "protocol 16",
            r#","release":{"version":"0.1.0","protocol":16}"#.to_owned(),
            release("0.1.0", 16, None),
        ),
        (
            "protocol 17 to 19",
            format!(
                r#","release":{{"version":"0.1.0","protocol":19,"build":"{}"}}"#,
                "b".repeat(64)
            ),
            release("0.1.0", 19, Some('b')),
        ),
    ] {
        let facts: MeshFacts = serde_json::from_str(&mesh_show(&reported))?;
        let planned = plan_node(
            &WireguardMesh::default(),
            &members,
            &members[0],
            &[],
            Some(&facts),
        );
        assert_eq!(planned.release.as_ref(), Some(&shown), "{era}");
        let nodes = [("app-1", planned.release.as_ref())];
        let refused = release_notices(&nodes, &this(), ReleaseConsent::default())
            .err()
            .ok_or(format!("{era}: replaced without --upgrade"))?;
        for part in [
            format!("app-1 runs mm {shown}, and this mm is {}", this()),
            "pass --upgrade, or pin this release with deploy.mm-version".to_owned(),
        ] {
            assert!(refused.contains(&part), "{era}: {part}: {refused}");
        }
        assert_eq!(
            release_notices(&nodes, &this(), ASKED)?,
            [format!(
                "app-1: replaces mm and mmd {shown} with {}",
                this()
            )],
            "{era}"
        );
    }
    Ok(())
}

#[test]
fn every_earlier_protocol_is_older_and_a_later_one_is_refused() -> TestResult {
    for protocol in 1..=PROTOCOL_VERSION {
        let earlier = release("0.1.0", protocol, Some('b'));
        assert!(!earlier.is_newer_than(&this()), "{earlier}");
        assert!(!earlier.same_build(&this()), "{earlier}");
        assert!(release_notices(&[("app-1", Some(&earlier))], &this(), ASKED).is_ok());
    }
    for newer in [
        release(THIS, PROTOCOL_VERSION + 1, Some('b')),
        release("99.0.0", PROTOCOL_VERSION, Some('b')),
    ] {
        let nodes = [("app-1", Some(&newer))];
        for consent in [ReleaseConsent::default(), ASKED] {
            let refused = release_notices(&nodes, &this(), consent)
                .err()
                .ok_or(format!("{newer} was replaced without --downgrade"))?;
            for part in [
                format!("app-1 runs mm {newer}, newer than this mm {}", this()),
                format!("Install mm {} or later on this machine", newer.version),
                "pass --downgrade".to_owned(),
            ] {
                assert!(refused.contains(&part), "{part}: {refused}");
            }
        }
        assert_eq!(
            release_notices(
                &nodes,
                &this(),
                ReleaseConsent {
                    downgrade: true,
                    upgrade: false
                }
            )?,
            [format!(
                "app-1: downgrades mm and mmd from {newer} to {} (--downgrade)",
                this()
            )]
        );
    }
    Ok(())
}

#[test]
fn the_same_build_is_installed_again_without_a_flag() -> TestResult {
    for same in [this(), release(THIS, PROTOCOL_VERSION, None)] {
        assert_eq!(
            release_notices(
                &[("app-1", Some(&same))],
                &this(),
                ReleaseConsent::default()
            )?,
            [format!(
                "app-1 runs this release of mm, {}; setup installs its host binaries again",
                this()
            )]
        );
    }
    Ok(())
}

#[test]
fn a_client_and_a_host_on_different_protocols_are_told_which_side_to_upgrade() {
    let host = ProtocolRange::THIS;
    let range = |minimum: u16, maximum: u16| ProtocolRange { minimum, maximum };
    assert_eq!(
        protocol_refusal(("", None), (THIS, host)),
        format!(
            "This mm is from before 0.1.1, and the host's mmd {THIS} speaks remote protocol {host}. Update mm on this machine to {THIS} or newer with the installer, https://modmon.dev/install.sh"
        )
    );
    let later_host = range(PROTOCOL_VERSION + 1, PROTOCOL_VERSION + 2);
    let behind = protocol_refusal((THIS, Some(host)), ("0.3.0", later_host));
    assert!(
        behind.contains(&format!(
            "This mm {THIS} speaks remote protocol {host}, and the host's mmd 0.3.0 speaks {later_host}. Update mm on this machine to 0.3.0 or newer: run `mm update`"
        )),
        "{behind}"
    );
    let newer_client = range(PROTOCOL_VERSION + 1, PROTOCOL_VERSION + 3);
    let older_host = protocol_refusal(("9.0.0", Some(newer_client)), (THIS, host));
    assert!(
        older_host.contains(&format!(
            "This mm 9.0.0 speaks remote protocol {newer_client}, and the host's mmd {THIS} speaks {host}. Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade"
        )),
        "{older_host}"
    );
    assert_eq!(negotiate(newer_client, host), Err(Behind::Daemon));
    assert_eq!(negotiate(host, later_host), Err(Behind::Client));
}

#[test]
fn a_host_from_before_0_1_1_is_older_whatever_protocol_it_reports() -> TestResult {
    for protocol in 16..=19 {
        let before = release("0.1.0", protocol, Some('b'));
        assert!(!before.is_newer_than(&this()), "{before}");
        assert!(this().is_newer_than(&before), "{before}");
        let nodes = [("app-1", Some(&before))];
        let refused = release_notices(&nodes, &this(), ReleaseConsent::default())
            .err()
            .ok_or(format!("{before} was replaced without --upgrade"))?;
        assert!(refused.contains("pass --upgrade"), "{refused}");
        assert!(!refused.contains("--downgrade"), "{refused}");
        assert_eq!(
            release_notices(&nodes, &this(), ASKED)?,
            [format!(
                "app-1: replaces mm and mmd {before} with {}",
                this()
            )]
        );
    }
    Ok(())
}

struct Runs(Option<HostRelease>);

#[async_trait::async_trait]
impl mm::app::ports::HostReleaseReader for Runs {
    async fn host_facts(
        &self,
        _: &mm::domain::target::TargetTransport,
    ) -> Result<mm::domain::target::HostFacts, mm::MmError> {
        Ok(mm::domain::target::HostFacts {
            release: self.0.clone(),
            cpu: Some("x86_64".to_owned()),
        })
    }
}

struct SingleHost {
    workspace: tempfile::TempDir,
    selection: mm::app::WorkspaceSelection,
    targets: std::sync::Arc<dyn mm::app::ports::TargetStore>,
}

impl SingleHost {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        use mm::adapters::config::ConfigLoader;
        let workspace = temp_dirs::tempdir()?;
        let root = workspace.path().join("project");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("compose.yaml"), "services: {}\n")?;
        ConfigLoader.initialize(&root, Some("single-host"))?;
        let targets: std::sync::Arc<dyn mm::app::ports::TargetStore> = std::sync::Arc::new(
            mm::adapters::target::FilesystemTargetStore::new(workspace.path().join("targets.yaml")),
        );
        mm::app::TargetManager::new(
            std::sync::Arc::new(ConfigLoader),
            std::sync::Arc::clone(&targets),
        )
        .add_ssh(
            "production",
            mm::app::SshTargetInput {
                host: "203.0.113.20".to_owned(),
                port: 22,
                user: "deploy".to_owned(),
                host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"
                    .to_owned(),
                host_key: None,
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: None,
            },
        )?;
        let binaries = workspace.path().join("release");
        std::fs::create_dir(&binaries)?;
        std::fs::write(binaries.join("mm"), linux_binary(b"mm of this release"))?;
        std::fs::write(binaries.join("mmd"), linux_binary(b"mmd of this release"))?;
        let mut selection = mm::app::WorkspaceSelection::new(&root, None);
        selection.target = Some("production".to_owned());
        Ok(Self {
            workspace,
            selection,
            targets,
        })
    }

    fn this(&self) -> HostRelease {
        HostRelease {
            version: THIS.to_owned(),
            protocol: PROTOCOL_VERSION,
            build: Some(
                blake3::hash(&linux_binary(b"mmd of this release"))
                    .to_hex()
                    .to_string(),
            ),
        }
    }

    async fn plan(
        &self,
        running: Option<HostRelease>,
        consent: ReleaseConsent,
    ) -> Result<mm::domain::target::TargetBootstrapPlan, mm::MmError> {
        let request = mm::app::BootstrapAccessRequest {
            origin: String::new(),
            origin_host_key: None,
            break_glass: vec![
                mm::domain::host_policy::parse_break_glass(
                    "ops=ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w ops@offline",
                )
                .map_err(|message| mm::MmError::Target { message })?,
            ],
            binaries: Some(self.workspace.path().join("release")),
            upgrade: consent.upgrade,
            downgrade: consent.downgrade,
        };
        mm::app::TargetManager::new(
            std::sync::Arc::new(mm::adapters::config::ConfigLoader),
            std::sync::Arc::clone(&self.targets),
        )
        .with_host_releases(std::sync::Arc::new(Runs(running)))
        .bootstrap_plan_on_host(&self.selection, None, Some(&request))
        .await
    }
}

fn notices(plan: &mm::domain::target::TargetBootstrapPlan) -> Vec<String> {
    plan.access
        .as_ref()
        .map(|access| access.notices.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_target_without_nodes_reads_its_host_release_as_a_cluster_reads_each_node() -> TestResult
{
    let host = SingleHost::new()?;
    let this = host.this();
    const DOWN: ReleaseConsent = ReleaseConsent {
        downgrade: true,
        upgrade: false,
    };

    let fresh = host.plan(None, ReleaseConsent::default()).await?;
    assert_eq!(
        notices(&fresh),
        [format!(
            "production: installs mm and mmd {this}, in place of the earlier release the node may run"
        )]
    );

    let same = host
        .plan(Some(this.clone()), ReleaseConsent::default())
        .await?;
    assert_eq!(
        notices(&same),
        [format!(
            "production runs this release of mm, {this}; setup installs its host binaries again"
        )]
    );
    assert_ne!(same.plan_hash, fresh.plan_hash);
    assert!(!same.apply_command().contains("grade"));

    for earlier in [
        release("0.1.0", 19, Some('b')),
        release(THIS, PROTOCOL_VERSION, Some('b')),
    ] {
        let refused = host
            .plan(Some(earlier.clone()), ReleaseConsent::default())
            .await
            .err()
            .ok_or("an older or other build was replaced without a flag")?
            .to_string();
        assert!(refused.contains("pass --upgrade"), "{refused}");
        let upgrade = host.plan(Some(earlier.clone()), ASKED).await?;
        assert_eq!(
            notices(&upgrade),
            [format!(
                "production: replaces mm and mmd {earlier} with {this}"
            )]
        );
        assert!(upgrade.apply_command().ends_with(" --upgrade"));
        upgrade.validate()?;
    }

    let newer = release("9.0.0", PROTOCOL_VERSION + 1, Some('c'));
    for consent in [ReleaseConsent::default(), ASKED] {
        let refused = host
            .plan(Some(newer.clone()), consent)
            .await
            .err()
            .ok_or("a newer release was replaced without --downgrade")?
            .to_string();
        assert!(refused.contains("pass --downgrade"), "{refused}");
    }
    let downgrade = host.plan(Some(newer.clone()), DOWN).await?;
    assert_eq!(
        notices(&downgrade),
        [format!(
            "production: downgrades mm and mmd from {newer} to {this} (--downgrade)"
        )]
    );
    assert!(downgrade.apply_command().ends_with(" --downgrade"));
    Ok(())
}

#[test]
fn a_setup_command_carries_the_release_flags_to_plan_and_apply() {
    use mm::domain::target::SetupCommandLine;
    let plain = SetupCommandLine::new("uat");
    assert_eq!(plain.plan(), "mm --environment uat target setup plan");
    assert_eq!(
        plain.apply("97d8abb4ef35"),
        "mm --environment uat target setup apply --plan 97d8abb4ef35"
    );
    let upgrade = plain.with("--nodes", "app-1,app-2").with_release(ASKED);
    assert_eq!(
        upgrade.plan(),
        "mm --environment uat target setup plan --nodes app-1,app-2 --upgrade"
    );
    assert_eq!(
        upgrade.apply("97d8abb4ef35"),
        "mm --environment uat target setup apply --plan 97d8abb4ef35 --nodes app-1,app-2 --upgrade"
    );
}

/// `payload` behind the ELF header of an x86-64 Linux executable.
fn linux_binary(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
    bytes.resize(18, 0);
    bytes.extend_from_slice(&62_u16.to_le_bytes());
    bytes.resize(64, 0);
    bytes.extend_from_slice(payload);
    bytes
}
