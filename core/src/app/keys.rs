//! This device's SSH keys: created protected, older plain ones protected in
//! place, replaced and removed, with the vault entry kept in step.
//!
//! A passphrase leaves the vault in one place only, `KeyService::unlock`,
//! and only for the key the caller was built for: the device key for the
//! user's own commands, the agent key for `mm mcp serve`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::ports::{
    KeptConnections, KeyEntry, KeyFileFacts, KeyFiles, KeyUnlock, KeyVault, NewKeyProtection,
};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::key::{
    KeyChange, KeyChangePlan, KeyChangeStep, KeyFindings, KeyPassphrase, KeyProtection,
    KeyVaultKind, ManagedKey, ManagedKeyKind,
};

/// Which vault takes the passphrase of a new or newly protected key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VaultPreference {
    /// The system keychain when it answers, else a file beside the key.
    #[default]
    Auto,
    /// The system keychain, or fail.
    Keychain,
    /// A file beside the key, and the keychain is never asked.
    File,
}

impl VaultPreference {
    /// The value of `MM_KEY_VAULT`.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.map(str::trim) {
            None | Some("" | "auto") => Ok(Self::Auto),
            Some("keychain") => Ok(Self::Keychain),
            Some("file") => Ok(Self::File),
            Some(other) => Err(format!(
                "MM_KEY_VAULT is '{other}'; use keychain, file, or leave it unset to let mm choose"
            )),
        }
    }
}

pub struct KeyServiceDependencies {
    pub files: Arc<dyn KeyFiles>,
    pub keychain: Arc<dyn KeyVault>,
    pub file_vault: Arc<dyn KeyVault>,
    pub connections: Arc<dyn KeptConnections>,
    /// `MM_HOME/identities/ssh`.
    pub directory: PathBuf,
    pub preference: VaultPreference,
    /// `<user>@<host>`, the second part of a new key's comment.
    pub owner: String,
}

/// A key after a command, and what the user should know about it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct KeyOutcome {
    #[serde(flatten)]
    pub key: ManagedKey,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// After a rotation: the fingerprint of the key this one replaced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced: Option<String>,
}

pub struct KeyService {
    dependencies: KeyServiceDependencies,
}

impl KeyService {
    #[must_use]
    pub fn new(dependencies: KeyServiceDependencies) -> Self {
        Self { dependencies }
    }

    /// Where the private half of a key is.
    #[must_use]
    pub fn path(&self, kind: ManagedKeyKind) -> PathBuf {
        self.dependencies.directory.join(kind.file_name())
    }

    /// The key and how it is protected, or `None` when the device has none.
    pub fn key(&self, kind: ManagedKeyKind) -> Result<Option<ManagedKey>, MmError> {
        self.key_at(kind, self.path(kind))
    }

    /// The new key `mm key rotate` made beside the current one, while the
    /// rotation is not finished.
    pub fn next_key(&self, kind: ManagedKeyKind) -> Result<Option<ManagedKey>, MmError> {
        self.key_at(kind, crate::domain::key::next_key_path(&self.path(kind)))
    }

    /// The key file at `file`, of `kind`. Its passphrase is kept under the
    /// key's own place, by fingerprint, wherever the file is.
    fn key_at(&self, kind: ManagedKeyKind, file: PathBuf) -> Result<Option<ManagedKey>, MmError> {
        let Some(facts) = self.dependencies.files.inspect(&file)? else {
            return Ok(None);
        };
        let protection = self.protection(kind, &self.path(kind), &facts)?;
        Ok(Some(ManagedKey {
            kind,
            path: file,
            public_key: facts.public_key,
            fingerprint: facts.fingerprint,
            protection,
        }))
    }

    fn protection(
        &self,
        kind: ManagedKeyKind,
        path: &Path,
        facts: &KeyFileFacts,
    ) -> Result<KeyProtection, MmError> {
        if facts.security_key {
            return Ok(KeyProtection::SecurityKey);
        }
        if !facts.encrypted {
            return Ok(KeyProtection::Unprotected);
        }
        let entry = KeyEntry {
            kind,
            path,
            fingerprint: &facts.fingerprint,
        };
        let vault = self.holding_vault(&entry)?;
        Ok(if vault.contains(&entry)? {
            KeyProtection::Passphrase {
                vault: vault.kind(),
            }
        } else {
            KeyProtection::PassphraseMissing {
                vault: vault.kind(),
            }
        })
    }

    /// The vault that holds, or should hold, a key's passphrase: the file
    /// beside the key when there is one, else the keychain.
    fn holding_vault(&self, entry: &KeyEntry<'_>) -> Result<&dyn KeyVault, MmError> {
        Ok(if self.dependencies.file_vault.contains(entry)? {
            self.dependencies.file_vault.as_ref()
        } else {
            self.dependencies.keychain.as_ref()
        })
    }

    /// The vault a new passphrase goes to now.
    fn target_vault(&self) -> KeyVaultKind {
        match self.dependencies.preference {
            VaultPreference::File => KeyVaultKind::File,
            VaultPreference::Keychain => KeyVaultKind::Keychain,
            VaultPreference::Auto if self.dependencies.keychain.available() => {
                KeyVaultKind::Keychain
            }
            VaultPreference::Auto => KeyVaultKind::File,
        }
    }

    /// Stores a passphrase in `vault` and checks that it reads back, so a
    /// vault that silently drops it never ends up guarding a key.
    fn store_in(
        &self,
        vault: KeyVaultKind,
        entry: &KeyEntry<'_>,
        passphrase: &KeyPassphrase,
    ) -> Result<(), MmError> {
        let (chosen, other) = match vault {
            KeyVaultKind::Keychain => (&self.dependencies.keychain, &self.dependencies.file_vault),
            KeyVaultKind::File => (&self.dependencies.file_vault, &self.dependencies.keychain),
        };
        chosen.store(entry, passphrase)?;
        if chosen.fetch(entry)?.as_ref() != Some(passphrase) {
            if let Err(error) = chosen.remove(entry) {
                tracing::debug!(%error, "could not remove a passphrase that did not read back");
            }
            return Err(MmError::Secret {
                message: format!(
                    "{} did not return the passphrase it was given for {}",
                    vault.description(),
                    entry.kind.description()
                ),
            });
        }
        // One vault per key: a passphrase left in the other would be stale.
        if let Err(error) = other.remove(entry) {
            tracing::debug!(%error, "could not remove a key's entry in the other vault");
        }
        Ok(())
    }

    /// Stores the passphrase of a new key where the preference says, and
    /// says so when that is not the keychain.
    fn store_new(
        &self,
        entry: &KeyEntry<'_>,
        passphrase: &KeyPassphrase,
    ) -> Result<(KeyVaultKind, Option<String>), MmError> {
        let in_file = |reason: String| {
            self.store_in(KeyVaultKind::File, entry, passphrase)?;
            Ok((
                KeyVaultKind::File,
                Some(format!(
                    "{reason}, so the passphrase is in a file beside the key. That stops the key file being used on its own; it does not stop a program that reads both files. Where a keychain is available, `mm key protect` moves it there"
                )),
            ))
        };
        match self.target_vault() {
            KeyVaultKind::File if self.dependencies.preference == VaultPreference::File => {
                self.store_in(KeyVaultKind::File, entry, passphrase)?;
                Ok((KeyVaultKind::File, None))
            }
            KeyVaultKind::File => in_file(match self.dependencies.keychain.remedy() {
                Some(remedy) => format!("mm cannot reach a system keychain here ({remedy})"),
                None => "This machine has no system keychain".to_owned(),
            }),
            KeyVaultKind::Keychain => {
                match self.store_in(KeyVaultKind::Keychain, entry, passphrase) {
                    Ok(()) => Ok((KeyVaultKind::Keychain, None)),
                    Err(error) if self.dependencies.preference == VaultPreference::Auto => in_file(
                        format!("The system keychain did not take the passphrase ({error})"),
                    ),
                    Err(error) => Err(error),
                }
            }
        }
    }

    /// Creates a key. It is encrypted from the first byte written, under a
    /// passphrase that goes to the vault.
    pub fn create(&self, kind: ManagedKeyKind, security_key: bool) -> Result<KeyOutcome, MmError> {
        let path = self.path(kind);
        if self.dependencies.files.inspect(&path)?.is_some() {
            return Err(MmError::Environment {
                message: format!(
                    "{} already exists; `mm key show{}` prints it, `mm key rotate{}` replaces it",
                    path.display(),
                    kind.flag(),
                    kind.flag()
                ),
            });
        }
        let notes = self.generate(kind, &path, &path, security_key)?;
        self.dependencies.connections.close_all();
        self.outcome(kind, notes)
    }

    /// Generates a key at `staged`, whose final place is `path`, and puts
    /// its passphrase in the vault under the final place.
    fn generate(
        &self,
        kind: ManagedKeyKind,
        staged: &Path,
        path: &Path,
        security_key: bool,
    ) -> Result<Vec<String>, MmError> {
        let files = &self.dependencies.files;
        let comment = format!("{}:{}", kind.comment_prefix(), self.dependencies.owner);
        if security_key {
            files.generate(staged, &comment, NewKeyProtection::SecurityKey)?;
            return Ok(Vec::new());
        }
        if let Some(reason) = files.cannot_protect() {
            files.generate(staged, &comment, NewKeyProtection::None)?;
            return Ok(vec![format!(
                "This key is a plain private key, because mm cannot protect it here: {reason}. Anything running as you can use or copy it. After upgrading, run `mm key protect`"
            )]);
        }
        let passphrase = files.new_passphrase()?;
        files.generate(staged, &comment, NewKeyProtection::Passphrase(&passphrase))?;
        let stored = self.facts(staged).and_then(|facts| {
            self.store_new(
                &KeyEntry {
                    kind,
                    path,
                    fingerprint: &facts.fingerprint,
                },
                &passphrase,
            )
        });
        match stored {
            Ok((_, note)) => Ok(note.into_iter().collect()),
            Err(error) => {
                // A key nobody can open is worth nothing: take it away.
                if let Err(cleanup) = files.delete(staged) {
                    tracing::warn!(%cleanup, "could not remove a key whose passphrase was not stored");
                }
                Err(error)
            }
        }
    }

    fn facts(&self, path: &Path) -> Result<KeyFileFacts, MmError> {
        self.dependencies
            .files
            .inspect(path)?
            .ok_or_else(|| MmError::State {
                message: format!("ssh-keygen wrote no key at {}", path.display()),
            })
    }

    fn outcome(&self, kind: ManagedKeyKind, notes: Vec<String>) -> Result<KeyOutcome, MmError> {
        let key = self.key(kind)?.ok_or_else(|| MmError::State {
            message: format!("{} is gone", self.path(kind).display()),
        })?;
        Ok(KeyOutcome {
            key,
            notes,
            replaced: None,
        })
    }

    fn existing(&self, kind: ManagedKeyKind) -> Result<ManagedKey, MmError> {
        self.key(kind)?.ok_or_else(|| MmError::Environment {
            message: format!(
                "this device has no {} yet; run `mm key create{}`",
                match kind {
                    ManagedKeyKind::Device => "mm key",
                    ManagedKeyKind::Agent => "agent key",
                },
                kind.flag()
            ),
        })
    }

    /// What `mm key protect` would do: every plain key is encrypted in
    /// place, and a passphrase in a file moves to the keychain when one
    /// answers.
    pub fn protect_plan(&self) -> Result<KeyChangePlan, MmError> {
        let target = self.target_vault();
        let mut steps = Vec::new();
        for kind in ManagedKeyKind::ALL {
            let Some(key) = self.key(kind)? else {
                continue;
            };
            let change = match key.protection {
                KeyProtection::Unprotected => KeyChange::Encrypt,
                KeyProtection::Passphrase {
                    vault: KeyVaultKind::File,
                } if target == KeyVaultKind::Keychain => KeyChange::MoveToKeychain,
                _ => continue,
            };
            steps.push(KeyChangeStep {
                kind,
                fingerprint: key.fingerprint,
                change,
                vault: Some(target),
                keeps_access: true,
            });
        }
        if steps.iter().any(|step| step.change == KeyChange::Encrypt)
            && let Some(reason) = self.dependencies.files.cannot_protect()
        {
            return Err(MmError::Environment {
                message: format!("mm cannot protect a key on this machine: {reason}"),
            });
        }
        Ok(KeyChangePlan::new(steps))
    }

    /// Applies a reviewed `protect_plan`. Public keys do not change, so no
    /// host has to learn anything.
    pub fn protect(&self, reviewed: &str) -> Result<Vec<ManagedKey>, MmError> {
        let plan = self.protect_plan()?;
        plan.check(reviewed, "mm key protect")
            .map_err(|message| MmError::Security { message })?;
        for step in &plan.steps {
            let path = self.path(step.kind);
            let entry = KeyEntry {
                kind: step.kind,
                path: &path,
                fingerprint: &step.fingerprint,
            };
            let vault = step.vault.unwrap_or(KeyVaultKind::File);
            match step.change {
                KeyChange::Encrypt => self.encrypt(&entry, vault)?,
                KeyChange::MoveToKeychain => {
                    let passphrase =
                        self.dependencies.file_vault.fetch(&entry)?.ok_or_else(|| {
                            MmError::State {
                                message: format!(
                                    "the passphrase file of {} is gone",
                                    step.kind.description()
                                ),
                            }
                        })?;
                    self.store_in(vault, &entry, &passphrase)?;
                }
                KeyChange::Replace | KeyChange::Stage | KeyChange::Finish | KeyChange::Remove => {}
            }
        }
        self.dependencies.connections.close_all();
        plan.steps
            .iter()
            .map(|step| self.existing(step.kind))
            .collect()
    }

    /// Encrypts a plain key in place. The passphrase is in the vault
    /// before the key needs it, so a failure between the two steps never
    /// leaves a key nobody can open.
    fn encrypt(&self, entry: &KeyEntry<'_>, vault: KeyVaultKind) -> Result<(), MmError> {
        let passphrase = self.dependencies.files.new_passphrase()?;
        self.store_in(vault, entry, &passphrase)?;
        if let Err(error) = self
            .dependencies
            .files
            .encrypt(entry.path, None, &passphrase)
        {
            for vault in [&self.dependencies.keychain, &self.dependencies.file_vault] {
                if let Err(cleanup) = vault.remove(entry) {
                    tracing::debug!(%cleanup, "could not remove the passphrase of a key that stayed plain");
                }
            }
            return Err(error);
        }
        Ok(())
    }

    /// What `mm key rotate` would do. A key under a passphrase gets a new
    /// key beside it, and both are offered until the rotation is finished,
    /// so the device keeps its access while hosts learn the new key:
    /// a sole admin can rotate. A security key is replaced at once.
    pub fn rotate_plan(&self, kind: ManagedKeyKind) -> Result<KeyChangePlan, MmError> {
        let key = self.existing(kind)?;
        if let Some(next) = self.next_key(kind)? {
            return Err(MmError::Environment {
                message: format!(
                    "a rotation of {} is in progress: the new key {} waits beside it. Finish it with `mm key rotate{} --finish` once every users file lists the new key and was synced",
                    kind.description(),
                    next.fingerprint,
                    kind.flag()
                ),
            });
        }
        let security = key.protection == KeyProtection::SecurityKey;
        // A key whose passphrase is lost cannot be offered any more: its
        // replacement takes its place at once.
        let at_once = security || matches!(key.protection, KeyProtection::PassphraseMissing { .. });
        let protected = !security && self.dependencies.files.cannot_protect().is_none();
        Ok(KeyChangePlan::new(vec![KeyChangeStep {
            kind,
            fingerprint: key.fingerprint,
            change: if at_once {
                KeyChange::Replace
            } else {
                KeyChange::Stage
            },
            vault: protected.then(|| self.target_vault()),
            keeps_access: !at_once,
        }]))
    }

    /// What `mm key rotate --finish` would do: the new key takes the
    /// current one's place.
    pub fn finish_plan(&self, kind: ManagedKeyKind) -> Result<KeyChangePlan, MmError> {
        let next = self.next_key(kind)?.ok_or_else(|| MmError::Environment {
            message: format!(
                "no rotation of {} is in progress; `mm key rotate{}` starts one",
                kind.description(),
                kind.flag()
            ),
        })?;
        Ok(KeyChangePlan::new(vec![KeyChangeStep {
            kind,
            fingerprint: next.fingerprint,
            change: KeyChange::Finish,
            vault: None,
            keeps_access: false,
        }]))
    }

    /// Finishes a rotation: the new key replaces the current one, whose
    /// passphrase is forgotten. Connections made with the old key close.
    pub fn finish(&self, kind: ManagedKeyKind, reviewed: &str) -> Result<KeyOutcome, MmError> {
        let command = format!("mm key rotate{} --finish", kind.flag());
        self.finish_plan(kind)?
            .check(reviewed, &command)
            .map_err(|message| MmError::Security { message })?;
        let old = self.existing(kind)?;
        let path = self.path(kind);
        self.dependencies.connections.close_all();
        self.dependencies
            .files
            .replace(&crate::domain::key::next_key_path(&path), &path)?;
        self.forget(&KeyEntry {
            kind,
            path: &path,
            fingerprint: &old.fingerprint,
        });
        let mut outcome = self.outcome(kind, Vec::new())?;
        outcome.replaced = Some(old.fingerprint);
        Ok(outcome)
    }

    /// Starts a rotation (a key under a passphrase), or replaces a security
    /// key at once.
    pub fn rotate(&self, kind: ManagedKeyKind, reviewed: &str) -> Result<KeyOutcome, MmError> {
        let command = format!("mm key rotate{}", kind.flag());
        let plan = self.rotate_plan(kind)?;
        plan.check(reviewed, &command)
            .map_err(|message| MmError::Security { message })?;
        if plan
            .steps
            .iter()
            .any(|step| step.change == KeyChange::Stage)
        {
            return self.stage(kind);
        }
        self.replace_at_once(kind)
    }

    /// Makes the new key beside the current one. Nothing else changes.
    fn stage(&self, kind: ManagedKeyKind) -> Result<KeyOutcome, MmError> {
        let old = self.existing(kind)?;
        let path = self.path(kind);
        let next = crate::domain::key::next_key_path(&path);
        let notes = self.generate(kind, &next, &path, false)?;
        // The next connection offers both keys.
        self.dependencies.connections.close_all();
        let key = self.next_key(kind)?.ok_or_else(|| MmError::State {
            message: format!("{} is gone", next.display()),
        })?;
        Ok(KeyOutcome {
            key,
            notes,
            replaced: Some(old.fingerprint),
        })
    }

    /// Replaces a security key, or a key whose passphrase is lost, with a
    /// new one at once.
    fn replace_at_once(&self, kind: ManagedKeyKind) -> Result<KeyOutcome, MmError> {
        let old = self.existing(kind)?;
        let path = self.path(kind);
        let staged = crate::domain::key::next_key_path(&path);
        let files = &self.dependencies.files;
        files.delete(&staged)?;
        // The new key and its passphrase exist before the old ones go, so
        // a failure on the way leaves the old key working.
        let notes = self.generate(
            kind,
            &staged,
            &path,
            old.protection == KeyProtection::SecurityKey,
        )?;
        let new = self.facts(&staged)?;
        // Nothing authenticated with the old key is used after this.
        self.dependencies.connections.close_all();
        if let Err(error) = files.replace(&staged, &path) {
            self.forget(&KeyEntry {
                kind,
                path: &path,
                fingerprint: &new.fingerprint,
            });
            if let Err(cleanup) = files.delete(&staged) {
                tracing::warn!(%cleanup, "could not remove a new key that did not replace the old one");
            }
            return Err(error);
        }
        self.forget(&KeyEntry {
            kind,
            path: &path,
            fingerprint: &old.fingerprint,
        });
        let mut outcome = self.outcome(kind, notes)?;
        // `mm team join --agent` takes exactly this key out of the user's
        // entry when it puts the new one in.
        if kind == ManagedKeyKind::Agent
            && let Err(error) = files.remember_replaced(&path, &old.public_key)
        {
            outcome.notes.push(format!(
                "mm could not record the old public key ({error}), so `mm team join --agent` will add the new key beside it: remove the line of {} from your entry in the users file by hand",
                old.fingerprint
            ));
        }
        outcome.replaced = Some(old.fingerprint);
        Ok(outcome)
    }

    /// The public keys of this device that rotations replaced since the
    /// users file last took this device's key: the lines `mm team join`
    /// may take out of the user's entry, and no others.
    pub fn replaced(&self, kind: ManagedKeyKind) -> Result<Vec<String>, MmError> {
        self.dependencies.files.replaced(&self.path(kind))
    }

    /// Forgets them, once the users file has the new key in their place.
    pub fn forget_replaced(&self, kind: ManagedKeyKind) -> Result<(), MmError> {
        self.dependencies.files.forget_replaced(&self.path(kind))
    }

    /// What `mm team join` puts in the user's entry, and the fingerprints
    /// it takes out: while a rotation is in progress the new key goes in
    /// and the current one comes out; the keys earlier rotations replaced
    /// come out too. Never another device's key.
    pub fn joining(&self, kind: ManagedKeyKind) -> Result<(ManagedKey, Vec<String>), MmError> {
        let current = self.existing(kind)?;
        let mut leaving = self
            .replaced(kind)?
            .iter()
            .filter_map(
                |line| match crate::domain::host_policy::parse_public_key(line) {
                    Ok(key) => Some(key.fingerprint),
                    // A line mm wrote that no longer parses names no key.
                    Err(_) => None,
                },
            )
            .collect::<Vec<_>>();
        match self.next_key(kind)? {
            Some(next) => {
                leaving.push(current.fingerprint);
                Ok((next, leaving))
            }
            None => Ok((current, leaving)),
        }
    }

    /// What `mm key remove` would do.
    pub fn remove_plan(&self, kind: ManagedKeyKind) -> Result<KeyChangePlan, MmError> {
        let key = self.existing(kind)?;
        Ok(KeyChangePlan::new(vec![KeyChangeStep {
            kind,
            fingerprint: key.fingerprint,
            change: KeyChange::Remove,
            vault: None,
            keeps_access: false,
        }]))
    }

    /// Deletes a key and its passphrase.
    pub fn remove(&self, kind: ManagedKeyKind, reviewed: &str) -> Result<ManagedKey, MmError> {
        let command = format!("mm key remove{}", kind.flag());
        self.remove_plan(kind)?
            .check(reviewed, &command)
            .map_err(|message| MmError::Security { message })?;
        let key = self.existing(kind)?;
        self.dependencies.connections.close_all();
        self.dependencies.files.delete(&key.path)?;
        self.forget(&KeyEntry {
            kind,
            path: &key.path,
            fingerprint: &key.fingerprint,
        });
        Ok(key)
    }

    /// Removes a key's passphrase from both vaults. A vault that does not
    /// answer keeps its entry, which opens nothing once the key is gone.
    fn forget(&self, entry: &KeyEntry<'_>) {
        for vault in [&self.dependencies.keychain, &self.dependencies.file_vault] {
            if let Err(error) = vault.remove(entry) {
                tracing::warn!(
                    %error,
                    fingerprint = entry.fingerprint,
                    "could not remove a key's passphrase from {}",
                    vault.kind().description()
                );
            }
        }
    }

    /// What `mm doctor` reports about this device's keys.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, MmError> {
        let mut keys = Vec::new();
        for kind in ManagedKeyKind::ALL {
            keys.extend(self.key(kind)?);
        }
        let in_file = keys.iter().any(|key| {
            key.protection
                == KeyProtection::Passphrase {
                    vault: KeyVaultKind::File,
                }
        });
        let unprotected = keys
            .iter()
            .any(|key| key.protection == KeyProtection::Unprotected);
        let cannot_protect = unprotected
            .then(|| self.dependencies.files.cannot_protect())
            .flatten();
        // The keychain is asked only when the answer changes a finding.
        let file_chosen = self.dependencies.preference == VaultPreference::File;
        let ask = in_file && !file_chosen;
        let keychain_remedy = ask.then(|| self.dependencies.keychain.remedy()).flatten();
        let mut found = KeyFindings {
            keys: &keys,
            keychain_available: ask && self.dependencies.keychain.available(),
            keychain_remedy: keychain_remedy.as_deref(),
            file_chosen,
            cannot_protect: cannot_protect.as_deref(),
        }
        .diagnostics();
        for kind in ManagedKeyKind::ALL {
            if let Some(next) = self.next_key(kind)? {
                found.push(
                    Diagnostic::warning(
                        "MM_KEY_ROTATION_PENDING",
                        format!(
                            "a rotation of {} is not finished: mm offers the new key {} and the old one",
                            kind.description(),
                            next.fingerprint
                        ),
                    )
                    .with_help(format!(
                        "Run `mm team join{flag}` in each project, commit, and run `mm team sync`; then `mm key rotate{flag} --finish` drops the old key",
                        flag = kind.flag()
                    )),
                );
            }
        }
        Ok(found)
    }

    /// The one way a passphrase leaves the vault: for authenticating with
    /// the key of `kind`, and no other.
    #[must_use]
    pub fn unlock(self: &Arc<Self>, kind: ManagedKeyKind) -> Arc<dyn KeyUnlock> {
        Arc::new(KindUnlock {
            service: Arc::clone(self),
            kind,
        })
    }
}

struct KindUnlock {
    service: Arc<KeyService>,
    kind: ManagedKeyKind,
}

impl KeyUnlock for KindUnlock {
    fn manages(&self, path: &Path) -> bool {
        let own = self.service.path(self.kind);
        path == own || path == crate::domain::key::next_key_path(&own)
    }

    fn passphrase(&self, path: &Path) -> Result<KeyPassphrase, MmError> {
        let kind = self.kind;
        if !self.manages(path) {
            return Err(MmError::Security {
                message: format!("{} is not {}", path.display(), kind.description()),
            });
        }
        let _phase = crate::app::timing::phase("key.unlock");
        let facts = self.service.facts(path)?;
        // The passphrase of a rotation's new key is kept under the key's
        // own place, by its fingerprint.
        let own = self.service.path(kind);
        let entry = KeyEntry {
            kind,
            path: &own,
            fingerprint: &facts.fingerprint,
        };
        let vault = self.service.holding_vault(&entry)?;
        let recovery = format!(
            "If the keychain is locked, unlock it and run the command again. If the entry is gone, the key cannot be recovered: run `mm key rotate{flag}`, then `mm key show{flag}` and have an admin register the new public key",
            flag = kind.flag()
        );
        match vault.fetch(&entry) {
            Ok(Some(passphrase)) => Ok(passphrase),
            Ok(None) => Err(MmError::Secret {
                message: format!(
                    "{} is encrypted and its passphrase is not in {}. {recovery}",
                    kind.description(),
                    vault.kind().description()
                ),
            }),
            Err(error) => Err(MmError::Secret {
                message: format!(
                    "{} is encrypted and {} did not release its passphrase: {error}. {recovery}",
                    kind.description(),
                    vault.kind().description()
                ),
            }),
        }
    }
}
