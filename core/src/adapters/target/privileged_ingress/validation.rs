struct VerifiedPrivilegedArtifact {
    haproxy: Vec<u8>,
    unit: Vec<u8>,
}

#[cfg(unix)]
fn installed_forwarder_matches(plan: &TargetIngressPlan) -> Result<bool, MmError> {
    let owner = match read_existing_fixed(Path::new(OWNER_FILE)) {
        Ok(Some(bytes)) => serde_json::from_slice::<HostIngressOwner>(&bytes).map_err(|error| MmError::Security {
            message: format!("existing ingress host ownership is invalid: {error}"),
        })?,
        Ok(None) => return Ok(false),
        // Older installations kept attribution private. One reviewed repair
        // publishes it; never bypass ownership verification in that case.
        Err(MmError::Io { source, .. }) if source.kind() == std::io::ErrorKind::PermissionDenied => return Ok(false),
        Err(error) => return Err(error),
    };
    if owner.target_id != plan.target_id || owner.source_uid != rustix::process::getuid().as_raw() {
        return Ok(false);
    }
    let config = read_existing_fixed(Path::new(HAPROXY_CONFIG))?;
    let unit = read_existing_fixed(Path::new(SYSTEM_UNIT))?;
    if !installed_files_match(plan.target_id, config.as_deref(), unit.as_deref()) { return Ok(false); }
    let running = Command::new("/usr/bin/systemctl")
        .args(["is-active", "--quiet", "mm-ingress-forwarder.service"])
        .env_clear().status().is_ok_and(|status| status.success());
    Ok(running && verify_public_forwarder().is_ok())
}

/// Any byte difference, including files written by an older mm template,
/// sends the caller through the reviewed reinstall path.
#[cfg(unix)]
fn installed_files_match(target_id: TargetId, config: Option<&[u8]>, unit: Option<&[u8]>) -> bool {
    let (http, https, _) = crate::domain::target_ingress::ingress_loopback_ports(target_id);
    config == Some(haproxy_config(http, https).as_bytes())
        && unit == Some(haproxy_service_unit().as_bytes())
}

#[cfg(not(unix))]
fn installed_forwarder_matches(_plan: &TargetIngressPlan) -> Result<bool, MmError> { Ok(false) }

fn verify_fixed_templates(
    target_id: TargetId,
    artifact: &VerifiedPrivilegedArtifact,
) -> Result<(), MmError> {
    let (proxy_http_port, proxy_https_port, _) =
        crate::domain::target_ingress::ingress_loopback_ports(target_id);
    if artifact.haproxy == haproxy_config(proxy_http_port, proxy_https_port).as_bytes()
        && artifact.unit == haproxy_service_unit().as_bytes()
    {
        Ok(())
    } else {
        Err(MmError::Security {
            message: "privileged ingress files do not match mm's fixed templates".to_owned(),
        })
    }
}

fn verify_source_artifact(
    root: &Path,
    target_id: TargetId,
    artifact_hash: &str,
    source_uid: u32,
) -> Result<VerifiedPrivilegedArtifact, MmError> {
    ensure_no_symlink_components(root)?;
    verify_source_root(root, target_id, artifact_hash, source_uid)?;
    let manifest_path = root.join("manifest.json");
    verify_source_file(&manifest_path, source_uid)?;
    let manifest = verified_manifest(&manifest_path, target_id, artifact_hash)?;
    verify_support_files(root, source_uid, &manifest)?;
    read_fixed_artifacts(root, source_uid, &manifest)
}

fn verify_source_root(
    root: &Path,
    target_id: TargetId,
    artifact_hash: &str,
    source_uid: u32,
) -> Result<(), MmError> {
    use std::os::unix::fs::MetadataExt;

    let canonical = dunce::canonicalize(root).map_err(|source| MmError::Io {
        context: format!("resolving ingress artifact {}", root.display()),
        source,
    })?;
    let target_id_text = target_id.0.to_string();
    let target_directory = root.parent().and_then(Path::file_name).and_then(|name| name.to_str());
    if canonical != root
        || root.file_name().and_then(|name| name.to_str()) != Some(artifact_hash)
        || target_directory != Some(target_id_text.as_str())
    {
        return Err(MmError::Security {
            message: "privileged ingress artifact path does not match its target and hash".to_owned(),
        });
    }
    let metadata = fs::symlink_metadata(root).map_err(|source| MmError::Io {
        context: format!("inspecting ingress artifact {}", root.display()),
        source,
    })?;
    if !metadata.is_dir() || metadata.uid() != source_uid || metadata.mode() & 0o022 != 0 {
        return Err(MmError::Security {
            message: "ingress artifact is not a directory owned by the invoking user".to_owned(),
        });
    }
    Ok(())
}

fn verified_manifest(
    path: &Path,
    target_id: TargetId,
    artifact_hash: &str,
) -> Result<TargetIngressArtifactManifest, MmError> {
    let bytes = read_regular_file(path, MAX_ARTIFACT_BYTES)?.ok_or_else(|| MmError::Security {
        message: "ingress artifact manifest disappeared".to_owned(),
    })?;
    let manifest: TargetIngressArtifactManifest =
        serde_json::from_slice(&bytes).map_err(|error| MmError::Security {
            message: format!("ingress artifact manifest is invalid: {error}"),
        })?;
    if manifest.target_id != target_id
        || manifest.artifact_hash != artifact_hash
        || manifest.canonical_hash() != artifact_hash
    {
        return Err(MmError::Security {
            message: "ingress artifact manifest does not match the reviewed target and hash".to_owned(),
        });
    }
    Ok(manifest)
}

fn verify_support_files(
    root: &Path,
    source_uid: u32,
    manifest: &TargetIngressArtifactManifest,
) -> Result<(), MmError> {
    let names = manifest.support_files.iter().map(|file| file.name.as_str()).collect::<BTreeSet<_>>();
    if manifest.support_files.len() != INGRESS_SUPPORT_FILES.len()
        || names.len() != INGRESS_SUPPORT_FILES.len()
        || INGRESS_SUPPORT_FILES.iter().any(|name| !names.contains(name))
    {
        return Err(MmError::Security {
            message: "privileged ingress artifact has an unexpected support-file set".to_owned(),
        });
    }
    let mut expected = BTreeSet::from(["compose.yaml".to_owned(), "manifest.json".to_owned()]);
    for support in &manifest.support_files {
        validate_support_name(&support.name)?;
        if !expected.insert(support.name.clone()) {
            return Err(MmError::Security {
                message: format!("ingress artifact contains duplicate file entry '{}'", support.name),
            });
        }
        let path = root.join(&support.name);
        verify_source_file(&path, source_uid)?;
        let bytes = read_regular_file(&path, MAX_ARTIFACT_BYTES)?.ok_or_else(|| MmError::Security {
            message: format!("ingress support file '{}' disappeared", support.name),
        })?;
        if blake3::hash(&bytes).to_hex().as_str() != support.digest {
            return Err(MmError::Security {
                message: format!("ingress support file '{}' failed digest verification", support.name),
            });
        }
    }
    verify_exact_file_set(root, &expected)
}

fn verify_exact_file_set(root: &Path, expected: &BTreeSet<String>) -> Result<(), MmError> {
    let entries = fs::read_dir(root).map_err(|source| MmError::Io {
        context: format!("reading ingress artifact {}", root.display()),
        source,
    })?;
    let mut actual = BTreeSet::new();
    for entry in entries {
        let name = entry.map_err(|source| MmError::Io {
            context: format!("reading ingress artifact {}", root.display()),
            source,
        })?.file_name().into_string().map_err(|_| MmError::Security {
            message: "ingress artifact contains a non-Unicode file name".to_owned(),
        })?;
        actual.insert(name);
        if actual.len() > expected.len() {
            return Err(MmError::Security { message: "ingress artifact contains unexpected files".to_owned() });
        }
    }
    if actual != *expected {
        return Err(MmError::Security { message: "ingress artifact contains unexpected files".to_owned() });
    }
    Ok(())
}

fn read_fixed_artifacts(
    root: &Path,
    source_uid: u32,
    manifest: &TargetIngressArtifactManifest,
) -> Result<VerifiedPrivilegedArtifact, MmError> {
    let compose_path = root.join("compose.yaml");
    verify_source_file(&compose_path, source_uid)?;
    let compose =
        read_regular_file(&compose_path, MAX_ARTIFACT_BYTES)?.ok_or_else(|| MmError::Security {
            message: "ingress Compose artifact disappeared".to_owned(),
        })?;
    if blake3::hash(&compose).to_hex().as_str() != manifest.compose_digest {
        return Err(MmError::Security {
            message: "ingress Compose artifact failed digest verification".to_owned(),
        });
    }

    let haproxy_manifest = manifest
        .support_files
        .iter()
        .find(|file| file.name == "haproxy.cfg")
        .filter(|file| file.destination == HAPROXY_CONFIG && file.mode == "0644")
        .ok_or_else(|| MmError::Security {
            message: "ingress artifact has no canonical HAProxy configuration".to_owned(),
        })?;
    let unit_manifest = manifest
        .support_files
        .iter()
        .find(|file| file.name == "mm-ingress-forwarder.service")
        .filter(|file| file.destination == SYSTEM_UNIT && file.mode == "0644")
        .ok_or_else(|| MmError::Security {
            message: "ingress artifact has no canonical privileged systemd unit".to_owned(),
        })?;
    let haproxy_path = root.join(&haproxy_manifest.name);
    let unit_path = root.join(&unit_manifest.name);
    Ok(VerifiedPrivilegedArtifact {
        haproxy: read_regular_file(&haproxy_path, MAX_ARTIFACT_BYTES)?.ok_or_else(|| {
            MmError::Security {
                message: "HAProxy artifact disappeared".to_owned(),
            }
        })?,
        unit: read_regular_file(&unit_path, MAX_ARTIFACT_BYTES)?.ok_or_else(|| {
            MmError::Security {
                message: "systemd unit artifact disappeared".to_owned(),
            }
        })?,
    })
}

fn verify_source_file(path: &Path, source_uid: u32) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting ingress artifact file {}", path.display()),
        source,
    })?;
    if !metadata.file_type().is_file()
        || metadata.uid() != source_uid
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(MmError::Security {
            message: format!(
                "ingress artifact file '{}' is not a protected regular file owned by the invoking user",
                path.display()
            ),
        });
    }
    Ok(())
}

struct ValidationFiles {
    haproxy: PathBuf,
    unit: PathBuf,
}

#[cfg(unix)]
impl ValidationFiles {
    fn new(haproxy: &[u8], unit: &[u8]) -> Result<Self, MmError> {
        let nonce = uuid::Uuid::new_v4();
        let parent = Path::new("/etc/mm/ingress");
        let haproxy_path = parent.join(format!(".validate-{nonce}.cfg"));
        let unit_path = parent.join(format!(".validate-{nonce}.service"));
        write_validation_file(&haproxy_path, haproxy)?;
        if let Err(error) = write_validation_file(&unit_path, unit) {
            return match remove_validation_file(&haproxy_path) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(MmError::operation_failure(
                    nonce.to_string(),
                    crate::domain::error::MutationStatus::NotStarted,
                    error,
                    vec![cleanup.to_string()],
                    "remove the privileged ingress validation file before retrying",
                )),
            };
        }
        Ok(Self {
            haproxy: haproxy_path,
            unit: unit_path,
        })
    }
}

#[cfg(unix)]
impl Drop for ValidationFiles {
    fn drop(&mut self) {
        for path in [&self.haproxy, &self.unit] {
            if let Err(error) = remove_validation_file(path) {
                tracing::warn!(path = %path.display(), %error, "privileged ingress validation cleanup failed");
            }
        }
    }
}

#[cfg(unix)]
fn remove_validation_file(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn write_validation_file(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    use std::os::unix::fs::OpenOptionsExt;

    ensure_no_symlink_components(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|source| MmError::Io {
            context: format!("creating privileged validation file {}", path.display()),
            source,
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing privileged validation file {}", path.display()),
            source,
        })
}

#[cfg(unix)]
fn host_lock() -> Result<File, MmError> {
    use rustix::fs::{Mode, OFlags};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    ensure_no_symlink_components(Path::new(HOST_LOCK))?;
    let descriptor = rustix::fs::open(
        HOST_LOCK,
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|error| MmError::Io {
        context: "opening the host ingress lock".to_owned(),
        source: error.into(),
    })?;
    let file = File::from(descriptor);
    let metadata = file.metadata().map_err(|source| MmError::Io {
        context: "inspecting the host ingress lock".to_owned(),
        source,
    })?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(MmError::Security {
            message: "the host ingress lock is not a protected root-owned regular file".to_owned(),
        });
    }
    match <File as FileExt>::try_lock(&file) {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            return Err(MmError::State {
                message: "another privileged ingress installation is in progress".to_owned(),
            });
        }
        Err(TryLockError::Error(source)) => {
            return Err(MmError::Io {
                context: "locking the host ingress installer".to_owned(),
                source,
            });
        }
    }
    Ok(file)
}

#[cfg(unix)]
fn ensure_fixed_directories() -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    for path in [Path::new("/etc/mm"), Path::new("/etc/mm/ingress")] {
        ensure_no_symlink_components(path)?;
        if !path.exists() {
            fs::create_dir(path).map_err(|source| MmError::Io {
                context: format!("creating privileged ingress directory {}", path.display()),
                source,
            })?;
        }
        let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
            context: format!("inspecting privileged ingress directory {}", path.display()),
            source,
        })?;
        if !metadata.is_dir() || metadata.uid() != 0 {
            return Err(MmError::Security {
                message: format!(
                    "privileged ingress path '{}' is not a root-owned directory",
                    path.display()
                ),
            });
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|source| {
            MmError::Io {
                context: format!("setting permissions on {}", path.display()),
                source,
            }
        })?;
    }
    let systemd_directory = Path::new("/etc/systemd/system");
    ensure_no_symlink_components(systemd_directory)?;
    let metadata = fs::symlink_metadata(systemd_directory).map_err(|source| MmError::Io {
        context: "inspecting the systemd unit directory".to_owned(),
        source,
    })?;
    if !metadata.is_dir() || metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
        return Err(MmError::Security {
            message: "the systemd unit directory is not a protected root-owned directory"
                .to_owned(),
        });
    }
    Ok(())
}

#[cfg(unix)]
fn validate_owner(target_id: TargetId, source_uid: u32) -> Result<(), MmError> {
    let owner_path = Path::new(OWNER_FILE);
    match read_existing_fixed(owner_path)? {
        Some(bytes) => {
            let owner: HostIngressOwner =
                serde_json::from_slice(&bytes).map_err(|error| MmError::Security {
                    message: format!("existing ingress host ownership is invalid: {error}"),
                })?;
            if owner.target_id != target_id || owner.source_uid != source_uid {
                return Err(MmError::Security {
                    message: "this host's public ingress ports are owned by another target or user"
                        .to_owned(),
                });
            }
        }
        None => {
            if Path::new(HAPROXY_CONFIG).exists() || Path::new(SYSTEM_UNIT).exists() {
                return Err(MmError::Security {
                    message: "refusing to adopt existing unmanaged ingress files".to_owned(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn read_existing_fixed(path: &Path) -> Result<Option<Vec<u8>>, MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    ensure_no_symlink_components(path)?;
    let bytes = read_regular_file(path, MAX_ARTIFACT_BYTES)?;
    if bytes.is_some() {
        let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
            context: format!("inspecting privileged ingress file {}", path.display()),
            source,
        })?;
        if !metadata.is_file()
            || metadata.uid() != 0
            || metadata.nlink() != 1
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err(MmError::Security {
                message: format!(
                    "privileged ingress file '{}' is not a protected root-owned regular file",
                    path.display()
                ),
            });
        }
    }
    Ok(bytes)
}

#[cfg(unix)]
fn restore_fixed(path: &Path, previous: Option<&[u8]>, mode: u32) -> Result<(), MmError> {
    if let Some(previous) = previous {
        atomic_replace(path, previous, mode)
    } else if path.exists() {
        fs::remove_file(path).map_err(|source| MmError::Io {
            context: format!("removing failed ingress file {}", path.display()),
            source,
        })
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn rollback_host_install(
    previous_owner: Option<&[u8]>,
    previous_config: Option<&[u8]>,
    previous_unit: Option<&[u8]>,
    service_attempted: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    let previous_complete =
        previous_owner.is_some() && previous_config.is_some() && previous_unit.is_some();
    if !previous_complete && service_attempted {
        retain_cleanup_error(
            &mut failures,
            run_checked(
                "/usr/bin/systemctl",
                &["disable", "--now", "mm-ingress-forwarder.service"],
            ),
        );
    }
    retain_cleanup_error(
        &mut failures,
        restore_fixed(Path::new(HAPROXY_CONFIG), previous_config, 0o644),
    );
    retain_cleanup_error(
        &mut failures,
        restore_fixed(Path::new(SYSTEM_UNIT), previous_unit, 0o644),
    );
    retain_cleanup_error(
        &mut failures,
        restore_fixed(Path::new(OWNER_FILE), previous_owner, 0o600),
    );
    retain_cleanup_error(
        &mut failures,
        run_checked("/usr/bin/systemctl", &["daemon-reload"]),
    );
    if previous_complete {
        retain_cleanup_error(
            &mut failures,
            run_checked(
                "/usr/bin/systemctl",
                &["enable", "mm-ingress-forwarder.service"],
            ),
        );
        retain_cleanup_error(
            &mut failures,
            run_checked(
                "/usr/bin/systemctl",
                &["restart", "mm-ingress-forwarder.service"],
            ),
        );
        retain_cleanup_error(&mut failures, verify_public_forwarder());
    }
    failures
}

#[cfg(unix)]
fn retain_cleanup_error(failures: &mut Vec<String>, result: Result<(), MmError>) {
    if let Err(error) = result {
        failures.push(error.to_string());
    }
}
