//! Planning and applying across the nodes of a remote environment.
//!
//! A plan for a cluster is every selected node's own plan, bound under one
//! hash that each of those nodes keeps. Applying asks the nodes for that
//! seal first, so nothing changes unless every node of the plan still holds
//! exactly what was reviewed, and then walks the nodes.

use futures_util::future::join_all;

use super::progress::Watched;
use super::{RemoteEnvironment, RemoteNode, in_node, reporting};
use crate::app::ports::{NoProgress, ProgressReporter, progress_step};
use crate::domain::build::BuildReceipt;
use crate::domain::deployment::DeploymentResult;
use crate::domain::error::MmError;
use crate::domain::progress::{ProgressEvent, StepOutcome};
use crate::domain::release_facts::ReleaseRequest;
use crate::domain::remote::{
    ClusterAccessPlan, ClusterApplyReport, ClusterDeploymentPlan, ClusterPlanSeal,
    ClusterPlanSection, NodeAccessPlan, NodeApplyOutcome, NodeApplyState, NodeDeploymentPlan,
    RemotePlanBinding, RemotePlanKind, SealCarried, UnfinishedRollout, check_moves_are_planned,
    service_moves,
};
use crate::domain::rollout::{ClusterRollout, NodeRolloutInput, ProgressStep, RolloutProgress};
use crate::domain::shared_storage::{ClusterStoragePlan, SealedStorage};
use crate::domain::users::AccessChange;

/// What the nodes planned, before the plan is sealed.
struct PlannedSections {
    nodes: Vec<NodeDeploymentPlan>,
    moves: Vec<crate::domain::remote::ServiceMove>,
    networks: Option<crate::domain::cluster_network::ClusterNetworkPlan>,
    storage: Option<ClusterStoragePlan>,
    ingress: Option<crate::domain::cluster_ingress::SealedIngress>,
    rollout: Option<ClusterRollout>,
}

/// Called each time a node starts a step of an apply, so a long apply shows
/// where it is.
pub type NodeProgress<'a> = &'a (dyn Fn(&RolloutProgress) + Send + Sync);

impl RemoteEnvironment {
    /// Asks every selected node to plan `commit`, all at once, and binds the
    /// answers under one hash. `only` limits the plan to some nodes; the
    /// others keep what they run.
    pub async fn plan_deployment(
        &self,
        commit: &str,
        receipt: Option<&BuildReceipt>,
        only: Option<&[String]>,
    ) -> Result<ClusterDeploymentPlan, MmError> {
        let release = ReleaseRequest::default();
        self.plan_deployment_reporting((commit, receipt, &release), only, &NoProgress)
            .await
    }

    /// As [`Self::plan_deployment`], for the release `release` names and
    /// saying what each node is doing.
    pub async fn plan_deployment_reporting(
        &self,
        (commit, receipt, release): (&str, Option<&BuildReceipt>, &ReleaseRequest),
        only: Option<&[String]>,
        progress: &dyn ProgressReporter,
    ) -> Result<ClusterDeploymentPlan, MmError> {
        let (selected, skipped) = self.select(only)?;
        self.check_pinned_client()?;
        // Hosts that fetch nothing plan from the files mm sends them.
        let delivered = self.deliver(commit, &selected, progress).await?;
        let short = crate::domain::rollout::short_id(commit);
        let planning = format!(
            "Planning commit {short} on {}",
            reporting::nodes(selected.len())
        );
        let planned = format!(
            "Planned commit {short} on {}",
            reporting::nodes(selected.len())
        );
        progress.report(ProgressEvent::started(reporting::PLAN, planning));
        self.report_left_out(reporting::PLAN, &skipped, progress);
        let sections = self
            .plan_sections((commit, receipt, release), (&selected, &skipped), progress)
            .await;
        progress.report(ProgressEvent::finished(
            reporting::PLAN,
            if sections.is_ok() {
                StepOutcome::Done
            } else {
                StepOutcome::Failed
            },
            sections.is_ok().then_some(planned),
        ));
        let PlannedSections {
            nodes,
            moves,
            networks,
            storage,
            ingress,
            rollout,
        } = sections?;
        let bindings = nodes
            .iter()
            .map(|node| &node.plan.binding)
            .collect::<Vec<_>>();
        if let Some(delivered) = &delivered {
            delivered.check(&bindings)?;
        }
        let sealing = self.bind(
            RemotePlanKind::Deployment,
            commit,
            (&selected, &bindings, &skipped),
            SealCarried {
                networks: networks.as_ref().map(|plan| plan.sealed.clone()),
                rollout: rollout.clone(),
                storage: storage.as_ref().map(ClusterStoragePlan::sealed),
                ingress: ingress.clone(),
            },
        );
        let plan_hash = progress_step(
            progress,
            (reporting::SEAL, "Sealing the plan on every node"),
            sealing,
            |hash: &String| {
                Some(format!(
                    "Sealed plan {} on {}",
                    crate::domain::rollout::short_id(hash),
                    reporting::nodes(selected.len())
                ))
            },
        )
        .await?;
        let unfinished = self.unfinished(&nodes);
        Ok(ClusterDeploymentPlan {
            plan_hash,
            target: self.target.clone(),
            environment: self.first().scope.environment.clone(),
            commit: commit.to_owned(),
            expires_at_unix_ms: earliest_expiry(&bindings),
            nodes,
            skipped,
            moves,
            networks,
            rollout,
            storage,
            ingress,
            unfinished,
            bundle: delivered.map(|delivered| delivered.plan),
        })
    }

    /// Each node says, with its plan, whether it holds a rollout nobody
    /// finished: applying this plan undoes it first.
    fn unfinished(&self, nodes: &[NodeDeploymentPlan]) -> Vec<UnfinishedRollout> {
        nodes
            .iter()
            .filter_map(|node| {
                let rollout = node.plan.unfinished.as_ref()?;
                Some(UnfinishedRollout {
                    node: node.node.clone().unwrap_or_else(|| self.target.clone()),
                    plan_hash: rollout.plan_hash.clone(),
                    commit: rollout.commit.clone(),
                    started_by: rollout.started_by.clone(),
                    started_at_unix_ms: rollout.started_at_unix_ms,
                })
            })
            .collect()
    }

    /// Every selected node's plan and what the cluster's plan makes of them.
    async fn plan_sections(
        &self,
        (commit, receipt, release): (&str, Option<&BuildReceipt>, &ReleaseRequest),
        (selected, skipped): (&[&RemoteNode], &[String]),
        progress: &dyn ProgressReporter,
    ) -> Result<PlannedSections, MmError> {
        // What every node stored for the cluster's networks is asked for
        // while the nodes plan: planning changes nothing on a node, so the
        // answer is the same before, during, and after it. So is the
        // release each one runs, which a pinned environment checks.
        let (answers, stored, pinned) = tokio::join!(
            self.ask_all(
                (reporting::PLAN, "planning"),
                selected,
                progress,
                |node| async move {
                    node.connect()?
                        .plan_deployment(
                            node.scope.clone(),
                            commit.to_owned(),
                            receipt.cloned(),
                            release.clone(),
                        )
                        .await
                }
            ),
            self.stored_networks(),
            self.check_pinned_hosts(selected)
        );
        let plans = self.all_or_explain(selected, answers)?;
        pinned?;
        let nodes = selected
            .iter()
            .zip(plans)
            .map(|(node, plan)| NodeDeploymentPlan {
                node: node.name.clone(),
                plan,
            })
            .collect::<Vec<_>>();
        // A service that moves starts on its new nodes before it stops on
        // the old one, so every node it moves to has to be in the plan.
        let moves = service_moves(&nodes);
        check_moves_are_planned(&moves, skipped)
            .map_err(|message| MmError::Placement { message })?;
        let networks = self.plan_cluster_networks(selected, &nodes, stored)?;
        let storage = self.plan_storage(&nodes, skipped)?;
        let ingress = self.plan_ingress(&nodes, skipped)?;
        let rollout = self.is_cluster().then(|| cluster_rollout(&nodes));
        Ok(PlannedSections {
            nodes,
            moves,
            networks,
            storage,
            ingress,
            rollout,
        })
    }

    /// Applies the plan `plan_hash` names. A cluster plan rolls out stack
    /// by stack and node by node (`rollout.rs`); the report says what each
    /// node did. An error means nothing was applied.
    pub async fn apply_deployment(
        &self,
        plan_hash: &str,
        progress: NodeProgress<'_>,
    ) -> Result<ClusterApplyReport<DeploymentResult>, MmError> {
        let applied = self.deploy(plan_hash, progress).await;
        // A deployment also applies the commit's users file.
        self.close_kept_connections();
        applied
    }

    async fn deploy(
        &self,
        plan_hash: &str,
        progress: NodeProgress<'_>,
    ) -> Result<ClusterApplyReport<DeploymentResult>, MmError> {
        // The operation's ID comes first, so a log that stops anywhere says
        // what to look up and what to cancel.
        let environment = &self.first().scope.environment;
        progress(&RolloutProgress::new(
            environment.clone(),
            ProgressStep::Operation(format!(
                "operation {}: deploying to '{environment}'. `mm deploy status` shows where it is; `mm deploy cancel {}` stops it",
                crate::domain::rollout::short_id(plan_hash),
                crate::domain::rollout::short_id(plan_hash),
            )),
        ));
        if !self.is_cluster() {
            let node = self.first();
            let applying = async {
                node.connect()?
                    .apply_deployment(node.scope.clone(), plan_hash.to_owned())
                    .await
            };
            let watched = Watched {
                node,
                step: "deployment",
                operation: plan_hash,
            };
            let result = self.with_heartbeat(watched, progress, applying).await?;
            return self.report(plan_hash, vec![applied(result)], Vec::new());
        }
        let reviewed = self.reviewed(plan_hash, RemotePlanKind::Deployment).await?;
        let rollout = reviewed.rollout.clone().ok_or_else(|| MmError::Security {
            message: format!(
                "cluster plan {plan_hash} carries no rollout order; run `mm deploy plan` again"
            ),
        })?;
        let mut outcomes = self
            .roll_out(
                plan_hash,
                &reviewed.nodes,
                (&rollout, reviewed.storage.as_ref()),
                progress,
            )
            .await?;
        finish_moves(&reviewed.nodes, plan_hash, &mut outcomes, progress).await;
        self.report(plan_hash, outcomes, reviewed.skipped)
    }

    /// Plans an access sync from `commit` on every selected node.
    pub async fn plan_team_sync(
        &self,
        commit: &str,
        only: Option<&[String]>,
    ) -> Result<ClusterAccessPlan, MmError> {
        let (selected, skipped) = self.select(only)?;
        let delivered = self.deliver(commit, &selected, &NoProgress).await?;
        let answers = join_all(selected.iter().map(|node| async move {
            node.connect()?
                .plan_team_sync(node.scope.clone(), commit.to_owned())
                .await
        }))
        .await;
        let plans = self.all_or_explain(&selected, answers)?;
        let bindings = plans.iter().map(|plan| &plan.binding).collect::<Vec<_>>();
        if let Some(delivered) = &delivered {
            delivered.check(&bindings)?;
        }
        let plan_hash = self
            .bind(
                RemotePlanKind::TeamSync,
                commit,
                (&selected, &bindings, &skipped),
                SealCarried::default(),
            )
            .await?;
        Ok(ClusterAccessPlan {
            plan_hash,
            target: self.target.clone(),
            environment: self.first().scope.environment.clone(),
            commit: commit.to_owned(),
            expires_at_unix_ms: earliest_expiry(&bindings),
            nodes: selected
                .iter()
                .zip(plans)
                .map(|(node, plan)| NodeAccessPlan {
                    node: node.name.clone(),
                    plan,
                })
                .collect(),
            skipped,
        })
    }

    /// Applies a reviewed access sync on every node of its plan.
    pub async fn apply_team_sync(
        &self,
        plan_hash: &str,
        progress: NodeProgress<'_>,
    ) -> Result<ClusterApplyReport<AccessChange>, MmError> {
        let applied = self.sync_team(plan_hash, progress).await;
        self.close_kept_connections();
        applied
    }

    async fn sync_team(
        &self,
        plan_hash: &str,
        progress: NodeProgress<'_>,
    ) -> Result<ClusterApplyReport<AccessChange>, MmError> {
        if !self.is_cluster() {
            let change = self.sync_one(self.first(), plan_hash, progress).await?;
            return self.report(plan_hash, vec![applied(Some(change))], Vec::new());
        }
        let Reviewed { nodes, skipped, .. } =
            self.reviewed(plan_hash, RemotePlanKind::TeamSync).await?;
        let outcomes = walk_nodes(&nodes, progress, |node| async move {
            self.sync_one(node, plan_hash, progress).await.map(Some)
        })
        .await;
        self.report(plan_hash, outcomes, skipped)
    }

    /// One node's access sync. It may wait its turn behind a deployment
    /// step; the heartbeat says what it waits for.
    async fn sync_one(
        &self,
        node: &RemoteNode,
        plan_hash: &str,
        progress: NodeProgress<'_>,
    ) -> Result<AccessChange, MmError> {
        let watched = Watched {
            node,
            step: "access sync",
            operation: plan_hash,
        };
        let applying = async {
            node.connect()?
                .apply_team_sync(node.scope.clone(), plan_hash.to_owned())
                .await
        };
        self.with_heartbeat(watched, progress, applying).await
    }

    /// The hash that applies the node plans: the one host's own plan hash,
    /// or, for a cluster, the hash over every node section, kept by each of
    /// those nodes.
    async fn bind(
        &self,
        kind: RemotePlanKind,
        commit: &str,
        (selected, bindings, skipped): (&[&RemoteNode], &[&RemotePlanBinding], &[String]),
        carried: SealCarried,
    ) -> Result<String, MmError> {
        if !self.is_cluster() {
            return bindings
                .first()
                .map(|binding| binding.plan_hash.clone())
                .ok_or_else(|| MmError::State {
                    message: "the host returned no plan".to_owned(),
                });
        }
        let sections = selected
            .iter()
            .zip(bindings)
            .map(|(node, binding)| ClusterPlanSection {
                node: self.name_of(node).to_owned(),
                plan_hash: binding.plan_hash.clone(),
            })
            .collect();
        let seal = ClusterPlanSeal::new(
            kind,
            &self.target_scope(),
            commit,
            sections,
            skipped.to_vec(),
            carried,
        )
        .map_err(|message| MmError::State { message })?;
        let kept = join_all(selected.iter().map(|node| {
            let seal = seal.clone();
            async move { node.connect()?.seal_plan(node.scope.clone(), seal).await }
        }))
        .await;
        self.all_or_explain(selected, kept)?;
        Ok(seal.cluster_plan_hash)
    }

    /// The nodes the reviewed cluster plan `plan_hash` covers, in declared
    /// order, and the nodes it leaves out. Every node of the plan must still
    /// hold the same seal; otherwise nothing is applied.
    async fn reviewed(
        &self,
        plan_hash: &str,
        kind: RemotePlanKind,
    ) -> Result<Reviewed<'_>, MmError> {
        let mut answers = join_all(self.nodes.iter().map(|node| async move {
            node.connect()?
                .sealed_plan(node.scope.clone(), plan_hash.to_owned())
                .await
        }))
        .await;
        // Any node that holds the seal says which nodes the plan covers.
        let held = answers.iter().find_map(|answer| match answer {
            Ok(seal) => Some(seal.clone()),
            Err(_) => None,
        });
        let Some(seal) = held else {
            return Err(self.no_reviewed_plan(plan_hash, answers));
        };
        if seal.kind != kind {
            return Err(MmError::Security {
                message: format!(
                    "plan {plan_hash} is {}; apply it with `{}`",
                    match seal.kind {
                        RemotePlanKind::Deployment => "a deployment plan",
                        RemotePlanKind::TeamSync => "a team sync plan",
                    },
                    match seal.kind {
                        RemotePlanKind::Deployment => "mm deploy apply --plan",
                        RemotePlanKind::TeamSync => "mm team sync --plan",
                    }
                ),
            });
        }
        let mut nodes = Vec::with_capacity(seal.sections.len());
        for (node, answer) in self.nodes.iter().zip(answers.drain(..)) {
            if seal.section(self.name_of(node)).is_none() {
                continue;
            }
            match answer {
                Ok(held) if held == seal => nodes.push(node),
                Ok(_) => {
                    return Err(MmError::Security {
                        message: format!(
                            "node '{}' holds a different plan under {plan_hash}; nothing was applied. Run the plan again",
                            self.name_of(node)
                        ),
                    });
                }
                Err(error) => return Err(nothing_applied(in_node(node.name.as_deref(), error))),
            }
        }
        if nodes.len() != seal.sections.len() {
            return Err(MmError::Security {
                message: format!(
                    "plan {plan_hash} covers a node that target '{}' no longer lists; run the plan again",
                    self.target
                ),
            });
        }
        Ok(Reviewed {
            nodes,
            skipped: seal.skipped,
            rollout: seal.rollout,
            storage: seal.storage,
        })
    }

    /// Why no node could hand back the reviewed plan: prefer a node's own
    /// answer over one that merely could not be reached.
    fn no_reviewed_plan(
        &self,
        plan_hash: &str,
        answers: Vec<Result<ClusterPlanSeal, MmError>>,
    ) -> MmError {
        let mut errors = self
            .nodes
            .iter()
            .zip(answers)
            .filter_map(|(node, answer)| match answer {
                Ok(_) => None,
                Err(error) => Some(in_node(node.name.as_deref(), error)),
            })
            .collect::<Vec<_>>();
        let refusal = errors
            .iter()
            .position(|error| !matches!(error, MmError::Unreachable { .. }));
        match refusal {
            Some(index) => errors.swap_remove(index),
            None if errors.is_empty() => MmError::Security {
                message: format!("no node holds reviewed plan {plan_hash}; run the plan again"),
            },
            None => nothing_applied(errors.swap_remove(0)),
        }
    }

    fn report<T>(
        &self,
        plan_hash: &str,
        nodes: Vec<NodeApplyOutcome<T>>,
        skipped: Vec<String>,
    ) -> Result<ClusterApplyReport<T>, MmError> {
        Ok(ClusterApplyReport {
            plan_hash: plan_hash.to_owned(),
            target: self.target.clone(),
            environment: self.first().scope.environment.clone(),
            nodes: nodes
                .into_iter()
                .map(|mut outcome| {
                    if outcome.node.is_empty() {
                        outcome.node.clone_from(&self.target);
                    }
                    outcome
                })
                .collect(),
            skipped,
        })
    }
}

/// A reviewed cluster plan: the nodes it covers, in declared order, the
/// nodes it leaves out, and, for a deployment, the order it rolls out in.
pub(super) struct Reviewed<'a> {
    pub nodes: Vec<&'a RemoteNode>,
    pub skipped: Vec<String>,
    pub rollout: Option<ClusterRollout>,
    /// The plan's shared volumes, when it has any.
    pub storage: Option<SealedStorage>,
}

/// The stages of a deployment plan over `nodes`: what each node renders,
/// with its own policy.
fn cluster_rollout(nodes: &[NodeDeploymentPlan]) -> ClusterRollout {
    let rendered = nodes
        .iter()
        .filter_map(|node| {
            let plan = node.plan.plan.as_ref()?;
            let stacks = plan
                .stacks
                .iter()
                .map(|stack| stack.stack.clone())
                .collect::<Vec<_>>();
            let services = plan
                .node
                .as_ref()
                .map_or(&[][..], |node| &node.services[..]);
            Some((node.node.as_deref()?, stacks, &plan.rollout, services))
        })
        .collect::<Vec<_>>();
    ClusterRollout::of(
        &rendered
            .iter()
            .map(|(node, stacks, policy, services)| NodeRolloutInput {
                node,
                stacks,
                policy,
                services,
            })
            .collect::<Vec<_>>(),
    )
    .knowing_ran_once(nodes.iter().flat_map(|node| node.plan.once.clone()))
    .for_daemons(
        &nodes
            .iter()
            .map(|node| {
                (
                    node.node.clone().unwrap_or_default(),
                    node.plan.binding.daemon_version.clone(),
                )
            })
            .collect::<Vec<_>>(),
        env!("CARGO_PKG_VERSION"),
        &nodes
            .iter()
            .filter_map(|node| node.plan.plan.as_ref())
            .flat_map(|plan| plan.stack_after.clone())
            .collect(),
    )
}

/// Applies each node's section in declared order and stops at the first
/// failure: the failed node is reported with its error and every later node
/// as not started. An access sync is applied this way.
async fn walk_nodes<'a, T, Applying>(
    nodes: &[&'a RemoteNode],
    progress: NodeProgress<'_>,
    mut apply: impl FnMut(&'a RemoteNode) -> Applying,
) -> Vec<NodeApplyOutcome<T>>
where
    Applying: Future<Output = Result<Option<T>, MmError>>,
{
    let mut outcomes = Vec::with_capacity(nodes.len());
    let mut stopped = false;
    for node in nodes.iter().copied() {
        let name = node.name.clone().unwrap_or_default();
        if stopped {
            outcomes.push(NodeApplyOutcome {
                node: name,
                state: NodeApplyState::NotStarted,
                result: None,
                error: None,
                rolled_back: false,
                log: Vec::new(),
                stopped: Vec::new(),
                unchanged: false,
                code: None,
                kept: false,
            });
            continue;
        }
        if !name.is_empty() {
            progress(&RolloutProgress::new(name.clone(), ProgressStep::Applying));
        }
        match apply(node).await {
            Ok(result) => outcomes.push(NodeApplyOutcome {
                node: name,
                state: NodeApplyState::Applied,
                result,
                error: None,
                rolled_back: false,
                log: Vec::new(),
                stopped: Vec::new(),
                unchanged: false,
                code: None,
                kept: false,
            }),
            Err(error) => {
                stopped = true;
                // A node busy with another change refused this one before
                // it touched anything: it is unchanged, and says why.
                let busy =
                    matches!(&error, MmError::Remote { code, .. } if *code == "MM_HOST_BUSY");
                outcomes.push(NodeApplyOutcome {
                    node: name,
                    state: if busy {
                        NodeApplyState::NotStarted
                    } else {
                        NodeApplyState::Failed
                    },
                    result: None,
                    error: Some(error.to_string()),
                    rolled_back: false,
                    log: Vec::new(),
                    stopped: Vec::new(),
                    unchanged: false,
                    code: Some(error.code().to_owned()),
                    kept: false,
                });
            }
        }
    }
    outcomes
}

/// The one outcome of applying to a target that names one host.
fn applied<T>(result: Option<T>) -> NodeApplyOutcome<T> {
    NodeApplyOutcome {
        node: String::new(),
        state: NodeApplyState::Applied,
        result,
        error: None,
        rolled_back: false,
        log: Vec::new(),
        stopped: Vec::new(),
        unchanged: false,
        code: None,
        kept: false,
    }
}

/// The last step of a cluster deployment. A service that moved keeps
/// running on its old node while the walk starts it on the new ones; once
/// every node has applied, each old node stops it. After a failed walk
/// nothing is stopped: the service keeps running where it was.
pub(super) async fn finish_moves(
    nodes: &[&RemoteNode],
    plan_hash: &str,
    outcomes: &mut [NodeApplyOutcome<DeploymentResult>],
    progress: NodeProgress<'_>,
) {
    if outcomes
        .iter()
        .any(|outcome| outcome.state != NodeApplyState::Applied)
    {
        return;
    }
    for (node, outcome) in nodes.iter().zip(outcomes.iter_mut()) {
        let moved = outcome
            .result
            .as_ref()
            .map(|result| result.moved_away.as_slice())
            .unwrap_or_default();
        if moved.is_empty() {
            continue;
        }
        let names = moved
            .iter()
            .map(|service| {
                format!(
                    "{} (now on {})",
                    service.service,
                    service.moves_to.join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        progress(&RolloutProgress::new(
            outcome.node.clone(),
            ProgressStep::Note(format!("stopping {names}")),
        ));
        let finished = match node.connect() {
            Ok(transport) => {
                transport
                    .finish_moves(node.scope.clone(), plan_hash.to_owned())
                    .await
            }
            Err(error) => Err(error),
        };
        match finished {
            Ok(stopped) => outcome.stopped = stopped,
            Err(error) => {
                outcome.state = NodeApplyState::Failed;
                outcome.error = Some(format!(
                    "the node applied its section, but the services that moved away were not stopped: {names}: {error}. They still run on this node beside their new containers"
                ));
                return;
            }
        }
    }
}

fn earliest_expiry(bindings: &[&RemotePlanBinding]) -> i64 {
    bindings
        .iter()
        .map(|binding| binding.expires_at_unix_ms)
        .min()
        .unwrap_or_default()
}

/// A failure before the walk started changed no node.
fn nothing_applied(error: MmError) -> MmError {
    match error {
        MmError::Unreachable {
            subject,
            problem,
            help,
        } => MmError::Unreachable {
            subject,
            problem,
            help: format!(
                "{help}. Nothing was applied. To apply without this node, plan again with `--nodes` naming the others"
            ),
        },
        other => other,
    }
}
