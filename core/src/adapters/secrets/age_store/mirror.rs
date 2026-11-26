// Host-side import: a host keeps a mirror of the operator's age store for the
// environments it runs, with the operator's revisions.

impl AgeSecretStore {
    /// Creates the store, and the identity when missing, then returns the
    /// identity's age recipient. Safe to repeat and to race.
    pub fn ensure_recipient(&self) -> Result<String, MmError> {
        if !self.initialized() {
            match self.initialize() {
                Ok(initialized) => return Ok(initialized.recipient),
                // Another process created the store first.
                Err(_) if self.initialized() => {}
                Err(error) => return Err(error),
            }
        }
        let _lock = self.exclusive_lock()?;
        // Reading the document proves the identity opens this store.
        self.read_document()?;
        Ok(self.read_identity()?.to_public().to_string())
    }

    /// Opens a document sealed to this store's recipient.
    pub fn open_sealed(&self, sealed: &[u8], limit: usize) -> Result<Zeroizing<Vec<u8>>, MmError> {
        ensure_initialized(self)?;
        let identity = self.read_identity()?;
        crate::adapters::secrets::open_sealed(&identity, sealed, limit)
    }

    /// Opens a bundle sealed to this store's recipient.
    pub fn open_secret_bundle(
        &self,
        sealed: &[u8],
    ) -> Result<crate::domain::secret::SecretBundle, MmError> {
        ensure_initialized(self)?;
        let identity = self.read_identity()?;
        crate::adapters::secrets::open_secret_bundle(&identity, sealed)
    }

    /// Writes each secret as the active value with the revision it carries,
    /// so this store's revisions equal the sender's. A secret whose active
    /// revision already matches is left alone; any other value moves to
    /// `previous` as a replacement would. Secrets not listed are kept. The
    /// batch is written at once, or not at all.
    pub fn mirror(
        &self,
        scope: &SecretScope,
        secrets: Vec<crate::domain::secret::BundledSecret>,
        now_unix_ms: i64,
    ) -> Result<Vec<SecretWriteResult>, MmError> {
        ensure_initialized(self)?;
        let mut keys = BTreeSet::new();
        for secret in &secrets {
            ensure_age(&secret.reference)?;
            validate_material(&secret.material)?;
            crate::domain::secret::validate_secret_revision(&secret.revision)
                .map_err(|message| MmError::Secret { message })?;
            if !keys.insert(storage_key(scope, &secret.reference)) {
                return Err(MmError::Secret {
                    message: "a secret mirror batch contains a duplicate provider location"
                        .to_owned(),
                });
            }
        }

        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let mut results = Vec::with_capacity(secrets.len());
        let mut changed = false;
        for secret in secrets {
            let key = storage_key(scope, &secret.reference);
            let key_for_record = key.clone();
            let next = StoredSecret {
                value: secret.material.into_zeroizing(),
                revision: secret.revision,
                created_at_unix_ms: secret.set_at_unix_ms.unwrap_or(now_unix_ms),
                set_by: secret.set_by,
            };
            let revision = next.revision.clone();
            let entry_changed = match document.entries.get_mut(&key) {
                Some(entry) if entry.active.revision == revision => false,
                Some(entry) => {
                    replace_active(entry, next, now_unix_ms)?;
                    true
                }
                None => {
                    document.entries.insert(
                        key,
                        SecretEntry {
                            active: next,
                            candidate: None,
                            previous: None,
                            retired: Vec::new(),
                        },
                    );
                    true
                }
            };
            if entry_changed {
                document.host.changes.insert(
                    format!("secret:{key_for_record}"),
                    ChangeRecord {
                        revision: revision.clone(),
                        changed_by: None,
                        changed_at_unix_ms: now_unix_ms,
                        origin: crate::domain::staged::ValueOrigin::Setup,
                    },
                );
            }
            changed |= entry_changed;
            results.push(SecretWriteResult {
                revision,
                changed: entry_changed,
            });
        }
        if changed {
            self.save_document(&document)?;
        }
        Ok(results)
    }
}

#[cfg(test)]
#[path = "mirror/tests.rs"]
mod mirror_tests;
