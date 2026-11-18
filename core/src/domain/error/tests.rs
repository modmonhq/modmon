use super::{MmError, MutationStatus};
use crate::domain::runtime::{RuntimeCapability, RuntimeKind};
use crate::domain::secret_contract::SecretWorkloadKind;
use std::path::PathBuf;

#[test]
fn mutation_status_and_error_code_exit_contracts_are_complete() {
    assert_eq!(MutationStatus::NotStarted.to_string(), "not started");
    assert_eq!(MutationStatus::Unknown.to_string(), "unknown");
    assert_eq!(MutationStatus::Applied.to_string(), "applied");

    let errors = vec![
        (
            MmError::ConfigNotFound {
                start: PathBuf::from("."),
            },
            "MM_CONFIG_NOT_FOUND",
            2,
        ),
        (
            MmError::ConfigRead {
                path: PathBuf::from("mm.yaml"),
                source: std::io::Error::other("read"),
            },
            "MM_CONFIG_READ",
            2,
        ),
        (
            MmError::ConfigParse {
                path: PathBuf::from("mm.yaml"),
                message: "parse".to_owned(),
            },
            "MM_CONFIG_PARSE",
            2,
        ),
        (MmError::EnvironmentRequired, "MM_ENVIRONMENT_REQUIRED", 2),
        (
            MmError::Environment {
                message: "environment".to_owned(),
            },
            "MM_ENVIRONMENT",
            2,
        ),
        (
            MmError::Secret {
                message: "secret".to_owned(),
            },
            "MM_SECRET",
            3,
        ),
        (
            MmError::SecretCompatibility {
                logical_secret: "secret".to_owned(),
                backend: "age".to_owned(),
                delivery: "direct".to_owned(),
                runtime: RuntimeKind::DockerCompose,
                workload: SecretWorkloadKind::Service,
                supported: "none".to_owned(),
            },
            "MM_SECRET_INCOMPATIBLE",
            3,
        ),
        (
            MmError::Target {
                message: "target".to_owned(),
            },
            "MM_TARGET",
            2,
        ),
        (
            MmError::Build {
                message: "build".to_owned(),
            },
            "MM_BUILD",
            2,
        ),
        (
            MmError::TargetOperationInProgress {
                target_id: uuid::Uuid::nil(),
            },
            "MM_TARGET_OPERATION_IN_PROGRESS",
            3,
        ),
        (
            MmError::Security {
                message: "security".to_owned(),
            },
            "MM_SECURITY",
            3,
        ),
        (MmError::AuthenticationFailed, "MM_SECURITY", 3),
        (MmError::Validation { errors: 1 }, "MM_CONFIG_INVALID", 2),
        (
            MmError::ConfigAlreadyExists {
                path: PathBuf::from("mm.yaml"),
            },
            "MM_CONFIG_EXISTS",
            2,
        ),
        (
            MmError::Io {
                context: "read".to_owned(),
                source: std::io::Error::other("io"),
            },
            "MM_IO",
            1,
        ),
        (
            MmError::ProcessSpawn {
                program: "tool".to_owned(),
                source: std::io::Error::other("spawn"),
            },
            "MM_PROCESS_SPAWN",
            3,
        ),
        (
            MmError::ProcessTimeout {
                program: "tool".to_owned(),
                seconds: 1,
            },
            "MM_PROCESS_TIMEOUT",
            3,
        ),
        (
            MmError::ProcessFailed {
                program: "tool".to_owned(),
                code: Some(1),
                stderr: "failed".to_owned(),
            },
            "MM_PROCESS_FAILED",
            3,
        ),
        (
            MmError::RuntimeResponse {
                message: "response".to_owned(),
            },
            "MM_RUNTIME_RESPONSE",
            3,
        ),
        (
            MmError::unsupported_capability(
                RuntimeKind::DockerCompose,
                None,
                "exec",
                vec![RuntimeCapability::Exec],
                Some("1.0".to_owned()),
            ),
            "MM_RUNTIME_UNSUPPORTED",
            3,
        ),
        (
            MmError::Timeout {
                message: "timeout".to_owned(),
            },
            "MM_TIMEOUT",
            3,
        ),
        (
            MmError::Cancelled {
                message: "cancelled".to_owned(),
            },
            "MM_CANCELLED",
            3,
        ),
        (
            MmError::State {
                message: "state".to_owned(),
            },
            "MM_STATE",
            1,
        ),
        (
            MmError::Terminal {
                message: "terminal".to_owned(),
            },
            "MM_TERMINAL",
            1,
        ),
    ];
    for (error, code, exit_code) in errors {
        assert_eq!(error.code(), code);
        assert_eq!(error.exit_code(), exit_code);
        assert!(!error.to_string().is_empty());
    }

    let wrapped = MmError::operation_failure(
        "operation",
        MutationStatus::Unknown,
        MmError::Build {
            message: "failed".to_owned(),
        },
        vec!["cleanup".to_owned()],
        "retry",
    );
    assert_eq!(wrapped.code(), "MM_BUILD");
    assert_eq!(wrapped.exit_code(), 2);
    assert!(wrapped.to_string().contains("mutation state: unknown"));
}

#[test]
fn a_wrong_node_refusal_names_both_nodes_and_what_to_check() {
    let error = MmError::WrongNode {
        requested: "app-1".to_owned(),
        actual: Some("app-2".to_owned()),
    };
    assert_eq!(error.code(), crate::domain::error::NODE_MISMATCH_CODE);
    assert_eq!(error.exit_code(), 3);
    let text = error.to_string();
    assert!(text.contains("for node 'app-1'"), "{text}");
    assert!(text.contains("is node 'app-2'"), "{text}");
    assert!(text.contains("mm target setup record"), "{text}");

    let unnamed = MmError::WrongNode {
        requested: "app-1".to_owned(),
        actual: None,
    }
    .to_string();
    assert!(unnamed.contains("/etc/mm/node.env is missing"), "{unnamed}");
}

#[test]
fn an_unreachable_host_reads_as_subject_problem_and_next_step() {
    let error = MmError::Unreachable {
        subject: "node 'app-2' (203.0.113.12:22)".to_owned(),
        problem: "did not answer: Connection timed out".to_owned(),
        help: "Check that the host is up".to_owned(),
    };
    assert_eq!(error.code(), "MM_UNREACHABLE");
    assert_eq!(error.exit_code(), 3);
    assert_eq!(
        error.to_string(),
        "node 'app-2' (203.0.113.12:22) did not answer: Connection timed out. Check that the host is up"
    );
}
