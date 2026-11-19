use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::app::config_middleware::{
    MAX_MIDDLEWARE_FILE_BYTES, bundle_is_active, middleware_unknown_keys,
    namespaced_middleware_name, read_middleware_file,
};
use crate::domain::config::ConfigDocument;
use crate::domain::config::{
    EnvironmentBinding, ModuleConfig, SecretGeneratorConfig, SecretStorageConfig,
};
use crate::domain::config_resolve::{
    effective_bindings, effective_domains, parse_domain_value_ref, route_is_active,
};
use crate::domain::diagnostic::{Diagnostic, ValidationReport};
use crate::domain::environment::is_safe_environment_value;
use crate::domain::local_ingress::{ProjectSlug, ServiceSlug, is_reserved_mm_host};
use crate::domain::path_policy::read_regular_file;
use crate::domain::runtime::{RuntimeTarget, reserved_runtime_environment_names};
use crate::domain::target::NamedTarget;
use crate::domain::target::TargetId;
use crate::domain::target::validate_target_name;
use crate::domain::workspace::ProjectId;

use identifiers::{
    is_build_identifier, is_middleware_resource_name, is_oci_platform, looks_like_secret_name,
    looks_sensitive_filename,
};
pub(crate) use identifiers::{is_global_key, is_secret_name, is_variable_name};
use path_validation::validate_no_symlinks;

mod cluster_compose;
mod config_files;
mod development;
mod environment_ingress;
mod environments;
mod hardening;
mod health;
mod host_network;
mod identifiers;
mod ingress_mounts;
mod job_services;
mod jobs_schedules;
mod modules_services;
mod path_validation;
mod placement;
mod project;
mod release_inputs;
mod rollout;
mod scaling;
mod secret_store;
mod secrets_bindings;
mod shared_volumes;
mod stacks_networks;
mod user_namespace;

use cluster_compose::validate_cluster_compose;
use config_files::{
    validate_compose_environment, validate_config_files, validate_file_environments,
};
use development::validate_development;
use environment_ingress::validate_environment_ingress;
use environments::*;
use hardening::validate_hardening;
use health::validate_health;
use host_network::validate_host_network;
use ingress_mounts::*;
use job_services::validate_job_services;
use jobs_schedules::*;
use modules_services::*;
use placement::validate_placement;
use project::*;
pub use project::{
    compose_project_directory, compose_project_name, runtime_targets,
    target_scoped_compose_project_name,
};
use release_inputs::validate_release_inputs;
use rollout::validate_rollout;
use scaling::validate_scaling;
use secret_store::validate_secret_store;
use secrets_bindings::*;
use shared_volumes::validate_shared_volumes;
pub(crate) use stacks_networks::ingress_route_name;
use stacks_networks::*;
use user_namespace::validate_user_namespaces;

struct ValidationIndexes {
    configured_services: BTreeSet<(String, String)>,
}

struct ValidationContext<'a> {
    document: &'a ConfigDocument,
    environment: &'a str,
    automatic_local_ingress: bool,
    indexes: ValidationIndexes,
}

impl<'a> ValidationContext<'a> {
    fn new(document: &'a ConfigDocument, environment: &'a str) -> Self {
        let automatic_local_ingress = document.config.project.development_environment.as_deref()
            == Some(environment)
            && document.config.ingress.automatic_local != Some(false)
            && document
                .config
                .environments
                .get(environment)
                .is_some_and(|environment| environment.automatic_local_ingress != Some(false));
        let configured_services = document
            .config
            .modules
            .values()
            .flat_map(|module| {
                module
                    .services
                    .iter()
                    .map(|(service, config)| (config.stack.clone(), service.clone()))
            })
            .collect();
        Self {
            document,
            environment,
            automatic_local_ingress,
            indexes: ValidationIndexes {
                configured_services,
            },
        }
    }
}

#[must_use]
pub fn validate(document: &ConfigDocument, environment: &str) -> ValidationReport {
    let context = ValidationContext::new(document, environment);
    let mut report = ValidationReport::default();
    for key in &document.unknown_keys {
        report.push(key.diagnostic(&document.root));
    }
    validate_project(&context, &mut report);
    validate_stacks(&context, &mut report);
    validate_selected_environment(&context, &mut report);
    validate_modules(&context, &mut report);
    validate_secrets_and_bindings(&context, &mut report);
    validate_environment_ingress(document, &mut report);
    validate_release_inputs(document, &mut report);
    validate_secret_store(document, &mut report);
    validate_users(document, &mut report);
    validate_placement(&context, &mut report);
    validate_scaling(&context, &mut report);
    validate_health(&context, &mut report);
    validate_job_services(&context, &mut report);
    validate_development(&context, &mut report);
    validate_cluster_compose(&context, &mut report);
    validate_shared_volumes(&context, &mut report);
    validate_user_namespaces(&context, &mut report);
    validate_host_network(&context, &mut report);
    validate_rollout(&context, &mut report);
    validate_config_files(&context, &mut report);
    validate_file_environments(&context, &mut report);
    validate_compose_environment(&context, &mut report);
    validate_registries(document, &mut report);
    validate_hardening(&context, &mut report);
    report
}

/// Registry hosts are host names and their passwords name declared secrets.
fn validate_registries(document: &ConfigDocument, report: &mut ValidationReport) {
    for (host, registry) in &document.config.registries {
        let mut problems = Vec::new();
        if let Err(problem) = crate::domain::config::validate_registry_host(host) {
            problems.push(problem);
        }
        if registry.username.trim().is_empty() || registry.username.contains(char::is_whitespace) {
            problems.push(format!("registry '{host}' needs a username without spaces"));
        }
        if !document
            .config
            .secrets
            .contains_key(&registry.password.secret)
        {
            problems.push(format!(
                "registry '{host}' password names secret '{}', which secrets: does not declare",
                registry.password.secret
            ));
        }
        for problem in problems {
            report.push(
                Diagnostic::error("MM_REGISTRY_INVALID", problem)
                    .at_path(document.source("registries")),
            );
        }
    }
}

/// The users file: names, keys, and environments, checked locally before
/// `mmd` checks them again on reconcile.
fn validate_users(document: &ConfigDocument, report: &mut ValidationReport) {
    let environments = document
        .config
        .environments
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    for problem in crate::domain::users::validate_users(&document.config.users, &environments) {
        report
            .push(Diagnostic::error("MM_USERS_INVALID", problem).at_path(document.source("users")));
    }
    for problem in crate::domain::agents::validate_agents(&document.config.agents, &environments) {
        report.push(
            Diagnostic::error("MM_AGENTS_INVALID", problem).at_path(document.source("agents")),
        );
    }
    for (name, target) in &document.config.targets {
        if let Err(problem) = target.validate(name) {
            report.push(
                Diagnostic::error("MM_REPO_TARGET_INVALID", problem)
                    .at_path(document.source("targets")),
            );
            continue;
        }
        for provider in target.providers.keys() {
            let external = document
                .config
                .providers
                .get(provider)
                .is_some_and(|declared| {
                    declared.configuration.mode()
                        == crate::domain::secret_backend::ProviderMode::External
                });
            if !external {
                report.push(
                    Diagnostic::error(
                        "MM_REPO_TARGET_INVALID",
                        format!(
                            "target '{name}' providers names '{provider}', which is not an external provider under providers:"
                        ),
                    )
                    .at_path(document.source("targets")),
                );
            }
        }
        if let Some(deploy_key) = target
            .setup
            .as_ref()
            .and_then(|setup| setup.deploy_key.as_ref())
            && !document.config.secrets.contains_key(&deploy_key.secret)
        {
            report.push(
                Diagnostic::error(
                    "MM_REPO_TARGET_INVALID",
                    format!(
                        "target '{name}' setup deploy-key names secret '{}', which secrets: does not declare",
                        deploy_key.secret
                    ),
                )
                .at_path(document.source("targets")),
            );
        }
    }
    for (name, environment) in &document.config.environments {
        let Some(branches) = &environment.deploy_from else {
            continue;
        };
        let problem = if branches.is_empty() {
            Some("lists no branches".to_owned())
        } else {
            branches
                .iter()
                .find_map(|branch| crate::domain::branch_pattern::validate(branch).err())
        };
        if let Some(problem) = problem {
            report.push(
                Diagnostic::error(
                    "MM_DEPLOY_FROM_INVALID",
                    format!(
                        "environment '{name}' deploy-from {problem}; list branch names, patterns such as \"*.x\", or \"*\""
                    ),
                )
                .at_path(document.source("environments")),
            );
        }
    }
}
