use super::{LocalDeploymentRuntime, OwnedVolumeFile, VolumeOwnership};
use crate::domain::error::MmError;
use crate::domain::runtime::{ResourceEnsureResult, RuntimeResource, RuntimeTarget};
use crate::domain::target::{NamedTarget, TargetDefinition, TargetId};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
struct MinimalRuntime {
    replacement: Mutex<Option<(String, u32)>>,
}

#[async_trait]
impl LocalDeploymentRuntime for MinimalRuntime {
    async fn preflight(
        &self,
        _target: &NamedTarget,
        _resources: &[RuntimeResource],
        _projects: &[RuntimeTarget],
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn ensure_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        Ok(ResourceEnsureResult::Unchanged)
    }

    async fn inspect_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<bool, MmError> {
        Ok(false)
    }

    async fn remove_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn ensure_volume(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        Ok(ResourceEnsureResult::Unchanged)
    }

    async fn remove_volume(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn create_volume_file_if_absent(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        _file_name: &str,
        _mode: u32,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn replace_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        _volume_resource: &str,
        file_name: &str,
        _source: &Path,
        mode: u32,
    ) -> Result<(), MmError> {
        *self.replacement.lock().map_err(|_| MmError::State {
            message: "replacement lock poisoned".to_owned(),
        })? = Some((file_name.to_owned(), mode));
        Ok(())
    }

    async fn apply(&self, _target: &RuntimeTarget) -> Result<(), MmError> {
        Ok(())
    }

    async fn remove(&self, _target: &RuntimeTarget) -> Result<(), MmError> {
        Ok(())
    }
}

#[tokio::test]
async fn default_owned_volume_operations_fail_closed_or_preserve_reviewed_mode()
-> Result<(), MmError> {
    let runtime = MinimalRuntime::default();
    let target = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::from_u128(1))),
    );
    let ownership = VolumeOwnership {
        uid: 1000,
        gid: 1000,
        mode: 0o640,
    };
    assert!(
        runtime
            .set_volume_owner(&target, "data", "resource", ownership)
            .await
            .is_err()
    );
    assert!(
        runtime
            .remove_volume_file(&target, "data", "resource", "config")
            .await
            .is_err()
    );
    runtime
        .replace_volume_file_owned(
            &target,
            "data",
            "resource",
            OwnedVolumeFile {
                name: "config",
                ownership,
            },
            Path::new("/reviewed/source"),
        )
        .await?;
    assert_eq!(
        *runtime.replacement.lock().map_err(|_| MmError::State {
            message: "replacement lock poisoned".to_owned(),
        })?,
        Some(("config".to_owned(), 0o640))
    );
    Ok(())
}
