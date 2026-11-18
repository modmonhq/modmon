use super::*;
use crate::domain::build::{BuildReceipt, ImageBuildUnitPlan, ImageBuildVariantPlan};

/// The build a remote plan deploys, when it came with a receipt.
pub(super) fn write_receipt_lines(
    writer: &mut impl Write,
    receipt: Option<&BuildReceipt>,
) -> Result<(), MmError> {
    let Some(receipt) = receipt else {
        return Ok(());
    };
    writeln!(
        writer,
        "  build:    receipt {} ({} image(s), built for {})",
        terminal_safe(&receipt.receipt_hash),
        receipt.result.images.len(),
        terminal_safe(&receipt.plan.environment)
    )
    .map_err(output_error)?;
    write_receipt_images(writer, receipt)
}

/// The receipt a deployment plan deploys, and the environment it was built for.
pub(super) fn write_receipt_origin_line(
    writer: &mut impl Write,
    origin: Option<&crate::domain::build::BuildReceiptOrigin>,
) -> Result<(), MmError> {
    let Some(origin) = origin else {
        return Ok(());
    };
    writeln!(
        writer,
        "  build receipt: {} (built for {} from {})",
        terminal_safe(&origin.receipt_hash),
        terminal_safe(&origin.environment),
        terminal_safe(&origin.commit)
    )
    .map_err(output_error)
}

pub(super) fn write_build_args(
    writer: &mut impl Write,
    unit: &ImageBuildUnitPlan,
    variant: &ImageBuildVariantPlan,
    indent: &str,
) -> Result<(), MmError> {
    let args = unit
        .variant_args(variant)
        .into_iter()
        .filter(|(name, _)| name != "VERSION")
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>();
    if args.is_empty() {
        return Ok(());
    }
    writeln!(writer, "{indent}args: {}", terminal_safe(&args.join(" "))).map_err(output_error)
}

fn write_receipt_images(writer: &mut impl Write, receipt: &BuildReceipt) -> Result<(), MmError> {
    for image in &receipt.result.images {
        writeln!(
            writer,
            "    {}:{} -> {}@{}",
            terminal_safe(&image.module),
            image
                .target
                .as_deref()
                .map_or_else(|| "default".to_owned(), terminal_safe),
            terminal_safe(&image.repository),
            terminal_safe(&image.digest)
        )
        .map_err(output_error)?;
        if let Some(unit) = receipt
            .plan
            .units
            .iter()
            .find(|unit| unit.module == image.module)
            && let Some(variant) = unit.variant(image.target.as_deref())
        {
            write_build_args(writer, unit, variant, "      ")?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct WrittenReceipt<'a> {
    path: &'a std::path::Path,
    receipt_hash: &'a str,
    commit: &'a str,
    environment: &'a str,
    images: Vec<String>,
}

/// `mm build receipt FILE`: what the file holds.
pub(crate) fn write_build_receipt_written(
    path: &std::path::Path,
    receipt: &BuildReceipt,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&WrittenReceipt {
            path,
            receipt_hash: &receipt.receipt_hash,
            commit: receipt.source_commit(),
            environment: &receipt.plan.environment,
            images: receipt
                .result
                .images
                .iter()
                .map(|image| format!("{}@{}", image.repository, image.digest))
                .collect(),
        });
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Wrote build receipt {} to {}",
        terminal_safe(&receipt.receipt_hash),
        terminal_safe(&path.display().to_string())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  commit:      {}",
        terminal_safe(receipt.source_commit())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  environment: {}",
        terminal_safe(&receipt.plan.environment)
    )
    .map_err(output_error)?;
    write_receipt_images(&mut writer, receipt)
}

#[derive(Serialize)]
struct ImportOutput<'a> {
    #[serde(flatten)]
    plan: &'a crate::domain::build::BuildImportPlan,
    applied: bool,
}

/// `mm build import`: the reviewed import, or what it recorded.
pub(crate) fn write_build_import(
    plan: &crate::domain::build::BuildImportPlan,
    applied: bool,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&ImportOutput { plan, applied });
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    let heading = if applied {
        "Imported build receipt"
    } else {
        "Build import plan"
    };
    writeln!(
        writer,
        "{heading} {}",
        terminal_safe(if applied {
            &plan.receipt_hash
        } else {
            &plan.plan_hash
        })
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  receipt:     {} (commit {}, built for {})",
        terminal_safe(&plan.receipt_hash),
        terminal_safe(&plan.commit),
        terminal_safe(&plan.source_environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  environment: {}",
        terminal_safe(&plan.environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  records:     {}",
        if plan.cache_only {
            "reusable images only; deployments are unchanged"
        } else {
            "this checkout's published build, and its reusable images"
        }
    )
    .map_err(output_error)?;
    for image in &plan.images {
        writeln!(
            writer,
            "    {}:{} -> {}@{}{}",
            terminal_safe(&image.module),
            image
                .target
                .as_deref()
                .map_or_else(|| "default".to_owned(), terminal_safe),
            terminal_safe(&image.repository),
            terminal_safe(&image.digest),
            if image.reuse_fingerprint.is_some() {
                ""
            } else {
                " (not reusable)"
            }
        )
        .map_err(output_error)?;
    }
    if !applied {
        writeln!(
            writer,
            "Apply with: mm build import FILE{} --plan {} --yes",
            if plan.cache_only { " --cache-only" } else { "" },
            terminal_safe(&plan.plan_hash)
        )
        .map_err(output_error)?;
    }
    Ok(())
}
