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
    written: String,
    path: PathBuf,
    read_only: bool,
    propagation: Option<String>,
    relabels: bool,
}

impl HostBind {
    /// Why no host exception can allow the mount, as the change to make.
    fn refusal(&self) -> Option<&'static str> {
        if !self.read_only {
            Some("writable; add `:ro`, or `read_only: true` in the long syntax")
        } else if matches!(self.propagation.as_deref(), Some("shared" | "rshared")) {
            Some("with shared propagation; use `rslave`, or leave propagation out")
        } else if self.relabels {
            Some("with an SELinux relabel; remove `z` or `Z`")
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
    let (source, written, read_only, propagation, relabels) = match volume {
        Value::String(text) => {
            let mut parts = text.splitn(3, ':');
            let source = parts.next()?;
            parts.next()?;
            let options = parts
                .next()
                .unwrap_or_default()
                .split(',')
                .collect::<Vec<_>>();
            (
                source.to_owned(),
                text.clone(),
                options.contains(&"ro"),
                options
                    .iter()
                    .find(|option| {
                        option.ends_with("shared")
                            || option.ends_with("slave")
                            || option.ends_with("private")
                    })
                    .map(|option| (*option).to_owned()),
                options.iter().any(|option| matches!(*option, "z" | "Z")),
            )
        }
        Value::Mapping(long) => long_bind(long)?,
        _ => return None,
    };
    if !is_bind_source(&source) || source.starts_with('~') || interpolates(&source) {
        return None;
    }
    let absolute = if Path::new(&source).is_absolute() {
        PathBuf::from(&source)
    } else {
        compose_directory.join(&source)
    };
    let path = normalize_absolute_path(&absolute)?;
    if path.starts_with(project_root) {
        return None;
    }
    Some(HostBind {
        written,
        path,
        read_only,
        propagation,
        relabels,
    })
}

type Parsed = (String, String, bool, Option<String>, bool);

fn long_bind(long: &Mapping) -> Option<Parsed> {
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
    let propagation = text(bind.and_then(|bind| bind.get("propagation")));
    let relabels = text(bind.and_then(|bind| bind.get("selinux")))
        .is_some_and(|label| matches!(label.as_str(), "z" | "Z"));
    Some((
        source.clone(),
        format!("{source}:{target}"),
        read_only,
        propagation,
        relabels,
    ))
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
                bind.written
            ),
            None => format!(
                "volumes: {} in {file} mounts the host's {path} read-only",
                bind.written
            ),
        };
        findings.push(GuardrailFinding {
            subject: subject.to_owned(),
            feature: HOST_PATH.to_owned(),
            detail,
            outright: !bind.runs_when_listed(),
            host_path: Some(bind.path),
        });
    }
}
