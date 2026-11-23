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

/// The `security_opt` entries a host runs without an exception, written
/// `key:value` in lower case.
const SAFE_SECURITY_OPTIONS: [&str; 3] = [
    "no-new-privileges",
    "no-new-privileges:true",
    "seccomp:builtin",
];

/// One Compose feature that would reach the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailFinding {
    /// `<stack>/<service>`, or `<stack>` for project-level resources.
    pub subject: String,
    /// Stable feature name a host exception names, such as `pid-host`.
    pub feature: String,
    pub detail: String,
    host_path: Option<std::path::PathBuf>,
    outright: bool,
    /// The `name:` of a volume or network: a host decides it against the
    /// stack's Compose project name there.
    resource_name: Option<String>,
}

impl GuardrailFinding {
    /// Whether a host exception can allow this finding.
    #[must_use]
    pub fn grantable(&self) -> bool {
        !self.outright && self.feature != namespaces::CONTAINER_JOIN
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
    findings.retain(|finding| finding.resource_name.is_none() && !allows(allowed, finding));
    Ok(findings)
}

/// The `name:` of each volume or network in `environment` that is not
/// under its stack's Compose project name in `project_names`, which only
/// the host that plans knows, minus host exceptions. A stack missing from
/// `project_names` does not run there.
pub fn check_named_resources(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    project_names: &std::collections::BTreeMap<String, String>,
    allowed: &[GuardrailException],
) -> Result<Vec<GuardrailFinding>, crate::domain::error::MmError> {
    let mut findings = project_findings(document, environment)?;
    findings.retain(|finding| {
        let Some(name) = &finding.resource_name else {
            return false;
        };
        let stack = finding.subject.split('/').next().unwrap_or_default();
        project_names
            .get(stack)
            .is_some_and(|project| name != project && !name.starts_with(&format!("{project}_")))
            && !allows(allowed, finding)
    });
    for finding in &mut findings {
        let project = project_names
            .get(finding.subject.split('/').next().unwrap_or_default())
            .cloned()
            .unwrap_or_default();
        finding.detail = format!(
            "{} is outside this stack's Compose project `{project}`; leaving `name:` out makes Compose create a new, empty one, so to keep an existing volume's data a host admin allows {} for {}",
            finding.detail, finding.feature, finding.subject
        );
    }
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
        let compose_directory = super::compose_project_directory(document, stack);
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
                findings.extend(compose_findings(
                    stack,
                    file,
                    compose,
                    (&document.root, &compose_directory),
                ));
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

/// Whether a host exception covers `finding`: the feature for its service,
/// and for a host path, that path among the exception's `paths`.
fn allows(allowed: &[GuardrailException], finding: &GuardrailFinding) -> bool {
    finding.grantable()
        && allowed.iter().any(|exception| {
            exception.service == finding.subject
                && exception.features.contains(&finding.feature)
                && finding
                    .host_path
                    .as_deref()
                    .is_none_or(|path| host_paths::lists(&exception.paths, path))
        })
}

fn compose_findings(
    stack: &str,
    file: &str,
    compose: &Mapping,
    (project_root, compose_directory): (&Path, &Path),
) -> Vec<GuardrailFinding> {
    let mut findings = Vec::new();
    if let Some(services) = compose.get("services").and_then(Value::as_mapping) {
        for (name, service) in services {
            let (Some(name), Some(service)) = (name.as_str(), service.as_mapping()) else {
                continue;
            };
            let subject = format!("{stack}/{name}");
            check_service(&subject, file, service, &mut findings);
            check_options(&subject, file, service, &mut findings);
            host_paths::check_service(
                &subject,
                file,
                service,
                (project_root, compose_directory),
                &mut findings,
            );
            namespaces::check_service(&subject, file, service, &mut findings);
        }
    }
    check_project_resources(
        stack,
        file,
        compose,
        (project_root, compose_directory),
        &mut findings,
    );
    findings
}

fn push(findings: &mut Vec<GuardrailFinding>, subject: &str, feature: &str, detail: String) {
    findings.push(GuardrailFinding {
        subject: subject.to_owned(),
        feature: feature.to_owned(),
        detail,
        host_path: None,
        outright: false,
        resource_name: None,
    });
}

/// Whether Compose puts a variable into `text`: an unescaped `$`.
fn interpolates(text: &str) -> bool {
    text.replace("$$", "").contains('$')
}

/// A checked key set from a variable, refused as the feature it could turn
/// on.
fn push_variable(
    findings: &mut Vec<GuardrailFinding>,
    (subject, feature): (&str, &str),
    (key, text): (&str, &str),
    file: &str,
) {
    push(
        findings,
        subject,
        feature,
        format!("{key}: {text} in {file} comes from a variable; write the value itself"),
    );
}

/// A Compose boolean as it turns a feature on, or the text of the variable
/// that sets it.
fn turns_on(value: Option<&Value>) -> Result<bool, &str> {
    match value {
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(Value::String(text)) if interpolates(text) => Err(text),
        Some(Value::String(text)) => Ok(!matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "false" | "no" | "off" | "0" | ""
        )),
        _ => Ok(false),
    }
}

fn check_service(
    subject: &str,
    file: &str,
    service: &Mapping,
    findings: &mut Vec<GuardrailFinding>,
) {
    match turns_on(service.get("privileged")) {
        Ok(true) => push(
            findings,
            subject,
            "privileged",
            format!(
                "privileged: {}",
                service
                    .get("privileged")
                    .map(value_text)
                    .unwrap_or_default()
            ),
        ),
        Err(text) => push_variable(
            findings,
            (subject, "privileged"),
            ("privileged", text),
            file,
        ),
        Ok(false) => {}
    }
    let defaults = DEFAULT_CAPABILITIES.into_iter().collect::<BTreeSet<_>>();
    for capability in strings(service.get("cap_add")) {
        let upper = capability.trim().to_ascii_uppercase();
        let normalized = upper.trim_start_matches("CAP_");
        if interpolates(&capability) {
            push_variable(
                findings,
                (subject, "cap-add"),
                ("cap_add", &capability),
                file,
            );
        } else if normalized == "ALL" || !defaults.contains(normalized) {
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
    if service.get("build").is_some() {
        push(
            findings,
            subject,
            "build",
            "remote hosts never build; publish images from CI".to_owned(),
        );
    }
}

fn check_options(
    subject: &str,
    file: &str,
    service: &Mapping,
    findings: &mut Vec<GuardrailFinding>,
) {
    for option in strings(service.get("security_opt")) {
        let compact = option
            .replace(' ', "")
            .replace('=', ":")
            .to_ascii_lowercase();
        if interpolates(&option) {
            push_variable(
                findings,
                (subject, "security-opt"),
                ("security_opt", &option),
                file,
            );
        } else if !SAFE_SECURITY_OPTIONS.contains(&compact.as_str()) {
            push(
                findings,
                subject,
                "security-opt",
                format!(
                    "security_opt: {option} in {file}; a host runs only {}",
                    SAFE_SECURITY_OPTIONS.join(", ")
                ),
            );
        }
    }
    if let Some(sysctls) = service.get("sysctls") {
        for name in sysctl_names(sysctls) {
            if interpolates(&name) {
                push_variable(findings, (subject, "sysctls"), ("sysctls", &name), file);
            } else if !name.starts_with("net.") {
                push(findings, subject, "sysctls", format!("sysctl {name}"));
            }
        }
    }
}

/// `external:` of a top-level volume or network.
fn check_external(
    findings: &mut Vec<GuardrailFinding>,
    (subject, feature): (&str, &str),
    entry: &Value,
    file: &str,
) {
    let external = entry.get("external");
    if external.is_some_and(Value::is_mapping) {
        push(findings, subject, feature, "external is set".to_owned());
        return;
    }
    match turns_on(external) {
        Ok(true) => push(findings, subject, feature, "external: true".to_owned()),
        Err(text) => push_variable(findings, (subject, feature), ("external", text), file),
        Ok(false) => {
            if let Some(name) = entry.get("name") {
                let name = value_text(name);
                if interpolates(&name) {
                    push_variable(findings, (subject, feature), ("name", &name), file);
                } else {
                    findings.push(GuardrailFinding {
                        subject: subject.to_owned(),
                        feature: feature.to_owned(),
                        detail: format!("name: {name} in {file}"),
                        host_path: None,
                        outright: false,
                        resource_name: Some(name),
                    });
                }
            }
        }
    }
}

fn check_driver_options(
    findings: &mut Vec<GuardrailFinding>,
    subject: &str,
    options: &Mapping,
    file: &str,
) {
    let variable = options.iter().find(|(key, value)| {
        matches!(key.as_str(), Some("o" | "type")) && interpolates(&value_text(value))
    });
    if let Some((key, value)) = variable {
        let key = format!("driver_opts.{}", key.as_str().unwrap_or_default());
        push_variable(
            findings,
            (subject, "volume-bind"),
            (&key, &value_text(value)),
            file,
        );
        return;
    }
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
            subject,
            "volume-bind",
            "driver_opts bind or device mount".to_owned(),
        );
    }
}

fn check_project_resources(
    stack: &str,
    file: &str,
    compose: &Mapping,
    (project_root, compose_directory): (&Path, &Path),
    findings: &mut Vec<GuardrailFinding>,
) {
    if let Some(volumes) = compose.get("volumes").and_then(Value::as_mapping) {
        for (name, volume) in volumes {
            let name = name.as_str().unwrap_or_default();
            let subject = format!("{stack}/volume:{name}");
            check_external(findings, (&subject, "external-volume"), volume, file);
            if let Some(options) = volume.get("driver_opts").and_then(Value::as_mapping) {
                check_driver_options(findings, &subject, options, file);
            }
        }
    }
    if let Some(networks) = compose.get("networks").and_then(Value::as_mapping) {
        for (name, network) in networks {
            let name = name.as_str().unwrap_or_default();
            let subject = format!("{stack}/network:{name}");
            check_external(findings, (&subject, "external-network"), network, file);
        }
    }
    for key in ["configs", "secrets"] {
        let Some(entries) = compose.get(key).and_then(Value::as_mapping) else {
            continue;
        };
        for (name, entry) in entries {
            let Some(source) = entry.get("file").and_then(Value::as_str) else {
                continue;
            };
            let name = name.as_str().unwrap_or_default();
            let subject = format!("{stack}/{key}:{name}");
            if interpolates(source) {
                push_variable(
                    findings,
                    (&subject, "file-outside-project"),
                    (&format!("{key}.{name}.file"), source),
                    file,
                );
                continue;
            }
            let escapes = super::source_policy::unsafe_volume_source(
                source,
                true,
                project_root,
                compose_directory,
            )
            .unwrap_or(true);
            if escapes {
                push(
                    findings,
                    &subject,
                    "file-outside-project",
                    format!(
                        "{key}.{name}.file: {source} in {file} is outside the project or behind a symlink; keep the file itself in the project"
                    ),
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

pub(super) mod host_paths;
mod namespaces;
mod review;
pub(super) use review::on_local_target;
pub use review::remote_guardrail_diagnostics;

#[cfg(test)]
#[path = "guardrails/tests.rs"]
mod tests;
