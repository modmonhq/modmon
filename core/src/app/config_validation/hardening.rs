use super::*;

/// A service that leaves a default of `hardening: strict` out has to say
/// why: the reason is what a reviewer reads in the plan.
pub(super) fn validate_hardening(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    for (module_name, module) in &document.config.modules {
        for (service_name, service) in &module.services {
            let Some(hardening) = &service.hardening else {
                continue;
            };
            for (key, reason) in hardening.except.entries() {
                if reason.trim().is_empty() {
                    let key = key.as_str();
                    report.push(
                        Diagnostic::error(
                            "MM_HARDENING_REASON",
                            format!(
                                "modules.{module_name}.services.{service_name}.hardening.except.{key} gives no reason"
                            ),
                        )
                        .with_help(format!(
                            "say in a few words why the service cannot take it, such as `{key}: \"writes its cache under /var/cache\"`, or remove the line to harden the service"
                        ))
                        .at_path(document.source("modules")),
                    );
                }
            }
        }
    }
}
