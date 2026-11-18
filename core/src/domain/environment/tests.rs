use super::*;
use crate::domain::environment_delivery::{
    EnvironmentDeliveryMode, EnvironmentDeliveryProvenance, ResolvedEnvironmentDelivery,
};
use crate::domain::secret::{SecretMaterial, SecretStatus};
use std::sync::Arc;

#[test]
fn status_conversion_compilation_redaction_and_review_hashes_are_complete() -> Result<(), String> {
    let statuses = [
        (SecretStatus::Set, ResolutionStatus::Resolved),
        (SecretStatus::Unset, ResolutionStatus::Unset),
        (SecretStatus::NotChecked, ResolutionStatus::NotChecked),
        (
            SecretStatus::ProviderUnavailable,
            ResolutionStatus::ProviderUnavailable,
        ),
        (SecretStatus::AccessDenied, ResolutionStatus::AccessDenied),
        (SecretStatus::Invalid, ResolutionStatus::Invalid),
    ];
    for (source, expected) in statuses {
        assert_eq!(ResolutionStatus::from(source), expected);
    }
    assert!(is_safe_environment_value("ordinary value"));
    assert!(!is_safe_environment_value("line\nbreak"));

    let direct = crate::domain::runtime::RuntimeEnvironment::from_sensitive_values(BTreeMap::from(
        [("TOKEN".to_owned(), "direct-sentinel".to_owned())],
    ));
    let runtime_secret = CompiledRuntimeSecret {
        logical_name: "database.password".to_owned(),
        material: Arc::new(SecretMaterial::new("secret-sentinel".to_owned())?),
    };
    let compiled = CompiledServiceEnvironment::new(
        "api".to_owned(),
        "app".to_owned(),
        "web".to_owned(),
        b"file-sentinel".to_vec(),
    )
    .with_intent(direct)
    .with_runtime_secrets(vec![runtime_secret.clone()]);
    assert_eq!(compiled.bytes(), b"file-sentinel");
    let debug = format!("{compiled:?} {runtime_secret:?}");
    for sentinel in ["direct-sentinel", "file-sentinel", "secret-sentinel"] {
        assert!(!debug.contains(sentinel));
    }

    let base = EnvironmentReview {
        input_hash: "input".to_owned(),
        services: vec![CompiledServiceManifest {
            module: "api".to_owned(),
            stack: "app".to_owned(),
            service: "web".to_owned(),
            variable_count: 2,
            secret_count: 1,
        }],
        secret_revisions: BTreeMap::from([
            ("present".to_owned(), Some("r1".to_owned())),
            ("absent".to_owned(), None),
        ]),
        delivery: ResolvedEnvironmentDelivery {
            mode: EnvironmentDeliveryMode::Direct,
            provenance: EnvironmentDeliveryProvenance::EnvironmentOverride,
        },
    };
    let digest = base.digest();
    for mutation in [
        |review: &mut EnvironmentReview| review.input_hash.push('x'),
        |review: &mut EnvironmentReview| review.services[0].variable_count += 1,
        |review: &mut EnvironmentReview| {
            review.secret_revisions.insert("present".to_owned(), None);
        },
        |review: &mut EnvironmentReview| review.delivery.mode = EnvironmentDeliveryMode::EnvFile,
    ] {
        let mut changed = base.clone();
        mutation(&mut changed);
        assert_ne!(changed.digest(), digest);
    }
    Ok(())
}
