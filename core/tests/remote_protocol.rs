use std::time::Duration;

use base64::Engine as _;
use mm::adapters::remote::{decode_frame, encode_frame, read_raw_frame, write_raw_frame};
use mm::domain::remote::{
    ClientFrame, ClientHello, MAX_FRAME_BYTES, PROTOCOL_VERSION, RemotePlanBinding,
};
use mm::domain::runtime::RuntimeKind;
use mm::domain::target::{
    TargetBootstrap, TargetDefinition, TargetId, TargetTransport, validate_target_definition,
};
use mm::domain::workspace::ProjectId;
use sha2::Digest as _;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

mod temp_dirs;

#[tokio::test]
async fn frame_parser_rejects_truncation_zero_and_oversize()
-> Result<(), Box<dyn std::error::Error>> {
    for prefix in [
        vec![0_u8],
        vec![0, 0],
        vec![0, 0, 0],
        vec![0, 0, 0, 0],
        u32::try_from(mm::domain::remote::MAX_FRAME_BYTES + 1)?
            .to_be_bytes()
            .to_vec(),
    ] {
        let (mut writer, mut reader) = tokio::io::duplex(64);
        writer.write_all(&prefix).await?;
        writer.shutdown().await?;
        assert!(read_raw_frame(&mut reader).await.is_err());
    }

    let (mut writer, mut reader) = tokio::io::duplex(64);
    writer.write_all(&5_u32.to_be_bytes()).await?;
    writer.write_all(b"ab").await?;
    writer.shutdown().await?;
    assert!(read_raw_frame(&mut reader).await.is_err());
    Ok(())
}

#[test]
fn strict_frames_reject_unknown_fields_and_fuzz_corpus_without_panicking()
-> Result<(), Box<dyn std::error::Error>> {
    let unknown = br#"{"type":"hello","version":1,"hello":{"client_nonce":"00000000-0000-0000-0000-000000000001","expected_daemon_fingerprint":"x","principal":"forged"}}"#;
    assert!(decode_frame::<ClientFrame>(unknown).is_err());

    let mut state = 0x9e37_79b9_u32;
    for length in 0..4096_usize {
        let bytes = (0..length.min(257))
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect::<Vec<_>>();
        let _ = decode_frame::<ClientFrame>(&bytes);
    }
    Ok(())
}

#[test]
fn encoded_and_decoded_frames_enforce_the_exact_byte_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let at_limit = "a".repeat(MAX_FRAME_BYTES - 2);
    let encoded = encode_frame(&at_limit)?;
    assert_eq!(encoded.len(), MAX_FRAME_BYTES);
    assert_eq!(decode_frame::<String>(&encoded)?, at_limit);

    let over_limit = "a".repeat(MAX_FRAME_BYTES - 1);
    assert!(matches!(
        encode_frame(&over_limit),
        Err(error) if error.to_string().contains("protocol bound")
    ));

    let oversized_json = format!("\"{over_limit}\"");
    assert_eq!(oversized_json.len(), MAX_FRAME_BYTES + 1);
    assert!(matches!(
        decode_frame::<String>(oversized_json.as_bytes()),
        Err(error) if error.to_string().contains("protocol bound")
    ));

    assert!(matches!(
        decode_frame::<String>(&[]),
        Err(error) if error.to_string().contains("protocol bound")
    ));
    Ok(())
}

#[tokio::test]
async fn raw_frame_writer_rejects_an_empty_protocol_frame() {
    let (mut writer, _reader) = tokio::io::duplex(8);
    assert!(matches!(
        write_raw_frame(&mut writer, &[]).await,
        Err(error) if error.to_string().contains("protocol bound")
    ));
}

#[tokio::test]
async fn raw_frames_enforce_the_exact_protocol_boundary() -> Result<(), Box<dyn std::error::Error>>
{
    let payload = vec![b'a'; MAX_FRAME_BYTES];
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&u32::try_from(payload.len())?.to_be_bytes());
    framed.extend_from_slice(&payload);
    assert_eq!(
        read_raw_frame(&mut std::io::Cursor::new(framed)).await?,
        Some(payload.clone())
    );
    assert!(
        write_raw_frame(&mut tokio::io::sink(), &payload)
            .await
            .is_ok()
    );

    let oversized_length = u32::try_from(MAX_FRAME_BYTES + 1)?.to_be_bytes();
    assert!(
        read_raw_frame(&mut std::io::Cursor::new(oversized_length))
            .await
            .is_err()
    );
    assert!(
        write_raw_frame(&mut tokio::io::sink(), &vec![0; MAX_FRAME_BYTES + 1])
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn protocol_version_and_plan_scope_are_cryptographically_bound()
-> Result<(), Box<dyn std::error::Error>> {
    let hello = ClientFrame::Hello {
        version: PROTOCOL_VERSION + 1,
        minimum: PROTOCOL_VERSION + 1,
        hello: ClientHello {
            client_nonce: Uuid::from_u128(1),
            expected_daemon_fingerprint: "a".repeat(64),
            client_version: String::new(),
        },
    };
    let decoded: ClientFrame = decode_frame(&encode_frame(&hello)?)?;
    assert_eq!(decoded.version(), PROTOCOL_VERSION + 1);
    assert_ne!(decoded.version(), PROTOCOL_VERSION);

    let mut binding = RemotePlanBinding {
        protocol_version: PROTOCOL_VERSION,
        daemon_version: "0.1.0".to_owned(),
        daemon_fingerprint: "a".repeat(64),
        target_fingerprint: "b".repeat(64),
        target: "production".to_owned(),
        node: None,
        kind: mm::domain::remote::RemotePlanKind::Deployment,
        project_id: ProjectId(Uuid::from_u128(2)),
        environment: "production".to_owned(),
        runtime: RuntimeKind::DockerCompose,
        commit: "e".repeat(40),
        deployment_plan_hash: "c".repeat(64),
        access_change_hash: "c".repeat(64),
        principal_id: mm::domain::access::PrincipalId(Uuid::from_u128(4)),
        authorization_revision: 1,
        authorization_hash: "d".repeat(64),
        plan_nonce: Uuid::from_u128(3),
        expires_at_unix_ms: 10_000,
        bundle_hash: None,
        plan_hash: String::new(),
    };
    binding.plan_hash = binding.canonical_hash()?;
    binding.validate()?;
    let reviewed = binding.plan_hash.clone();
    assert_eq!(binding.canonical_hash()?, reviewed);
    let mut old_protocol = binding.clone();
    old_protocol.protocol_version = PROTOCOL_VERSION - 1;
    assert!(old_protocol.canonical_hash().is_err());
    binding.environment = "staging".to_owned();
    assert_ne!(binding.canonical_hash()?, reviewed);
    assert!(binding.validate().is_err());
    Ok(())
}

#[test]
fn enrolled_host_public_key_must_match_the_reviewed_fingerprint()
-> Result<(), Box<dyn std::error::Error>> {
    let mut key_blob = Vec::new();
    for field in [&b"ssh-ed25519"[..], &[7_u8; 32][..]] {
        key_blob.extend_from_slice(&u32::try_from(field.len())?.to_be_bytes());
        key_blob.extend_from_slice(field);
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(&key_blob);
    let fingerprint = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(sha2::Sha256::digest(&key_blob))
    );
    let definition = TargetDefinition {
        id: TargetId(Uuid::from_u128(4)),
        transport: TargetTransport::Ssh {
            host: "host.example.test".to_owned(),
            port: 22,
            user: "mm-relay".to_owned(),
            host_key_fingerprint: fingerprint,
            host_key: Some(format!("ssh-ed25519 {encoded}")),
            identity_file: Some("/var/lib/mm-client/identity".into()),
            remote_root: Some("/srv/mm/example".into()),
            daemon_fingerprint: Some("d".repeat(64)),
        },
        bootstrap: TargetBootstrap::Ansible,
        ingress: None,
        default_runtime: Some(RuntimeKind::DockerCompose),
        available_runtimes: vec![RuntimeKind::DockerCompose],
        runtime: None,
        nodes: Vec::new(),
    };
    validate_target_definition(&definition)?;
    let mut mismatched = definition.clone();
    if let TargetTransport::Ssh {
        host_key_fingerprint,
        ..
    } = &mut mismatched.transport
    {
        *host_key_fingerprint = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU".to_owned();
    }
    assert!(validate_target_definition(&mismatched).is_err());
    let mut mismatched_type = definition;
    if let TargetTransport::Ssh { host_key, .. } = &mut mismatched_type.transport {
        *host_key = Some(format!("ssh-rsa {encoded}"));
    }
    assert!(validate_target_definition(&mismatched_type).is_err());
    Ok(())
}

#[test]
fn public_protocol_limits_are_finite_and_consistent() {
    const {
        assert!(mm::domain::remote::MAX_FRAME_BYTES <= 8 * 1024 * 1024);
        assert!(mm::domain::remote::MAX_STREAM_BYTES < u64::MAX);
        assert!(mm::domain::remote::MAX_STREAM_FRAMES < u64::MAX);
    }
    assert!(
        Duration::from_millis(mm::domain::remote::MAX_REQUEST_TIMEOUT_MS)
            <= Duration::from_secs(24 * 60 * 60)
    );
}

#[test]
fn forced_relay_rejects_arbitrary_ssh_command_text_without_execution()
-> Result<(), Box<dyn std::error::Error>> {
    use assert_cmd::cargo::CommandCargoExt;

    let temporary = temp_dirs::tempdir()?;
    let sentinel = temporary.path().join("must-not-exist");
    let original = format!("touch {}", sentinel.display());
    let status = std::process::Command::cargo_bin("mm")?
        .env("SSH_CONNECTION", "127.0.0.1 10000 127.0.0.1 22")
        .env("SSH_ORIGINAL_COMMAND", original)
        .args(["relay", "serve", "--stdio", "--socket", "/does/not/exist"])
        .status()?;
    assert!(!status.success());
    assert!(!sentinel.exists());
    Ok(())
}

/// A socket the daemon closed for writing.
struct ClosedSocket;

impl tokio::io::AsyncWrite for ClosedSocket {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
        _bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn a_relay_passes_on_the_daemons_answer_when_the_client_still_sends()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::domain::remote::{MutationState, RemoteError, ServerFrame};

    let request_id = Uuid::from_u128(5);
    let (mut client, mut relay_stdin) = tokio::io::duplex(64 * 1024);
    let (mut relay_stdout, mut client_reads) = tokio::io::duplex(64 * 1024);
    let (mut daemon, mut relay_socket) = tokio::io::duplex(64 * 1024);
    let mut closed = ClosedSocket;
    let relay = tokio::spawn(async move {
        mm::adapters::remote::relay_session(
            (&mut relay_stdin, &mut relay_stdout),
            (&mut relay_socket, &mut closed),
            Duration::from_secs(10),
        )
        .await
    });
    write_raw_frame(
        &mut client,
        &encode_frame(&ClientFrame::Data {
            version: PROTOCOL_VERSION,
            request_id,
            bytes: "AAAA".to_owned(),
        })?,
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    write_raw_frame(
        &mut daemon,
        &encode_frame(&ServerFrame::Error {
            version: PROTOCOL_VERSION,
            request_id: Some(request_id),
            error: RemoteError {
                code: "MM_CANCELLED".to_owned(),
                message: "delivery of commit cccccccccccc was cancelled by ops".to_owned(),
                retryable: false,
                mutation_state: MutationState::NotStarted,
            },
        })?,
    )
    .await?;
    drop(daemon);
    let frame = tokio::time::timeout(Duration::from_secs(5), read_raw_frame(&mut client_reads))
        .await??
        .ok_or("the relay passed nothing on")?;
    match decode_frame::<ServerFrame>(&frame)? {
        ServerFrame::Error { error, .. } => assert_eq!(error.code, "MM_CANCELLED"),
        other => return Err(format!("unexpected frame: {other:?}").into()),
    }
    tokio::time::timeout(Duration::from_secs(5), relay).await???;
    Ok(())
}
