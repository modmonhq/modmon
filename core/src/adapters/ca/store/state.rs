#[path = "../trust/mod.rs"]
mod trust;

pub use trust::PlatformTrustStore;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose, PublicKeyData,
};
use serde::{Deserialize, Serialize};

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_user_controlled_directory, read_regular_file,
};
use crate::domain::ca::{
    IssuedCertificate, LeafCertificateStatus, LocalCaStatus, certificate_fingerprint,
    is_local_ca_host, local_ca_compatibility_warning, validate_hostname,
};
use crate::domain::error::MmError;

const CA_CERT_FILE: &str = "ca.crt";
const CA_KEY_FILE: &str = "ca.key";
const STATE_FILE: &str = "state.json";
const ISSUED_DIRECTORY: &str = "issued";
const MAX_STATE_BYTES: usize = 64 * 1024;
const MAX_CERT_BYTES: usize = 16 * 1024;
const STATE_VERSION: u32 = 1;
const LEAF_VALID_DAYS: i64 = 30;
const LEAF_RENEWAL_DAYS: i64 = 7;
const ROTATION_CERT_BACKUP: &str = ".ca.crt.rotation-backup";
const ROTATION_KEY_BACKUP: &str = ".ca.key.rotation-backup";
const ROTATION_STATE_BACKUP: &str = ".state.rotation-backup";

#[derive(Debug, Clone)]
pub struct FilesystemLocalCaStore {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedCaState {
    version: u32,
    generation: u64,
    fingerprint: String,
    issued_hosts: BTreeSet<String>,
    #[serde(default)]
    trust_disabled: bool,
    #[serde(default)]
    leaves: BTreeMap<String, PersistedLeaf>,
    #[serde(default)]
    rotation_pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedLeaf {
    fingerprint: String,
    generation: u64,
    expires_at_unix: i64,
}

impl FilesystemLocalCaStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn status(&self) -> Result<LocalCaStatus, MmError> {
        if !self.root.join(CA_CERT_FILE).is_file() {
            return Ok(LocalCaStatus {
                installed: false,
                fingerprint: None,
                generation: 0,
                issued_hosts: BTreeSet::new(),
                key_permissions_valid: false,
                trust_disabled: false,
                rotation_pending: false,
                leaves: Vec::new(),
                trust: None,
            });
        }
        ensure_user_controlled_directory(&self.root)?;
        let state = self.load_state()?;
        let ca_certificate = read_text(&self.root.join(CA_CERT_FILE))?;
        let fingerprint = certificate_fingerprint(ca_certificate.as_bytes())
            .map_err(ca_error)?
            .to_string();
        if fingerprint != state.fingerprint {
            return Err(MmError::Security {
                message: "local CA certificate fingerprint does not match its owned state"
                    .to_owned(),
            });
        }
        let ca_key = read_text(&self.root.join(CA_KEY_FILE))?;
        let key_permissions_valid = key_permissions_valid(&self.root.join(CA_KEY_FILE))
            && certificate_matches_key(&ca_certificate, &ca_key);
        let mut leaves = Vec::with_capacity(state.leaves.len());
        for (host, leaf) in &state.leaves {
            leaves.push(LeafCertificateStatus {
                hosts: vec![host.clone()],
                fingerprint: leaf.fingerprint.clone(),
                generation: leaf.generation,
                expires_at_unix: leaf.expires_at_unix,
                stale: !self.existing_leaf_valid(host, leaf, state.generation)?,
            });
        }
        Ok(LocalCaStatus {
            installed: true,
            fingerprint: Some(fingerprint),
            generation: state.generation,
            issued_hosts: state.issued_hosts,
            key_permissions_valid,
            trust_disabled: state.trust_disabled,
            rotation_pending: state.rotation_pending,
            leaves,
            trust: None,
        })
    }

    pub fn install(&self) -> Result<LocalCaStatus, MmError> {
        if self.root.join(CA_CERT_FILE).is_file() {
            return self.status();
        }
        prepare_private_directory(&self.root)?;
        let (cert_pem, key_pem) = generate_ca()?;
        write_atomic_bundle(&[
            (&self.root.join(CA_CERT_FILE), cert_pem.as_bytes(), 0o644),
            (&self.root.join(CA_KEY_FILE), key_pem.as_bytes(), 0o600),
        ])?;
        prepare_private_directory(&self.root.join(ISSUED_DIRECTORY))?;
        let state = PersistedCaState {
            version: STATE_VERSION,
            generation: 1,
            fingerprint: certificate_fingerprint(cert_pem.as_bytes())
                .map_err(ca_error)?
                .to_string(),
            issued_hosts: BTreeSet::new(),
            trust_disabled: false,
            leaves: BTreeMap::new(),
            rotation_pending: false,
        };
        self.save_state(&state)?;
        self.status()
    }

    pub fn rotate(&self) -> Result<LocalCaStatus, MmError> {
        let previous = self.require_installed()?;
        let (cert_pem, key_pem) = generate_ca()?;
        if previous.rotation_pending {
            return Err(MmError::State {
                message:
                    "a local CA rotation is pending; run `mm dev tls repair` before rotating again"
                        .to_owned(),
            });
        }
        let backup_cert = self.root.join(ROTATION_CERT_BACKUP);
        let backup_key = self.root.join(ROTATION_KEY_BACKUP);
        let backup_state = self.root.join(ROTATION_STATE_BACKUP);
        let state_bytes = serde_json::to_vec_pretty(&previous).map_err(|error| MmError::State {
            message: format!("could not serialize CA rotation backup: {error}"),
        })?;
        write_atomic_bundle(&[
            (
                &backup_cert,
                read_text(&self.root.join(CA_CERT_FILE))?.as_bytes(),
                0o600,
            ),
            (
                &backup_key,
                read_text(&self.root.join(CA_KEY_FILE))?.as_bytes(),
                0o600,
            ),
            (&backup_state, &state_bytes, 0o600),
        ])?;
        write_atomic_bundle(&[
            (&self.root.join(CA_CERT_FILE), cert_pem.as_bytes(), 0o644),
            (&self.root.join(CA_KEY_FILE), key_pem.as_bytes(), 0o600),
        ])?;
        let generation = previous.generation + 1;
        let mut issued = BTreeSet::new();
        let mut leaves = BTreeMap::new();
        for host in &previous.issued_hosts {
            match self.issue_host(host, generation) {
                Ok(certificate) => {
                    leaves.insert(host.clone(), persisted_leaf(&certificate));
                    issued.insert(host.clone());
                }
                Err(error) => {
                    restore_previous_rotation(
                        self,
                        &previous,
                        &backup_cert,
                        &backup_key,
                        &backup_state,
                    )?;
                    return Err(error);
                }
            }
        }
        let state = PersistedCaState {
            version: STATE_VERSION,
            generation,
            fingerprint: certificate_fingerprint(cert_pem.as_bytes())
                .map_err(ca_error)?
                .to_string(),
            issued_hosts: issued,
            trust_disabled: previous.trust_disabled,
            leaves,
            rotation_pending: true,
        };
        if let Err(error) = self.save_state(&state) {
            restore_previous_rotation(self, &previous, &backup_cert, &backup_key, &backup_state)?;
            return Err(error);
        }
        self.status()
    }

    pub fn finish_rotation(&self) -> Result<LocalCaStatus, MmError> {
        let mut state = self.require_installed()?;
        if state.rotation_pending {
            state.rotation_pending = false;
            self.save_state(&state)?;
        }
        remove_if_exists(&self.root.join(ROTATION_CERT_BACKUP))?;
        remove_if_exists(&self.root.join(ROTATION_KEY_BACKUP))?;
        remove_if_exists(&self.root.join(ROTATION_STATE_BACKUP))?;
        self.status()
    }

    pub fn rollback_rotation(&self) -> Result<LocalCaStatus, MmError> {
        let backup_cert = self.root.join(ROTATION_CERT_BACKUP);
        let backup_key = self.root.join(ROTATION_KEY_BACKUP);
        let backup_state = self.root.join(ROTATION_STATE_BACKUP);
        let bytes =
            read_regular_file(&backup_state, MAX_STATE_BYTES)?.ok_or_else(|| MmError::State {
                message: "local CA rotation backup is missing; refusing an unsafe rollback"
                    .to_owned(),
            })?;
        let mut previous: PersistedCaState =
            serde_json::from_slice(&bytes).map_err(|error| MmError::State {
                message: format!("local CA rotation backup is invalid: {error}"),
            })?;
        restore_rotation_backup(self, &backup_cert, &backup_key)?;
        for host in &previous.issued_hosts {
            let certificate = self.issue_host(host, previous.generation)?;
            previous
                .leaves
                .insert(host.clone(), persisted_leaf(&certificate));
        }
        self.save_state(&previous)?;
        remove_if_exists(&backup_state)?;
        self.status()
    }

    pub fn rotation_previous_fingerprint(&self) -> Result<Option<String>, MmError> {
        let path = self.root.join(ROTATION_STATE_BACKUP);
        let Some(bytes) = read_regular_file(&path, MAX_STATE_BYTES)? else {
            return Ok(None);
        };
        let previous: PersistedCaState =
            serde_json::from_slice(&bytes).map_err(|error| MmError::State {
                message: format!("local CA rotation backup is invalid: {error}"),
            })?;
        Ok(Some(previous.fingerprint))
    }

    pub fn uninstall(&self) -> Result<LocalCaStatus, MmError> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root).map_err(|source| MmError::Io {
                context: format!("removing local CA directory {}", self.root.display()),
                source,
            })?;
        }
        self.status()
    }

    pub fn issue(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        let mut state = self.require_installed()?;
        let mut issued = Vec::new();
        for host in hosts {
            let host = validate_hostname(host).map_err(|message| MmError::Target { message })?;
            if !is_local_ca_host(&host) {
                return Err(MmError::Target {
                    message: format!(
                        "hostname '{host}' is not a reserved local name; local CA issues only .test, .localhost, .local, and .invalid names"
                    ),
                });
            }
            if let Some(warning) = local_ca_compatibility_warning(&host) {
                tracing::warn!("{warning}");
            }
            let certificate = if let Some(existing) = state.leaves.get(&host)
                && self.existing_leaf_valid(&host, existing, state.generation)?
            {
                IssuedCertificate {
                    hosts: vec![host.clone()],
                    fingerprint: existing.fingerprint.clone(),
                    generation: existing.generation,
                    expires_at_unix: existing.expires_at_unix,
                }
            } else {
                self.issue_host(&host, state.generation)?
            };
            state.issued_hosts.insert(host);
            state
                .leaves
                .insert(certificate.hosts[0].clone(), persisted_leaf(&certificate));
            issued.push(certificate);
        }
        self.save_state(&state)?;
        Ok(issued)
    }

    pub fn certificate_pem(&self, host: &str) -> Result<String, MmError> {
        read_text(&self.issued_path(host, "crt"))
    }

    pub fn key_pem(&self, host: &str) -> Result<String, MmError> {
        read_text(&self.issued_path(host, "key"))
    }

    pub fn ca_certificate_pem(&self) -> Result<String, MmError> {
        self.require_installed()?;
        read_text(&self.root.join(CA_CERT_FILE))
    }

    pub fn set_trust_disabled(&self, disabled: bool) -> Result<LocalCaStatus, MmError> {
        let mut state = self.require_installed()?;
        state.trust_disabled = disabled;
        self.save_state(&state)?;
        self.status()
    }

    pub fn reconcile(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        let normalized = hosts
            .iter()
            .map(|host| validate_hostname(host).map_err(|message| MmError::Target { message }))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let issued = self.issue(&normalized)?;
        let mut state = self.require_installed()?;
        let removed = state
            .issued_hosts
            .difference(&normalized)
            .cloned()
            .collect::<Vec<_>>();
        for host in removed {
            remove_if_exists(&self.issued_path(&host, "crt"))?;
            remove_if_exists(&self.issued_path(&host, "key"))?;
            state.issued_hosts.remove(&host);
            state.leaves.remove(&host);
        }
        self.save_state(&state)?;
        Ok(issued)
    }

    pub fn certificate_path(&self, host: &str) -> Result<PathBuf, MmError> {
        let host = validate_hostname(host).map_err(|message| MmError::Target { message })?;
        let path = self.issued_path(&host, "crt");
        ensure_no_symlink_components(&path)?;
        read_regular_file(&path, MAX_CERT_BYTES)?.ok_or_else(|| MmError::State {
            message: format!("local certificate for '{host}' is missing"),
        })?;
        Ok(path)
    }

    pub fn key_path(&self, host: &str) -> Result<PathBuf, MmError> {
        let host = validate_hostname(host).map_err(|message| MmError::Target { message })?;
        let path = self.issued_path(&host, "key");
        ensure_no_symlink_components(&path)?;
        if !key_permissions_valid(&path) {
            return Err(MmError::Security {
                message: format!(
                    "local certificate key '{}' is not private or is not owned by the current user",
                    path.display()
                ),
            });
        }
        Ok(path)
    }

    fn issue_host(&self, host: &str, generation: u64) -> Result<IssuedCertificate, MmError> {
        let ca_cert_pem = read_text(&self.root.join(CA_CERT_FILE))?;
        let ca_key_pem = read_text(&self.root.join(CA_KEY_FILE))?;
        if !certificate_matches_key(&ca_cert_pem, &ca_key_pem) {
            return Err(MmError::Security {
                message: "local CA certificate and private key do not match".to_owned(),
            });
        }
        let (cert_pem, key_pem) = issue_leaf(&ca_cert_pem, &ca_key_pem, host)?;
        prepare_private_directory(&self.root.join(ISSUED_DIRECTORY))?;
        write_atomic_bundle(&[
            (&self.issued_path(host, "crt"), cert_pem.as_bytes(), 0o644),
            (&self.issued_path(host, "key"), key_pem.as_bytes(), 0o600),
        ])?;
        let expires_at_unix = leaf_expiration(&cert_pem)?;
        Ok(IssuedCertificate {
            hosts: vec![host.to_owned()],
            fingerprint: certificate_fingerprint(cert_pem.as_bytes())
                .map_err(ca_error)?
                .to_string(),
            generation,
            expires_at_unix,
        })
    }

    fn existing_leaf_valid(
        &self,
        host: &str,
        leaf: &PersistedLeaf,
        generation: u64,
    ) -> Result<bool, MmError> {
        if leaf.generation != generation
            || leaf.expires_at_unix
                <= time::OffsetDateTime::now_utc().unix_timestamp()
                    + time::Duration::days(LEAF_RENEWAL_DAYS).whole_seconds()
        {
            return Ok(false);
        }
        let certificate_path = self.issued_path(host, "crt");
        let key_path = self.issued_path(host, "key");
        ensure_no_symlink_components(&certificate_path)?;
        ensure_no_symlink_components(&key_path)?;
        if !key_permissions_valid(&key_path) {
            return Ok(false);
        }
        let Some(certificate) = read_regular_file(&certificate_path, MAX_CERT_BYTES)? else {
            return Ok(false);
        };
        let Some(key) = read_regular_file(&key_path, MAX_CERT_BYTES)? else {
            return Ok(false);
        };
        let (Ok(certificate), Ok(key)) =
            (std::str::from_utf8(&certificate), std::str::from_utf8(&key))
        else {
            return Ok(false);
        };
        Ok(certificate_fingerprint(certificate.as_bytes())
            .is_ok_and(|fingerprint| fingerprint.to_string() == leaf.fingerprint)
            && leaf_expiration(certificate).is_ok_and(|expires| expires == leaf.expires_at_unix)
            && verify_leaf_matches_host(certificate, host).is_ok()
            && certificate_matches_key(certificate, key))
    }

    fn issued_path(&self, host: &str, extension: &str) -> PathBuf {
        self.root
            .join(ISSUED_DIRECTORY)
            .join(format!("{host}.{extension}"))
    }

    fn require_installed(&self) -> Result<PersistedCaState, MmError> {
        if !self.root.join(CA_CERT_FILE).is_file() || !self.root.join(CA_KEY_FILE).is_file() {
            return Err(MmError::Target {
                message: "local CA is not installed; run `mm dev setup`".to_owned(),
            });
        }
        ensure_user_controlled_directory(&self.root)?;
        self.load_state()
    }

    fn load_state(&self) -> Result<PersistedCaState, MmError> {
        let bytes =
            read_regular_file(&self.root.join(STATE_FILE), MAX_STATE_BYTES)?.ok_or_else(|| {
                MmError::State {
                    message: "local CA state file is missing".to_owned(),
                }
            })?;
        let state: PersistedCaState =
            serde_json::from_slice(&bytes).map_err(|error| MmError::State {
                message: format!("local CA state is invalid: {error}"),
            })?;
        if state.version != STATE_VERSION {
            return Err(MmError::State {
                message: format!(
                    "local CA state version {} is unsupported; expected {STATE_VERSION}",
                    state.version
                ),
            });
        }
        Ok(state)
    }

    fn save_state(&self, state: &PersistedCaState) -> Result<(), MmError> {
        let mut bytes = serde_json::to_vec_pretty(state).map_err(|error| MmError::State {
            message: format!("could not serialize local CA state: {error}"),
        })?;
        bytes.push(b'\n');
        write_private_file(&self.root.join(STATE_FILE), &bytes, 0o600)
    }
}

impl crate::app::ports::LocalCaStore for FilesystemLocalCaStore {
    fn status(&self) -> Result<LocalCaStatus, MmError> {
        Self::status(self)
    }

    fn install(&self) -> Result<LocalCaStatus, MmError> {
        Self::install(self)
    }

    fn rotate(&self) -> Result<LocalCaStatus, MmError> {
        Self::rotate(self)
    }

    fn finish_rotation(&self) -> Result<LocalCaStatus, MmError> {
        Self::finish_rotation(self)
    }

    fn rollback_rotation(&self) -> Result<LocalCaStatus, MmError> {
        Self::rollback_rotation(self)
    }

    fn rotation_previous_fingerprint(&self) -> Result<Option<String>, MmError> {
        Self::rotation_previous_fingerprint(self)
    }

    fn uninstall(&self) -> Result<LocalCaStatus, MmError> {
        Self::uninstall(self)
    }

    fn issue(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        Self::issue(self, hosts)
    }

    fn certificate_pem(&self, host: &str) -> Result<String, MmError> {
        Self::certificate_pem(self, host)
    }

    fn key_pem(&self, host: &str) -> Result<String, MmError> {
        Self::key_pem(self, host)
    }

    fn ca_certificate_pem(&self) -> Result<String, MmError> {
        Self::ca_certificate_pem(self)
    }

    fn set_trust_disabled(&self, disabled: bool) -> Result<LocalCaStatus, MmError> {
        Self::set_trust_disabled(self, disabled)
    }

    fn reconcile(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        Self::reconcile(self, hosts)
    }

    fn certificate_path(&self, host: &str) -> Result<PathBuf, MmError> {
        Self::certificate_path(self, host)
    }

    fn key_path(&self, host: &str) -> Result<PathBuf, MmError> {
        Self::key_path(self, host)
    }
}
