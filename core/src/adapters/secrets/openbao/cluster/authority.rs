//! The authority and recovery copy of each managed store, on the operator's machine.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DistinguishedName,
    DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::adapters::node_files::{read_whole, write_whole};
use crate::adapters::secrets::AgeSecretStore;
use crate::app::ports::{ClusterStoreKeeper, StoreScope};
use crate::domain::error::MmError;
use crate::domain::secret_store::StoreRecovery;
use crate::domain::stored_state::{StoredOn, newer_mm_message, stored_version};
use crate::domain::workspace::ProjectId;

const DOCUMENT_VERSION: u32 = 1;
const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_SNAPSHOT_FILE_BYTES: usize = 16 * 1024 * 1024;
const AUTHORITY_DAYS: i64 = 3650;
const MEMBER_DAYS: i64 = 825;

#[derive(Default, Serialize, Deserialize)]
struct KeptStores {
    version: u32,
    stores: BTreeMap<String, KeptStore>,
}

#[derive(Serialize, Deserialize)]
struct ExportedStore {
    version: u32,
    authority_pem: String,
    authority_key_pem: Zeroizing<String>,
    recovery: StoreRecovery,
}

#[derive(Serialize, Deserialize)]
struct KeptStore {
    authority_pem: String,
    authority_key_pem: Zeroizing<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery: Option<String>,
}

/// The keeper of one machine's mm home: one sealed file per project beside
/// the project's age store, readable only with the operator's identity.
#[derive(Debug, Clone)]
pub struct FileStoreKeeper {
    identity_path: PathBuf,
    secrets_directory: PathBuf,
}

fn authority_error(error: impl std::fmt::Display) -> MmError {
    MmError::Security {
        message: format!("the secret store's certificate authority: {error}"),
    }
}

fn key_of(scope: StoreScope<'_>) -> String {
    format!("{}/{}", scope.environment, scope.provider)
}

impl FileStoreKeeper {
    #[must_use]
    pub fn new(identity_path: PathBuf, secrets_directory: PathBuf) -> Self {
        Self {
            identity_path,
            secrets_directory,
        }
    }

    fn age_store(&self, project: ProjectId) -> AgeSecretStore {
        AgeSecretStore::new(
            self.identity_path.clone(),
            self.secrets_directory.join(format!("{}.age", project.0)),
        )
    }

    fn path(&self, project: ProjectId) -> PathBuf {
        self.secrets_directory
            .join(format!("{}.stores.age", project.0))
    }

    fn read(&self, project: ProjectId) -> Result<KeptStores, MmError> {
        let Some(sealed) = read_whole(&self.path(project))? else {
            return Ok(KeptStores {
                version: DOCUMENT_VERSION,
                stores: BTreeMap::new(),
            });
        };
        let opened = self
            .age_store(project)
            .open_sealed(&sealed, MAX_DOCUMENT_BYTES)?;
        if let Some(version) = stored_version(&opened, "version")
            && version > DOCUMENT_VERSION
        {
            return Err(MmError::Secret {
                message: newer_mm_message(
                    &self.path(project),
                    &format!("version {version}"),
                    StoredOn::ThisMachine,
                ),
            });
        }
        let kept: KeptStores = serde_json::from_slice(&opened).map_err(|_| MmError::Secret {
            message: format!(
                "{} is not a document of managed secret stores",
                self.path(project).display()
            ),
        })?;
        if kept.version != DOCUMENT_VERSION {
            return Err(MmError::Secret {
                message: format!(
                    "{} has version {}, which this mm does not read",
                    self.path(project).display(),
                    kept.version
                ),
            });
        }
        Ok(kept)
    }

    fn write(&self, project: ProjectId, kept: &KeptStores) -> Result<(), MmError> {
        let recipient = self.operator_recipient(project)?;
        let plaintext =
            Zeroizing::new(serde_json::to_vec(kept).map_err(|error| MmError::State {
                message: format!("serializing the managed secret stores: {error}"),
            })?);
        let sealed = crate::adapters::secrets::seal_to_recipient(&recipient, &plaintext)?;
        write_whole(&self.path(project), sealed.as_bytes(), 0o600)
    }

    /// Runs `change` on the project's document under its lock.
    fn update<T>(
        &self,
        project: ProjectId,
        change: impl FnOnce(&mut KeptStores) -> Result<T, MmError>,
    ) -> Result<T, MmError> {
        self.operator_recipient(project)?;
        let lock = self.path(project).with_extension("lock");
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(false).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let held = options.open(&lock).map_err(|source| MmError::Io {
            context: format!("opening {}", lock.display()),
            source,
        })?;
        crate::adapters::file_lock::exclusive(&held, &lock)?;
        let mut kept = self.read(project)?;
        let answer = change(&mut kept)?;
        self.write(project, &kept)?;
        Ok(answer)
    }

    fn new_authority(scope: StoreScope<'_>) -> Result<KeptStore, MmError> {
        let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(authority_error)?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(authority_error)?;
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.not_before = time::OffsetDateTime::now_utc() - time::Duration::minutes(5);
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(AUTHORITY_DAYS);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(
            DnType::CommonName,
            format!("mm secret store {} {}", scope.environment, scope.project.0),
        );
        let certificate = params.self_signed(&key).map_err(authority_error)?;
        Ok(KeptStore {
            authority_pem: certificate.pem(),
            authority_key_pem: Zeroizing::new(key.serialize_pem()),
            recovery: None,
        })
    }

    /// The recovery copy opened with the operator's identity.
    pub fn open_recovery(&self, scope: StoreScope<'_>) -> Result<StoreRecovery, MmError> {
        let sealed = self.recovery(scope)?.ok_or_else(|| no_recovery(scope))?;
        let opened = self
            .age_store(scope.project)
            .open_sealed(sealed.as_bytes(), MAX_DOCUMENT_BYTES)?;
        serde_json::from_slice(&opened).map_err(|_| MmError::Secret {
            message: "the recovery copy is not a recovery document".to_owned(),
        })
    }

    /// The environment's authority and recovery document, sealed to
    /// `recipient`, or to this machine's operator when none is given.
    pub fn export(
        &self,
        scope: StoreScope<'_>,
        recipient: Option<&str>,
    ) -> Result<String, MmError> {
        let kept = self.read(scope.project)?;
        let store = kept
            .stores
            .get(&key_of(scope))
            .ok_or_else(|| no_recovery(scope))?;
        let exported = ExportedStore {
            version: DOCUMENT_VERSION,
            authority_pem: store.authority_pem.clone(),
            authority_key_pem: store.authority_key_pem.clone(),
            recovery: self.open_recovery(scope)?,
        };
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(&exported).map_err(|error| MmError::State {
                    message: format!("serializing the store's recovery copy: {error}"),
                })?,
            );
        let recipient = match recipient {
            Some(recipient) => recipient.to_owned(),
            None => self.operator_recipient(scope.project)?,
        };
        crate::adapters::secrets::seal_to_recipient(&recipient, &plaintext)
    }

    /// Keeps what [`Self::export`] sealed to this machine's operator, once
    /// it is the copy of this environment's store.
    pub fn import(&self, scope: StoreScope<'_>, sealed: &str) -> Result<(), MmError> {
        let opened = self
            .age_store(scope.project)
            .open_sealed(sealed.as_bytes(), MAX_DOCUMENT_BYTES)?;
        if let Some(version) = stored_version(&opened, "version")
            && version > DOCUMENT_VERSION
        {
            return Err(MmError::Secret {
                message: format!(
                    "that recovery copy was written by a newer mm (version {version}); {}",
                    StoredOn::ThisMachine.next_step()
                ),
            });
        }
        let exported: ExportedStore =
            serde_json::from_slice(&opened).map_err(|_| MmError::Secret {
                message: "that file is not a recovery copy of a managed secret store".to_owned(),
            })?;
        let recovery = &exported.recovery;
        if recovery.project_id != scope.project
            || recovery.environment != scope.environment
            || recovery.provider != scope.provider
        {
            return Err(MmError::Secret {
                message: format!(
                    "that recovery copy is of the store '{}' of environment '{}', not of '{}' of '{}'",
                    recovery.provider, recovery.environment, scope.provider, scope.environment
                ),
            });
        }
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(recovery).map_err(|error| MmError::State {
                    message: format!("serializing the store's recovery document: {error}"),
                })?,
            );
        let resealed = crate::adapters::secrets::seal_to_recipient(
            &self.operator_recipient(scope.project)?,
            &plaintext,
        )?;
        self.update(scope.project, |kept| {
            kept.stores.insert(
                key_of(scope),
                KeptStore {
                    authority_pem: exported.authority_pem.clone(),
                    authority_key_pem: exported.authority_key_pem.clone(),
                    recovery: Some(resealed),
                },
            );
            Ok(())
        })
    }
}

fn no_recovery(scope: StoreScope<'_>) -> MmError {
    MmError::Secret {
        message: format!(
            "this machine holds no recovery copy of the secret store '{}' of environment '{}'; on the machine that ran its first `mm target setup apply`, `mm -e {} secret provider recovery export {} --recipient AGE_RECIPIENT --to FILE` seals it to you, and `mm -e {} secret provider recovery import {} FILE` keeps it here",
            scope.provider,
            scope.environment,
            scope.environment,
            scope.provider,
            scope.environment,
            scope.provider
        ),
    }
}

impl ClusterStoreKeeper for FileStoreKeeper {
    fn operator_recipient(&self, project: ProjectId) -> Result<String, MmError> {
        self.age_store(project).ensure_recipient()
    }

    fn authority(&self, scope: StoreScope<'_>) -> Result<String, MmError> {
        self.update(scope.project, |kept| {
            if let Some(store) = kept.stores.get(&key_of(scope)) {
                return Ok(store.authority_pem.clone());
            }
            let store = Self::new_authority(scope)?;
            let pem = store.authority_pem.clone();
            kept.stores.insert(key_of(scope), store);
            Ok(pem)
        })
    }

    fn sign(
        &self,
        scope: StoreScope<'_>,
        request_pem: &str,
        (node, address): (&str, Ipv4Addr),
    ) -> Result<String, MmError> {
        let kept = self.read(scope.project)?;
        let store = kept
            .stores
            .get(&key_of(scope))
            .ok_or_else(|| MmError::State {
                message: format!(
                    "no certificate authority is kept here for the secret store of '{}'",
                    scope.environment
                ),
            })?;
        let key = KeyPair::from_pem(&store.authority_key_pem).map_err(authority_error)?;
        let issuer =
            rcgen::Issuer::from_ca_cert_pem(&store.authority_pem, key).map_err(authority_error)?;
        let mut request =
            CertificateSigningRequestParams::from_pem(request_pem).map_err(authority_error)?;
        let mut params = CertificateParams::new(vec![
            node.to_owned(),
            "localhost".to_owned(),
            address.to_string(),
            Ipv4Addr::LOCALHOST.to_string(),
        ])
        .map_err(authority_error)?;
        params.not_before = time::OffsetDateTime::now_utc() - time::Duration::minutes(5);
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(MEMBER_DAYS);
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(DnType::CommonName, node);
        request.params = params;
        request
            .signed_by(&issuer)
            .map(|certificate| certificate.pem())
            .map_err(authority_error)
    }

    fn keep_recovery(&self, scope: StoreScope<'_>, sealed: &str) -> Result<(), MmError> {
        self.update(scope.project, |kept| {
            let store = match kept.stores.entry(key_of(scope)) {
                std::collections::btree_map::Entry::Occupied(store) => store.into_mut(),
                std::collections::btree_map::Entry::Vacant(store) => {
                    store.insert(Self::new_authority(scope)?)
                }
            };
            store.recovery = Some(sealed.to_owned());
            Ok(())
        })
    }

    fn recovery(&self, scope: StoreScope<'_>) -> Result<Option<String>, MmError> {
        Ok(self
            .read(scope.project)?
            .stores
            .get(&key_of(scope))
            .and_then(|store| store.recovery.clone()))
    }

    fn seal(&self, recipient: &str, plaintext: &[u8]) -> Result<String, MmError> {
        crate::adapters::secrets::seal_to_recipient(recipient, plaintext)
    }

    fn open(&self, project: ProjectId, sealed: &str) -> Result<Zeroizing<Vec<u8>>, MmError> {
        self.age_store(project)
            .open_sealed(sealed.as_bytes(), MAX_SNAPSHOT_FILE_BYTES)
    }

    fn reseal_recovery(
        &self,
        scope: StoreScope<'_>,
        recipient: &str,
    ) -> Result<Zeroizing<String>, MmError> {
        let recovery = self.open_recovery(scope)?;
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(&recovery).map_err(|error| MmError::State {
                    message: format!("serializing the store's recovery document: {error}"),
                })?,
            );
        crate::adapters::secrets::seal_to_recipient(recipient, &plaintext).map(Zeroizing::new)
    }
}
