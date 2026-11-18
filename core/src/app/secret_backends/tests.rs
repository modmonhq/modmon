use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::domain::secret_backend::{
    ProviderDriverId, ProviderModeDescriptor, ProviderScope, RemoteSecretBackendStatusRequest,
};

struct FakeRemote {
    calls: AtomicUsize,
    succeed: bool,
}

mod legacy;
mod managed;

mod helpers;
use helpers::{external_descriptor, external_instance, external_request};

#[tokio::test]
async fn external_lifecycle_is_reviewed_exact_and_registration_validation_fails_closed()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::secret_backend::{ProviderConfigurationField, ProviderFieldKind};

    let fake = Arc::new(FakeRemote {
        calls: AtomicUsize::new(0),
        succeed: true,
    });
    let descriptor = external_descriptor(
        vec![
            ProviderOperation::Add,
            ProviderOperation::Edit,
            ProviderOperation::Test,
            ProviderOperation::Status,
        ],
        Vec::new(),
    )?;
    let (name, instance) = external_instance()?;
    let manager = SecretBackendManager::new(
        vec![ProviderDriverRegistration {
            descriptor: descriptor.clone(),
            remote: Some(fake.clone()),
            managed: None,
        }],
        [(name.clone(), instance.clone())].into_iter().collect(),
    )?;
    assert_eq!(manager.instances()?.len(), 1);
    manager.require_operation(&name, ProviderOperation::Status)?;
    assert!(
        manager
            .require_operation(&name, ProviderOperation::Backup)
            .is_err()
    );
    assert!(
        manager
            .require_operation(
                &ProviderInstanceName::new("missing")?,
                ProviderOperation::Test
            )
            .is_err()
    );
    assert!(format!("{manager:?}").contains("team-secrets"));
    assert!(
        format!(
            "{:?}",
            ProviderDriverRegistration {
                descriptor: descriptor.clone(),
                remote: Some(fake.clone()),
                managed: None,
            }
        )
        .contains("Future Safe")
    );

    let request = external_request(name.clone(), ProviderOperation::Add);
    let plan = manager.plan_remote_setup(request.clone())?;
    assert!(
        manager
            .apply_remote_setup(
                request.clone(),
                "stale",
                SecretMaterial::new("bootstrap".to_owned())?,
            )
            .await
            .is_err()
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    let status = manager
        .apply_remote_setup(
            request,
            &plan.plan_hash,
            SecretMaterial::new("bootstrap".to_owned())?,
        )
        .await?;
    assert!(status.authenticated);
    assert!(
        manager
            .remote_status(&RemoteSecretBackendStatusRequest {
                instance: name.clone(),
                project_id: crate::domain::workspace::ProjectId(uuid::Uuid::nil()),
                environment: "local".to_owned(),
            })
            .await?
            .health
            .healthy
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);

    let mut empty = external_request(name.clone(), ProviderOperation::Edit);
    empty.locations.clear();
    assert!(manager.plan_remote_setup(empty).is_err());
    let invalid = external_request(name.clone(), ProviderOperation::Status);
    assert!(manager.plan_remote_setup(invalid).is_err());

    let duplicate = SecretBackendManager::new(
        vec![
            ProviderDriverRegistration {
                descriptor: descriptor.clone(),
                remote: Some(fake.clone()),
                managed: None,
            },
            ProviderDriverRegistration {
                descriptor: descriptor.clone(),
                remote: Some(fake.clone()),
                managed: None,
            },
        ],
        BTreeMap::new(),
    );
    assert!(duplicate.is_err());

    for invalid_descriptor in [
        ProviderDescriptor {
            driver: ProviderDriverId::new("future")?,
            display_name: String::new(),
            modes: descriptor.modes.clone(),
        },
        ProviderDescriptor {
            driver: ProviderDriverId::new("future")?,
            display_name: "Future".to_owned(),
            modes: Vec::new(),
        },
    ] {
        assert!(
            SecretBackendManager::new(
                vec![ProviderDriverRegistration {
                    descriptor: invalid_descriptor,
                    remote: Some(fake.clone()),
                    managed: None,
                }],
                BTreeMap::new(),
            )
            .is_err()
        );
    }
    assert!(
        SecretBackendManager::new(
            vec![ProviderDriverRegistration {
                descriptor: descriptor.clone(),
                remote: None,
                managed: None,
            }],
            BTreeMap::new(),
        )
        .is_err()
    );

    let secret_field = ProviderConfigurationField {
        key: "namespace".to_owned(),
        label: "Namespace".to_owned(),
        kind: ProviderFieldKind::Text,
        required: false,
        secret: true,
        default: None,
        allowed_values: Vec::new(),
    };
    assert!(
        SecretBackendManager::new(
            vec![ProviderDriverRegistration {
                descriptor: external_descriptor(vec![ProviderOperation::Test], vec![secret_field])?,
                remote: Some(fake.clone()),
                managed: None,
            }],
            BTreeMap::new(),
        )
        .is_err()
    );

    let required = ProviderConfigurationField {
        key: "namespace".to_owned(),
        label: "Namespace".to_owned(),
        kind: ProviderFieldKind::Text,
        required: true,
        secret: false,
        default: None,
        allowed_values: Vec::new(),
    };
    assert!(
        SecretBackendManager::new(
            vec![ProviderDriverRegistration {
                descriptor: external_descriptor(vec![ProviderOperation::Test], vec![required])?,
                remote: Some(fake.clone()),
                managed: None,
            }],
            [(name.clone(), instance.clone())].into_iter().collect(),
        )
        .is_err()
    );

    let allowed = ProviderConfigurationField {
        key: "storage-mount".to_owned(),
        label: "Storage mount".to_owned(),
        kind: ProviderFieldKind::Text,
        required: true,
        secret: false,
        default: None,
        allowed_values: vec!["other".to_owned()],
    };
    assert!(
        SecretBackendManager::new(
            vec![ProviderDriverRegistration {
                descriptor: external_descriptor(vec![ProviderOperation::Test], vec![allowed])?,
                remote: Some(fake.clone()),
                managed: None,
            }],
            [(name.clone(), instance.clone())].into_iter().collect(),
        )
        .is_err()
    );

    let mut wrong_scope = instance.clone();
    wrong_scope.scope = ProviderScope::Installation;
    assert!(
        SecretBackendManager::new(
            vec![ProviderDriverRegistration {
                descriptor: descriptor.clone(),
                remote: Some(fake.clone()),
                managed: None,
            }],
            [(name.clone(), wrong_scope)].into_iter().collect(),
        )
        .is_err()
    );
    let unregistered = ProviderInstanceConfig {
        driver: ProviderDriverId::new("absent")?,
        ..instance
    };
    assert!(
        SecretBackendManager::new(
            vec![ProviderDriverRegistration {
                descriptor,
                remote: Some(fake),
                managed: None,
            }],
            [(name, unregistered)].into_iter().collect(),
        )
        .is_err()
    );
    Ok(())
}
