//! `mm update`: this machine's `mm` from a verified release.

use super::*;
use crate::domain::release::{ReleaseTag, UpdateCheckReport, UpdateStatus};
use crate::domain::self_update::{Replacement, ReplacementKind, SelfUpdatePlan, SelfUpdateResult};
use crate::presentation::inline::{self, Progress};

#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    /// Only say whether a newer stable release exists; change nothing.
    #[arg(long, conflicts_with_all = ["version", "force", "yes"])]
    check: bool,
    /// Install this release instead of the latest stable one, such as
    /// v0.1.2, or a release candidate such as v0.1.3-rc.1.
    #[arg(long, value_name = "vX.Y.Z")]
    version: Option<String>,
    /// Install an older release named with --version, install the same
    /// release again, or replace an mm built from a checkout.
    #[arg(long)]
    force: bool,
    /// Replace without asking. Needed when standard input is not a terminal.
    #[arg(long)]
    yes: bool,
}

pub(super) async fn run(args: &UpdateArgs, format: OutputFormat) -> Result<u8, MmError> {
    if args.check {
        let report = composition::update_check_service()?
            .check(env!("CARGO_PKG_VERSION"))
            .await;
        write_check(&report, format)?;
        return Ok(u8::from(report.status == UpdateStatus::Unavailable));
    }
    let version = args
        .version
        .as_deref()
        .map(ReleaseTag::parse)
        .transpose()
        .map_err(|message| MmError::Update { message })?;
    let executable = std::env::current_exe().map_err(|source| MmError::Io {
        context: "locating this mm executable".to_owned(),
        source,
    })?;
    let request = crate::app::self_update_request(executable, version, args.force);
    let service = composition::self_update_service()?;
    let progress = Progress::new(format);
    let plan = inline::step(
        &progress,
        ("release", "Reading the release"),
        service.plan(&request),
        |_| Some("Read the release".to_owned()),
    )
    .await;
    progress.finish();
    let replacement = match plan? {
        SelfUpdatePlan::Replace(replacement) => replacement,
        other => return write_unchanged(&other, format).map(|()| 0),
    };
    if format == OutputFormat::Json && !args.yes {
        return Err(MmError::Terminal {
            message: "JSON mm update requires --yes".to_owned(),
        });
    }
    if format == OutputFormat::Human {
        for line in plan_lines(&replacement) {
            crate::outln!("{}", output::terminal_safe(&line));
        }
    }
    if !args.yes {
        if !std::io::stdin().is_terminal() {
            return Err(MmError::Terminal {
                message:
                    "mm update replaces binaries; pass --yes when standard input is not a terminal"
                        .to_owned(),
            });
        }
        if !super::terminal::confirm(&format!("Update mm to {}?", replacement.version))? {
            return Ok(1);
        }
    }
    let progress = Progress::new(format);
    let result = service.apply(&replacement, &progress).await;
    progress.finish();
    write_result(&result?, format).map(|()| 0)
}

fn write_check(report: &UpdateCheckReport, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(report);
    }
    crate::outln!("{}", output::terminal_safe(&report.message));
    Ok(())
}

fn write_unchanged(plan: &SelfUpdatePlan, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(plan);
    }
    match plan {
        SelfUpdatePlan::UpToDate { current, .. } => {
            crate::outln!("mm {current} is up to date.");
        }
        SelfUpdatePlan::InstalledNewer { current, latest } => {
            crate::outln!(
                "mm {current} is newer than the latest stable release, {latest}; nothing was changed."
            );
        }
        SelfUpdatePlan::Replace(_) => {}
    }
    Ok(())
}

fn plan_lines(plan: &Replacement) -> Vec<String> {
    let kind = match plan.kind {
        ReplacementKind::Upgrade => "",
        ReplacementKind::Reinstall => " (the same release again)",
        ReplacementKind::Downgrade => " (an older release)",
    };
    let mut lines = vec![format!(
        "mm {} → {}{kind} in {}: {}",
        plan.current,
        plan.version,
        shown(&plan.directory),
        plan.binaries.join(", ")
    )];
    lines.push(format!(
        "From {} for {}, checked against the release's {}.",
        plan.download.tag, plan.download.target, plan.download.checked_against
    ));
    if let Some(checkout) = &plan.checkout {
        lines.push(format!(
            "This replaces an mm built from {checkout}, and its {}.",
            crate::domain::self_update::RECEIPT_FILE
        ));
    }
    lines
}

fn write_result(result: &SelfUpdateResult, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(result);
    }
    let mut lines = vec![format!(
        "Updated mm {} to {} in {}: {}.",
        result.previous,
        result.version,
        shown(&result.directory),
        result.binaries.join(", ")
    )];
    if let Some(command) = &result.helper_refresh {
        lines.push(
            "Local ingress still runs the old root-owned mm-ingress-helper. Refresh it with:"
                .to_owned(),
        );
        lines.push(format!("  {command}"));
    }
    lines.push(result.hosts.clone());
    for line in lines {
        crate::outln!("{}", output::terminal_safe(&line));
    }
    Ok(())
}

/// A directory under the home directory as `~/…`.
fn shown(directory: &std::path::Path) -> String {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .filter(|home| home.is_absolute())
        .and_then(|home| {
            directory
                .strip_prefix(&home)
                .ok()
                .map(|rest| std::path::Path::new("~").join(rest))
        })
        .unwrap_or_else(|| directory.to_path_buf())
        .display()
        .to_string()
}
