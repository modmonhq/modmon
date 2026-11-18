use std::collections::BTreeSet;
use std::sync::Arc;

use crate::app::ports::{LocalCaStore, TrustStore};
use crate::domain::ca::{IssuedCertificate, LocalCaStatus};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;

#[derive(Clone)]
pub struct LocalCaManager {
    store: Arc<dyn LocalCaStore>,
    trust: Arc<dyn TrustStore>,
}

impl std::fmt::Debug for LocalCaManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalCaManager")
            .finish_non_exhaustive()
    }
}

impl LocalCaManager {
    #[must_use]
    pub fn new(store: Arc<dyn LocalCaStore>, trust: Arc<dyn TrustStore>) -> Self {
        Self { store, trust }
    }

    pub async fn status(&self) -> Result<LocalCaStatus, MmError> {
        self.with_trust(self.store.status()?).await
    }

    pub async fn setup(&self, trust: bool) -> Result<LocalCaStatus, MmError> {
        let before = self.store.status()?;
        self.store.install()?;
        if let Err(error) = self
            .store
            .issue(&["mm.test".to_owned()].into_iter().collect())
        {
            return Err(self.rollback_new_install(&before, error));
        }
        if let Err(error) = self.trust.secure_private_keys().await {
            return Err(self.rollback_new_install(&before, error));
        }
        if let Err(error) = self.store.set_trust_disabled(!trust) {
            return Err(self.rollback_new_install(&before, error));
        }
        let status = self.store.status()?;
        if !trust {
            return self.with_trust(status).await;
        }
        match self.sync_trust(&status, false).await {
            Ok(status) => {
                self.store.set_trust_disabled(false)?;
                Ok(status)
            }
            Err(error) if !before.installed => {
                let cleanup = self.store.uninstall();
                match cleanup {
                    Ok(_) => Err(error),
                    Err(cleanup) => Err(MmError::Security {
                        message: format!(
                            "trust setup failed with {} and generated CA rollback failed with {}",
                            error.code(),
                            cleanup.code()
                        ),
                    }),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub async fn repair(&self) -> Result<LocalCaStatus, MmError> {
        let status = self.store.status()?;
        if !status.installed {
            return Err(MmError::Target {
                message: "local CA is missing; run `mm dev setup`".to_owned(),
            });
        }
        if status.trust_disabled {
            return self.with_trust(status).await;
        }
        self.trust.secure_private_keys().await?;
        let repaired = self.sync_trust(&status, true).await?;
        if status.rotation_pending {
            if let Some(previous) = self.store.rotation_previous_fingerprint()? {
                self.trust.uninstall(&previous).await?;
            }
            return self.with_trust(self.store.finish_rotation()?).await;
        }
        Ok(repaired)
    }

    pub async fn rotate(&self) -> Result<LocalCaStatus, MmError> {
        let previous = self.status().await?;
        if !previous.installed {
            return Err(MmError::Target {
                message: "local CA is not installed; run `mm dev setup`".to_owned(),
            });
        }
        if !previous.trust_disabled && !previous.trust.as_ref().is_some_and(|trust| trust.installed)
        {
            return Err(MmError::Security {
                message: "the current root is not owned and installed in the selected trust scope; repair that scope before rotation"
                    .to_owned(),
            });
        }
        let rotated = self.store.rotate()?;
        if previous.trust_disabled {
            return self.with_trust(self.store.finish_rotation()?).await;
        }
        let pem = self.store.ca_certificate_pem()?;
        let fingerprint = rotated
            .fingerprint
            .as_deref()
            .ok_or_else(|| MmError::State {
                message: "rotated local CA has no fingerprint".to_owned(),
            })?;
        if let Err(error) = self.trust.install(&pem, fingerprint).await {
            return match self.store.rollback_rotation() {
                Ok(_) => Err(error),
                Err(rollback) => Err(MmError::Security {
                    message: format!(
                        "CA rotation trust staging failed with {} and rollback failed with {}",
                        error.code(),
                        rollback.code()
                    ),
                }),
            };
        }
        // Deployed ingress volumes can still contain old-generation leaves. Keep
        // both roots trusted until the user reapplies routes and completes the
        // overlap explicitly with `mm dev tls repair`.
        self.with_trust(rotated).await
    }

    pub async fn uninstall(&self) -> Result<LocalCaStatus, MmError> {
        let previous = self.store.status()?;
        if let Some(fingerprint) = previous.fingerprint.as_deref() {
            self.trust.uninstall(fingerprint).await?;
        }
        if let Some(previous) = self.store.rotation_previous_fingerprint()? {
            self.trust.uninstall(&previous).await?;
        }
        self.with_trust(self.store.uninstall()?).await
    }

    pub fn issue(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        self.store.issue(hosts)
    }

    pub fn certificate_pem(&self, host: &str) -> Result<String, MmError> {
        self.store.certificate_pem(host)
    }

    pub fn key_pem(&self, host: &str) -> Result<String, MmError> {
        self.store.key_pem(host)
    }

    pub fn ca_certificate_pem(&self) -> Result<String, MmError> {
        self.store.ca_certificate_pem()
    }

    pub fn reconcile(&self, hosts: &BTreeSet<String>) -> Result<Vec<IssuedCertificate>, MmError> {
        self.store.reconcile(hosts)
    }

    pub fn certificate_path(&self, host: &str) -> Result<std::path::PathBuf, MmError> {
        self.store.certificate_path(host)
    }

    pub fn key_path(&self, host: &str) -> Result<std::path::PathBuf, MmError> {
        self.store.key_path(host)
    }

    pub async fn diagnostics(&self) -> Result<Vec<Diagnostic>, MmError> {
        let status = self.status().await?;
        let mut diagnostics = Vec::new();
        if !status.installed {
            diagnostics.push(
                Diagnostic::error("MM_TLS_CA_MISSING", "the mm local CA is missing")
                    .with_help("run `mm dev setup`"),
            );
            return Ok(diagnostics);
        }
        if !status.key_permissions_valid {
            diagnostics.push(
                Diagnostic::error(
                    "MM_TLS_KEY_PERMISSIONS",
                    "the local CA private key is not a current-user-owned private regular file",
                )
                .with_help("run `mm dev tls repair`; do not copy or chmod the CA key broadly"),
            );
        }
        if status.trust_disabled {
            diagnostics.push(
                Diagnostic::warning(
                    "MM_TLS_TRUST_DISABLED",
                    "OS trust was explicitly disabled during setup; HTTPS is degraded",
                )
                .with_help("run `mm dev setup` without --no-trust"),
            );
        } else if let Some(trust) = &status.trust {
            if !trust.supported {
                diagnostics.push(
                    Diagnostic::error(
                        "MM_TLS_TRUST_UNSUPPORTED",
                        "this platform has no supported native trust-store adapter",
                    )
                    .with_help(
                        "use a supported platform or run `mm dev setup --no-trust` explicitly",
                    ),
                );
            } else if trust.mismatch {
                diagnostics.push(
                    Diagnostic::error(
                        "MM_TLS_TRUST_MISMATCH",
                        "the platform trust entry does not match mm's CA fingerprint",
                    )
                    .with_help(
                        "run `mm dev tls repair`; mm will not replace or delete a foreign certificate",
                    ),
                );
            } else if !trust.installed {
                diagnostics.push(
                    Diagnostic::error(
                        "MM_TLS_TRUST_MISSING",
                        "the exact mm CA DER certificate is absent from the platform trust store",
                    )
                    .with_help("run `mm dev tls repair` with the documented privilege"),
                );
            }
        }
        if status.leaves.iter().any(|leaf| leaf.stale) {
            diagnostics.push(Diagnostic::error(
                "MM_TLS_LEAF_STALE",
                "one or more local route certificates are expired, near expiry, or from another root generation",
            ).with_help("run `mm dev tls repair`; deployment also renews referenced leaves"));
        }
        if status.rotation_pending {
            diagnostics.push(
                Diagnostic::warning(
                    "MM_TLS_ROTATION_PENDING",
                    "CA rotation has safe trust overlap but old-root cleanup is incomplete",
                )
                .with_help("run `mm dev tls repair`"),
            );
        }
        Ok(diagnostics)
    }

    async fn sync_trust(
        &self,
        status: &LocalCaStatus,
        repair: bool,
    ) -> Result<LocalCaStatus, MmError> {
        let Some(fingerprint) = status.fingerprint.as_deref() else {
            return self.with_trust(status.clone()).await;
        };
        let pem = self.store.ca_certificate_pem()?;
        let trust = if repair {
            self.trust.repair(&pem, fingerprint).await?
        } else {
            self.trust.install(&pem, fingerprint).await?
        };
        let mut status = status.clone();
        status.trust = Some(trust);
        Ok(status)
    }

    async fn with_trust(&self, mut status: LocalCaStatus) -> Result<LocalCaStatus, MmError> {
        status.key_permissions_valid &= self.trust.private_keys_valid().await?;
        status.trust = Some(self.trust.status(status.fingerprint.as_deref()).await?);
        Ok(status)
    }

    fn rollback_new_install(&self, before: &LocalCaStatus, error: MmError) -> MmError {
        if before.installed {
            return error;
        }
        match self.store.uninstall() {
            Ok(_) => error,
            Err(cleanup) => MmError::Security {
                message: format!(
                    "local CA setup failed with {} and generated CA rollback failed with {}",
                    error.code(),
                    cleanup.code()
                ),
            },
        }
    }
}
