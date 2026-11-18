use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::error::MmError;
use crate::domain::gate::{
    GateAuditRecord, GateDriverId, GateDriverResult, GatePolicy, GateRouteAttachment,
    GateSessionClaims, GateSessionMetadata, GateUserMetadata, GateUserRecord, PasswordHashRecord,
};
use crate::domain::ingress::{CanonicalMiddleware, IngressRouteClaim};
use crate::domain::secret::SecretMaterial;
use crate::domain::target::TargetId;
use crate::domain::workspace::ProjectId;

pub trait GateRepository: Send + Sync {
    fn policy(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
    ) -> Result<Option<GatePolicy>, MmError>;
    fn policy_by_id(&self, id: Uuid) -> Result<Option<GatePolicy>, MmError>;
    fn list_policies(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Vec<GatePolicy>, MmError>;
    fn insert_policy(&self, policy: &GatePolicy) -> Result<(), MmError>;
    fn replace_policy(&self, policy: &GatePolicy, expected_revision: u64) -> Result<(), MmError>;
    fn remove_policy(&self, id: Uuid, expected_revision: u64) -> Result<bool, MmError>;

    fn user(
        &self,
        policy_id: Uuid,
        normalized_username: &str,
    ) -> Result<Option<GateUserRecord>, MmError>;
    fn user_by_id(&self, id: Uuid) -> Result<Option<GateUserRecord>, MmError>;
    fn list_users(&self, policy_id: Uuid) -> Result<Vec<GateUserMetadata>, MmError>;
    fn insert_user(&self, user: &GateUserRecord) -> Result<(), MmError>;
    fn replace_user(
        &self,
        user: &GateUserRecord,
        expected_password_version: u64,
    ) -> Result<(), MmError>;
    fn remove_user(
        &self,
        policy_id: Uuid,
        normalized_username: &str,
        expected_password_version: u64,
    ) -> Result<bool, MmError>;

    fn record_authentication_failure(&self, user_id: Uuid, now_unix_ms: i64)
    -> Result<(), MmError>;
    fn clear_authentication_failures(&self, user_id: Uuid, now_unix_ms: i64)
    -> Result<(), MmError>;

    fn insert_session(&self, session: &GateSessionMetadata) -> Result<(), MmError>;
    fn insert_session_replacing(
        &self,
        session: &GateSessionMetadata,
        replaced_session_id: Uuid,
        now_unix_ms: i64,
    ) -> Result<(), MmError>;
    fn session(&self, id: Uuid) -> Result<Option<GateSessionMetadata>, MmError>;
    fn list_sessions(
        &self,
        policy_id: Uuid,
        now_unix_ms: i64,
    ) -> Result<Vec<GateSessionMetadata>, MmError>;
    fn revoke_session(
        &self,
        id: Uuid,
        now_unix_ms: i64,
        replaced_by: Option<Uuid>,
    ) -> Result<bool, MmError>;
    fn revoke_user_sessions(&self, user_id: Uuid, now_unix_ms: i64) -> Result<u64, MmError>;

    fn attachment(
        &self,
        target_id: crate::domain::target::TargetId,
        router_name: &str,
    ) -> Result<Option<GateRouteAttachment>, MmError>;
    fn list_attachments(&self, policy_id: Uuid) -> Result<Vec<GateRouteAttachment>, MmError>;
    fn insert_attachment(&self, attachment: &GateRouteAttachment) -> Result<(), MmError>;
    fn remove_attachment(
        &self,
        target_id: crate::domain::target::TargetId,
        router_name: &str,
        policy_id: Uuid,
    ) -> Result<bool, MmError>;

    fn consume_cap_token(
        &self,
        policy_id: Uuid,
        token_digest: &str,
        now_unix_ms: i64,
        expires_at_unix_ms: i64,
    ) -> Result<bool, MmError>;

    fn append_audit(&self, record: &GateAuditRecord) -> Result<(), MmError>;
}

pub trait GatePolicySource: Send + Sync {
    fn gate_components_required(&self) -> Result<bool, MmError>;
}

#[async_trait]
pub trait GateIngressPublisher: Send + Sync {
    async fn publish(
        &self,
        target_id: TargetId,
        claims: &[IngressRouteClaim],
        middlewares: &std::collections::BTreeMap<String, CanonicalMiddleware>,
    ) -> Result<String, MmError>;
}

pub struct ProvisionedCapSite {
    pub site_key: String,
    pub site_secret: SecretMaterial,
}

impl std::fmt::Debug for ProvisionedCapSite {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProvisionedCapSite")
            .field("site_key", &self.site_key)
            .field("site_secret", &"[REDACTED]")
            .finish()
    }
}

#[async_trait]
pub trait CapSiteProvisioner: Send + Sync {
    async fn create(&self, name: &str) -> Result<ProvisionedCapSite, MmError>;
    async fn remove(&self, site_key: &str) -> Result<(), MmError>;
}

pub struct GateAuthenticationInput<'a> {
    pub username: &'a str,
    pub password: &'a SecretMaterial,
    pub cap_token: &'a SecretMaterial,
}

#[async_trait]
pub trait GateDriver: Send + Sync {
    fn id(&self) -> &GateDriverId;
    async fn authenticate(
        &self,
        policy: &GatePolicy,
        input: &GateAuthenticationInput<'_>,
    ) -> Result<GateDriverResult, MmError>;
}

pub trait PasswordVerifier: Send + Sync {
    fn hash(
        &self,
        password: &SecretMaterial,
        pepper: &SecretMaterial,
    ) -> Result<PasswordHashRecord, MmError>;
    fn verify(
        &self,
        password: &SecretMaterial,
        pepper: &SecretMaterial,
        record: &PasswordHashRecord,
    ) -> Result<bool, MmError>;
    fn requires_upgrade(&self, record: &PasswordHashRecord) -> bool;
}

#[cfg(test)]
mod tests;

pub trait GateSessionCipher: Send + Sync {
    fn seal(
        &self,
        claims: &GateSessionClaims,
        key_version: u32,
        key: &SecretMaterial,
    ) -> Result<String, MmError>;
    fn open(
        &self,
        value: &str,
        key_version: u32,
        key: &SecretMaterial,
    ) -> Result<GateSessionClaims, MmError>;
}

#[async_trait]
pub trait CapTokenVerifier: Send + Sync {
    async fn verify(
        &self,
        site_key: &str,
        site_secret: &SecretMaterial,
        token: &SecretMaterial,
    ) -> Result<bool, MmError>;
}
