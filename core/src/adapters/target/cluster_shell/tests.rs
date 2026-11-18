use super::*;

#[test]
fn programs_are_quoted_and_raised_for_non_root_accounts() {
    let command = ClusterNodeCommand::Program(vec![
        "/usr/libexec/mmd".to_owned(),
        "secrets".to_owned(),
        "it's".to_owned(),
    ]);
    assert_eq!(
        remote_command("root", &command),
        "'/usr/libexec/mmd' 'secrets' 'it'\"'\"'s'"
    );
    assert!(remote_command("deploy", &command).starts_with("sudo -n '/usr/libexec/mmd'"));
}

#[test]
fn scripts_run_through_sh_with_their_arguments() {
    let command = ClusterNodeCommand::Script {
        script: ClusterScript::Reach,
        arguments: vec!["1420".to_owned(), "10.77.0.2".to_owned()],
    };
    let line = remote_command("root", &command);
    assert!(line.starts_with("'/bin/sh' '-c' '# Check that every peer"));
    assert!(line.ends_with("'mm-setup' '1420' '10.77.0.2'"));
}

#[test]
fn known_hosts_lines_bracket_non_default_ports() {
    assert_eq!(
        known_hosts_line("203.0.113.1", 22, "ssh-ed25519 AAAA "),
        "203.0.113.1 ssh-ed25519 AAAA\n"
    );
    assert_eq!(
        known_hosts_line("node.test", 2222, "ssh-ed25519 AAAA"),
        "[node.test]:2222 ssh-ed25519 AAAA\n"
    );
}

#[derive(Default)]
struct RecordingProcess {
    arguments: std::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl ProcessRunner for RecordingProcess {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        if let Ok(mut arguments) = self.arguments.lock() {
            *arguments = specification.arguments().map(str::to_owned).collect();
        }
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new("ok\n".to_owned()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

#[tokio::test]
async fn ssh_pins_the_host_key_and_ignores_user_configuration()
-> Result<(), Box<dyn std::error::Error>> {
    let process = Arc::new(RecordingProcess::default());
    let shell = SshClusterNodeShell::new(process.clone(), ManagedKeys::none());
    let node = ClusterNodeConnection {
        name: "app-1".to_owned(),
        host: "203.0.113.1".to_owned(),
        port: 22,
        user: "root".to_owned(),
        host_key: "ssh-ed25519 AAAA".to_owned(),
        identity_file: std::path::PathBuf::from("/home/op/.mm/identities/ssh/id_ed25519"),
    };
    shell
        .run(
            &node,
            &ClusterNodeCommand::Program(vec!["true".to_owned()]),
            None,
        )
        .await?;
    let arguments = process.arguments.lock().map_err(|_| "poisoned")?.clone();
    let joined = arguments.join(" ");
    assert!(
        joined.starts_with("-F /dev/null -o UserKnownHostsFile="),
        "{joined}"
    );
    for option in [
        "StrictHostKeyChecking=yes",
        "GlobalKnownHostsFile=/dev/null",
        "IdentitiesOnly=yes",
        "BatchMode=yes",
    ] {
        assert!(
            arguments.iter().any(|argument| argument == option),
            "{option}"
        );
    }
    assert_eq!(arguments.last().map(String::as_str), Some("'true'"));
    Ok(())
}

#[test]
fn no_setup_script_runs_docker_swarm() {
    for script in [
        ClusterScript::Prepare,
        ClusterScript::Mesh,
        ClusterScript::MeshFacts,
        ClusterScript::Reach,
        ClusterScript::Storage,
    ] {
        let line = remote_command(
            "root",
            &ClusterNodeCommand::Script {
                script,
                arguments: Vec::new(),
            },
        );
        assert!(
            !line.to_ascii_lowercase().contains("swarm"),
            "{script:?} mentions Docker Swarm"
        );
    }
}

#[test]
fn the_mesh_scripts_leave_the_interface_and_the_key_to_mmd() {
    // No script writes a WireGuard configuration or makes a key any more.
    for script in [PREPARE, MESH, MESH_FACTS, REACH] {
        for forbidden in [
            "wg genkey",
            "wg-quick up",
            "wg set",
            "PrivateKey",
            ">/etc/wireguard",
        ] {
            assert!(!script.contains(forbidden), "{forbidden} in:\n{script}");
        }
    }
    assert!(MESH.contains("/usr/libexec/mmd mesh apply"), "{MESH}");
    assert!(
        MESH.contains("systemctl enable --quiet mm-mesh.service"),
        "{MESH}"
    );
    // The boot path runs once during setup, in the unit's sandbox.
    assert!(MESH.contains("systemctl reload mm-mesh.service"), "{MESH}");
    assert!(MESH.contains("systemctl start mm-mesh.service"), "{MESH}");
}

#[test]
fn a_host_with_the_earlier_wg_quick_mesh_hands_it_over() {
    // Only a configuration mm wrote is touched, and its unit is stopped
    // before mmd takes the port and the address.
    let marker = "grep -q '^# Managed by mm target setup'";
    assert!(MESH.contains(marker), "{MESH}");
    assert!(MESH_FACTS.contains(marker), "{MESH_FACTS}");
    let stop = MESH.find("systemctl disable --quiet --now wg-quick@wg0.service");
    let apply = MESH.find("/usr/libexec/mmd mesh apply");
    assert!(stop.is_some() && stop < apply, "{MESH}");
    // Before the new mmd is installed the plan still learns the node's
    // public key, computed on the node from the key that stays there.
    assert!(
        MESH_FACTS.contains("wg pubkey </etc/wireguard/wg0.key"),
        "{MESH_FACTS}"
    );
    assert!(
        MESH_FACTS.contains("exec /usr/libexec/mmd mesh show"),
        "{MESH_FACTS}"
    );
}

#[test]
fn the_reach_script_sends_one_packet_at_the_mesh_mtu() {
    // MTU PEER...: 28 bytes of IPv4 and ICMP headers make the packet MTU-sized.
    assert!(REACH.contains("mtu=$1"), "{REACH}");
    assert!(REACH.contains("-M do -s \"$((mtu - 28))\""), "{REACH}");
    assert!(REACH.contains("mm-mtu: %s"), "{REACH}");
    assert!(REACH.contains("mm-unreachable: %s"), "{REACH}");
}

#[test]
fn the_storage_script_listens_on_the_address_it_is_given() {
    // STORAGE_ROOT LISTEN_ADDRESS CLIENT...
    assert!(STORAGE.contains("root=$1 listen=$2"), "{STORAGE}");
    assert!(STORAGE.contains("host=%s"), "{STORAGE}");
    assert!(
        !STORAGE.contains("0.0.0.0"),
        "storage never listens on every address"
    );
}

#[test]
fn the_storage_script_leaves_no_helper_daemon_listening() {
    // rpc.mountd opens sockets on every address unless both old protocol
    // versions are off and it is restarted; rpcbind is not needed at all.
    assert!(STORAGE.contains("vers2=n\\nvers3=n"), "{STORAGE}");
    assert!(
        STORAGE.contains("systemctl restart nfs-mountd nfs-server"),
        "{STORAGE}"
    );
    assert!(
        STORAGE.contains("systemctl mask --quiet rpcbind.service rpcbind.socket"),
        "{STORAGE}"
    );
}

#[test]
fn the_prepare_script_rewrites_a_file_only_when_its_content_changes() {
    assert!(PREPARE.contains("cmp -s \"$1.tmp\" \"$1\""), "{PREPARE}");
    assert!(PREPARE.contains("| keep /etc/mm/node.env"), "{PREPARE}");
}
