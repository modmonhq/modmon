fn prepared_item(
    item: &RuntimeSecretPlanItem,
    runtime_name: String,
    created_at_unix_ms: Option<i64>,
) -> PreparedRuntimeSecret {
    PreparedRuntimeSecret {
        logical_name: item.logical_name.clone(),
        source_revision: item.source_revision.clone(),
        runtime_name,
        created_at_unix_ms,
        consumers: item.consumers.clone(),
    }
}

fn operation_root(run_directory: &Path, operation_id: OperationId) -> Result<PathBuf, MmError> {
    create_private_directory(run_directory)?;
    let root = run_directory.join(operation_id.0.to_string());
    if root.exists() {
        return Err(MmError::Security {
            message: format!("runtime secret operation {} already exists", operation_id.0),
        });
    }
    create_private_directory(&root)?;
    Ok(root)
}

fn render_overlays(
    root: &Path,
    prepared: &[PreparedRuntimeSecret],
    external: bool,
) -> Result<Vec<(String, PathBuf)>, MmError> {
    let key = |secret: &PreparedRuntimeSecret| overlay_key(&secret.logical_name);
    render_overlay_values(root, prepared, key, |secret| {
        let mut definition = Mapping::new();
        definition.insert(Value::String("external".to_owned()), Value::Bool(external));
        definition.insert(
            Value::String("name".to_owned()),
            Value::String(secret.runtime_name.clone()),
        );
        Value::Mapping(definition)
    })
}

fn render_compose_overlays(
    root: &Path,
    prepared: &[PreparedRuntimeSecret],
    secret_root: &Path,
) -> Result<Vec<(String, PathBuf)>, MmError> {
    // The key carries the revision: Compose compares a service's secret
    // references, not the file behind them, so only a changed key makes it
    // replace the services that use the secret.
    // A copy for one consumer's user (rootful Podman) has a key of its own.
    let key = |secret: &PreparedRuntimeSecret| {
        let (name, owner) = FileOwner::split(&secret.runtime_name);
        let revision = name
            .rsplit('-')
            .next()
            .unwrap_or_default()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>();
        let key = format!("{}_{revision}", overlay_key(&secret.logical_name));
        owner.map_or_else(
            || key.clone(),
            |owner| format!("{key}_{}", owner.replace('.', "_")),
        )
    };
    render_overlay_values(root, prepared, key, |secret| {
        let mut definition = Mapping::new();
        definition.insert(
            Value::String("file".to_owned()),
            Value::String(
                secret_root
                    .join(&secret.runtime_name)
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
        Value::Mapping(definition)
    })
}

fn render_overlay_values(
    root: &Path,
    prepared: &[PreparedRuntimeSecret],
    key: impl Fn(&PreparedRuntimeSecret) -> String,
    definition: impl Fn(&PreparedRuntimeSecret) -> Value,
) -> Result<Vec<(String, PathBuf)>, MmError> {
    let mut stacks = BTreeSet::new();
    for secret in prepared {
        stacks.extend(
            secret
                .consumers
                .iter()
                .map(|consumer| consumer.stack.clone()),
        );
    }
    let mut overlays = Vec::new();
    for stack in stacks {
        let selected = prepared
            .iter()
            .filter(|secret| {
                secret
                    .consumers
                    .iter()
                    .any(|consumer| consumer.stack == stack)
            })
            .collect::<Vec<_>>();
        let mut secrets = Mapping::new();
        let mut services = Mapping::new();
        for secret in selected {
            let key = key(secret);
            secrets.insert(Value::String(key.clone()), definition(secret));
            for consumer in secret
                .consumers
                .iter()
                .filter(|consumer| consumer.stack == stack)
            {
                let service = services
                    .entry(Value::String(consumer.service.clone()))
                    .or_insert_with(|| Value::Mapping(Mapping::new()));
                let service = service.as_mapping_mut().ok_or_else(|| MmError::Security {
                    message: "runtime secret overlay service shape collided".to_owned(),
                })?;
                let mounts = service
                    .entry(Value::String("secrets".to_owned()))
                    .or_insert_with(|| Value::Sequence(Vec::new()));
                let mounts = mounts.as_sequence_mut().ok_or_else(|| MmError::Security {
                    message: "runtime secret overlay mount shape collided".to_owned(),
                })?;
                let mut mount = Mapping::new();
                mount.insert(
                    Value::String("source".to_owned()),
                    Value::String(key.clone()),
                );
                mount.insert(
                    Value::String("target".to_owned()),
                    Value::String(consumer.target.clone()),
                );
                // A copy given to its consumer's user (rootful Podman)
                // carries its owner and mode itself: Docker Compose
                // bind-mounts a file secret and only warns about these.
                if FileOwner::split(&secret.runtime_name).1.is_some() {
                    mounts.push(Value::Mapping(mount));
                    continue;
                }
                mount.insert(
                    Value::String("uid".to_owned()),
                    Value::String(consumer.uid.clone()),
                );
                mount.insert(
                    Value::String("gid".to_owned()),
                    Value::String(consumer.gid.clone()),
                );
                mount.insert(
                    Value::String("mode".to_owned()),
                    Value::Number(consumer.mode.into()),
                );
                mounts.push(Value::Mapping(mount));
            }
        }
        let mut document = Mapping::new();
        document.insert(
            Value::String("services".to_owned()),
            Value::Mapping(services),
        );
        document.insert(Value::String("secrets".to_owned()), Value::Mapping(secrets));
        let yaml = serde_yaml_ng::to_string(&Value::Mapping(document)).map_err(|error| {
            MmError::RuntimeResponse {
                message: format!("could not render runtime secret overlay: {error}"),
            }
        })?;
        let path = root.join(format!("{stack}.secrets.compose.yaml"));
        write_private_file(&path, yaml.as_bytes())?;
        overlays.push((stack, path));
    }
    Ok(overlays)
}

fn overlay_key(logical: &str) -> String {
    format!(
        "mm_secret_{}",
        &blake3::hash(logical.as_bytes()).to_hex()[..20]
    )
}

fn create_private_directory(path: &Path) -> Result<(), MmError> {
    ensure_private_permissions_supported()?;
    ensure_no_symlink_components(path)?;
    if path.exists() {
        ensure_owned_by_current_user(path)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|source| MmError::Io {
                context: format!(
                    "creating private runtime secret directory {}",
                    path.display()
                ),
                source,
            })?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!(
                    "protecting private runtime secret directory {}",
                    path.display()
                ),
                source,
            }
        })?;
        Ok(())
    }
    #[cfg(not(unix))]
    Err(MmError::Security {
        message: "runtime secret operation files require implemented private ACL enforcement"
            .to_owned(),
    })
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("creating private runtime secret file {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing private runtime secret file {}", path.display()),
            source,
        })
}

fn dispose_operation(
    run_directory: &Path,
    prepared: &PreparedRuntimeSecrets,
) -> Result<(), MmError> {
    if prepared.root.parent() != Some(run_directory)
        || prepared.root.file_name().and_then(|name| name.to_str())
            != Some(prepared.operation_id.0.to_string().as_str())
    {
        return Err(MmError::Security {
            message: "refusing to dispose runtime secret files outside their operation root"
                .to_owned(),
        });
    }
    ensure_no_symlink_components(&prepared.root)?;
    match fs::remove_dir_all(&prepared.root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(MmError::Io {
            context: format!(
                "disposing runtime secret operation {}",
                prepared.operation_id.0
            ),
            source,
        }),
    }
}

fn reconcile_operation_files(
    run_directory: &Path,
    active: &HashSet<OperationId>,
) -> Result<Vec<String>, MmError> {
    if !run_directory.exists() {
        return Ok(Vec::new());
    }
    create_private_directory(run_directory)?;
    let mut removed = Vec::new();
    for entry in fs::read_dir(run_directory).map_err(|source| MmError::Io {
        context: "enumerating runtime secret operations".to_owned(),
        source,
    })? {
        let entry = entry.map_err(|source| MmError::Io {
            context: "enumerating a runtime secret operation".to_owned(),
            source,
        })?;
        if !entry
            .file_type()
            .map_err(|source| MmError::Io {
                context: "inspecting a runtime secret operation".to_owned(),
                source,
            })?
            .is_dir()
        {
            return Err(MmError::Security {
                message: "runtime secret operation storage contains a non-directory entry"
                    .to_owned(),
            });
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| MmError::Security {
                message: "runtime secret operation storage contains a non-Unicode entry".to_owned(),
            })?;
        let operation = OperationId(uuid::Uuid::parse_str(&name).map_err(|error| {
            MmError::Security {
                message: format!("runtime secret operation directory has an invalid ID: {error}"),
            }
        })?);
        if active.contains(&operation) {
            continue;
        }
        ensure_owned_by_current_user(&entry.path())?;
        fs::remove_dir_all(entry.path()).map_err(|source| MmError::Io {
            context: "removing orphaned runtime secret operation files".to_owned(),
            source,
        })?;
        removed.push(name);
    }
    Ok(removed)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
