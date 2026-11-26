//! While a container starts, Docker Engine 25 and later probe its health
//! check every `start_interval`: mm's overlay sets one second, and a start
//! period long enough for the service's own checks, so a deployment sees a
//! healthy container within a second instead of after the first `interval`.

use mm::domain::compose_healthcheck::{docker_probes_at_start, start_probes};
use serde_yaml_ng::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn models(files: &[&str]) -> Result<Vec<Value>, serde_yaml_ng::Error> {
    files
        .iter()
        .map(|file| serde_yaml_ng::from_str(file))
        .collect()
}

fn keys(
    probes: &std::collections::BTreeMap<String, serde_yaml_ng::Mapping>,
    service: &str,
) -> Vec<(String, String)> {
    probes
        .get(service)
        .map(|keys| {
            keys.iter()
                .map(|(key, value)| {
                    (
                        key.as_str().unwrap_or_default().to_owned(),
                        value.as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn only_docker_engine_25_and_later_probe_at_start() {
    for (version, probes) in [
        ("24.0.9", false),
        ("25.0.0", true),
        ("27.3.1", true),
        ("v28.1.0", true),
        ("Docker version 29.10.1, build 54cd494", true),
        ("Docker version 24.0.9, build 2936816", false),
        ("Docker version 25.0.0-rc.1, build 1234567", true),
        ("Docker version unknown, build 1234567", false),
        ("29.10.1", true),
        ("", false),
        ("unknown", false),
    ] {
        assert_eq!(docker_probes_at_start(version), probes, "{version}");
    }
}

#[test]
fn a_service_with_a_health_check_probes_every_second_while_it_starts() -> TestResult {
    let probes = start_probes(&models(&["services:
  api: {image: api, healthcheck: {test: [CMD, true]}}
  web: {image: web, healthcheck: {test: [CMD, true], interval: 10s, retries: 5}}
  slow: {image: slow, healthcheck: {test: [CMD, true], interval: 1m30s, retries: 2}}
  set: {image: set, healthcheck: {test: [CMD, true], start_period: 2m}}
  plain: {image: plain}
"])?);
    assert_eq!(
        keys(&probes, "api"),
        [
            ("start_period".to_owned(), "90s".to_owned()),
            ("start_interval".to_owned(), "1s".to_owned())
        ],
        "Docker's defaults: every 30s, three times"
    );
    assert_eq!(
        keys(&probes, "web")[0],
        ("start_period".to_owned(), "50s".to_owned())
    );
    assert_eq!(
        keys(&probes, "slow")[0],
        ("start_period".to_owned(), "180s".to_owned())
    );
    assert_eq!(
        keys(&probes, "set"),
        [("start_interval".to_owned(), "1s".to_owned())],
        "a start period the service sets stays"
    );
    assert!(
        !probes.contains_key("plain"),
        "no health check, nothing to add"
    );
    Ok(())
}

#[test]
fn a_check_that_is_off_or_already_quick_is_left_alone_and_files_merge() -> TestResult {
    let probes = start_probes(&models(&[
        "services:
  off: {image: off, healthcheck: {disable: true}}
  none: {image: none, healthcheck: {test: [NONE]}}
  own: {image: own, healthcheck: {test: [CMD, true], start_interval: 2s}}
  later: {image: later, healthcheck: {test: [CMD, true]}}
  odd: {image: odd, healthcheck: {test: [CMD, true], interval: soon}}
",
        "services:
  later: {healthcheck: {interval: 5s}}
  own: {image: own}
",
    ])?);
    for service in ["off", "none", "own", "odd"] {
        assert!(!probes.contains_key(service), "{service}");
    }
    assert_eq!(
        keys(&probes, "later")[0],
        ("start_period".to_owned(), "15s".to_owned()),
        "a later file's interval counts"
    );
    Ok(())
}
