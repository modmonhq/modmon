fn generate_ca() -> Result<(String, String), MmError> {
    let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(ca_error)?;
    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(ca_error)?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::minutes(5);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(3650);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "mm local CA");
    let cert = params.self_signed(&key_pair).map_err(ca_error)?;
    Ok((cert.pem(), key_pair.serialize_pem()))
}

fn issue_leaf(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    host: &str,
) -> Result<(String, String), MmError> {
    let ca_key = KeyPair::from_pem(ca_key_pem).map_err(ca_error)?;
    let issuer = rcgen::Issuer::from_ca_cert_pem(ca_cert_pem, ca_key).map_err(ca_error)?;
    let mut params = CertificateParams::new(vec![host.to_owned()]).map_err(ca_error)?;
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::minutes(5);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(LEAF_VALID_DAYS);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = false;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, host);
    let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(ca_error)?;
    let cert = params
        .signed_by(&key_pair, &issuer)
        .map_err(ca_error)?;
    let leaf_pem = cert.pem();
    verify_leaf_matches_host(&leaf_pem, host)?;
    Ok((leaf_pem, key_pair.serialize_pem()))
}

fn verify_leaf_matches_host(leaf_pem: &str, host: &str) -> Result<(), MmError> {
    let der = crate::domain::ca::certificate_der(leaf_pem.as_bytes()).map_err(ca_error)?;
    let normalized = crate::domain::ca::normalize_hostname(host).map_err(ca_error)?;
    use x509_parser::prelude::FromDer;
    let (_, certificate) =
        x509_parser::certificate::X509Certificate::from_der(&der).map_err(ca_error)?;
    let names = certificate
        .subject_alternative_name()
        .map_err(ca_error)?
        .ok_or_else(|| ca_error("issued leaf certificate has no subjectAltName"))?;
    let matches = names.value.general_names.iter().any(|name| {
        matches!(name, x509_parser::extensions::GeneralName::DNSName(value) if *value == normalized)
    });
    if !matches {
        return Err(ca_error(format!(
            "issued leaf certificate does not name host '{normalized}'"
        )));
    }
    Ok(())
}

fn leaf_expiration(leaf_pem: &str) -> Result<i64, MmError> {
    use x509_parser::prelude::FromDer;
    let der = crate::domain::ca::certificate_der(leaf_pem.as_bytes()).map_err(ca_error)?;
    let (_, certificate) =
        x509_parser::certificate::X509Certificate::from_der(&der).map_err(ca_error)?;
    Ok(certificate.validity().not_after.timestamp())
}

fn certificate_matches_key(certificate_pem: &str, key_pem: &str) -> bool {
    use x509_parser::prelude::FromDer;

    let Ok(key) = KeyPair::from_pem(key_pem) else {
        return false;
    };
    let Ok(der) = crate::domain::ca::certificate_der(certificate_pem.as_bytes()) else {
        return false;
    };
    let Ok((_, certificate)) = x509_parser::certificate::X509Certificate::from_der(&der) else {
        return false;
    };
    certificate.public_key().raw == key.subject_public_key_info()
}

fn persisted_leaf(certificate: &IssuedCertificate) -> PersistedLeaf {
    PersistedLeaf {
        fingerprint: certificate.fingerprint.clone(),
        generation: certificate.generation,
        expires_at_unix: certificate.expires_at_unix,
    }
}

fn restore_rotation_backup(
    store: &FilesystemLocalCaStore,
    certificate: &Path,
    key: &Path,
) -> Result<(), MmError> {
    write_atomic_bundle(&[
        (
            &store.root.join(CA_CERT_FILE),
            read_text(certificate)?.as_bytes(),
            0o644,
        ),
        (
            &store.root.join(CA_KEY_FILE),
            read_text(key)?.as_bytes(),
            0o600,
        ),
    ])?;
    remove_if_exists(certificate)?;
    remove_if_exists(key)
}

fn restore_previous_rotation(
    store: &FilesystemLocalCaStore,
    previous: &PersistedCaState,
    certificate: &Path,
    key: &Path,
    state_backup: &Path,
) -> Result<(), MmError> {
    restore_rotation_backup(store, certificate, key)?;
    let mut previous = previous.clone();
    for host in &previous.issued_hosts {
        let certificate = store.issue_host(host, previous.generation)?;
        previous
            .leaves
            .insert(host.clone(), persisted_leaf(&certificate));
    }
    store.save_state(&previous)?;
    remove_if_exists(state_backup)
}

fn remove_if_exists(path: &Path) -> Result<(), MmError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(MmError::Io {
            context: format!("removing {}", path.display()),
            source,
        }),
    }
}

#[cfg(unix)]
fn key_permissions_valid(path: &Path) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file()
            && metadata.uid() == rustix::process::geteuid().as_raw()
            && metadata.permissions().mode() & 0o077 == 0
            && metadata.nlink() == 1
    })
}

#[cfg(not(unix))]
fn key_permissions_valid(path: &Path) -> bool {
    path.is_file()
}

fn prepare_private_directory(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating local CA directory {}", path.display()),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!("setting local CA directory permissions {}", path.display()),
                source,
            }
        })?;
    }
    ensure_user_controlled_directory(path)
}

fn write_atomic_bundle(files: &[(&Path, &[u8], u32)]) -> Result<(), MmError> {
    let mut staged = Vec::with_capacity(files.len());
    for (path, bytes, mode) in files {
        staged.push((write_private_temp(path, bytes, *mode)?, *path));
    }
    for (temp, dest) in &staged {
        fs::rename(temp, dest).map_err(|source| MmError::Io {
            context: format!("publishing {}", dest.display()),
            source,
        })?;
    }
    if let Some((path, _, _)) = files.first()
        && let Some(parent) = path.parent()
    {
        sync_directory(parent)?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8], mode: u32) -> Result<(), MmError> {
    write_atomic_bundle(&[(path, bytes, mode)])
}

fn write_private_temp(path: &Path, bytes: &[u8], _mode: u32) -> Result<PathBuf, MmError> {
    ensure_no_symlink_components(path)?;
    let parent = path.parent().ok_or_else(|| MmError::Io {
        context: format!("resolving parent for {}", path.display()),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent"),
    })?;
    let name = path.file_name().ok_or_else(|| MmError::Io {
        context: format!("resolving file name for {}", path.display()),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name"),
    })?;
    let temp = parent.join(format!(".{}.tmp", name.to_string_lossy()));
    if temp.exists() {
        fs::remove_file(&temp).map_err(|source| MmError::Io {
            context: format!("removing incomplete {}", temp.display()),
            source,
        })?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(_mode);
    }
    let mut file = options.open(&temp).map_err(|source| MmError::Io {
        context: format!("creating {}", temp.display()),
        source,
    })?;
    file.write_all(bytes).map_err(|source| MmError::Io {
        context: format!("writing {}", temp.display()),
        source,
    })?;
    file.sync_all().map_err(|source| MmError::Io {
        context: format!("synchronizing {}", temp.display()),
        source,
    })?;
    Ok(temp)
}

fn sync_directory(path: &Path) -> Result<(), MmError> {
    let directory = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|source| MmError::Io {
            context: format!("opening directory {}", path.display()),
            source,
        })?;
    directory.sync_all().map_err(|source| MmError::Io {
        context: format!("synchronizing directory {}", path.display()),
        source,
    })
}

fn read_text(path: &Path) -> Result<String, MmError> {
    let bytes = read_regular_file(path, MAX_CERT_BYTES)?.ok_or_else(|| MmError::State {
        message: format!("local CA file {} is missing", path.display()),
    })?;
    String::from_utf8(bytes).map_err(|_| MmError::State {
        message: format!("local CA file {} is not valid UTF-8", path.display()),
    })
}

fn ca_error(error: impl std::fmt::Display) -> MmError {
    MmError::Target {
        message: format!("local CA operation failed: {error}"),
    }
}
