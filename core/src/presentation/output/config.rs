use super::*;

pub(crate) fn write_initialized(
    initialized: &InitializedProject,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(initialized);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Initialized {}",
        terminal_safe(&initialized.project_name)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  root:   {}",
        terminal_safe(&initialized.root.display().to_string())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  config: {}",
        terminal_safe(&initialized.config_path.display().to_string())
    )
    .map_err(output_error)?;
    for starter in &initialized.starter_files {
        writeln!(
            writer,
            "  starter: {}",
            terminal_safe(&starter.display().to_string())
        )
        .map_err(output_error)?;
    }
    for proposal in &initialized.mount_proposals {
        writeln!(
            writer,
            "  mount proposal: {} ({:?}, confirmation required)",
            terminal_safe(&proposal.module),
            proposal.confidence
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_initialization_plan(
    plan: &crate::domain::onboarding::InitializationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Project initialization plan {}",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    for path in &plan.affected_files {
        writeln!(
            writer,
            "  affected: {}",
            terminal_safe(&path.display().to_string())
        )
        .map_err(output_error)?;
    }
    write_terminal_safe_multiline(&mut writer, &plan.canonical_diff)?;
    writeln!(
        writer,
        "Apply with `mm init --name {} --plan {} --yes`.",
        terminal_safe(&plan.project_name),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

pub(crate) fn write_config_mutation_plan(
    plan: &crate::domain::onboarding::ConfigMutationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Configuration plan {}",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(writer, "  affected files:").map_err(output_error)?;
    for path in &plan.affected_files {
        writeln!(writer, "    {}", terminal_safe(&path.display().to_string()))
            .map_err(output_error)?;
    }
    for resource in &plan.generated_resources {
        writeln!(writer, "  resource: {}", terminal_safe(resource)).map_err(output_error)?;
    }
    for route in &plan.generated_routes {
        writeln!(writer, "  route: {}", terminal_safe(route)).map_err(output_error)?;
    }
    for prompt in &plan.privilege_prompts {
        writeln!(writer, "  privilege: {}", terminal_safe(prompt)).map_err(output_error)?;
    }
    write_terminal_safe_multiline(&mut writer, &plan.canonical_diff)?;
    writeln!(
        writer,
        "Review complete. Apply with --plan {} --yes.",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

pub(crate) fn write_config_mutation_result(
    result: &crate::domain::onboarding::ConfigMutationResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Applied configuration plan {}",
        terminal_safe(&result.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  config: {}",
        terminal_safe(&result.config_path.display().to_string())
    )
    .map_err(output_error)
}

pub(crate) fn write_config_layout_plan(
    plan: &crate::domain::onboarding::ConfigLayoutPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Configuration layout plan {}",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(writer, "  affected files:").map_err(output_error)?;
    for path in &plan.affected_files {
        writeln!(writer, "    {}", terminal_safe(&path.display().to_string()))
            .map_err(output_error)?;
    }
    for note in &plan.notes {
        writeln!(writer, "  note: {}", terminal_safe(note)).map_err(output_error)?;
    }
    write_terminal_safe_multiline(&mut writer, &plan.canonical_diff)?;
    writeln!(
        writer,
        "The configuration means the same after this move. Apply with `mm config {} {} --plan {} --yes`.",
        plan.request.action.label(),
        terminal_safe(&plan.request.key),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

pub(crate) fn write_config_layout_result(
    result: &crate::domain::onboarding::ConfigLayoutResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Applied configuration layout plan {}",
        terminal_safe(&result.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  {}: {}",
        terminal_safe(&result.request.key),
        terminal_safe(&result.config_path.display().to_string())
    )
    .map_err(output_error)
}

#[derive(Debug, serde::Serialize)]
struct ConfigSourcesView {
    root: std::path::PathBuf,
    anchor: std::path::PathBuf,
    keys: Vec<ConfigSourceView>,
}

#[derive(Debug, serde::Serialize)]
struct ConfigSourceView {
    key: &'static str,
    /// Project-relative file that declares the key; absent when the key is
    /// left at its default.
    file: Option<std::path::PathBuf>,
    split: bool,
}

pub(crate) fn write_config_sources(
    document: &crate::domain::config::ConfigDocument,
    format: OutputFormat,
) -> Result<(), MmError> {
    let relative = |path: &std::path::Path| {
        path.strip_prefix(&document.root)
            .unwrap_or(path)
            .to_path_buf()
    };
    let keys = crate::domain::config::SPLITTABLE_CONFIG_KEYS
        .iter()
        .map(|key| {
            let file = document.sources.get(*key);
            ConfigSourceView {
                key,
                file: file.map(|path| relative(path)),
                split: file.is_some_and(|path| *path != document.path),
            }
        })
        .collect::<Vec<_>>();
    let view = ConfigSourcesView {
        root: document.root.clone(),
        anchor: relative(&document.path),
        keys,
    };
    if format == OutputFormat::Json {
        return write_json(&view);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    let width = view
        .keys
        .iter()
        .map(|entry| entry.key.len())
        .max()
        .unwrap_or(0);
    for entry in &view.keys {
        let file = entry
            .file
            .as_ref()
            .map_or_else(|| "(default)".to_owned(), |path| path.display().to_string());
        writeln!(writer, "{:<width$}  {}", entry.key, terminal_safe(&file))
            .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_workflow_review(
    review: &crate::domain::onboarding::WorkflowReview,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(review);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "{}", terminal_safe(&review.title)).map_err(output_error)?;
    writeln!(writer, "  plan: {}", terminal_safe(&review.plan_hash)).map_err(output_error)?;
    for proposal in &review.proposals {
        writeln!(
            writer,
            "  proposal: {} = {} ({:?})",
            terminal_safe(&proposal.subject),
            terminal_safe(&proposal.proposed_value),
            proposal.confidence
        )
        .map_err(output_error)?;
        for reason in &proposal.reasons {
            writeln!(writer, "    because: {}", terminal_safe(reason)).map_err(output_error)?;
        }
    }
    if !review.canonical_diff.is_empty() {
        write_terminal_safe_multiline(&mut writer, &review.canonical_diff)?;
    }
    for prompt in &review.privilege_prompts {
        writeln!(writer, "  privilege: {}", terminal_safe(prompt)).map_err(output_error)?;
    }
    for action in &review.next_actions {
        writeln!(writer, "  next: {}", terminal_safe(action)).map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_dashboard_links(project_name: &str) -> Result<(), MmError> {
    let slug = project_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Dashboard: https://mm.test").map_err(output_error)?;
    writeln!(
        writer,
        "Project:   https://mm.{}.test",
        terminal_safe(if slug.is_empty() { "project" } else { slug })
    )
    .map_err(output_error)?;
    writeln!(writer, "Mail:      https://mail.mm.test").map_err(output_error)
}
