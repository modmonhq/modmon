use super::*;

pub(super) fn validate_project(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    let automatic_local_ingress = context.automatic_local_ingress;
    validate_name(
        &document.config.project.name,
        "project name",
        document.source("project"),
        report,
    );
    if automatic_local_ingress
        && let Err(message) = ProjectSlug::normalize(&document.config.project.name)
    {
        report.push(
            Diagnostic::error("MM_PROJECT_SLUG_INVALID", message)
                .at_path(document.source("project")),
        );
    }
    if let Some(default_environment) = &document.config.default_environment
        && !document
            .config
            .environments
            .contains_key(default_environment)
    {
        report.push(
            Diagnostic::error(
                "MM_DEFAULT_ENVIRONMENT_UNKNOWN",
                format!("default environment '{default_environment}' is not configured"),
            )
            .at_path(document.source("default-environment")),
        );
    }
    if let Some(development_environment) = &document.config.project.development_environment
        && !document
            .config
            .environments
            .contains_key(development_environment)
    {
        report.push(
            Diagnostic::error(
                "MM_DEVELOPMENT_ENVIRONMENT_UNKNOWN",
                format!("development environment '{development_environment}' is not configured"),
            )
            .at_path(document.source("project")),
        );
    }
    validate_network_catalog(document, report);
    validate_jobs_schedules_and_gates(document, report);
    for key in document.config.development_mail.bindings.keys() {
        if !is_variable_name(key) {
            report.push(
                Diagnostic::error(
                    "MM_DEVELOPMENT_MAIL_KEY_INVALID",
                    format!(
                        "development mail binding '{key}' is not a valid environment variable name"
                    ),
                )
                .at_path(document.source("development-mail")),
            );
        }
    }

    if let Err(message) = validate_target_name(&document.config.target) {
        report.push(
            Diagnostic::error("MM_TARGET_NAME_INVALID", message).at_path(document.source("target")),
        );
    }
    for name in document.config.providers.keys() {
        if matches!(name.as_str(), "age" | "env" | "file") {
            report.push(
                Diagnostic::error(
                    "MM_PROVIDER_INSTANCE_RESERVED",
                    format!(
                        "provider instance name '{name}' is reserved for a built-in secret storage provider"
                    ),
                )
                .at_path(document.source("providers")),
            );
        }
    }
}

#[must_use]
pub fn runtime_targets(
    document: &ConfigDocument,
    environment: &str,
    target: Option<&NamedTarget>,
) -> Vec<RuntimeTarget> {
    document
        .config
        .stacks
        .iter()
        .map(|(stack_name, stack)| RuntimeTarget {
            root: document.root.clone(),
            project_directory: compose_project_directory(document, stack_name),
            target: target.cloned(),
            project_id: document.config.project.id,
            environment: environment.to_owned(),
            stack_name: stack_name.clone(),
            project_name: {
                let base = compose_project_name(document, environment, stack_name);
                target.map_or(base.clone(), |target| {
                    target_scoped_compose_project_name(
                        &base,
                        target.definition.id,
                        document.config.project.id,
                        environment,
                        stack_name,
                    )
                })
            },
            artifact_hash: String::new(),
            compose_files: stack.files.clone(),
            user_compose_file_count: stack.files.len(),
            interpolation_environment: Default::default(),
            installation_scoped: false,
            registry_credentials: Vec::new(),
        })
        .collect()
}

#[must_use]
pub fn compose_project_directory(
    document: &ConfigDocument,
    stack_name: &str,
) -> std::path::PathBuf {
    document
        .config
        .stacks
        .get(stack_name)
        .and_then(|stack| stack.files.first())
        .and_then(|file| document.root.join(file).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| document.root.clone())
}

pub fn compose_project_name(
    document: &ConfigDocument,
    environment: &str,
    stack_name: &str,
) -> String {
    let project_id = document.config.project.id.0.to_string();
    let default_project_name = format!("mm-{project_id}-{environment}-{stack_name}");
    document
        .config
        .stacks
        .get(stack_name)
        .and_then(|stack| stack.project_name.as_deref())
        .map_or(default_project_name, |configured| {
            configured
                .replace("{environment}", environment)
                .replace("{stack}", stack_name)
                .replace("{project-id}", &project_id)
        })
}

pub fn target_scoped_compose_project_name(
    base: &str,
    target_id: TargetId,
    project_id: ProjectId,
    environment: &str,
    stack: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [
        target_id.0.to_string(),
        project_id.0.to_string(),
        environment.to_owned(),
        stack.to_owned(),
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    format!("{base}-mm-{}", &hasher.finalize().to_hex()[..16])
}

pub(super) fn is_compose_project_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
}

#[cfg(test)]
mod tests;
