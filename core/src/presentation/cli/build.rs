use super::*;
use crate::presentation::inline;

#[derive(Debug, Clone, Subcommand)]
pub enum BuildCommand {
    /// Print the deterministic image build plan without running Docker.
    Plan,
    /// Execute an exact plan hash printed by `mm build plan`.
    Apply {
        /// Hash printed by `mm build plan`.
        #[arg(long, value_name = "HASH")]
        plan: String,
    },
    /// Write the latest published build of this checkout and environment to
    /// FILE, for `mm deploy plan --receipt` or `mm build import` on another
    /// machine.
    Receipt {
        #[arg(value_name = "FILE")]
        path: std::path::PathBuf,
    },
    /// Review or apply importing a build receipt from `mm build receipt`.
    ///
    /// Without `--plan`, prints the review and its hash. A normal import
    /// records the receipt as this checkout's published build for the
    /// environment, so deploy plans and status use its digests; it must be a
    /// build of HEAD, for this environment or one that builds the same
    /// units. `--cache-only` accepts a build of any commit and only lets
    /// later `mm build --publish` runs reuse its unchanged images.
    #[command(
        after_help = "Examples:\n  mm build import build-receipt.json\n  mm build import build-receipt.json --plan HASH --yes\n  mm build import baseline-receipt.json --cache-only --plan HASH --yes"
    )]
    Import {
        #[arg(value_name = "FILE")]
        path: std::path::PathBuf,
        /// Only seed build reuse; never select deployment images.
        #[arg(long)]
        cache_only: bool,
        /// Apply the import with this hash, printed by the review.
        #[arg(long, value_name = "HASH")]
        plan: Option<String>,
    },
}

pub(super) struct BuildCommandRequest<'a> {
    pub(super) command: Option<BuildCommand>,
    pub(super) request: crate::app::BuildRequest,
    pub(super) yes: bool,
    pub(super) selection: &'a WorkspaceSelection,
    pub(super) explicit_home: Option<&'a std::path::Path>,
    pub(super) format: OutputFormat,
    pub(super) runtime: &'a RuntimeComposition,
    pub(super) runtime_kind: crate::domain::runtime::RuntimeKind,
}

pub(super) async fn run(
    command: Option<BuildCommand>,
    request: crate::app::BuildRequest,
    yes: bool,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    run_build_command(BuildCommandRequest {
        command,
        request,
        yes,
        selection: &invocation.selection,
        explicit_home: invocation.home(),
        format: invocation.format,
        runtime: &invocation.services,
        runtime_kind: invocation.runtime_kind,
    })
    .await
}

pub(super) async fn run_build_command(request: BuildCommandRequest<'_>) -> Result<u8, MmError> {
    let BuildCommandRequest {
        command,
        request,
        yes,
        selection,
        explicit_home,
        format,
        runtime,
        runtime_kind,
    } = request;
    if let Some(BuildCommand::Receipt { path }) = &command {
        return write_receipt(path, selection, explicit_home, format, runtime);
    }
    if let Some(BuildCommand::Import {
        path,
        cache_only,
        plan,
    }) = &command
    {
        return run_build_import(
            BuildImportRequest {
                path,
                cache_only: *cache_only,
                plan: plan.as_deref(),
                yes,
            },
            selection,
            explicit_home,
            format,
            runtime,
        )
        .await;
    }
    let planner =
        runtime.build_manager_for(selection, explicit_home, (runtime_kind, request.publish))?;
    let progress = inline::Progress::new(format);
    let preview = inline::step(
        &progress,
        ("plan", "Planning the image builds"),
        planner.plan(selection, request),
        |_| Some("Planned the image builds".to_owned()),
    )
    .await?;
    progress.finish();
    if matches!(command, Some(BuildCommand::Plan)) {
        output::write_image_build_plan(&preview, format)?;
        return Ok(0);
    }
    if format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON image builds require --yes; use `mm build plan` to review first"
                .to_owned(),
        });
    }
    let expected_plan_hash = match command {
        Some(BuildCommand::Apply { plan }) => {
            if plan != preview.plan_hash {
                return Err(MmError::State {
                    message: format!(
                        "image build plan changed (expected {plan}, current {}); review the new plan",
                        preview.plan_hash
                    ),
                });
            }
            plan
        }
        Some(BuildCommand::Plan | BuildCommand::Receipt { .. } | BuildCommand::Import { .. }) => {
            return Ok(0);
        }
        None => preview.plan_hash.clone(),
    };
    if format == OutputFormat::Human {
        output::write_image_build_plan(&preview, format)?;
    }
    if !yes
        && !confirm(&format!(
            "Execute image build plan {}?",
            output::terminal_safe(inline::short(&expected_plan_hash))
        ))?
    {
        return Ok(0);
    }
    let engine = planner.local_engine();
    let build = runtime.build_service(planner, selection, explicit_home)?;
    apply_build(
        &build,
        selection,
        (request, engine, &expected_plan_hash),
        format,
    )
    .await
}

/// `mm build receipt FILE`.
fn write_receipt(
    path: &std::path::Path,
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    runtime: &RuntimeComposition,
) -> Result<u8, MmError> {
    let receipt = runtime
        .build_service(runtime.build_manager(), selection, explicit_home)?
        .receipt(selection)?;
    let mut bytes = serde_json::to_vec_pretty(&receipt).map_err(|error| MmError::Build {
        message: format!("could not serialize the build receipt: {error}"),
    })?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(|source| MmError::Io {
        context: format!("write build receipt {}", path.display()),
        source,
    })?;
    output::write_build_receipt_written(path, &receipt, format)?;
    Ok(0)
}

/// Builds, and publishes with `--publish`, the reviewed plan.
async fn apply_build(
    build: &crate::app::BuildService,
    selection: &WorkspaceSelection,
    (request, engine, expected_plan_hash): (
        crate::app::BuildRequest,
        Option<crate::domain::build::LocalImageEngine>,
        &str,
    ),
    format: OutputFormat,
) -> Result<u8, MmError> {
    let progress = inline::Progress::new(format);
    let label = match (request.publish, engine) {
        (true, _) => "Building and publishing the images".to_owned(),
        (false, Some(engine)) => format!("Building the images into {}", engine.name()),
        (false, None) => "Building the images".to_owned(),
    };
    let result = inline::step(
        &progress,
        ("build", &label),
        build.apply_reviewed(selection, request, expected_plan_hash),
        |result| Some(output::built_images_line(result)),
    )
    .await?;
    progress.finish();
    output::write_image_build_result(&result, format)?;
    Ok(0)
}

struct BuildImportRequest<'a> {
    path: &'a std::path::Path,
    cache_only: bool,
    plan: Option<&'a str>,
    yes: bool,
}

async fn run_build_import(
    request: BuildImportRequest<'_>,
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    format: OutputFormat,
    runtime: &RuntimeComposition,
) -> Result<u8, MmError> {
    let BuildImportRequest {
        path,
        cache_only,
        plan,
        yes,
    } = request;
    let receipt = read_receipt(path)?;
    let service = runtime.build_service(runtime.build_manager(), selection, explicit_home)?;
    let Some(plan) = plan else {
        if yes {
            return Err(MmError::Terminal {
                message:
                    "--yes applies a reviewed import; pass --plan HASH from `mm build import FILE`"
                        .to_owned(),
            });
        }
        let review = service
            .review_import(selection, &receipt, cache_only)
            .await?;
        output::write_build_import(&review, false, format)?;
        return Ok(0);
    };
    if !yes {
        if format == OutputFormat::Json {
            return Err(MmError::Terminal {
                message: "JSON build imports require --yes with the reviewed --plan hash"
                    .to_owned(),
            });
        }
        let review = service
            .review_import(selection, &receipt, cache_only)
            .await?;
        output::write_build_import(&review, false, format)?;
        if !confirm(&format!(
            "Apply build import plan {}?",
            output::terminal_safe(plan)
        ))? {
            return Ok(0);
        }
    }
    let applied = service
        .apply_import(selection, &receipt, cache_only, plan)
        .await?;
    output::write_build_import(&applied, true, format)?;
    Ok(0)
}

/// Receipts are a few kilobytes; anything near a protocol frame is not one.
const MAX_RECEIPT_BYTES: u64 = 4 * 1024 * 1024;

pub(super) fn read_receipt(
    path: &std::path::Path,
) -> Result<crate::domain::build::BuildReceipt, MmError> {
    use crate::domain::build::RECEIPT_SCHEMA;
    use crate::domain::stored_state::{StoredOn, newer_mm_message, unknown_in};

    let io = |source| MmError::Io {
        context: format!("read build receipt {}", path.display()),
        source,
    };
    if std::fs::metadata(path).map_err(io)?.len() > MAX_RECEIPT_BYTES {
        return Err(MmError::Build {
            message: format!("{} is too large to be a build receipt", path.display()),
        });
    }
    let bytes = std::fs::read(path).map_err(io)?;
    let newer = |version: &str| MmError::Build {
        message: newer_mm_message(path, version, StoredOn::ThisMachine),
    };
    let schema = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| value.get("schema")?.as_str().map(str::to_owned));
    if let Some(schema) = schema.filter(|schema| schema != RECEIPT_SCHEMA) {
        return Err(newer(&format!("build receipt schema '{schema}'")));
    }
    let receipt: crate::domain::build::BuildReceipt =
        serde_json::from_slice(&bytes).map_err(|error| match unknown_in(&error) {
            Some(unknown) => newer(&format!("build receipt with {unknown}")),
            None => MmError::Build {
                message: format!("{} is not a build receipt: {error}", path.display()),
            },
        })?;
    receipt.validate().map_err(|message| MmError::Build {
        message: format!("{}: {message}", path.display()),
    })?;
    Ok(receipt)
}
