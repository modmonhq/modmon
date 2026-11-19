use super::*;

/// A service's `host-network` says why it runs on the host's network.
pub(super) fn validate_host_network(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    for (module, config) in &context.document.config.modules {
        for (service, config) in &config.services {
            if config
                .host_network
                .as_deref()
                .is_some_and(|reason| reason.trim().is_empty())
            {
                report.push(
                    Diagnostic::error(
                        "MM_HOST_NETWORK",
                        format!(
                            "modules.{module}.services.{service}.host-network has no reason; write why the service runs on the host's network, or remove the key"
                        ),
                    )
                    .at_path(context.document.source("modules")),
                );
            }
        }
    }
}
