use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use zeroize::Zeroizing;

use super::tests::{plan, project};
use super::*;

struct StaticProcess {
    code: i32,
    stdout: String,
    stderr: String,
    truncated: bool,
}

#[async_trait]
impl ProcessRunner for StaticProcess {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        Ok(CommandOutput {
            code: Some(self.code),
            stdout: Zeroizing::new(self.stdout.clone()),
            stderr: Zeroizing::new(self.stderr.clone()),
            truncated: self.truncated,
        })
    }
}

fn static_executor(process: StaticProcess) -> AnsibleTargetBootstrapExecutor {
    AnsibleTargetBootstrapExecutor::with_executables(
        Arc::new(process),
        PathBuf::from("/tmp/mm-bootstrap-tests"),
        "ansible-playbook",
        "ssh-keyscan",
        None,
    )
}

#[tokio::test]
async fn account_keyscan_and_auth_socket_validation_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    for user in ["", "bad user", "abcdefghijklmnopqrstuvwxyz123456789"] {
        let executor = static_executor(StaticProcess {
            code: 0,
            stdout: user.to_owned(),
            stderr: String::new(),
            truncated: false,
        });
        assert!(executor.local_user().await.is_err());
    }
    let executor = static_executor(StaticProcess {
        code: 2,
        stdout: String::new(),
        stderr: "denied".to_owned(),
        truncated: false,
    });
    assert!(executor.local_user().await.is_err());

    for (stdout, truncated) in [
        ("host ssh-ed25519 not-base64".to_owned(), false),
        ("host unsupported value".to_owned(), false),
        ("host ssh-ed25519 dGVzdA==".to_owned(), true),
        (
            (0..33).map(|_| "host ssh-ed25519 dGVzdA==\n").collect(),
            false,
        ),
    ] {
        let executor = static_executor(StaticProcess {
            code: 0,
            stdout,
            stderr: String::new(),
            truncated,
        });
        assert!(
            executor
                .known_hosts("host", 22, "SHA256:no-match")
                .await
                .is_err()
        );
    }

    let executor = static_executor(StaticProcess {
        code: 0,
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
    });
    assert!(executor.require_ssh_auth_socket().is_err());
    let relative = AnsibleTargetBootstrapExecutor {
        ssh_auth_sock: Some(PathBuf::from("relative.sock")),
        ..executor.clone()
    };
    assert!(relative.require_ssh_auth_socket().is_err());
    let temp = crate::test_support::tempdir()?;
    let regular = temp.path().join("regular");
    fs::write(&regular, b"file")?;
    let regular = AnsibleTargetBootstrapExecutor {
        ssh_auth_sock: Some(regular),
        ..executor
    };
    assert!(regular.require_ssh_auth_socket().is_err());
    Ok(())
}

#[test]
fn inventory_snapshot_and_private_file_helpers_cover_invalid_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let mut local = plan(project.path(), TargetTransport::Local, false)?;
    assert!(inventory(&local, None, None).is_err());
    assert!(inventory(&local, None, Some("local-user"))?.contains("ansible_connection: local"));
    assert!(inventory(&local, None, Some("local-user"))?.contains("mm_runtime: podman-compose"));
    local.selected_runtime = crate::domain::runtime::RuntimeKind::DockerCompose;
    assert!(inventory(&local, None, Some("local-user"))?.contains("mm_runtime: docker-compose"));

    let transport = TargetTransport::Ssh {
        host: "host".to_owned(),
        port: 2222,
        user: "deploy".to_owned(),
        host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU".to_owned(),
        host_key: None,
        identity_file: None,
        remote_root: None,
        daemon_fingerprint: None,
    };
    let remote = plan(project.path(), transport, false)?;
    assert!(inventory(&remote, None, None).is_err());
    assert!(shell_quote("a'b").contains("\"'\""));

    let destination = crate::test_support::tempdir()?;
    snapshot_inputs(project.path(), destination.path(), &remote)?;
    let mut changed = remote.clone();
    changed.input_files[0].digest = "0".repeat(64);
    assert!(snapshot_inputs(project.path(), destination.path(), &changed).is_err());

    let private = destination.path().join("private");
    write_private_file(&private, b"secret")?;
    assert!(write_private_file(&private, b"again").is_err());
    let prepared = destination.path().join("prepared");
    prepare_staging_root(&prepared)?;
    assert!(prepared.is_dir());
    assert!(fixed_executable(&["/definitely/missing"]).is_err());
    assert_eq!(fixed_executable(&["/bin/true"])?, "/bin/true");
    assert!(
        require_success(
            "tool",
            &CommandOutput {
                code: Some(9),
                stdout: Zeroizing::new(String::new()),
                stderr: Zeroizing::new("failed".to_owned()),
                truncated: false,
            },
        )
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn keyscan_rejects_unsafe_and_unmatched_output() -> Result<(), Box<dyn std::error::Error>> {
    for stdout in [
        "# comment\n\n".to_owned(),
        format!("host ssh-ed25519 {}\n", "x".repeat(16 * 1024 + 1)),
        "host ssh-ed25519 dGVzdA==\nnon-ascii\u{e9} ssh-ed25519 dGVzdA==\n".to_owned(),
    ] {
        let executor = static_executor(StaticProcess {
            code: 0,
            stdout,
            stderr: String::new(),
            truncated: false,
        });
        assert!(
            executor
                .known_hosts("host", 22, "SHA256:no-match")
                .await
                .is_err()
        );
    }

    let executor = static_executor(StaticProcess {
        code: 3,
        stdout: String::new(),
        stderr: "scan failed".to_owned(),
        truncated: false,
    });
    assert!(
        executor
            .known_hosts("host", 22, "SHA256:no-match")
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn snapshot_and_directory_helpers_report_filesystem_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let local = plan(project.path(), TargetTransport::Local, false)?;
    fs::remove_file(project.path().join(&local.input_files[0].path))?;
    let destination = crate::test_support::tempdir()?;
    assert!(snapshot_inputs(project.path(), destination.path(), &local).is_err());

    let blocked_parent = destination.path().join("blocked");
    fs::write(&blocked_parent, b"not a directory")?;
    assert!(write_private_file(&blocked_parent.join("private"), b"secret").is_err());
    assert!(prepare_staging_root(&blocked_parent.join("child")).is_err());
    assert!(set_private_directory_permissions(&destination.path().join("missing")).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn auth_socket_rejects_non_utf8_paths() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStringExt as _;
    use std::os::unix::net::UnixListener;

    if !crate::test_support::NON_UTF8_FILE_NAMES {
        return Ok(());
    }
    let temp = crate::test_support::tempdir()?;
    let name = std::ffi::OsString::from_vec(vec![b'a', 0xff]);
    let socket = temp.path().join(name);
    let _listener = UnixListener::bind(&socket)?;
    let executor = AnsibleTargetBootstrapExecutor {
        ssh_auth_sock: Some(socket),
        ..static_executor(StaticProcess {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
            truncated: false,
        })
    };
    assert!(matches!(
        executor.require_ssh_auth_socket(),
        Err(MmError::Security { .. })
    ));
    Ok(())
}

#[test]
fn failed_playbooks_report_ansibles_failure_lines() {
    let output = |code: i32, stdout: &str, stderr: &str| CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    };
    let failure =
        |output: CommandOutput| match require_playbook_success("ansible-playbook", &output) {
            Err(MmError::ProcessFailed { stderr, .. }) => Some(stderr),
            _ => None,
        };
    let stdout = "PLAY [Install]\n\nTASK [Check sshd]\nfatal: [mm_target]: FAILED! => {\"msg\": \"sshd does not apply the relay drop-in\"}\n\nPLAY RECAP\nmm_target : ok=3 failed=1\n";
    assert_eq!(
        failure(output(2, stdout, "")).as_deref(),
        Some("fatal: [mm_target]: FAILED! => {\"msg\": \"sshd does not apply the relay drop-in\"}")
    );
    assert_eq!(
        failure(output(4, "ERROR! no hosts\n", "")).as_deref(),
        Some("ERROR! no hosts")
    );
    // Warnings on stderr do not replace the task that failed.
    assert_eq!(
        failure(output(2, stdout, "[WARNING]: a deprecated setting")).as_deref(),
        Some("fatal: [mm_target]: FAILED! => {\"msg\": \"sshd does not apply the relay drop-in\"}")
    );
    assert_eq!(
        failure(output(4, "PLAY [Install]\n", "connection refused")).as_deref(),
        Some("connection refused")
    );
    assert!(require_playbook_success("ansible-playbook", &output(0, "ok", "")).is_ok());
}

#[test]
fn a_home_too_long_for_the_control_socket_connects_without_sharing() {
    // `<home>/.ansible/cp/<10>` plus ssh's 17-character suffix is 40 bytes.
    let fits = Path::new("/").join("h".repeat(59));
    let too_long = Path::new("/").join("h".repeat(60));
    assert!(control_socket_fits(&fits));
    assert!(!control_socket_fits(&too_long));
    assert!(control_socket_fits(Path::new(
        "/home/operator/.mm/run/bootstrap/.bootstrap-abcdef"
    )));
}
