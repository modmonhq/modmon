use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::operation::OperationId;
use crate::domain::runtime_secret::{
    PreparedRuntimeSecrets, RuntimeSecretCapability, RuntimeSecretMaterial, RuntimeSecretPlan,
};
use crate::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretReplacement,
    SecretResolution, SecretScope, SecretWriteResult,
};
use crate::domain::secret_contract::{StorageCapabilityDescriptor, StorageProviderId};

pub trait SecretStorageCapabilityCatalog: Send + Sync {
    fn storage_capabilities(&self) -> Vec<StorageCapabilityDescriptor>;
    fn storage_capability(
        &self,
        provider: &StorageProviderId,
    ) -> Result<StorageCapabilityDescriptor, MmError>;
}

#[async_trait]
pub trait SecretMetadataInspector: Send + Sync {
    async fn inspect(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError>;

    /// The metadata of each of `references` in `scope`, in their order.
    async fn inspect_many(
        &self,
        scope: &SecretScope,
        references: &[SecretReference],
    ) -> Result<Vec<SecretMetadata>, MmError> {
        let mut found = Vec::with_capacity(references.len());
        for reference in references {
            found.push(self.inspect(scope, reference).await?);
        }
        Ok(found)
    }
}

#[async_trait]
pub trait SecretMaterialResolver: Send + Sync {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError>;
}

#[async_trait]
pub trait SecretCreator: Send + Sync {
    async fn create(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        now_unix_ms: i64,
    ) -> Result<SecretWriteResult, MmError>;
}

#[async_trait]
pub trait SecretReplacer: Send + Sync {
    async fn replace(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError>;
}

#[async_trait]
pub trait SecretBatchReplacer: Send + Sync {
    async fn replace_batch(
        &self,
        replacements: Vec<SecretReplacement>,
        now_unix_ms: i64,
    ) -> Result<Vec<String>, MmError>;
}

#[async_trait]
pub trait SecretCandidateLifecycle: Send + Sync {
    async fn stage(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError>;
    async fn promote(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError>;
}

#[async_trait]
pub trait SecretRemover: Send + Sync {
    async fn unset(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
    ) -> Result<bool, MmError>;
}

#[async_trait]
pub trait RuntimeSecretDelivery: Send + Sync {
    fn capability(&self) -> &RuntimeSecretCapability;

    async fn prepare(
        &self,
        operation_id: OperationId,
        plan: &RuntimeSecretPlan,
        materials: Vec<RuntimeSecretMaterial>,
    ) -> Result<PreparedRuntimeSecrets, MmError>;

    fn dispose(&self, prepared: &PreparedRuntimeSecrets) -> Result<(), MmError>;

    async fn remove_revision(
        &self,
        plan: &RuntimeSecretPlan,
        logical_name: &str,
        source_revision: &str,
    ) -> Result<bool, MmError>;

    async fn reconcile(
        &self,
        plan: &RuntimeSecretPlan,
        retained_revisions: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
        active_operations: &std::collections::HashSet<OperationId>,
    ) -> Result<Vec<String>, MmError>;
}
