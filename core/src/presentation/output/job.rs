use super::*;

pub(crate) fn write_job_run(
    run: &crate::domain::scheduler::JobRun,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(run);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    let node = run
        .node
        .as_deref()
        .map(|node| format!(" node={}", terminal_safe(node)))
        .unwrap_or_default();
    writeln!(
        writer,
        "{}  job={} state={:?} attempt={}/{} exit={:?}{node}",
        run.id,
        terminal_safe(&run.job),
        run.state,
        run.attempt,
        run.maximum_attempts,
        run.exit_code,
    )
    .map_err(output_error)?;
    for line in run.output.lines() {
        writeln!(writer, "{}", terminal_safe(line)).map_err(output_error)?;
    }
    if run.output_truncated {
        writeln!(writer, "[output truncated at configured bound]").map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_job_history(
    runs: &[crate::domain::scheduler::JobRun],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&runs);
    }
    for run in runs {
        write_job_run(run, format)?;
    }
    Ok(())
}

pub(crate) fn write_schedules(
    schedules: &[crate::domain::scheduler::ScheduleRecord],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&schedules);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for schedule in schedules {
        writeln!(
            writer,
            "{}  job={} enabled={} retired={} next={:?} generation={}",
            terminal_safe(&schedule.name),
            terminal_safe(&schedule.job),
            schedule.enabled,
            schedule.retired,
            schedule.next_due_at_unix_ms,
            schedule.generation,
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_scheduler_plan(
    plan: &crate::domain::scheduler::ReconciliationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Scheduler reconciliation {}",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    for change in &plan.changes {
        writeln!(
            writer,
            "  {:?} {}",
            change.action,
            terminal_safe(&change.name)
        )
        .map_err(output_error)?;
    }
    writeln!(
        writer,
        "Review complete. Apply with --plan {} --yes.",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

/// `mm deploy logs`: each job the last deployment ran, with its output.
pub(crate) fn write_deployment_logs(
    runs: &[crate::domain::scheduler::JobRun],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&runs);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if runs.is_empty() {
        writeln!(writer, "The last deployment ran no jobs.").map_err(output_error)?;
        return Ok(());
    }
    for run in runs {
        let label = match &run.node {
            Some(node) => format!("{} {}", terminal_safe(node), terminal_safe(&run.job)),
            None => terminal_safe(&run.job),
        };
        let ended = match run.exit_code {
            Some(code) => format!("{:?}, exit {code}", run.state).to_lowercase(),
            None => format!("{:?}", run.state).to_lowercase(),
        };
        writeln!(
            writer,
            "{label}: {} ({ended}), run {}",
            terminal_safe(&run.source),
            run.id
        )
        .map_err(output_error)?;
        for line in run.output.lines() {
            writeln!(writer, "{label} | {}", terminal_safe(line)).map_err(output_error)?;
        }
        if run.output_truncated {
            writeln!(
                writer,
                "{label} | [output truncated at the job's output-limit-bytes]"
            )
            .map_err(output_error)?;
        }
    }
    Ok(())
}

/// The jobs a deployment ran or started once it was in place, one line
/// each; `None` when there were none.
pub(crate) fn job_outcome_lines(outcomes: &[crate::domain::scheduler::JobOutcome]) -> Vec<String> {
    use crate::domain::config::JobWhen;
    outcomes
        .iter()
        .map(|outcome| {
            let place = outcome
                .node
                .as_deref()
                .map(|node| format!(" on {node}"))
                .unwrap_or_default();
            let run = outcome
                .run_id
                .map(|id| format!(" (run {id})"))
                .unwrap_or_default();
            match (outcome.run, outcome.went_well()) {
                (JobWhen::Once, true) => format!(
                    "once job {} started{place}{run}; it runs on under mmd. Follow it with `mm job status {}`",
                    outcome.job,
                    outcome.run_id.map(|id| id.to_string()).unwrap_or_default()
                ),
                (_, true) => format!("{} job {} succeeded{place}{run}", outcome.run.as_str(), outcome.job),
                (_, false) => format!(
                    "{} job {} failed{place}{run}: {}",
                    outcome.run.as_str(),
                    outcome.job,
                    outcome.problem.as_deref().unwrap_or("it did not succeed")
                ),
            }
        })
        .collect()
}

/// Prints what the deployment did with its jobs once it was in place, and
/// fails when a job that had to succeed did not: the deployment stays.
pub(crate) fn report_job_outcomes(
    outcomes: &[crate::domain::scheduler::JobOutcome],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Human {
        let stderr = io::stderr();
        let mut writer = stderr.lock();
        for line in job_outcome_lines(outcomes) {
            writeln!(writer, "==> {}", terminal_safe(&line)).map_err(output_error)?;
        }
    }
    let failed = outcomes
        .iter()
        .filter(|outcome| !outcome.went_well())
        .map(|outcome| outcome.job.clone())
        .collect::<Vec<_>>();
    if failed.is_empty() {
        return Ok(());
    }
    Err(MmError::RuntimeResponse {
        message: format!(
            "the deployment is in place, and its job {} did not succeed. `mm deploy logs` shows the output; `mm job run {}` runs it again",
            failed.join(", "),
            failed[0]
        ),
    })
}

/// What a rollback plan says about the jobs that ran since the release it
/// returns to: rolling back does not undo them.
pub(crate) fn jobs_since_line(runs: &[crate::domain::scheduler::JobRun]) -> String {
    if runs.is_empty() {
        return "No deployment ran a job since then.".to_owned();
    }
    let ran = runs
        .iter()
        .map(|run| {
            format!(
                "{}{} ({})",
                run.job,
                run.node
                    .as_deref()
                    .map(|node| format!(" on {node}"))
                    .unwrap_or_default(),
                format!("{:?}", run.state).to_lowercase()
            )
        })
        .collect::<Vec<_>>();
    format!(
        "A rollback does not undo jobs. Since then the deployments ran: {}.",
        ran.join(", ")
    )
}

/// `mm job list`: each declared job, when a deployment runs it, its node,
/// and its last run.
pub(crate) fn write_job_list(
    document: &crate::domain::config::ConfigDocument,
    selection: &crate::app::WorkspaceSelection,
    runs: &[crate::domain::scheduler::JobRun],
    format: OutputFormat,
) -> Result<(), MmError> {
    let environment =
        crate::domain::config::resolve_environment(document, selection.environment.as_deref())?;
    let target = document
        .config
        .environments
        .get(&environment)
        .and_then(|config| config.target.clone())
        .unwrap_or_else(|| document.config.target.clone());
    let rows = document
        .config
        .jobs
        .iter()
        .filter(|(_, job)| job.runs_in(&environment))
        .map(|(name, job)| {
            let node = crate::domain::placement::job_node_on(&document.config, &target, name)
                .ok()
                .flatten();
            let last = runs.iter().find(|run| &run.job == name);
            serde_json::json!({
                "job": name,
                "run": job.run.map(crate::domain::config::JobWhen::as_str),
                "node": node,
                "last_run": last,
            })
        })
        .collect::<Vec<_>>();
    if format == OutputFormat::Json {
        return write_json(&rows);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if rows.is_empty() {
        writeln!(writer, "No job is declared for '{environment}'.").map_err(output_error)?;
        return Ok(());
    }
    for (name, job) in document
        .config
        .jobs
        .iter()
        .filter(|(_, job)| job.runs_in(&environment))
    {
        let when = job
            .run
            .map_or("when asked", crate::domain::config::JobWhen::as_str);
        let node = crate::domain::placement::job_node_on(&document.config, &target, name)
            .ok()
            .flatten()
            .map(|node| format!(" on {node}"))
            .unwrap_or_default();
        let last = runs.iter().find(|run| &run.job == name).map_or_else(
            || "never ran".to_owned(),
            |run| {
                format!(
                    "last run {} {}{}",
                    run.id,
                    format!("{:?}", run.state).to_lowercase(),
                    run.exit_code
                        .map(|code| format!(" (exit {code})"))
                        .unwrap_or_default()
                )
            },
        );
        writeln!(writer, "{}  {when}{node}  {last}", terminal_safe(name)).map_err(output_error)?;
    }
    Ok(())
}
