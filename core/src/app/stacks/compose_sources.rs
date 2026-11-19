//! The Compose source policy of one stack in one environment: the
//! compilation keeps a file only when it passes.

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
