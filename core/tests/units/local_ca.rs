//! The local certificate authority's manager against a scripted store and
//! trust store: setup, repair, rotation, removal, and what each failure
//! leaves behind.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

use mm::adapters::ca::FilesystemLocalCaStore;
use mm::app::LocalCaManager;
use mm::app::ports::{LocalCaStore, TrustStore};
use mm::domain::ca::{IssuedCertificate, LocalCaStatus};
use mm::domain::ca::{LeafCertificateStatus, TrustScope, TrustStoreStatus};
use mm::domain::diagnostic::Diagnostic;
use mm::domain::error::MmError;

fn status(installed: bool) -> LocalCaStatus {
    LocalCaStatus {
        installed,
        fingerprint: installed.then(|| "old".to_owned()),
        generation: u64::from(installed),
        issued_hosts: BTreeSet::new(),
        key_permissions_valid: true,
        trust_disabled: false,
        rotation_pending: false,
        leaves: Vec::new(),
        trust: None,
    }
}

fn fake_error(operation: &str) -> MmError {
    MmError::State {
        message: format!("fake {operation} failure"),
    }
}

fn failure<T>(result: Result<T, MmError>) -> Result<MmError, MmError> {
    match result {
        Err(error) => Ok(error),
        Ok(_) => Err(fake_error("expected failure")),
    }
}

struct FakeCaStore {
    state: Mutex<LocalCaStatus>,
    previous: Mutex<Option<String>>,
    rotate_fingerprint: Mutex<Option<String>>,
    failures: BTreeSet<&'static str>,
}

impl FakeCaStore {
    fn new(state: LocalCaStatus) -> Self {
        Self {
            state: Mutex::new(state),
            previous: Mutex::new(None),
            rotate_fingerprint: Mutex::new(Some("new".to_owned())),
            failures: BTreeSet::new(),
        }
    }

    fn failing(state: LocalCaStatus, failures: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            failures: failures.into_iter().collect(),
            ..Self::new(state)
        }
    }

    fn state(&self) -> Result<MutexGuard<'_, LocalCaStatus>, MmError> {
        self.state.lock().map_err(|_| fake_error("state lock"))
    }

    fn check(&self, operation: &'static str) -> Result<(), MmError> {
        if self.failures.contains(operation) {
            Err(fake_error(operation))
        } else {
            Ok(())
        }
    }

    fn set_previous(&self, fingerprint: Option<&str>) -> Result<(), MmError> {
        *self
            .previous
            .lock()
            .map_err(|_| fake_error("previous lock"))? = fingerprint.map(str::to_owned);
        Ok(())
    }

    fn set_rotate_fingerprint(&self, fingerprint: Option<&str>) -> Result<(), MmError> {
        *self
            .rotate_fingerprint
            .lock()
            .map_err(|_| fake_error("rotate fingerprint lock"))? = fingerprint.map(str::to_owned);
        Ok(())
    }
}

impl LocalCaStore for FakeCaStore {
    fn status(&self) -> Result<LocalCaStatus, MmError> {
        self.check("status")?;
        Ok(self.state()?.clone())
    }

    fn install(&self) -> Result<LocalCaStatus, MmError> {
        self.check("install")?;
        let mut state = self.state()?;
        state.installed = true;
        state.fingerprint = Some("old".to_owned());
        state.generation = 1;
        Ok(state.clone())
    }

    fn rotate(&self) -> Result<LocalCaStatus, MmError> {
        self.check("rotate")?;
        let mut state = self.state()?;
        self.set_previous(state.fingerprint.as_deref())?;
        state.fingerprint = self
            .rotate_fingerprint
            .lock()
            .map_err(|_| fake_error("rotate fingerprint lock"))?
            .clone();
        state.generation += 1;
        state.rotation_pending = true;
        Ok(state.clone())
    }

    fn finish_rotation(&self) -> Result<LocalCaStatus, MmError> {
        self.check("finish_rotation")?;
        self.set_previous(None)?;
        let mut state = self.state()?;
        state.rotation_pending = false;
        Ok(state.clone())
    }

    fn rollback_rotation(&self) -> Result<LocalCaStatus, MmError> {
        self.check("rollback_rotation")?;
        let previous = self
            .previous
            .lock()
            .map_err(|_| fake_error("previous lock"))?
            .take();
        let mut state = self.state()?;
        state.fingerprint = previous;
        state.rotation_pending = false;
        Ok(state.clone())
    }

    fn rotation_previous_fingerprint(&self) -> Result<Option<String>, MmError> {
        self.check("rotation_previous_fingerprint")?;
        self.previous
            .lock()
            .map(|previous| previous.clone())
            .map_err(|_| fake_error("previous lock"))
    }

    fn uninstall(&self) -> Result<LocalCaStatus, MmError> {
        self.check("uninstall")?;
        let mut state = self.state()?;
        *state = status(false);
        Ok(state.clone())
    }

    fn issue(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        self.check("issue")?;
        self.state()?.issued_hosts.extend(hosts.iter().cloned());
        Ok(vec![IssuedCertificate {
            hosts: hosts.iter().cloned().collect(),
            fingerprint: "leaf".to_owned(),
            generation: 1,
            expires_at_unix: 42,
        }])
    }

    fn certificate_pem(&self, host: &str) -> Result<String, MmError> {
        self.check("certificate_pem")?;
        Ok(format!("certificate:{host}"))
    }

    fn key_pem(&self, host: &str) -> Result<String, MmError> {
        self.check("key_pem")?;
        Ok(format!("key:{host}"))
    }

    fn ca_certificate_pem(&self) -> Result<String, MmError> {
        self.check("ca_certificate_pem")?;
        Ok("ca-certificate".to_owned())
    }

    fn set_trust_disabled(&self, disabled: bool) -> Result<LocalCaStatus, MmError> {
        self.check("set_trust_disabled")?;
        let mut state = self.state()?;
        state.trust_disabled = disabled;
        Ok(state.clone())
    }

    fn reconcile(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        self.check("reconcile")?;
        self.issue(hosts)
    }

    fn certificate_path(&self, host: &str) -> Result<PathBuf, MmError> {
        self.check("certificate_path")?;
        Ok(PathBuf::from(format!("certificates/{host}.pem")))
    }

    fn key_path(&self, host: &str) -> Result<PathBuf, MmError> {
        self.check("key_path")?;
        Ok(PathBuf::from(format!("keys/{host}.pem")))
    }
}

struct FakeTrustStore {
    installed: Mutex<BTreeSet<String>>,
    failures: BTreeSet<&'static str>,
    supported: bool,
    mismatch: bool,
    private_keys_valid: bool,
}

impl Default for FakeTrustStore {
    fn default() -> Self {
        Self {
            installed: Mutex::new(BTreeSet::new()),
            failures: BTreeSet::new(),
            supported: true,
            mismatch: false,
            private_keys_valid: true,
        }
    }
}

impl FakeTrustStore {
    fn failing(operation: &'static str) -> Self {
        Self {
            failures: [operation].into_iter().collect(),
            ..Self::default()
        }
    }

    fn check(&self, operation: &'static str) -> Result<(), MmError> {
        if self.failures.contains(operation) {
            Err(fake_error(operation))
        } else {
            Ok(())
        }
    }

    fn fingerprints(&self) -> Result<BTreeSet<String>, MmError> {
        self.installed
            .lock()
            .map(|installed| installed.clone())
            .map_err(|_| fake_error("trust lock"))
    }

    fn seed(&self, fingerprint: &str) -> Result<(), MmError> {
        self.installed
            .lock()
            .map_err(|_| fake_error("trust lock"))?
            .insert(fingerprint.to_owned());
        Ok(())
    }

    fn status_value(&self, fingerprint: Option<&str>) -> Result<TrustStoreStatus, MmError> {
        let installed = fingerprint.is_some_and(|fingerprint| {
            self.installed
                .lock()
                .is_ok_and(|installed| installed.contains(fingerprint))
        });
        Ok(TrustStoreStatus {
            supported: self.supported,
            installed,
            platform: "test".to_owned(),
            scope: TrustScope::CurrentUser,
            fingerprint: installed.then(|| fingerprint.unwrap_or_default().to_owned()),
            mismatch: self.mismatch,
            needs_privilege: false,
            entry: installed.then_some("test-root".to_owned()),
        })
    }
}

#[async_trait]
impl TrustStore for FakeTrustStore {
    async fn secure_private_keys(&self) -> Result<(), MmError> {
        self.check("secure_private_keys")
    }

    async fn private_keys_valid(&self) -> Result<bool, MmError> {
        self.check("private_keys_valid")?;
        Ok(self.private_keys_valid)
    }

    async fn status(&self, fingerprint: Option<&str>) -> Result<TrustStoreStatus, MmError> {
        self.check("status")?;
        self.status_value(fingerprint)
    }

    async fn install(
        &self,
        _certificate_pem: &str,
        fingerprint: &str,
    ) -> Result<TrustStoreStatus, MmError> {
        self.check("install")?;
        self.seed(fingerprint)?;
        self.status_value(Some(fingerprint))
    }

    async fn repair(
        &self,
        certificate_pem: &str,
        fingerprint: &str,
    ) -> Result<TrustStoreStatus, MmError> {
        self.check("repair")?;
        self.install(certificate_pem, fingerprint).await
    }

    async fn uninstall(&self, fingerprint: &str) -> Result<TrustStoreStatus, MmError> {
        self.check("uninstall")?;
        self.installed
            .lock()
            .map_err(|_| fake_error("trust lock"))?
            .remove(fingerprint);
        self.status_value(Some(fingerprint))
    }
}

fn manager(store: Arc<FakeCaStore>, trust: Arc<FakeTrustStore>) -> LocalCaManager {
    LocalCaManager::new(store, trust)
}

fn diagnostic_codes(diagnostics: &[Diagnostic]) -> BTreeSet<&str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

#[tokio::test]
async fn setup_without_trust_and_store_delegations_are_exact() -> Result<(), MmError> {
    let store = Arc::new(FakeCaStore::new(status(false)));
    let manager = manager(store, Arc::new(FakeTrustStore::default()));
    let setup = manager.setup(false).await?;
    assert!(setup.installed);
    assert!(setup.trust_disabled);
    assert!(format!("{manager:?}").starts_with("LocalCaManager"));

    let hosts = ["app.test".to_owned()].into_iter().collect();
    assert_eq!(manager.issue(&hosts)?[0].hosts, vec!["app.test"]);
    assert_eq!(manager.reconcile(&hosts)?[0].fingerprint, "leaf");
    assert_eq!(manager.certificate_pem("app.test")?, "certificate:app.test");
    assert_eq!(manager.key_pem("app.test")?, "key:app.test");
    assert_eq!(manager.ca_certificate_pem()?, "ca-certificate");
    assert_eq!(
        manager.certificate_path("app.test")?,
        PathBuf::from("certificates/app.test.pem")
    );
    assert_eq!(
        manager.key_path("app.test")?,
        PathBuf::from("keys/app.test.pem")
    );
    Ok(())
}

#[tokio::test]
async fn setup_rolls_back_each_new_install_failure() -> Result<(), MmError> {
    for operation in ["issue", "set_trust_disabled"] {
        let store = Arc::new(FakeCaStore::failing(status(false), [operation]));
        let error = failure(
            manager(store.clone(), Arc::new(FakeTrustStore::default()))
                .setup(false)
                .await,
        )?;
        assert_eq!(error.code(), "MM_STATE");
        assert!(!store.status()?.installed);
    }
    let store = Arc::new(FakeCaStore::new(status(false)));
    assert!(
        manager(
            store.clone(),
            Arc::new(FakeTrustStore::failing("secure_private_keys")),
        )
        .setup(false)
        .await
        .is_err()
    );
    assert!(!store.status()?.installed);
    Ok(())
}

#[tokio::test]
async fn setup_reports_cleanup_failures_without_hiding_original_context() -> Result<(), MmError> {
    let store = Arc::new(FakeCaStore::new(status(false)));
    let error = failure(
        manager(store.clone(), Arc::new(FakeTrustStore::failing("install")))
            .setup(true)
            .await,
    )?;
    assert_eq!(error.code(), "MM_STATE");
    assert!(!store.status()?.installed);

    let store = Arc::new(FakeCaStore::failing(status(false), ["issue", "uninstall"]));
    let error = failure(
        manager(store, Arc::new(FakeTrustStore::default()))
            .setup(false)
            .await,
    )?;
    assert!(matches!(error, MmError::Security { .. }));
    assert!(error.to_string().contains("rollback failed"));

    let store = Arc::new(FakeCaStore::failing(status(false), ["uninstall"]));
    let error = failure(
        manager(store, Arc::new(FakeTrustStore::failing("install")))
            .setup(true)
            .await,
    )?;
    assert!(matches!(error, MmError::Security { .. }));
    assert!(error.to_string().contains("generated CA rollback failed"));
    Ok(())
}

#[tokio::test]
async fn setup_preserves_preexisting_ca_when_refresh_fails() -> Result<(), MmError> {
    let store = Arc::new(FakeCaStore::failing(status(true), ["issue"]));
    assert!(
        manager(store.clone(), Arc::new(FakeTrustStore::default()))
            .setup(false)
            .await
            .is_err()
    );
    assert!(store.status()?.installed);

    let store = Arc::new(FakeCaStore::new(status(true)));
    assert!(
        manager(store.clone(), Arc::new(FakeTrustStore::failing("install")))
            .setup(true)
            .await
            .is_err()
    );
    assert!(store.status()?.installed);
    Ok(())
}

#[tokio::test]
async fn repair_enforces_install_and_trust_state() -> Result<(), MmError> {
    let missing = manager(
        Arc::new(FakeCaStore::new(status(false))),
        Arc::new(FakeTrustStore::default()),
    );
    assert!(matches!(
        missing.repair().await,
        Err(MmError::Target { .. })
    ));

    let mut disabled = status(true);
    disabled.trust_disabled = true;
    let disabled = manager(
        Arc::new(FakeCaStore::new(disabled)),
        Arc::new(FakeTrustStore::default()),
    );
    assert!(disabled.repair().await?.trust_disabled);

    let trust = Arc::new(FakeTrustStore::default());
    let repaired = manager(Arc::new(FakeCaStore::new(status(true))), trust.clone())
        .repair()
        .await?;
    assert!(repaired.trust.is_some_and(|trust| trust.installed));
    assert_eq!(
        trust.fingerprints()?,
        ["old".to_owned()].into_iter().collect()
    );

    let mut pending = status(true);
    pending.rotation_pending = true;
    let pending_store = Arc::new(FakeCaStore::new(pending));
    pending_store.set_previous(Some("older"))?;
    assert!(
        manager(
            pending_store,
            Arc::new(FakeTrustStore::failing("uninstall")),
        )
        .repair()
        .await
        .is_err()
    );

    let mut pending = status(true);
    pending.rotation_pending = true;
    let pending_store = Arc::new(FakeCaStore::new(pending));
    pending_store.set_previous(Some("older"))?;
    let trust = Arc::new(FakeTrustStore::default());
    trust.seed("old")?;
    trust.seed("older")?;
    let completed = manager(pending_store, trust.clone()).repair().await?;
    assert!(!completed.rotation_pending);
    assert_eq!(
        trust.fingerprints()?,
        ["old".to_owned()].into_iter().collect()
    );

    let mut no_fingerprint = status(true);
    no_fingerprint.fingerprint = None;
    assert!(
        manager(
            Arc::new(FakeCaStore::new(no_fingerprint)),
            Arc::new(FakeTrustStore::default()),
        )
        .repair()
        .await?
        .trust
        .is_some()
    );
    Ok(())
}

#[tokio::test]
async fn rotate_rejects_unsafe_states_and_supports_disabled_trust() -> Result<(), MmError> {
    let missing = manager(
        Arc::new(FakeCaStore::new(status(false))),
        Arc::new(FakeTrustStore::default()),
    );
    assert!(matches!(
        missing.rotate().await,
        Err(MmError::Target { .. })
    ));

    let untrusted = manager(
        Arc::new(FakeCaStore::new(status(true))),
        Arc::new(FakeTrustStore::default()),
    );
    assert!(matches!(
        untrusted.rotate().await,
        Err(MmError::Security { .. })
    ));

    let mut disabled = status(true);
    disabled.trust_disabled = true;
    let rotated = manager(
        Arc::new(FakeCaStore::new(disabled)),
        Arc::new(FakeTrustStore::default()),
    )
    .rotate()
    .await?;
    assert!(!rotated.rotation_pending);
    assert_eq!(rotated.fingerprint.as_deref(), Some("new"));
    Ok(())
}

#[tokio::test]
async fn rotate_requires_new_fingerprint_and_rolls_back_trust_failure() -> Result<(), MmError> {
    let no_fingerprint = Arc::new(FakeCaStore::new(status(true)));
    no_fingerprint.set_rotate_fingerprint(None)?;
    let trust = Arc::new(FakeTrustStore::default());
    trust.seed("old")?;
    let error = failure(manager(no_fingerprint, trust).rotate().await)?;
    assert!(matches!(error, MmError::State { .. }));

    for rollback_failure in [false, true] {
        let failures = rollback_failure.then_some("rollback_rotation");
        let store = Arc::new(FakeCaStore::failing(status(true), failures));
        let trust = Arc::new(FakeTrustStore::failing("install"));
        trust.seed("old")?;
        let error = failure(manager(store.clone(), trust).rotate().await)?;
        if rollback_failure {
            assert!(matches!(error, MmError::Security { .. }));
        } else {
            assert_eq!(store.status()?.fingerprint.as_deref(), Some("old"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn uninstall_removes_current_and_previous_trust_roots() -> Result<(), MmError> {
    let store = Arc::new(FakeCaStore::new(status(true)));
    store.set_previous(Some("older"))?;
    let trust = Arc::new(FakeTrustStore::default());
    trust.seed("old")?;
    trust.seed("older")?;
    let removed = manager(store, trust.clone()).uninstall().await?;
    assert!(!removed.installed);
    assert!(trust.fingerprints()?.is_empty());

    assert!(
        manager(
            Arc::new(FakeCaStore::new(status(true))),
            Arc::new(FakeTrustStore::failing("uninstall")),
        )
        .uninstall()
        .await
        .is_err()
    );

    let store = Arc::new(FakeCaStore::new(status(false)));
    store.set_previous(Some("older"))?;
    assert!(
        manager(store, Arc::new(FakeTrustStore::failing("uninstall")))
            .uninstall()
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn diagnostics_report_each_actionable_ca_state() -> Result<(), MmError> {
    let trusted = Arc::new(FakeTrustStore::default());
    trusted.seed("old")?;
    assert!(
        manager(Arc::new(FakeCaStore::new(status(true))), trusted)
            .diagnostics()
            .await?
            .is_empty()
    );

    let missing = manager(
        Arc::new(FakeCaStore::new(status(false))),
        Arc::new(FakeTrustStore::default()),
    );
    assert_eq!(
        diagnostic_codes(&missing.diagnostics().await?),
        ["MM_TLS_CA_MISSING"].into_iter().collect()
    );

    let mut degraded = status(true);
    degraded.key_permissions_valid = false;
    degraded.trust_disabled = true;
    degraded.rotation_pending = true;
    degraded.leaves.push(LeafCertificateStatus {
        hosts: vec!["app.test".to_owned()],
        fingerprint: "leaf".to_owned(),
        generation: 1,
        expires_at_unix: 42,
        stale: true,
    });
    let degraded_diagnostics = manager(
        Arc::new(FakeCaStore::new(degraded)),
        Arc::new(FakeTrustStore::default()),
    )
    .diagnostics()
    .await?;
    let codes = diagnostic_codes(&degraded_diagnostics);
    assert_eq!(
        codes,
        [
            "MM_TLS_KEY_PERMISSIONS",
            "MM_TLS_LEAF_STALE",
            "MM_TLS_ROTATION_PENDING",
            "MM_TLS_TRUST_DISABLED"
        ]
        .into_iter()
        .collect()
    );

    for (trust, expected) in [
        (
            FakeTrustStore {
                supported: false,
                ..FakeTrustStore::default()
            },
            "MM_TLS_TRUST_UNSUPPORTED",
        ),
        (
            FakeTrustStore {
                mismatch: true,
                ..FakeTrustStore::default()
            },
            "MM_TLS_TRUST_MISMATCH",
        ),
        (FakeTrustStore::default(), "MM_TLS_TRUST_MISSING"),
    ] {
        let diagnostics = manager(Arc::new(FakeCaStore::new(status(true))), Arc::new(trust))
            .diagnostics()
            .await?;
        assert_eq!(
            diagnostic_codes(&diagnostics),
            [expected].into_iter().collect()
        );
    }
    Ok(())
}

#[tokio::test]
async fn rotation_keeps_trust_overlap_until_explicit_repair() -> Result<(), MmError> {
    let directory = crate::temp_dirs::tempdir().map_err(|source| MmError::Io {
        context: "creating CA manager test directory".to_owned(),
        source,
    })?;
    let trust = Arc::new(FakeTrustStore::default());
    let manager = LocalCaManager::new(
        Arc::new(FilesystemLocalCaStore::new(directory.path().join("ca"))),
        trust.clone(),
    );
    let original = manager.setup(true).await?;
    let original_fingerprint = original
        .fingerprint
        .ok_or_else(|| fake_error("original fingerprint"))?;

    let rotated = manager.rotate().await?;
    let rotated_fingerprint = rotated
        .fingerprint
        .clone()
        .ok_or_else(|| fake_error("rotated fingerprint"))?;
    assert!(rotated.rotation_pending);
    assert_eq!(
        trust.fingerprints()?,
        [original_fingerprint, rotated_fingerprint.clone()]
            .into_iter()
            .collect()
    );

    let repaired = manager.repair().await?;
    assert!(!repaired.rotation_pending);
    assert_eq!(
        trust.fingerprints()?,
        [rotated_fingerprint].into_iter().collect()
    );
    Ok(())
}
