use super::*;

impl JobSchedulerService {
    #[must_use]
    pub fn new(dependencies: JobSchedulerDependencies) -> Self {
        Self {
            application: dependencies.application,
            repository: dependencies.repository,
            runtime: dependencies.runtime,
            secrets: dependencies.secrets,
            authorizer: dependencies.authorizer,
            clock: dependencies.clock,
            launch_targets: None,
        }
    }

    /// Creates each job's container with what the environment's last
    /// deployment gave its service: the same Compose files, environment,
    /// and secrets, staged for the run and removed once it started.
    #[must_use]
    pub fn with_launch_targets(mut self, targets: Arc<dyn JobLaunchTargets>) -> Self {
        self.launch_targets = Some(targets);
        self
    }

    pub fn plan_reconciliation(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
    ) -> Result<ReconciliationPlan, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(
            &document,
            &environment,
            approval,
            RuntimeOperation::ScheduleManage,
        )?;
        let desired = desired_definitions(&document, &environment)?;
        let stored = self
            .repository
            .definition_hashes(document.config.project.id, &environment)?;
        reconciliation_plan(
            document.config.project.id,
            environment,
            document.source_hash,
            &desired,
            &stored,
        )
        .map_err(|message| MmError::State { message })
    }

    pub fn apply_reconciliation(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
        reviewed_plan_hash: &str,
    ) -> Result<ReconciliationPlan, MmError> {
        let plan = self.plan_reconciliation(selection, approval)?;
        if plan.plan_hash != reviewed_plan_hash {
            return Err(MmError::Security {
                message: "scheduler reconciliation requires the exact current reviewed plan hash"
                    .to_owned(),
            });
        }
        let (document, environment) = self.context(selection)?;
        let desired = desired_definitions(&document, &environment)?;
        self.repository.reconcile(&ReconciliationCommit {
            plan: plan.clone(),
            desired,
            now_unix_ms: self.clock.now_unix_ms(),
            principal: approval.principal.clone(),
            source: approval.source.clone(),
        })?;
        Ok(plan)
    }

    pub fn list_jobs(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
    ) -> Result<DesiredDefinitions, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRead)?;
        self.repository
            .list_jobs(document.config.project.id, &environment)
    }

    pub fn list_schedules(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
    ) -> Result<Vec<ScheduleRecord>, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRead)?;
        self.repository
            .list_schedules(document.config.project.id, &environment)
    }

    pub fn history(
        &self,
        selection: &WorkspaceSelection,
        job: Option<&str>,
        schedule: Option<&str>,
        limit: usize,
        approval: &ApprovalContext,
    ) -> Result<Vec<JobRun>, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRead)?;
        self.repository.history(
            document.config.project.id,
            &environment,
            job,
            schedule,
            limit.min(1000),
        )
    }

    pub fn show_run(
        &self,
        selection: &WorkspaceSelection,
        run_id: Uuid,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRead)?;
        let run = self
            .repository
            .run(run_id)?
            .ok_or_else(|| MmError::Environment {
                message: format!("job run '{run_id}' was not found"),
            })?;
        if run.project_id != document.config.project.id || run.environment != environment {
            return Err(MmError::Security {
                message: "job run is not owned by the selected project and environment".to_owned(),
            });
        }
        Ok(run)
    }

    pub fn enqueue_job(
        &self,
        selection: &WorkspaceSelection,
        job_name: &str,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRun)?;
        let job = active_job(&document, &environment, job_name)?.clone();
        self.insert_run(NewRunInput {
            id: None,
            document: &document,
            environment: &environment,
            job_name,
            job,
            schedule: None,
            trigger: JobTrigger::Manual,
            retry_of: None,
            approval,
        })
    }

    pub fn run_schedule_now(
        &self,
        selection: &WorkspaceSelection,
        schedule_name: &str,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRun)?;
        let schedule = document
            .config
            .schedules
            .get(schedule_name)
            .ok_or_else(|| MmError::Environment {
                message: format!("schedule '{schedule_name}' is not configured"),
            })?
            .clone();
        let job = active_job(&document, &environment, &schedule.job)?.clone();
        let job_name = schedule.job.clone();
        self.insert_run(NewRunInput {
            id: None,
            document: &document,
            environment: &environment,
            job_name: &job_name,
            job,
            schedule: Some((schedule_name.to_owned(), schedule)),
            trigger: JobTrigger::RunNow,
            retry_of: None,
            approval,
        })
    }

    pub fn retry_run(
        &self,
        selection: &WorkspaceSelection,
        run_id: Uuid,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let previous = self.show_run(selection, run_id, approval)?;
        if !previous.state.is_terminal() {
            return Err(MmError::Environment {
                message: "only a terminal job run can be retried".to_owned(),
            });
        }
        let (document, environment) = self.context(selection)?;
        self.authorize(&document, &environment, approval, RuntimeOperation::JobRun)?;
        let job = active_job(&document, &environment, &previous.job)?.clone();
        let schedule = previous.schedule.as_ref().and_then(|name| {
            document
                .config
                .schedules
                .get(name)
                .cloned()
                .map(|schedule| (name.clone(), schedule))
        });
        self.insert_run(NewRunInput {
            id: None,
            document: &document,
            environment: &environment,
            job_name: &previous.job,
            job,
            schedule,
            trigger: JobTrigger::Retry,
            retry_of: Some(run_id),
            approval,
        })
    }

    pub fn cancel_run(
        &self,
        selection: &WorkspaceSelection,
        run_id: Uuid,
        approval: &ApprovalContext,
    ) -> Result<JobRun, MmError> {
        let run = self.show_run(selection, run_id, approval)?;
        let (document, environment) = self.context(selection)?;
        self.authorize(
            &document,
            &environment,
            approval,
            RuntimeOperation::JobCancel,
        )?;
        self.repository.request_cancel(
            run.id,
            &audit(
                &document,
                &environment,
                approval,
                self.clock.now_unix_ms(),
                "cancel",
                "run",
                run.id.to_string(),
            ),
        )
    }

    pub fn set_schedule_enabled(
        &self,
        selection: &WorkspaceSelection,
        name: &str,
        enabled: bool,
        approval: &ApprovalContext,
    ) -> Result<ScheduleRecord, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(
            &document,
            &environment,
            approval,
            RuntimeOperation::ScheduleManage,
        )?;
        let schedule = document
            .config
            .schedules
            .get(name)
            .ok_or_else(|| MmError::Environment {
                message: format!("schedule '{name}' is not configured"),
            })?;
        let next = if enabled {
            schedule_next(
                schedule,
                &format!("{}:{}:{}", document.config.project.id.0, environment, name),
                self.clock.now_unix_ms().saturating_sub(1),
            )?
        } else {
            None
        };
        self.repository.set_schedule_enabled(
            document.config.project.id,
            &environment,
            name,
            enabled,
            next,
            &audit(
                &document,
                &environment,
                approval,
                self.clock.now_unix_ms(),
                if enabled { "enable" } else { "disable" },
                "schedule",
                name.to_owned(),
            ),
        )
    }

    pub async fn cycle(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
    ) -> Result<SchedulerCycleReport, MmError> {
        let (document, environment) = self.context(selection)?;
        self.authorize(
            &document,
            &environment,
            approval,
            RuntimeOperation::ScheduleManage,
        )?;
        self.application
            .require_runtime_capabilities(
                "durable one-shot jobs",
                &[crate::domain::runtime::RuntimeCapability::OneShotJobs],
            )
            .await?;
        let owner = Uuid::new_v4();
        let now = self.clock.now_unix_ms();
        let schedules = self.repository.claim_due_schedules(
            document.config.project.id,
            &environment,
            owner,
            now,
            SCHEDULER_LEASE,
            MAX_SCHEDULES_PER_CYCLE,
        )?;
        let mut report = SchedulerCycleReport {
            schedules_claimed: schedules.len(),
            ..SchedulerCycleReport::default()
        };
        for claim in schedules {
            let due = due_occurrences(
                &claim.definition,
                &format!(
                    "{}:{}:{}",
                    claim.project_id.0, claim.environment, claim.name
                ),
                claim.next_due_at_unix_ms,
                now,
            )
            .map_err(|message| MmError::Environment { message })?;
            report.runs_enqueued += self
                .repository
                .commit_schedule_tick(&ScheduleTickCommit {
                    claim,
                    due_at_unix_ms: due.runs,
                    next_due_at_unix_ms: due.next_due_at_unix_ms,
                    now_unix_ms: now,
                    principal: approval.principal.clone(),
                    source: approval.source.clone(),
                })?
                .len();
        }
        for _ in 0..MAX_RUNS_PER_CYCLE {
            let Some(claim) = self.repository.claim_runnable_run(
                document.config.project.id,
                &environment,
                owner,
                self.clock.now_unix_ms(),
                SCHEDULER_LEASE,
            )?
            else {
                break;
            };
            if claim.run.project_id != document.config.project.id
                || claim.run.environment != environment
            {
                return Err(MmError::Security {
                    message: "scheduler repository returned a run outside the selected owner"
                        .to_owned(),
                });
            }
            report.runs_processed += 1;
            if self
                .process_claim(&document, selection, claim, approval)
                .await?
            {
                report.unknown_runs += 1;
            }
        }
        Ok(report)
    }

    pub async fn run_controller(
        &self,
        selection: &WorkspaceSelection,
        approval: &ApprovalContext,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<(), MmError> {
        while !cancellation.is_cancelled() {
            self.cycle(selection, approval).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(())
    }

    pub(super) async fn process_claim(
        &self,
        document: &ConfigDocument,
        selection: &WorkspaceSelection,
        claim: ClaimedJobRun,
        approval: &ApprovalContext,
    ) -> Result<bool, MmError> {
        let (_, _, targets) = self.application.runtime_operation_context(selection)?;
        let target = job_target(document, &targets, &claim.job_definition)?;
        let redactions = self.redactions(document, &claim.run.environment).await?;
        let output_limit = usize::try_from(claim.job_definition.output_limit_bytes)
            .unwrap_or(crate::domain::scheduler::MAX_JOB_OUTPUT_BYTES as usize);

        if claim.run.cancellation_requested || claim.run.state == JobRunState::Cancelling {
            self.cancel_claim(target, &claim).await?;
            return Ok(false);
        }

        let observed = self
            .runtime
            .inspect(
                target,
                &claim.run.runtime_name,
                claim.run.runtime_id.as_deref(),
                output_limit,
                redactions,
            )
            .await;
        match observed {
            Ok(RuntimeJobState::Exited {
                exit_code,
                output,
                truncated,
            }) => {
                self.finish_observed(target, &claim, exit_code, output, truncated)
                    .await?;
                return Ok(false);
            }
            Ok(RuntimeJobState::Running) => {
                if timed_out(&claim, self.clock.now_unix_ms())? {
                    self.runtime
                        .cancel(
                            target,
                            &claim.run.runtime_name,
                            claim.run.runtime_id.as_deref(),
                        )
                        .await?;
                    self.fail_or_retry(
                        target,
                        &claim,
                        "MM_JOB_TIMEOUT",
                        JobRunState::TimedOut,
                        (Some(124), String::new(), false),
                    )
                    .await?;
                }
                return Ok(false);
            }
            Ok(RuntimeJobState::Unknown { diagnostic_code }) => {
                self.repository.mark_run_unknown(&claim, &diagnostic_code)?;
                return Ok(true);
            }
            Err(error) => {
                self.repository
                    .mark_run_unknown(&claim, "MM_RUNTIME_TRANSPORT_UNKNOWN")?;
                self.repository.append_job_audit(&audit(
                    document,
                    &claim.run.environment,
                    approval,
                    self.clock.now_unix_ms(),
                    "reconcile_unknown",
                    "run",
                    claim.run.id.to_string(),
                ))?;
                tracing::warn!(run_id = %claim.run.id, code = error.code(), "job runtime state is unknown; retry suppressed");
                return Ok(true);
            }
            Ok(RuntimeJobState::Missing)
                if matches!(claim.run.state, JobRunState::Running | JobRunState::Unknown) =>
            {
                self.fail_or_retry(
                    target,
                    &claim,
                    "MM_JOB_RUNTIME_MISSING",
                    JobRunState::Failed,
                    (None, String::new(), false),
                )
                .await?;
                return Ok(false);
            }
            Ok(RuntimeJobState::Missing) => {}
        }

        self.start(document, selection, target, &claim, output_limit)
            .await?;
        Ok(false)
    }

    async fn cancel_claim(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        claim: &ClaimedJobRun,
    ) -> Result<(), MmError> {
        self.runtime
            .cancel(
                target,
                &claim.run.runtime_name,
                claim.run.runtime_id.as_deref(),
            )
            .await?;
        self.complete(
            claim,
            RunCompletion {
                state: JobRunState::Cancelled,
                exit_code: Some(130),
                error_code: Some("MM_CANCELLED"),
                diagnostic_code: None,
                output: String::new(),
                output_truncated: false,
            },
        )?;
        self.runtime
            .cleanup(
                target,
                &claim.run.runtime_name,
                claim.run.runtime_id.as_deref(),
            )
            .await
    }

    async fn finish_observed(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        claim: &ClaimedJobRun,
        exit_code: i32,
        output: String,
        truncated: bool,
    ) -> Result<(), MmError> {
        if exit_code == 0 {
            self.complete(
                claim,
                RunCompletion {
                    state: JobRunState::Succeeded,
                    exit_code: Some(0),
                    error_code: None,
                    diagnostic_code: None,
                    output,
                    output_truncated: truncated,
                },
            )?;
            self.runtime
                .cleanup(
                    target,
                    &claim.run.runtime_name,
                    claim.run.runtime_id.as_deref(),
                )
                .await?;
            self.repository.prune_history(
                claim.run.project_id,
                &claim.run.environment,
                &claim.run.job,
                claim.job_definition.history_limit as usize,
            )?;
        } else {
            self.fail_or_retry(
                target,
                claim,
                "MM_JOB_NON_ZERO",
                JobRunState::Failed,
                (Some(exit_code), output, truncated),
            )
            .await?;
        }
        Ok(())
    }

    /// `exit` is how the attempt ended: its exit code, its output, and
    /// whether that output was cut. The last attempt's output is kept.
    pub(super) async fn fail_or_retry(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        claim: &ClaimedJobRun,
        error_code: &str,
        terminal_state: JobRunState,
        exit: (Option<i32>, String, bool),
    ) -> Result<(), MmError> {
        self.runtime
            .cleanup(
                target,
                &claim.run.runtime_name,
                claim.run.runtime_id.as_deref(),
            )
            .await?;
        if claim.run.attempt < claim.run.maximum_attempts {
            let retry = claim
                .schedule_definition
                .as_ref()
                .and_then(|schedule| schedule.retry.as_ref())
                .unwrap_or(&claim.job_definition.retry);
            let delay = retry_delay(
                retry.backoff.parse().map_err(environment_error)?,
                retry.max_backoff.parse().map_err(environment_error)?,
                claim.run.attempt,
            );
            let delay = i64::try_from(delay.as_millis()).unwrap_or(i64::MAX);
            self.repository.schedule_retry(
                claim,
                self.clock.now_unix_ms().saturating_add(delay),
                error_code,
            )?;
        } else {
            self.complete(
                claim,
                RunCompletion {
                    state: terminal_state,
                    exit_code: exit.0,
                    error_code: Some(error_code),
                    diagnostic_code: None,
                    output: exit.1,
                    output_truncated: exit.2,
                },
            )?;
        }
        Ok(())
    }

    fn complete(
        &self,
        claim: &ClaimedJobRun,
        completion: RunCompletion<'_>,
    ) -> Result<JobRun, MmError> {
        self.repository.complete_run(&JobRunCompletion {
            run_id: claim.run.id,
            lease_owner: claim.lease_owner,
            fence_token: claim.run.fence_token,
            state: completion.state,
            finished_at_unix_ms: self.clock.now_unix_ms(),
            exit_code: completion.exit_code,
            error_code: completion.error_code.map(str::to_owned),
            diagnostic_code: completion.diagnostic_code,
            output: completion.output,
            output_truncated: completion.output_truncated,
        })
    }

    pub(super) async fn redactions(
        &self,
        document: &ConfigDocument,
        environment: &str,
    ) -> Result<Vec<zeroize::Zeroizing<String>>, MmError> {
        crate::app::environment::environment_redactions(
            &self.application,
            self.secrets.as_ref(),
            document,
            environment,
        )
        .await
    }

    pub(super) fn context(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<(ConfigDocument, String), MmError> {
        let (document, environment, _) = self.application.runtime_operation_context(selection)?;
        Ok((document, environment))
    }

    pub(super) fn authorize(
        &self,
        document: &ConfigDocument,
        environment: &str,
        approval: &ApprovalContext,
        operation: RuntimeOperation,
    ) -> Result<(), MmError> {
        self.authorizer.authorize(&AuthorizationRequest {
            principal: approval.principal.clone(),
            source: approval.source.clone(),
            operation,
            project_id: document.config.project.id,
            environment: environment.to_owned(),
            selector: None,
            approved_high_risk: approval.approved_high_risk,
        })
    }
}
