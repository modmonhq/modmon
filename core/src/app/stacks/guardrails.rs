//! Workload guardrails for remote targets. Trusting the repo means trusting
//! it to say what runs inside the limits the host sets; these Compose
//! features would escape those limits to the host, so a remote target refuses
//! them unless its host policy allows the feature for that service.

use std::collections::BTreeSet;
use std::path::Path;

use serde_yaml_ng::{Mapping, Value};

use crate::domain::host_policy::GuardrailException;

/// Capabilities Docker grants by default; adding any other is an escape.
const DEFAULT_CAPABILITIES: [&str; 14] = [
    "AUDIT_WRITE",
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "KILL",
    "MKNOD",
    "NET_BIND_SERVICE",
    "NET_RAW",
    "SETFCAP",
    "SETGID",
    "SETPCAP",
    "SETUID",
    "SYS_CHROOT",
];

/// One Compose feature that would reach the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailFinding {
    /// `<stack>/<service>`, or `<stack>` for project-level resources.
    pub subject: String,
    /// Stable feature name a host exception names, such as `pid-host`.
    pub feature: String,
    pub detail: String,
}

impl GuardrailFinding {
    /// Whether a host exception can allow this finding.
    #[must_use]
    pub fn grantable(&self) -> bool {
        self.feature != namespaces::CONTAINER_JOIN
    }
}

impl std::fmt::Display for GuardrailFinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} uses {} ({})",
            self.subject, self.feature, self.detail
        )
    }
}

/// Findings for the services a project checkout runs in `environment`,
/// minus host exceptions. Each Compose file is checked on its own, as
/// merging files only adds, and `service:` joins across all of them.
pub fn check_project(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    allowed: &[GuardrailException],
) -> Result<Vec<GuardrailFinding>, crate::domain::error::MmError> {
    let mut findings = project_findings(document, environment)?;
    findings.retain(|finding| !allows(allowed, finding));
    Ok(findings)
}

/// Every finding for the services active in `environment`, allowed or not.
/// A service the environment leaves out, and a stack left with no service,
/// contribute nothing.
fn project_findings(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
) -> Result<Vec<GuardrailFinding>, crate::domain::error::MmError> {
    let excluded = super::service_filter::environment_excluded_services(document, environment);
    let mut findings = Vec::new();
    for (stack, config) in &document.config.stacks {
        let mut sources = Vec::with_capacity(config.files.len());
        for file in &config.files {
            let mut compose = read_compose(&document.root.join(file))?;
            if let Some(services) = compose.get_mut("services").and_then(Value::as_mapping_mut) {
                for left_out in excluded.iter().filter(|service| &service.stack == stack) {
                    services.remove(left_out.service.as_str());
                }
            }
            sources.push((file.display().to_string(), compose));
        }
        let runs_a_service = sources.iter().any(|(_, compose)| {
            compose
                .get("services")
                .and_then(Value::as_mapping)
                .is_some_and(|services| !services.is_empty())
        });
        if runs_a_service {
            for (file, compose) in &sources {
                findings.extend(compose_findings(stack, file, compose, &document.root));
            }
            namespaces::check_joins(stack, &sources, &mut findings);
        }
    }
    Ok(findings)
}

fn read_compose(path: &Path) -> Result<Mapping, crate::domain::error::MmError> {
    use crate::domain::error::MmError;
    let bytes =
        crate::domain::path_policy::read_regular_file(path, 4 * 1024 * 1024)?.ok_or_else(|| {
            MmError::ConfigRead {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Compose file is missing from the commit",
                ),
            }
        })?;
    let parse = |message: String| MmError::ConfigParse {
        path: path.to_path_buf(),
        message,
    };
    let mut value: Value =
        serde_yaml_ng::from_slice(&bytes).map_err(|error| parse(error.to_string()))?;
    value
        .apply_merge()
        .map_err(|error| parse(error.to_string()))?;
    let mut compose: Mapping =
        serde_yaml_ng::from_value(value).map_err(|error| parse(error.to_string()))?;
    crate::domain::compose_extends::resolve_extends(&mut compose);
    Ok(compose)
}

/// Whether a host exception covers `finding`.
fn allows(allowed: &[GuardrailException], finding: &GuardrailFinding) -> bool {
    finding.grantable()
        && allowed.iter().any(|exception| {
            exception.service == finding.subject && exception.features.contains(&finding.feature)
        })
}

fn compose_findings(
    stack: &str,
    file: &str,
    compose: &Mapping,
    project_root: &Path,
) -> Vec<GuardrailFinding> {
    let mut findings = Vec::new();
    if let Some(services) = compose.get("services").and_then(Value::as_mapping) {
        for (name, service) in services {
            let (Some(name), Some(service)) = (name.as_str(), service.as_mapping()) else {
                continue;
            };
            let subject = format!("{stack}/{name}");
            check_service(&subject, service, &mut findings);
            namespaces::check_service(&subject, file, service, &mut findings);
        }
    }
    check_project_resources(stack, compose, project_root, &mut findings);
    findings
}

fn push(findings: &mut Vec<GuardrailFinding>, subject: &str, feature: &str, detail: String) {
    findings.push(GuardrailFinding {
        subject: subject.to_owned(),
        feature: feature.to_owned(),
        detail,
    });
}

fn check_service(subject: &str, service: &Mapping, findings: &mut Vec<GuardrailFinding>) {
    if service.get("privileged").and_then(Value::as_bool) == Some(true) {
        push(
            findings,
            subject,
            "privileged",
            "privileged: true".to_owned(),
        );
    }
    let defaults = DEFAULT_CAPABILITIES.into_iter().collect::<BTreeSet<_>>();
    for capability in strings(service.get("cap_add")) {
        let normalized = capability.trim_start_matches("CAP_").to_ascii_uppercase();
        if normalized == "ALL" || !defaults.contains(normalized.as_str()) {
            push(
                findings,
                subject,
                "cap-add",
                format!("cap_add: {capability}"),
            );
        }
    }
    for key in ["devices", "device_cgroup_rules", "gpus"] {
        if service.get(key).is_some_and(|value| !is_empty(value)) {
            push(findings, subject, "devices", format!("{key} is set"));
        }
    }
    if service
        .get("deploy")
        .and_then(|deploy| deploy.get("resources"))
        .and_then(|resources| resources.get("reservations"))
        .and_then(|reservations| reservations.get("devices"))
        .is_some_and(|value| !is_empty(value))
    {
        push(
            findings,
            subject,
            "devices",
            "deploy.resources.reservations.devices is set".to_owned(),
        );
    }
    if service.get("cgroup_parent").is_some() {
        push(
            findings,
            subject,
            "cgroup-parent",
            "cgroup_parent is set".to_owned(),
        );
    }
    for option in strings(service.get("security_opt")) {
        let compact = option.replace([' ', '='], ":").to_ascii_lowercase();
        if [
            "seccomp:unconfined",
            "apparmor:unconfined",
            "label:disable",
            "systempaths:unconfined",
            "no-new-privileges:false",
        ]
        .iter()
        .any(|unsafe_option| compact == *unsafe_option)
        {
            push(
                findings,
                subject,
                "security-opt",
                format!("security_opt: {option}"),
            );
        }
    }
    if let Some(sysctls) = service.get("sysctls") {
        for name in sysctl_names(sysctls) {
            if !name.starts_with("net.") {
                push(findings, subject, "sysctls", format!("sysctl {name}"));
            }
        }
    }
    if service.get("build").is_some() {
        push(
            findings,
            subject,
            "build",
            "remote hosts never build; publish images from CI".to_owned(),
        );
    }
}

fn check_project_resources(
    stack: &str,
    compose: &Mapping,
    project_root: &Path,
    findings: &mut Vec<GuardrailFinding>,
) {
    if let Some(volumes) = compose.get("volumes").and_then(Value::as_mapping) {
        for (name, volume) in volumes {
            let name = name.as_str().unwrap_or_default();
            let subject = format!("{stack}/volume:{name}");
            if volume.get("external").and_then(Value::as_bool) == Some(true) {
                push(
                    findings,
                    &subject,
                    "external-volume",
                    "external: true".to_owned(),
                );
            }
            if let Some(options) = volume.get("driver_opts").and_then(Value::as_mapping) {
                let binds = options.iter().any(|(key, value)| {
                    let key = key.as_str().unwrap_or_default();
                    let value = value_text(value);
                    key == "device"
                        || (key == "o"
                            && value
                                .split(',')
                                .any(|part| part.trim() == "bind" || part.trim() == "rbind"))
                        || (key == "type" && value == "none")
                });
                if binds {
                    push(
                        findings,
                        &subject,
                        "volume-bind",
                        "driver_opts bind or device mount".to_owned(),
                    );
                }
            }
        }
    }
    if let Some(networks) = compose.get("networks").and_then(Value::as_mapping) {
        for (name, network) in networks {
            if network.get("external").and_then(Value::as_bool) == Some(true) {
                let name = name.as_str().unwrap_or_default();
                push(
                    findings,
                    &format!("{stack}/network:{name}"),
                    "external-network",
                    "external: true".to_owned(),
                );
            }
        }
    }
    for key in ["configs", "secrets"] {
        let Some(entries) = compose.get(key).and_then(Value::as_mapping) else {
            continue;
        };
        for (name, entry) in entries {
            let Some(file) = entry.get("file").and_then(Value::as_str) else {
                continue;
            };
            let path = Path::new(file);
            let inside = path.is_relative()
                && !path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir));
            let absolute_inside = path.is_absolute() && path.starts_with(project_root);
            if !inside && !absolute_inside {
                let name = name.as_str().unwrap_or_default();
                push(
                    findings,
                    &format!("{stack}/{key}:{name}"),
                    "file-outside-project",
                    format!("{key}.{name}.file: {file}"),
                );
            }
        }
    }
}

pub(super) fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Sequence(items)) => items.iter().map(value_text).collect(),
        Some(value @ Value::String(_)) => vec![value_text(value)],
        _ => Vec::new(),
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

pub(super) fn sysctl_names(value: &Value) -> Vec<String> {
    match value {
        Value::Mapping(entries) => entries
            .keys()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Value::Sequence(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|entry| entry.split('=').next().unwrap_or(entry).to_owned())
            .collect(),
        _ => Vec::new(),
    }
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Sequence(items) => items.is_empty(),
        Value::Mapping(entries) => entries.is_empty(),
        Value::String(text) => text.is_empty(),
        _ => false,
    }
}

mod namespaces;
mod review;
pub(super) use review::on_local_target;
pub use review::remote_guardrail_diagnostics;

#[cfg(test)]
#[path = "guardrails/tests.rs"]
mod tests;
