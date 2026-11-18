use super::server_error;
use crate::domain::error::MmError;
use crate::domain::remote::{MutationState, RemoteError};

fn remote(code: &str, message: &str) -> MmError {
    server_error(RemoteError {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable: false,
        mutation_state: MutationState::Unknown,
    })
}

#[test]
fn a_host_error_reads_as_it_did_on_the_host_with_its_own_code() {
    let error = remote(
        "MM_TARGET",
        "target error: node 'app-2' runs the services of 'production'",
    );
    assert_eq!(error.code(), "MM_TARGET");
    assert_eq!(
        error.to_string(),
        "target error: node 'app-2' runs the services of 'production'",
        "the prefix is not doubled and nothing wraps it"
    );
    assert_eq!(error.exit_code(), 2);

    let error = remote("MM_PLACEMENT", "placement: 'cache-1' is not a node");
    assert_eq!(error.code(), "MM_PLACEMENT");
    assert_eq!(error.to_string(), "placement: 'cache-1' is not a node");

    let error = remote(
        "MM_NOT_DEPLOYED",
        "nothing is deployed to 'production' on this host yet",
    );
    assert_eq!(error.code(), "MM_NOT_DEPLOYED");
    assert_eq!(
        error.refusal().as_deref(),
        Some("nothing is deployed to 'production' on this host yet"),
        "an agent gets the host's own words"
    );
}

#[test]
fn only_a_policy_rejection_is_reported_as_one() {
    let denied = remote(
        "MM_SECURITY",
        "security policy rejected the operation: alice may not deploy production",
    );
    assert_eq!(denied.code(), "MM_SECURITY");
    assert_eq!(
        denied.to_string(),
        "security policy rejected the operation: alice may not deploy production"
    );
    for (code, message) in [
        ("MM_ENVIRONMENT", "environment error: nothing is deployed"),
        ("MM_STATE", "state store error: plan changed"),
        ("MM_BUILD", "build error: images are missing"),
        (
            "MM_PROCESS_FAILED",
            "docker failed with exit code Some(1): no such image",
        ),
        ("MM_NODE_MISMATCH", "this request is for node 'app-2'"),
    ] {
        let error = remote(code, message);
        assert_eq!(error.code(), code);
        assert_eq!(error.to_string(), message);
        assert!(!error.to_string().contains("security policy"), "{error}");
    }
}

#[test]
fn a_code_this_client_does_not_know_is_kept_in_the_message() {
    let error = remote("MM_FROM_THE_FUTURE", "something new");
    assert_eq!(error.code(), "MM_REMOTE");
    assert_eq!(error.to_string(), "MM_FROM_THE_FUTURE: something new");
}
