use std::collections::BTreeSet;
use std::path::{Component, PathBuf};

use crate::adapters::environment::template::TemplateParser;
use crate::domain::config::{ConfigDocument, EnvironmentBinding, ModuleEnvConfig};
use crate::domain::config_file::{MAX_CONFIG_FILE_BYTES, template_keys};
use crate::domain::config_resolve::{effective_binding, effective_bindings};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::environment::{
    ENVIRONMENT_GRAMMAR_VERSION, EnvironmentIndex, ModuleEnvironmentIndex, StackService,
};
use crate::domain::path_policy::{ensure_no_symlink_components, read_regular_file};

#[derive(Debug, Clone, Copy, Default)]
pub struct EnvironmentIndexer;

impl EnvironmentIndexer {
    #[must_use]
    pub fn index(&self, document: &ConfigDocument, environment: &str) -> EnvironmentIndex {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&ENVIRONMENT_GRAMMAR_VERSION.to_le_bytes());
        hasher.update(document.source_hash.as_bytes());
        hasher.update(environment.as_bytes());
        let mut modules = Vec::new();
        let mut diagnostics = Vec::new();
        let mut referenced_globals = BTreeSet::new();

        for (module_name, module) in &document.config.modules {
            if !module.services.is_empty()
                && !module
                    .services
                    .values()
                    .any(|service| service.is_active(environment))
            {
                continue;
            }
            let Some(env) = &module.env else {
                continue;
            };
            let Some((relative_path, template_path, source)) =
                load_template(document, env, &module.path, &mut diagnostics)
            else {
                continue;
            };
            hasher.update(module_name.as_bytes());
            hasher.update(relative_path.to_string_lossy().as_bytes());
            hasher.update(source.as_bytes());
            let parsed = TemplateParser.parse(&template_path, &source);
            diagnostics.extend(parsed.diagnostics);
            for variable in &parsed.variables {
                if let Some(global_key) = &variable.global_key {
                    referenced_globals.insert(global_key.clone());
                    if let Some(EnvironmentBinding::Secret(_)) =
                        effective_binding(&document.config, environment, global_key)
                    {
                        for (service, service_config) in &module.services {
                            if !service_config.is_active(environment) {
                                continue;
                            }
                            if service_config.env.contains_key(&variable.name) {
                                diagnostics.push(
                                    Diagnostic::error(
                                        "MM_ENV_SECRET_OVERRIDE",
                                        format!(
                                            "service '{service}' may not replace secret-backed variable '{}'",
                                            variable.name
                                        ),
                                    )
                                    .at_location(
                                        &variable.location.path,
                                        variable.location.line,
                                        variable.location.column,
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            hash_active_services(&mut hasher, module, environment);
            modules.push(ModuleEnvironmentIndex {
                module: module_name.clone(),
                template_path,
                services: module
                    .services
                    .iter()
                    .filter(|(_, service)| service.is_active(environment))
                    .map(|(name, config)| StackService {
                        stack: config.stack.clone(),
                        name: name.clone(),
                    })
                    .collect(),
                variables: parsed.variables,
            });
        }

        referenced_globals.extend(config_file_keys(document, environment));
        for key in effective_bindings(&document.config, environment).keys() {
            if !referenced_globals.contains(*key) {
                diagnostics.push(
                    Diagnostic::warning(
                        "MM_ENV_BINDING_UNUSED",
                        format!("environment binding '{key}' is not referenced by any template"),
                    )
                    .at_path(binding_source(document, environment, key)),
                );
            }
        }

        EnvironmentIndex {
            environment: environment.to_owned(),
            input_hash: hasher.finalize().to_hex().to_string(),
            modules,
            diagnostics,
        }
    }
}

/// The bindings that the config file templates of the environment's
/// services name. Problems with those files are `mm config validate`'s
/// `MM_CONFIG_FILE` findings; a template that cannot be read names nothing
/// here.
fn config_file_keys(document: &ConfigDocument, environment: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for module in document.config.modules.values() {
        let templates = module
            .services
            .values()
            .filter(|service| service.is_active(environment))
            .flat_map(|service| service.config_files_in(environment).map(|(_, file)| file))
            .filter_map(|file| file.template.as_ref());
        for template in templates {
            let relative = module.path.join(template);
            if relative.is_absolute()
                || relative.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                continue;
            }
            let path = document.root.join(relative);
            if ensure_no_symlink_components(&path).is_err() {
                continue;
            }
            if let Ok(Some(bytes)) = read_regular_file(&path, MAX_CONFIG_FILE_BYTES) {
                keys.extend(template_keys(&String::from_utf8_lossy(&bytes)));
            }
        }
    }
    keys
}

fn hash_active_services(
    hasher: &mut blake3::Hasher,
    module: &crate::domain::config::ModuleConfig,
    environment: &str,
) {
    for (service, config) in &module.services {
        if !config.is_active(environment) {
            continue;
        }
        hasher.update(config.stack.as_bytes());
        hasher.update(service.as_bytes());
        for (key, value) in &config.env {
            hasher.update(key.as_bytes());
            hasher.update(value.to_string().as_bytes());
        }
    }
}

fn load_template(
    document: &ConfigDocument,
    environment: &ModuleEnvConfig,
    module_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(PathBuf, PathBuf, String)> {
    let relative_path = module_path.join(&environment.template);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        diagnostics.push(
            Diagnostic::error(
                "MM_ENV_TEMPLATE_UNSAFE",
                "environment template path must remain inside the project root",
            )
            .at_path(document.source("modules")),
        );
        return None;
    }
    let unresolved = document.root.join(&relative_path);
    if let Err(error) = ensure_no_symlink_components(&unresolved) {
        diagnostics.push(
            Diagnostic::error("MM_ENV_TEMPLATE_UNSAFE", error.to_string()).at_path(&unresolved),
        );
        return None;
    }
    let template_path = match dunce::canonicalize(&unresolved) {
        Ok(path) if path.starts_with(&document.root) => path,
        Ok(_) => {
            diagnostics.push(
                Diagnostic::error(
                    "MM_ENV_TEMPLATE_UNSAFE",
                    "environment template resolves outside the project root",
                )
                .at_path(&unresolved),
            );
            return None;
        }
        Err(error) => {
            diagnostics.push(
                Diagnostic::error(
                    "MM_ENV_TEMPLATE_READ",
                    format!("could not resolve environment template: {error}"),
                )
                .at_path(&unresolved),
            );
            return None;
        }
    };
    let bytes = match read_regular_file(&template_path, 1024 * 1024) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            diagnostics.push(
                Diagnostic::error(
                    "MM_ENV_TEMPLATE_READ",
                    "environment template does not exist",
                )
                .at_path(&template_path),
            );
            return None;
        }
        Err(error) => {
            diagnostics.push(
                Diagnostic::error(
                    "MM_ENV_TEMPLATE_READ",
                    format!("could not read environment template: {error}"),
                )
                .at_path(&template_path),
            );
            return None;
        }
    };
    match String::from_utf8(bytes) {
        Ok(source) => Some((relative_path, template_path, source)),
        Err(_) => {
            diagnostics.push(
                Diagnostic::error(
                    "MM_ENV_TEMPLATE_READ",
                    "environment template must be valid UTF-8",
                )
                .at_path(&template_path),
            );
            None
        }
    }
}

impl crate::app::ports::EnvironmentSourceDiscovery for EnvironmentIndexer {
    fn index(&self, document: &ConfigDocument, environment: &str) -> EnvironmentIndex {
        Self::index(self, document, environment)
    }
}

/// The file that declares a binding: the selected environment's own bindings
/// win over top-level ones, matching `effective_bindings`.
fn binding_source<'a>(
    document: &'a ConfigDocument,
    environment: &str,
    key: &str,
) -> &'a std::path::Path {
    let environment_binding = document
        .config
        .environments
        .get(environment)
        .is_some_and(|config| config.bindings.contains_key(key));
    document.source(if environment_binding {
        "environments"
    } else {
        "bindings"
    })
}
