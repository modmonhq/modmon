use super::*;
use crate::app::ports::ProgressReporter;

#[derive(Debug, Clone, Subcommand)]
pub enum JobCommand {
    /// List the declared jobs of the environment: when a deployment runs
    /// each, on which node, and how its last run ended.
    List,
    /// Enqueue a named job and wait for its durable terminal status.
    Run {
        #[arg(value_name = "JOB")]
        name: String,
        /// Return after durable enqueue instead of driving execution.
        #[arg(long)]
        no_wait: bool,
    },
    /// Show one durable run by UUID.
    Status { run_id: uuid::Uuid },
    /// List bounded run history, optionally for one job.
    History {
        #[arg(value_name = "JOB")]
        name: Option<String>,
        /// Maximum number of newest runs to return.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Request cancellation of a queued or active run.
    Cancel { run_id: uuid::Uuid },
    /// Enqueue a new run from the current definition of a terminal run.
    Retry {
        run_id: uuid::Uuid,
        /// Return after durable enqueue instead of driving execution.
        #[arg(long)]
        no_wait: bool,
    },
    /// Operate schedules for configured jobs.
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ScheduleCommand {
    /// Plan or apply transactional definition reconciliation.
    Reconcile {
        /// Exact hash printed by the schedule reconciliation review.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Apply the reviewed reconciliation plan.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
    /// List schedules and their durable next due time.
    List,
    /// Show one schedule.
    Show { name: String },
    /// Enable one reconciled schedule.
    Enable { name: String },
    /// Disable one reconciled schedule without deleting history.
    Disable { name: String },
    /// Enqueue the schedule's job immediately, independent of enabled state.
    RunNow {
        name: String,
        /// Return after durable enqueue instead of driving execution.
        #[arg(long)]
        no_wait: bool,
    },
    /// Show the durable next due time for one schedule.
    Next { name: String },
    /// List bounded history for one schedule.
    History {
        name: String,
        /// Maximum number of newest runs to return.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Execute one fake-clock-friendly controller cycle.
    #[command(hide = true)]
    Tick,
    /// Run the scheduler controller until interrupted.
    #[command(hide = true)]
    Daemon,
}

pub(super) async fn run(
    command: JobCommand,
    invocation: &super::dispatch::Invocation,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    run_job_command(
        command,
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
        invocation.format,
        &invocation.services,
        cancellation,
    )
    .await
}

async fn run_job_command(
    command: JobCommand,
    selection: &WorkspaceSelection,
    mm_home: Option<&std::path::Path>,
    runtime: crate::domain::runtime::RuntimeKind,
    format: OutputFormat,
    services: &RuntimeComposition,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    let command = match command {
        JobCommand::Schedule { command } => {
            return run_schedule_command(
                command,
                selection,
                mm_home,
                runtime,
                format,
                services,
                cancellation,
            )
            .await;
        }
        command => command,
    };
    let gateway = composition::job_gateway_as(
        selection,
        mm_home,
        runtime,
        services,
        composition::RemoteIdentity::User,
    )?;
    let approval = approval("cli", false);
    match command {
        JobCommand::List => {
            let document = composition::project_documents().load_from(&selection.start)?;
            let runs = gateway
                .runs(
                    crate::domain::scheduler::JobRunsRequest {
                        job: None,
                        run: None,
                        limit: crate::domain::scheduler::MAX_LISTED_RUNS,
                    },
                    &approval,
                )
                .await?;
            output::write_job_list(&document, selection, &runs, format)?;
            Ok(0)
        }
        JobCommand::Run { name, no_wait } => {
            let run = gateway
                .run(
                    crate::domain::scheduler::JobRunRequest::manual(&name, false),
                    &approval,
                )
                .await?;
            if no_wait {
                output::write_job_run(&run, format)?;
                return Ok(0);
            }
            follow_job(&gateway, run, &approval, format, cancellation).await
        }
        JobCommand::Status { run_id } => {
            output::write_job_run(&gateway.run_status(run_id, &approval).await?, format)?;
            Ok(0)
        }
        JobCommand::History { name, limit } => {
            let request = crate::domain::scheduler::JobRunsRequest {
                job: name,
                run: None,
                limit: u32::try_from(limit).unwrap_or(u32::MAX),
            };
            output::write_job_history(&gateway.runs(request, &approval).await?, format)?;
            Ok(0)
        }
        JobCommand::Cancel { run_id } => {
            output::write_job_run(&gateway.cancel(run_id, &approval).await?, format)?;
            Ok(0)
        }
        JobCommand::Retry { run_id, no_wait } => {
            let run = gateway.retry(run_id, false, &approval).await?;
            if no_wait {
                output::write_job_run(&run, format)?;
                return Ok(0);
            }
            follow_job(&gateway, run, &approval, format, cancellation).await
        }
        JobCommand::Schedule { .. } => Ok(0),
    }
}

/// Follows a run until it ends and prints it: exit 0 when it succeeded.
/// Interrupted, a run on this machine is cancelled; one on a remote host
/// goes on there.
async fn follow_job(
    gateway: &crate::app::JobGateway,
    mut run: crate::domain::scheduler::JobRun,
    approval: &crate::domain::observability::ApprovalContext,
    format: OutputFormat,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    use crate::domain::progress::{ProgressEvent, StepOutcome};
    use crate::domain::scheduler::JobRunState;
    let progress = crate::presentation::inline::Progress::new(format);
    progress.report(ProgressEvent::started(
        "job",
        format!("Running job {}", run.job),
    ));
    let mut cancelled = false;
    while !run.state.is_terminal() && run.state != JobRunState::Unknown {
        progress.report(ProgressEvent::detail(
            "job",
            format!("{:?}", run.state).to_lowercase(),
        ));
        if cancellation.is_cancelled() && !cancelled {
            cancelled = true;
            if let Some(note) = gateway.left_running(run.id) {
                eprintln!("{}", output::terminal_safe(&note));
                return Ok(130);
            }
            if let Err(error) = gateway.cancel(run.id, approval).await {
                eprintln!("{}", output::terminal_safe(&error.to_string()));
                return Ok(130);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        run = gateway.run_status(run.id, approval).await?;
    }
    progress.report(ProgressEvent::finished(
        "job",
        if run.state == JobRunState::Succeeded {
            StepOutcome::Done
        } else {
            StepOutcome::Failed
        },
        Some(format!("Job {} ended {:?}", run.job, run.state).to_lowercase()),
    ));
    progress.finish();
    output::write_job_run(&run, format)?;
    Ok(if run.state == JobRunState::Succeeded {
        0
    } else {
        3
    })
}

pub(super) async fn run_schedule_command(
    command: ScheduleCommand,
    selection: &WorkspaceSelection,
    mm_home: Option<&std::path::Path>,
    runtime: crate::domain::runtime::RuntimeKind,
    format: OutputFormat,
    services: &RuntimeComposition,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    if composition::remote_environment(selection, mm_home, runtime)?.is_some() {
        return Err(crate::domain::scheduler::remote_schedules(
            selection
                .environment
                .as_deref()
                .unwrap_or("the environment"),
        ));
    }
    let manager = composition::job_scheduler_service(&selection.start, mm_home, runtime, services)?;
    let approval = approval("cli", false);
    match command {
        ScheduleCommand::Reconcile { plan, yes } => {
            let preview = manager.plan_reconciliation(selection, &approval)?;
            let Some(plan) = plan else {
                output::write_scheduler_plan(&preview, format)?;
                return Ok(0);
            };
            if !yes {
                return Err(MmError::Terminal {
                    message:
                        "scheduler reconciliation requires --yes with the reviewed --plan hash"
                            .to_owned(),
                });
            }
            let applied = manager.apply_reconciliation(selection, &approval, &plan)?;
            output::write_scheduler_plan(&applied, format)?;
            Ok(0)
        }
        ScheduleCommand::List => {
            output::write_schedules(&manager.list_schedules(selection, &approval)?, format)?;
            Ok(0)
        }
        ScheduleCommand::Show { name } | ScheduleCommand::Next { name } => {
            let schedules = manager
                .list_schedules(selection, &approval)?
                .into_iter()
                .filter(|schedule| schedule.name == name)
                .collect::<Vec<_>>();
            if schedules.is_empty() {
                return Err(MmError::Environment {
                    message: format!("schedule '{name}' was not found"),
                });
            }
            output::write_schedules(&schedules, format)?;
            Ok(0)
        }
        ScheduleCommand::Enable { name } => {
            output::write_schedules(
                &[manager.set_schedule_enabled(selection, &name, true, &approval)?],
                format,
            )?;
            Ok(0)
        }
        ScheduleCommand::Disable { name } => {
            output::write_schedules(
                &[manager.set_schedule_enabled(selection, &name, false, &approval)?],
                format,
            )?;
            Ok(0)
        }
        ScheduleCommand::RunNow { name, no_wait } => {
            let run = manager.run_schedule_now(selection, &name, &approval)?;
            if no_wait {
                output::write_job_run(&run, format)?;
                return Ok(0);
            }
            wait_for_job(&manager, selection, run.id, &approval, format, cancellation).await
        }
        ScheduleCommand::History { name, limit } => {
            let runs = manager.history(selection, None, Some(&name), limit, &approval)?;
            output::write_job_history(&runs, format)?;
            Ok(0)
        }
        ScheduleCommand::Tick => {
            output::write_json(&manager.cycle(selection, &approval).await?)?;
            Ok(0)
        }
        ScheduleCommand::Daemon => {
            manager
                .run_controller(selection, &approval, cancellation)
                .await?;
            Ok(0)
        }
    }
}

pub(super) async fn wait_for_job(
    manager: &crate::app::JobSchedulerService,
    selection: &WorkspaceSelection,
    run_id: uuid::Uuid,
    approval: &crate::domain::observability::ApprovalContext,
    format: OutputFormat,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    loop {
        if cancellation.is_cancelled() {
            manager.cancel_run(selection, run_id, approval).await?;
        }
        manager.cycle(selection, approval).await?;
        let run = manager.show_run(selection, run_id, approval)?;
        if run.state.is_terminal() || run.state == crate::domain::scheduler::JobRunState::Unknown {
            output::write_job_run(&run, format)?;
            return Ok(
                if run.state == crate::domain::scheduler::JobRunState::Succeeded {
                    0
                } else {
                    3
                },
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
