//! mm-managed SSH keys are encrypted at rest, their passphrases live in a
//! vault, and only mm's own authentication releases one.
//!
//! Every key here is a throwaway made in a temporary home with the real
//! `ssh-keygen`. No test asks a real keychain: the keychain is a fake in
//! memory, or a stand-in `secret-tool` on `PATH`.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use mm::adapters::keys::{FileKeyVault, ManagedKeys, OpenSshKeyFiles};
use mm::app::ports::{KeptConnections, KeyEntry, KeyFiles, KeyUnlock, KeyVault, NewKeyProtection};
use mm::app::{KeyService, KeyServiceDependencies, VaultPreference};
use mm::domain::diagnostic::Severity;
use mm::domain::key::{
    KeyChange, KeyChangePlan, KeyChangeStep, KeyFindings, KeyPassphrase, KeyProtection,
    KeyVaultKind, ManagedKey, ManagedKeyKind, PrivateKeyHeader, ed25519_private_key,
    openssh_version,
};

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A keychain in memory: what the Secret Service is to mm.
#[derive(Default)]
struct MemoryKeychain {
    entries: Mutex<BTreeMap<String, KeyPassphrase>>,
    unavailable: AtomicBool,
}

impl MemoryKeychain {
    fn fingerprints(&self) -> Vec<String> {
        self.entries
            .lock()
            .map(|entries| entries.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn passphrases(&self) -> Vec<KeyPassphrase> {
        self.entries
            .lock()
            .map(|entries| entries.values().cloned().collect())
            .unwrap_or_default()
    }

    fn forget_everything(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

impl KeyVault for MemoryKeychain {
    fn kind(&self) -> KeyVaultKind {
        KeyVaultKind::Keychain
    }

    fn available(&self) -> bool {
        !self.unavailable.load(Ordering::Relaxed)
    }

    fn store(&self, entry: &KeyEntry<'_>, passphrase: &KeyPassphrase) -> Result<(), mm::MmError> {
        if !self.available() {
            return Err(mm::MmError::Security {
                message: "the keychain does not answer".to_owned(),
            });
        }
        self.entries
            .lock()
            .map_err(|_| mm::MmError::State {
                message: "poisoned".to_owned(),
            })?
            .insert(entry.fingerprint.to_owned(), passphrase.clone());
        Ok(())
    }

    fn fetch(&self, entry: &KeyEntry<'_>) -> Result<Option<KeyPassphrase>, mm::MmError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| mm::MmError::State {
                message: "poisoned".to_owned(),
            })?
            .get(entry.fingerprint)
            .cloned())
    }

    fn contains(&self, entry: &KeyEntry<'_>) -> Result<bool, mm::MmError> {
        Ok(self.fetch(entry)?.is_some())
    }

    fn remove(&self, entry: &KeyEntry<'_>) -> Result<(), mm::MmError> {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(entry.fingerprint);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Closed(AtomicUsize);

impl KeptConnections for Closed {
    fn close_all(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

struct Device {
    home: tempfile::TempDir,
    keychain: Arc<MemoryKeychain>,
    closed: Arc<Closed>,
    keys: Arc<KeyService>,
}

impl Device {
    fn new(preference: VaultPreference) -> std::io::Result<Self> {
        let home = temp_dirs::tempdir()?;
        let keychain = Arc::new(MemoryKeychain::default());
        let closed = Arc::new(Closed::default());
        let keys = Arc::new(KeyService::new(KeyServiceDependencies {
            files: Arc::new(OpenSshKeyFiles::new(home.path().join("run"))),
            keychain: keychain.clone(),
            file_vault: Arc::new(FileKeyVault),
            connections: closed.clone(),
            directory: home.path().join("identities/ssh"),
            preference,
            owner: "op@laptop".to_owned(),
        }));
        Ok(Self {
            home,
            keychain,
            closed,
            keys,
        })
    }

    fn key_path(&self, kind: ManagedKeyKind) -> PathBuf {
        self.keys.path(kind)
    }

    fn passphrase_file(&self, kind: ManagedKeyKind) -> PathBuf {
        self.home
            .path()
            .join("identities/ssh")
            .join(format!("{}.passphrase", kind.file_name()))
    }

    /// A plain key as mm made them before it protected keys.
    fn plain_key(&self, kind: ManagedKeyKind) -> TestResult {
        let path = self.key_path(kind);
        std::fs::create_dir_all(path.parent().ok_or("parent")?)?;
        let made = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "mm:op@laptop", "-f"])
            .arg(&path)
            .status()?;
        assert!(made.success());
        Ok(())
    }
}

fn header(path: &Path) -> Result<PrivateKeyHeader, Box<dyn std::error::Error>> {
    Ok(PrivateKeyHeader::parse(&std::fs::read_to_string(path)?).ok_or("not an OpenSSH key")?)
}

/// What `ssh -o BatchMode=yes -i <key>` does first: load the key with no
/// passphrase and nobody to ask.
fn loads_without_a_passphrase(path: &Path) -> std::io::Result<bool> {
    Ok(Command::new("ssh-keygen")
        .args(["-y", "-P", "", "-f"])
        .arg(path)
        .env_remove("SSH_ASKPASS")
        .output()?
        .status
        .success())
}

/// Every file under `root` that contains `needle`.
fn files_containing(root: &Path, needle: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            let kind = std::fs::symlink_metadata(&path)?.file_type();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file()
                && String::from_utf8_lossy(&std::fs::read(&path)?).contains(needle)
            {
                found.push(path);
            }
        }
    }
    Ok(found)
}

#[test]
fn a_new_key_is_encrypted_from_the_start_and_its_passphrase_is_only_in_the_vault() -> TestResult {
    let device = Device::new(VaultPreference::Auto)?;
    let created = device.keys.create(ManagedKeyKind::Device, false)?;

    assert_eq!(
        created.key.protection,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::Keychain
        }
    );
    assert!(created.notes.is_empty(), "{:?}", created.notes);
    assert!(created.key.public_key.starts_with("ssh-ed25519 "));
    assert!(created.key.public_key.ends_with(" mm:op@laptop"));
    let path = device.key_path(ManagedKeyKind::Device);
    assert!(header(&path)?.encrypted);
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o600
    );
    // The file alone opens nothing.
    assert!(!loads_without_a_passphrase(&path)?);
    // The vault names the entry after the key, and the passphrase is in
    // no file under the home: not beside the key, not in a helper's
    // leftovers.
    assert_eq!(
        device.keychain.fingerprints(),
        vec![created.key.fingerprint]
    );
    let passphrase = device.keychain.passphrases().remove(0);
    assert_eq!(passphrase.expose().len(), 43);
    assert_eq!(
        files_containing(device.home.path(), passphrase.expose())?,
        Vec::<PathBuf>::new()
    );
    assert!(!device.passphrase_file(ManagedKeyKind::Device).exists());
    // Of the passphrase helper only its script stays, which holds nothing.
    let left = std::fs::read_dir(device.home.path().join("run"))?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(left, vec!["askpass".to_owned()]);
    assert!(
        !std::fs::read_to_string(device.home.path().join("run/askpass"))?
            .contains(passphrase.expose())
    );
    // A second key is refused, with the commands that show or replace it.
    let again = device.keys.create(ManagedKeyKind::Device, false);
    assert!(
        matches!(&again, Err(mm::MmError::Environment { message })
            if message.contains("`mm key show`") && message.contains("`mm key rotate`")),
        "{again:?}"
    );
    Ok(())
}

#[test]
fn protecting_an_older_plain_key_keeps_its_public_key() -> TestResult {
    let device = Device::new(VaultPreference::Auto)?;
    device.plain_key(ManagedKeyKind::Device)?;
    let path = device.key_path(ManagedKeyKind::Device);
    assert!(loads_without_a_passphrase(&path)?);
    let before = device
        .keys
        .key(ManagedKeyKind::Device)?
        .ok_or("the plain key is there")?;
    assert_eq!(before.protection, KeyProtection::Unprotected);

    // `mm doctor` names it and the command.
    let findings = device.keys.diagnostics()?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "MM_KEY_UNPROTECTED");
    assert!(
        findings[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("`mm key protect`"))
    );

    // The step is reviewed: it applies only under its own hash.
    let plan = device.keys.protect_plan()?;
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].change, KeyChange::Encrypt);
    assert!(plan.steps[0].keeps_access);
    let refused = device.keys.protect("0000");
    assert!(
        matches!(&refused, Err(mm::MmError::Security { message }) if message.contains("mm key protect")),
        "{refused:?}"
    );
    assert!(
        loads_without_a_passphrase(&path)?,
        "a refused plan changed the key"
    );

    let protected = device.keys.protect(&plan.plan_hash)?;
    assert_eq!(protected.len(), 1);
    // Same key: no host has to learn anything.
    assert_eq!(protected[0].public_key, before.public_key);
    assert_eq!(protected[0].fingerprint, before.fingerprint);
    assert_eq!(
        protected[0].protection,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::Keychain
        }
    );
    assert!(header(&path)?.encrypted);
    assert!(!loads_without_a_passphrase(&path)?);
    assert_eq!(device.closed.0.load(Ordering::Relaxed), 1);
    // The passphrase in the vault is the one that opens the key: changing
    // the passphrase needs the old one.
    let passphrase = device.keychain.passphrases().remove(0);
    let files = OpenSshKeyFiles::new(device.home.path().join("run"));
    files.encrypt(&path, Some(&passphrase), &passphrase)?;
    let wrong = KeyPassphrase::from_random(&[9; 32]);
    assert!(files.encrypt(&path, Some(&wrong), &passphrase).is_err());

    // Nothing is left to protect.
    assert!(device.keys.protect_plan()?.steps.is_empty());
    assert!(device.keys.diagnostics()?.is_empty());
    Ok(())
}

#[test]
fn rotating_and_removing_a_key_keep_the_vault_in_step() -> TestResult {
    let device = Device::new(VaultPreference::Auto)?;
    let first = device.keys.create(ManagedKeyKind::Agent, false)?.key;
    let first_passphrase = device.keychain.passphrases().remove(0);

    // The new key is made beside the current one, which keeps working.
    let plan = device.keys.rotate_plan(ManagedKeyKind::Agent)?;
    assert_eq!(plan.steps[0].change, KeyChange::Stage);
    assert!(plan.steps[0].keeps_access);
    assert!(device.keys.rotate(ManagedKeyKind::Agent, "0000").is_err());
    let staged = device
        .keys
        .rotate(ManagedKeyKind::Agent, &plan.plan_hash)?
        .key;
    assert_ne!(staged.fingerprint, first.fingerprint);
    assert_eq!(device.keys.key(ManagedKeyKind::Agent)?, Some(first.clone()));
    assert_eq!(device.keychain.fingerprints().len(), 2);
    let unlock = device.keys.unlock(ManagedKeyKind::Agent);
    assert!(unlock.manages(&staged.path));
    assert!(unlock.passphrase(&staged.path).is_ok());
    assert!(unlock.passphrase(&first.path).is_ok());
    // A second rotation waits for the first to finish.
    assert!(device.keys.rotate_plan(ManagedKeyKind::Agent).is_err());

    // Finishing it puts the new key in place and forgets the old one.
    let finish = device.keys.finish_plan(ManagedKeyKind::Agent)?;
    assert_eq!(finish.steps[0].change, KeyChange::Finish);
    assert!(!finish.steps[0].keeps_access);
    assert!(device.keys.finish(ManagedKeyKind::Agent, "0000").is_err());
    let rotated = device
        .keys
        .finish(ManagedKeyKind::Agent, &finish.plan_hash)?
        .key;
    assert_eq!(rotated.fingerprint, staged.fingerprint);
    assert_eq!(rotated.path, first.path);
    assert!(rotated.public_key.ends_with(" mm-agent:op@laptop"));
    // One entry, the new key's, under a new passphrase.
    assert_eq!(
        device.keychain.fingerprints(),
        vec![rotated.fingerprint.clone()]
    );
    assert_ne!(device.keychain.passphrases().remove(0), first_passphrase);
    assert!(device.keys.next_key(ManagedKeyKind::Agent)?.is_none());
    // The review of a rotation is of the key that was there: it does not
    // apply to the new one.
    assert!(
        device
            .keys
            .finish(ManagedKeyKind::Agent, &finish.plan_hash)
            .is_err()
    );

    let removal = device.keys.remove_plan(ManagedKeyKind::Agent)?;
    let removed = device
        .keys
        .remove(ManagedKeyKind::Agent, &removal.plan_hash)?;
    assert_eq!(removed.fingerprint, rotated.fingerprint);
    assert!(device.keychain.fingerprints().is_empty());
    assert!(!removed.path.exists());
    assert!(!removed.path.with_extension("pub").exists());
    assert_eq!(device.keys.key(ManagedKeyKind::Agent)?, None);
    // Connections made with the key were closed with each change.
    assert_eq!(device.closed.0.load(Ordering::Relaxed), 4);
    // With nothing to remove, the command says what to run instead.
    let nothing = device.keys.remove_plan(ManagedKeyKind::Agent);
    assert!(
        matches!(&nothing, Err(mm::MmError::Environment { message }) if message.contains("mm key create --agent")),
        "{nothing:?}"
    );
    Ok(())
}

#[test]
fn a_lost_vault_entry_is_reported_and_a_new_key_recovers() -> TestResult {
    let device = Device::new(VaultPreference::Auto)?;
    let lost = device.keys.create(ManagedKeyKind::Device, false)?.key;
    device.keychain.forget_everything();

    let key = device
        .keys
        .key(ManagedKeyKind::Device)?
        .ok_or("the key file is still there")?;
    assert_eq!(
        key.protection,
        KeyProtection::PassphraseMissing {
            vault: KeyVaultKind::Keychain
        }
    );
    assert!(key.protection_summary().contains("`mm key rotate`"));
    let findings = device.keys.diagnostics()?;
    assert_eq!(findings[0].code, "MM_KEY_PASSPHRASE_MISSING");
    assert_eq!(findings[0].severity, Severity::Error);
    // Authenticating fails with the recovery, not with a prompt.
    let unlock = device.keys.unlock(ManagedKeyKind::Device);
    let refused = unlock.passphrase(&device.key_path(ManagedKeyKind::Device));
    assert!(
        matches!(&refused, Err(mm::MmError::Secret { message })
            if message.contains("its passphrase is not in the system keychain")
                && message.contains("`mm key rotate`")
                && message.contains("`mm key show`")),
        "{refused:?}"
    );

    // Recovery: a new key. It needs no passphrase of the lost one.
    let plan = device.keys.rotate_plan(ManagedKeyKind::Device)?;
    let recovered = device
        .keys
        .rotate(ManagedKeyKind::Device, &plan.plan_hash)?
        .key;
    assert_ne!(recovered.fingerprint, lost.fingerprint);
    assert_eq!(
        recovered.protection,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::Keychain
        }
    );
    assert!(
        unlock
            .passphrase(&device.key_path(ManagedKeyKind::Device))
            .is_ok()
    );
    assert!(device.keys.diagnostics()?.is_empty());
    Ok(())
}

#[test]
fn without_a_keychain_the_passphrase_goes_to_a_file_and_moves_when_one_appears() -> TestResult {
    let device = Device::new(VaultPreference::Auto)?;
    device.keychain.unavailable.store(true, Ordering::Relaxed);
    let created = device.keys.create(ManagedKeyKind::Device, false)?;
    assert_eq!(
        created.key.protection,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::File
        }
    );
    // mm says what that is worth.
    assert!(
        created
            .notes
            .iter()
            .any(|note| note.contains("no system keychain")
                && note.contains("does not stop a program that reads both files")),
        "{:?}",
        created.notes
    );
    let file = device.passphrase_file(ManagedKeyKind::Device);
    assert_eq!(
        std::fs::metadata(&file)?.permissions().mode() & 0o777,
        0o600
    );
    assert!(std::fs::read_to_string(&file)?.starts_with(&created.key.fingerprint));
    assert!(!loads_without_a_passphrase(
        &device.key_path(ManagedKeyKind::Device)
    )?);
    assert!(device.keychain.fingerprints().is_empty());
    let unlock = device.keys.unlock(ManagedKeyKind::Device);
    let in_file = unlock.passphrase(&device.key_path(ManagedKeyKind::Device))?;
    // On a machine that stays without a keychain, doctor only informs.
    let findings = device.keys.diagnostics()?;
    assert_eq!(findings[0].code, "MM_KEY_PASSPHRASE_IN_FILE");
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(device.keys.protect_plan()?.steps.is_empty());

    // A keychain answers now: doctor points at the move, and the move
    // takes the passphrase out of the file. The key does not change.
    device.keychain.unavailable.store(false, Ordering::Relaxed);
    let findings = device.keys.diagnostics()?;
    assert_eq!(findings[0].severity, Severity::Warning);
    let plan = device.keys.protect_plan()?;
    assert_eq!(plan.steps[0].change, KeyChange::MoveToKeychain);
    let moved = device.keys.protect(&plan.plan_hash)?;
    assert_eq!(moved[0].fingerprint, created.key.fingerprint);
    assert_eq!(
        moved[0].protection,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::Keychain
        }
    );
    assert!(!file.exists());
    assert_eq!(
        unlock.passphrase(&device.key_path(ManagedKeyKind::Device))?,
        in_file
    );
    Ok(())
}

#[test]
fn each_identity_opens_its_own_key_and_no_other() -> TestResult {
    let device = Device::new(VaultPreference::Auto)?;
    device.keys.create(ManagedKeyKind::Device, false)?;
    device.keys.create(ManagedKeyKind::Agent, false)?;
    let device_key = device.key_path(ManagedKeyKind::Device);
    let agent_key = device.key_path(ManagedKeyKind::Agent);

    // What the MCP server is given opens the agent key only.
    let agent = device.keys.unlock(ManagedKeyKind::Agent);
    assert!(agent.manages(&agent_key));
    assert!(!agent.manages(&device_key));
    assert!(agent.passphrase(&agent_key).is_ok());
    let refused = agent.passphrase(&device_key);
    assert!(
        matches!(&refused, Err(mm::MmError::Security { .. })),
        "{refused:?}"
    );
    // And what the user's commands are given opens the device key only.
    let user = device.keys.unlock(ManagedKeyKind::Device);
    assert!(user.passphrase(&device_key).is_ok());
    assert!(user.passphrase(&agent_key).is_err());
    assert_ne!(
        user.passphrase(&device_key)?,
        agent.passphrase(&agent_key)?,
        "each key has a passphrase of its own"
    );
    Ok(())
}

#[test]
fn the_file_preference_never_asks_a_keychain_and_a_bad_value_is_named() -> TestResult {
    assert_eq!(VaultPreference::parse(None), Ok(VaultPreference::Auto));
    assert_eq!(
        VaultPreference::parse(Some("file")),
        Ok(VaultPreference::File)
    );
    assert_eq!(
        VaultPreference::parse(Some("keychain")),
        Ok(VaultPreference::Keychain)
    );
    let refused = VaultPreference::parse(Some("vault"));
    assert!(
        refused.as_ref().is_err_and(
            |message| message.contains("MM_KEY_VAULT") && message.contains("keychain, file")
        ),
        "{refused:?}"
    );

    let device = Device::new(VaultPreference::File)?;
    let created = device.keys.create(ManagedKeyKind::Device, false)?;
    assert_eq!(
        created.key.protection,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::File
        }
    );
    // Chosen, so not explained.
    assert!(created.notes.is_empty());
    assert!(device.passphrase_file(ManagedKeyKind::Device).exists());
    // Doctor gives the choice as the reason, not a missing keychain: the
    // device's keychain answers and was never asked.
    let findings = device.keys.diagnostics()?;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, "MM_KEY_PASSPHRASE_IN_FILE");
    assert!(
        findings[0]
            .message
            .contains("because MM_KEY_VAULT=file is set"),
        "{}",
        findings[0].message
    );
    assert!(!findings[0].message.contains("no system keychain"));

    // With the keychain required and not answering, nothing is created.
    let strict = Device::new(VaultPreference::Keychain)?;
    strict.keychain.unavailable.store(true, Ordering::Relaxed);
    assert!(strict.keys.create(ManagedKeyKind::Device, false).is_err());
    assert!(!strict.key_path(ManagedKeyKind::Device).exists());
    assert!(
        !strict
            .key_path(ManagedKeyKind::Device)
            .with_extension("pub")
            .exists()
    );
    Ok(())
}

// ---- the binary -----------------------------------------------------------

/// A stand-in for libsecret's `secret-tool`: keeps each secret in a file
/// under `$FAKE_KEYRING` and writes every command line it is run with to
/// `calls`. The secret arrives on standard input, as with the real tool.
const FAKE_SECRET_TOOL: &str = r#"#!/bin/sh
action=$1; shift
printf '%s\n' "$action $*" >> "$FAKE_KEYRING/calls"
if [ "$action" = store ]; then shift 2; fi
name=$(printf '%s' "$4" | tr '/+:' '___')
case "$action" in
  store) cat > "$FAKE_KEYRING/$name" ;;
  lookup) [ -f "$FAKE_KEYRING/$name" ] || exit 1; cat "$FAKE_KEYRING/$name" ;;
  clear) rm -f "$FAKE_KEYRING/$name" ;;
esac
"#;

/// A stand-in for `ssh`. It writes its arguments and the names and values
/// of its environment to files, asks the passphrase helper as `ssh` would
/// for an encrypted key, and keeps what the helper answered. Then it ends,
/// as a host that closes the connection does.
const FAKE_SSH: &str = r#"#!/bin/sh
printf '%s\n' "$@" > "$FAKE_SSH/arguments"
env > "$FAKE_SSH/environment"
if [ -n "${SSH_ASKPASS:-}" ]; then
  ls -ld "$MM_ASKPASS_DIR" | cut -c1-10 > "$FAKE_SSH/helper-directory"
  "$SSH_ASKPASS" "Enter passphrase for key '/somewhere/id_ed25519': " > "$FAKE_SSH/answer"
  "$SSH_ASKPASS" "Enter passphrase for key '/somewhere/id_ed25519': " > "$FAKE_SSH/second-answer"
fi
echo "Connection closed by remote host" >&2
exit 255
"#;

struct Workstation {
    root: tempfile::TempDir,
}

impl Workstation {
    fn new() -> std::io::Result<Self> {
        let root = temp_dirs::tempdir()?;
        for directory in ["home", "bin", "keyring", "ssh", "project/.config"] {
            std::fs::create_dir_all(root.path().join(directory))?;
        }
        Ok(Self { root })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn tool(&self, name: &str, script: &str) -> std::io::Result<()> {
        let path = self.path("bin").join(name);
        std::fs::write(&path, script)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
    }

    /// `mm` as the user runs it on a desktop whose keychain is the fake.
    fn mm(&self) -> assert_cmd::Command {
        let mut command = temp_dirs::mm();
        command
            .env_remove("MM_KEY_VAULT")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.path("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("HOME", self.path("home"))
            .env("FAKE_KEYRING", self.path("keyring"))
            .env("FAKE_SSH", self.path("ssh"))
            .arg("--mm-home")
            .arg(self.path("home/.mm"));
        command
    }

    fn keyring_entries(&self) -> std::io::Result<Vec<String>> {
        let mut entries = std::fs::read_dir(self.path("keyring"))?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name != "calls")
            .collect::<Vec<_>>();
        entries.sort();
        Ok(entries)
    }

    fn stored_passphrase(&self) -> Result<String, Box<dyn std::error::Error>> {
        let entries = self.keyring_entries()?;
        let [entry] = entries.as_slice() else {
            return Err(format!("expected one keychain entry, found {entries:?}").into());
        };
        Ok(std::fs::read_to_string(self.path("keyring").join(entry))?
            .trim()
            .to_owned())
    }

    fn key(&self) -> PathBuf {
        self.path("home/.mm/identities/ssh/id_ed25519")
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn plan_hash(output: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    let plan: serde_json::Value = serde_json::from_slice(output)?;
    Ok(plan["plan_hash"].as_str().ok_or("plan_hash")?.to_owned())
}

#[test]
fn mm_key_keeps_the_keychain_in_step_and_never_shows_a_passphrase() -> TestResult {
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;

    let created = desk.mm().args(["key", "create"]).assert().success();
    let created = created.get_output();
    let shown = text(&created.stdout);
    assert!(
        shown.contains("protection:  passphrase in the system keychain"),
        "{shown}"
    );
    assert!(shown.contains("This key is for mm only"), "{shown}");
    assert!(header(&desk.key())?.encrypted);
    assert!(!loads_without_a_passphrase(&desk.key())?);
    let passphrase = desk.stored_passphrase()?;
    assert_eq!(passphrase.len(), 43);
    // Not on the terminal, not in an argument of the keychain tool, and
    // in no file of the mm home.
    let mut seen = shown + &text(&created.stderr);
    assert_eq!(
        files_containing(&desk.path("home"), &passphrase)?,
        Vec::<PathBuf>::new()
    );

    let show = desk
        .mm()
        .args(["--output", "json", "key", "show"])
        .assert()
        .success();
    let key: serde_json::Value = serde_json::from_slice(&show.get_output().stdout)?;
    assert_eq!(key["kind"], "device");
    assert_eq!(key["protection"]["state"], "passphrase");
    assert_eq!(key["protection"]["vault"], "keychain");
    let first = key["fingerprint"].as_str().ok_or("fingerprint")?.to_owned();
    seen += &text(&show.get_output().stdout);

    // Rotation: reviewed. The new key waits beside the current one with an
    // entry of its own; finishing it leaves one entry again, for the new key.
    let review = desk.mm().args(["key", "rotate"]).assert().success();
    let review = text(&review.get_output().stdout);
    assert!(
        review.contains("make a new key beside this device's mm key"),
        "{review}"
    );
    assert!(review.contains("Apply: mm key rotate --plan "), "{review}");
    let plan = desk
        .mm()
        .args(["--output", "json", "key", "rotate"])
        .assert()
        .success();
    let hash = plan_hash(&plan.get_output().stdout)?;
    let rotated = desk
        .mm()
        .args(["key", "rotate", "--plan", &hash, "--yes"])
        .assert()
        .success();
    seen += &text(&rotated.get_output().stdout);
    assert!(
        text(&rotated.get_output().stdout)
            .contains("mm offers both, so this device keeps its access"),
        "{}",
        text(&rotated.get_output().stdout)
    );
    assert!(PathBuf::from(format!("{}.next", desk.key().display())).is_file());
    assert_eq!(desk.keyring_entries()?.len(), 2);
    let plan = desk
        .mm()
        .args(["--output", "json", "key", "rotate", "--finish"])
        .assert()
        .success();
    let hash = plan_hash(&plan.get_output().stdout)?;
    let finished = desk
        .mm()
        .args(["key", "rotate", "--finish", "--plan", &hash, "--yes"])
        .assert()
        .success();
    seen += &text(&finished.get_output().stdout);
    assert!(
        text(&finished.get_output().stdout).contains("The old key (SHA256:"),
        "{}",
        text(&finished.get_output().stdout)
    );
    assert!(!PathBuf::from(format!("{}.next", desk.key().display())).exists());
    let second = desk.stored_passphrase()?;
    assert_ne!(second, passphrase);
    assert!(!desk.keyring_entries()?[0].contains(&first.replace([':', '/', '+'], "_")));

    // Removal takes the entry with the key.
    let plan = desk
        .mm()
        .args(["--output", "json", "key", "remove"])
        .assert()
        .success();
    let hash = plan_hash(&plan.get_output().stdout)?;
    let removed = desk
        .mm()
        .args(["key", "remove", "--plan", &hash, "--yes"])
        .assert()
        .success();
    seen += &text(&removed.get_output().stdout);
    assert!(desk.keyring_entries()?.is_empty());
    assert!(!desk.key().exists());

    for secret in [&passphrase, &second] {
        assert!(!seen.contains(secret.as_str()), "a passphrase was printed");
        assert!(
            !std::fs::read_to_string(desk.path("keyring/calls"))?.contains(secret.as_str()),
            "a passphrase was an argument of the keychain tool"
        );
        assert_eq!(
            files_containing(&desk.path("home"), secret)?,
            Vec::<PathBuf>::new()
        );
    }
    Ok(())
}

#[test]
fn an_older_plain_key_is_reported_by_doctor_and_protected_in_place() -> TestResult {
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;
    // The key an earlier mm made: a plain file.
    std::fs::create_dir_all(desk.key().parent().ok_or("parent")?)?;
    assert!(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "mm:op@laptop", "-f"])
            .arg(desk.key())
            .status()?
            .success()
    );
    let public = std::fs::read_to_string(desk.key().with_extension("pub"))?;

    let doctor = desk
        .mm()
        .arg("--root")
        .arg(desk.path("project"))
        .args(["--output", "json", "doctor"])
        .assert();
    let report = text(&doctor.get_output().stdout);
    assert!(report.contains("MM_KEY_UNPROTECTED"), "{report}");
    assert!(report.contains("Run `mm key protect`"), "{report}");

    let show = desk.mm().args(["key", "show"]).assert().success();
    assert!(text(&show.get_output().stdout).contains("protection:  none; run `mm key protect`"));

    let review = desk.mm().args(["key", "protect"]).assert().success();
    let review = text(&review.get_output().stdout);
    assert!(review.contains("encrypt this device's mm key"), "{review}");
    assert!(review.contains("The public key stays the same"), "{review}");
    assert!(
        loads_without_a_passphrase(&desk.key())?,
        "a review changed the key"
    );
    let plan = desk
        .mm()
        .args(["--output", "json", "key", "protect"])
        .assert()
        .success();
    let hash = plan_hash(&plan.get_output().stdout)?;
    desk.mm()
        .args(["key", "protect", "--plan", &hash, "--yes"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(desk.key().with_extension("pub"))?,
        public
    );
    assert!(header(&desk.key())?.encrypted);
    assert!(!loads_without_a_passphrase(&desk.key())?);
    assert_eq!(desk.keyring_entries()?.len(), 1);
    let doctor = desk
        .mm()
        .arg("--root")
        .arg(desk.path("project"))
        .args(["--output", "json", "doctor"])
        .assert();
    assert!(!text(&doctor.get_output().stdout).contains("MM_KEY_"));
    let nothing = desk.mm().args(["key", "protect"]).assert().success();
    assert!(text(&nothing.get_output().stdout).contains("Nothing to do"));
    Ok(())
}

#[test]
fn an_agent_in_a_shell_gets_no_key_command_at_all() -> TestResult {
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;
    desk.mm().args(["key", "create"]).assert().success();
    desk.mm()
        .args(["key", "create", "--agent"])
        .assert()
        .success();
    let before = desk.keyring_entries()?;
    let calls = std::fs::read_to_string(desk.path("keyring/calls"))?;

    for arguments in [
        vec!["key", "show"],
        vec!["key", "show", "--agent"],
        vec!["key", "protect"],
        vec!["key", "rotate"],
        vec!["key", "rotate", "--agent"],
        vec!["key", "remove"],
        vec!["key", "create"],
        vec!["target", "connections"],
        vec!["target", "connections", "--close"],
        vec!["doctor"],
        vec!["team", "whoami"],
    ] {
        let refused = desk
            .mm()
            .env("CLAUDECODE", "1")
            .args(&arguments)
            .assert()
            .failure();
        let said = text(&refused.get_output().stderr);
        assert!(
            said.contains("mm does not run for an AI agent"),
            "{arguments:?}: {said}"
        );
    }
    // Refused before anything was read: the keychain was not asked once.
    assert_eq!(desk.keyring_entries()?, before);
    assert_eq!(std::fs::read_to_string(desk.path("keyring/calls"))?, calls);
    Ok(())
}

const REMOTE_PROJECT: &str = r"schema-version: 1
default-environment: staging
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de4
  name: key-live
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    project-name: key-live-{environment}-{stack}
    files: [compose.yaml]
modules:
  api:
    path: api
    services:
      api:
        stack: app
targets:
  remote:
    host: 203.0.113.11
    host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
    daemon-fingerprint: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
environments:
  local: {}
  staging:
    target: remote
";

#[test]
fn a_connection_gets_the_passphrase_through_the_helper_and_nowhere_else() -> TestResult {
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;
    desk.tool("ssh", FAKE_SSH)?;
    std::fs::write(desk.path("project/.config/mm.yaml"), REMOTE_PROJECT)?;
    std::fs::create_dir_all(desk.path("project/api"))?;
    std::fs::write(
        desk.path("project/compose.yaml"),
        "services:\n  api:\n    image: busybox:1.36.1\n",
    )?;
    desk.mm().args(["key", "create"]).assert().success();
    let passphrase = desk.stored_passphrase()?;

    let whoami = desk
        .mm()
        .arg("--root")
        .arg(desk.path("project"))
        .args(["--runtime", "docker-compose", "--environment", "staging"])
        .args(["team", "whoami"])
        .env("MM_TIMING", "1")
        .env("MM_SSH_REUSE", "off")
        .env("MM_LOG", "debug")
        .env("RUST_LOG", "debug")
        .assert()
        .failure();
    let output = whoami.get_output();
    let said = text(&output.stdout) + &text(&output.stderr);

    // `ssh` was started for the device key, allowed to ask once, and the
    // helper gave it the passphrase the keychain holds. A second question
    // gets nothing.
    let arguments = std::fs::read_to_string(desk.path("ssh/arguments"))?;
    assert!(arguments.contains("BatchMode=no"), "{arguments}");
    assert!(
        arguments.contains("NumberOfPasswordPrompts=1"),
        "{arguments}"
    );
    assert!(arguments.contains("IdentityAgent=none"), "{arguments}");
    assert!(
        arguments.contains(&desk.key().display().to_string()),
        "{arguments}"
    );
    assert_eq!(
        std::fs::read_to_string(desk.path("ssh/answer"))?.trim(),
        passphrase
    );
    assert_eq!(std::fs::read_to_string(desk.path("ssh/second-answer"))?, "");
    // The pipes are in a directory only the user can enter.
    assert_eq!(
        std::fs::read_to_string(desk.path("ssh/helper-directory"))?.trim(),
        "drwx------"
    );
    // Never an argument, never in the environment of the process mm
    // started, never in what mm printed or logged, and gone from the
    // helper's directory, which is gone itself.
    assert!(!arguments.contains(&passphrase));
    let environment = std::fs::read_to_string(desk.path("ssh/environment"))?;
    assert!(!environment.contains(&passphrase));
    assert!(
        environment.contains("SSH_ASKPASS_REQUIRE=force"),
        "{environment}"
    );
    assert!(!said.contains(&passphrase), "the passphrase was printed");
    assert_eq!(
        files_containing(&desk.path("home"), &passphrase)?,
        Vec::<PathBuf>::new()
    );
    let helpers = std::fs::read_dir(desk.path("home/.mm/run"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("askpass-"))
        .count();
    assert_eq!(helpers, 0, "a passphrase helper outlived its connection");
    // The trace shows the vault being asked, once.
    assert_eq!(said.matches("key.unlock").count(), 1, "{said}");

    // With the entry gone, mm says how to recover instead of starting ssh.
    std::fs::remove_file(desk.path("ssh/arguments"))?;
    for entry in desk.keyring_entries()? {
        std::fs::remove_file(desk.path("keyring").join(entry))?;
    }
    let lost = desk
        .mm()
        .arg("--root")
        .arg(desk.path("project"))
        .args(["--runtime", "docker-compose", "--environment", "staging"])
        .args(["team", "whoami"])
        .env("MM_SSH_REUSE", "off")
        .assert()
        .failure();
    let said = text(&lost.get_output().stderr);
    assert!(
        said.contains("its passphrase is not in the system keychain"),
        "{said}"
    );
    assert!(said.contains("`mm key rotate`"), "{said}");
    assert!(
        !desk.path("ssh/arguments").exists(),
        "ssh was started without a passphrase"
    );
    Ok(())
}

#[test]
fn a_headless_machine_keeps_the_passphrase_in_a_file_and_says_so() -> TestResult {
    let desk = Workstation::new()?;
    // No `secret-tool` on PATH and MM_KEY_VAULT unset: nothing to ask. The
    // fake is deliberately absent; a real one is shadowed by a tool that
    // fails the way a machine without a session bus does.
    desk.tool(
        "secret-tool",
        "#!/bin/sh\necho 'Cannot autolaunch D-Bus without X11 $DISPLAY' >&2\nexit 1\n",
    )?;
    let created = desk.mm().args(["key", "create"]).assert().success();
    let shown = text(&created.get_output().stdout);
    assert!(
        shown.contains("protection:  passphrase in a file beside the key"),
        "{shown}"
    );
    assert!(
        shown.contains("This machine has no system keychain"),
        "{shown}"
    );
    assert!(
        shown.contains("does not stop a program that reads both files"),
        "{shown}"
    );
    assert!(header(&desk.key())?.encrypted);
    assert!(!loads_without_a_passphrase(&desk.key())?);
    let file = desk.key().with_extension("passphrase");
    assert_eq!(
        std::fs::metadata(&file)?.permissions().mode() & 0o777,
        0o600
    );
    Ok(())
}

// ---- the git deploy key ---------------------------------------------------

/// The public key OpenSSH derives from a private key it is given on
/// standard input.
fn openssh_public_key(private: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Write as _;

    let mut child = Command::new("ssh-keygen")
        .args(["-y", "-P", "", "-f", "/dev/stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child.stdin.take().ok_or("stdin")?.write_all(private)?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "ssh-keygen refused the key: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[tokio::test]
async fn a_deploy_key_is_made_in_memory_and_openssh_reads_it() -> TestResult {
    use mm::app::ports::ClusterNodeShell as _;

    let shell = mm::adapters::target::SshClusterNodeShell::new(
        Arc::new(mm::adapters::process::TokioProcessRunner),
        mm::adapters::keys::ManagedKeys::none(),
    );
    let (private, public) = shell
        .generate_deploy_key("mm-deploy-shop-production")
        .await?;
    assert!(public.starts_with("ssh-ed25519 "), "{public}");
    assert!(public.ends_with(" mm-deploy-shop-production"), "{public}");
    // OpenSSH reads the key mm wrote and derives the same public half.
    assert_eq!(openssh_public_key(&private)?, public);
    // And mm reads it back without OpenSSH and without a file.
    assert_eq!(shell.public_key_of(&private).await?, public);
    let (another, another_public) = shell
        .generate_deploy_key("mm-deploy-shop-production")
        .await?;
    assert_ne!(another_public, public);
    assert_ne!(another.as_slice(), private.as_slice());

    // A key in another format is derived by OpenSSH, from standard input.
    let scratch = temp_dirs::tempdir()?;
    let pem = scratch.path().join("pem");
    assert!(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ecdsa", "-m", "PEM", "-N", "", "-C", "", "-f"])
            .arg(&pem)
            .status()?
            .success()
    );
    let derived = shell.public_key_of(&std::fs::read(&pem)?).await?;
    assert!(derived.starts_with("ecdsa-sha2-nistp256 "), "{derived}");
    assert_eq!(
        derived.split_whitespace().nth(1),
        std::fs::read_to_string(pem.with_extension("pub"))?
            .split_whitespace()
            .nth(1)
    );

    // A deploy key under a passphrase is no use to a host, and is refused
    // by name rather than tried.
    let device = Device::new(VaultPreference::File)?;
    device.keys.create(ManagedKeyKind::Device, false)?;
    let encrypted = std::fs::read(device.key_path(ManagedKeyKind::Device))?;
    let refused = shell.public_key_of(&encrypted).await;
    assert!(
        matches!(&refused, Err(mm::MmError::Security { message }) if message.contains("passphrase")),
        "{refused:?}"
    );
    Ok(())
}

// ---- the types, with no key at all ----------------------------------------

fn managed_key(kind: ManagedKeyKind, protection: KeyProtection) -> ManagedKey {
    ManagedKey {
        kind,
        path: PathBuf::from("/srv/mm-home/identities/ssh").join(kind.file_name()),
        public_key: "ssh-ed25519 AAAA mm:op@host".to_owned(),
        fingerprint: "SHA256:abc".to_owned(),
        protection,
    }
}

#[test]
fn a_generated_passphrase_is_long_plain_and_never_printed() {
    let passphrase = KeyPassphrase::from_random(&[0xA5; 32]);
    assert_eq!(passphrase.expose().len(), 43);
    assert!(
        passphrase
            .expose()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    );
    assert_eq!(format!("{passphrase:?}"), "KeyPassphrase([REDACTED])");
    // What a vault returns is taken only when mm could have written it.
    let stored = zeroize::Zeroizing::new(format!("{}\n", passphrase.expose()));
    assert_eq!(KeyPassphrase::from_vault(stored), Ok(passphrase));
    for damaged in ["", "short", "has a space in the middle of this long value!"] {
        assert!(KeyPassphrase::from_vault(zeroize::Zeroizing::new(damaged.to_owned())).is_err());
    }
}

#[test]
fn a_written_ed25519_key_reads_back_with_its_public_half_and_comment() {
    let (private, public) = ed25519_private_key(&[1; 32], &[2; 32], "mm-deploy-shop-prod", 7);
    let header = PrivateKeyHeader::parse(&private);
    assert_eq!(
        header.as_ref().map(|header| header.key_type.as_str()),
        Some("ssh-ed25519")
    );
    let header = header.unwrap_or_else(|| PrivateKeyHeader {
        key_type: String::new(),
        public_blob: Vec::new(),
        encrypted: true,
        comment: None,
    });
    assert!(!header.encrypted);
    assert!(!header.is_security_key());
    assert_eq!(header.comment.as_deref(), Some("mm-deploy-shop-prod"));
    assert_eq!(
        format!("{} mm-deploy-shop-prod", header.public_line()),
        public
    );
    // Lines of the armored body stay within what OpenSSH writes.
    assert!(private.lines().all(|line| line.len() <= 70));
    assert_eq!(PrivateKeyHeader::parse("not a key"), None);
}

#[test]
fn a_reviewed_plan_applies_only_under_its_own_hash() {
    let step = |change| KeyChangeStep {
        kind: ManagedKeyKind::Device,
        fingerprint: "SHA256:abc".to_owned(),
        change,
        vault: Some(KeyVaultKind::Keychain),
        keeps_access: change == KeyChange::Encrypt,
    };
    let protect = KeyChangePlan::new(vec![step(KeyChange::Encrypt)]);
    let rotate = KeyChangePlan::new(vec![step(KeyChange::Replace)]);
    assert_ne!(protect.plan_hash, rotate.plan_hash);
    assert_eq!(protect.check(&protect.plan_hash, "mm key protect"), Ok(()));
    let refused = protect.check(&rotate.plan_hash, "mm key protect");
    assert!(
        refused
            .as_ref()
            .is_err_and(|message| message.contains("`mm key protect`")),
        "{refused:?}"
    );
    assert!(
        protect.steps[0]
            .summary()
            .contains("public key stays the same")
    );
    assert!(
        rotate.steps[0]
            .summary()
            .contains("until an admin registers")
    );
}

#[test]
fn doctor_names_each_unprotected_key_and_the_command_that_fixes_it() {
    let keys = [
        managed_key(ManagedKeyKind::Device, KeyProtection::Unprotected),
        managed_key(
            ManagedKeyKind::Agent,
            KeyProtection::PassphraseMissing {
                vault: KeyVaultKind::Keychain,
            },
        ),
    ];
    let findings = KeyFindings {
        keys: &keys,
        keychain_available: true,
        keychain_remedy: None,
        file_chosen: false,
        cannot_protect: None,
    }
    .diagnostics();
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].code, "MM_KEY_UNPROTECTED");
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].message.contains("this device's mm key"));
    assert!(
        findings[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("`mm key protect`"))
    );
    assert_eq!(findings[1].code, "MM_KEY_PASSPHRASE_MISSING");
    assert_eq!(findings[1].severity, Severity::Error);
    assert!(
        findings[1]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("`mm key rotate --agent`"))
    );
}

#[test]
fn doctor_is_quiet_about_protected_keys_and_plain_about_a_passphrase_file() {
    let protected = [
        managed_key(
            ManagedKeyKind::Device,
            KeyProtection::Passphrase {
                vault: KeyVaultKind::Keychain,
            },
        ),
        managed_key(ManagedKeyKind::Agent, KeyProtection::SecurityKey),
    ];
    assert!(
        KeyFindings {
            keys: &protected,
            keychain_available: true,
            keychain_remedy: None,
            file_chosen: false,
            cannot_protect: None,
        }
        .diagnostics()
        .is_empty()
    );
    let in_file = [managed_key(
        ManagedKeyKind::Device,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::File,
        },
    )];
    let with_keychain = KeyFindings {
        keys: &in_file,
        keychain_available: true,
        keychain_remedy: None,
        file_chosen: false,
        cannot_protect: None,
    }
    .diagnostics();
    assert_eq!(with_keychain[0].severity, Severity::Warning);
    assert!(
        with_keychain[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("`mm key protect`"))
    );
    let without = KeyFindings {
        keys: &in_file,
        keychain_available: false,
        keychain_remedy: None,
        file_chosen: false,
        cannot_protect: None,
    }
    .diagnostics();
    assert_eq!(without[0].severity, Severity::Info);
    assert!(
        without[0]
            .message
            .contains("does not stop a program that reads both files")
    );
    assert!(
        without[0]
            .message
            .contains("because this machine has no system keychain"),
        "{}",
        without[0].message
    );
    // The file was chosen: no keychain was asked, so nothing is said about
    // whether there is one.
    let chosen = KeyFindings {
        keys: &in_file,
        keychain_available: false,
        keychain_remedy: None,
        file_chosen: true,
        cannot_protect: None,
    }
    .diagnostics();
    assert_eq!(chosen[0].code, "MM_KEY_PASSPHRASE_IN_FILE");
    assert_eq!(chosen[0].severity, Severity::Info);
    assert!(
        chosen[0]
            .message
            .contains("because MM_KEY_VAULT=file is set"),
        "{}",
        chosen[0].message
    );
    assert!(!chosen[0].message.contains("no system keychain"));
    assert!(
        chosen[0]
            .message
            .contains("does not stop a program that reads both files")
    );
    assert!(chosen[0].help.as_deref().is_some_and(
        |help| help.contains("unset MM_KEY_VAULT") && help.contains("`mm key protect`")
    ));
    // A machine whose OpenSSH is too old is told that, not to run a command
    // that would fail.
    let plain = [managed_key(
        ManagedKeyKind::Device,
        KeyProtection::Unprotected,
    )];
    let old = KeyFindings {
        keys: &plain,
        keychain_available: false,
        keychain_remedy: None,
        file_chosen: false,
        cannot_protect: Some("OpenSSH 8.0 is installed"),
    }
    .diagnostics();
    assert!(old[0].message.contains("OpenSSH 8.0 is installed"));
}

#[test]
fn the_openssh_version_is_read_from_what_ssh_prints() {
    assert_eq!(
        openssh_version("OpenSSH_9.2p1 Debian-2+deb12u10, OpenSSL 3.0.20 7 Apr 2026\n"),
        Some((9, 2))
    );
    assert_eq!(
        openssh_version("OpenSSH_10.2p1, OpenSSL 3.5.9"),
        Some((10, 2))
    );
    // The first OpenSSH that takes a passphrase from a helper is 8.4.
    assert!(openssh_version("OpenSSH_8.0p1, OpenSSL 1.1.1k  FIPS") < Some((8, 4)));
    assert!(openssh_version("OpenSSH_8.4p1") >= Some((8, 4)));
    // A build that names itself another way is taken as recent.
    assert_eq!(openssh_version("OpenSSH_for_Windows_9.5p1"), None);
    assert_eq!(openssh_version("dropbear"), None);
}

// ---- the helper and the private agent, against the real OpenSSH tools ----

struct Holds {
    key: PathBuf,
    passphrase: KeyPassphrase,
    asked: AtomicUsize,
}

impl KeyUnlock for Holds {
    fn manages(&self, path: &Path) -> bool {
        path == self.key
    }

    fn passphrase(&self, _path: &Path) -> Result<KeyPassphrase, mm::MmError> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        Ok(self.passphrase.clone())
    }
}

fn protected_key(
    home: &Path,
    passphrase: &KeyPassphrase,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let key = home.join("identities/ssh/id_ed25519");
    OpenSshKeyFiles::new(home.join("run")).generate(
        &key,
        "mm:test@device",
        NewKeyProtection::Passphrase(passphrase),
    )?;
    Ok(key)
}

fn agent_lists(socket: &Path) -> std::io::Result<String> {
    let output = Command::new("ssh-add")
        .arg("-l")
        .env("SSH_AUTH_SOCK", socket)
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[tokio::test]
async fn a_private_agent_holds_the_key_until_it_is_dropped()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let passphrase = KeyPassphrase::from_random(&[3; 32]);
    let key = protected_key(home.path(), &passphrase)?;
    let unlock = Arc::new(Holds {
        key: key.clone(),
        passphrase,
        asked: AtomicUsize::new(0),
    });
    let keys = ManagedKeys::new(unlock.clone(), home.path().join("run"));

    let agent = keys
        .agent_for(&key)
        .await?
        .ok_or("an encrypted key mm manages gets an agent")?;
    let socket = agent.socket().to_path_buf();
    assert!(agent_lists(&socket)?.contains("mm:test@device"));
    // The socket is in a directory only the user can enter.
    let directory = socket.parent().ok_or("parent")?;
    assert_eq!(
        std::fs::metadata(directory)?.permissions().mode() & 0o777,
        0o700
    );
    drop(agent);
    assert!(!socket.exists(), "the agent's socket outlived it");

    // One passphrase for every connection of this process: the vault was
    // asked once, although a helper is made per connection.
    assert!(keys.askpass_for(&key).await?.is_some());
    assert!(keys.askpass_for(&key).await?.is_some());
    assert_eq!(unlock.asked.load(Ordering::Relaxed), 1);
    // The helpers are gone with their connections; their script, which
    // holds nothing, stays for the next one.
    let left = std::fs::read_dir(home.path().join("run"))?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        left,
        vec!["askpass".to_owned()],
        "a helper or an agent left files behind"
    );
    Ok(())
}

#[tokio::test]
async fn keys_mm_does_not_manage_or_that_need_no_passphrase_get_no_helper()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let passphrase = KeyPassphrase::from_random(&[4; 32]);
    let key = protected_key(home.path(), &passphrase)?;
    // Nothing unlocks: the key file is used as it is.
    assert!(ManagedKeys::none().askpass_for(&key).await?.is_none());
    assert!(!ManagedKeys::none().manages(&key));
    // Another key than the one this process may open.
    let other = Arc::new(Holds {
        key: home.path().join("identities/ssh/agent_ed25519"),
        passphrase: passphrase.clone(),
        asked: AtomicUsize::new(0),
    });
    let keys = ManagedKeys::new(other.clone(), home.path().join("run"));
    assert!(keys.askpass_for(&key).await?.is_none());
    assert!(keys.agent_for(&key).await?.is_none());
    // A plain key in the managed place needs no passphrase, so the vault
    // is not asked for one.
    let plain = home.path().join("identities/ssh/agent_ed25519");
    OpenSshKeyFiles::new(home.path().join("run")).generate(
        &plain,
        "mm-agent:test@device",
        NewKeyProtection::None,
    )?;
    assert!(keys.manages(&plain));
    assert!(keys.askpass_for(&plain).await?.is_none());
    assert_eq!(other.asked.load(Ordering::Relaxed), 0);
    Ok(())
}

#[tokio::test]
async fn a_passphrase_that_does_not_open_the_key_is_an_error_with_no_second_try()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let key = protected_key(home.path(), &KeyPassphrase::from_random(&[5; 32]))?;
    let wrong = Arc::new(Holds {
        key: key.clone(),
        passphrase: KeyPassphrase::from_random(&[6; 32]),
        asked: AtomicUsize::new(0),
    });
    let started = std::time::Instant::now();
    let refused = ManagedKeys::new(wrong, home.path().join("run"))
        .agent_for(&key)
        .await;
    assert!(
        matches!(&refused, Err(mm::MmError::Secret { message }) if message.contains("did not open it")),
        "{:?}",
        refused
            .as_ref()
            .map(|_| "an agent")
            .map_err(ToString::to_string)
    );
    // The helper answers once; asked again, it says nothing, and ssh-add
    // gives up instead of waiting.
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    Ok(())
}

// ---- the keychain adapters, against stand-ins for the system's tools -----

fn script(path: &Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

fn entry<'a>(path: &'a Path, fingerprint: &'a str) -> KeyEntry<'a> {
    KeyEntry {
        kind: ManagedKeyKind::Device,
        path,
        fingerprint,
    }
}

#[test]
fn a_desktop_without_secret_tool_is_told_what_to_install() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let keychain = Arc::new(mm::adapters::keys::SecretServiceVault::with_program(
        home.path().join("no-such-secret-tool"),
    ));
    assert!(!keychain.available());
    let remedy = keychain.remedy().ok_or("a missing tool has a remedy")?;
    assert!(remedy.contains("libsecret-tools"), "{remedy}");

    let keys = KeyService::new(KeyServiceDependencies {
        files: Arc::new(OpenSshKeyFiles::new(home.path().join("run"))),
        keychain,
        file_vault: Arc::new(FileKeyVault),
        connections: Arc::new(Closed::default()),
        directory: home.path().join("identities/ssh"),
        preference: VaultPreference::Auto,
        owner: "op@laptop".to_owned(),
    });
    let created = keys.create(ManagedKeyKind::Device, false)?;
    assert_eq!(
        created.key.protection,
        KeyProtection::Passphrase {
            vault: KeyVaultKind::File
        }
    );
    assert!(
        created.notes.iter().any(
            |note| note.contains("mm cannot reach a system keychain here")
                && note.contains("libsecret-tools")
        ),
        "{:?}",
        created.notes
    );
    // Doctor says the same, as something to do rather than a fact of life.
    let findings = keys.diagnostics()?;
    assert_eq!(findings[0].code, "MM_KEY_PASSPHRASE_IN_FILE");
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(
        findings[0].help.as_deref().is_some_and(
            |help| help.contains("libsecret-tools") && help.contains("`mm key protect`")
        ),
        "{:?}",
        findings[0].help
    );
    Ok(())
}

#[test]
fn the_secret_service_takes_and_returns_a_passphrase_on_standard_streams() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let store = home.path().join("keyring");
    std::fs::create_dir_all(&store)?;
    let tool = home.path().join("secret-tool");
    script(
        &tool,
        &FAKE_SECRET_TOOL.replace("$FAKE_KEYRING", &store.display().to_string()),
    )?;
    let vault = mm::adapters::keys::SecretServiceVault::with_program(&tool);
    let key = home.path().join("id_ed25519");
    let entry = entry(&key, "SHA256:abc/def+ghi");
    let passphrase = KeyPassphrase::from_random(&[0x42; 32]);

    assert!(vault.available());
    assert_eq!(vault.remedy(), None);
    assert!(!vault.contains(&entry)?);
    assert_eq!(vault.fetch(&entry)?, None);
    vault.store(&entry, &passphrase)?;
    assert!(vault.contains(&entry)?);
    assert_eq!(vault.fetch(&entry)?, Some(passphrase.clone()));
    // Another key's entry is another entry.
    assert_eq!(vault.fetch(&self::entry(&key, "SHA256:other"))?, None);
    vault.remove(&entry)?;
    vault.remove(&entry)?;
    assert_eq!(vault.fetch(&entry)?, None);
    let calls = std::fs::read_to_string(store.join("calls"))?;
    assert!(
        calls.contains(
            "store --label mm SSH key passphrase service mm-ssh-key fingerprint SHA256:abc/def+ghi"
        ),
        "{calls}"
    );
    assert!(
        !calls.contains(passphrase.expose()),
        "the passphrase was an argument"
    );
    Ok(())
}

/// A stand-in for macOS's `security`, as far as mm uses it: adding an item
/// in interactive mode (the command comes on standard input), finding one,
/// deleting one, and status 44 for an item that is not there.
const FAKE_SECURITY: &str = r#"#!/bin/sh
store=STORE
printf '%s\n' "$*" >> "$store/calls"
name() { printf '%s' "$1" | tr '/+:' '___'; }
case "$1" in
  list-keychains) exit 0 ;;
  -i)
    while read -r verb update a account s service l label w passphrase; do
      [ "$verb" = add-generic-password ] || exit 1
      printf '%s' "$passphrase" > "$store/$(name "$account")"
    done ;;
  find-generic-password)
    [ -f "$store/$(name "$3")" ] || exit 44
    if [ "${6:-}" = -w ]; then cat "$store/$(name "$3")"; echo; fi ;;
  delete-generic-password)
    [ -f "$store/$(name "$3")" ] || exit 44
    rm "$store/$(name "$3")" ;;
  *) exit 1 ;;
esac
"#;

#[test]
fn the_macos_keychain_gets_the_passphrase_on_standard_input_never_as_an_argument() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let store = home.path().join("keychain");
    std::fs::create_dir_all(&store)?;
    let tool = home.path().join("security");
    script(
        &tool,
        &FAKE_SECURITY.replace("STORE", &store.display().to_string()),
    )?;
    let vault = mm::adapters::keys::MacKeychainVault::with_program(&tool);
    let key = home.path().join("id_ed25519");
    let entry = entry(&key, "SHA256:abc/def+ghi");
    let passphrase = KeyPassphrase::from_random(&[0x24; 32]);

    assert!(vault.available());
    assert!(!vault.contains(&entry)?);
    assert_eq!(vault.fetch(&entry)?, None);
    vault.store(&entry, &passphrase)?;
    assert!(vault.contains(&entry)?);
    assert_eq!(vault.fetch(&entry)?, Some(passphrase.clone()));
    vault.remove(&entry)?;
    // Removing what is not there is not an error.
    vault.remove(&entry)?;
    assert_eq!(vault.fetch(&entry)?, None);
    let calls = std::fs::read_to_string(store.join("calls"))?;
    assert!(calls.lines().any(|call| call == "-i"), "{calls}");
    assert!(
        calls.contains("find-generic-password -a SHA256:abc/def+ghi -s mm-ssh-key -w"),
        "{calls}"
    );
    assert!(
        !calls.contains(passphrase.expose()),
        "the passphrase was an argument"
    );
    // A fingerprint that would need quoting is refused, not quoted.
    assert!(
        vault
            .store(&self::entry(&key, "SHA256:a b"), &passphrase)
            .is_err()
    );
    Ok(())
}

#[test]
fn mm_target_connections_lists_and_closes_what_is_kept() -> TestResult {
    let desk = Workstation::new()?;
    let nothing = desk.mm().args(["target", "connections"]).assert().success();
    assert_eq!(
        text(&nothing.get_output().stdout).trim(),
        "No connection is kept open."
    );
    let json = desk
        .mm()
        .args(["--output", "json", "target", "connections"])
        .assert()
        .success();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&json.get_output().stdout)?,
        serde_json::json!([])
    );

    // What a connection leaves under the home: its socket (a plain file
    // stands in for it here) and the note of its host and key.
    let kept = desk.path("home/.mm/run/ssh");
    std::fs::create_dir_all(&kept)?;
    std::fs::set_permissions(&kept, std::fs::Permissions::from_mode(0o700))?;
    std::fs::write(kept.join("0123456789abcdef"), "")?;
    std::fs::write(
        kept.join("0123456789abcdef.owner"),
        format!(
            "{}\n{}\nmm@203.0.113.11:22\nmanaged\n",
            "0".repeat(64),
            desk.key().display()
        ),
    )?;
    let listed = desk.mm().args(["target", "connections"]).assert().success();
    let listed = text(&listed.get_output().stdout);
    assert!(listed.contains("Kept open"), "{listed}");
    assert!(
        listed.contains(&format!("mm@203.0.113.11:22  key {}", desk.key().display())),
        "{listed}"
    );
    assert!(listed.contains("no longer answering"), "{listed}");
    assert!(
        listed.contains("Close them: mm target connections --close"),
        "{listed}"
    );

    desk.tool("ssh", FAKE_SSH)?;
    let closed = desk
        .mm()
        .args(["target", "connections", "--close"])
        .assert()
        .success();
    assert!(text(&closed.get_output().stdout).starts_with("Closed:"));
    assert_eq!(std::fs::read_dir(&kept)?.count(), 0);
    // Closing asks the background ssh to take no more sessions: a session
    // still open on it (another mm's upload) finishes. `-O exit` cuts it.
    let asked = std::fs::read_to_string(desk.path("ssh/arguments"))?;
    assert!(asked.lines().any(|line| line == "stop"), "{asked}");
    assert!(!asked.lines().any(|line| line == "exit"), "{asked}");
    Ok(())
}

/// A users file with one user: the device key and the agent key of
/// another of their devices.
const OTHER_AGENT: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB mm-agent:op@other";

/// Runs a reviewed command: its plan as JSON for the hash, then the apply.
/// Returns what the plan and the apply print for a person.
fn reviewed(
    desk: &Workstation,
    arguments: &[&str],
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let project = desk.path("project");
    let mm = || {
        let mut command = desk.mm();
        command.arg("--root").arg(&project).env("USER", "op");
        command
    };
    let said = mm().args(arguments).assert().success();
    let plan = mm()
        .args(["--output", "json"])
        .args(arguments)
        .assert()
        .success();
    let hash = plan_hash(&plan.get_output().stdout)?;
    let applied = mm()
        .args(arguments)
        .args(["--plan", &hash, "--yes"])
        .assert()
        .success();
    Ok((
        text(&said.get_output().stdout),
        text(&applied.get_output().stdout),
    ))
}

/// `mm key rotate --agent` makes the new key beside the current one, and
/// `mm team join --agent` puts it in the user's entry in place of exactly
/// that key; `--finish` moves it into place. Every other key stays.
#[test]
fn joining_with_a_rotated_agent_key_replaces_this_devices_previous_one() -> TestResult {
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;
    desk.mm().args(["key", "create"]).assert().success();
    desk.mm()
        .args(["key", "create", "--agent"])
        .assert()
        .success();
    let device = std::fs::read_to_string(desk.key().with_extension("pub"))?;
    let agent_public = desk.path("home/.mm/identities/ssh/agent_ed25519.pub");
    let recorded = desk.path("home/.mm/identities/ssh/agent_ed25519.previous.pub");
    let config = desk.path("project/.config/mm.yaml");
    std::fs::write(
        &config,
        format!(
            "{REMOTE_PROJECT}users:\n  op:\n    access:\n      staging: admin\n    keys:\n    - {}\n    - agent: true\n      key: {OTHER_AGENT}\n",
            device.trim()
        ),
    )?;
    let fingerprint =
        |line: &str| mm::domain::host_policy::parse_public_key(line).map(|key| key.fingerprint);
    let listed = || std::fs::read_to_string(&config);

    // The first join adds the agent key: there is nothing to replace.
    let first = std::fs::read_to_string(&agent_public)?.trim().to_owned();
    let (plan, applied) = reviewed(&desk, &["team", "join", "--agent"])?;
    assert!(!plan.contains("previous agent key"), "{plan}");
    assert!(!applied.contains("previous agent key"), "{applied}");
    assert!(listed()?.contains(&first));
    assert!(!recorded.exists());

    // A rotation makes the new key beside the current one: hosts keep
    // accepting the device, and a second rotation waits for the first.
    let next_public = desk.path("home/.mm/identities/ssh/agent_ed25519.next.pub");
    let (_, rotated) = reviewed(&desk, &["key", "rotate", "--agent"])?;
    assert!(
        rotated.contains(
            "Run `mm team join --agent` in each project: it puts the new key in your entry in place of the current one"
        ),
        "{rotated}"
    );
    assert_eq!(std::fs::read_to_string(&agent_public)?.trim(), first);
    let third = std::fs::read_to_string(&next_public)?.trim().to_owned();
    let again = desk
        .mm()
        .args(["key", "rotate", "--agent"])
        .assert()
        .failure();
    assert!(
        text(&again.get_output().stderr)
            .contains("a rotation of this device's agent key is in progress"),
        "{}",
        text(&again.get_output().stderr)
    );

    // Joining puts the new key in place of the current one.
    let (plan, applied) = reviewed(&desk, &["team", "join", "--agent"])?;
    let replaced = format!(
        "the previous agent key {} of this device in user 'op'; hosts stop accepting it after the next `mm team sync`.",
        fingerprint(&first)?
    );
    assert!(
        plan.contains(&format!("This replaces {replaced}")),
        "{plan}"
    );
    assert!(
        applied.contains(&format!("Replaced {replaced}")),
        "{applied}"
    );
    // Finishing moves the new key into place.
    reviewed(&desk, &["key", "rotate", "--agent", "--finish"])?;
    assert_eq!(std::fs::read_to_string(&agent_public)?.trim(), third);
    assert!(!next_public.exists());
    let users = listed()?;
    assert!(users.contains(&third), "{users}");
    assert!(!users.contains(&first), "{users}");
    // The device key and the agent key of the other device are untouched.
    assert!(users.contains(device.trim()), "{users}");
    assert!(users.contains(OTHER_AGENT), "{users}");
    // Nothing is left to take out later.
    assert!(!recorded.exists());

    let again = desk
        .mm()
        .arg("--root")
        .arg(desk.path("project"))
        .env("USER", "op")
        .args(["team", "join", "--agent"])
        .assert()
        .failure();
    assert!(
        text(&again.get_output().stderr).contains("already lists this device's agent key"),
        "{}",
        text(&again.get_output().stderr)
    );
    Ok(())
}

/// A stand-in for `ssh` that asks the helper as `ssh` asks for two
/// encrypted keys: the new one of a rotation, then the current one.
const FAKE_SSH_TWO_KEYS: &str = r#"#!/bin/sh
printf '%s\n' "$@" > "$FAKE_SSH/arguments"
"$SSH_ASKPASS" "Enter passphrase for key '/somewhere/id_ed25519.next': " > "$FAKE_SSH/next-answer"
"$SSH_ASKPASS" "Enter passphrase for key '/somewhere/id_ed25519': " > "$FAKE_SSH/answer"
echo "Connection closed by remote host" >&2
exit 255
"#;

/// While a rotation is in progress, a connection offers the new key first
/// and the current one after it, and the helper answers each key's prompt
/// with that key's passphrase. A sole admin therefore keeps access while
/// the users file and the hosts take the new key, and `mm team
/// join` puts the new key in the entry in place of the current one.
#[test]
fn a_rotation_in_progress_offers_both_keys_and_joins_the_new_one() -> TestResult {
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;
    desk.tool("ssh", FAKE_SSH_TWO_KEYS)?;
    std::fs::create_dir_all(desk.path("project/api"))?;
    std::fs::write(
        desk.path("project/compose.yaml"),
        "services:\n  api:\n    image: busybox:1.36.1\n",
    )?;
    desk.mm().args(["key", "create"]).assert().success();
    let current = std::fs::read_to_string(desk.key().with_extension("pub"))?
        .trim()
        .to_owned();
    let current_passphrase = desk.stored_passphrase()?;
    std::fs::write(
        desk.path("project/.config/mm.yaml"),
        format!(
            "{REMOTE_PROJECT}users:\n  op:\n    access:\n      staging: admin\n    keys:\n    - {current}\n    - agent: true\n      key: {OTHER_AGENT}\n"
        ),
    )?;
    reviewed(&desk, &["key", "rotate"])?;
    let next_key = PathBuf::from(format!("{}.next", desk.key().display()));
    let next = std::fs::read_to_string(next_key.with_extension("next.pub"))?
        .trim()
        .to_owned();

    desk.mm()
        .arg("--root")
        .arg(desk.path("project"))
        .args(["--runtime", "docker-compose", "--environment", "staging"])
        .args(["team", "whoami"])
        .env("MM_SSH_REUSE", "off")
        .assert()
        .failure();
    let arguments = std::fs::read_to_string(desk.path("ssh/arguments"))?;
    let offered = arguments
        .lines()
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|pair| pair[0] == "-i")
        .map(|pair| pair[1].to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        offered,
        [
            next_key.display().to_string(),
            desk.key().display().to_string()
        ],
        "{arguments}"
    );
    let answer = std::fs::read_to_string(desk.path("ssh/answer"))?;
    let next_answer = std::fs::read_to_string(desk.path("ssh/next-answer"))?;
    assert_eq!(answer.trim(), current_passphrase);
    assert!(!next_answer.trim().is_empty());
    assert_ne!(next_answer.trim(), current_passphrase);

    // Joining replaces the current key with the new one in the entry.
    let (_, applied) = reviewed(&desk, &["team", "join"])?;
    assert!(
        applied.contains("Replaced the previous mm key"),
        "{applied}"
    );
    let users = std::fs::read_to_string(desk.path("project/.config/mm.yaml"))?;
    assert!(users.contains(&next), "{users}");
    assert!(!users.contains(&current), "{users}");
    assert!(users.contains(OTHER_AGENT), "{users}");
    Ok(())
}

#[test]
fn a_rotation_joins_the_entry_that_lists_the_key_and_finishes_only_once_it_is_there() -> TestResult
{
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;
    desk.mm().args(["key", "create"]).assert().success();
    let current = std::fs::read_to_string(desk.key().with_extension("pub"))?
        .trim()
        .to_owned();
    let config = desk.path("project/.config/mm.yaml");
    std::fs::write(
        &config,
        format!(
            "{REMOTE_PROJECT}users:\n  operator:\n    access:\n      staging: admin\n    keys:\n    - {current}\n"
        ),
    )?;
    let in_project = |arguments: &[&str]| {
        let mut command = desk.mm();
        command
            .arg("--root")
            .arg(desk.path("project"))
            .env("USER", "op")
            .args(arguments);
        command
    };

    let refused = in_project(&["team", "join"]).assert().failure();
    let said = text(&refused.get_output().stderr);
    assert!(
        said.contains("user 'operator' already lists this device's mm key"),
        "{said}"
    );

    let (_, rotated) = reviewed(&desk, &["key", "rotate"])?;
    assert!(
        rotated.contains("root's ~/.ssh/authorized_keys"),
        "{rotated}"
    );
    let next = std::fs::read_to_string(format!("{}.next.pub", desk.key().display()))?
        .trim()
        .to_owned();

    let early = in_project(&["key", "rotate", "--finish"])
        .assert()
        .failure();
    let said = text(&early.get_output().stderr);
    assert!(
        said.contains("lists this device's mm key in user 'operator' but not the new key"),
        "{said}"
    );
    assert!(said.contains("--unlisted"), "{said}");
    let anyway = in_project(&["key", "rotate", "--finish", "--unlisted"])
        .assert()
        .success();
    assert!(
        text(&anyway.get_output().stdout).contains("mm key rotate --finish --unlisted --plan"),
        "{}",
        text(&anyway.get_output().stdout)
    );

    let (_, applied) = reviewed(&desk, &["team", "join"])?;
    assert!(
        applied.contains("Replaced the previous mm key"),
        "{applied}"
    );
    let users = std::fs::read_to_string(&config)?;
    assert!(
        users.contains(&next) && !users.contains(&current),
        "{users}"
    );
    assert!(!users.contains("\n  op:"), "{users}");

    let (_, finished) = reviewed(&desk, &["key", "rotate", "--finish"])?;
    assert!(
        finished.contains("root's ~/.ssh/authorized_keys"),
        "{finished}"
    );
    assert_eq!(
        std::fs::read_to_string(desk.key().with_extension("pub"))?.trim(),
        next
    );
    Ok(())
}

#[test]
fn a_rotation_whose_key_no_entry_lists_asks_for_the_name() -> TestResult {
    let desk = Workstation::new()?;
    desk.tool("secret-tool", FAKE_SECRET_TOOL)?;
    desk.mm().args(["key", "create"]).assert().success();
    let config = desk.path("project/.config/mm.yaml");
    std::fs::write(
        &config,
        format!("{REMOTE_PROJECT}users:\n  someone:\n    keys:\n    - {OTHER_AGENT}\n"),
    )?;
    reviewed(&desk, &["key", "rotate"])?;
    let refused = desk
        .mm()
        .arg("--root")
        .arg(desk.path("project"))
        .env("USER", "op")
        .args(["team", "join"])
        .assert()
        .failure();
    let said = text(&refused.get_output().stderr);
    assert!(
        said.contains("no user in the users file lists this device's key"),
        "{said}"
    );
    assert!(said.contains("pass --name"), "{said}");
    Ok(())
}

struct RefusingSsh;

#[async_trait::async_trait]
impl mm::app::ports::ProcessRunner for RefusingSsh {
    async fn run(
        &self,
        _specification: mm::app::ports::CommandSpec,
    ) -> Result<mm::app::ports::CommandOutput, mm::MmError> {
        Ok(mm::app::ports::CommandOutput {
            code: Some(255),
            stdout: zeroize::Zeroizing::new(String::new()),
            stderr: zeroize::Zeroizing::new(
                "root@203.0.113.11: Permission denied (publickey).\n".to_owned(),
            ),
            truncated: false,
        })
    }
}

#[tokio::test]
async fn setup_refused_by_a_node_names_the_key_to_authorize() -> TestResult {
    use mm::app::ports::ClusterNodeShell as _;

    let scratch = temp_dirs::tempdir()?;
    let identity = scratch.path().join("id_ed25519");
    let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB mm:op@desk";
    std::fs::write(identity.with_extension("pub"), format!("{line}\n"))?;
    let shell =
        mm::adapters::target::SshClusterNodeShell::new(Arc::new(RefusingSsh), ManagedKeys::none());
    let node = mm::app::ports::ClusterNodeConnection {
        name: "app-1".to_owned(),
        host: "203.0.113.11".to_owned(),
        port: 22,
        user: "root".to_owned(),
        host_key:
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"
                .to_owned(),
        identity_file: identity.clone(),
    };
    let refused = shell
        .run(
            &node,
            &mm::app::ports::ClusterNodeCommand::Program(vec!["true".to_owned()]),
            None,
        )
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    for part in [
        "node 'app-1' (root@203.0.113.11) refused the key setup logs in with",
        &format!("Add the line `{line}` to ~root/.ssh/authorized_keys"),
        "mm key rotate --finish",
    ] {
        assert!(refused.contains(part), "{part}: {refused}");
    }
    Ok(())
}
