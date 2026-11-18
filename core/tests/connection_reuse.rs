//! Which SSH connections `mm` keeps open between requests, and where.
//!
//! A kept connection is a way to act as its key without the key, so only a
//! key that mm can use with nobody present anyway is kept (a plain key
//! file, or this device's key, whose passphrase mm takes from a vault), in
//! a directory that only the user can enter.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use mm::adapters::keys::ManagedKeys;
use mm::adapters::remote::{ClientAccess, ClientKey, ConnectionReuse, SshRemoteTransport};
use mm::app::ports::KeyUnlock;
use mm::domain::key::KeyPassphrase;
use mm::domain::runtime::RuntimeKind;
use mm::domain::target::{
    NamedTarget, TargetBootstrap, TargetDefinition, TargetId, TargetTransport,
};

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HOST_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn target(host: &str) -> NamedTarget {
    NamedTarget::new(
        "remote".to_owned(),
        TargetDefinition {
            id: TargetId(uuid::Uuid::from_u128(1)),
            transport: TargetTransport::Ssh {
                host: host.to_owned(),
                port: 22,
                user: "mm".to_owned(),
                host_key_fingerprint: "SHA256:kmYcvdi2GkPeWxB6XLjrZB8JHsy2Hm8luHMFp9GMvqk"
                    .to_owned(),
                host_key: Some(HOST_KEY.to_owned()),
                identity_file: None,
                remote_root: None,
                daemon_fingerprint: Some("a".repeat(64)),
            },
            bootstrap: TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: Some(RuntimeKind::DockerCompose),
            available_runtimes: vec![RuntimeKind::DockerCompose],
            runtime: None,
            nodes: Vec::new(),
        },
    )
}

fn field(bytes: &[u8]) -> Vec<u8> {
    let mut field = u32::try_from(bytes.len())
        .unwrap_or(u32::MAX)
        .to_be_bytes()
        .to_vec();
    field.extend_from_slice(bytes);
    field
}

/// A file in the layout of an OpenSSH private key, with no key in it: the
/// header, the cipher, and a public half of the given type. `mm` reads no
/// further than that.
fn key_file(directory: &Path, name: &str, cipher: &str, kind: &str) -> std::io::Result<PathBuf> {
    let public = [field(kind.as_bytes()), field(&[7; 32])].concat();
    let body = [
        b"openssh-key-v1\0".to_vec(),
        field(cipher.as_bytes()),
        field(b"none"),
        field(b""),
        1_u32.to_be_bytes().to_vec(),
        field(&public),
        field(&[0; 64]),
    ]
    .concat();
    let armor = |edge: &str| format!("-----{edge} OPENSSH {} KEY-----", "PRIVATE");
    let path = directory.join(name);
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n{}\n",
            armor("BEGIN"),
            base64::engine::general_purpose::STANDARD.encode(body),
            armor("END")
        ),
    )?;
    Ok(path)
}

fn kept(reuse: &ConnectionReuse, key: &ClientKey) -> Result<Option<PathBuf>, mm::MmError> {
    kept_with(reuse, key, &ManagedKeys::none())
}

fn kept_with(
    reuse: &ConnectionReuse,
    key: &ClientKey,
    keys: &ManagedKeys,
) -> Result<Option<PathBuf>, mm::MmError> {
    Ok(
        SshRemoteTransport::for_target(&target("203.0.113.11"), ClientAccess { key, reuse, keys })?
            .kept_connection()
            .map(Path::to_path_buf),
    )
}

/// Stands in for the vault: says which key file mm keeps the passphrase
/// of. Nothing here connects, so no passphrase is ever asked for.
struct Unlocks(PathBuf);

impl KeyUnlock for Unlocks {
    fn manages(&self, path: &Path) -> bool {
        path == self.0
    }

    fn passphrase(&self, _path: &Path) -> Result<KeyPassphrase, mm::MmError> {
        Err(mm::MmError::Security {
            message: "no connection is made in this test".to_owned(),
        })
    }
}

fn unlocking(key: &Path, home: &Path) -> ManagedKeys {
    ManagedKeys::new(
        std::sync::Arc::new(Unlocks(key.to_path_buf())),
        home.join("run"),
    )
}

#[test]
fn a_plain_key_file_keeps_its_connection_in_a_directory_of_the_user_alone() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let key = key_file(home.path(), "device", "none", "ssh-ed25519")?;
    let reuse = ConnectionReuse::kept_under(&home.path().join("run"));

    let socket = kept(&reuse, &ClientKey::Default(Some(key.clone())))?
        .ok_or("a plain key file should keep its connection")?;

    let directory = home.path().join("run").join("ssh");
    assert_eq!(socket.parent(), Some(directory.as_path()));
    assert_eq!(
        std::fs::metadata(&directory)?.permissions().mode() & 0o777,
        0o700
    );
    let name = socket
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("socket name")?;
    assert_eq!(name.len(), 16);
    assert!(name.bytes().all(|byte| byte.is_ascii_hexdigit()));
    // The same host and key share one connection; another host has its own.
    assert_eq!(
        kept(&reuse, &ClientKey::Default(Some(key.clone())))?,
        Some(socket.clone())
    );
    let other = SshRemoteTransport::for_target(
        &target("203.0.113.12"),
        ClientAccess {
            key: &ClientKey::Default(Some(key)),
            reuse: &reuse,
            keys: &ManagedKeys::none(),
        },
    )?;
    assert!(other.kept_connection().is_some_and(|path| path != socket));
    Ok(())
}

#[test]
fn keys_that_need_their_owner_and_the_agent_key_connect_anew_every_time() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let reuse = ConnectionReuse::kept_under(&home.path().join("run"));
    let plain = key_file(home.path(), "plain", "none", "ssh-ed25519")?;

    // A security key asks for a touch per connection; keeping one open
    // would let anything running as the user skip the touch.
    let security = key_file(home.path(), "sk", "none", "sk-ssh-ed25519@openssh.com")?;
    assert_eq!(kept(&reuse, &ClientKey::Default(Some(security)))?, None);
    // An encrypted key whose passphrase mm does not keep needs its owner.
    let encrypted = key_file(home.path(), "encrypted", "aes256-ctr", "ssh-ed25519")?;
    assert_eq!(
        kept(&reuse, &ClientKey::Default(Some(encrypted.clone())))?,
        None
    );
    assert_eq!(
        kept_with(
            &reuse,
            &ClientKey::Default(Some(encrypted)),
            &unlocking(&home.path().join("another-key"), home.path())
        )?,
        None
    );
    // A file mm cannot tell, and no file at all.
    let unknown = home.path().join("unknown");
    std::fs::write(&unknown, "not a key\n")?;
    assert_eq!(kept(&reuse, &ClientKey::Default(Some(unknown)))?, None);
    assert_eq!(
        kept(
            &reuse,
            &ClientKey::Default(Some(home.path().join("missing")))
        )?,
        None
    );
    // The agent key is a plain file, and the MCP server never keeps it.
    assert_eq!(kept(&reuse, &ClientKey::Only(plain.clone()))?, None);
    // Reuse turned off keeps nothing.
    assert_eq!(
        kept(&ConnectionReuse::off(), &ClientKey::Default(Some(plain)))?,
        None
    );
    Ok(())
}

#[test]
fn a_directory_others_can_enter_or_a_path_too_long_for_a_socket_keeps_nothing() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let key = key_file(home.path(), "device", "none", "ssh-ed25519")?;

    let open = home.path().join("open");
    std::fs::create_dir_all(open.join("ssh"))?;
    std::fs::set_permissions(open.join("ssh"), std::fs::Permissions::from_mode(0o755))?;
    assert_eq!(
        kept(
            &ConnectionReuse::kept_under(&open),
            &ClientKey::Default(Some(key.clone()))
        )?,
        None
    );

    let linked = home.path().join("linked");
    std::fs::create_dir_all(&linked)?;
    std::fs::create_dir(home.path().join("elsewhere"))?;
    std::os::unix::fs::symlink(home.path().join("elsewhere"), linked.join("ssh"))?;
    assert_eq!(
        kept(
            &ConnectionReuse::kept_under(&linked),
            &ClientKey::Default(Some(key.clone()))
        )?,
        None
    );

    let deep = home.path().join("d".repeat(90));
    assert_eq!(
        kept(
            &ConnectionReuse::kept_under(&deep),
            &ClientKey::Default(Some(key))
        )?,
        None
    );
    assert!(
        !deep.exists(),
        "nothing is created for a path that is too long"
    );
    Ok(())
}

#[test]
fn a_connection_whose_key_file_is_gone_or_replaced_is_closed() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let key = key_file(home.path(), "device", "none", "ssh-ed25519")?;
    let socket = kept(
        &ConnectionReuse::kept_under(&run),
        &ClientKey::Default(Some(key.clone())),
    )?
    .ok_or("kept")?;
    // What `mm` leaves when it connects: the socket (here a plain file
    // standing in for it) and the note of which key made it.
    let owner = socket.with_extension("owner");
    let public = blake3::hash(&[field(b"ssh-ed25519"), field(&[7; 32])].concat())
        .to_hex()
        .to_string();
    let leave = || -> std::io::Result<()> {
        std::fs::write(&socket, "")?;
        std::fs::write(&owner, format!("{public}\n{}\n", key.display()))
    };

    // While the key file is the one that made the connection, it stays.
    leave()?;
    let reuse = mm_reuse(&run);
    assert!(socket.exists() && owner.exists());
    drop(reuse);

    // Another key in the same place: the connection was made with a key
    // that no longer exists.
    key_file(home.path(), "device", "none", "ssh-rsa")?;
    drop(mm_reuse(&run));
    assert!(!socket.exists() && !owner.exists());

    // The key file removed.
    key_file(home.path(), "device", "none", "ssh-ed25519")?;
    leave()?;
    std::fs::remove_file(&key)?;
    drop(mm_reuse(&run));
    assert!(!socket.exists() && !owner.exists());

    // A key or access change closes everything, whatever made it.
    key_file(home.path(), "device", "none", "ssh-ed25519")?;
    leave()?;
    ConnectionReuse::close_all_under(&run);
    assert!(!socket.exists() && !owner.exists());
    Ok(())
}

#[test]
fn the_device_key_under_a_passphrase_mm_keeps_stays_connected_and_the_agent_key_never() -> TestResult
{
    let home = temp_dirs::tempdir()?;
    let run = home.path().join("run");
    let reuse = ConnectionReuse::kept_under(&run);
    let device = key_file(home.path(), "id_ed25519", "aes256-ctr", "ssh-ed25519")?;
    let keys = unlocking(&device, home.path());

    // mm takes the passphrase from the vault itself, so it can use the key
    // with nobody present, and a program running as the user can ask the
    // same vault: the socket gives it nothing the vault does not.
    let socket = kept_with(&reuse, &ClientKey::Default(Some(device.clone())), &keys)?
        .ok_or("the protected device key should keep its connection")?;
    assert_eq!(socket.parent(), Some(run.join("ssh").as_path()));
    // The agent key is unlocked the same way and is never kept: every
    // call of the MCP server authenticates anew.
    let agent = key_file(home.path(), "agent_ed25519", "aes256-ctr", "ssh-ed25519")?;
    assert_eq!(
        kept_with(
            &reuse,
            &ClientKey::Only(agent.clone()),
            &unlocking(&agent, home.path())
        )?,
        None
    );
    // A security key is a security key whoever manages it.
    let security = key_file(home.path(), "sk", "none", "sk-ssh-ed25519@openssh.com")?;
    assert_eq!(
        kept_with(
            &reuse,
            &ClientKey::Default(Some(security.clone())),
            &unlocking(&security, home.path())
        )?,
        None
    );

    // What mm leaves when it connects with the protected key: the note
    // says the key was one mm unlocks, so the connection survives the
    // look every later command takes, and is listed with its host.
    let owner = socket.with_extension("owner");
    let public = blake3::hash(&[field(b"ssh-ed25519"), field(&[7; 32])].concat())
        .to_hex()
        .to_string();
    std::fs::write(&socket, "")?;
    std::fs::write(
        &owner,
        format!(
            "{public}\n{}\nmm@203.0.113.11:22\nmanaged\n",
            device.display()
        ),
    )?;
    drop(mm_reuse(&run));
    assert!(socket.exists() && owner.exists());
    let listed = ConnectionReuse::list_under(&run);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].peer.as_deref(), Some("mm@203.0.113.11:22"));
    assert_eq!(listed[0].key.as_deref(), Some(device.as_path()));
    // A plain file stands in for the socket: nothing answers behind it.
    assert!(!listed[0].open);

    // The same note without the mark is a connection made with a key
    // file as it was: once that file is encrypted, it is closed.
    std::fs::write(&owner, format!("{public}\n{}\n", device.display()))?;
    drop(mm_reuse(&run));
    assert!(!socket.exists() && !owner.exists());
    assert!(ConnectionReuse::list_under(&run).is_empty());
    Ok(())
}

/// What every remote command does first: look at what is kept.
fn mm_reuse(run: &Path) -> ConnectionReuse {
    ConnectionReuse::under(run)
}
