//! Namespace keys: the host's own, another container's by name, and the
//! ones a service reaches by joining another service's.

use std::collections::{BTreeMap, BTreeSet};

use serde_yaml_ng::{Mapping, Value};

use super::{GuardrailFinding, push};

/// The feature no host exception covers: joining a container by name.
pub(super) const CONTAINER_JOIN: &str = "container-join";

const HOST_NAMESPACES: [(&str, &str); 6] = [
    ("pid", "pid-host"),
    ("ipc", "ipc-host"),
    ("uts", "uts-host"),
    ("userns_mode", "userns-host"),
    ("cgroup", "cgroup-host"),
    ("network_mode", "network-host"),
];

const JOINABLE: [(&str, &str); 3] = [
    ("pid", "pid-host"),
    ("ipc", "ipc-host"),
    ("network_mode", "network-host"),
];

/// What one service of `file` sets on its own.
pub(super) fn check_service(
    subject: &str,
    file: &str,
    service: &Mapping,
    findings: &mut Vec<GuardrailFinding>,
) {
    for (key, feature) in HOST_NAMESPACES {
        let Some(mode) = service.get(key).and_then(Value::as_str) else {
            continue;
        };
        if mode == "host" {
            push(findings, subject, feature, format!("{key}: host"));
        } else if mode.contains('$') {
            push(
                findings,
                subject,
                feature,
                format!("{key}: {mode} in {file} comes from a variable; write the mode itself"),
            );
        } else if mode.starts_with("container:") {
            push(
                findings,
                subject,
                CONTAINER_JOIN,
                format!(
                    "{key}: {mode} in {file} joins any container of that name on the host; write `{key}: service:<service>` to join a service of this stack"
                ),
            );
        }
    }
}

/// Each `service:` join of `stack` that reaches a host namespace, under the
/// joining service: it needs the same exception as the service it joins.
pub(super) fn check_joins(
    stack: &str,
    sources: &[(String, Mapping)],
    findings: &mut Vec<GuardrailFinding>,
) {
    for (key, feature) in JOINABLE {
        let modes = written_modes(sources, key);
        for (service, written) in &modes {
            for (mode, file) in written {
                let Some(joined) = mode.strip_prefix("service:") else {
                    continue;
                };
                if let Some((holder, held)) = reaches_host(&modes, joined) {
                    push(
                        findings,
                        &format!("{stack}/{service}"),
                        feature,
                        format!(
                            "{key}: {mode} in {file}, which reaches `{key}: {held}` of {stack}/{holder}"
                        ),
                    );
                }
            }
        }
    }
}

type Modes = BTreeMap<String, BTreeMap<String, String>>;

/// Every value each service gives `key` in any of the files, with the file
/// that writes it.
fn written_modes(sources: &[(String, Mapping)], key: &str) -> Modes {
    let mut modes = Modes::new();
    for (file, compose) in sources {
        let Some(services) = compose.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in services {
            let (Some(name), Some(mode)) =
                (name.as_str(), service.get(key).and_then(Value::as_str))
            else {
                continue;
            };
            modes
                .entry(name.to_owned())
                .or_default()
                .entry(mode.to_owned())
                .or_insert_with(|| file.clone());
        }
    }
    modes
}

/// The first service, from `start` along `service:` joins, whose mode is
/// the host's or comes from a variable, with that mode.
fn reaches_host<'a>(modes: &'a Modes, start: &'a str) -> Option<(&'a str, &'a str)> {
    let mut seen = BTreeSet::new();
    let mut next = vec![start];
    while let Some(service) = next.pop() {
        if !seen.insert(service) {
            continue;
        }
        for mode in modes.get(service).into_iter().flat_map(BTreeMap::keys) {
            if mode == "host" || mode.contains('$') {
                return Some((service, mode));
            }
            if let Some(joined) = mode.strip_prefix("service:") {
                next.push(joined);
            }
        }
    }
    None
}
