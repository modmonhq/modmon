//! Connecting to a node: one connection per node, and what a failed
//! connection tells the user.

use super::{
    ClientAccess, ClientKey, ConnectionReuse, HandshakeRefusal, ManagedKeys, SshRemoteTransport,
    agreed, connect_failure,
};
use crate::domain::remote::{PROTOCOL_VERSION, RemoteError, ServerFrame, UPGRADE_REQUIRED_CODE};
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::{NamedTarget, TargetDefinition, TargetId, TargetTransport};

const KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn ssh_target() -> NamedTarget {
    NamedTarget::new(
        "remote".to_owned(),
        TargetDefinition {
            id: TargetId(uuid::Uuid::from_u128(1)),
            transport: TargetTransport::Ssh {
                host: "example.test".to_owned(),
                port: 2222,
                user: "relay".to_owned(),
                host_key_fingerprint: "SHA256:kmYcvdi2GkPeWxB6XLjrZB8JHsy2Hm8luHMFp9GMvqk"
                    .to_owned(),
                host_key: Some(KEY.to_owned()),
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: Some("a".repeat(64)),
            },
            bootstrap: crate::domain::target::TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: Some(RuntimeKind::DockerCompose),
            available_runtimes: vec![RuntimeKind::DockerCompose],
            runtime: None,
            nodes: Vec::new(),
        },
    )
}

fn node(name: &str, host: &str, recorded: bool) -> crate::domain::target::TargetNode {
    crate::domain::target::TargetNode {
        name: name.to_owned(),
        host: host.to_owned(),
        port: 22,
        user: "relay".to_owned(),
        private_address: None,
        host_key: recorded.then(|| KEY.to_owned()),
        host_key_fingerprint: recorded
            .then(|| "SHA256:kmYcvdi2GkPeWxB6XLjrZB8JHsy2Hm8luHMFp9GMvqk".to_owned()),
        daemon_fingerprint: recorded.then(|| "b".repeat(64)),
        identity_file: None,
    }
}

#[test]
fn each_node_gets_its_own_connection_and_is_named_in_messages()
-> Result<(), crate::domain::error::MmError> {
    let mut target = ssh_target();
    target.definition.nodes = vec![
        node("app-1", "10.0.0.1", true),
        node("app-2", "10.0.0.2", true),
        node("app-3", "10.0.0.3", false),
    ];
    let client_key = ClientKey::Default(None);
    let reuse = ConnectionReuse::off();
    let keys = ManagedKeys::none();
    let access = ClientAccess {
        key: &client_key,
        reuse: &reuse,
        keys: &keys,
    };
    let second = SshRemoteTransport::for_node(&target, &target.definition.nodes[1], None, access)?;
    assert_eq!(second.endpoint.host, "10.0.0.2");
    assert_eq!(second.endpoint.subject, "node 'app-2' (10.0.0.2:22)");
    assert_eq!(second.endpoint.daemon_fingerprint, "b".repeat(64));

    // A node that was never set up has no keys to connect with.
    let unrecorded =
        SshRemoteTransport::for_node(&target, &target.definition.nodes[2], None, access);
    assert!(
        matches!(&unrecorded, Err(crate::domain::error::MmError::Unreachable { subject, problem, help })
            if subject == "node 'app-3' (10.0.0.3:22)"
                && problem.contains("no recorded keys")
                && help.contains("mm target setup record")),
        "{unrecorded:?}"
    );

    target.definition.nodes[1].name = "app-1".to_owned();
    assert!(
        SshRemoteTransport::for_node(&target, &target.definition.nodes[0], None, access).is_err(),
        "duplicate node names"
    );
    Ok(())
}

/// `ssh` shares a connection only through mm's own socket, and only when
/// the transport keeps one; otherwise sharing stays switched off, whatever
/// the user's own `ssh` configuration says.
#[cfg(unix)]
#[test]
fn ssh_shares_a_connection_only_through_the_kept_socket() -> Result<(), Box<dyn std::error::Error>>
{
    use base64::Engine as _;

    let options = |transport: &SshRemoteTransport,
                   share: bool|
     -> Result<Vec<String>, crate::domain::error::MmError> {
        let (command, _) =
            transport
                .endpoint
                .ssh_command(std::path::Path::new("/known-hosts"), share, None)?;
        Ok(command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect())
    };
    let armor = |edge: &str| format!("-----{edge} OPENSSH {} KEY-----", "PRIVATE");
    let anew = options(&SshRemoteTransport::new(&ssh_target())?, true)?;
    // Without a key mm unlocks, ssh never asks for anything.
    assert!(anew.contains(&"BatchMode=yes".to_owned()));
    assert!(anew.contains(&"ControlMaster=no".to_owned()));
    assert!(anew.contains(&"ControlPath=none".to_owned()));

    // The layout of a key file, with no key in it.
    let field = |bytes: &[u8]| {
        let mut field = u32::try_from(bytes.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes()
            .to_vec();
        field.extend_from_slice(bytes);
        field
    };
    let public = [field(b"ssh-ed25519"), field(&[7; 32])].concat();
    let body = [
        b"openssh-key-v1\0".to_vec(),
        field(b"none"),
        field(b"none"),
        field(b""),
        1_u32.to_be_bytes().to_vec(),
        field(&public),
    ]
    .concat();
    let home = crate::test_support::tempdir()?;
    let key = home.path().join("device");
    std::fs::write(
        &key,
        format!(
            "{}\n{}\n{}\n",
            armor("BEGIN"),
            base64::engine::general_purpose::STANDARD.encode(body),
            armor("END")
        ),
    )?;
    let kept = SshRemoteTransport::for_target(
        &ssh_target(),
        ClientAccess {
            key: &ClientKey::Default(Some(key)),
            reuse: &ConnectionReuse::kept_under(&home.path().join("run")),
            keys: &ManagedKeys::none(),
        },
    )?;
    let socket = kept.kept_connection().ok_or("the connection is kept")?;
    let shared = options(&kept, true)?;
    assert!(shared.contains(&"ControlMaster=auto".to_owned()));
    assert!(shared.contains(&format!("ControlPath={}", socket.display())));
    assert!(shared.contains(&"ControlPersist=300".to_owned()));
    assert!(!shared.iter().any(|option| option == "ControlMaster=no"));
    // When ssh cannot share a connection here, mm connects as it did
    // before it kept any.
    let unshared = options(&kept, false)?;
    assert!(unshared.contains(&"ControlMaster=no".to_owned()));
    assert!(
        !unshared
            .iter()
            .any(|option| option.starts_with("ControlPersist"))
    );
    assert!(super::blames_sharing(
        "unix_listener: cannot bind to path /srv/mm-home/run/ssh/0a1b: Operation not supported\n"
    ));
    assert!(super::blames_sharing(
        "Control socket connect(/srv/mm-home/run/ssh/0a1b): Connection refused\n"
    ));
    assert!(super::blames_sharing(
        "control_persist_detach: fork: Resource temporarily unavailable\n"
    ));
    for host in [
        "ssh: connect to host 203.0.113.12 port 22: Connection timed out\n",
        "mm@203.0.113.12: Permission denied (publickey).\n",
        "Host key verification failed.\n",
    ] {
        assert!(!super::blames_sharing(host), "{host}");
    }
    // Everything else stays as strict as without sharing.
    for option in [
        "IdentitiesOnly=yes",
        "IdentityAgent=none",
        "StrictHostKeyChecking=yes",
    ] {
        assert!(shared.contains(&option.to_owned()), "{option}");
    }
    Ok(())
}

#[test]
fn a_failed_connection_says_what_happened_and_what_to_run() {
    let cases = [
        (
            "ssh: connect to host 203.0.113.12 port 22: Connection timed out\r\n",
            "did not answer (ssh: connect to host 203.0.113.12 port 22: Connection timed out)",
            "Check that the host is up",
        ),
        (
            "ssh: connect to host 203.0.113.12 port 22: Connection refused\n",
            "did not answer",
            "reach its SSH port",
        ),
        (
            "ssh: Could not resolve hostname app-2.example.test: Name or service not known\n",
            "did not answer",
            "Check that the host is up",
        ),
        (
            "error[MM_IO]: I/O error while inspecting relay daemon socket /run/mm/mm.sock: No such file or directory (os error 2)\n",
            "accepts SSH, but mmd is not running there",
            "systemctl start mmd",
        ),
        (
            "error[MM_IO]: I/O error while connecting restricted relay to /run/mm/mm.sock: Connection refused (os error 111)\n",
            "accepts SSH, but mmd is not running there",
            "journalctl -u mmd",
        ),
        (
            "@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@\nHost key verification failed.\n",
            "presented an SSH host key that differs from the recorded one",
            "--trust-new-host-keys",
        ),
        (
            "mm@203.0.113.12: Permission denied (publickey).\n",
            "refused this device's key",
            "mm key show",
        ),
        (
            "error[MM_RUNTIME_RESPONSE]: restricted relay rejected a client protocol version\n",
            "cannot read the hello of this mm",
            "mm target setup plan --upgrade",
        ),
        (
            "error[MM_RUNTIME_RESPONSE]: runtime response was invalid: remote protocol frame is malformed or unsupported: unknown field `minimum`, expected `version` or `hello` at line 1 column 31\n",
            "runs an mm from before 0.1.1",
            "mm target setup plan --upgrade",
        ),
        (
            "",
            "closed the connection before mmd answered",
            "systemctl status mmd",
        ),
    ];
    for (stderr, problem, help) in cases {
        let (actual_problem, actual_help) = connect_failure(stderr);
        let actual_problem = crate::domain::error::shown(&actual_problem);
        assert!(
            actual_problem.contains(problem),
            "{stderr:?}: {actual_problem}"
        );
        assert!(actual_help.contains(help), "{stderr:?}: {actual_help}");
    }
    // Nor is what sshd says in every session about the relay account's home.
    let (problem, _) = connect_failure(
        "Could not chdir to home directory /nonexistent: No such file or directory\n",
    );
    assert_eq!(problem, "closed the connection before mmd answered");
    // Banners and warnings are not the reason.
    let (problem, _) = connect_failure(
        "Warning: Permanently added the host key.\nssh: connect to host h port 22: No route to host\n",
    );
    let problem = crate::domain::error::shown(&problem);
    assert!(
        problem.ends_with("(ssh: connect to host h port 22: No route to host)"),
        "{problem}"
    );
}

#[test]
fn a_newer_daemon_agrees_on_the_protocol_this_mm_speaks() -> Result<(), Box<dyn std::error::Error>>
{
    let nonce = uuid::Uuid::from_u128(9);
    let fingerprint = "a".repeat(64);
    let answer = |version: u16| -> Result<ServerFrame, Box<dyn std::error::Error>> {
        let frame = serde_json::json!({
            "type": "hello",
            "version": version,
            "added_by_a_newer_mmd": 1,
            "hello": {
                "client_nonce": nonce,
                "server_nonce": uuid::Uuid::from_u128(10),
                "daemon_version": "9.9.9",
                "daemon_fingerprint": fingerprint,
                "instance_id": uuid::Uuid::from_u128(11),
                "added_by_a_newer_mmd": [1],
            },
        });
        Ok(crate::adapters::remote::codec::decode_frame(
            &serde_json::to_vec(&frame)?,
        )?)
    };
    assert_eq!(
        agreed(answer(PROTOCOL_VERSION)?, nonce, &fingerprint).ok(),
        Some((PROTOCOL_VERSION, "9.9.9".to_owned()))
    );
    assert!(matches!(
        agreed(answer(PROTOCOL_VERSION + 1)?, nonce, &fingerprint),
        Err(Some(HandshakeRefusal::Mismatch))
    ));
    assert!(matches!(
        agreed(
            answer(PROTOCOL_VERSION)?,
            uuid::Uuid::from_u128(12),
            &fingerprint
        ),
        Err(Some(HandshakeRefusal::Mismatch))
    ));

    let client_key = ClientKey::Default(None);
    let reuse = ConnectionReuse::off();
    let keys = ManagedKeys::none();
    let transport = SshRemoteTransport::for_target(
        &ssh_target(),
        ClientAccess {
            key: &client_key,
            reuse: &reuse,
            keys: &keys,
        },
    )?;
    let said = "This mm 0.1.1 speaks remote protocol 1, and the host's mmd 0.3.0 speaks 2 to 3. Update mm on this machine to 0.3.0 or newer: run `mm update`";
    let refused = transport
        .endpoint
        .refused(HandshakeRefusal::Daemon(RemoteError {
            code: UPGRADE_REQUIRED_CODE.to_owned(),
            message: said.to_owned(),
            retryable: false,
            mutation_state: crate::domain::remote::MutationState::NotStarted,
        }));
    assert_eq!(refused.code(), "MM_UNREACHABLE");
    assert_eq!(
        refused.to_string(),
        format!(
            "target 'remote' (example.test:2222) has no remote protocol in common with this mm. {said}"
        )
    );
    Ok(())
}

#[test]
fn a_device_address_changes_where_ssh_connects_and_not_the_pinned_host()
-> Result<(), Box<dyn std::error::Error>> {
    let mut target = ssh_target();
    target.definition.nodes = vec![node("app-1", "203.0.113.1", true)];
    let client_key = ClientKey::Default(None);
    let reuse = ConnectionReuse::off();
    let keys = ManagedKeys::none();
    let access = ClientAccess {
        key: &client_key,
        reuse: &reuse,
        keys: &keys,
    };
    let address = "[fd00::3]:2222".parse::<crate::domain::target::NodeAddress>()?;
    let transport =
        SshRemoteTransport::for_node(&target, &target.definition.nodes[0], Some(&address), access)?;
    assert_eq!(
        transport.endpoint.subject,
        "node 'app-1' ([fd00::3]:2222, this device's address for it)"
    );
    let (command, _) =
        transport
            .endpoint
            .ssh_command(std::path::Path::new("/known-hosts"), false, None)?;
    let arguments = command
        .as_std()
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(arguments.contains(&"HostKeyAlias=203.0.113.1".to_owned()));
    assert!(arguments.windows(2).any(|pair| pair == ["-p", "2222"]));
    assert_eq!(arguments.last().map(String::as_str), Some("relay@fd00::3"));
    let known_hosts = transport.endpoint.pinned_known_hosts()?;
    assert_eq!(
        std::fs::read_to_string(known_hosts.path())?,
        format!("203.0.113.1 {KEY}\n")
    );

    let repo = SshRemoteTransport::for_node(&target, &target.definition.nodes[0], None, access)?;
    let (command, _) =
        repo.endpoint
            .ssh_command(std::path::Path::new("/known-hosts"), false, None)?;
    let arguments = command
        .as_std()
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(arguments.contains(&"HostKeyAlias=203.0.113.1".to_owned()));
    assert_eq!(
        arguments.last().map(String::as_str),
        Some("relay@203.0.113.1")
    );
    Ok(())
}
