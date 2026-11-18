use super::*;

pub(super) fn validate_jobs_schedules_and_gates(
    document: &ConfigDocument,
    report: &mut ValidationReport,
) {
    validate_jobs(document, report);
    validate_job_nodes(document, report);
    validate_schedules(document, report);
    validate_gates(document, report);
}

fn validate_jobs(document: &ConfigDocument, report: &mut ValidationReport) {
    let config_file = document.source("jobs");
    for (name, job) in &document.config.jobs {
        validate_name(name, "job name", config_file, report);
        let service_exists = document
            .config
            .modules
            .get(&job.module)
            .is_some_and(|module| module.services.contains_key(&job.service));
        if !service_exists {
            report.push(
                Diagnostic::error(
                    "MM_JOB_SERVICE_UNKNOWN",
                    format!(
                        "job '{name}' references unknown service '{}/{}'",
                        job.module, job.service
                    ),
                )
                .at_path(config_file),
            );
        }
        if job.command.is_empty() || job.command.iter().any(String::is_empty) {
            report.push(
                Diagnostic::error(
                    "MM_JOB_COMMAND_INVALID",
                    format!("job '{name}' must contain a non-empty argument vector"),
                )
                .at_path(config_file),
            );
        }
        if job.command.len() > crate::domain::observability::MAX_EXEC_ARGV
            || job.command.iter().any(|argument| {
                argument.as_bytes().contains(&0)
                    || argument.len() > crate::domain::observability::MAX_EXEC_ARGUMENT_BYTES
            })
        {
            report.push(
                Diagnostic::error(
                    "MM_JOB_COMMAND_BOUNDS",
                    format!("job '{name}' command exceeds the supported argv bounds"),
                )
                .at_path(config_file),
            );
        }
        match job.timeout.parse() {
            Ok(timeout)
                if !timeout.is_zero()
                    && timeout <= std::time::Duration::from_secs(24 * 60 * 60) => {}
            _ => report.push(
                Diagnostic::error(
                    "MM_JOB_TIMEOUT_INVALID",
                    format!("job '{name}' timeout must be greater than zero and no more than 24h"),
                )
                .at_path(config_file),
            ),
        }
        validate_retry_policy(name, &job.retry, config_file, report);
        if job.output_limit_bytes == 0
            || job.output_limit_bytes > crate::domain::scheduler::MAX_JOB_OUTPUT_BYTES
        {
            report.push(
                Diagnostic::error(
                    "MM_JOB_OUTPUT_LIMIT_INVALID",
                    format!(
                        "job '{name}' output-limit-bytes must be between 1 and {}",
                        crate::domain::scheduler::MAX_JOB_OUTPUT_BYTES
                    ),
                )
                .at_path(config_file),
            );
        }
        if job.history_limit == 0
            || job.history_limit > crate::domain::scheduler::MAX_JOB_HISTORY_ENTRIES
        {
            report.push(
                Diagnostic::error(
                    "MM_JOB_HISTORY_LIMIT_INVALID",
                    format!(
                        "job '{name}' history-limit must be between 1 and {}",
                        crate::domain::scheduler::MAX_JOB_HISTORY_ENTRIES
                    ),
                )
                .at_path(config_file),
            );
        }
        validate_job_environments(name, job, document, config_file, report);
    }
}

/// Every environment a job names exists.
fn validate_job_environments(
    name: &str,
    job: &crate::domain::config::JobConfig,
    document: &ConfigDocument,
    config_file: &std::path::Path,
    report: &mut ValidationReport,
) {
    for environment in job
        .environments
        .iter()
        .flat_map(|names| names.names())
        .filter(|name| !document.config.environments.contains_key(*name))
    {
        report.push(
            Diagnostic::error(
                "MM_JOB_ENVIRONMENT_UNKNOWN",
                format!("job '{name}' references unknown environment '{environment}'"),
            )
            .at_path(config_file),
        );
    }
}

/// Where each job runs on the cluster targets its environments use, and
/// who it runs as.
fn validate_job_nodes(document: &ConfigDocument, report: &mut ValidationReport) {
    let config_file = document.source("jobs");
    let targets = document
        .config
        .environments
        .values()
        .filter_map(|environment| environment.target.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    for (name, job) in &document.config.jobs {
        if let Some(user) = &job.user
            && (user.is_empty() || user.chars().any(|c| c.is_whitespace() || c.is_control()))
        {
            report.push(
                Diagnostic::error(
                    "MM_JOB_USER_INVALID",
                    format!(
                        "job '{name}' user must be a name the image knows, a uid, or uid:gid, without spaces"
                    ),
                )
                .at_path(config_file),
            );
        }
        let declared = document
            .config
            .modules
            .get(&job.module)
            .is_some_and(|module| module.services.contains_key(&job.service));
        for target in targets.iter().filter(|_| declared) {
            if let Err(problem) =
                crate::domain::placement::job_node_on(&document.config, target, name)
            {
                report.push(
                    Diagnostic::error("MM_PLACEMENT", format!("on target '{target}': {problem}"))
                        .at_path(config_file),
                );
            }
        }
    }
}

fn validate_schedules(document: &ConfigDocument, report: &mut ValidationReport) {
    let config_file = document.source("schedules");
    for (name, schedule) in &document.config.schedules {
        validate_name(name, "schedule name", config_file, report);
        if !document.config.jobs.contains_key(&schedule.job) {
            report.push(
                Diagnostic::error(
                    "MM_SCHEDULE_JOB_UNKNOWN",
                    format!(
                        "schedule '{name}' references unknown job '{}'",
                        schedule.job
                    ),
                )
                .at_path(config_file),
            );
        }
        validate_schedule_timing(name, schedule, config_file, report);
        if schedule.jitter.parse().map_or(true, |jitter| {
            jitter > std::time::Duration::from_secs(24 * 60 * 60)
        }) {
            report.push(
                Diagnostic::error(
                    "MM_SCHEDULE_JITTER_INVALID",
                    format!("schedule '{name}' jitter must be between 0s and 24h"),
                )
                .at_path(config_file),
            );
        }
        if matches!(
            schedule.misfire,
            crate::domain::config::MisfirePolicy::BoundedCatchUp { maximum: 0 }
        ) || matches!(schedule.misfire, crate::domain::config::MisfirePolicy::BoundedCatchUp { maximum } if maximum > crate::domain::scheduler::MAX_CATCH_UP_RUNS)
        {
            report.push(
                Diagnostic::error(
                    "MM_SCHEDULE_MISFIRE_INVALID",
                    format!(
                        "schedule '{name}' bounded catch-up must be between 1 and {}",
                        crate::domain::scheduler::MAX_CATCH_UP_RUNS
                    ),
                )
                .at_path(config_file),
            );
        }
        if let Some(retry) = &schedule.retry {
            validate_retry_policy(name, retry, config_file, report);
        }
        if schedule.timeout.as_ref().is_some_and(|timeout| {
            timeout.parse().map_or(true, |timeout| {
                timeout.is_zero() || timeout > std::time::Duration::from_secs(24 * 60 * 60)
            })
        }) {
            report.push(
                Diagnostic::error(
                    "MM_SCHEDULE_TIMEOUT_INVALID",
                    format!("schedule '{name}' timeout override must be greater than zero and no more than 24h"),
                )
                .at_path(config_file),
            );
        }
    }
}

fn validate_schedule_timing(
    name: &str,
    schedule: &crate::domain::config::ScheduleConfig,
    path: &Path,
    report: &mut ValidationReport,
) {
    let start = schedule
        .starts_at
        .as_deref()
        .map(crate::domain::scheduler::parse_instant)
        .transpose();
    let end = schedule
        .ends_at
        .as_deref()
        .map(crate::domain::scheduler::parse_instant)
        .transpose();
    let parsed_start = match &start {
        Ok(value) => *value,
        Err(_) => None,
    };
    let parsed_end = match &end {
        Ok(value) => *value,
        Err(_) => None,
    };
    let cron_valid = crate::domain::scheduler::next_occurrence(
        &schedule.cron,
        &schedule.timezone,
        0,
        parsed_start,
        parsed_end,
    );
    if cron_valid.is_err() {
        report.push(
            Diagnostic::error(
                "MM_SCHEDULE_CRON_INVALID",
                format!(
                    "schedule '{name}' has an invalid cron/timezone: {}",
                    cron_valid
                        .err()
                        .unwrap_or_else(|| "invalid expression".to_owned())
                ),
            )
            .at_path(path),
        );
    }
    if start.is_err()
        || end.is_err()
        || matches!((&start, &end), (Ok(Some(start)), Ok(Some(end))) if start > end)
    {
        report.push(
            Diagnostic::error(
                "MM_SCHEDULE_BOUNDS_INVALID",
                format!("schedule '{name}' requires valid ordered RFC 3339 start/end bounds"),
            )
            .at_path(path),
        );
    }
}

fn validate_gates(document: &ConfigDocument, report: &mut ValidationReport) {
    for (name, gate) in &document.config.gates {
        validate_name(name, "gate name", document.source("gates"), report);
        if gate.provider.trim().is_empty() {
            report.push(
                Diagnostic::error(
                    "MM_GATE_PROVIDER_INVALID",
                    format!("gate '{name}' must name an explicit provider"),
                )
                .at_path(document.source("gates")),
            );
        }
        if gate.ingress.iter().any(|route| route.trim().is_empty()) {
            report.push(
                Diagnostic::error(
                    "MM_GATE_INGRESS_INVALID",
                    format!("gate '{name}' contains an empty ingress reference"),
                )
                .at_path(document.source("gates")),
            );
        }
    }
}

fn validate_retry_policy(
    owner: &str,
    retry: &crate::domain::config::RetryPolicyConfig,
    path: &Path,
    report: &mut ValidationReport,
) {
    let backoff = retry.backoff.parse();
    let maximum = retry.max_backoff.parse();
    if retry.attempts == 0
        || retry.attempts > crate::domain::scheduler::MAX_JOB_ATTEMPTS
        || backoff.as_ref().map_or(true, |value| value.is_zero())
        || maximum.as_ref().map_or(true, |value| value.is_zero())
        || matches!((&backoff, &maximum), (Ok(backoff), Ok(maximum)) if backoff > maximum)
    {
        report.push(
            Diagnostic::error(
                "MM_JOB_RETRY_INVALID",
                format!(
                    "'{owner}' retry requires 1..={} attempts and positive backoff no greater than max-backoff",
                    crate::domain::scheduler::MAX_JOB_ATTEMPTS
                ),
            )
            .at_path(path),
        );
    }
}

#[cfg(test)]
mod tests;
