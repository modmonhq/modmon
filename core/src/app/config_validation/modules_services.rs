use super::*;

type ServiceOwners<'a> = BTreeMap<(&'a str, &'a str), &'a str>;

pub(super) fn validate_modules(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    validate_build_platforms(context.document, report);
    let mut service_owners = BTreeMap::new();
    let mut service_slug_owners = BTreeMap::new();
    for (module_name, module) in &context.document.config.modules {
        let module_path = validate_module_path(context.document, module_name, module, report);
        if let Some(build) = &module.build
            && build.enabled
        {
            validate_build_declaration(context.document, module_name, build, &module_path, report);
            validate_build_inputs(context.document, module_name, build, report);
            validate_build_dependencies(context.document, module_name, build, report);
            validate_extra_inputs(context.document, module_name, build, report);
        }
        for (service, service_config) in &module.services {
            validate_service_declaration(
                context,
                module_name,
                service,
                service_config,
                &mut service_owners,
                &mut service_slug_owners,
                report,
            );
            validate_service_delivery(context.document, service, service_config, report);
        }
        validate_module_environment(context.document, module_name, module, &module_path, report);
    }
    validate_build_cycles(context.document, report);
}

fn validate_build_platforms(document: &ConfigDocument, report: &mut ValidationReport) {
    for platform in &document.config.build.platforms {
        if !is_oci_platform(platform) {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_PLATFORM_INVALID",
                    format!(
                        "build platform '{platform}' must use a lowercase os/architecture[/variant] form"
                    ),
                )
                .at_path(document.source("build")),
            );
        }
    }
}

fn validate_module_path(
    document: &ConfigDocument,
    module_name: &str,
    module: &ModuleConfig,
    report: &mut ValidationReport,
) -> std::path::PathBuf {
    let config_file = document.source("modules");
    validate_name(module_name, "module name", config_file, report);
    validate_relative_path(&module.path, "module path", config_file, report);

    let module_path = document.root.join(&module.path);
    validate_no_symlinks(&module_path, "module path", report);
    if !module_path.is_dir() {
        report.push(
            Diagnostic::error(
                "MM_MODULE_PATH_MISSING",
                format!(
                    "module '{module_name}' directory does not exist: {}",
                    module_path.display()
                ),
            )
            .at_path(&module_path),
        );
    }
    if let Some(mount) = &module.development_mount
        && let Err(error) =
            crate::app::development_mount::validate_intent(document, module_name, module, mount)
    {
        report.push(
            Diagnostic::error(
                "MM_DEVELOPMENT_MOUNT_INVALID",
                format!("module '{module_name}' development mount is unsafe: {error}"),
            )
            .at_path(config_file),
        );
    }

    validate_network_attachments(
        document,
        config_file,
        &module.networks,
        &format!("module '{module_name}'"),
        report,
    );
    module_path
}

fn validate_build_declaration(
    document: &ConfigDocument,
    module_name: &str,
    build: &crate::domain::config::ModuleBuildConfig,
    module_path: &Path,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    if build.image.as_deref().is_none_or(str::is_empty) {
        report.push(
            Diagnostic::error(
                "MM_BUILD_IMAGE_REQUIRED",
                format!(
                    "module '{module_name}' has an enabled build but no authoritative build.image"
                ),
            )
            .at_path(config_file),
        );
    }
    if let Some(variant) = &build.default_variant
        && !build.targets.contains_key(variant)
    {
        report.push(
                    Diagnostic::error(
                        "MM_BUILD_DEFAULT_VARIANT_UNKNOWN",
                        format!(
                            "module '{module_name}' default-variant '{variant}' is not a configured build target"
                        ),
                    )
                    .at_path(config_file),
                );
    }
    if let Some(context) = &build.context {
        validate_relative_path(context, "build context", config_file, report);
        validate_no_symlinks(&module_path.join(context), "build context", report);
    }
    if let Some(dockerfile) = &build.dockerfile {
        validate_relative_path(dockerfile, "build Dockerfile", config_file, report);
        validate_no_symlinks(&module_path.join(dockerfile), "build Dockerfile", report);
    }
    validate_build_targets(document, module_name, build, report);
}

fn validate_build_inputs(
    document: &ConfigDocument,
    module_name: &str,
    build: &crate::domain::config::ModuleBuildConfig,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    validate_build_args(
        document,
        &format!("module '{module_name}'"),
        &build.args,
        report,
    );
    for (secret_id, secret_name) in &build.secrets {
        if !is_build_identifier(secret_id) {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_SECRET_ID_INVALID",
                    format!(
                        "module '{module_name}' build secret id '{secret_id}' is not BuildKit-safe"
                    ),
                )
                .at_path(config_file),
            );
        }
        if !document.config.secrets.contains_key(secret_name) {
            report.push(
                        Diagnostic::error(
                            "MM_BUILD_SECRET_UNKNOWN",
                            format!(
                                "module '{module_name}' build secret '{secret_id}' references unknown secret '{secret_name}'"
                            ),
                        )
                        .at_path(config_file),
                    );
        }
    }
    let mut credentials = std::collections::BTreeSet::new();
    for secret_id in &build.credential_only_secrets {
        let (code, problem) = if !build.secrets.contains_key(secret_id) {
            (
                "MM_BUILD_CREDENTIAL_SECRET_UNKNOWN",
                "is not a key of build.secrets",
            )
        } else if !credentials.insert(secret_id) {
            (
                "MM_BUILD_CREDENTIAL_SECRET_DUPLICATE",
                "is listed more than once",
            )
        } else {
            continue;
        };
        let message =
            format!("module '{module_name}' credential-only secret '{secret_id}' {problem}");
        report.push(Diagnostic::error(code, message).at_path(config_file));
    }
}

fn validate_build_dependencies(
    document: &ConfigDocument,
    module_name: &str,
    build: &crate::domain::config::ModuleBuildConfig,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    let mut dependency_args = BTreeSet::new();
    for (dependency, arg) in &build.depends_on {
        if dependency == module_name {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_DEPENDENCY_SELF",
                    format!("module '{module_name}' cannot depend on itself"),
                )
                .at_path(config_file),
            );
        }
        match document.config.modules.get(dependency) {
                    Some(upstream)
                        if upstream
                            .build
                            .as_ref()
                            .is_some_and(|build| build.enabled) => {}
                    Some(_) => report.push(
                        Diagnostic::error(
                            "MM_BUILD_DEPENDENCY_DISABLED",
                            format!(
                                "module '{module_name}' depends on '{dependency}', which is not an enabled build"
                            ),
                        )
                        .at_path(config_file),
                    ),
                    None => report.push(
                        Diagnostic::error(
                            "MM_BUILD_DEPENDENCY_UNKNOWN",
                            format!(
                                "module '{module_name}' depends on unknown module '{dependency}'"
                            ),
                        )
                        .at_path(config_file),
                    ),
                }
        if !is_variable_name(arg) {
            report.push(
                        Diagnostic::error(
                            "MM_BUILD_DEPENDENCY_ARG_INVALID",
                            format!(
                                "module '{module_name}' dependency argument '{arg}' is not a valid variable name"
                            ),
                        )
                        .at_path(config_file),
                    );
        } else if arg == "VERSION" {
            report.push(
                        Diagnostic::error(
                            "MM_BUILD_ARG_RESERVED",
                            format!(
                                "module '{module_name}' dependency argument 'VERSION' is supplied automatically from Git"
                            ),
                        )
                        .at_path(config_file),
                    );
        } else if sets_build_arg(build, arg) {
            report.push(
                        Diagnostic::error(
                            "MM_BUILD_DEPENDENCY_ARG_CONFLICT",
                            format!(
                                "module '{module_name}' dependency argument '{arg}' conflicts with a public build argument"
                            ),
                        )
                        .at_path(config_file),
                    );
        } else if looks_like_secret_name(arg) {
            report.push(
                        Diagnostic::error(
                            "MM_BUILD_SECRET_ARG_FORBIDDEN",
                            format!(
                                "module '{module_name}' dependency argument '{arg}' looks sensitive; use build.secrets"
                            ),
                        )
                        .at_path(config_file),
                    );
        }
        if !dependency_args.insert(arg) {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_DEPENDENCY_ARG_DUPLICATE",
                    format!(
                        "module '{module_name}' assigns dependency argument '{arg}' more than once"
                    ),
                )
                .at_path(config_file),
            );
        }
    }
}

fn validate_extra_inputs(
    document: &ConfigDocument,
    module_name: &str,
    build: &crate::domain::config::ModuleBuildConfig,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    let mut extra_inputs = BTreeSet::new();
    for input in &build.extra_inputs {
        validate_relative_path(input, "build extra input", config_file, report);
        if !extra_inputs.insert(input) {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_EXTRA_INPUT_DUPLICATE",
                    format!(
                        "module '{module_name}' lists extra rebuild input '{}' more than once",
                        input.display()
                    ),
                )
                .at_path(config_file),
            );
        }
        let extra_path = document.root.join(input);
        validate_no_symlinks(&extra_path, "build extra input", report);
        if extra_path.exists() && !extra_path.is_file() {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_EXTRA_INPUT_INVALID",
                    format!(
                        "module '{module_name}' extra rebuild input '{}' must be a regular file",
                        input.display()
                    ),
                )
                .at_path(&extra_path),
            );
        } else if !extra_path.is_file() {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_EXTRA_INPUT_MISSING",
                    format!(
                        "module '{module_name}' extra rebuild input '{}' does not exist",
                        input.display()
                    ),
                )
                .at_path(&extra_path),
            );
        }
        if extra_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(looks_sensitive_filename)
        {
            report.push(
                        Diagnostic::error(
                            "MM_BUILD_EXTRA_INPUT_SENSITIVE",
                            format!(
                                "module '{module_name}' extra rebuild input '{}' looks like secret material",
                                input.display()
                            ),
                        )
                        .at_path(&extra_path),
                    );
        }
    }
}

fn validate_service_declaration<'a>(
    context: &ValidationContext<'_>,
    module_name: &'a str,
    service: &'a str,
    service_config: &'a crate::domain::config::ServiceConfig,
    service_owners: &mut ServiceOwners<'a>,
    service_slug_owners: &mut BTreeMap<String, String>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let config_file = document.source("modules");
    let automatic_local_ingress = context.automatic_local_ingress;
    validate_name(service, "service name", config_file, report);
    validate_service_environments(document, module_name, service, service_config, report);
    let automatic_service = automatic_local_ingress
        && service_config.is_active(context.environment)
        && service_config.takes_automatic_local_ingress();
    validate_service_slug(
        module_name,
        service,
        service_config,
        automatic_service,
        config_file,
        service_slug_owners,
        report,
    );
    if service_config.http_port == Some(0) {
        report.push(
            Diagnostic::error(
                "MM_SERVICE_HTTP_PORT_INVALID",
                format!("service '{service}' HTTP port must be non-zero"),
            )
            .at_path(config_file),
        );
    }
    if service_config.local_https == Some(false) {
        report.push(
            Diagnostic::warning(
                "MM_INGRESS_HTTP_OPT_OUT",
                format!(
                    "service '{service}' explicitly disables HTTPS for its automatic local route"
                ),
            )
            .at_path(config_file)
            .with_help("remove local-https: false to use trusted HTTPS"),
        );
    }
    validate_network_attachments(
        document,
        config_file,
        &service_config.networks,
        &format!("module '{module_name}' service '{service}'"),
        report,
    );
    if !document.config.stacks.contains_key(&service_config.stack) {
        report.push(
            Diagnostic::error(
                "MM_SERVICE_STACK_UNKNOWN",
                format!(
                    "service '{service}' in module '{module_name}' references unknown stack '{}'",
                    service_config.stack
                ),
            )
            .at_path(config_file),
        );
    }
    if let Some(owner) = service_owners.insert((&service_config.stack, service), module_name) {
        report.push(
                    Diagnostic::error(
                        "MM_SERVICE_DUPLICATE",
                        format!(
                            "service '{service}' in stack '{}' is assigned to both module '{owner}' and '{module_name}'",
                            service_config.stack
                        ),
                    )
                    .at_path(config_file),
                );
    }
}

/// Automatic local routes derive a host from the service name, so it must
/// normalize to a slug that no other routed service already uses.
fn validate_service_slug(
    module_name: &str,
    service: &str,
    service_config: &crate::domain::config::ServiceConfig,
    automatic_service: bool,
    config_file: &Path,
    service_slug_owners: &mut BTreeMap<String, String>,
    report: &mut ValidationReport,
) {
    match ServiceSlug::normalize(service) {
        Ok(slug) => {
            if automatic_service {
                let owner = format!(
                    "module '{module_name}' service '{service}' in stack '{}'",
                    service_config.stack
                );
                if let Some(previous) = service_slug_owners.insert(slug.to_string(), owner.clone())
                {
                    report.push(
                                Diagnostic::error(
                                    "MM_SERVICE_SLUG_COLLISION",
                                    format!(
                                        "{previous} and {owner} both normalize to local ingress slug '{slug}'"
                                    ),
                                )
                                .at_path(config_file)
                                .with_help(
                                    "rename one service so generated local hosts remain unique",
                                ),
                            );
                }
            }
        }
        Err(message) if automatic_service => {
            report.push(Diagnostic::error("MM_SERVICE_SLUG_INVALID", message).at_path(config_file))
        }
        Err(_) => {}
    }
}

fn validate_service_environments(
    document: &ConfigDocument,
    module: &str,
    name: &str,
    service: &crate::domain::config::ServiceConfig,
    report: &mut ValidationReport,
) {
    for allowed in &service.environments {
        if !document.config.environments.contains_key(allowed) {
            report.push(
                Diagnostic::error(
                    "MM_SERVICE_ENVIRONMENT_UNKNOWN",
                    format!("service '{module}/{name}' allowlists unknown environment '{allowed}'"),
                )
                .at_path(document.source("modules")),
            );
        }
    }
}

fn validate_service_delivery(
    document: &ConfigDocument,
    service: &str,
    service_config: &crate::domain::config::ServiceConfig,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    for (variable, value) in &service_config.env {
        if !is_variable_name(variable) {
            report.push(
                        Diagnostic::error(
                            "MM_ENV_VARIABLE_INVALID",
                            format!(
                                "service '{service}' env '{variable}' is not a valid environment variable name"
                            ),
                        )
                        .at_path(config_file),
                    );
        }
        if !is_safe_environment_value(&value.to_string()) {
            report.push(
                Diagnostic::error(
                    "MM_ENV_VALUE_INVALID",
                    format!("service '{service}' env '{variable}' contains control characters"),
                )
                .at_path(config_file),
            );
        }
    }
    for (target, secret_file) in &service_config.secret_files {
        if let Err(message) =
            crate::domain::secret_contract::MaterializationName::file(target.clone())
        {
            report.push(
                Diagnostic::error("MM_SECRET_FILE_TARGET_INVALID", message).at_path(config_file),
            );
        }
        if !is_secret_name(&secret_file.secret)
            || !document.config.secrets.contains_key(&secret_file.secret)
        {
            report.push(
                        Diagnostic::error(
                            "MM_SECRET_FILE_UNKNOWN",
                            format!(
                                "service '{service}' secret file '{target}' references unknown logical secret '{}'",
                                secret_file.secret
                            ),
                        )
                        .at_path(config_file),
                    );
        }
        if secret_file.uid.is_empty()
            || !secret_file.uid.bytes().all(|byte| byte.is_ascii_digit())
            || secret_file.gid.is_empty()
            || !secret_file.gid.bytes().all(|byte| byte.is_ascii_digit())
            || secret_file.mode == 0
            || secret_file.mode & !0o777 != 0
        {
            report.push(
                        Diagnostic::error(
                            "MM_SECRET_FILE_PERMISSIONS_INVALID",
                            format!(
                                "service '{service}' secret file '{target}' requires numeric uid/gid and a non-zero Unix mode"
                            ),
                        )
                        .at_path(config_file),
                    );
        }
    }
}

fn validate_module_environment(
    document: &ConfigDocument,
    module_name: &str,
    module: &ModuleConfig,
    module_path: &Path,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    if module.env.is_none()
        && module
            .services
            .values()
            .any(|service| !service.env.is_empty() || !service.secret_files.is_empty())
    {
        report.push(
                Diagnostic::error(
                    "MM_ENV_WITHOUT_TEMPLATE",
                    format!(
                        "module '{module_name}' has service env or secret-file values but no environment template"
                    ),
                )
                .at_path(config_file),
            );
    }

    if let Some(env) = &module.env {
        if module.services.is_empty() {
            report.push(
                Diagnostic::error(
                    "MM_MODULE_SERVICES_EMPTY",
                    format!("module '{module_name}' has an environment template but no services"),
                )
                .at_path(config_file),
            );
        }
        validate_relative_path(&env.template, "environment template", config_file, report);
        let template = module_path.join(&env.template);
        validate_no_symlinks(&template, "environment template", report);
        if !template.is_file() {
            report.push(
                Diagnostic::error(
                    "MM_ENV_TEMPLATE_MISSING",
                    format!(
                        "environment template for module '{module_name}' does not exist: {}",
                        template.display()
                    ),
                )
                .at_path(template),
            );
        }
    }
}

fn validate_build_cycles(document: &ConfigDocument, report: &mut ValidationReport) {
    if let Some(cycle) = build_dependency_cycle(&document.config.modules) {
        report.push(
            Diagnostic::error(
                "MM_BUILD_DEPENDENCY_CYCLE",
                format!(
                    "module build dependencies form a cycle: {}",
                    cycle.join(" -> ")
                ),
            )
            .at_path(document.source("modules")),
        );
    }
}

pub(super) fn validate_name(name: &str, kind: &str, path: &Path, report: &mut ValidationReport) {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if !valid {
        report.push(
            Diagnostic::error(
                "MM_NAME_INVALID",
                format!("{kind} '{name}' must contain only ASCII letters, numbers, '-' or '_'"),
            )
            .at_path(path),
        );
    }
}

pub(super) fn build_dependency_cycle(
    modules: &BTreeMap<String, ModuleConfig>,
) -> Option<Vec<String>> {
    let mut graph = BTreeMap::<&str, Vec<&str>>::new();
    for (name, module) in modules {
        if let Some(build) = module.build.as_ref().filter(|build| build.enabled) {
            graph.insert(
                name.as_str(),
                build.depends_on.keys().map(String::as_str).collect(),
            );
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    for name in graph.keys().copied() {
        if let Some(cycle) =
            visit_build_dependency(name, &graph, &mut visiting, &mut visited, &mut stack)
        {
            return Some(cycle);
        }
    }
    None
}

pub(super) fn visit_build_dependency<'a>(
    name: &'a str,
    graph: &BTreeMap<&'a str, Vec<&'a str>>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
    stack: &mut Vec<&'a str>,
) -> Option<Vec<String>> {
    if visited.contains(name) {
        return None;
    }
    if !visiting.insert(name) {
        let start = stack.iter().position(|candidate| *candidate == name)?;
        let mut cycle = stack[start..]
            .iter()
            .map(|candidate| (*candidate).to_owned())
            .collect::<Vec<_>>();
        cycle.push(name.to_owned());
        return Some(cycle);
    }
    stack.push(name);
    if let Some(dependencies) = graph.get(name) {
        for dependency in dependencies {
            if let Some(cycle) = visit_build_dependency(dependency, graph, visiting, visited, stack)
            {
                return Some(cycle);
            }
        }
    }
    stack.pop();
    visiting.remove(name);
    visited.insert(name);
    None
}

mod build_targets;
use build_targets::{sets_build_arg, validate_build_args, validate_build_targets};

#[cfg(test)]
mod tests;
