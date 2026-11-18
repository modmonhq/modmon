use super::{
    ApprovalContext, ExecEnvironmentReference, ExecRequest, ExecStdinMode, ExecTtyMode, LogLimits,
    LogMode, LogOutputPolicy, LogRequest, MAX_EXEC_ARGUMENT_BYTES, MAX_LOG_BUFFER_BYTES,
    MAX_LOG_BUFFER_RECORDS, MAX_LOG_LINE_BYTES, ResolvedExecRequest, ServiceSelector, argv_hash,
    environment_values, validate_exec_request, validate_log_request, validate_selector,
};
use crate::domain::secret::SecretReference;
use std::collections::BTreeMap;
use std::time::Duration;

fn selector() -> ServiceSelector {
    ServiceSelector {
        stack: Some("app".to_owned()),
        module: Some("api".to_owned()),
        service: "web".to_owned(),
        container: Some("web-1".to_owned()),
    }
}

fn approval() -> ApprovalContext {
    ApprovalContext {
        principal: "operator".to_owned(),
        source: "test".to_owned(),
        approved_high_risk: false,
    }
}

fn log_request() -> LogRequest {
    LogRequest {
        selector: Some(selector()),
        nodes: Vec::new(),
        tail: 100,
        since_unix_ms: Some(100),
        until_unix_ms: Some(200),
        follow: false,
        timestamps: true,
        output: LogOutputPolicy::Both,
        mode: LogMode::Redacted,
        limits: LogLimits::default(),
        approval: approval(),
    }
}

fn exec_request() -> ExecRequest {
    ExecRequest {
        selector: selector(),
        argv: vec!["printf".to_owned(), "safe".to_owned()],
        user: None,
        workdir: None,
        environment: Vec::new(),
        tty: ExecTtyMode::Disabled,
        stdin: ExecStdinMode::Closed,
        timeout: Duration::from_secs(30),
        approval: approval(),
        shell_wrapper: false,
    }
}

#[test]
fn selectors_reject_empty_oversized_control_and_option_like_values() {
    assert_eq!(validate_selector(&selector()), Ok(()));
    for (field, value, expected) in [
        ("service", "", "service selector is invalid"),
        ("stack", "-stack", "stack selector is invalid"),
        ("module", "module\n", "module selector is invalid"),
        ("container", "-container", "container selector is invalid"),
    ] {
        let mut candidate = selector();
        match field {
            "service" => candidate.service = value.to_owned(),
            "stack" => candidate.stack = Some(value.to_owned()),
            "module" => candidate.module = Some(value.to_owned()),
            _ => candidate.container = Some(value.to_owned()),
        }
        assert_eq!(
            validate_selector(&candidate)
                .as_ref()
                .map_err(String::as_str),
            Err(expected)
        );
    }
    let mut candidate = selector();
    candidate.service = "x".repeat(256);
    assert_eq!(
        validate_selector(&candidate)
            .as_ref()
            .map_err(String::as_str),
        Err("service selector is invalid")
    );
}

#[test]
fn log_limits_reject_every_unbounded_or_reversed_request_shape() {
    assert_eq!(validate_log_request(&log_request()), Ok(()));

    let mut invalid = log_request();
    invalid.tail = 1_000_001;
    assert_eq!(
        validate_log_request(&invalid)
            .as_ref()
            .map_err(String::as_str),
        Err("log tail exceeds 1000000 records")
    );

    let mut invalid = log_request();
    invalid.since_unix_ms = Some(201);
    assert_eq!(
        validate_log_request(&invalid)
            .as_ref()
            .map_err(String::as_str),
        Err("log since timestamp is after until timestamp")
    );

    let invalid_limits = [
        LogLimits {
            maximum_line_bytes: 0,
            ..LogLimits::default()
        },
        LogLimits {
            maximum_line_bytes: MAX_LOG_LINE_BYTES + 1,
            ..LogLimits::default()
        },
        LogLimits {
            buffered_records: 0,
            ..LogLimits::default()
        },
        LogLimits {
            buffered_records: MAX_LOG_BUFFER_RECORDS + 1,
            ..LogLimits::default()
        },
        LogLimits {
            maximum_line_bytes: MAX_LOG_BUFFER_BYTES,
            buffered_records: 2,
            ..LogLimits::default()
        },
        LogLimits {
            maximum_bytes_per_second: Some(0),
            ..LogLimits::default()
        },
        LogLimits {
            maximum_lines_per_second: Some(0),
            ..LogLimits::default()
        },
    ];
    for limits in invalid_limits {
        let mut invalid = log_request();
        invalid.limits = limits;
        assert_eq!(
            validate_log_request(&invalid)
                .as_ref()
                .map_err(String::as_str),
            Err("log bounds are outside the supported range")
        );
    }
}

#[test]
fn exec_limits_reject_ambiguous_or_unsafe_command_and_environment_values() -> Result<(), String> {
    assert_eq!(validate_exec_request(&exec_request()), Ok(()));

    for argv in [Vec::new(), vec!["x".to_owned(); 257]] {
        let mut invalid = exec_request();
        invalid.argv = argv;
        assert_eq!(
            validate_exec_request(&invalid)
                .as_ref()
                .map_err(String::as_str),
            Err("exec argv must contain between 1 and 256 values")
        );
    }
    for argument in [
        String::new(),
        "nul\0byte".to_owned(),
        "x".repeat(MAX_EXEC_ARGUMENT_BYTES + 1),
    ] {
        let mut invalid = exec_request();
        invalid.argv = vec![argument];
        assert_eq!(
            validate_exec_request(&invalid)
                .as_ref()
                .map_err(String::as_str),
            Err("exec argv contains an empty, NUL, or oversized value")
        );
    }
    for timeout in [Duration::ZERO, Duration::from_secs(24 * 60 * 60 + 1)] {
        let mut invalid = exec_request();
        invalid.timeout = timeout;
        assert_eq!(
            validate_exec_request(&invalid)
                .as_ref()
                .map_err(String::as_str),
            Err("exec timeout must be between one nanosecond and 24 hours")
        );
    }

    let reference = "age://services/example".parse::<SecretReference>()?;
    for name in ["", "1TOKEN", "DASH-NAME", "NAME\n", &"X".repeat(256)] {
        let mut invalid = exec_request();
        invalid.environment.push(ExecEnvironmentReference {
            name: name.to_owned(),
            reference: reference.clone(),
        });
        assert_eq!(
            validate_exec_request(&invalid)
                .as_ref()
                .map_err(String::as_str),
            Err("exec environment name is invalid")
        );
    }

    let mut duplicate = exec_request();
    duplicate.environment = ["TOKEN", "TOKEN"]
        .into_iter()
        .map(|name| ExecEnvironmentReference {
            name: name.to_owned(),
            reference: reference.clone(),
        })
        .collect();
    assert_eq!(
        validate_exec_request(&duplicate)
            .as_ref()
            .map_err(String::as_str),
        Err("exec environment names must be unique")
    );
    Ok(())
}

#[test]
fn command_hashes_bind_argument_boundaries_and_runtime_environment_debug_hides_values() {
    let first = ["ab".to_owned(), "c".to_owned()];
    let second = ["a".to_owned(), "bc".to_owned()];
    assert_eq!(argv_hash(&first), argv_hash(&first));
    assert_ne!(argv_hash(&first), argv_hash(&second));

    let environment = environment_values(BTreeMap::from([
        ("API_TOKEN".to_owned(), "sentinel-one".to_owned()),
        ("DB_PASSWORD".to_owned(), "sentinel-two".to_owned()),
    ]));
    let debug = format!("{environment:?}");
    assert!(debug.contains("API_TOKEN"));
    assert!(debug.contains("DB_PASSWORD"));
    assert!(!debug.contains("sentinel-one"));
    assert!(!debug.contains("sentinel-two"));

    let resolved = ResolvedExecRequest {
        request: exec_request(),
        environment,
    };
    let debug = format!("{resolved:?}");
    assert!(debug.contains("API_TOKEN"));
    assert!(debug.contains("DB_PASSWORD"));
    assert!(!debug.contains("sentinel"));
}

#[test]
fn output_redaction_replaces_secrets_without_eating_ordinary_text() {
    use super::redact_output;

    // A long value is replaced wherever it appears, even inside other text.
    let long = ["s3cr3t-t0ken-value".to_owned()];
    assert_eq!(
        redact_output("GET /hook?key=s3cr3t-t0ken-valueXYZ 200", &long),
        "GET /hook?key=[REDACTED]XYZ 200"
    );

    // A short value is replaced where it stands alone.
    let short = ["hunter2".to_owned()];
    for (line, expected) in [
        ("password=hunter2", "password=[REDACTED]"),
        ("login hunter2 failed", "login [REDACTED] failed"),
        (r#"{"pw":"hunter2"}"#, r#"{"pw":"[REDACTED]"}"#),
        (
            "mysql://app:hunter2@db/app",
            "mysql://app:[REDACTED]@db/app",
        ),
        ("the password is hunter2.", "the password is [REDACTED]."),
        ("hunter2, hunter2", "[REDACTED], [REDACTED]"),
        // Part of a longer word, path, or name: not the secret.
        ("/srv/hunter2/app.log", "/srv/hunter2/app.log"),
        ("hunter22 and xhunter2", "hunter22 and xhunter2"),
        ("hunter2.example.com", "hunter2.example.com"),
        ("my-hunter2_backup", "my-hunter2_backup"),
    ] {
        assert_eq!(redact_output(line, &short), expected, "{line}");
    }

    // Words that fill ordinary output, and values too short to tell apart
    // from it, are left alone.
    let ordinary = ["null", "True", "abc", "", "1"].map(str::to_owned);
    let line = "exec 3>/dev/null; ok=True abc 1 value: null";
    assert_eq!(redact_output(line, &ordinary), line);

    // Every value is applied, longest first when the caller sorts them so.
    let both = ["hunter2-extended".to_owned(), "hunter2".to_owned()];
    assert_eq!(
        redact_output("a=hunter2-extended b=hunter2", &both),
        "a=[REDACTED] b=[REDACTED]"
    );
    let none: [String; 0] = [];
    assert_eq!(redact_output("unchanged", &none), "unchanged");
}
