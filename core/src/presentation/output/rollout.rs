//! How a deployment rolls out, in the words of a plan, and what the hosts
//! remember of their deployments (`mm deploy status`).

use super::*;
use crate::domain::remote::ClusterDeploymentPlan;
use crate::domain::rollout::{
    CancelReport, DeploymentRecord, DeploymentStatus, NodeRollout, OperationInFlight,
    RollbackTarget, RolloutPolicy, RolloutState, utc_time,
};

/// `600` as `10m`, `90` as `90s`.
pub(crate) fn window(seconds: u64) -> String {
    match seconds {
        seconds if seconds >= 3_600 && seconds % 3_600 == 0 => format!("{}h", seconds / 3_600),
        seconds if seconds >= 60 && seconds % 60 == 0 => format!("{}m", seconds / 60),
        seconds => format!("{seconds}s"),
    }
}

fn short(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// The header lines of a cluster plan that say in which order the nodes
/// are replaced, how long each may take, and what happens on failure.
pub(crate) fn plan_lines(plan: &ClusterDeploymentPlan) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(rollout) = &plan.rollout {
        let mut label = "  rollout: ";
        for stage in &rollout.stages {
            lines.push(format!("{label}{}", stage.describe()));
            label = "           ";
        }
        if rollout.stages.is_empty() {
            lines.push(format!("{label}no stack runs on these nodes"));
            label = "           ";
        }
        lines.push(format!(
            "{label}a stack may take {} on a node to become healthy (deploy.convergence-timeout); the next node starts once the others resolve its containers",
            window(rollout.convergence_timeout_seconds)
        ));
        lines.push(format!(
            "{label}on failure every node this deployment changed returns to what it ran before"
        ));
        for stage in &rollout.stages {
            if !stage.start_first.is_empty() {
                lines.push(format!(
                    "{label}{}: {} start{} before the old container stops (update.order: start-first)",
                    stage.stack,
                    stage.start_first.join(", "),
                    if stage.start_first.len() == 1 { "s" } else { "" }
                ));
            }
            if !stage.pause_on_failure.is_empty() {
                lines.push(format!(
                    "{label}{}: a failure of {} pauses the deployment instead (update.failure: pause)",
                    stage.stack,
                    stage.pause_on_failure.join(", ")
                ));
            }
        }
    }
    let jobs = plan
        .nodes
        .iter()
        .flat_map(|section| {
            section
                .plan
                .plan
                .iter()
                .flat_map(|node_plan| &node_plan.rollout.jobs)
                .map(move |job| (section.node.as_deref(), job))
        })
        .collect::<Vec<_>>();
    let ran_once = plan
        .rollout
        .as_ref()
        .map(|rollout| rollout.ran_once.clone())
        .unwrap_or_default();
    lines.extend(job_lines(&jobs, &ran_once));
    for unfinished in &plan.unfinished {
        lines.push(format!(
            "  unfinished: node {} holds a rollout of plan {} (commit {}) that {} started at {} and nobody finished; applying this plan first returns the node to what it ran before",
            unfinished.node,
            short(&unfinished.plan_hash),
            short(&unfinished.commit),
            unfinished.started_by,
            utc_time(unfinished.started_at_unix_ms)
        ));
    }
    lines
}

/// The lines of a one-host plan that say how long it waits and what it
/// does on failure.
pub(crate) fn policy_lines(policy: &RolloutPolicy) -> Vec<String> {
    if policy == &RolloutPolicy::default() {
        return Vec::new();
    }
    let mut lines = vec![format!(
        "  health: a stack may take {} to become healthy (deploy.convergence-timeout); on failure the deployment is rolled back",
        window(policy.convergence_timeout().as_secs())
    )];
    if policy.stack_order.len() > 1 {
        lines.push(format!("  order: {}", policy.stack_order.join(", then ")));
    }
    let first = policy
        .services
        .iter()
        .filter(|service| service.order == crate::domain::config::UpdateOrder::StartFirst)
        .map(|service| format!("{}/{}", service.stack, service.service))
        .collect::<Vec<_>>();
    if !first.is_empty() {
        lines.push(format!(
            "  start-first: {} (the new container is healthy before the old one stops)",
            first.join(", ")
        ));
    }
    lines
}

/// What `mm deploy rollback` says before its plan: what it undoes and
/// where the nodes return.
pub(crate) fn rollback_line(target: &RollbackTarget) -> String {
    format!(
        "Rollback: undoes {}; returns {} to commit {}{}.",
        target.undoes,
        target.nodes.join(", "),
        target.commit,
        super::release::rollback_release(&target.release)
    )
}

pub(crate) fn redeploy_line(target: &RollbackTarget) -> String {
    format!(
        "Redeploys {}{}",
        target.undoes,
        super::release::rollback_release(&target.release)
    )
}

fn record_line(label: &str, record: &DeploymentRecord) -> String {
    format!(
        "    {label} commit {}{}  {}  by {}{}{}",
        short(&record.commit),
        super::release::record_release(record.release.as_ref()),
        utc_time(record.deployed_at_unix_ms),
        record.deployed_by,
        record
            .plan_hash
            .as_deref()
            .map(|hash| format!("  plan {}", short(hash)))
            .unwrap_or_default(),
        if record.runs_services {
            ""
        } else {
            "  (runs nothing)"
        }
    )
}

fn rollout_lines(rollout: &NodeRollout) -> Vec<String> {
    let what = match (rollout.state, rollout.active) {
        (RolloutState::Held, true) => "is about to begin here",
        (RolloutState::Held, false) => return Vec::new(),
        (RolloutState::Running, true) => "is in progress",
        (RolloutState::Running, false) => {
            "was left UNFINISHED: the node is changed and nobody is working on it"
        }
        // The `runs:` line says it.
        (RolloutState::Finished, _) => return Vec::new(),
        (RolloutState::RolledBack, _) => "was rolled back: the node runs what it ran before",
    };
    let mut lines = vec![format!(
        "    rollout:  plan {} (commit {}) {what}; started by {} at {}",
        short(&rollout.plan_hash),
        short(&rollout.commit),
        rollout.started_by,
        utc_time(rollout.started_at_unix_ms),
    )];
    if rollout.state == RolloutState::Running {
        lines.push(format!(
            "              stacks applied: {}; last heard from the client at {}",
            if rollout.stacks.is_empty() {
                "none".to_owned()
            } else {
                rollout.stacks.join(", ")
            },
            utc_time(rollout.heard_at_unix_ms)
        ));
    }
    if let Some(failure) = &rollout.failure {
        lines.push(format!("              failed: {failure}"));
        lines.extend(
            failure
                .log
                .iter()
                .map(|line| format!("                {line}")),
        );
    }
    lines
}

/// `mm deploy status`: each host's last deployment, the one before, and a
/// rollout that runs or was left unfinished, then what to do about it.
pub(crate) fn write_deployment_status(
    status: &DeploymentStatus,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(status);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Deployments of {} [{}]",
        terminal_safe(&status.target),
        terminal_safe(&status.environment)
    )
    .map_err(output_error)?;
    for row in &status.nodes {
        writeln!(writer, "  {}", terminal_safe(&row.node)).map_err(output_error)?;
        let mut lines = Vec::new();
        match &row.status {
            None => lines.push(format!(
                "    no answer: {}",
                row.problem.as_deref().unwrap_or("the host did not answer")
            )),
            Some(node) => {
                match &node.current {
                    Some(current) => lines.push(record_line("runs:    ", current)),
                    None => lines.push("    runs:     nothing deployed yet".to_owned()),
                }
                lines.extend(
                    node.previous
                        .as_ref()
                        .map(|previous| record_line("before:  ", previous)),
                );
                lines.extend(node.rollout.iter().flat_map(rollout_lines));
                lines.extend(node.operations.iter().map(operation_line));
            }
        }
        for line in lines {
            writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
        }
    }
    for name in &status.answered_unhealthy {
        writeln!(
            writer,
            "\n{} is answered while unhealthy: none of its containers passes its health check, so the nodes answer and route to the running ones. `mm doctor` names the check.",
            terminal_safe(name)
        )
        .map_err(output_error)?;
    }
    let next = status.summary();
    writeln!(writer, "\n{}", terminal_safe(&next)).map_err(output_error)
}

/// The jobs a deployment runs, node by node, in the order they run.
pub(crate) fn job_lines(
    jobs: &[(Option<&str>, &crate::domain::rollout::DeployJob)],
    ran_once: &std::collections::BTreeMap<String, crate::domain::scheduler::OnceRecord>,
) -> Vec<String> {
    use crate::domain::config::JobWhen;
    jobs.iter()
        .map(|(node, job)| {
            let place = node.map(|node| format!(" on {node}")).unwrap_or_default();
            let when = match job.run {
                JobWhen::BeforeDeploy => format!(
                    "before {} is started or replaced, once what it depends on is healthy; a failure stops the deployment there",
                    job.service
                ),
                JobWhen::AfterDeploy => "once the deployment converged".to_owned(),
                JobWhen::Once => match ran_once.get(&job.job) {
                    Some(record) => format!(
                        "ran{} at {} (plan {}); `mm job run {}` runs it again",
                        record.node.as_deref().map(|node| format!(" on {node}")).unwrap_or_default(),
                        utc_time(record.at_unix_ms),
                        short(&record.plan_hash),
                        job.job
                    ),
                    None => "due: starts once the deployment converged and runs on under mmd".to_owned(),
                },
            };
            format!("  job: {} ({}){place}: {when}", job.job, job.run.as_str())
        })
        .collect()
}

/// One operation that runs on a host right now.
fn operation_line(operation: &OperationInFlight) -> String {
    format!(
        "    running:  {}; at work {}, stopped at {} if not done{}",
        operation.describe(),
        utc_time(operation.alive_at_unix_ms),
        utc_time(operation.deadline_unix_ms),
        operation
            .stopping
            .as_deref()
            .map(|why| format!("; {why}"))
            .unwrap_or_default()
    )
}

/// `mm deploy cancel`: what each host stopped and undid.
pub(crate) fn write_cancel_report(
    report: &CancelReport,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    let width = report
        .nodes
        .iter()
        .map(|node| node.node.len())
        .max()
        .unwrap_or(0);
    for node in &report.nodes {
        let mut said = Vec::new();
        if let Some(problem) = &node.problem {
            said.push(format!("no answer: {problem}"));
        }
        if let Some(outcome) = &node.outcome {
            said.extend(
                outcome
                    .stopped
                    .iter()
                    .map(|operation| format!("stopped {}", operation.describe())),
            );
            said.extend(outcome.runs.iter().map(|run| {
                format!(
                    "stopped job '{}' (run {}, {})",
                    run.job,
                    run.run,
                    serde_json::to_value(run.state)
                        .ok()
                        .and_then(|state| state.as_str().map(str::to_owned))
                        .unwrap_or_default()
                )
            }));
            if let Some(plan) = &outcome.rolled_back {
                said.push(format!(
                    "returned to what it ran before plan {}",
                    short(plan)
                ));
            }
            if let Some(plan) = &outcome.released {
                said.push(format!(
                    "plan {} had changed nothing here; the environment is free",
                    short(plan)
                ));
            }
            if let Some(problem) = &outcome.problem {
                said.push(problem.clone());
            }
        }
        if said.is_empty() {
            said.push("nothing ran here".to_owned());
        }
        writeln!(
            writer,
            "  {:<width$}  {}",
            terminal_safe(&node.node),
            terminal_safe(&said.join("; "))
        )
        .map_err(output_error)?;
    }
    let summary = if report.found_nothing() {
        format!(
            "Nothing ran on {} [{}]; nothing was cancelled.",
            terminal_safe(&report.target),
            terminal_safe(&report.environment)
        )
    } else {
        "`mm deploy status` shows what each host runs now.".to_owned()
    };
    writeln!(writer, "\n{summary}").map_err(output_error)
}
