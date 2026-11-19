
#[derive(Default, Serialize, Deserialize)]
struct HostStaging {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    variables: BTreeMap<String, StoredSecret>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    staged: BTreeMap<String, StagedEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    changes: BTreeMap<String, ChangeRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    undo: Option<StageUndo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    prepared: BTreeMap<Uuid, PreparedStage>,
}

fn no_host_staging(host: &&HostStaging) -> bool {
    host.is_empty()
}

impl HostStaging {
    fn is_empty(&self) -> bool {
        self.variables.is_empty()
            && self.staged.is_empty()
            && self.changes.is_empty()
            && self.undo.is_none()
            && self.prepared.is_empty()
    }
}

/// A stage a node checked and keeps until the client commits or reverts
/// it on every node. Its values stay sealed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedStage {
    pub environment: String,
    pub principal: String,
    pub changes: Vec<crate::domain::staged::ValueChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<String>,
    pub expires_at_unix_ms: i64,
}

#[derive(Serialize, Deserialize)]
struct StagedEntry {
    value: Option<Zeroizing<String>>,
    revision: String,
    staged_by: String,
    staged_at_unix_ms: i64,
}

impl std::fmt::Debug for StagedEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StagedEntry")
            .field("removal", &self.value.is_none())
            .field("revision", &self.revision)
            .field("staged_by", &self.staged_by)
            .finish_non_exhaustive()
    }
}

impl StagedEntry {
    fn metadata(&self) -> SecretMetadata {
        match &self.value {
            Some(_) => SecretMetadata {
                status: SecretStatus::Set,
                revision: Some(self.revision.clone()),
                created_at_unix_ms: Some(self.staged_at_unix_ms),
                candidate_revision: None,
            },
            None => unset_metadata(),
        }
    }

    fn resolution(self) -> Result<SecretResolution, MmError> {
        let metadata = self.metadata();
        let material = self
            .value
            .map(SecretMaterial::from_zeroizing)
            .transpose()
            .map_err(|message| MmError::Secret { message })?;
        Ok(SecretResolution { metadata, material })
    }

    fn revision(&self) -> crate::domain::staged::StagedRevision {
        crate::domain::staged::StagedRevision {
            revision: self.revision.clone(),
            removal: self.value.is_none(),
            staged_by: self.staged_by.clone(),
            staged_at_unix_ms: self.staged_at_unix_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChangeRecord {
    revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    changed_by: Option<String>,
    changed_at_unix_ms: i64,
    origin: crate::domain::staged::ValueOrigin,
}

#[derive(Debug, Serialize, Deserialize)]
struct StageUndo {
    stage: Uuid,
    previous: BTreeMap<String, Option<StagedEntry>>,
}

fn secret_slot(scope: &SecretScope, reference: &SecretReference) -> String {
    if reference.scheme() == "age" {
        format!("secret:{}", storage_key(scope, reference))
    } else {
        format!(
            "secret:{}/{}:{}",
            scope.environment(),
            reference.scheme(),
            reference.locator()
        )
    }
}

/// Where a staged change lands in a host's store.
#[derive(Clone, PartialEq, Eq)]
pub enum StagedSlot {
    Secret {
        environment: String,
        reference: SecretReference,
    },
    Variable {
        environment: String,
        key: String,
    },
}

impl std::fmt::Debug for StagedSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.id())
    }
}

impl StagedSlot {
    fn id(&self) -> String {
        match self {
            Self::Secret {
                environment,
                reference,
            } => secret_slot(&SecretScope::new(environment.as_str()), reference),
            Self::Variable { environment, key } => format!("variable:{environment}/{key}"),
        }
    }

    /// A secret whose live value another store keeps: only its staged
    /// changes are here.
    #[must_use]
    pub fn kept_elsewhere(&self) -> bool {
        matches!(self, Self::Secret { reference, .. } if reference.scheme() != "age")
    }

    fn live_key(&self) -> String {
        match self {
            Self::Secret {
                environment,
                reference,
            } => storage_key(&SecretScope::new(environment.as_str()), reference),
            Self::Variable { environment, key } => format!("{environment}/{key}"),
        }
    }
}

pub enum StoreChange {
    Set {
        revision: String,
        value: Zeroizing<String>,
    },
    Unset {
        revision: String,
    },
    Discard,
}

pub struct StoreStagedChange {
    pub slot: StagedSlot,
    pub change: StoreChange,
}

/// What a host's store holds for one slot. Never a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotReport {
    pub active: Option<crate::domain::staged::ValueRevision>,
    pub origin: Option<crate::domain::staged::ValueOrigin>,
    pub staged: Option<crate::domain::staged::StagedRevision>,
}

/// A variable as the next deployment on this host gets it.
pub struct HostVariable {
    pub key: String,
    pub value: Zeroizing<String>,
    pub revision: String,
    pub changed_at_unix_ms: i64,
}

impl AgeSecretStore {
    /// Opens a stage's values sealed to this store.
    pub fn open_staged_values(
        &self,
        sealed: &[u8],
    ) -> Result<crate::domain::staged::StagedPayload, MmError> {
        ensure_initialized(self)?;
        let identity = self.read_identity()?;
        crate::adapters::secrets::open_staged_values(&identity, sealed)
    }

    /// Keeps a checked stage until it is committed or reverted. Stages
    /// nobody finished are dropped once they expire.
    pub fn prepare_stage(
        &self,
        stage: Uuid,
        prepared: PreparedStage,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        ensure_initialized(self)?;
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        document
            .host
            .prepared
            .retain(|_, kept| kept.expires_at_unix_ms > now_unix_ms);
        document.host.prepared.insert(stage, prepared);
        self.save_document(&document)
    }

    /// Takes a prepared stage that has not expired.
    pub fn take_prepared(
        &self,
        stage: Uuid,
        now_unix_ms: i64,
    ) -> Result<Option<PreparedStage>, MmError> {
        if !self.initialized() {
            return Ok(None);
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let Some(prepared) = document.host.prepared.remove(&stage) else {
            return Ok(None);
        };
        self.save_document(&document)?;
        Ok((prepared.expires_at_unix_ms > now_unix_ms).then_some(prepared))
    }

    /// Stages `changes` as one stage that `revert_stage` can undo until the
    /// next stage. Nothing that is live changes.
    pub fn stage_values(
        &self,
        stage: Uuid,
        changes: Vec<StoreStagedChange>,
        staged_by: &str,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        ensure_initialized(self)?;
        for change in &changes {
            if let StoreChange::Set { value, .. } = &change.change {
                crate::domain::staged::validate_value(&change.slot.id(), value)
                    .map_err(|message| MmError::Secret { message })?;
            }
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let mut previous = BTreeMap::new();
        for change in changes {
            let id = change.slot.id();
            let entry = |value, revision| StagedEntry {
                value,
                revision,
                staged_by: staged_by.to_owned(),
                staged_at_unix_ms: now_unix_ms,
            };
            let before = match change.change {
                StoreChange::Set { revision, value } => document
                    .host
                    .staged
                    .insert(id.clone(), entry(Some(value), revision)),
                StoreChange::Unset { revision } => document
                    .host
                    .staged
                    .insert(id.clone(), entry(None, revision)),
                StoreChange::Discard => document.host.staged.remove(&id),
            };
            previous.entry(id).or_insert(before);
        }
        document.host.undo = Some(StageUndo { stage, previous });
        self.save_document(&document)
    }

    /// Puts back what `stage` replaced. False when the last stage is
    /// another one, so there is nothing of `stage` to undo.
    pub fn revert_stage(&self, stage: Uuid) -> Result<bool, MmError> {
        if !self.initialized() {
            return Ok(false);
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        if document.host.prepared.remove(&stage).is_some() {
            self.save_document(&document)?;
            return Ok(true);
        }
        let Some(undo) = document
            .host
            .undo
            .take_if(|undo| undo.stage == stage)
        else {
            return Ok(false);
        };
        for (id, before) in undo.previous {
            match before {
                Some(entry) => document.host.staged.insert(id, entry),
                None => document.host.staged.remove(&id),
            };
        }
        self.save_document(&document)?;
        Ok(true)
    }

    /// What is live and what is staged for each slot.
    pub fn slot_reports(&self, slots: &[StagedSlot]) -> Result<Vec<SlotReport>, MmError> {
        if !self.initialized() {
            return Ok(vec![SlotReport::default(); slots.len()]);
        }
        let _lock = self.exclusive_lock()?;
        let document = self.read_document()?;
        Ok(slots
            .iter()
            .map(|slot| {
                let id = slot.id();
                let record = document.host.changes.get(&id);
                if slot.kept_elsewhere() {
                    return SlotReport {
                        active: record.map(|record| crate::domain::staged::ValueRevision {
                            revision: record.revision.clone(),
                            changed_by: record.changed_by.clone(),
                            changed_at_unix_ms: Some(record.changed_at_unix_ms),
                        }),
                        origin: record.map(|record| record.origin),
                        staged: document.host.staged.get(&id).map(StagedEntry::revision),
                    };
                }
                let live = match slot {
                    StagedSlot::Secret { .. } => document
                        .entries
                        .get(&slot.live_key())
                        .map(|entry| &entry.active),
                    StagedSlot::Variable { .. } => document.host.variables.get(&slot.live_key()),
                };
                let active = live.map(|live| {
                    let record = record.filter(|record| record.revision == live.revision);
                    crate::domain::staged::ValueRevision {
                        revision: live.revision.clone(),
                        changed_by: record.and_then(|record| record.changed_by.clone()),
                        changed_at_unix_ms: Some(
                            record.map_or(live.created_at_unix_ms, |record| {
                                record.changed_at_unix_ms
                            }),
                        ),
                    }
                });
                SlotReport {
                    origin: live.map(|_| {
                        record.map_or(crate::domain::staged::ValueOrigin::Setup, |record| {
                            record.origin
                        })
                    }),
                    active,
                    staged: document.host.staged.get(&id).map(StagedEntry::revision),
                }
            })
            .collect())
    }

    /// Makes the staged changes of `items` live, each only while its
    /// staged revision is still the one given. Returns how many went live.
    pub fn go_live(
        &self,
        items: &[(StagedSlot, String)],
        now_unix_ms: i64,
    ) -> Result<usize, MmError> {
        if !self.initialized() || items.is_empty() {
            return Ok(0);
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let mut live = 0;
        for (slot, revision) in items {
            let id = slot.id();
            let Some(staged) = document
                .host
                .staged
                .remove_entry(&id)
                .and_then(|(key, staged)| {
                    if &staged.revision == revision {
                        Some(staged)
                    } else {
                        document.host.staged.insert(key, staged);
                        None
                    }
                })
            else {
                continue;
            };
            let key = slot.live_key();
            match (slot, staged.value) {
                (_, None) => {
                    document.entries.remove(&key);
                    document.host.variables.remove(&key);
                    document.host.changes.remove(&id);
                }
                (StagedSlot::Secret { .. }, Some(value)) => {
                    let next = StoredSecret {
                        value,
                        revision: staged.revision.clone(),
                        created_at_unix_ms: staged.staged_at_unix_ms,
                    };
                    match document.entries.get_mut(&key) {
                        Some(entry) => replace_active(entry, next, now_unix_ms)?,
                        None => {
                            document.entries.insert(
                                key.clone(),
                                SecretEntry {
                                    active: next,
                                    candidate: None,
                                    previous: None,
                                    retired: Vec::new(),
                                },
                            );
                        }
                    }
                }
                (StagedSlot::Variable { .. }, Some(value)) => {
                    document.host.variables.insert(
                        key.clone(),
                        StoredSecret {
                            value,
                            revision: staged.revision.clone(),
                            created_at_unix_ms: staged.staged_at_unix_ms,
                        },
                    );
                }
            }
            if document.entries.contains_key(&key) || document.host.variables.contains_key(&key) {
                document.host.changes.insert(
                    id,
                    ChangeRecord {
                        revision: staged.revision,
                        changed_by: Some(staged.staged_by),
                        changed_at_unix_ms: staged.staged_at_unix_ms,
                        origin: crate::domain::staged::ValueOrigin::Staged,
                    },
                );
            }
            live += 1;
        }
        if live > 0 {
            self.save_document(&document)?;
        }
        Ok(live)
    }

    /// The live value of an age secret and its revision, never what is
    /// staged for it.
    pub fn live_secret(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<Option<(String, Zeroizing<String>)>, MmError> {
        ensure_age(reference)?;
        if !self.initialized() {
            return Ok(None);
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        Ok(document
            .entries
            .remove(&storage_key(scope, reference))
            .map(|entry| (entry.active.revision, entry.active.value)))
    }

    /// What is staged for a secret another store keeps live: its
    /// metadata, `None` when nothing is.
    pub fn staged_metadata(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<Option<SecretMetadata>, MmError> {
        if !self.initialized() {
            return Ok(None);
        }
        let _lock = self.exclusive_lock()?;
        let document = self.read_document()?;
        Ok(document
            .host
            .staged
            .get(&secret_slot(scope, reference))
            .map(StagedEntry::metadata))
    }

    /// What is staged for a secret another store keeps live, with its value.
    pub fn staged_resolution(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<Option<SecretResolution>, MmError> {
        if !self.initialized() {
            return Ok(None);
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        document
            .host
            .staged
            .remove(&secret_slot(scope, reference))
            .map(StagedEntry::resolution)
            .transpose()
    }

    /// The change staged for `slot` while its revision is `revision`: the
    /// value, or `None` for a removal. It stays staged.
    pub fn staged_change(
        &self,
        slot: &StagedSlot,
        revision: &str,
    ) -> Result<Option<Option<Zeroizing<String>>>, MmError> {
        if !self.initialized() {
            return Ok(None);
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        Ok(document
            .host
            .staged
            .remove(&slot.id())
            .filter(|staged| staged.revision == revision)
            .map(|staged| staged.value))
    }

    /// Drops the change staged for `slot` once another store holds it live,
    /// and records who changed the value. False when another revision is
    /// staged by now.
    pub fn settle_staged(&self, slot: &StagedSlot, revision: &str) -> Result<bool, MmError> {
        if !self.initialized() {
            return Ok(false);
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let id = slot.id();
        let Some(staged) = document
            .host
            .staged
            .remove(&id)
            .filter(|staged| staged.revision == revision)
        else {
            return Ok(false);
        };
        if staged.value.is_some() {
            document.host.changes.insert(
                id,
                ChangeRecord {
                    revision: staged.revision,
                    changed_by: Some(staged.staged_by),
                    changed_at_unix_ms: staged.staged_at_unix_ms,
                    origin: crate::domain::staged::ValueOrigin::Staged,
                },
            );
        } else {
            document.host.changes.remove(&id);
        }
        self.save_document(&document)?;
        Ok(true)
    }

    /// The variables of `environment` as the next deployment gets them:
    /// what is staged, else what is live.
    pub fn variable_values(&self, environment: &str) -> Result<Vec<HostVariable>, MmError> {
        if !self.initialized() {
            return Ok(Vec::new());
        }
        let _lock = self.exclusive_lock()?;
        let document = self.read_document()?;
        let prefix = format!("{environment}/");
        let staged_prefix = format!("variable:{prefix}");
        let mut values = BTreeMap::new();
        for (key, live) in &document.host.variables {
            if let Some(name) = key.strip_prefix(&prefix) {
                values.insert(
                    name.to_owned(),
                    Some((live.value.clone(), live.revision.clone(), live.created_at_unix_ms)),
                );
            }
        }
        for (key, staged) in &document.host.staged {
            if let Some(name) = key.strip_prefix(&staged_prefix) {
                values.insert(
                    name.to_owned(),
                    staged.value.clone().map(|value| {
                        (value, staged.revision.clone(), staged.staged_at_unix_ms)
                    }),
                );
            }
        }
        Ok(values
            .into_iter()
            .filter_map(|(key, value)| {
                value.map(|(value, revision, changed_at_unix_ms)| HostVariable {
                    key,
                    value,
                    revision,
                    changed_at_unix_ms,
                })
            })
            .collect())
    }
}
