//! How long a Compose `up` or `down` may take, from the project's files.

use std::time::Duration;

use mm::domain::compose_timing::{
    COMPOSE_FLOOR, ComposeOrder, compose_duration, compose_limit, interpolate,
};
use serde_yaml_ng::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn models(files: &[&str]) -> Result<Vec<Value>, serde_yaml_ng::Error> {
    files
        .iter()
        .map(|file| serde_yaml_ng::from_str(file))
        .collect()
}

fn compose_wait(models: &[Value]) -> Duration {
    mm::domain::compose_timing::compose_wait(models, &|_| None, ComposeOrder::Dependencies)
}

fn seconds(value: u64) -> Duration {
    Duration::from_secs(value)
}

#[test]
fn compose_durations_read_as_compose_reads_them() {
    let read = |text: &str| compose_duration(&Value::String(text.to_owned()));
    assert_eq!(read("90s"), Some(seconds(90)));
    assert_eq!(read("1m30s"), Some(seconds(90)));
    assert_eq!(read("2h"), Some(seconds(7200)));
    assert_eq!(read("1.5m"), Some(seconds(90)));
    assert_eq!(read("500ms"), Some(Duration::from_millis(500)));
    assert_eq!(compose_duration(&Value::from(45_u64)), Some(seconds(45)));
    assert_eq!(read("${GRACE}"), None);
    assert_eq!(read("10"), None);
    assert_eq!(read("1d"), None);
    assert_eq!(read(""), None);
}

#[test]
fn services_that_stop_together_count_once_with_the_longest_grace() -> TestResult {
    let files = models(&[
        "services:\n  web:\n    image: web\n  db:\n    image: db\n    stop_grace_period: 3m\n  worker:\n    image: worker\n    stop_grace_period: 1m\n",
    ])?;
    assert_eq!(compose_wait(&files), seconds(180));
    assert_eq!(
        compose_limit(compose_wait(&files), COMPOSE_FLOOR),
        seconds(240)
    );
    Ok(())
}

#[test]
fn a_dependency_chain_stops_one_after_the_other() -> TestResult {
    let files = models(&[
        "services:\n  web:\n    image: web\n    stop_grace_period: 30s\n    depends_on: [api]\n  api:\n    image: api\n    stop_grace_period: 1m\n    depends_on:\n      db:\n        condition: service_started\n  db:\n    image: db\n    stop_grace_period: 2m\n",
    ])?;
    assert_eq!(compose_wait(&files), seconds(210));
    Ok(())
}

#[test]
fn a_dependency_awaited_healthy_adds_its_health_checks_longest_decision() -> TestResult {
    let files = models(&[
        "services:\n  api:\n    image: api\n    depends_on:\n      db:\n        condition: service_healthy\n  db:\n    image: db\n    stop_grace_period: 1m\n    healthcheck:\n      test: [CMD, ready]\n      interval: 5s\n      timeout: 2s\n      retries: 4\n      start_period: 1m\n",
    ])?;
    assert_eq!(compose_wait(&files), seconds(10 + 60 + 60 + 4 * 7));
    let defaults = models(&[
        "services:\n  api:\n    image: api\n    depends_on:\n      db:\n        condition: service_healthy\n  db:\n    image: db\n    healthcheck:\n      test: [CMD, ready]\n",
    ])?;
    assert_eq!(compose_wait(&defaults), seconds(10 + 10 + 3 * 60));
    let unawaited = models(&[
        "services:\n  db:\n    image: db\n    healthcheck:\n      test: [CMD, ready]\n      start_period: 10m\n",
    ])?;
    assert_eq!(compose_wait(&unawaited), seconds(10));
    Ok(())
}

#[test]
fn a_later_file_overrides_and_a_disabled_check_decides_at_once() -> TestResult {
    let files = models(&[
        "services:\n  api:\n    image: api\n    depends_on:\n      db:\n        condition: service_healthy\n  db:\n    image: db\n    stop_grace_period: 5m\n    healthcheck:\n      test: [CMD, ready]\n",
        "services:\n  db:\n    stop_grace_period: 20s\n    healthcheck:\n      disable: true\n",
    ])?;
    assert_eq!(compose_wait(&files), seconds(30));
    Ok(())
}

#[test]
fn a_small_project_keeps_the_floor_and_a_cycle_ends() -> TestResult {
    assert_eq!(
        compose_limit(compose_wait(&[]), COMPOSE_FLOOR),
        COMPOSE_FLOOR
    );
    let cycle = models(&[
        "services:\n  a:\n    image: a\n    depends_on: [b]\n  b:\n    image: b\n    depends_on: [a]\n",
    ])?;
    assert_eq!(compose_wait(&cycle), seconds(20));
    assert_eq!(
        compose_limit(compose_wait(&cycle), COMPOSE_FLOOR),
        COMPOSE_FLOOR
    );
    Ok(())
}

#[test]
fn variables_take_their_value_or_their_default() {
    let variables = |name: &str| match name {
        "GRACE" => Some("90s".to_owned()),
        "EMPTY" => Some(String::new()),
        _ => None,
    };
    let read = |text: &str| interpolate(text, &variables);
    assert_eq!(read("${GRACE}").as_deref(), Some("90s"));
    assert_eq!(read("$GRACE").as_deref(), Some("90s"));
    assert_eq!(read("${MISSING:-5m}").as_deref(), Some("5m"));
    assert_eq!(read("${EMPTY:-5m}").as_deref(), Some("5m"));
    assert_eq!(read("${EMPTY-5m}").as_deref(), Some(""));
    assert_eq!(read("${MISSING-${GRACE}}").as_deref(), Some("90s"));
    assert_eq!(read("${MISSING:-${ALSO:-2m}}").as_deref(), Some("2m"));
    assert_eq!(read("${GRACE:+1h}").as_deref(), Some("1h"));
    assert_eq!(read("${MISSING:+1h}").as_deref(), Some(""));
    assert_eq!(read("${MISSING:?set it}"), None);
    assert_eq!(read("$$GRACE").as_deref(), Some("$GRACE"));
    assert_eq!(read("${GRACE").as_deref(), None);
}

#[test]
fn a_grace_and_a_health_check_set_from_variables_count() -> TestResult {
    let files = models(&[
        "services:\n  api:\n    image: api\n    depends_on:\n      db:\n        condition: service_healthy\n  db:\n    image: db\n    stop_grace_period: ${DB_GRACE:-5m}\n    healthcheck:\n      test: [CMD, ready]\n      interval: ${INTERVAL:-5s}\n      timeout: 5s\n      retries: ${RETRIES:-2}\n      start_period: ${START}\n",
    ])?;
    assert_eq!(compose_wait(&files), seconds(10 + 300 + 2 * 10));
    let set = |name: &str| (name == "START").then(|| "1m".to_owned());
    assert_eq!(
        mm::domain::compose_timing::compose_wait(&files, &set, ComposeOrder::Dependencies),
        seconds(10 + 300 + 60 + 2 * 10)
    );
    Ok(())
}

#[test]
fn a_service_that_extends_another_of_its_file_inherits_its_grace_and_check() -> TestResult {
    let files = models(&[
        "x-base: &base\n  stop_grace_period: 1m\nservices:\n  base:\n    image: db\n    stop_grace_period: 4m\n    healthcheck:\n      test: [CMD, ready]\n      interval: 1s\n      timeout: 1s\n      retries: 1\n  db:\n    extends: {service: base}\n  api:\n    image: api\n    depends_on:\n      db:\n        condition: service_healthy\n  worker:\n    <<: *base\n    image: worker\n",
    ])?;
    assert_eq!(compose_wait(&files), seconds(10 + 240 + 2));
    Ok(())
}

#[test]
fn a_provider_that_takes_one_service_after_the_other_adds_them_all_up() -> TestResult {
    let files = models(&[
        "services:\n  web:\n    image: web\n  db:\n    image: db\n    stop_grace_period: 3m\n  worker:\n    image: worker\n    stop_grace_period: 1m\n",
    ])?;
    assert_eq!(
        mm::domain::compose_timing::compose_wait(&files, &|_| None, ComposeOrder::OneByOne),
        seconds(10 + 180 + 60)
    );
    Ok(())
}

#[test]
fn a_grace_too_large_to_add_saturates() -> TestResult {
    let files = models(&[
        "services:\n  db:\n    image: db\n    stop_grace_period: 18446744073709551615\n  api:\n    image: api\n    depends_on: [db]\n",
    ])?;
    let wait = compose_wait(&files);
    assert_eq!(compose_limit(wait, COMPOSE_FLOOR), Duration::MAX);
    assert_eq!(
        mm::domain::compose_timing::compose_wait(&files, &|_| None, ComposeOrder::OneByOne),
        Duration::MAX
    );
    Ok(())
}
