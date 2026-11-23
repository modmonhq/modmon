use super::*;
use crate::domain::build::BuiltImage;

/// Where a reviewed build puts its images.
fn build_mode(plan: &ImageBuildPlan) -> String {
    match (plan.publish, plan.engine) {
        (true, _) => "publish".to_owned(),
        (false, Some(engine)) => format!("load into {}, nothing pushed", engine.name()),
        (false, None) => "OCI archive".to_owned(),
    }
}

/// A local image by the tag it runs under; a published one by digest.
fn built_reference(image: &BuiltImage) -> String {
    match image.tags.first() {
        Some(tag) if crate::domain::build::is_local_image_reference(tag) => {
            format!("{tag} ({})", image.digest)
        }
        _ => format!("{}@{}", image.repository, image.digest),
    }
}

/// What a deployment builds before it starts anything, and why.
pub(crate) fn local_build_line(review: &crate::domain::build::LocalBuildReview) -> String {
    let modules = review
        .modules
        .iter()
        .map(|rebuild| format!("{} ({})", rebuild.module, rebuild.reason.describe()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("into {} first: {modules}", review.engine.name())
}

/// The summary line of a finished build.
pub(crate) fn built_images_line(result: &ImageBuildResult) -> String {
    let reused = result.images.iter().filter(|image| image.reused).count();
    let built = result.images.len() - reused;
    let images = |count: usize| if count == 1 { "image" } else { "images" };
    match (built, reused) {
        (_, 0) => format!("Built {built} {}", images(built)),
        (0, _) => format!("Kept {reused} unchanged {}", images(reused)),
        _ => format!("Built {built} {}, kept {reused} unchanged", images(built)),
    }
}

pub(crate) fn write_image_build_plan(
    plan: &ImageBuildPlan,
    published_for: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Image build plan {}",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  source: {}{}",
        terminal_safe(&plan.source.commit),
        if plan.source.dirty { " (dirty)" } else { "" }
    )
    .map_err(output_error)?;
    writeln!(writer, "  config: {}", terminal_safe(&plan.config_hash)).map_err(output_error)?;
    writeln!(
        writer,
        "  mode: {} on {}",
        build_mode(plan),
        terminal_safe(&plan.platforms.join(", "))
    )
    .map_err(output_error)?;
    for unit in &plan.units {
        writeln!(
            writer,
            "  module: {} -> {} ({})",
            terminal_safe(&unit.module),
            terminal_safe(&unit.image),
            terminal_safe(&unit.context_hash)
        )
        .map_err(output_error)?;
        if let Some(environments) = published_for.get(&unit.module) {
            let environments = environments.iter().cloned().collect::<Vec<_>>();
            writeln!(
                writer,
                "    for: {}",
                terminal_safe(&environments.join(", "))
            )
            .map_err(output_error)?;
        }
        if !unit.dependencies.is_empty() {
            let deps = unit
                .dependencies
                .iter()
                .map(|dependency| {
                    let via = match &dependency.reference {
                        crate::domain::build::BuildDependencyReference::BuildArgument {
                            arg,
                            ..
                        } => format!("ARG {arg}"),
                        crate::domain::build::BuildDependencyReference::Dockerfile {
                            instruction_index,
                            source_line,
                            ..
                        } => format!("FROM #{instruction_index} line {source_line}"),
                    };
                    format!("{} via {via}", dependency.producer_module)
                })
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(writer, "    depends-on: {}", terminal_safe(&deps)).map_err(output_error)?;
        }
        for variant in &unit.variants {
            writeln!(
                writer,
                "    target: {}{} [{}]",
                terminal_safe(variant.name()),
                variant
                    .stage
                    .as_deref()
                    .map_or_else(String::new, |stage| format!(
                        " (Dockerfile target {})",
                        terminal_safe(stage)
                    )),
                terminal_safe(&variant.tags.join(", "))
            )
            .map_err(output_error)?;
            super::build_receipt::write_build_args(&mut writer, unit, variant, "      ")?;
        }
        if !unit.secret_ids.is_empty() {
            writeln!(writer, "    build secrets: {}", unit.secret_ids.len())
                .map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) fn write_image_build_result(
    result: &ImageBuildResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Completed image build {}",
        terminal_safe(&result.plan_hash)
    )
    .map_err(output_error)?;
    for image in &result.images {
        writeln!(
            writer,
            "  {}:{} -> {}{}",
            terminal_safe(&image.module),
            image
                .target
                .as_deref()
                .map_or_else(|| "default".to_owned(), terminal_safe),
            terminal_safe(&built_reference(image)),
            if image.reused { " (unchanged)" } else { "" }
        )
        .map_err(output_error)?;
        if let Some(archive) = &image.archive {
            writeln!(
                writer,
                "    archive: {}",
                terminal_safe(&archive.display().to_string())
            )
            .map_err(output_error)?;
        }
    }
    for dependency in &result.dependencies {
        writeln!(
            writer,
            "  dependency: {} -> {}:{} = {}",
            terminal_safe(&dependency.edge.producer_module),
            terminal_safe(&dependency.edge.dependent_module),
            dependency
                .dependent_target
                .as_deref()
                .map_or_else(|| "default".to_owned(), terminal_safe),
            terminal_safe(&dependency.immutable_reference)
        )
        .map_err(output_error)?;
    }
    Ok(())
}
