use super::*;
use crate::app::ports::ProgressReporter;
use crate::domain::progress::{NodeState, ProgressEvent, StepOutcome};
use crate::domain::release_facts::ReleaseRequest;
use crate::presentation::inline::{self, Progress};

#[derive(Debug, Clone, Subcommand)]
pub enum DeployCommand {
    /// Print the deterministic deployment plan without applying it.
    Plan {
        /// Commit a remote environment deploys; the host fetches it from the
        /// project's origin. Defaults to this checkout's HEAD.
        #[arg(long, value_name = "SHA")]
        commit: Option<String>,
        /// Build receipt from `mm build receipt`: deploy its images, built
        /// from the deployed commit for this or another environment.
        #[arg(long, value_name = "FILE")]
        receipt: Option<std::path::PathBuf>,
    },
    /// Apply an exact plan hash printed by `mm deploy plan`.
    Apply {
        /// Hash printed by `mm deploy plan`.
        #[arg(long, value_name = "HASH")]
        plan: String,
        /// The build receipt the local plan was made with. A remote plan
        /// already carries its receipt.
        #[arg(long, value_name = "FILE")]
        receipt: Option<std::path::PathBuf>,
    },
    /// Plan a return to what a remote environment ran before, and apply it
    /// after review.
    ///
    /// Undoes a rollout that was left unfinished; otherwise undoes the last
    /// finished deployment. The hosts plan the earlier commit from their
    /// own copy of the repository, so this checkout does not need it.
    Rollback,
    /// Plan the commit a remote environment runs now again, and apply it
    /// after review, so that secrets and variables staged on its hosts go
    /// live.
    ///
    /// Rolling and drained like any deployment; nothing is built, and the
    /// hosts use the build they run.
    Redeploy,
    /// Show what each host of a remote environment last deployed, what it
    /// ran before, a rollout that is running or was left unfinished, and
    /// what runs on each host right now.
    Status,
    /// Show the output of the jobs the last deployment ran, node by node.
    Logs,
    /// Stop a deployment or other operation that runs on a remote
    /// environment, end its processes on every host, and return the hosts
    /// it changed to what they ran before.
    ///
    /// Anyone who may deploy to the environment may cancel. `mm deploy
    /// status` shows the operation's ID; without one, whatever runs is
    /// stopped.
    Cancel {
        /// The operation's ID from `mm deploy status` or `mm deploy
        /// apply`, or its first characters (at least six).
        #[arg(value_name = "OPERATION")]
        operation: Option<String>,
    },
}

pub(super) struct DeployCommandRequest<'a> {
    pub(super) command: Option<DeployCommand>,
    pub(super) yes: bool,
    /// `--nodes`: the nodes of a cluster target to plan; empty means all.
    pub(super) nodes: Vec<String>,
    /// `--release` and `--release-input`; `None` for `mm dev up`, which
    /// deploys no release.
    pub(super) release: Option<ReleaseRequest>,
    pub(super) selection: &'a WorkspaceSelection,
    pub(super) explicit_home: Option<&'a std::path::Path>,
    pub(super) format: OutputFormat,
    pub(super) runtime: &'a RuntimeComposition,
    pub(super) runtime_kind: crate::domain::runtime::RuntimeKind,
    /// `mm dev up`: build what is stale before deploying on this machine.
    pub(super) rebuild: bool,
}

pub(super) async fn run(
    command: Option<DeployCommand>,
    yes: bool,
    (nodes, release): (Vec<String>, Option<ReleaseRequest>),
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    run_deploy_command(DeployCommandRequest {
        command,
        yes,
        nodes,
        release,
        selection: &invocation.selection,
        explicit_home: invocation.home(),
        format: invocation.format,
        runtime: &invocation.services,
        runtime_kind: invocation.runtime_kind,
        rebuild: false,
    })
    .await
}

pub(super) async fn run_deploy_command(request: DeployCommandRequest<'_>) -> Result<u8, MmError> {
    let DeployCommandRequest {
        command,
        yes,
        nodes,
        release,
        selection,
        explicit_home,
        format,
        runtime,
        runtime_kind,
        rebuild,
    } = request;
    refuse_release_for(command.as_ref(), release.as_ref())?;
    if matches!(command, Some(DeployCommand::Logs)) {
        let jobs = composition::job_gateway_as(
            selection,
            explicit_home,
            runtime_kind,
            runtime,
            composition::RemoteIdentity::User,
        )?;
        let runs = jobs.deployment_runs(&approval("cli", false)).await?;
        output::write_deployment_logs(&runs, format)?;
        return Ok(0);
    }
    if let Some(environment) =
        composition::remote_environment(selection, explicit_home, runtime_kind)?
    {
        return run_remote_deploy(
            (selection, runtime),
            (command, &release.unwrap_or_default()),
            (yes, format),
            &environment,
            &nodes,
        )
        .await;
    }
    if !nodes.is_empty() {
        return Err(MmError::Environment {
            message: "--nodes applies to a remote target that lists nodes:; this environment runs on this machine"
                .to_owned(),
        });
    }
    if let Some(
        command @ (DeployCommand::Rollback
        | DeployCommand::Redeploy
        | DeployCommand::Status
        | DeployCommand::Cancel { .. }),
    ) = &command
    {
        return Err(MmError::Environment {
            message: format!(
                "`mm deploy {}` acts on what a remote host remembers of its deployments, and this environment runs on this machine. To return to an earlier version here, check out its commit and run `mm deploy`; to stop a local deployment, interrupt it with Ctrl-C",
                match command {
                    DeployCommand::Status => "status",
                    DeployCommand::Cancel { .. } => "cancel",
                    DeployCommand::Redeploy => "redeploy",
                    _ => "rollback",
                }
            ),
        });
    }
    let selection = &composition::released(selection, release);
    run_local_deploy(
        command,
        (yes, format),
        selection,
        explicit_home,
        (runtime, runtime_kind, rebuild),
    )
    .await
}

/// `--release` and `--release-input` belong to a plan: a rollback returns
/// to the release it finds, a redeploy keeps the one the hosts run, and the
/// other commands plan nothing.
fn refuse_release_for(
    command: Option<&DeployCommand>,
    release: Option<&ReleaseRequest>,
) -> Result<(), MmError> {
    let name = match command {
        _ if release.is_none_or(ReleaseRequest::is_empty) => return Ok(()),
        Some(DeployCommand::Rollback) => {
            return Err(MmError::ReleaseInput {
                message: "a rollback returns to the release the hosts ran before, with the name and inputs it ran with; leave out --release and --release-input".to_owned(),
            });
        }
        Some(DeployCommand::Redeploy) => {
            return Err(MmError::ReleaseInput {
                message: "a redeploy keeps the release the hosts run, with its name and inputs; leave out --release and --release-input, or give them to `mm deploy plan`".to_owned(),
            });
        }
        Some(DeployCommand::Status) => "status",
        Some(DeployCommand::Logs) => "logs",
        Some(DeployCommand::Cancel { .. }) => "cancel",
        Some(DeployCommand::Plan { .. } | DeployCommand::Apply { .. }) | None => return Ok(()),
    };
    Err(MmError::ReleaseInput {
        message: format!(
            "`mm deploy {name}` plans nothing; --release and --release-input go with `mm deploy plan`"
        ),
    })
}

/// The deployment service, with the build receipt the command names, or
/// for `mm dev up` with the environment's local build.
async fn local_deployment(
    command: &Option<DeployCommand>,
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    (runtime, runtime_kind, rebuild): LocalRuntime<'_>,
) -> Result<crate::app::DeploymentService, MmError> {
    let deployment = if rebuild {
        runtime
            .development_deployment(selection, explicit_home, runtime_kind)
            .await?
    } else {
        runtime.deployment_service(selection, explicit_home)?
    };
    let deployment = match command {
        Some(
            DeployCommand::Plan {
                receipt: Some(path),
                ..
            }
            | DeployCommand::Apply {
                receipt: Some(path),
                ..
            },
        ) => {
            let promoted = runtime
                .build_manager()
                .receipt_for_environment(
                    selection,
                    &super::build::read_receipt(path)?,
                    &crate::app::ReceiptCheckout::Worktree,
                )
                .await?;
            deployment.with_receipt(promoted)
        }
        _ => deployment,
    };
    Ok(deployment)
}

/// The runtime composition, the runtime kind, and whether stale local
/// images are built first.
type LocalRuntime<'a> = (
    &'a RuntimeComposition,
    crate::domain::runtime::RuntimeKind,
    bool,
);

/// Plans and applies on this machine's engine.
async fn run_local_deploy(
    command: Option<DeployCommand>,
    (yes, format): (bool, OutputFormat),
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    local: LocalRuntime<'_>,
) -> Result<u8, MmError> {
    let runtime = local.0;
    if matches!(
        &command,
        Some(DeployCommand::Plan {
            commit: Some(_),
            ..
        })
    ) {
        return Err(MmError::Environment {
            message: "--commit applies to remote environments; a local deploy uses this checkout"
                .to_owned(),
        });
    }
    let deployment = local_deployment(&command, selection, explicit_home, local).await?;
    let progress = Progress::new(format);
    let preview = inline::step(
        &progress,
        ("plan", "Planning on this machine"),
        deployment.plan(selection),
        |plan| {
            Some(format!(
                "Planned {}",
                inline::count(plan.stacks.len(), "stack", "stacks")
            ))
        },
    )
    .await?;
    let surface = progress.surface();
    progress.finish();
    let show_plan = || {
        inline::print_card(
            surface,
            || output::local_plan_card(&preview, &selection.development_with),
            || output::write_deployment_plan(&preview, format),
        )
    };
    if review_only(command.as_ref(), (yes, format), selection) {
        show_plan()?;
        return Ok(0);
    }

    if format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON deployment requires --yes; use `mm deploy plan` to review first"
                .to_owned(),
        });
    }
    let expected_plan_hash = match command {
        Some(DeployCommand::Apply { plan, .. }) => {
            if plan != preview.plan_hash {
                return Err(MmError::State {
                    message: format!(
                        "deployment plan changed (expected {plan}, current {}); review the new plan",
                        preview.plan_hash
                    ),
                });
            }
            plan
        }
        Some(
            DeployCommand::Plan { .. }
            | DeployCommand::Rollback
            | DeployCommand::Redeploy
            | DeployCommand::Status
            | DeployCommand::Logs
            | DeployCommand::Cancel { .. },
        ) => {
            return Ok(0);
        }
        None => preview.plan_hash.clone(),
    };
    if format == OutputFormat::Human {
        show_plan()?;
    }
    if !yes
        && !confirm(&format!(
            "Apply deployment plan {}?",
            output::terminal_safe(inline::short(&expected_plan_hash))
        ))?
    {
        return Ok(0);
    }
    apply_local(
        (&deployment, &preview, &expected_plan_hash),
        (format, surface),
        selection,
        explicit_home,
        runtime,
    )
    .await
}

/// `mm deploy plan`, and `mm --output json dev up --with` without `--yes`,
/// whose plan has no other place to be reviewed.
fn review_only(
    command: Option<&DeployCommand>,
    (yes, format): (bool, OutputFormat),
    selection: &WorkspaceSelection,
) -> bool {
    matches!(command, Some(DeployCommand::Plan { .. }))
        || (command.is_none()
            && format == OutputFormat::Json
            && !yes
            && !selection.development_with.is_empty())
}

/// Applies a reviewed local plan and runs the jobs that follow it.
async fn apply_local(
    (deployment, preview, plan_hash): (
        &crate::app::DeploymentService,
        &crate::domain::deployment::DeploymentPlan,
        &str,
    ),
    (format, surface): (OutputFormat, inline::Surface),
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    runtime: &RuntimeComposition,
) -> Result<u8, MmError> {
    let started = std::time::Instant::now();
    let progress = Progress::new(format);
    if let Some(build) = &preview.local_build {
        inline::step(
            &progress,
            (
                "build",
                &format!(
                    "Building {} into {}",
                    inline::count(build.modules.len(), "image", "images"),
                    build.engine.name()
                ),
            ),
            deployment.build_reviewed(selection, plan_hash),
            |result| result.as_ref().map(output::built_images_line),
        )
        .await?;
    }
    let mut result = inline::step(
        &progress,
        ("apply", "Applying the plan"),
        deployment.apply_reviewed(selection, plan_hash),
        |result| {
            Some(format!(
                "Applied {}",
                inline::count(result.applied_stacks.len(), "stack", "stacks")
            ))
        },
    )
    .await?;
    result.jobs = inline::step(
        &progress,
        ("jobs", "Running the jobs that follow a deployment"),
        after_local_deployment(selection, explicit_home, runtime, preview),
        |jobs| Some(format!("Ran {}", inline::count(jobs.len(), "job", "jobs"))),
    )
    .await?;
    progress.finish();
    inline::print_card(
        surface,
        || output::deployment_result_card(&result, started.elapsed()),
        || output::write_deployment_result(&result, format),
    )?;
    output::report_job_outcomes(&result.jobs, format)?;
    Ok(0)
}

/// What follows a deployment on this machine: its `after-deploy` jobs and
/// the `once` jobs that never ran here.
async fn after_local_deployment(
    selection: &WorkspaceSelection,
    explicit_home: Option<&std::path::Path>,
    runtime: &RuntimeComposition,
    plan: &crate::domain::deployment::DeploymentPlan,
) -> Result<Vec<crate::domain::scheduler::JobOutcome>, MmError> {
    let kind = selection
        .runtime
        .or(plan.runtime)
        .unwrap_or(crate::domain::runtime::RuntimeKind::DockerCompose);
    composition::job_gateway_as(
        selection,
        explicit_home,
        kind,
        runtime,
        composition::RemoteIdentity::User,
    )?
    .after_local_deployment(
        (&plan.plan_hash, &plan.rollout.jobs),
        &approval("cli", false),
    )
    .await
}

/// `mm deploy status` of a remote environment; fails when no node answered.
async fn remote_status(
    environment: &crate::app::RemoteEnvironment,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let progress = Progress::new(format);
    let status = inline::step(
        &progress,
        ("status", &format!("Asking {}", node_count(environment))),
        async { Ok::<_, MmError>(environment.deployment_report().await) },
        |_| Some(format!("Asked {}", node_count(environment))),
    )
    .await?;
    progress.finish();
    output::write_deployment_status(&status, format)?;
    if status.none_answered() {
        return Err(MmError::Unreachable {
            subject: format!("no node of {} [{}]", status.target, status.environment),
            problem: "answered `mm deploy status`".to_owned(),
            help: "Each node's line says why; `mm doctor` checks the nodes".to_owned(),
        });
    }
    Ok(0)
}

/// Plans and applies on a remote environment: one host, or every node of a
/// cluster target under one plan hash.
pub(super) async fn run_remote_deploy(
    (selection, runtime): (&WorkspaceSelection, &RuntimeComposition),
    (command, release): (Option<DeployCommand>, &ReleaseRequest),
    (yes, format): (bool, OutputFormat),
    environment: &crate::app::RemoteEnvironment,
    nodes: &[String],
) -> Result<u8, MmError> {
    if let Some(DeployCommand::Apply { plan, receipt }) = command {
        if receipt.is_some() || !release.is_empty() {
            return Err(MmError::Environment {
                message:
                    "a remote deployment plan carries its build receipt and release; apply it with --plan alone"
                        .to_owned(),
            });
        }
        if !nodes.is_empty() {
            return Err(MmError::Environment {
                message: "a plan hash already fixes the nodes it applies to; pass --nodes to `mm deploy plan`, not to apply"
                    .to_owned(),
            });
        }
        if format == OutputFormat::Json && !yes {
            return Err(MmError::Terminal {
                message: "JSON remote deployment requires --yes".to_owned(),
            });
        }
        if !yes
            && !confirm(&format!(
                "Apply one-shot remote deployment plan {}?",
                output::terminal_safe(&plan)
            ))?
        {
            return Ok(0);
        }
        return apply_remote_deploy(environment, &plan, format).await;
    }
    let only = (!nodes.is_empty()).then_some(nodes);
    match command {
        Some(DeployCommand::Status) => {
            if only.is_some() {
                return Err(MmError::Environment {
                    message: "`mm deploy status` shows every node; leave out --nodes".to_owned(),
                });
            }
            return remote_status(environment, format).await;
        }
        Some(DeployCommand::Cancel { operation }) => {
            if only.is_some() {
                return Err(MmError::Environment {
                    message:
                        "`mm deploy cancel` stops the operation on every node; leave out --nodes"
                            .to_owned(),
                });
            }
            return cancel_remote(environment, operation.as_deref(), format).await;
        }
        Some(DeployCommand::Rollback) => {
            return rollback_remote(environment, only, (yes, format)).await;
        }
        Some(DeployCommand::Redeploy) => {
            return redeploy_remote(environment, only, (yes, format)).await;
        }
        _ => {}
    }
    let (explicit, receipt) = match &command {
        Some(DeployCommand::Plan { commit, receipt }) => (commit.as_deref(), receipt.as_deref()),
        _ => (None, None),
    };
    let commit = composition::resolve_commit(&selection.start, explicit)?;
    let receipt = receipt.map(super::build::read_receipt).transpose()?;
    if let Some(receipt) = &receipt
        && receipt.source_commit() != commit
    {
        return Err(MmError::Build {
            message: format!(
                "the build receipt is a build of commit {}, but this deploys {commit}; pass --commit {} to deploy the receipt's commit",
                receipt.source_commit(),
                receipt.source_commit()
            ),
        });
    }
    if let Some(receipt) = &receipt {
        let planner = runtime.build_manager();
        planner.check_receipt_covers(selection, receipt).await?;
    }
    let progress = Progress::new(format);
    let preview = environment
        .plan_deployment_reporting((&commit, receipt.as_ref(), release), only, &progress)
        .await?;
    let surface = progress.surface();
    progress.finish();
    let plan_only = matches!(command, Some(DeployCommand::Plan { .. }));
    review_and_apply(environment, &preview, (yes, format, surface), plan_only).await
}

/// Plans the return to what the hosts ran before, then reviews and applies it.
async fn rollback_remote(
    environment: &crate::app::RemoteEnvironment,
    only: Option<&[String]>,
    (yes, format): (bool, OutputFormat),
) -> Result<u8, MmError> {
    let progress = Progress::new(format);
    let (target, preview) = environment.plan_rollback_reporting(only, &progress).await?;
    let surface = progress.surface();
    progress.finish();
    if format == OutputFormat::Human {
        crate::outln!("{}", output::terminal_safe(&output::rollback_line(&target)));
        let ran = environment
            .deployment_jobs_since(target.since_unix_ms)
            .await
            .unwrap_or_default();
        crate::outln!(
            "{}\n",
            output::terminal_safe(&output::jobs_since_line(&ran))
        );
    }
    review_and_apply(environment, &preview, (yes, format, surface), false).await
}

/// Plans the commit the hosts run again, then reviews and applies it.
async fn redeploy_remote(
    environment: &crate::app::RemoteEnvironment,
    only: Option<&[String]>,
    (yes, format): (bool, OutputFormat),
) -> Result<u8, MmError> {
    let progress = Progress::new(format);
    let (target, preview) = environment.plan_redeploy_reporting(only, &progress).await?;
    let surface = progress.surface();
    progress.finish();
    if format == OutputFormat::Human {
        crate::outln!(
            "{}\n",
            output::terminal_safe(&output::redeploy_line(&target))
        );
    }
    review_and_apply(environment, &preview, (yes, format, surface), false).await
}

/// Prints a plan and, unless only the plan was asked for, applies it once
/// it is confirmed.
async fn review_and_apply(
    environment: &crate::app::RemoteEnvironment,
    preview: &crate::domain::remote::ClusterDeploymentPlan,
    (yes, format, surface): (bool, OutputFormat, inline::Surface),
    plan_only: bool,
) -> Result<u8, MmError> {
    crate::presentation::notices::warn_hosts(preview);
    inline::print_card(
        surface,
        || output::remote_plan_card(preview, environment.is_cluster()),
        || {
            if environment.is_cluster() {
                output::write_cluster_deployment_plan(preview, format)
            } else if let Some(host) = preview.nodes.first() {
                output::write_remote_deployment_plan(&host.plan, format)
            } else {
                Ok(())
            }
        },
    )?;
    if plan_only {
        return Ok(0);
    }
    if format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON remote deployment requires --yes after review".to_owned(),
        });
    }
    if !yes
        && !confirm(&format!(
            "Apply deployment plan {}?",
            output::terminal_safe(inline::short(&preview.plan_hash))
        ))?
    {
        return Ok(0);
    }
    apply_remote_deploy(environment, &preview.plan_hash, format).await
}

/// Applies a reviewed plan and prints what each node did. When the apply
/// stops at a node, the report is printed first and the command fails with
/// which nodes changed and which did not.
async fn apply_remote_deploy(
    environment: &crate::app::RemoteEnvironment,
    plan_hash: &str,
    format: OutputFormat,
) -> Result<u8, MmError> {
    let started = std::time::Instant::now();
    let progress = Progress::new(format);
    let nodes = environment
        .nodes()
        .iter()
        .map(|node| {
            node.name
                .clone()
                .unwrap_or_else(|| environment.target().to_owned())
        })
        .collect::<Vec<_>>();
    let label = format!("Deploying to {}", node_count(environment));
    progress.report(ProgressEvent::started(inline::ROLLOUT, label));
    for node in &nodes {
        progress.report(ProgressEvent::node(
            inline::ROLLOUT,
            node,
            NodeState::Waiting,
            None,
        ));
    }
    let report = environment
        .apply_deployment(plan_hash, &|step| progress.rollout(step))
        .await;
    let went_through = report
        .as_ref()
        .is_ok_and(|report| report.stopped("mm deploy plan").is_none());
    progress.report(ProgressEvent::finished(
        inline::ROLLOUT,
        if went_through {
            StepOutcome::Done
        } else {
            StepOutcome::Failed
        },
        went_through.then(|| format!("Deployed to {}", node_count(environment))),
    ));
    let surface = progress.surface();
    progress.finish();
    let report = report?;
    let took = started.elapsed();
    if environment.is_cluster() {
        inline::print_card(
            surface,
            || output::apply_report_card(&report, "Deployment", output::deployment_outcome, took),
            || output::write_cluster_deployment_report(&report, format),
        )?;
    } else if let Some(result) = report.nodes.first().and_then(|node| node.result.as_ref()) {
        inline::print_card(
            surface,
            || output::deployment_result_card(result, took),
            || output::write_deployment_result(result, format),
        )?;
    }
    if let Some(error) = report.stopped("mm deploy plan") {
        return Err(error);
    }
    let jobs = report
        .nodes
        .iter()
        .filter_map(|node| node.result.as_ref())
        .flat_map(|result| result.jobs.iter().cloned())
        .collect::<Vec<_>>();
    output::report_job_outcomes(&jobs, format)?;
    Ok(0)
}

/// `1 node`, `3 nodes`, or `the host`.
fn node_count(environment: &crate::app::RemoteEnvironment) -> String {
    if environment.is_cluster() {
        inline::count(environment.nodes().len(), "node", "nodes")
    } else {
        "the host".to_owned()
    }
}

/// Stops what runs on a remote environment and prints what each host did.
/// Nothing to stop is not an error; a named operation that runs nowhere
/// is, and so is a host that did not answer.
async fn cancel_remote(
    environment: &crate::app::RemoteEnvironment,
    operation: Option<&str>,
    format: OutputFormat,
) -> Result<u8, MmError> {
    if let Some(operation) = operation
        && operation.len() < 6
    {
        return Err(MmError::Environment {
            message: format!(
                "'{operation}' is too short to name an operation; give at least six characters of the ID that `mm deploy status` shows"
            ),
        });
    }
    let progress = Progress::new(format);
    progress.report(ProgressEvent::started(
        "cancel",
        format!("Stopping what runs on {}", node_count(environment)),
    ));
    let (report, unanswered) = environment.cancel(operation).await;
    progress.report(ProgressEvent::finished(
        "cancel",
        if unanswered.is_none() {
            StepOutcome::Done
        } else {
            StepOutcome::Failed
        },
        Some(format!("Asked {} to stop", node_count(environment))),
    ));
    progress.finish();
    output::write_cancel_report(&report, format)?;
    if let Some(error) = unanswered {
        return Err(error);
    }
    match operation {
        Some(operation) if report.found_nothing() => Err(MmError::Environment {
            message: format!(
                "no operation {operation} runs on '{}', and no deployment of it holds the environment; `mm deploy status` shows what runs",
                report.environment
            ),
        }),
        _ => Ok(0),
    }
}
