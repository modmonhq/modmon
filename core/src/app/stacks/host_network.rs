//! Services in their host's network namespace (`network_mode: host`): on
//! this machine when `mm.yaml` says why (`host-network`), on a remote host
//! when the host's policy allows `network-host` for them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_yaml_ng::{Mapping, Value};

use super::source_policy::mapping_value;
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::stack::{CompiledComposeSource, HostNetworkUse};

/// What a stack's services may combine with `network_mode: host`.
#[derive(Debug, Default)]
pub(super) struct HostNetwork<'a> {
    stack: &'a str,
    host_decides: bool,
    services: BTreeMap<&'a str, Listed<'a>>,
    stack_network: Option<&'a str>,
}

#[derive(Debug)]
struct Listed<'a> {
    module: &'a str,
    reason: Option<&'a str>,
    /// A network of `mm.yaml` the module or the service joins, and the key.
    network: Option<(&'a str, String)>,
}

/// The services of a stack's Compose files on the host's network.
#[derive(Debug, Default)]
pub(super) struct HostNetworkServices {
    pub uses: Vec<HostNetworkUse>,
    /// Those whose Compose files set no `userns_mode`: the overlay keeps
    /// them in the host's user namespace.
    pub host_userns: BTreeSet<String>,
}

impl<'a> HostNetwork<'a> {
    /// `host_decides` is set where the host's policy decides rather than
    /// the project's `host-network`.
    pub(super) fn of(
        document: &'a ConfigDocument,
        environment: &str,
        stack: &'a str,
        host_decides: bool,
    ) -> Self {
        let mut services = BTreeMap::new();
        for (module_name, module) in &document.config.modules {
            for (name, service) in &module.services {
                if service.stack != stack || !service.is_active(environment) {
                    continue;
                }
                let network = service
                    .networks
                    .first()
                    .map(|network| {
                        let key = format!("modules.{module_name}.services.{name}.networks");
                        (network.as_str(), key)
                    })
                    .or_else(|| {
                        let key = format!("modules.{module_name}.networks");
                        module
                            .networks
                            .first()
                            .map(|network| (network.as_str(), key))
                    });
                let reason = service.host_network.as_deref();
                services.insert(
                    name.as_str(),
                    Listed {
                        module: module_name,
                        reason: reason.filter(|reason| !reason.trim().is_empty()),
                        network,
                    },
                );
            }
        }
        Self {
            stack,
            host_decides,
            services,
            stack_network: document
                .config
                .stacks
                .get(stack)
                .and_then(|stack| stack.networks.first())
                .map(String::as_str),
        }
    }

    /// Why the Compose file `path` may not give service `name` the
    /// `network_mode` it does. `written` is the service as the file writes
    /// it, before YAML merge keys; `routed` says whether it takes ingress
    /// traffic.
    pub(super) fn refusal(
        &self,
        path: &Path,
        name: &str,
        service: &Mapping,
        written: Option<&Mapping>,
        routed: bool,
    ) -> Option<String> {
        let mode = mapping_value(service, "network_mode")?;
        let file = path.display();
        let mode = mode.as_str().unwrap_or_default();
        if mode.contains('$') {
            return Some(format!(
                "Compose file '{file}' sets the network_mode of service '{name}' from a variable; write the mode itself, such as `network_mode: host`"
            ));
        }
        if !mode.eq_ignore_ascii_case("host") {
            return routed.then(|| format!(
                "Compose file '{file}' sets `network_mode` on service '{name}', which takes ingress traffic that mm sends over a network of its own; remove `network_mode` from the service"
            ));
        }
        let literal = written
            .and_then(|written| mapping_value(written, "network_mode"))
            .and_then(Value::as_str)
            == Some("host");
        if !literal {
            return Some(format!(
                "Compose file '{file}' puts service '{name}' on the host's network without writing `network_mode: host` on the service itself; write exactly that line under the service"
            ));
        }
        let listed = self.services.get(name);
        if !self.host_decides && listed.and_then(|listed| listed.reason).is_none() {
            let place = listed.map_or_else(
                || {
                    "list the service under a module of mm.yaml with `host-network: <why>`"
                        .to_owned()
                },
                |listed| {
                    format!(
                        "add `host-network: <why>` under modules.{}.services.{name} in mm.yaml",
                        listed.module
                    )
                },
            );
            return Some(format!(
                "Compose file '{file}' puts service '{name}' on the host's network (`network_mode: host`), which on this machine needs a reason: {place}"
            ));
        }
        self.conflict(path, name, service, routed, listed)
    }

    fn conflict(
        &self,
        path: &Path,
        name: &str,
        service: &Mapping,
        routed: bool,
        listed: Option<&Listed<'_>>,
    ) -> Option<String> {
        let file = path.display();
        if routed {
            return Some(format!(
                "Compose file '{file}' puts service '{name}' on the host's network, but an ingress route of stack '{}' selects it, and mm sends ingress traffic over a network of its own; remove the route, or `network_mode: host`",
                self.stack
            ));
        }
        for key in ["networks", "ports"] {
            if mapping_value(service, key).is_some() {
                return Some(format!(
                    "Compose file '{file}' gives service '{name}' both `network_mode: host` and `{key}`; on the host's network a container joins no other network and listens on the host's ports itself, so remove `{key}` from the service"
                ));
            }
        }
        let (network, key) = listed
            .and_then(|listed| listed.network.clone())
            .or_else(|| {
                self.stack_network
                    .map(|network| (network, format!("stacks.{}.networks", self.stack)))
            })?;
        Some(format!(
            "Compose file '{file}' puts service '{name}' on the host's network, but `{key}` in mm.yaml puts it on network '{network}' as well, and a container on the host's network joins no other; remove '{network}' from `{key}`, or move the service to a stack of its own"
        ))
    }

    /// The stack's services on the host's network, with the reasons
    /// `mm.yaml` gives.
    pub(super) fn services(
        &self,
        sources: &[CompiledComposeSource],
    ) -> Result<HostNetworkServices, MmError> {
        let mut found = BTreeMap::<String, (bool, bool)>::new();
        for source in sources {
            let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
                message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
            };
            let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
            model.apply_merge().map_err(invalid)?;
            if let Some(mapping) = model.as_mapping_mut() {
                crate::domain::compose_extends::resolve_extends(mapping);
            }
            let Some(listed) = model.get("services").and_then(Value::as_mapping) else {
                continue;
            };
            for (name, service) in listed {
                let Some(name) = name.as_str() else {
                    continue;
                };
                let entry = found.entry(name.to_owned()).or_default();
                entry.0 |= on_host_network(service);
                match super::user_namespace::userns_setting(service) {
                    super::user_namespace::UsernsSetting::Unset => {}
                    super::user_namespace::UsernsSetting::Reset => entry.1 = false,
                    super::user_namespace::UsernsSetting::Mode(_) => entry.1 = true,
                }
            }
        }
        let mut services = HostNetworkServices::default();
        for (name, (on_host, own_userns)) in found {
            if !on_host {
                continue;
            }
            services.uses.push(HostNetworkUse {
                reason: self
                    .services
                    .get(name.as_str())
                    .and_then(|listed| listed.reason)
                    .map(str::to_owned),
                service: name.clone(),
            });
            if !own_userns {
                services.host_userns.insert(name);
            }
        }
        Ok(services)
    }
}

/// Whether a Compose service runs in the host's network namespace.
pub(super) fn on_host_network(service: &Value) -> bool {
    service.get("network_mode").and_then(Value::as_str) == Some("host")
}
