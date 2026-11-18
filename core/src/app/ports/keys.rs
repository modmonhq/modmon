use std::path::Path;

use crate::domain::error::MmError;
use crate::domain::key::{KeyPassphrase, KeyVaultKind, ManagedKeyKind};

/// The vault entry of one managed key.
#[derive(Debug, Clone, Copy)]
pub struct KeyEntry<'a> {
    pub kind: ManagedKeyKind,
    /// The private key file.
    pub path: &'a Path,
    /// `SHA256:…` of the public key: what a keychain entry is named after.
    pub fingerprint: &'a str,
}

/// A place that keeps the passphrases of mm-managed keys apart from the
/// key files.
pub trait KeyVault: Send + Sync {
    fn kind(&self) -> KeyVaultKind;

    /// Whether the vault answers on this machine now. Changes nothing.
    fn available(&self) -> bool;

    /// What the user can do to make the vault answer, when it does not and
    /// there is something: a sentence that starts with a verb.
    fn remedy(&self) -> Option<String> {
        None
    }

    /// Stores `passphrase` for the key, replacing an earlier one.
    fn store(&self, entry: &KeyEntry<'_>, passphrase: &KeyPassphrase) -> Result<(), MmError>;

    /// The key's passphrase, or `None` when the vault has none for it.
    fn fetch(&self, entry: &KeyEntry<'_>) -> Result<Option<KeyPassphrase>, MmError>;

    /// Whether the vault has a passphrase for the key, without reading it
    /// into this process.
    fn contains(&self, entry: &KeyEntry<'_>) -> Result<bool, MmError>;

    /// Forgets the key's passphrase. Not an error when there is none.
    fn remove(&self, entry: &KeyEntry<'_>) -> Result<(), MmError>;
}

/// What a private key file says without its passphrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFileFacts {
    /// `<type> <base64> <comment>`.
    pub public_key: String,
    pub fingerprint: String,
    pub encrypted: bool,
    pub security_key: bool,
}

/// How a new key is protected.
#[derive(Debug, Clone, Copy)]
pub enum NewKeyProtection<'a> {
    Passphrase(&'a KeyPassphrase),
    /// A FIDO2 security key; the user touches it when asked.
    SecurityKey,
    /// A plain key, for a machine that cannot use a passphrase unattended.
    None,
}

/// The key files themselves, made and changed by OpenSSH's own tools.
pub trait KeyFiles: Send + Sync {
    /// The key at `path`, or `None` when there is none.
    fn inspect(&self, path: &Path) -> Result<Option<KeyFileFacts>, MmError>;

    /// Why this machine cannot use a passphrase-protected key with nobody
    /// present, when it cannot.
    fn cannot_protect(&self) -> Option<String>;

    /// A new passphrase from the system's random source.
    fn new_passphrase(&self) -> Result<KeyPassphrase, MmError>;

    /// Creates a key pair at `path` and `path.pub`. The private half is
    /// never written unencrypted when a passphrase is given.
    fn generate(
        &self,
        path: &Path,
        comment: &str,
        protection: NewKeyProtection<'_>,
    ) -> Result<(), MmError>;

    /// Encrypts the key at `path` in place under `new`. `old` opens a key
    /// that is encrypted already. The public key does not change.
    fn encrypt(
        &self,
        path: &Path,
        old: Option<&KeyPassphrase>,
        new: &KeyPassphrase,
    ) -> Result<(), MmError>;

    /// Moves the key pair at `staged` over the one at `path`.
    fn replace(&self, staged: &Path, path: &Path) -> Result<(), MmError>;

    /// Deletes the key pair at `path`. Not an error when there is none.
    fn delete(&self, path: &Path) -> Result<(), MmError>;

    /// The public keys the key at `path` took the place of, as recorded
    /// when each was replaced, oldest first. Empty when none is recorded.
    fn replaced(&self, path: &Path) -> Result<Vec<String>, MmError>;

    /// Records `public_key` as a key the one at `path` took the place of.
    fn remember_replaced(&self, path: &Path, public_key: &str) -> Result<(), MmError>;

    /// Forgets the recorded keys. Not an error when there are none.
    fn forget_replaced(&self, path: &Path) -> Result<(), MmError>;
}

/// The connections mm keeps open to remote hosts, which were authenticated
/// with a key and outlive the command that made them.
pub trait KeptConnections: Send + Sync {
    /// Closes every one, so nothing authenticated with a key is used after
    /// the key changed.
    fn close_all(&self);
}

/// Releases the passphrase of a managed key to the code that authenticates
/// with it. The only way a passphrase leaves the vault.
pub trait KeyUnlock: Send + Sync {
    /// Whether `path` is the key this unlocks.
    fn manages(&self, path: &Path) -> bool;

    /// The passphrase of the key at `path`. Fails, naming what to run,
    /// when the vault does not release one.
    fn passphrase(&self, path: &Path) -> Result<KeyPassphrase, MmError>;
}
