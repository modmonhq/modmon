//! The steps of one rollout on a node share the environment the first of
//! them compiled; a node keeps a few rollouts' environments at most, and
//! forgets one when its rollout ends.

use std::sync::Arc;

use mm::app::{EnvironmentMaterial, PreparedEnvironments};
use mm::domain::operation::OperationId;
use uuid::Uuid;

fn material() -> Arc<EnvironmentMaterial> {
    Arc::new(EnvironmentMaterial {
        compiled: None,
        registry_credentials: Vec::new(),
    })
}

fn operation(number: u128) -> OperationId {
    OperationId(Uuid::from_u128(number))
}

#[test]
fn every_step_of_a_rollout_gets_the_environment_its_first_step_kept() {
    let prepared = PreparedEnvironments::default();
    assert!(prepared.get(operation(1)).is_none());
    let kept = material();
    prepared.keep(operation(1), Arc::clone(&kept));
    for _ in 0..3 {
        let again = prepared.get(operation(1));
        assert!(again.is_some_and(|again| Arc::ptr_eq(&again, &kept)));
    }
    assert!(prepared.get(operation(2)).is_none());
    assert!(prepared.holds(operation(1)));

    prepared.forget(operation(1));
    assert!(prepared.get(operation(1)).is_none());
    assert!(prepared.is_empty());
}

#[test]
fn a_node_keeps_four_rollouts_and_lets_the_least_recently_used_go() {
    let prepared = PreparedEnvironments::default();
    for number in 1..=4 {
        prepared.keep(operation(number), material());
    }
    assert!(prepared.get(operation(1)).is_some());
    prepared.keep(operation(5), material());
    assert_eq!(prepared.len(), 4);
    assert!(prepared.holds(operation(1)), "used last, so kept");
    assert!(
        !prepared.holds(operation(2)),
        "the least recently used goes"
    );
    for number in [3, 4, 5] {
        assert!(prepared.holds(operation(number)));
    }

    prepared.keep(operation(3), material());
    assert_eq!(prepared.len(), 4, "keeping one again replaces it");
}
