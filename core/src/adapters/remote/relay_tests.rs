use std::os::unix::fs::symlink;

use super::*;
use crate::adapters::remote::codec::{read_frame, write_frame};

#[test]
fn openssh_fingerprints_accept_explicit_and_encoded_keys() -> Result<(), Box<dyn std::error::Error>>
{
    use base64::Engine as _;
    use sha2::Digest as _;

    let blob = b"authenticated-key-blob";
    let encoded = base64::engine::general_purpose::STANDARD.encode(blob);
    let expected = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(sha2::Sha256::digest(blob))
    );
    assert_eq!(
        authentication_fingerprint(&format!("publickey ssh-ed25519 {encoded}\n"))?,
        expected
    );
    assert_eq!(
        authentication_fingerprint("publickey SHA256:explicit\n")?,
        "SHA256:explicit"
    );
    assert!(authentication_fingerprint("publickey ssh-ed25519").is_err());
    assert!(authentication_fingerprint("publickey ssh-ed25519 !!!").is_err());
    assert!(authentication_fingerprint("password accepted").is_err());
    assert!(format!("{}", protocol("failure")).contains("failure"));
    Ok(())
}

#[test]
fn relay_rejects_foreign_relative_and_symlink_socket_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let socket = temporary.path().join("daemon.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket)?;
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata(&socket)?.uid();
    validate_socket_path(&socket, uid)?;
    assert!(validate_socket_path(Path::new("relative.sock"), uid).is_err());
    assert!(validate_socket_path(&socket, uid.saturating_add(1)).is_err());
    let link = temporary.path().join("link.sock");
    symlink(&socket, &link)?;
    assert!(validate_socket_path(&link, uid).is_err());
    Ok(())
}

#[tokio::test]
async fn relay_forwards_valid_client_and_server_frames_and_half_closes()
-> Result<(), Box<dyn std::error::Error>> {
    let client = ClientFrame::Cancel {
        version: PROTOCOL_VERSION,
        request_id: uuid::Uuid::from_u128(1),
    };
    let (mut input, mut reader) = tokio::io::duplex(4096);
    let (mut writer, mut output) = tokio::io::duplex(4096);
    write_frame(&mut input, &client).await?;
    input.shutdown().await?;
    relay_client_frames(
        &mut reader,
        &mut writer,
        Duration::from_secs(1),
        &RequestRelayed::new(),
    )
    .await?;
    assert_eq!(
        read_frame::<_, ClientFrame>(&mut output).await?,
        Some(client)
    );

    let server = ServerFrame::End {
        version: PROTOCOL_VERSION,
        request_id: uuid::Uuid::from_u128(2),
        frames: 1,
        bytes: 2,
    };
    let (mut input, mut reader) = tokio::io::duplex(4096);
    let (mut writer, mut output) = tokio::io::duplex(4096);
    write_frame(&mut input, &server).await?;
    input.shutdown().await?;
    relay_server_frames(
        &mut reader,
        &mut writer,
        Duration::from_secs(1),
        &RequestRelayed::new(),
    )
    .await?;
    assert_eq!(
        read_frame::<_, ServerFrame>(&mut output).await?,
        Some(server)
    );
    Ok(())
}

#[tokio::test]
async fn relay_rejects_wrong_versions_malformed_frames_and_idle_peers()
-> Result<(), Box<dyn std::error::Error>> {
    // A client of another protocol version reaches the daemon, which is the
    // one that can say which side to upgrade.
    let other_version = ClientFrame::Cancel {
        version: PROTOCOL_VERSION + 1,
        request_id: uuid::Uuid::nil(),
    };
    let (mut input, mut reader) = tokio::io::duplex(4096);
    let (mut writer, mut output) = tokio::io::duplex(4096);
    write_frame(&mut input, &other_version).await?;
    drop(input);
    relay_client_frames(
        &mut reader,
        &mut writer,
        Duration::from_secs(1),
        &RequestRelayed::new(),
    )
    .await?;
    assert_eq!(
        crate::adapters::remote::codec::read_frame::<_, ClientFrame>(&mut output).await?,
        Some(other_version)
    );

    // A daemon frame of another version is a broken host and is not relayed.
    let wrong = ServerFrame::End {
        version: PROTOCOL_VERSION + 1,
        request_id: uuid::Uuid::nil(),
        frames: 0,
        bytes: 0,
    };
    let (mut input, mut reader) = tokio::io::duplex(4096);
    let (mut writer, _output) = tokio::io::duplex(4096);
    write_frame(&mut input, &wrong).await?;
    assert!(
        relay_server_frames(
            &mut reader,
            &mut writer,
            Duration::from_secs(1),
            &RequestRelayed::new()
        )
        .await
        .is_err()
    );

    let (mut input, mut reader) = tokio::io::duplex(4096);
    let (mut writer, _output) = tokio::io::duplex(4096);
    write_raw_frame(&mut input, b"not-json").await?;
    assert!(
        relay_server_frames(
            &mut reader,
            &mut writer,
            Duration::from_secs(1),
            &RequestRelayed::new()
        )
        .await
        .is_err()
    );

    let (_input, mut reader) = tokio::io::duplex(16);
    let (mut writer, _output) = tokio::io::duplex(16);
    assert!(
        relay_client_frames(
            &mut reader,
            &mut writer,
            Duration::from_millis(1),
            &RequestRelayed::new()
        )
        .await
        .is_err()
    );
    Ok(())
}

#[test]
fn authenticated_session_and_stdio_relay_run_in_isolated_ssh_environments()
-> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read as _;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::process::{Command, Stdio};

    if let Ok(mode) = std::env::var("MM_RELAY_TEST_CHILD") {
        match mode.as_str() {
            "valid" => {
                validate_forced_command_environment()?;
                let session = authenticated_session()?;
                assert_eq!(session.key_fingerprint, "SHA256:validKey");
                assert_eq!(session.source, "127.0.0.1 1 127.0.0.1 2");
            }
            "command" => assert!(validate_forced_command_environment().is_err()),
            "bad-source" => assert!(authenticated_session().is_err()),
            "serve" => {
                let socket = std::env::var("MM_RELAY_TEST_SOCKET")?;
                let uid = std::fs::metadata(&socket)?.uid();
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(serve_stdio_relay(
                        Path::new(&socket),
                        uid,
                        Duration::from_secs(2),
                    ))?;
            }
            _ => return Err("unknown relay child mode".into()),
        }
        return Ok(());
    }

    let temporary = crate::test_support::tempdir()?;
    let auth = temporary.path().join("auth-info");
    std::fs::write(&auth, b"publickey SHA256:validKey\n")?;
    std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o600))?;
    let executable = std::env::current_exe()?;
    let test_name = "adapters::remote::relay::tests::authenticated_session_and_stdio_relay_run_in_isolated_ssh_environments";
    let run = |mode: &str, source: &str, command: Option<&str>, socket: Option<&Path>| {
        let mut child = Command::new(&executable);
        child
            .args(["--exact", test_name, "--nocapture"])
            .env("MM_RELAY_TEST_CHILD", mode)
            .env("SSH_CONNECTION", source)
            .env("SSH_USER_AUTH", &auth)
            .env_remove("SSH_ORIGINAL_COMMAND")
            .stdin(Stdio::null());
        if let Some(command) = command {
            child.env("SSH_ORIGINAL_COMMAND", command);
        }
        if let Some(socket) = socket {
            child.env("MM_RELAY_TEST_SOCKET", socket);
        }
        child.output()
    };
    assert!(
        run("valid", "127.0.0.1 1 127.0.0.1 2", None, None)?
            .status
            .success()
    );
    assert!(
        run("command", "valid", Some("forbidden"), None)?
            .status
            .success()
    );
    assert!(
        run("bad-source", "bad/source", None, None)?
            .status
            .success()
    );

    let socket = temporary.path().join("daemon.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket)?;
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        let mut request = Vec::new();
        stream.read_to_end(&mut request)?;
        Ok(())
    });
    let served = run("serve", "127.0.0.1 1 127.0.0.1 2", None, Some(&socket))?;
    assert!(
        served.status.success(),
        "{}",
        String::from_utf8_lossy(&served.stderr)
    );
    server
        .join()
        .map_err(|_| "relay server thread panicked")??;
    Ok(())
}

#[tokio::test]
async fn relay_waits_past_the_idle_limit_once_the_request_went_through()
-> Result<(), Box<dyn std::error::Error>> {
    // The daemon's reply is already being read when the request goes
    // through; a long apply then answers after the idle limit.
    let requested = RequestRelayed::new();
    let server = ServerFrame::End {
        version: PROTOCOL_VERSION,
        request_id: uuid::Uuid::from_u128(3),
        frames: 0,
        bytes: 0,
    };
    let (mut input, mut reader) = tokio::io::duplex(4096);
    let (mut writer, mut output) = tokio::io::duplex(4096);
    let relay = relay_server_frames(
        &mut reader,
        &mut writer,
        Duration::from_millis(50),
        &requested,
    );
    let daemon = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        requested.mark();
        tokio::time::sleep(Duration::from_millis(250)).await;
        write_frame(&mut input, &server).await?;
        input.shutdown().await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let (relayed, answered) = tokio::join!(relay, daemon);
    relayed?;
    answered?;
    assert_eq!(
        read_frame::<_, ServerFrame>(&mut output).await?,
        Some(server)
    );

    // Before the request, the idle limit still applies.
    let (_input, mut reader) = tokio::io::duplex(16);
    let (mut writer, _output) = tokio::io::duplex(16);
    assert!(
        relay_server_frames(
            &mut reader,
            &mut writer,
            Duration::from_millis(20),
            &RequestRelayed::new()
        )
        .await
        .is_err()
    );
    Ok(())
}

#[test]
fn relay_exits_when_the_daemon_hangs_up_while_the_client_stays_connected()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::process::{Command, Stdio};

    if std::env::var("MM_RELAY_HANGUP_CHILD").is_ok() {
        let socket = std::env::var("MM_RELAY_TEST_SOCKET")?;
        let uid = std::fs::metadata(&socket)?.uid();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(serve_stdio_relay(
                Path::new(&socket),
                uid,
                Duration::from_secs(30),
            ))?;
        return Ok(());
    }

    let temporary = crate::test_support::tempdir()?;
    let auth = temporary.path().join("auth-info");
    std::fs::write(&auth, b"publickey SHA256:validKey\n")?;
    std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o600))?;
    let socket = temporary.path().join("daemon.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket)?;
    let test_name = "adapters::remote::relay::tests::relay_exits_when_the_daemon_hangs_up_while_the_client_stays_connected";
    // The client keeps its end open and silent, as an SSH client waiting
    // for a reply does.
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", test_name, "--nocapture"])
        .env("MM_RELAY_HANGUP_CHILD", "1")
        .env("MM_RELAY_TEST_SOCKET", &socket)
        .env("SSH_CONNECTION", "127.0.0.1 1 127.0.0.1 2")
        .env("SSH_USER_AUTH", &auth)
        .env_remove("SSH_ORIGINAL_COMMAND")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let _client_stdin = child.stdin.take();
    let (mut stream, _) = listener.accept()?;
    // Take the relay's session attribution, then hang up, as a daemon that
    // ended the connection mid-request does.
    let mut length = [0_u8; 4];
    std::io::Read::read_exact(&mut stream, &mut length)?;
    let mut session = vec![0_u8; usize::try_from(u32::from_be_bytes(length))?];
    std::io::Read::read_exact(&mut stream, &mut session)?;
    drop(stream);

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() > deadline {
            child.kill()?;
            return Err("relay kept running after the daemon hung up".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        std::io::Read::read_to_string(&mut pipe, &mut stderr)?;
    }
    assert!(status.success(), "relay exited with {status}: {stderr}");
    Ok(())
}

#[tokio::test]
async fn relay_leaves_when_its_client_is_gone_whatever_the_daemon_does()
-> Result<(), Box<dyn std::error::Error>> {
    // The client sent its request and was killed: standard input ends. The
    // daemon is stuck and neither answers nor hangs up.
    let request = ClientFrame::Request {
        version: PROTOCOL_VERSION,
        request_id: uuid::Uuid::from_u128(7),
        operation_nonce: uuid::Uuid::from_u128(8),
        deadline_unix_ms: i64::MAX,
        scope: crate::domain::remote::RemoteScope {
            project_id: crate::domain::workspace::ProjectId(uuid::Uuid::from_u128(4)),
            environment: "production".to_owned(),
            target: "production".to_owned(),
            target_fingerprint: "a".repeat(64),
            runtime: crate::domain::runtime::RuntimeKind::DockerCompose,
            node: None,
        },
        method: Box::new(crate::domain::remote::RemoteMethod::Status),
    };
    let (mut client, mut stdin) = tokio::io::duplex(4096);
    let (mut stdout, _client_reads) = tokio::io::duplex(4096);
    let (_daemon_writes, mut socket_reader) = tokio::io::duplex(4096);
    let (mut socket_writer, mut daemon_reads) = tokio::io::duplex(4096);
    write_frame(&mut client, &request).await?;
    drop(client);
    let started = tokio::time::Instant::now();
    tokio::time::timeout(
        CLIENT_GONE_GRACE + Duration::from_secs(3),
        relay_session(
            (&mut stdin, &mut stdout),
            (&mut socket_reader, &mut socket_writer),
            Duration::from_secs(30),
        ),
    )
    .await
    .map_err(|_| "the relay stayed after its client was gone")??;
    assert!(
        started.elapsed() >= CLIENT_GONE_GRACE,
        "the daemon gets a moment to hang up"
    );
    // The daemon was sent the request and then told the client is gone.
    assert_eq!(
        read_frame::<_, ClientFrame>(&mut daemon_reads).await?,
        Some(request)
    );
    assert_eq!(read_frame::<_, ClientFrame>(&mut daemon_reads).await?, None);
    Ok(())
}
