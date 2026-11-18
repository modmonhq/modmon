use std::collections::BTreeMap;

use crate::domain::config::RuntimeConfig;
use crate::domain::repo_target::RepoTargetConfig;

fn error_of<T: serde::de::DeserializeOwned>(yaml: &str) -> String {
    serde_yaml_ng::from_str::<T>(yaml)
        .map(drop)
        .map_err(|error| error.to_string())
        .err()
        .unwrap_or_default()
}

/// The message says the key or value is gone and what to write instead,
/// where serde alone would only call it unknown.
fn explains(error: &str, replacement: &str) {
    assert!(error.contains("was removed"), "{error}");
    assert!(error.contains(replacement), "{error}");
    assert!(
        !error.contains("unknown field") && !error.contains("unknown variant"),
        "{error}"
    );
}

#[test]
fn every_removed_form_names_its_replacement() {
    explains(
        &error_of::<RuntimeConfig>("driver: docker-swarm\n"),
        "docker-compose",
    );
    explains(
        &error_of::<BTreeMap<String, RepoTargetConfig>>(
            "production:\n  nodes: [{name: app-1, host: 203.0.113.11}]\n  swarm: {storage: app-1}\n",
        ),
        "cluster:",
    );
    explains(
        &error_of::<BTreeMap<String, RepoTargetConfig>>(
            "production:\n  host: a.example.test\n  members: [{host: b.example.test}]\n",
        ),
        "nodes:",
    );
}
