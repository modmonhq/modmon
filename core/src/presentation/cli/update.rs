//! `mm update`: this machine's `mm` from a verified release.

use super::*;
use crate::domain::release::{ReleaseTag, UpdateCheckReport, UpdateStatus};
use crate::domain::self_update::{Replacement, ReplacementKind, SelfUpdatePlan, SelfUpdateResult};
use crate::domain::update_steps::{FollowUpPlan, ServiceOutcome, UpdateSummary};
use crate::presentation::inline::{self, Progress};
use serde::Serialize;

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
    /// Replace and restart without asking. Needed when standard input is not
    /// a terminal.
    #[arg(long)]
    yes: bool,
}

pub(super) async fn run(cli: &Cli, args: &UpdateArgs) -> Result<u8, MmError> {
    let format = cli.output;
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
    let request = crate::app::self_update_request(executable.clone(), version, args.force);
    let service = composition::self_update_service()?;
    let steps = composition::update_follow_up_service(
        cli.root
            .clone()
            .map_or_else(super::terminal::current_directory, Ok)?,
        cli.mm_home.clone(),
        cli.environment.clone(),
    )?;
    let progress = Progress::new(format);
    let plan = inline::step(
        &progress,
        ("release", "Reading the release"),
        service.plan(&request),
        |_| Some("Read the release".to_owned()),
    )
    .await;
    progress.finish();
    let plan = plan?;
    let (replacement, version) = match &plan {
        SelfUpdatePlan::Replace(replacement) => {
            (Some(replacement.as_ref()), replacement.version.clone())
        }
        SelfUpdatePlan::UpToDate { current, .. }
        | SelfUpdatePlan::InstalledNewer { current, .. } => (None, current.clone()),
    };
    let installed = resolved(replacement.map_or(executable, |plan| plan.executable.clone()));
    let progress = Progress::new(format);
    let follow = inline::step(
        &progress,
        ("follow-up", "Reading local services and hosts"),
        steps.plan(&installed, &version, replacement.is_some()),
        |_| Some("Read local services and hosts".to_owned()),
    )
    .await;
    progress.finish();
    let follow = follow?;
    if (replacement.is_some() || !follow.restart.is_empty())
        && !confirmed(args, format, replacement, &follow)?
    {
        return Ok(1);
    }
    let result = match replacement {
        Some(replacement) => {
            let progress = Progress::new(format);
            let result = service.apply(replacement, &progress).await;
            progress.finish();
            Some(result?)
        }
        None => None,
    };
    let progress = Progress::new(format);
    let summary = inline::step(
        &progress,
        ("services", "Checking local services"),
        async { Ok::<_, MmError>(steps.apply(&follow, &installed, &version).await) },
        |_| Some("Checked local services".to_owned()),
    )
    .await?;
    progress.finish();
    let failed = summary.services.iter().any(|service| {
        matches!(
            service.outcome,
            ServiceOutcome::Unhealthy | ServiceOutcome::RestartFailed { .. }
        )
    });
    write_outcome(&plan, result.as_ref(), &summary, format)?;
    Ok(u8::from(failed))
}

fn confirmed(
    args: &UpdateArgs,
    format: OutputFormat,
    replacement: Option<&Replacement>,
    follow: &FollowUpPlan,
) -> Result<bool, MmError> {
    if format == OutputFormat::Json && !args.yes {
        return Err(MmError::Terminal {
            message: "JSON mm update requires --yes".to_owned(),
        });
    }
    if format == OutputFormat::Human {
        let mut lines = replacement.map(plan_lines).unwrap_or_default();
        lines.extend(
            follow
                .restart
                .iter()
                .map(|name| format!("Restarts {name}, which runs mm, so it runs the new release.")),
        );
        for line in lines {
            crate::outln!("{}", output::terminal_safe(&line));
        }
    }
    if args.yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(MmError::Terminal {
            message:
                "mm update replaces binaries and restarts services; pass --yes when standard input is not a terminal"
                    .to_owned(),
        });
    }
    let question = match replacement {
        Some(replacement) => format!("Update mm to {}?", replacement.version),
        None => "Restart these services?".to_owned(),
    };
    super::terminal::confirm(&question)
}

fn resolved(executable: std::path::PathBuf) -> std::path::PathBuf {
    std::fs::canonicalize(&executable).unwrap_or(executable)
}

fn write_check(report: &UpdateCheckReport, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(report);
    }
    crate::outln!("{}", output::terminal_safe(&report.message));
    Ok(())
}

#[derive(Serialize)]
struct Outcome<'a, T: Serialize> {
    #[serde(flatten)]
    outcome: &'a T,
    summary: &'a UpdateSummary,
}

fn write_outcome(
    plan: &SelfUpdatePlan,
    result: Option<&SelfUpdateResult>,
    summary: &UpdateSummary,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return match result {
            Some(result) => output::write_json(&Outcome {
                outcome: result,
                summary,
            }),
            None => output::write_json(&Outcome {
                outcome: plan,
                summary,
            }),
        };
    }
    let mut lines = match result {
        Some(result) => result_lines(result),
        None => unchanged_lines(plan),
    };
    lines.extend(output::update_summary_lines(summary));
    for line in lines {
        crate::outln!("{}", output::terminal_safe(&line));
    }
    Ok(())
}

fn unchanged_lines(plan: &SelfUpdatePlan) -> Vec<String> {
    match plan {
        SelfUpdatePlan::UpToDate { current, .. } => vec![format!("mm {current} is up to date.")],
        SelfUpdatePlan::InstalledNewer { current, latest } => vec![format!(
            "mm {current} is newer than the latest stable release, {latest}; nothing was changed."
        )],
        SelfUpdatePlan::Replace(_) => Vec::new(),
    }
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

fn result_lines(result: &SelfUpdateResult) -> Vec<String> {
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
    lines
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
