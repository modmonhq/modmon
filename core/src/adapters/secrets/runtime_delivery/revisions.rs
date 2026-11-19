const REVISION_NAME_KEY: &str = "name.key";

/// The revision files of one project's environment on one target.
struct RevisionFiles {
    directory: PathBuf,
    /// Made on this host and never leaves it: without it a file name says
    /// nothing about a revision, and a revision that a provider derives
    /// from a value cannot be tried against names.
    key: [u8; 32],
}

impl RevisionFiles {
    fn scope(plan: &RuntimeSecretPlan) -> String {
        let scope = blake3::hash(
            format!(
                "{}\0{}\0{}",
                plan.project_id, plan.target_id, plan.environment
            )
            .as_bytes(),
        );
        scope.to_hex()[..40].to_owned()
    }

    fn open(root: &Path, plan: &RuntimeSecretPlan) -> Result<Self, MmError> {
        create_private_directory(root)?;
        let key_path = root.join(REVISION_NAME_KEY);
        let key = match read_name_key(&key_path)? {
            Some(key) => key,
            None => {
                let mut key = [0_u8; 32];
                key[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
                key[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
                match write_private_file(&key_path, &key) {
                    Ok(()) => key,
                    // Another deployment on this host made it first.
                    Err(_) => read_name_key(&key_path)?.ok_or_else(|| MmError::Security {
                        message: "the runtime secret file name key could not be created"
                            .to_owned(),
                    })?,
                }
            }
        };
        let directory = root.join(Self::scope(plan));
        create_private_directory(&directory)?;
        Ok(Self { directory, key })
    }

    /// The files that are there, without creating anything.
    fn existing(root: &Path, plan: &RuntimeSecretPlan) -> Result<Option<Self>, MmError> {
        let directory = root.join(Self::scope(plan));
        if !directory.is_dir() {
            return Ok(None);
        }
        ensure_no_symlink_components(&directory)?;
        ensure_owned_by_current_user(&directory)?;
        Ok(read_name_key(&root.join(REVISION_NAME_KEY))?.map(|key| Self { directory, key }))
    }

    fn name(&self, plan: &RuntimeSecretPlan, logical: &str, revision: &str) -> String {
        let owner = blake3::hash(
            format!(
                "{}\0{}\0{}\0{logical}",
                plan.project_id, plan.target_id, plan.environment
            )
            .as_bytes(),
        );
        let revision = blake3::keyed_hash(
            &self.key,
            format!(
                "{}\0{}\0{}\0{logical}\0{revision}",
                plan.project_id, plan.target_id, plan.environment
            )
            .as_bytes(),
        );
        format!(
            "mm-{}-{}",
            &owner.to_hex()[..40],
            &revision.to_hex()[..20]
        )
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }

    /// The files whose revision `keep` says to keep, each copy included.
    fn copies(&self, keep: impl Fn(&str) -> bool) -> Result<Vec<String>, MmError> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(|source| MmError::Io {
            context: "enumerating runtime secret revision files".to_owned(),
            source,
        })? {
            let entry = entry.map_err(|source| MmError::Io {
                context: "enumerating a runtime secret revision file".to_owned(),
                source,
            })?;
            if let Ok(name) = entry.file_name().into_string()
                && keep(FileOwner::split(&name).0)
            {
                names.push(name);
            }
        }
        Ok(names)
    }

    /// Writes the revision's file unless it is already kept, given to
    /// `owner` when there is one.
    fn keep(
        &self,
        operation_id: OperationId,
        logical: &str,
        name: &str,
        bytes: &[u8],
        owner: Option<FileOwner>,
    ) -> Result<(), MmError> {
        let path = self.path(name);
        ensure_no_symlink_components(&path)?;
        match fs::read(&path).map(zeroize::Zeroizing::new) {
            Ok(kept) if kept.as_slice() == bytes => Ok(()),
            Ok(_) => Err(MmError::Security {
                message: format!(
                    "this host keeps the same revision of runtime secret '{logical}' with other content; write a new version of the secret and plan again"
                ),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let staged = self.directory.join(format!(".{}.tmp", operation_id.0));
                write_private_file(&staged, bytes)?;
                if let Some(owner) = owner {
                    give_to(&staged, owner)?;
                }
                fs::rename(&staged, &path).map_err(|source| MmError::Io {
                    context: "keeping a runtime secret revision file".to_owned(),
                    source,
                })
            }
            Err(source) => Err(MmError::Io {
                context: "reading a kept runtime secret revision file".to_owned(),
                source,
            }),
        }
    }
}

fn read_name_key(path: &Path) -> Result<Option<[u8; 32]>, MmError> {
    ensure_no_symlink_components(path)?;
    match fs::read(path) {
        Ok(bytes) => <[u8; 32]>::try_from(bytes.as_slice())
            .map(Some)
            .map_err(|_| MmError::Security {
                message: "the runtime secret file name key is damaged".to_owned(),
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(MmError::Io {
            context: "reading the runtime secret file name key".to_owned(),
            source,
        }),
    }
}

/// Removes every revision file of the plan's environment that `retained`
/// does not name.
fn prune_revision_files(
    root: &Path,
    plan: &RuntimeSecretPlan,
    retained: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<String>, MmError> {
    let Some(files) = RevisionFiles::existing(root, plan)? else {
        return Ok(Vec::new());
    };
    let keep = retained
        .iter()
        .flat_map(|(logical, revisions)| {
            revisions
                .iter()
                .map(|revision| files.name(plan, logical, revision))
        })
        .collect::<BTreeSet<_>>();
    let mut removed = Vec::new();
    for entry in fs::read_dir(&files.directory).map_err(|source| MmError::Io {
        context: "enumerating runtime secret revision files".to_owned(),
        source,
    })? {
        let entry = entry.map_err(|source| MmError::Io {
            context: "enumerating a runtime secret revision file".to_owned(),
            source,
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| MmError::Security {
                message: "runtime secret revision storage contains a non-Unicode entry".to_owned(),
            })?;
        if keep.contains(FileOwner::split(&name).0) {
            continue;
        }
        let is_file = entry
            .file_type()
            .map_err(|source| MmError::Io {
                context: "inspecting a runtime secret revision file".to_owned(),
                source,
            })?
            .is_file();
        if !is_file {
            return Err(MmError::Security {
                message: "runtime secret revision storage contains an entry that is not a file"
                    .to_owned(),
            });
        }
        fs::remove_file(entry.path()).map_err(|source| MmError::Io {
            context: "removing a superseded runtime secret revision file".to_owned(),
            source,
        })?;
        removed.push(name);
    }
    Ok(removed)
}
