use super::*;

pub(super) fn validate_project_middlewares(
    document: &ConfigDocument,
    environment: &str,
    report: &mut ValidationReport,
) {
    let mut namespaced =
        BTreeMap::<String, (String, crate::domain::ingress::CanonicalMiddleware)>::new();
    for (bundle_name, bundle) in &document.config.ingress.middlewares {
        if !is_middleware_resource_name(bundle_name) {
            report.push(
                Diagnostic::error(
                    "MM_MIDDLEWARE_NAME_INVALID",
                    format!(
                        "middleware bundle '{bundle_name}' must be a lowercase Traefik resource name"
                    ),
                )
                .at_path(document.source("ingress")),
            );
        }
        for allowed in &bundle.environments {
            if !document.config.environments.contains_key(allowed) {
                report.push(
                    Diagnostic::error(
                        "MM_MIDDLEWARE_ENVIRONMENT_UNKNOWN",
                        format!(
                            "middleware bundle '{bundle_name}' allowlists unknown environment '{allowed}'"
                        ),
                    )
                    .at_path(document.source("ingress")),
                );
            }
        }
        validate_relative_path(
            &bundle.file,
            "middleware file",
            document.source("ingress"),
            report,
        );
        let resolved = document.root.join(&bundle.file);
        validate_no_symlinks(&resolved, "middleware file", report);
        if !resolved.is_file() {
            report.push(
                Diagnostic::error(
                    "MM_MIDDLEWARE_FILE_MISSING",
                    format!(
                        "middleware bundle '{bundle_name}' file does not exist: {}",
                        resolved.display()
                    ),
                )
                .at_path(&resolved),
            );
            continue;
        }
        if !bundle_is_active(&bundle.environments, environment) {
            continue;
        }
        match read_regular_file(&resolved, MAX_MIDDLEWARE_FILE_BYTES) {
            Ok(Some(bytes)) => validate_middleware_definitions(
                document,
                bundle_name,
                &resolved,
                &bytes,
                &mut namespaced,
                report,
            ),
            Ok(None) => {
                report.push(
                    Diagnostic::error(
                        "MM_MIDDLEWARE_FILE_MISSING",
                        format!(
                            "middleware bundle '{bundle_name}' file disappeared: {}",
                            resolved.display()
                        ),
                    )
                    .at_path(&resolved),
                );
            }
            Err(error) => {
                report.push(
                    Diagnostic::error("MM_MIDDLEWARE_UNREADABLE", error.to_string())
                        .at_path(&resolved),
                );
            }
        }
    }
}

fn validate_middleware_definitions(
    document: &ConfigDocument,
    bundle_name: &str,
    resolved: &Path,
    bytes: &[u8],
    namespaced: &mut BTreeMap<String, (String, crate::domain::ingress::CanonicalMiddleware)>,
    report: &mut ValidationReport,
) {
    match parse_middleware_file(resolved, bytes) {
        Ok(definitions) if definitions.is_empty() => report.push(
            Diagnostic::error(
                "MM_MIDDLEWARE_EMPTY",
                format!("middleware bundle '{bundle_name}' defines no middlewares"),
            )
            .at_path(resolved),
        ),
        Ok(definitions) => {
            for (definition_name, definition) in definitions {
                let name = namespaced_middleware_name(
                    document.config.project.id,
                    bundle_name,
                    &definition_name,
                );
                if let Some((owner, existing)) = namespaced.get(&name)
                    && existing != &definition
                {
                    report.push(
                        Diagnostic::error(
                            "MM_MIDDLEWARE_CONFLICT",
                            format!("middleware '{name}' conflicts with bundle '{owner}'"),
                        )
                        .at_path(resolved),
                    );
                } else {
                    namespaced.insert(name, (bundle_name.to_owned(), definition));
                }
            }
        }
        Err(error) => report
            .push(Diagnostic::error("MM_MIDDLEWARE_INVALID", error.to_string()).at_path(resolved)),
    }
}

#[cfg(test)]
mod tests;
