//! Namespace keys: the host's own, the modes a host runs, and the ones a
//! service reaches by joining another service's.

use std::collections::{BTreeMap, BTreeSet};

use serde_yaml_ng::{Mapping, Value};

use super::{GuardrailFinding, push};

/// The feature no host exception covers: a namespace other than the
/// service's own, a service's of its stack, or the host's.
pub(super) const CONTAINER_JOIN: &str = "container-join";

const HOST_NAMESPACES: [(&str, &str); 6] = [
    ("pid", "pid-host"),
    ("ipc", "ipc-host"),
    ("uts", "uts-host"),
    ("userns_mode", "userns-host"),
    ("cgroup", "cgroup-host"),
    ("network_mode", "network-host"),
];

/// The modes each key may take besides `host`, `service:` where it joins,
/// and leaving it unset.
const PRIVATE_MODES: [(&str, &[&str]); 6] = [
    ("pid", &["private"]),
    ("ipc", &["none", "private", "shareable"]),
    ("uts", &["private"]),
    ("userns_mode", &["auto", "private"]),
    ("cgroup", &["private"]),
    ("network_mode", &["bridge", "none", "default", "private"]),
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
        let joinable = JOINABLE.iter().any(|(joins, _)| *joins == key);
        let private = PRIVATE_MODES
            .iter()
            .find(|(named, _)| *named == key)
            .map_or(&[][..], |(_, modes)| *modes);
        if mode == "host" {
            push(findings, subject, feature, format!("{key}: host"));
        } else if super::interpolates(mode) {
            super::push_variable(findings, (subject, CONTAINER_JOIN), (key, mode), file);
        } else if mode.is_empty()
            || private.contains(&mode)
            || (joinable && mode.starts_with("service:"))
        {
            continue;
        } else if joinable && mode.starts_with("container:") {
            push(
                findings,
                subject,
                CONTAINER_JOIN,
                format!(
                    "{key}: {mode} in {file} joins any container of that name on the host; write `{key}: service:<service>` to join a service of this stack"
                ),
            );
        } else {
            let join = if joinable {
                ", `service:<service>`"
            } else {
                ""
            };
            push(
                findings,
                subject,
                CONTAINER_JOIN,
                format!(
                    "{key}: {mode} in {file} is not a mode a host runs; write {}{join}, or leave `{key}` out",
                    private
                        .iter()
                        .map(|mode| format!("`{mode}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
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
            if mode == "host" || super::interpolates(mode) {
                return Some((service, mode));
            }
            if let Some(joined) = mode.strip_prefix("service:") {
                next.push(joined);
            }
        }
    }
    None
}
