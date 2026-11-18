use super::*;

pub(crate) fn desired_definitions(
    document: &ConfigDocument,
    environment: &str,
) -> Result<DesiredDefinitions, MmError> {
    let jobs = document
        .config
        .jobs
        .iter()
        .filter(|(_, job)| {
            job.runs_in(environment) && job_service_active(document, job, environment)
        })
        .map(|(name, job)| (name.clone(), job.clone()))
        .collect::<BTreeMap<_, _>>();
    let schedules = document
        .config
        .schedules
        .iter()
        .filter(|(_, schedule)| jobs.contains_key(&schedule.job))
        .map(|(name, schedule)| (name.clone(), schedule.clone()))
        .collect();
    if document.config.schedules.values().any(|schedule| {
        document.config.jobs.get(&schedule.job).is_none_or(|job| {
            job_service_active(document, job, environment) && !jobs.contains_key(&schedule.job)
        })
    }) {
        return Err(MmError::Environment {
            message: "a schedule references a job fixed to a different environment".to_owned(),
        });
    }
    Ok(DesiredDefinitions { jobs, schedules })
}

pub(super) fn active_job<'a>(
    document: &'a ConfigDocument,
    environment: &str,
    name: &str,
) -> Result<&'a JobConfig, MmError> {
    document
        .config
        .jobs
        .get(name)
        .filter(|job| job.runs_in(environment) && job_service_active(document, job, environment))
        .ok_or_else(|| MmError::Environment {
            message: format!("job '{name}' is not configured for environment '{environment}'"),
        })
}

fn job_service_active(document: &ConfigDocument, job: &JobConfig, environment: &str) -> bool {
    job.placed_elsewhere.is_none()
        && document
            .config
            .modules
            .get(&job.module)
            .and_then(|module| module.services.get(&job.service))
            .is_none_or(|service| service.is_active(environment))
}

pub(super) fn job_target<'a>(
    document: &ConfigDocument,
    targets: &'a [crate::domain::runtime::RuntimeTarget],
    job: &JobConfig,
) -> Result<&'a crate::domain::runtime::RuntimeTarget, MmError> {
    let service = document
        .config
        .modules
        .get(&job.module)
        .and_then(|module| module.services.get(&job.service))
        .ok_or_else(|| MmError::Environment {
            message: format!(
                "job service '{}/{}' is not configured",
                job.module, job.service
            ),
        })?;
    targets
        .iter()
        .find(|target| target.stack_name == service.stack)
        .ok_or_else(|| MmError::Environment {
            message: format!("job service stack '{}' is not available", service.stack),
        })
}

pub(super) fn effective_timeout(claim: &ClaimedJobRun) -> Result<Duration, MmError> {
    claim
        .schedule_definition
        .as_ref()
        .and_then(|schedule| schedule.timeout.as_ref())
        .unwrap_or(&claim.job_definition.timeout)
        .parse()
        .map_err(environment_error)
}

pub(super) fn timed_out(claim: &ClaimedJobRun, now_unix_ms: i64) -> Result<bool, MmError> {
    let Some(started) = claim.run.started_at_unix_ms else {
        return Ok(false);
    };
    let timeout = i64::try_from(effective_timeout(claim)?.as_millis()).unwrap_or(i64::MAX);
    Ok(now_unix_ms >= started.saturating_add(timeout))
}

pub(super) fn schedule_next(
    schedule: &ScheduleConfig,
    identity: &str,
    after: i64,
) -> Result<Option<i64>, MmError> {
    let start = schedule
        .starts_at
        .as_deref()
        .map(crate::domain::scheduler::parse_instant)
        .transpose()
        .map_err(environment_error)?;
    let end = schedule
        .ends_at
        .as_deref()
        .map(crate::domain::scheduler::parse_instant)
        .transpose()
        .map_err(environment_error)?;
    let jitter = schedule.jitter.parse().map_err(environment_error)?;
    next_occurrence(&schedule.cron, &schedule.timezone, after, start, end)
        .map(|next| next.map(|next| crate::domain::scheduler::apply_jitter(identity, next, jitter)))
        .map_err(environment_error)
}

pub(super) fn audit(
    document: &ConfigDocument,
    environment: &str,
    approval: &ApprovalContext,
    now_unix_ms: i64,
    action: &str,
    subject_kind: &str,
    subject_id: String,
) -> JobAuditRecord {
    JobAuditRecord {
        occurred_at_unix_ms: now_unix_ms,
        project_id: document.config.project.id,
        environment: environment.to_owned(),
        principal: approval.principal.clone(),
        source: approval.source.clone(),
        action: action.to_owned(),
        subject_kind: subject_kind.to_owned(),
        subject_id,
        definition_hash: None,
        details: BTreeMap::new(),
    }
}

pub(super) fn environment_error(message: String) -> MmError {
    MmError::Environment { message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_services_disable_jobs_and_schedules_without_hiding_unknown_jobs()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = crate::test_support::tempdir()?;
        let mut document = crate::adapters::config::ConfigLoader
            .initialize(root.path(), None)?
            .document;
        document.config.modules.insert(
            "app".to_owned(),
            serde_yaml_ng::from_str(
                "path: .\nservices:\n  worker: {stack: app, environments: [production]}\n",
            )?,
        );
        document.config.jobs.insert(
            "job".to_owned(),
            serde_yaml_ng::from_str("module: app\nservice: worker\ncommand: ['true']\n")?,
        );
        document.config.schedules.insert(
            "schedule".to_owned(),
            serde_yaml_ng::from_str("job: job\ncron: '* * * * *'\n")?,
        );
        let local = desired_definitions(&document, "local")?;
        assert!(local.jobs.is_empty());
        assert!(local.schedules.is_empty());
        assert!(active_job(&document, "local", "job").is_err());
        let production = desired_definitions(&document, "production")?;
        assert_eq!(production.jobs.len(), 1);
        assert_eq!(production.schedules.len(), 1);
        document.config.jobs.clear();
        assert!(desired_definitions(&document, "local").is_err());
        Ok(())
    }
}
