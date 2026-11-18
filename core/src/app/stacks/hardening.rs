//! The security options of a stack's services: the defaults of
//! `hardening: strict`, each service's exceptions, and the OCI runtime the
//! host allows or asks for.
//!
//! mm adds what is missing in its overlay, which Compose merges last: a
//! scalar there wins (`read_only`, `runtime`) and a list is added to the
//! service's own (`cap_drop`, `security_opt`, `tmpfs`). What the Compose
//! files already say is read first, so nothing is added twice and a plan
//! shows what the container really runs with.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_yaml_ng::{Mapping, Value};

use super::guardrails::strings;
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::hardening::{
    Hardening, HardeningException, HardeningExceptions, HardeningKey, HostRuntimes, ServiceSecurity,
};
use crate::domain::stack::CompiledComposeSource;
use crate::domain::target::{NamedTarget, TargetTransport};

/// Where `hardening: strict` puts a tmpfs.
const TMPFS: &str = "/tmp";
const NO_NEW_PRIVILEGES: &str = "no-new-privileges:true";
/// The Compose key that names a service's OCI runtime, however it is quoted.
const RUNTIME_KEY: &[u8] = b"runtime";

/// What mm's overlay adds to one service.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(super) struct GeneratedSecurity {
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_only: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cap_drop: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    security_opt: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tmpfs: Vec<&'static str>,
}

impl GeneratedSecurity {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The security of one stack: what the overlay adds to each service, and
/// what the plan shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct StackSecurity {
    pub overlay: BTreeMap<String, GeneratedSecurity>,
    pub services: Vec<ServiceSecurity>,
}

/// Whether `hardening:` applies to `environment` on `target`. It applies
/// to every deployment and not to the development environment on this
/// machine, where `mm dev` mounts the code and tools install and write
/// where they like.
pub(crate) fn hardening_applies(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
) -> bool {
    let development = document.config.project.development_environment.as_deref()
        == Some(environment)
        && target.definition.transport == TargetTransport::Local;
    document.config.hardening == Some(Hardening::Strict) && !development
}

/// What one service's Compose files say, later files over earlier ones the
/// way Compose merges them.
#[derive(Debug, Default)]
struct Declared {
    runtime: Option<String>,
    read_only: bool,
    privileged: bool,
    cap_drop: BTreeSet<String>,
    cap_add: BTreeSet<String>,
    security_opt: Vec<String>,
    /// Where the service has a tmpfs.
    tmpfs: Vec<String>,
    /// Where the service mounts anything.
    mounts: BTreeSet<String>,
    publishes: bool,
}

fn capability(name: &str) -> String {
    let upper = name.trim().to_ascii_uppercase();
    upper.strip_prefix("CAP_").unwrap_or(&upper).to_owned()
}

/// The container path of one `volumes:` entry.
fn mount_target(entry: &Value) -> Option<String> {
    match entry {
        Value::String(short) => {
            let mut parts = short.split(':');
            let first = parts.next()?;
            Some(parts.next().unwrap_or(first).to_owned())
        }
        Value::Mapping(long) => long
            .get("target")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

impl Declared {
    fn merge(&mut self, service: &Mapping) {
        if let Some(runtime) = service.get("runtime").and_then(Value::as_str) {
            self.runtime = Some(runtime.to_owned());
        }
        if let Some(read_only) = service.get("read_only").and_then(Value::as_bool) {
            self.read_only = read_only;
        }
        if let Some(privileged) = service.get("privileged").and_then(Value::as_bool) {
            self.privileged = privileged;
        }
        self.cap_drop.extend(
            strings(service.get("cap_drop"))
                .iter()
                .map(|name| capability(name)),
        );
        self.cap_add.extend(
            strings(service.get("cap_add"))
                .iter()
                .map(|name| capability(name)),
        );
        for option in strings(service.get("security_opt")) {
            if !self.security_opt.contains(&option) {
                self.security_opt.push(option);
            }
        }
        for entry in strings(service.get("tmpfs")) {
            let path = entry.split(':').next().unwrap_or(&entry).to_owned();
            if !self.tmpfs.contains(&path) {
                self.tmpfs.push(path.clone());
            }
            self.mounts.insert(path);
        }
        if let Some(volumes) = service.get("volumes").and_then(Value::as_sequence) {
            for volume in volumes {
                let Some(target) = mount_target(volume) else {
                    continue;
                };
                if volume.get("type").and_then(Value::as_str) == Some("tmpfs")
                    && !self.tmpfs.contains(&target)
                {
                    self.tmpfs.push(target.clone());
                }
                self.mounts.insert(target);
            }
        }
        self.publishes |= service
            .get("ports")
            .and_then(Value::as_sequence)
            .is_some_and(|ports| !ports.is_empty())
            || service.get("network_mode").and_then(Value::as_str) == Some("host");
    }

    /// The `security_opt` entries in one spelling: `key:value`, lower case.
    fn options(&self) -> impl Iterator<Item = (String, &str)> {
        self.security_opt.iter().map(|option| {
            (
                option.replace([' ', '='], ":").to_ascii_lowercase(),
                option.as_str(),
            )
        })
    }

    fn no_new_privileges(&self) -> Option<bool> {
        self.options()
            .filter_map(|(compact, _)| match compact.as_str() {
                "no-new-privileges" | "no-new-privileges:true" => Some(true),
                "no-new-privileges:false" => Some(false),
                _ => None,
            })
            .last()
    }

    /// The seccomp profile the Compose files name, as written.
    fn seccomp(&self) -> Option<&str> {
        self.options()
            .filter(|(compact, _)| compact.starts_with("seccomp:"))
            .map(|(_, written)| written)
            .last()
    }

    /// The capabilities the container keeps by the Compose files alone.
    fn capabilities(&self) -> Option<Vec<String>> {
        (!self.privileged && self.cap_drop.contains("ALL"))
            .then(|| self.cap_add.iter().cloned().collect())
    }
}

/// Every service of the stack's Compose files with what they declare.
fn declared(sources: &[CompiledComposeSource]) -> Result<BTreeMap<String, Declared>, MmError> {
    let mut services = BTreeMap::<String, Declared>::new();
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        let Some(listed) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in listed {
            let (Some(name), Some(service)) = (name.as_str(), service.as_mapping()) else {
                continue;
            };
            services.entry(name.to_owned()).or_default().merge(service);
        }
    }
    Ok(services)
}

/// The exceptions `service` of `stack` declares in `mm.yaml`.
fn exceptions<'a>(
    document: &'a ConfigDocument,
    stack: &str,
    service: &str,
) -> Option<&'a HardeningExceptions> {
    document
        .config
        .modules
        .values()
        .flat_map(|module| &module.services)
        .find(|(name, config)| name.as_str() == service && config.stack == stack)
        .and_then(|(_, config)| config.hardening.as_ref())
        .map(|hardening| &hardening.except)
}

fn refused(message: String) -> MmError {
    MmError::Security { message }
}

/// How a service leaves a default out, for a refusal.
fn except_hint(key: HardeningKey) -> String {
    format!(
        "say why under the service in mm.yaml: `hardening: {{except: {{{}: \"<reason>\"}}}}`",
        key.as_str()
    )
}

/// The runtime `subject` runs under on a host that says `host` about
/// runtimes, and whether the host chose it.
fn runtime_on(
    host: &HostRuntimes,
    subject: &str,
    declared: Option<&str>,
    exposed: bool,
) -> Result<(Option<String>, bool), MmError> {
    if let Some(runtime) = declared
        && !host.advertised.contains(runtime)
    {
        return Err(refused(format!(
            "service '{subject}' sets `runtime: {runtime}`, which this host does not offer. It offers {}. Use one of those or remove `runtime:` from the service; to offer {runtime}, the host's admin installs it and adds it to `runtimes:` in /etc/mm/host.yaml",
            host.offered()
        )));
    }
    match (&host.exposed, declared) {
        (Some(required), Some(runtime)) if exposed && runtime != required => Err(refused(format!(
            "service '{subject}' sets `runtime: {runtime}`, but it publishes a port or takes ingress traffic, and this host runs such services under {required} (`exposed-runtime` in /etc/mm/host.yaml). Remove `runtime:` from the service or set it to {required}"
        ))),
        (Some(required), None) if exposed => Ok((Some(required.clone()), true)),
        _ => Ok((declared.map(str::to_owned), false)),
    }
}

/// What `hardening: strict` adds to one service, and the options it then
/// runs with.
fn harden(
    subject: &str,
    declared: &Declared,
    except: &HardeningExceptions,
    security: &mut ServiceSecurity,
) -> Result<GeneratedSecurity, MmError> {
    let excepted = |key| except.reason(key).is_some();
    let mut added = GeneratedSecurity::default();
    security.hardened = true;
    security.exceptions = except
        .entries()
        .map(|(key, reason)| HardeningException {
            key,
            reason: reason.trim().to_owned(),
        })
        .collect();
    if declared.privileged && !(excepted(HardeningKey::CapDrop) && excepted(HardeningKey::Seccomp))
    {
        return Err(refused(format!(
            "service '{subject}' sets `privileged: true`, which gives it every capability and no seccomp profile, and the project sets `hardening: strict`. Remove `privileged`, or {} and the same for `seccomp`",
            except_hint(HardeningKey::CapDrop)
        )));
    }
    security.privileged = declared.privileged;
    if !excepted(HardeningKey::ReadOnly) {
        added.read_only = Some(true);
        security.read_only = true;
    }
    if !excepted(HardeningKey::CapDrop) {
        if !declared.cap_drop.contains("ALL") {
            added.cap_drop.push("ALL");
        }
        security.capabilities = Some(declared.cap_add.iter().cloned().collect());
    }
    if !excepted(HardeningKey::NoNewPrivileges) {
        match declared.no_new_privileges() {
            Some(true) => {}
            Some(false) => {
                return Err(refused(format!(
                    "service '{subject}' sets `security_opt: no-new-privileges:false` and the project sets `hardening: strict`. Remove the option, or {}",
                    except_hint(HardeningKey::NoNewPrivileges)
                )));
            }
            None => added.security_opt.push(NO_NEW_PRIVILEGES),
        }
        security.no_new_privileges = true;
    }
    if let (Some(profile), false) = (declared.seccomp(), excepted(HardeningKey::Seccomp)) {
        return Err(refused(format!(
            "service '{subject}' sets `security_opt: {profile}` and the project sets `hardening: strict`, which keeps the engine's default seccomp profile. Remove the option, or {}",
            except_hint(HardeningKey::Seccomp)
        )));
    }
    if !excepted(HardeningKey::Tmpfs) && !declared.mounts.contains(TMPFS) {
        added.tmpfs.push(TMPFS);
        security.tmpfs.push(TMPFS.to_owned());
    }
    Ok(added)
}

/// The security options of the services `sources` define. `routed` are the
/// services that take ingress traffic.
pub(super) fn stack_security(
    document: &ConfigDocument,
    environment: &str,
    target: &NamedTarget,
    stack: &str,
    sources: &[CompiledComposeSource],
    routed: &BTreeSet<&str>,
) -> Result<StackSecurity, MmError> {
    let hardened = hardening_applies(document, environment, target);
    let host = document.config.host_runtimes.as_ref();
    let mut result = StackSecurity::default();
    if !hardened && host.is_none() {
        // Nothing to add and nothing to check: a runtime named in a Compose
        // file is still shown. Most projects name none, and their files are
        // not read a second time for it.
        let names_runtime = |source: &CompiledComposeSource| {
            source
                .bytes
                .windows(RUNTIME_KEY.len())
                .any(|window| window == RUNTIME_KEY)
        };
        if !sources.iter().any(names_runtime) {
            return Ok(result);
        }
        for (name, declared) in declared(sources)? {
            if let Some(runtime) = declared.runtime {
                result.services.push(ServiceSecurity {
                    service: name,
                    runtime: Some(runtime),
                    ..ServiceSecurity::default()
                });
            }
        }
        return Ok(result);
    }
    let none = HardeningExceptions::default();
    for (name, declared) in declared(sources)? {
        let subject = format!("{stack}/{name}");
        let exposed = declared.publishes || routed.contains(name.as_str());
        let (runtime, runtime_from_host) = match host {
            Some(host) => runtime_on(host, &subject, declared.runtime.as_deref(), exposed)?,
            None => (declared.runtime.clone(), false),
        };
        let mut security = ServiceSecurity {
            service: name.clone(),
            runtime,
            runtime_from_host,
            ..ServiceSecurity::default()
        };
        let mut added = GeneratedSecurity::default();
        if hardened {
            security.read_only = declared.read_only;
            security.capabilities = declared.capabilities();
            security.no_new_privileges = declared.no_new_privileges() == Some(true);
            security.seccomp = declared
                .seccomp()
                .map(|written| written.replace("seccomp=", "").replace("seccomp:", ""));
            security.tmpfs.clone_from(&declared.tmpfs);
            let except = exceptions(document, stack, &name).unwrap_or(&none);
            added = harden(&subject, &declared, except, &mut security)?;
        }
        if runtime_from_host {
            added.runtime.clone_from(&security.runtime);
        }
        if !hardened && security.runtime.is_none() {
            continue;
        }
        if !added.is_empty() {
            result.overlay.insert(name, added);
        }
        result.services.push(security);
    }
    Ok(result)
}
