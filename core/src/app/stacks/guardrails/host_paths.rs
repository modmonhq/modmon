//! Bind mounts of the host's own paths, outside the project: a host runs
//! one read-only when its policy lists the path for the service.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use super::{GuardrailFinding, interpolates};
use crate::app::stacks::source_policy::{is_bind_source, normalize_absolute_path};

pub(super) const HOST_PATH: &str = "host-path";

/// A bind mount whose source is a path of the host outside the project.
pub(in crate::app::stacks) struct HostBind {
    path: PathBuf,
    mount: Mount,
}

/// What a bind mount's entry says, as Compose applies it.
struct Mount {
    source: String,
    written: String,
    read_only: bool,
    recursive_read_only: bool,
    propagation: Option<String>,
    relabels: bool,
    /// What makes the entry's effect unclear or unsafe, as the change to
    /// make.
    problem: Option<String>,
}

impl HostBind {
    /// Why no host exception can allow the mount, as the change to make.
    fn refusal(&self) -> Option<String> {
        let mount = &self.mount;
        if let Some(problem) = &mount.problem {
            Some(problem.clone())
        } else if !mount.read_only {
            Some("writable; add `read_only: true`".to_owned())
        } else if !mount.recursive_read_only {
            Some(
                "without `bind.recursive: readonly`, so the mounts under it stay writable; add it"
                    .to_owned(),
            )
        } else if let Some(propagation) = mount
            .propagation
            .as_deref()
            .filter(|propagation| !matches!(*propagation, "private" | "rprivate"))
        {
            Some(format!(
                "with `{propagation}` propagation; leave propagation out"
            ))
        } else if mount.relabels {
            Some("with an SELinux relabel; remove `z` or `Z`".to_owned())
        } else {
            None
        }
    }

    /// Whether a host whose policy lists the path may run the mount.
    pub(in crate::app::stacks) fn runs_when_listed(&self) -> bool {
        self.refusal().is_none()
    }
}

/// `volume` of a service when it bind-mounts a host path outside
/// `project_root`. Sources the project's own policy refuses whatever the
/// host says (`~`, a variable, a path that climbs above `/`) are left to it.
pub(in crate::app::stacks) fn host_bind(
    volume: &Value,
    (project_root, compose_directory): (&Path, &Path),
) -> Option<HostBind> {
    let mount = match volume {
        Value::String(text) => short_bind(text)?,
        Value::Mapping(long) => long_bind(long)?,
        _ => return None,
    };
    let source = &mount.source;
    if !is_bind_source(source) || source.starts_with('~') || interpolates(source) {
        return None;
    }
    let absolute = if Path::new(source).is_absolute() {
        PathBuf::from(source)
    } else {
        compose_directory.join(source)
    };
    let path = normalize_absolute_path(&absolute)?;
    if path.starts_with(project_root) {
        return None;
    }
    Some(HostBind { path, mount })
}

fn short_bind(text: &str) -> Option<Mount> {
    let mut parts = text.splitn(3, ':');
    let source = parts.next()?;
    parts.next()?;
    Some(Mount {
        source: source.to_owned(),
        written: text.to_owned(),
        read_only: false,
        recursive_read_only: false,
        propagation: None,
        relabels: false,
        problem: Some(
            "in the short syntax, which cannot mount it recursively read-only; write it in the long syntax with `read_only: true` and `bind.recursive: readonly`"
                .to_owned(),
        ),
    })
}

fn long_bind(long: &Mapping) -> Option<Mount> {
    let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    if !text(long.get("type")).is_some_and(|kind| kind.eq_ignore_ascii_case("bind")) {
        return None;
    }
    let source = text(long.get("source"))?;
    let target = text(long.get("target")).unwrap_or_default();
    let read_only = match long.get("read_only") {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(flag)) => flag.eq_ignore_ascii_case("true"),
        _ => false,
    };
    let bind = long.get("bind");
    let recursive = bind
        .and_then(|bind| bind.get("recursive"))
        .map(super::value_text);
    let recursive_read_only = recursive.as_deref() == Some("readonly");
    Some(Mount {
        written: format!("{source}:{target}"),
        source,
        read_only,
        recursive_read_only,
        propagation: text(bind.and_then(|bind| bind.get("propagation"))),
        relabels: text(bind.and_then(|bind| bind.get("selinux")))
            .is_some_and(|label| matches!(label.as_str(), "z" | "Z")),
        problem: recursive
            .filter(|recursive| recursive != "readonly")
            .map(|recursive| format!("with `bind.recursive: {recursive}`; write `readonly`")),
    })
}

/// Whether `listed` names `path`, written with or without a trailing `/`.
pub(super) fn lists(listed: &BTreeSet<String>, path: &Path) -> bool {
    listed.iter().any(|entry| {
        normalize_absolute_path(Path::new(entry.trim())).is_some_and(|entry| entry == path)
    })
}

/// Each host path a service of `file` bind-mounts.
pub(super) fn check_service(
    subject: &str,
    file: &str,
    service: &Mapping,
    roots: (&Path, &Path),
    findings: &mut Vec<GuardrailFinding>,
) {
    let Some(volumes) = service.get("volumes").and_then(Value::as_sequence) else {
        return;
    };
    for bind in volumes.iter().filter_map(|volume| host_bind(volume, roots)) {
        let path = bind.path.display();
        let detail = match bind.refusal() {
            Some(change) => format!(
                "volumes: {} in {file} mounts the host's {path} {change}",
                bind.mount.written
            ),
            None => format!(
                "volumes: {} in {file} mounts the host's {path} read-only",
                bind.mount.written
            ),
        };
        findings.push(GuardrailFinding {
            subject: subject.to_owned(),
            feature: HOST_PATH.to_owned(),
            detail,
            outright: !bind.runs_when_listed(),
            resource_name: None,
            host_path: Some(bind.path),
        });
    }
}
