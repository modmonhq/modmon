use crate::app::config_validation::validate;
use crate::domain::config::{ConfigDocument, MisfirePolicy, MmConfig};
use std::path::PathBuf;

#[test]
fn malformed_jobs_schedules_and_gates_report_all_bounded_contract_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: scheduler-validation
target: local
stacks: {}
jobs:
  "bad job":
    module: missing
    service: missing
    command: []
    environment: absent
    timeout: 0s
    retry:
      attempts: 0
      backoff: 0s
      max-backoff: 0s
    output-limit-bytes: 0
    history-limit: 0
schedules:
  "bad schedule":
    job: missing
    cron: invalid
    timezone: Not/AZone
    starts-at: invalid
    ends-at: invalid
    jitter: 25h
    retry:
      attempts: 101
      backoff: 2m
      max-backoff: 1m
    timeout: 0s
  reversed:
    job: missing
    cron: "* * * * *"
    timezone: UTC
    starts-at: 2026-01-02T00:00:00Z
    ends-at: 2026-01-01T00:00:00Z
gates:
  "bad gate":
    provider: " "
    ingress: [""]
environments:
  local:
    bindings: {}
"#;
    let mut config = serde_yaml_ng::from_str::<MmConfig>(source)?;
    let oversized = config.jobs["bad job"].clone();
    config.jobs.insert(
        "oversized".to_owned(),
        crate::domain::config::JobConfig {
            command: vec!["x".to_owned(); 257],
            timeout: crate::domain::config::DurationSpec("25h".to_owned()),
            output_limit_bytes: crate::domain::scheduler::MAX_JOB_OUTPUT_BYTES + 1,
            history_limit: crate::domain::scheduler::MAX_JOB_HISTORY_ENTRIES + 1,
            placement: None,
            user: None,
            placed_elsewhere: None,
            development: None,
            off_locally: None,
            ..oversized
        },
    );
    config
        .schedules
        .get_mut("bad schedule")
        .ok_or("missing schedule")?
        .misfire = MisfirePolicy::BoundedCatchUp { maximum: 0 };
    let mut excessive_catch_up = config.schedules["bad schedule"].clone();
    excessive_catch_up.misfire = MisfirePolicy::BoundedCatchUp {
        maximum: crate::domain::scheduler::MAX_CATCH_UP_RUNS + 1,
    };
    config
        .schedules
        .insert("excessive-catch-up".to_owned(), excessive_catch_up);
    let document = ConfigDocument {
        config,
        root: PathBuf::from("/workspace"),
        path: PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    };
    let report = validate(&document, "local");
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for expected in [
        "MM_NAME_INVALID",
        "MM_JOB_SERVICE_UNKNOWN",
        "MM_JOB_COMMAND_INVALID",
        "MM_JOB_COMMAND_BOUNDS",
        "MM_JOB_TIMEOUT_INVALID",
        "MM_JOB_RETRY_INVALID",
        "MM_JOB_OUTPUT_LIMIT_INVALID",
        "MM_JOB_HISTORY_LIMIT_INVALID",
        "MM_JOB_ENVIRONMENT_UNKNOWN",
        "MM_SCHEDULE_JOB_UNKNOWN",
        "MM_SCHEDULE_CRON_INVALID",
        "MM_SCHEDULE_BOUNDS_INVALID",
        "MM_SCHEDULE_JITTER_INVALID",
        "MM_SCHEDULE_MISFIRE_INVALID",
        "MM_SCHEDULE_TIMEOUT_INVALID",
        "MM_GATE_PROVIDER_INVALID",
        "MM_GATE_INGRESS_INVALID",
    ] {
        assert!(
            codes.contains(expected),
            "missing diagnostic {expected}: {codes:?}"
        );
    }
    Ok(())
}
