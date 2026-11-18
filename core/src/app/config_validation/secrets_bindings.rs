use super::*;

pub(super) fn validate_secrets_and_bindings(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    validate_secret_declarations(document, report);
    let mut used_secrets = validate_environment_bindings(document, report);
    // Secrets mm itself consumes: registry logins, provided certificates,
    // and cluster deploy keys.
    for registry in document.config.registries.values() {
        used_secrets.insert(registry.password.secret.clone());
    }
    for (name, environment) in &document.config.environments {
        let Some(tls) = &environment.tls else {
            continue;
        };
        for (field, secret) in [("certificate", &tls.certificate), ("key", &tls.key)] {
            used_secrets.insert(secret.clone());
            if !document.config.secrets.contains_key(secret) {
                report.push(
                    Diagnostic::error(
                        "MM_ENVIRONMENT_TLS_INVALID",
                        format!(
                            "environments.{name}.tls.{field} names secret '{secret}', which secrets: does not declare; declare it, then store the PEM file with `base64 -w0 FILE | mm secret set {secret} --stdin`"
                        ),
                    )
                    .at_path(document.source("environments")),
                );
            }
        }
    }
    used_secrets.extend(
        document
            .config
            .environments
            .values()
            .filter_map(|environment| environment.ingress.as_ref()?.tunnel())
            .map(|(token, _)| token),
    );
    // And the files services mount with `secret-files`.
    for module in document.config.modules.values() {
        for service in module.services.values() {
            used_secrets.extend(
                service
                    .secret_files
                    .values()
                    .map(|file| file.secret.clone()),
            );
        }
    }
    for target in document.config.targets.values() {
        if let Some(deploy_key) = target
            .setup
            .as_ref()
            .and_then(|setup| setup.deploy_key.as_ref())
        {
            used_secrets.insert(deploy_key.secret.clone());
        }
    }
    for secret_name in document.config.secrets.keys() {
        if !used_secrets.contains(secret_name) {
            report.push(
                Diagnostic::warning(
                    "MM_SECRET_UNUSED",
                    format!("secret '{secret_name}' is not used by any environment binding"),
                )
                .at_path(document.source("secrets")),
            );
        }
    }
}

fn validate_secret_declarations(document: &ConfigDocument, report: &mut ValidationReport) {
    let config_file = document.source("secrets");
    let mut provider_owners = BTreeMap::<String, &str>::new();
    for (secret_name, declaration) in &document.config.secrets {
        if !is_secret_name(secret_name) {
            report.push(
                Diagnostic::error(
                    "MM_SECRET_NAME_INVALID",
                    format!(
                        "secret name '{secret_name}' must use letters, numbers, '.', '-' or '_'"
                    ),
                )
                .at_path(config_file),
            );
        }
        validate_secret_reference(
            secret_name,
            &declaration.storage,
            &document.config,
            config_file,
            report,
        );
        if let Ok(reference) = declaration.storage.reference() {
            let canonical = reference.canonical();
            if let Some(owner) = provider_owners.insert(canonical, secret_name) {
                report.push(
                    Diagnostic::error(
                        "MM_SECRET_PROVIDER_DUPLICATE",
                        format!(
                            "secrets '{owner}' and '{secret_name}' use the same provider location"
                        ),
                    )
                    .at_path(config_file),
                );
            }
        }
        if declaration.generate.is_some() && declaration.input.is_some() {
            report.push(
                Diagnostic::error(
                    "MM_SECRET_SOURCE_CONFLICT",
                    format!(
                        "secret '{secret_name}' may use either generate or input, but not both"
                    ),
                )
                .at_path(config_file),
            );
        }
        if let Some(generator) = &declaration.generate {
            validate_secret_generator(secret_name, generator, config_file, report);
        }
        if let Some(input) = &declaration.input
            && (input.prompt.trim().is_empty() || input.prompt.chars().any(char::is_control))
        {
            report.push(
                Diagnostic::error(
                    "MM_SECRET_PROMPT_INVALID",
                    format!("secret '{secret_name}' has an empty or unsafe input prompt"),
                )
                .at_path(config_file),
            );
        }
        if declaration.rotation.interval_days == Some(0) {
            report.push(
                Diagnostic::error(
                    "MM_SECRET_ROTATION_INVALID",
                    format!("secret '{secret_name}' rotation interval must be at least one day"),
                )
                .at_path(config_file),
            );
        }
    }
}

fn validate_environment_bindings(
    document: &ConfigDocument,
    report: &mut ValidationReport,
) -> BTreeSet<String> {
    let config_file = document.source("environments");
    let mut used_secrets = BTreeSet::new();
    validate_binding_map(
        &document.config.bindings,
        "top-level",
        document,
        document.source("bindings"),
        &mut used_secrets,
        report,
    );
    for (environment_name, environment_config) in &document.config.environments {
        validate_name(environment_name, "environment name", config_file, report);
        for (domain_key, host) in &environment_config.domains {
            validate_name(domain_key, "domain key", config_file, report);
            if !is_route_host(host) {
                report.push(
                    Diagnostic::error(
                        "MM_INGRESS_HOST_INVALID",
                        format!(
                            "environment '{environment_name}' domain '{domain_key}' has invalid lowercase hostname '{host}'"
                        ),
                    )
                    .at_path(config_file),
                );
            }
        }
        if let Some(target) = &environment_config.target
            && let Err(message) = validate_target_name(target)
        {
            report.push(
                Diagnostic::error(
                    "MM_TARGET_NAME_INVALID",
                    format!("environment '{environment_name}' target is invalid: {message}"),
                )
                .at_path(config_file),
            );
        }
        if let Some(variant) = &environment_config.build_variant
            && !is_build_identifier(variant)
        {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_VARIANT_INVALID",
                    format!(
                        "environment '{environment_name}' build-variant '{variant}' is not a safe Docker target"
                    ),
                )
                .at_path(config_file),
            );
        }
        validate_binding_map(
            &environment_config.bindings,
            &format!("environment '{environment_name}'"),
            document,
            config_file,
            &mut used_secrets,
            report,
        );
        for (key, binding) in effective_bindings(&document.config, environment_name) {
            let Some(cast) = binding.cast() else {
                continue;
            };
            match parse_domain_value_ref(&cast.value_ref) {
                Ok(domain) => {
                    if !effective_domains(&document.config, environment_name).contains_key(domain) {
                        report.push(
                            Diagnostic::error(
                                "MM_ENV_VALUE_REF_INVALID",
                                format!(
                                    "environment '{environment_name}' binding '{key}' references unknown domain '{domain}'"
                                ),
                            )
                            .at_path(config_file),
                        );
                    }
                }
                Err(message) => {
                    report.push(
                        Diagnostic::error(
                            "MM_ENV_VALUE_REF_INVALID",
                            format!("environment '{environment_name}' binding '{key}': {message}"),
                        )
                        .at_path(config_file),
                    );
                }
            }
        }
    }

    used_secrets
}

fn validate_binding_map(
    bindings: &BTreeMap<String, EnvironmentBinding>,
    scope: &str,
    document: &ConfigDocument,
    source: &Path,
    used_secrets: &mut BTreeSet<String>,
    report: &mut ValidationReport,
) {
    for key in bindings.keys() {
        if !is_global_key(key) {
            report.push(
                Diagnostic::error(
                    "MM_ENV_KEY_INVALID",
                    format!("{scope} contains invalid global key '{key}'"),
                )
                .at_path(source)
                .with_help("use dot-separated uppercase names such as DB.HOST"),
            );
        }
    }
    for (key, binding) in bindings {
        match binding {
            EnvironmentBinding::Secret(secret) => {
                used_secrets.insert(secret.secret.clone());
                if !document.config.secrets.contains_key(&secret.secret) {
                    report.push(
                        Diagnostic::error(
                            "MM_SECRET_UNKNOWN",
                            format!(
                                "{scope} binding '{key}' references unknown secret '{}'",
                                secret.secret
                            ),
                        )
                        .at_path(source),
                    );
                }
            }
            EnvironmentBinding::Value(value)
                if !is_safe_environment_value(&value.value.to_string()) =>
            {
                report.push(
                    Diagnostic::error(
                        "MM_ENV_VALUE_INVALID",
                        format!("{scope} binding '{key}' contains control characters"),
                    )
                    .at_path(source),
                );
            }
            EnvironmentBinding::Value(_)
            | EnvironmentBinding::Cast(_)
            | EnvironmentBinding::Reference(_) => {}
        }
    }
}

pub(super) fn validate_relative_path(
    candidate: &Path,
    kind: &str,
    config_path: &Path,
    report: &mut ValidationReport,
) {
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        report.push(
            Diagnostic::error(
                "MM_PATH_INVALID",
                format!(
                    "{kind} must be a project-relative path: {}",
                    candidate.display()
                ),
            )
            .at_path(config_path),
        );
    }
}

pub(super) fn validate_secret_reference(
    secret_name: &str,
    storage: &SecretStorageConfig,
    config: &crate::domain::config::MmConfig,
    path: &Path,
    report: &mut ValidationReport,
) {
    let validation = storage.reference().and_then(|_| {
        let built_in = matches!(storage.provider.as_str(), "age" | "env");
        let configured = config
            .providers
            .keys()
            .any(|name| name.as_str() == storage.provider.as_str());
        if !built_in && !configured {
            Err(format!(
                "provider instance '{}' is not declared in providers",
                storage.provider
            ))
        } else if storage.provider.as_str() == "env" && !is_variable_name(&storage.location) {
            Err("env storage locations must be environment variable names".to_owned())
        } else if storage.provider.as_str() == "env"
            && reserved_runtime_environment_names().any(|name| name == storage.location)
        {
            Err(
                "env storage locations may not use runtime-reserved environment variables"
                    .to_owned(),
            )
        } else {
            Ok(())
        }
    });
    if let Err(message) = validation {
        report.push(
            Diagnostic::error(
                "MM_SECRET_REF_INVALID",
                format!("secret '{secret_name}' has an invalid storage location: {message}"),
            )
            .at_path(path),
        );
    }
}

pub(super) fn validate_secret_generator(
    secret_name: &str,
    generator: &SecretGeneratorConfig,
    path: &Path,
    report: &mut ValidationReport,
) {
    if let Some(length) = generator.length()
        && !(16..=4096).contains(&length)
    {
        report.push(
            Diagnostic::error(
                "MM_SECRET_GENERATOR_INVALID",
                format!("secret '{secret_name}' generator length must be between 16 and 4096"),
            )
            .at_path(path),
        );
    }
}

#[cfg(test)]
mod tests;
