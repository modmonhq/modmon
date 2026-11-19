//! The Compose source policy of one stack in one environment. The
//! compilation keeps a file only when it passes, and `mm config validate`
//! asks the same policy about every environment, so a file it accepts is
//! not refused later at `mm deploy` or `mm dev up`.

use crate::domain::diagnostic::{Diagnostic, ValidationReport};
use crate::domain::target::TargetTransport;

use super::*;

/// Where an environment's stacks run, as far as the policy asks.
#[derive(Debug, Clone, Copy)]
pub(super) struct SourceScope {
    automatic_local_ingress: bool,
    /// The host's policy decides about `network_mode: host`: on a remote
    /// target, and in `mmd`, which reads the configuration under it.
    host_decides: bool,
    proxy_ports: Option<(u16, u16, u16)>,
}

impl SourceScope {
    pub(super) fn compiling(
        document: &ConfigDocument,
        environment: &str,
        target: &NamedTarget,
    ) -> Self {
        Self {
            automatic_local_ingress: automatic_local_ingress_enabled(document, environment, target),
            host_decides: document.config.host_runtimes.is_some()
                || target.definition.transport != TargetTransport::Local,
            proxy_ports: Some(ingress_loopback_ports(target.definition.id)),
        }
    }

    /// Without the machine's targets: an environment on the target `local`
    /// runs on this machine, any other on a host.
    fn validating(document: &ConfigDocument, environment: &str) -> Self {
        let target = document
            .config
            .environments
            .get(environment)
            .and_then(|overlay| overlay.target.as_deref())
            .unwrap_or(document.config.target.as_str());
        let here = target == "local" && !document.config.targets.contains_key(target);
        Self {
            automatic_local_ingress: here
                && automatic_local_ingress_configured(document, environment),
            host_decides: !here,
            proxy_ports: None,
        }
    }

    pub(super) const fn automatic_local_ingress(self) -> bool {
        self.automatic_local_ingress
    }
}

/// What one stack's Compose files may say in one environment.
pub(super) struct SourcePolicy<'a> {
    document: &'a ConfigDocument,
    /// The services the environment leaves out of the stack here.
    pub excluded: BTreeSet<String>,
    routed: BTreeSet<&'a str>,
    pub compose_directory: PathBuf,
    proxy_ports: Option<(u16, u16, u16)>,
    pub host_network: super::host_network::HostNetwork<'a>,
}

impl<'a> SourcePolicy<'a> {
    pub(super) fn new(
        document: &'a ConfigDocument,
        environment: &str,
        stack_name: &'a str,
        ingress_routes: &'a [ResolvedIngressRoute],
        scope: SourceScope,
    ) -> Self {
        Self {
            document,
            excluded: excluded_service_names(document, environment, stack_name),
            routed: routed_service_names(
                document,
                environment,
                stack_name,
                ingress_routes,
                scope.automatic_local_ingress,
            ),
            compose_directory: compose_project_directory(document, stack_name),
            proxy_ports: scope.proxy_ports,
            host_network: super::host_network::HostNetwork::of(
                document,
                environment,
                stack_name,
                scope.host_decides,
            ),
        }
    }

    /// A Compose file of the stack as the compilation keeps it: without the
    /// services the environment leaves out, or refused.
    pub(super) fn check(&self, path: &Path, bytes: Vec<u8>) -> Result<Vec<u8>, MmError> {
        let bytes = filter_compose_services(path, bytes, &self.excluded)?;
        reject_reserved_ingress_configuration(
            path,
            &bytes,
            &self.routed,
            self.proxy_ports,
            &self.document.root,
            &self.compose_directory,
            &self.host_network,
        )?;
        reject_nonportable_compose(
            path,
            &bytes,
            &self.document.config.networks.keys().cloned().collect(),
        )?;
        Ok(bytes)
    }
}

/// What compiling each environment would refuse in the stacks' Compose
/// files, as findings. Files that are missing or not YAML, and stacks whose
/// routes do not resolve, are left to the other checks.
pub(crate) fn compose_source_diagnostics(document: &ConfigDocument, report: &mut ValidationReport) {
    let mut refused = BTreeMap::<(PathBuf, String), Vec<&str>>::new();
    for environment in document.config.environments.keys() {
        let scope = SourceScope::validating(document, environment);
        for (stack_name, stack) in &document.config.stacks {
            let Ok(routes) = resolve_ingress_services(document, environment, stack_name, stack)
            else {
                continue;
            };
            let policy = SourcePolicy::new(document, environment, stack_name, &routes, scope);
            for file in &stack.files {
                let path = document.root.join(file);
                let Ok(Some(bytes)) = read_regular_file(&path, MAX_COMPOSE_FILE_BYTES) else {
                    continue;
                };
                if serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&bytes).is_err() {
                    continue;
                }
                let checked = reject_transitive_compose_files(&path, &bytes)
                    .and_then(|()| policy.check(&path, bytes));
                if let Err(error) = checked {
                    let message = match error {
                        MmError::Environment { message } => message,
                        other => other.to_string(),
                    };
                    refused
                        .entry((path, message))
                        .or_default()
                        .push(environment);
                }
            }
        }
    }
    for ((path, message), environments) in refused {
        if report
            .diagnostics
            .iter()
            .any(|known| known.message == message)
        {
            continue;
        }
        let environments = match environments.as_slice() {
            [one] => format!("environment {one}"),
            many => format!("environments {}", many.join(", ")),
        };
        report.push(
            Diagnostic::error(
                "MM_COMPOSE_SOURCE",
                format!("{message} (in {environments})"),
            )
            .at_path(path),
        );
    }
}
