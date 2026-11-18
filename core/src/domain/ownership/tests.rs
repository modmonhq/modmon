use super::{
    LABEL_GENERATION, LABEL_RESOURCE, LABEL_RESOURCE_KIND, LABEL_TARGET_ID, OwnershipVerdict,
    ResourceKind, ResourceOwnership, validate_resource_label, validate_resource_name,
};
use crate::domain::target::{NamedTarget, TargetDefinition, TargetId};
use uuid::Uuid;

fn ownership() -> ResourceOwnership {
    ResourceOwnership::for_named_target(
        &NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(Uuid::nil())),
        ),
        ResourceKind::Network,
        "mm-ingress",
        "ingress-network",
    )
}

#[test]
fn owned_labels_match_exactly() {
    let ownership = ownership();
    let labels = ownership.labels();
    assert_eq!(
        ownership.verify("mm-ingress", Some(&labels)),
        OwnershipVerdict::Owned
    );
}

#[test]
fn unmanaged_same_name_is_a_conflict() {
    let ownership = ownership();
    let expected = ownership.labels();
    assert_eq!(
        ownership.verify("mm-ingress", None),
        OwnershipVerdict::Conflict {
            expected: expected.clone(),
            observed: [("name".to_owned(), "mm-ingress".to_owned())]
                .into_iter()
                .collect(),
        }
    );
    let mut foreign = expected.clone();
    foreign.insert(
        LABEL_TARGET_ID.to_owned(),
        "019c527b-6242-7b43-bfd4-bd60e27c1de3".to_owned(),
    );
    let mut observed = foreign.clone();
    observed.insert("name".to_owned(), "mm-ingress".to_owned());
    assert_eq!(
        ownership.verify("mm-ingress", Some(&foreign)),
        OwnershipVerdict::Conflict { expected, observed }
    );
}

#[test]
fn stale_generation_and_wrong_kind_are_conflicts() {
    let ownership = ownership();
    let mut stale = ownership.labels();
    stale.insert(LABEL_GENERATION.to_owned(), "0".to_owned());
    assert!(matches!(
        ownership.verify("mm-ingress", Some(&stale)),
        OwnershipVerdict::Conflict { .. }
    ));
    let mut wrong_kind = ownership.labels();
    wrong_kind.insert(LABEL_RESOURCE_KIND.to_owned(), "volume".to_owned());
    assert!(matches!(
        ownership.verify("mm-ingress", Some(&wrong_kind)),
        OwnershipVerdict::Conflict { .. }
    ));
    let mut wrong_resource = ownership.labels();
    wrong_resource.insert(LABEL_RESOURCE.to_owned(), "other".to_owned());
    assert!(matches!(
        ownership.verify("mm-ingress", Some(&wrong_resource)),
        OwnershipVerdict::Conflict { .. }
    ));
}

#[test]
fn name_mismatch_is_a_conflict() {
    let ownership = ownership();
    let labels = ownership.labels();
    assert!(matches!(
        ownership.verify("other", Some(&labels)),
        OwnershipVerdict::Conflict { .. }
    ));
}

#[test]
fn resource_kind_labels_and_runtime_names_cover_every_security_boundary() {
    let kinds = [
        ResourceKind::Network,
        ResourceKind::Volume,
        ResourceKind::Config,
        ResourceKind::Secret,
        ResourceKind::Service,
        ResourceKind::Stack,
    ];
    assert_eq!(
        kinds.map(ResourceKind::as_str),
        ["network", "volume", "config", "secret", "service", "stack"]
    );
    for valid in ["a", "resource_1", "resource.name", "resource-name"] {
        assert!(validate_resource_name(valid).is_ok());
        assert!(validate_resource_label(valid).is_ok());
    }
    for invalid in ["", "-leading", "bad/name", &"x".repeat(256)] {
        assert!(validate_resource_name(invalid).is_err(), "{invalid:?}");
    }
    for invalid in ["", "line\nbreak", &"x".repeat(256)] {
        assert!(validate_resource_label(invalid).is_err(), "{invalid:?}");
    }

    let mut stack = ownership();
    stack.kind = ResourceKind::Stack;
    assert_eq!(
        stack.labels().get(super::LABEL_STACK),
        Some(&stack.resource)
    );
    assert!(
        stack
            .require_owned("mm-ingress", Some(&stack.labels()))
            .is_ok()
    );
    assert!(stack.require_owned("foreign", None).is_err());
}
