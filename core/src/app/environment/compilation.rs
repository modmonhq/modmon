use super::*;
use crate::domain::release_facts::ReleaseValue;

pub(super) fn compile_bundle(
    document: &ConfigDocument,
    environment: &str,
    index: &EnvironmentIndex,
    input_hash: &str,
    overrides: &BTreeMap<String, EnvironmentValueOverride>,
    secrets: &BTreeMap<String, Arc<SecretMaterial>>,
) -> Result<CompiledEnvironmentBundle, MmError> {
    let mut compiled = Vec::new();
    let mut manifests = Vec::new();
    for module in &index.modules {
        let module_config = &document.config.modules[&module.module];
        for service in &module.services {
            let service_config = &module_config.services[&service.name];
            let mut secret_count = 0;
            let mut names = BTreeSet::new();
            let mut assignments = Vec::new();
            let mut direct_values = crate::domain::runtime::RuntimeEnvironment::default();
            for declaration in &module.variables {
                names.insert(declaration.name.clone());
                let (value, sensitive) = resolved_value(
                    document,
                    environment,
                    declaration,
                    &service_config.env,
                    overrides,
                    secrets,
                )?;
                if declaration.required && value.is_empty() {
                    return Err(MmError::Environment {
                        message: format!(
                            "required variable '{}' for service '{service}' is empty",
                            declaration.name,
                            service = service.name
                        ),
                    });
                }
                secret_count += usize::from(sensitive);
                drop(direct_values.insert(declaration.name.clone(), value.to_string()));
                assignments.push((declaration.name.clone(), value));
            }
            for (name, value) in &service_config.env {
                if names.insert(name.clone()) {
                    drop(direct_values.insert(name.clone(), value.to_string()));
                    assignments.push((name.clone(), Zeroizing::new(value.to_string())));
                }
            }
            let runtime_secrets =
                compile_runtime_secrets(document, environment, service_config, secrets)?;
            let estimated_bytes = assignments.iter().fold(0_usize, |total, (name, value)| {
                total
                    .saturating_add(name.len())
                    .saturating_add(value.len().saturating_mul(2))
                    .saturating_add(4)
            });
            let mut bytes = Zeroizing::new(Vec::new());
            bytes
                .try_reserve_exact(estimated_bytes)
                .map_err(|error| MmError::Environment {
                    message: format!("could not allocate compiled environment output: {error}"),
                })?;
            for (name, value) in &assignments {
                encode_assignment(&mut bytes, name, value)?;
            }
            manifests.push(CompiledServiceManifest {
                module: module.module.clone(),
                stack: service.stack.clone(),
                service: service.name.clone(),
                variable_count: names.len(),
                secret_count: secret_count + service_config.secret_files_in(environment).count(),
            });
            compiled.push(
                CompiledServiceEnvironment::new(
                    module.module.clone(),
                    service.stack.clone(),
                    service.name.clone(),
                    std::mem::take(&mut *bytes),
                )
                .with_intent(direct_values)
                .with_runtime_secrets(runtime_secrets),
            );
        }
    }
    Ok(CompiledEnvironmentBundle {
        manifest: CompiledEnvironmentManifest {
            project_name: document.config.project.name.clone(),
            environment: environment.to_owned(),
            input_hash: input_hash.to_owned(),
            services: manifests,
        },
        services: compiled,
    })
}

fn compile_runtime_secrets(
    document: &ConfigDocument,
    environment: &str,
    service: &crate::domain::config::ServiceConfig,
    secrets: &BTreeMap<String, Arc<SecretMaterial>>,
) -> Result<Vec<CompiledRuntimeSecret>, MmError> {
    let mut runtime_secrets = Vec::new();
    for (_, configured) in service.secret_files_in(environment) {
        let declaration = document
            .config
            .secrets
            .get(&configured.secret)
            .ok_or_else(|| MmError::Secret {
                message: format!("logical secret '{}' is not configured", configured.secret),
            })?;
        let reference = configured_reference(declaration)?;
        let material = secrets
            .get(&reference.canonical())
            .ok_or_else(|| MmError::Secret {
                message: format!(
                    "logical secret '{}' has no resolved material",
                    configured.secret
                ),
            })?;
        runtime_secrets.push(CompiledRuntimeSecret {
            logical_name: configured.secret.clone(),
            material: Arc::clone(material),
        });
    }
    runtime_secrets.sort_by(|left, right| left.logical_name.cmp(&right.logical_name));
    runtime_secrets.dedup_by(|left, right| left.logical_name == right.logical_name);
    Ok(runtime_secrets)
}

pub(super) fn resolved_value(
    document: &ConfigDocument,
    environment: &str,
    declaration: &VariableDeclaration,
    service_overrides: &BTreeMap<String, crate::domain::config::EnvScalar>,
    overrides: &BTreeMap<String, EnvironmentValueOverride>,
    secrets: &BTreeMap<String, Arc<SecretMaterial>>,
) -> Result<(Zeroizing<String>, bool), MmError> {
    if let Some(value) = service_overrides.get(&declaration.name) {
        return Ok((Zeroizing::new(value.to_string()), false));
    }
    let Some(global_key) = &declaration.global_key else {
        return Ok((
            Zeroizing::new(declaration.literal_value.clone().unwrap_or_default()),
            false,
        ));
    };
    match crate::domain::release_facts::release_value(&document.config, environment, global_key) {
        Some(ReleaseValue::Known(value)) => return Ok((Zeroizing::new(value), false)),
        Some(ReleaseValue::Missing(reason)) => {
            return Err(MmError::ReleaseInput {
                message: format!(
                    "variable '{}' reads {global_key}, but {reason}",
                    declaration.name
                ),
            });
        }
        Some(ReleaseValue::Pending) => {
            return Ok((
                Zeroizing::new(declaration.default_value.clone().unwrap_or_default()),
                false,
            ));
        }
        None => {}
    }
    let binding = effective_binding(&document.config, environment, global_key);
    let secret_declaration = binding
        .and_then(EnvironmentBinding::secret_name)
        .and_then(|secret_name| document.config.secrets.get(secret_name));
    let reference = secret_declaration
        .map(|declaration| {
            declaration
                .storage
                .reference()
                .map_err(|message| MmError::Environment { message })
        })
        .transpose()?;
    if let Some(reference) = reference {
        if let Some(material) = secrets.get(&reference.canonical()) {
            return Ok((Zeroizing::new(material.expose(str::to_owned)), true));
        }
        return Ok((
            Zeroizing::new(declaration.default_value.clone().unwrap_or_default()),
            false,
        ));
    }
    if let Some(value) = overrides.get(global_key) {
        return Ok((Zeroizing::new(value.value.clone()), false));
    }
    if let Some(cast) = binding.and_then(EnvironmentBinding::cast) {
        let value = resolve_cast_url(&document.config, environment, cast)
            .map_err(|message| MmError::Environment { message })?;
        return Ok((Zeroizing::new(value), false));
    }
    if let Some(value) = binding.and_then(EnvironmentBinding::value) {
        return Ok((Zeroizing::new(value.to_string()), false));
    }
    Ok((
        Zeroizing::new(declaration.default_value.clone().unwrap_or_default()),
        false,
    ))
}

pub(super) fn encode_assignment(
    output: &mut Vec<u8>,
    name: &str,
    value: &str,
) -> Result<(), MmError> {
    if value.contains('\0') || value.contains(['\n', '\r']) {
        return Err(MmError::Environment {
            message: format!("value for '{name}' contains an unsupported NUL or newline"),
        });
    }
    output.extend_from_slice(name.as_bytes());
    output.push(b'=');
    if value.is_empty() {
        output.push(b'\n');
        return Ok(());
    }
    let quote = value.chars().any(|character| {
        character.is_whitespace() || matches!(character, '#' | '\'' | '"' | '\\' | '$' | '`')
    });
    if quote {
        output.push(b'"');
        for character in value.chars() {
            match character {
                '"' => output.extend_from_slice(b"\\\""),
                '\\' => output.extend_from_slice(b"\\\\"),
                '$' => output.extend_from_slice(b"$$"),
                '\t' => output.extend_from_slice(b"\\t"),
                other => {
                    let mut buffer = [0; 4];
                    output.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
                }
            }
        }
        output.push(b'"');
    } else {
        output.extend_from_slice(value.as_bytes());
    }
    output.push(b'\n');
    Ok(())
}
