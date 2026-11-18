//! The SSH keys mm creates on a device, and how each one is protected.
//!
//! A managed key is for mm only. Its private half is encrypted with a
//! passphrase mm generates, and the passphrase is kept apart from the key
//! file, in a vault. Nobody chooses or types it.

use std::path::PathBuf;

use serde::Serialize;
use zeroize::Zeroizing;

use crate::domain::diagnostic::Diagnostic;

mod openssh;
pub use openssh::{PrivateKeyHeader, ed25519_private_key, openssh_version};

/// Which of this device's keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedKeyKind {
    /// The key the user's own commands authenticate with.
    Device,
    /// The key `mm mcp serve` authenticates with, for the user's AI agent.
    Agent,
}

impl ManagedKeyKind {
    pub const ALL: [Self; 2] = [Self::Device, Self::Agent];

    /// The key's file name under `MM_HOME/identities/ssh`.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Device => "id_ed25519",
            Self::Agent => "agent_ed25519",
        }
    }

    /// How messages name the key.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Device => "this device's mm key",
            Self::Agent => "this device's agent key",
        }
    }

    /// The flag that selects the key on `mm key` commands, with its
    /// leading space, or nothing for the device key.
    #[must_use]
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Device => "",
            Self::Agent => " --agent",
        }
    }

    /// The first part of the key's comment.
    #[must_use]
    pub const fn comment_prefix(self) -> &'static str {
        match self {
            Self::Device => "mm",
            Self::Agent => "mm-agent",
        }
    }
}

/// Where the passphrase of a key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyVaultKind {
    /// The operating system's keychain: the Secret Service on Linux (GNOME
    /// Keyring, KWallet), the login keychain on macOS.
    Keychain,
    /// A file beside the key, for a machine with no keychain.
    File,
}

impl KeyVaultKind {
    /// How messages name the vault.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Keychain => "the system keychain",
            Self::File => "a file beside the key",
        }
    }
}

/// A passphrase mm generated for one of its keys. It is never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyPassphrase(Zeroizing<String>);

impl KeyPassphrase {
    /// The passphrase for 32 random bytes: 43 characters of URL-safe
    /// base64, so it survives every tool it passes through unquoted.
    #[must_use]
    pub fn from_random(bytes: &[u8; 32]) -> Self {
        use base64::Engine as _;
        Self(Zeroizing::new(
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
        ))
    }

    /// A passphrase read back from a vault. Refuses anything mm would not
    /// have written, so a damaged entry is reported instead of tried.
    pub fn from_vault(value: Zeroizing<String>) -> Result<Self, String> {
        let trimmed = value.trim_end_matches(['\r', '\n']);
        if trimmed.len() < 32
            || trimmed.len() > 128
            || !trimmed
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err("the stored value is not a passphrase mm wrote".to_owned());
        }
        Ok(Self(Zeroizing::new(trimmed.to_owned())))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for KeyPassphrase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("KeyPassphrase([REDACTED])")
    }
}

/// How a managed key is protected at rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum KeyProtection {
    /// Encrypted; the passphrase is in `vault`.
    Passphrase { vault: KeyVaultKind },
    /// Encrypted, and `vault` has no passphrase for it: the key cannot be
    /// used until the vault is unlocked, or ever again if the entry is gone.
    PassphraseMissing { vault: KeyVaultKind },
    /// A FIDO2 security key: the file is a handle, and every connection
    /// needs a touch.
    SecurityKey,
    /// A plain private key: anything that reads the file can use it.
    Unprotected,
}

/// One of this device's keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagedKey {
    pub kind: ManagedKeyKind,
    pub path: PathBuf,
    /// `<type> <base64> <comment>`, ready for the users file.
    pub public_key: String,
    pub fingerprint: String,
    pub protection: KeyProtection,
}

impl ManagedKey {
    /// One line on how the key is protected, for `mm key show`.
    #[must_use]
    pub fn protection_summary(&self) -> String {
        match self.protection {
            KeyProtection::Passphrase { vault } => {
                format!("passphrase in {}", vault.description())
            }
            KeyProtection::PassphraseMissing { vault } => format!(
                "passphrase missing from {}; unlock it, or replace the key with `mm key rotate{}`",
                vault.description(),
                self.kind.flag()
            ),
            KeyProtection::SecurityKey => "security key (a touch per connection)".to_owned(),
            KeyProtection::Unprotected => "none; run `mm key protect`".to_owned(),
        }
    }
}

/// What a reviewed key command does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyChange {
    /// Encrypt a plain key in place and put its passphrase in the vault.
    Encrypt,
    /// Move the passphrase from the file beside the key into the keychain.
    MoveToKeychain,
    /// Replace the key with a new one at once (a security key).
    Replace,
    /// Make a new key beside the current one. mm offers both until the
    /// rotation is finished, so the device keeps its access meanwhile.
    Stage,
    /// Put the new key of a rotation in the current one's place and drop
    /// the old key and its passphrase.
    Finish,
    /// Delete the key and its passphrase.
    Remove,
}

/// Where `mm key rotate` keeps the new key beside the key at `path` until
/// the rotation is finished: `id_ed25519.next`.
#[must_use]
pub fn next_key_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".next");
    path.with_file_name(name)
}

/// One key a reviewed command changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyChangeStep {
    pub kind: ManagedKeyKind,
    pub fingerprint: String,
    pub change: KeyChange,
    /// Where the passphrase will be afterwards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<KeyVaultKind>,
    /// Whether hosts keep accepting this device afterwards.
    pub keeps_access: bool,
}

impl KeyChangeStep {
    /// One line for the review.
    #[must_use]
    pub fn summary(&self) -> String {
        let key = format!("{} ({})", self.kind.description(), self.fingerprint);
        let vault = self.vault.map_or("a vault", KeyVaultKind::description);
        match self.change {
            KeyChange::Encrypt => format!(
                "encrypt {key} in place; its passphrase goes to {vault}. The public key stays the same"
            ),
            KeyChange::MoveToKeychain => format!(
                "move the passphrase of {key} from the file beside it into {vault}. The key itself does not change"
            ),
            KeyChange::Replace => format!(
                "replace {key} with a new key; its passphrase goes to {vault}. Hosts refuse this device until an admin registers the new public key"
            ),
            KeyChange::Stage => format!(
                "make a new key beside {key}; its passphrase goes to {vault}. mm offers both keys until `mm key rotate{} --finish`, so hosts keep accepting this device while the users file and the hosts take the new key",
                self.kind.flag()
            ),
            KeyChange::Finish => format!(
                "put the new key {key} in place of the current one and delete the old key and its passphrase. Hosts whose users file does not list the new key yet refuse this device from then on"
            ),
            KeyChange::Remove => {
                format!(
                    "delete {key} and its passphrase. Hosts keep its public key until an admin removes it from the users file"
                )
            }
        }
    }
}

/// A reviewed change to this device's keys. Applying it needs the hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyChangePlan {
    pub steps: Vec<KeyChangeStep>,
    pub plan_hash: String,
}

impl KeyChangePlan {
    #[must_use]
    pub fn new(steps: Vec<KeyChangeStep>) -> Self {
        let bytes = serde_json::to_vec(&("mm-key-change-v1", &steps)).unwrap_or_default();
        Self {
            plan_hash: blake3::hash(&bytes).to_hex().to_string(),
            steps,
        }
    }

    /// Refuses a hash that is not this plan's: the keys changed since the
    /// review, or the hash belongs to another command.
    pub fn check(&self, reviewed: &str, command: &str) -> Result<(), String> {
        if reviewed == self.plan_hash {
            return Ok(());
        }
        Err(format!(
            "the keys changed since that review, or the hash is another command's; run `{command}` and review it again"
        ))
    }
}

/// A connection mm keeps open to a remote host between commands. It was
/// authenticated with a key, and for as long as it is open a session with
/// the host's relay needs no key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeptConnection {
    /// `user@host:port`; unknown for a connection an older mm made.
    pub peer: Option<String>,
    /// The key file it was authenticated with.
    pub key: Option<PathBuf>,
    /// Whether the background `ssh` still answers.
    pub open: bool,
    /// How long ago it was made.
    pub age_seconds: Option<u64>,
}

/// What `mm doctor` says about this device's keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFindings<'a> {
    pub keys: &'a [ManagedKey],
    /// Whether the system keychain answers now.
    pub keychain_available: bool,
    /// What the user can do to make it answer, when there is something.
    pub keychain_remedy: Option<&'a str>,
    /// Whether the user chose the file with `MM_KEY_VAULT=file`: then no
    /// keychain was asked, and whether this machine has one is not known.
    pub file_chosen: bool,
    /// Why this machine cannot use a passphrase-protected key unattended,
    /// when it cannot.
    pub cannot_protect: Option<&'a str>,
}

impl KeyFindings<'_> {
    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.keys
            .iter()
            .filter_map(|key| self.diagnostic(key))
            .collect()
    }

    fn diagnostic(&self, key: &ManagedKey) -> Option<Diagnostic> {
        let name = key.kind.description();
        let flag = key.kind.flag();
        match key.protection {
            KeyProtection::Passphrase {
                vault: KeyVaultKind::Keychain,
            }
            | KeyProtection::SecurityKey => None,
            KeyProtection::Unprotected => Some(match self.cannot_protect {
                Some(reason) => Diagnostic::warning(
                    "MM_KEY_UNPROTECTED",
                    format!(
                        "{name} ({}) is a plain private key: anything running as you can use or copy it. mm cannot protect it here: {reason}",
                        key.fingerprint
                    ),
                )
                .with_help("Upgrade OpenSSH, then run `mm key protect`"),
                None => Diagnostic::warning(
                    "MM_KEY_UNPROTECTED",
                    format!(
                        "{name} ({}) is a plain private key: anything running as you can use or copy it",
                        key.fingerprint
                    ),
                )
                .with_help("Run `mm key protect`: it encrypts the key in place and keeps its public key, so no host access changes"),
            }),
            KeyProtection::PassphraseMissing { vault } => Some(
                Diagnostic::error(
                    "MM_KEY_PASSPHRASE_MISSING",
                    format!(
                        "{name} ({}) is encrypted and its passphrase is not in {}",
                        key.fingerprint,
                        vault.description()
                    ),
                )
                .with_help(format!(
                    "If the keychain is locked, unlock it. If the entry is gone, the key cannot be recovered: run `mm key rotate{flag}`, then `mm key show{flag}` and have an admin register the new public key"
                )),
            ),
            KeyProtection::Passphrase {
                vault: KeyVaultKind::File,
            } if self.file_chosen => Some(
                Diagnostic::info(
                    "MM_KEY_PASSPHRASE_IN_FILE",
                    format!(
                        "the passphrase of {name} ({}) is in a file beside the key, because MM_KEY_VAULT=file is set. That stops the key file being used on its own; it does not stop a program that reads both files",
                        key.fingerprint
                    ),
                )
                .with_help("To keep it in the system keychain instead, unset MM_KEY_VAULT and run `mm key protect`"),
            ),
            KeyProtection::Passphrase {
                vault: KeyVaultKind::File,
            } if self.keychain_available => Some(
                Diagnostic::warning(
                    "MM_KEY_PASSPHRASE_IN_FILE",
                    format!(
                        "the passphrase of {name} ({}) is in a file beside the key, and a system keychain is available",
                        key.fingerprint
                    ),
                )
                .with_help("Run `mm key protect` to move it into the keychain"),
            ),
            KeyProtection::Passphrase {
                vault: KeyVaultKind::File,
            } => Some(match self.keychain_remedy {
                Some(remedy) => Diagnostic::warning(
                    "MM_KEY_PASSPHRASE_IN_FILE",
                    format!(
                        "the passphrase of {name} ({}) is in a file beside the key, because mm cannot reach a system keychain here",
                        key.fingerprint
                    ),
                )
                .with_help(format!("{remedy}, then run `mm key protect`")),
                None => Diagnostic::info(
                    "MM_KEY_PASSPHRASE_IN_FILE",
                    format!(
                        "the passphrase of {name} ({}) is in a file beside the key, because this machine has no system keychain. That stops the key file being used on its own; it does not stop a program that reads both files",
                        key.fingerprint
                    ),
                ),
            }),
        }
    }
}
