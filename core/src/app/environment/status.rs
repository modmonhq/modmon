use super::*;

impl EnvironmentManager {
    pub(super) fn runtime_secret_statuses(
        &self,
        document: &ConfigDocument,
        environment: &str,
        requested_key: Option<&str>,
        runtime_metadata: &BTreeMap<String, SecretMetadata>,
        delivery: &str,
    ) -> Vec<RuntimeSecretStatusItem> {
        let mut statuses = BTreeMap::<String, RuntimeSecretStatusItem>::new();
        for (service_name, service) in document.config.active_services(environment) {
            for (target, configured) in service.secret_files_in(environment) {
                if requested_key.is_some_and(|requested| requested != configured.secret) {
                    continue;
                }
                let Some(declaration) = document.config.secrets.get(&configured.secret) else {
                    continue;
                };
                let metadata = runtime_metadata.get(&configured.secret);
                let created_at = metadata.and_then(|metadata| metadata.created_at_unix_ms);
                let status = statuses
                    .entry(configured.secret.clone())
                    .or_insert_with(|| RuntimeSecretStatusItem {
                        logical_name: configured.secret.clone(),
                        backend: declaration.storage.provider.to_string(),
                        delivery: delivery.to_owned(),
                        revision: metadata.and_then(|metadata| metadata.revision.clone()),
                        age_seconds: created_at.map(|created_at| {
                            self.clock.now_unix_ms().saturating_sub(created_at).max(0) as u64 / 1000
                        }),
                        consumers: Vec::new(),
                    });
                status
                    .consumers
                    .push(format!("{}/{service_name}:/{}", service.stack, target));
            }
        }
        let mut statuses = statuses.into_values().collect::<Vec<_>>();
        for status in &mut statuses {
            status.consumers.sort();
            status.consumers.dedup();
        }
        statuses
    }
}

pub(super) fn append_secret_diagnostic(
    canonical_reference: &str,
    metadata: &SecretMetadata,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scheme = canonical_reference
        .split_once('/')
        .map_or("unknown", |(scheme, _)| scheme);
    match metadata.status {
        SecretStatus::ProviderUnavailable => diagnostics.push(Diagnostic::error(
            "MM_SECRET_PROVIDER_UNAVAILABLE",
            format!("secret provider '{scheme}' is not initialized or available"),
        )),
        SecretStatus::AccessDenied => diagnostics.push(Diagnostic::error(
            "MM_SECRET_ACCESS_DENIED",
            format!("secret provider '{scheme}' denied access"),
        )),
        SecretStatus::Invalid => diagnostics.push(Diagnostic::error(
            "MM_SECRET_INVALID",
            format!("secret provider '{scheme}' returned an unusable value"),
        )),
        SecretStatus::Set | SecretStatus::Unset | SecretStatus::NotChecked => {}
    }
}

pub(super) fn select_compiled_service<'a>(
    bundle: &'a CompiledEnvironmentBundle,
    module: &str,
    requested: Option<&str>,
) -> Result<&'a CompiledServiceEnvironment, MmError> {
    let services = bundle
        .services
        .iter()
        .filter(|compiled| compiled.module == module)
        .collect::<Vec<_>>();
    if let Some(requested) = requested {
        return services
            .into_iter()
            .find(|compiled| compiled.service == requested)
            .ok_or_else(|| MmError::Environment {
                message: format!(
                    "service '{requested}' has no compiled environment in module '{module}'"
                ),
            });
    }
    let Some(first) = services.first().copied() else {
        return Err(MmError::Environment {
            message: format!("module '{module}' has no compiled service environments"),
        });
    };
    if services
        .iter()
        .skip(1)
        .any(|compiled| compiled.bytes() != first.bytes())
    {
        return Err(MmError::Environment {
            message: format!(
                "module '{module}' has different service environments; pass --service"
            ),
        });
    }
    Ok(first)
}

pub(super) struct EnvironmentContext {
    pub(super) document: ConfigDocument,
    pub(super) environment: String,
    pub(super) index: EnvironmentIndex,
    pub(super) validation: ValidationReport,
    pub(super) workspace: WorkspaceRecord,
}

pub(super) fn ensure_index_valid(validation: &ValidationReport) -> Result<(), MmError> {
    let errors = validation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    if errors == 0 {
        Ok(())
    } else {
        Err(MmError::Validation { errors })
    }
}

pub(super) fn ensure_override_allowed(
    document: &ConfigDocument,
    environment: &str,
    index: &EnvironmentIndex,
    key: &str,
) -> Result<(), MmError> {
    let referenced = index.modules.iter().any(|module| {
        module
            .variables
            .iter()
            .any(|variable| variable.global_key.as_deref() == Some(key))
    });
    if !referenced {
        return Err(MmError::Environment {
            message: format!("global key '{key}' is not referenced by any template"),
        });
    }
    if let Some(secret_name) = effective_binding(&document.config, environment, key)
        .and_then(EnvironmentBinding::secret_name)
    {
        return Err(MmError::Environment {
            message: format!(
                "global key '{key}' is secret-backed; use `mm secret set {secret_name}`"
            ),
        });
    }
    Ok(())
}

pub(super) fn used_secret_references(
    document: &ConfigDocument,
    environment: &str,
    index: &EnvironmentIndex,
) -> Result<BTreeMap<String, SecretReference>, MmError> {
    let used_keys = index
        .modules
        .iter()
        .flat_map(|module| module.variables.iter())
        .filter_map(|variable| variable.global_key.clone())
        .collect::<BTreeSet<_>>();
    let mut references = BTreeMap::new();
    for (key, binding) in effective_bindings(&document.config, environment) {
        let Some(secret_name) = used_keys
            .contains(key)
            .then(|| binding.secret_name())
            .flatten()
        else {
            continue;
        };
        let declaration =
            document
                .config
                .secrets
                .get(secret_name)
                .ok_or_else(|| MmError::Environment {
                    message: format!(
                        "global key '{key}' references undeclared secret '{secret_name}'"
                    ),
                })?;
        let reference = configured_reference(declaration)?;
        references.insert(reference.canonical(), reference);
    }
    for configured in document
        .config
        .modules
        .values()
        .flat_map(|module| module.services.values())
        .filter(|service| service.is_active(environment))
        .flat_map(|service| service.secret_files_in(environment).map(|(_, file)| file))
    {
        if let Some(declaration) = document.config.secrets.get(&configured.secret) {
            let reference = configured_reference(declaration)?;
            references.insert(reference.canonical(), reference);
        }
    }
    Ok(references)
}

pub(super) fn configured_reference(
    declaration: &SecretDeclaration,
) -> Result<SecretReference, MmError> {
    declaration
        .storage
        .reference()
        .map_err(|message| MmError::Secret { message })
}

pub(super) fn status_modules(
    document: &ConfigDocument,
    environment: &str,
    index: &EnvironmentIndex,
    overrides: &BTreeMap<String, EnvironmentValueOverride>,
    secret_metadata: &BTreeMap<String, SecretMetadata>,
    check_secrets: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ModuleEnvironmentStatus> {
    index
        .modules
        .iter()
        .map(|module| {
            let module_config = &document.config.modules[&module.module];
            let services = module
                .services
                .iter()
                .map(|service| {
                    let service_config = &module_config.services[&service.name];
                    let mut variables = module
                        .variables
                        .iter()
                        .map(|declaration| {
                            variable_status(VariableStatusRequest {
                                document,
                                environment,
                                service: &service.name,
                                declaration,
                                service_overrides: &service_config.env,
                                overrides,
                                secret_metadata,
                                check_secrets,
                                diagnostics,
                            })
                        })
                        .collect::<Vec<_>>();
                    let declared = module
                        .variables
                        .iter()
                        .map(|variable| variable.name.as_str())
                        .collect::<BTreeSet<_>>();
                    for (name, value) in &service_config.env {
                        if !declared.contains(name.as_str()) {
                            variables.push(VariableStatus {
                                name: name.clone(),
                                global_key: None,
                                required: false,
                                has_default: true,
                                sensitive: false,
                                source: ValueSource::ServiceEnv,
                                status: ResolutionStatus::Resolved,
                                value: Some(value.to_string()),
                                provider: None,
                                revision: None,
                                location: crate::domain::environment::SourceLocation {
                                    path: document.source("modules").to_path_buf(),
                                    line: 1,
                                    column: 1,
                                },
                            });
                        }
                    }
                    ServiceEnvironmentStatus {
                        stack: service.stack.clone(),
                        service: service.name.clone(),
                        variables,
                    }
                })
                .collect();
            ModuleEnvironmentStatus {
                module: module.module.clone(),
                template_path: module.template_path.clone(),
                services,
            }
        })
        .collect()
}

pub(super) struct VariableStatusRequest<'a> {
    document: &'a ConfigDocument,
    environment: &'a str,
    service: &'a str,
    declaration: &'a VariableDeclaration,
    service_overrides: &'a BTreeMap<String, crate::domain::config::EnvScalar>,
    overrides: &'a BTreeMap<String, EnvironmentValueOverride>,
    secret_metadata: &'a BTreeMap<String, SecretMetadata>,
    check_secrets: bool,
    diagnostics: &'a mut Vec<Diagnostic>,
}

pub(super) fn variable_status(request: VariableStatusRequest<'_>) -> VariableStatus {
    let release = request
        .declaration
        .global_key
        .as_deref()
        .filter(|_| {
            !request
                .service_overrides
                .contains_key(&request.declaration.name)
        })
        .and_then(|key| {
            crate::domain::release_facts::release_value(
                &request.document.config,
                request.environment,
                key,
            )
            .map(|release| (key, release))
        });
    match release {
        Some(release) => release_status(request.declaration, release, request.diagnostics),
        None => binding_status(request),
    }
}

fn binding_status(request: VariableStatusRequest<'_>) -> VariableStatus {
    let VariableStatusRequest {
        document,
        environment,
        service,
        declaration,
        service_overrides,
        overrides,
        secret_metadata,
        check_secrets,
        diagnostics,
    } = request;
    let mut status = if let Some(value) = service_overrides.get(&declaration.name) {
        public_status(declaration, ValueSource::ServiceEnv, value.to_string())
    } else if let Some(global_key) = &declaration.global_key {
        let binding = effective_binding(&document.config, environment, global_key);
        let secret_declaration = binding
            .and_then(EnvironmentBinding::secret_name)
            .and_then(|secret_name| document.config.secrets.get(secret_name));
        let reference = secret_declaration.and_then(|secret| match configured_reference(secret) {
            Ok(reference) => Some(reference),
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "MM_SECRET_REFERENCE_INVALID",
                    error.to_string(),
                ));
                None
            }
        });
        if let Some(reference) = reference {
            let metadata = secret_metadata.get(&reference.canonical());
            if metadata.is_some_and(|metadata| metadata.status == SecretStatus::Unset)
                && declaration.default_value.is_some()
            {
                let mut status = public_status(
                    declaration,
                    ValueSource::TemplateDefault,
                    declaration.default_value.clone().unwrap_or_default(),
                );
                status.provider = Some(reference.scheme().to_owned());
                status
            } else {
                secret_status(declaration, reference.scheme(), metadata)
            }
        } else if let Some(value) = overrides.get(global_key) {
            public_status(
                declaration,
                ValueSource::SqliteOverride,
                value.value.clone(),
            )
        } else if let Some(cast) = binding.and_then(EnvironmentBinding::cast) {
            match resolve_cast_url(&document.config, environment, cast) {
                Ok(value) => public_status(declaration, ValueSource::ConfigValue, value),
                Err(_) => missing_status(declaration, ResolutionStatus::Invalid),
            }
        } else if let Some(value) = binding.and_then(EnvironmentBinding::value) {
            public_status(declaration, ValueSource::ConfigValue, value.to_string())
        } else if let Some(value) = &declaration.default_value {
            public_status(declaration, ValueSource::TemplateDefault, value.clone())
        } else {
            missing_status(declaration, ResolutionStatus::Unset)
        }
    } else {
        public_status(
            declaration,
            ValueSource::TemplateLiteral,
            declaration.literal_value.clone().unwrap_or_default(),
        )
    };

    let unresolved = match status.status {
        ResolutionStatus::Unset
        | ResolutionStatus::ProviderUnavailable
        | ResolutionStatus::AccessDenied
        | ResolutionStatus::Invalid => true,
        ResolutionStatus::Resolved | ResolutionStatus::Defaulted => {
            !status.sensitive && status.value.as_deref().is_none_or(str::is_empty)
        }
        ResolutionStatus::NotChecked => false,
    };
    if declaration.required && unresolved && (check_secrets || !status.sensitive) {
        status.status = ResolutionStatus::Unset;
        diagnostics.push(
            Diagnostic::error(
                "MM_ENV_REQUIRED_UNRESOLVED",
                format!(
                    "required variable '{}' for service '{service}' is unresolved",
                    declaration.name
                ),
            )
            .at_location(
                &declaration.location.path,
                declaration.location.line,
                declaration.location.column,
            ),
        );
    }
    status
}

/// A variable that reads a release fact: its value in the deployment
/// being planned, or, with nothing being deployed, not known yet.
fn release_status(
    declaration: &VariableDeclaration,
    (key, release): (&str, crate::domain::release_facts::ReleaseValue),
    diagnostics: &mut Vec<Diagnostic>,
) -> VariableStatus {
    use crate::domain::release_facts::ReleaseValue;
    match release {
        ReleaseValue::Known(value) => public_status(declaration, ValueSource::Release, value),
        ReleaseValue::Pending => VariableStatus {
            source: ValueSource::Release,
            status: ResolutionStatus::NotChecked,
            ..missing_status(declaration, ResolutionStatus::NotChecked)
        },
        ReleaseValue::Missing(reason) => {
            diagnostics.push(
                Diagnostic::error(
                    "MM_RELEASE_INPUT",
                    format!("variable '{}' reads {key}, but {reason}", declaration.name),
                )
                .at_location(
                    &declaration.location.path,
                    declaration.location.line,
                    declaration.location.column,
                ),
            );
            missing_status(declaration, ResolutionStatus::Invalid)
        }
    }
}

fn secret_status(
    declaration: &VariableDeclaration,
    provider: &str,
    metadata: Option<&SecretMetadata>,
) -> VariableStatus {
    VariableStatus {
        name: declaration.name.clone(),
        global_key: declaration.global_key.clone(),
        required: declaration.required,
        has_default: declaration.default_value.is_some(),
        sensitive: true,
        source: ValueSource::Secret,
        status: metadata.map_or(ResolutionStatus::NotChecked, |metadata| {
            metadata.status.into()
        }),
        value: None,
        provider: Some(provider.to_owned()),
        revision: metadata.and_then(|metadata| metadata.revision.clone()),
        location: declaration.location.clone(),
    }
}

fn missing_status(declaration: &VariableDeclaration, status: ResolutionStatus) -> VariableStatus {
    VariableStatus {
        name: declaration.name.clone(),
        global_key: declaration.global_key.clone(),
        required: declaration.required,
        has_default: declaration.default_value.is_some(),
        sensitive: false,
        source: ValueSource::Missing,
        status,
        value: None,
        provider: None,
        revision: None,
        location: declaration.location.clone(),
    }
}

pub(super) fn public_status(
    declaration: &VariableDeclaration,
    source: ValueSource,
    value: String,
) -> VariableStatus {
    VariableStatus {
        name: declaration.name.clone(),
        global_key: declaration.global_key.clone(),
        required: declaration.required,
        has_default: declaration.default_value.is_some(),
        sensitive: false,
        source,
        status: if source == ValueSource::TemplateDefault {
            ResolutionStatus::Defaulted
        } else {
            ResolutionStatus::Resolved
        },
        value: Some(value),
        provider: None,
        revision: None,
        location: declaration.location.clone(),
    }
}

pub(super) fn status_input_hash(
    index: &EnvironmentIndex,
    overrides: &BTreeMap<String, EnvironmentValueOverride>,
    secret_metadata: &BTreeMap<String, SecretMetadata>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(index.input_hash.as_bytes());
    for (key, value) in overrides {
        hasher.update(key.as_bytes());
        hasher.update(&value.revision.to_le_bytes());
    }
    for (reference, metadata) in secret_metadata {
        let scheme = reference
            .split_once('/')
            .map_or(reference.as_str(), |(scheme, _)| scheme);
        hasher.update(scheme.as_bytes());
        hasher.update(
            metadata
                .revision
                .as_deref()
                .unwrap_or("volatile")
                .as_bytes(),
        );
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests;
