use std::collections::BTreeSet;

use crate::domain::error::MmError;
use async_trait::async_trait;

pub trait LocalCaStore: Send + Sync {
    fn status(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError>;
    fn install(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError>;
    fn rotate(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError>;
    fn finish_rotation(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError>;
    fn rollback_rotation(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError>;
    fn rotation_previous_fingerprint(&self) -> Result<Option<String>, MmError>;
    fn uninstall(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError>;
    fn issue(
        &self,
        hosts: &BTreeSet<String>,
    ) -> Result<Vec<crate::domain::ca::IssuedCertificate>, MmError>;
    fn certificate_pem(&self, host: &str) -> Result<String, MmError>;
    fn key_pem(&self, host: &str) -> Result<String, MmError>;
    fn ca_certificate_pem(&self) -> Result<String, MmError>;
    fn set_trust_disabled(
        &self,
        disabled: bool,
    ) -> Result<crate::domain::ca::LocalCaStatus, MmError>;
    fn reconcile(
        &self,
        hosts: &BTreeSet<String>,
    ) -> Result<Vec<crate::domain::ca::IssuedCertificate>, MmError>;
    fn certificate_path(&self, host: &str) -> Result<std::path::PathBuf, MmError>;
    fn key_path(&self, host: &str) -> Result<std::path::PathBuf, MmError>;
}

#[async_trait]
pub trait TrustStore: Send + Sync {
    async fn secure_private_keys(&self) -> Result<(), MmError>;
    async fn private_keys_valid(&self) -> Result<bool, MmError>;
    async fn status(
        &self,
        fingerprint: Option<&str>,
    ) -> Result<crate::domain::ca::TrustStoreStatus, MmError>;
    async fn install(
        &self,
        certificate_pem: &str,
        fingerprint: &str,
    ) -> Result<crate::domain::ca::TrustStoreStatus, MmError>;
    async fn repair(
        &self,
        certificate_pem: &str,
        fingerprint: &str,
    ) -> Result<crate::domain::ca::TrustStoreStatus, MmError>;
    async fn uninstall(
        &self,
        fingerprint: &str,
    ) -> Result<crate::domain::ca::TrustStoreStatus, MmError>;
}
