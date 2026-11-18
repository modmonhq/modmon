use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{
    EnvironmentArtifactGuard, dispose_bundle_root, lifecycle::completed_mutation_journal,
    retain_runtime_plan_revisions,
};
use crate::app::ports::{EnvironmentArtifactRequest, EnvironmentArtifactStore};
use crate::domain::environment::{CompiledEnvironmentManifest, PreparedEnvironmentBundle};
use crate::domain::error::MmError;
use crate::domain::operation::OperationId;

struct RecordingStore(Arc<AtomicBool>);

struct FailingDisposeStore;

impl EnvironmentArtifactStore for RecordingStore {
    fn bundle_root(&self, operation_id: OperationId) -> PathBuf {
        PathBuf::from(operation_id.0.to_string())
    }

    fn publish(
        &self,
        _request: EnvironmentArtifactRequest<'_>,
    ) -> Result<PreparedEnvironmentBundle, MmError> {
        Err(MmError::Environment {
            message: "unexpected publish".to_owned(),
        })
    }

    fn dispose(&self, _bundle: &PreparedEnvironmentBundle) -> Result<(), MmError> {
        self.0.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn reconcile(&self, _active_operations: &HashSet<OperationId>) -> Result<(), MmError> {
        Ok(())
    }
}

impl EnvironmentArtifactStore for FailingDisposeStore {
    fn bundle_root(&self, operation_id: OperationId) -> PathBuf {
        PathBuf::from(operation_id.0.to_string())
    }

    fn publish(
        &self,
        _request: EnvironmentArtifactRequest<'_>,
    ) -> Result<PreparedEnvironmentBundle, MmError> {
        Err(MmError::Environment {
            message: "unexpected publish".to_owned(),
        })
    }

    fn dispose(&self, _bundle: &PreparedEnvironmentBundle) -> Result<(), MmError> {
        Err(MmError::Environment {
            message: "injected drop cleanup failure".to_owned(),
        })
    }

    fn reconcile(&self, _active_operations: &HashSet<OperationId>) -> Result<(), MmError> {
        Ok(())
    }
}

#[test]
fn environment_guard_disposes_while_unwinding_a_panic_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let disposed = Arc::new(AtomicBool::new(false));
    let store: Arc<dyn EnvironmentArtifactStore> = Arc::new(RecordingStore(Arc::clone(&disposed)));
    let operation_id = OperationId::new();
    let bundle = PreparedEnvironmentBundle {
        operation_id,
        root: PathBuf::from(operation_id.0.to_string()),
        manifest: CompiledEnvironmentManifest {
            project_name: "test".to_owned(),
            environment: "test".to_owned(),
            input_hash: "test".to_owned(),
            services: Vec::new(),
        },
        services: Vec::new(),
        interpolation_environment: Default::default(),
        runtime_secret_overlays: Vec::new(),
        runtime_secret_rollback_overlays: Vec::new(),
        registry_credentials: Vec::new(),
    };

    let runtime_store = Arc::new(
        crate::adapters::secrets::DockerComposeFileSecretDelivery::new(
            std::env::temp_dir().join(operation_id.0.to_string()),
        )?,
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = EnvironmentArtifactGuard::new(store, Some(bundle), runtime_store, None);
        std::panic::resume_unwind(Box::new("injected panic boundary"));
    }));

    assert!(result.is_err());
    assert!(disposed.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn environment_guard_logs_drop_failures_and_ignores_unowned_bundle_roots()
-> Result<(), Box<dyn std::error::Error>> {
    let operation_id = OperationId::new();
    let bundle = PreparedEnvironmentBundle {
        operation_id,
        root: PathBuf::from(operation_id.0.to_string()),
        manifest: CompiledEnvironmentManifest {
            project_name: "test".to_owned(),
            environment: "test".to_owned(),
            input_hash: "test".to_owned(),
            services: Vec::new(),
        },
        services: Vec::new(),
        interpolation_environment: Default::default(),
        runtime_secret_overlays: Vec::new(),
        runtime_secret_rollback_overlays: Vec::new(),
        registry_credentials: Vec::new(),
    };
    let store: Arc<dyn EnvironmentArtifactStore> = Arc::new(FailingDisposeStore);
    let runtime_store = Arc::new(
        crate::adapters::secrets::DockerComposeFileSecretDelivery::new(
            std::env::temp_dir().join(operation_id.0.to_string()),
        )?,
    );
    drop(EnvironmentArtifactGuard::new(
        Arc::clone(&store),
        Some(bundle),
        runtime_store,
        None,
    ));

    dispose_bundle_root(store.as_ref(), std::path::Path::new("/"))?;
    dispose_bundle_root(store.as_ref(), std::path::Path::new("not-an-operation"))?;
    Ok(())
}

#[test]
fn journal_finish_failure_after_mutation_retains_applied_state_and_operation_id()
-> Result<(), Box<dyn std::error::Error>> {
    let operation_id = OperationId(uuid::Uuid::from_u128(42));
    let error = completed_mutation_journal(
        operation_id,
        Err(MmError::State {
            message: "injected journal finish failure".to_owned(),
        }),
    )
    .err()
    .ok_or("injected journal failure unexpectedly succeeded")?;

    assert!(matches!(
        error,
        MmError::OperationFailure {
            mutation_status: crate::domain::error::MutationStatus::Applied,
            ..
        }
    ));
    Ok(())
}

#[test]
fn runtime_secret_revision_retention_distinguishes_pre_and_post_convergence() -> Result<(), String>
{
    let plan = crate::domain::runtime_secret::RuntimeSecretPlan::new(
        uuid::Uuid::from_u128(1).to_string(),
        uuid::Uuid::from_u128(2).to_string(),
        "local".to_owned(),
        crate::domain::runtime::RuntimeKind::DockerCompose,
        vec![crate::domain::runtime_secret::RuntimeSecretPlanItem {
            logical_name: "database.password".to_owned(),
            backend: "age".to_owned(),
            delivery: crate::domain::secret_contract::DeliveryDriverId::new("docker-secret")?,
            source_revision: "current".to_owned(),
            previous_source_revision: Some("previous".to_owned()),
            consumers: vec![crate::domain::runtime_secret::RuntimeSecretConsumer {
                stack: "app".to_owned(),
                service: "web".to_owned(),
                target: "database-password".to_owned(),
                uid: "0".to_owned(),
                gid: "0".to_owned(),
                mode: 0o400,
            }],
        }],
    )?;

    let mut before = BTreeMap::new();
    retain_runtime_plan_revisions(&plan, true, &mut before);
    assert_eq!(
        before["database.password"],
        ["current".to_owned(), "previous".to_owned()]
            .into_iter()
            .collect()
    );

    let mut after = BTreeMap::new();
    retain_runtime_plan_revisions(&plan, false, &mut after);
    assert_eq!(
        after["database.password"],
        ["current".to_owned()].into_iter().collect()
    );
    Ok(())
}
