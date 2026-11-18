//! Jobs and schedules: running, toggling, and reconciling them from the Jobs view.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsPane {
    Jobs,
    Schedules,
    Runs,
}

impl JobsPane {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Jobs => Self::Schedules,
            Self::Schedules => Self::Runs,
            Self::Runs => Self::Jobs,
        }
    }

    #[must_use]
    pub const fn previous(self) -> Self {
        match self {
            Self::Jobs => Self::Runs,
            Self::Schedules => Self::Jobs,
            Self::Runs => Self::Schedules,
        }
    }
}

pub(super) async fn apply_scheduler_action(
    model: &mut Model,
    action: Action,
    use_cases: &TuiUseCases<'_>,
) {
    let approval = tui_approval();
    let result = match action {
        Action::RunJob(name) => use_cases
            .scheduler
            .enqueue_job(use_cases.selection, &name, &approval)
            .map(|run| format!("Enqueued run {}", run.id)),
        Action::RunSchedule(name) => use_cases
            .scheduler
            .run_schedule_now(use_cases.selection, &name, &approval)
            .map(|run| format!("Enqueued schedule '{name}' as run {}", run.id)),
        Action::ToggleSchedule(name, enabled) => use_cases
            .scheduler
            .set_schedule_enabled(use_cases.selection, &name, enabled, &approval)
            .map(|_| format!("Schedule '{name}' enabled={enabled}")),
        Action::CancelJob(run_id) => use_cases
            .scheduler
            .cancel_run(use_cases.selection, run_id, &approval)
            .map(|_| format!("Cancellation requested for {run_id}")),
        Action::RetryJob(run_id) => use_cases
            .scheduler
            .retry_run(use_cases.selection, run_id, &approval)
            .map(|run| format!("Retried {run_id} as run {}", run.id)),
        _ => return,
    };
    model.scheduler_message = Some(result.unwrap_or_else(|error| error.to_string()));
    refresh_scheduler(model, use_cases).await;
}

pub(super) async fn apply_scheduler_admin(
    model: &mut Model,
    action: Action,
    use_cases: &TuiUseCases<'_>,
) {
    let approval = tui_approval();
    let result = match action {
        Action::ReconcileSchedules => reconcile_schedules(model, use_cases, &approval),
        Action::TickScheduler => use_cases
            .scheduler
            .cycle(use_cases.selection, &approval)
            .await
            .map(|report| {
                format!(
                    "Cycle: {} schedules, {} enqueued, {} processed",
                    report.schedules_claimed, report.runs_enqueued, report.runs_processed
                )
            }),
        _ => return,
    };
    model.scheduler_message = Some(result.unwrap_or_else(|error| error.to_string()));
    refresh_scheduler(model, use_cases).await;
}

fn reconcile_schedules(
    model: &mut Model,
    use_cases: &TuiUseCases<'_>,
    approval: &crate::domain::observability::ApprovalContext,
) -> Result<String, MmError> {
    if let Some(plan) = model.scheduler_plan.take() {
        return use_cases
            .scheduler
            .apply_reconciliation(use_cases.selection, approval, &plan.plan_hash)
            .map(|plan| format!("Applied scheduler plan {}", plan.plan_hash));
    }
    use_cases
        .scheduler
        .plan_reconciliation(use_cases.selection, approval)
        .map(|plan| {
            let message = format!(
                "Review {} changes in plan {}; press x again to apply",
                plan.changes.len(),
                plan.plan_hash
            );
            model.scheduler_plan = Some(plan);
            message
        })
}

fn tui_approval() -> crate::domain::observability::ApprovalContext {
    crate::domain::observability::ApprovalContext {
        principal: "local-operator".to_owned(),
        source: "tui".to_owned(),
        approved_high_risk: false,
    }
}

pub(super) async fn refresh_scheduler(model: &mut Model, use_cases: &TuiUseCases<'_>) {
    let approval = tui_approval();
    if let Ok(jobs) = use_cases
        .scheduler
        .list_jobs(use_cases.selection, &approval)
    {
        model.jobs = jobs;
    }
    if let Ok(schedules) = use_cases
        .scheduler
        .list_schedules(use_cases.selection, &approval)
    {
        model.schedules = schedules;
    }
    if let Ok(runs) = use_cases
        .scheduler
        .history(use_cases.selection, None, None, 100, &approval)
    {
        model.job_runs = runs;
    }
}
