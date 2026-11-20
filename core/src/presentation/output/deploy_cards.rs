//! The cards of `mm deploy`.

use std::time::Duration;

use serde::Serialize;

use crate::domain::deployment::{DeploymentPlan, DeploymentResult, DeploymentRouteDiff};
use crate::domain::remote::{ClusterApplyReport, ClusterDeploymentPlan, NodeApplyState};
use crate::domain::rollout::utc_time;
use crate::presentation::inline::{Card, Mark, Value, count, duration, short};
use crate::presentation::tui::Tone;

/// A remote environment's plan: one host, or every node of a cluster.
#[must_use]
pub fn remote_plan_card(plan: &ClusterDeploymentPlan, cluster: bool) -> Card {
    let mut card = Card::new(format!("Deployment to {}", plan.environment))
        .badge(format!("plan {}", short(&plan.plan_hash)));
    remote_fields(&mut card, plan, cluster);
    remote_nodes(&mut card, plan);
    remote_changes(&mut card, plan, cluster);
    card.command(
        "Apply within ten minutes",
        format!(
            "mm --environment {} deploy apply --plan {} --yes",
            plan.environment, plan.plan_hash
        ),
    );
    card
}

fn remote_fields(card: &mut Card, plan: &ClusterDeploymentPlan, cluster: bool) {
    let mut target = vec![Value::new(plan.target.clone())];
    if cluster {
        target.push(Value::new(count(plan.nodes.len(), "node", "nodes")));
    }
    if !plan.skipped.is_empty() {
        target.push(Value::tone(
            format!("{} left out", plan.skipped.join(", ")),
            Tone::Warn,
        ));
    }
    card.fields("target", target);
    card.field("commit", short(&plan.commit).to_owned());
    super::release::release_fields(card, super::release::cluster_release(plan));
    if let Some(bundle) = &plan.bundle {
        card.field(
            "files",
            format!(
                "{} files, {}; {}",
                bundle.files,
                crate::domain::delivery::human_size(bundle.bytes),
                if bundle.sent_to.is_empty() {
                    "every node held them".to_owned()
                } else {
                    format!("sent to {}", bundle.sent_to.join(", "))
                }
            ),
        );
    }
    if let Some(ingress) = plan
        .ingress
        .as_ref()
        .filter(|ingress| !ingress.routes.is_empty())
    {
        card.fields(
            "ingress",
            vec![
                Value::new(count(ingress.routes.len(), "route", "routes")),
                Value::new(format!("served by {}", ingress.nodes.join(", "))),
            ],
        );
    }
    if let Some(rollout) = &plan.rollout {
        let stages = rollout
            .stages
            .iter()
            .map(|stage| stage.describe())
            .collect::<Vec<_>>();
        card.fields(
            "rollout",
            if stages.is_empty() {
                vec![Value::dim("no stack runs on these nodes")]
            } else {
                stages.into_iter().map(Value::new).collect()
            },
        );
    }
    card.field("expires", Value::dim(utc_time(plan.expires_at_unix_ms)));
}

fn remote_nodes(card: &mut Card, plan: &ClusterDeploymentPlan) {
    card.table(
        &["node", "runs", "images", "users"],
        plan.nodes
            .iter()
            .map(|section| {
                let name = section.node.as_deref().unwrap_or(&plan.target).to_owned();
                let runs = section
                    .plan
                    .plan
                    .as_ref()
                    .map_or_else(|| Value::dim("nothing"), stacks_value);
                let images = section.plan.plan.as_ref().map_or(0, image_count);
                let users = section.plan.access.entries.len();
                vec![
                    Value::new(name),
                    runs,
                    Value::dim(images.to_string()),
                    if users == 0 {
                        Value::dim("unchanged")
                    } else {
                        Value::tone(count(users, "change", "changes"), Tone::Warn)
                    },
                ]
            })
            .collect(),
    );
}

fn remote_changes(card: &mut Card, plan: &ClusterDeploymentPlan, cluster: bool) {
    let mut changed = Vec::new();
    for moved in &plan.moves {
        changed.push((
            Mark::Change,
            format!("{}/{}", moved.stack, moved.service),
            format!("moves from {} to {}", moved.from, moved.to.join(", ")),
        ));
    }
    for section in &plan.nodes {
        let name = section.node.as_deref().unwrap_or(&plan.target);
        if let Some(node_plan) = &section.plan.plan {
            changed.extend(node_changes(name, node_plan));
        }
    }
    if !cluster && let Some(node_plan) = plan.nodes.first().and_then(|node| node.plan.plan.as_ref())
    {
        changed.extend(route_changes(&node_plan.route_diff));
    }
    changed.extend(staged_changes(plan, cluster));
    let quiet = changed.is_empty();
    card.changes(
        changed,
        quiet.then(|| "no route, placement or security change".to_owned()),
    );
    let mut notes = Vec::new();
    for section in &plan.nodes {
        let name = section.node.as_deref().unwrap_or(&plan.target);
        let deployment = section.plan.plan.as_ref();
        if let Some(line) = deployment.and_then(DeploymentPlan::adopted_routes_line) {
            notes.push((Tone::Warn, format!("{name} {line}")));
        }
    }
    for unfinished in &plan.unfinished {
        notes.push((
            Tone::Warn,
            format!(
                "{} holds a rollout of commit {} that {} started and nobody finished; this plan undoes it first",
                unfinished.node,
                short(&unfinished.commit),
                unfinished.started_by
            ),
        ));
    }
    card.notes(notes);
}

/// The staged secrets and variables the plan makes live, once per change
/// with the nodes that hold it.
fn staged_changes(plan: &ClusterDeploymentPlan, cluster: bool) -> Vec<(Mark, String, String)> {
    use crate::domain::staged::{StagedChangeKind, ValueKind};
    let mut grouped = std::collections::BTreeMap::new();
    for section in &plan.nodes {
        let name = section.node.as_deref().unwrap_or(&plan.target);
        for item in &section.plan.staged {
            let entry = grouped
                .entry((item.kind, item.key.clone(), item.revision.clone()))
                .or_insert_with(|| (item.clone(), Vec::new()));
            entry.1.push(name.to_owned());
        }
    }
    grouped
        .into_values()
        .map(|(item, nodes)| {
            let mark = match item.change {
                StagedChangeKind::Added => Mark::Add,
                StagedChangeKind::Changed => Mark::Change,
                StagedChangeKind::Removed => Mark::Remove,
            };
            let mut detail = vec![match (item.kind, item.change) {
                (ValueKind::Secret, StagedChangeKind::Changed) => {
                    "secret, revision changed".to_owned()
                }
                (ValueKind::Variable, StagedChangeKind::Changed) => {
                    "variable, revision changed".to_owned()
                }
                (kind, _) => kind.as_str().to_owned(),
            }];
            detail.push(format!("staged by {}", item.staged_by));
            if !item.services.is_empty() {
                detail.push(format!("for {}", item.services.join(", ")));
            }
            if cluster {
                detail.push(format!("on {}", nodes.join(", ")));
            }
            (mark, item.key, detail.join(", "))
        })
        .collect()
}

/// A plan for an environment on this machine.
#[must_use]
pub fn local_plan_card(plan: &DeploymentPlan, with: &[String]) -> Card {
    let mut card = Card::new(format!("Deployment to {}", plan.environment))
        .badge(format!("plan {}", short(&plan.plan_hash)));
    card.fields(
        "target",
        vec![
            Value::new(plan.target_name.clone()),
            Value::new(plan.project_name.clone()),
        ],
    );
    card.field(
        "values",
        format!(
            "{} (from {})",
            plan.environment_review.delivery.mode,
            plan.environment_review.delivery.provenance.as_str()
        ),
    );
    super::release::release_fields(&mut card, plan.release.as_ref());
    if let Some(build) = &plan.local_build {
        card.field("builds", super::local_build_line(build));
    }
    card.table(
        &["stack", "services", "images"],
        plan.stacks
            .iter()
            .map(|stack| {
                let services = &stack.artifact.manifest.services;
                vec![
                    Value::new(stack.stack.clone()),
                    Value::new(
                        services
                            .iter()
                            .map(|service| service.service.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    Value::dim(
                        services
                            .iter()
                            .filter(|service| service.image.is_some())
                            .count()
                            .to_string(),
                    ),
                ]
            })
            .collect(),
    );
    let mut changed = node_changes(&plan.target_name, plan);
    changed.extend(route_changes(&plan.route_diff));
    let quiet = changed.is_empty();
    card.changes(
        changed,
        quiet.then(|| "no route or security change".to_owned()),
    );
    card.notes(local_notes(plan));
    let apply = if with.is_empty() {
        format!(
            "mm --environment {} deploy apply --plan {}{} --yes",
            plan.environment,
            plan.plan_hash,
            plan.release
                .as_ref()
                .map(|release| release.request().arguments())
                .unwrap_or_default()
        )
    } else {
        format!(
            "mm --environment {} dev up{} --plan {} --yes",
            plan.environment,
            with.iter()
                .map(|unit| format!(" --with {unit}"))
                .collect::<String>(),
            plan.plan_hash
        )
    };
    card.command("Apply it", apply);
    card
}

/// The services the plan leaves off on this machine, the secret files that
/// belong to the user mm runs as instead of the ids they name, and what the
/// development mounts mask.
fn local_notes(plan: &DeploymentPlan) -> Vec<(Tone, String)> {
    let mut notes = (plan.adopted_routes_line().into_iter())
        .map(|line| (Tone::Warn, format!("The plan {line}")))
        .chain(
            super::environment::off_locally_notes(&plan.excluded_services)
                .into_iter()
                .map(|note| (Tone::Idle, note)),
        )
        .collect::<Vec<_>>();
    notes.extend(plan.secret_file_owners.iter().map(|file| {
        let tone = if file.readable_by_all() {
            Tone::Idle
        } else {
            Tone::Warn
        };
        (tone, file.line())
    }));
    notes.extend(masked_notes(plan));
    notes
}

/// What the development mounts mask, path by path.
fn masked_notes(plan: &DeploymentPlan) -> impl Iterator<Item = (Tone, String)> + '_ {
    plan.stacks
        .iter()
        .flat_map(|stack| &stack.artifact.manifest.masked_paths)
        .map(|path| {
            let tone = match path.kind {
                crate::domain::development_mount::MaskedPathKind::Missing => Tone::Warn,
                _ => Tone::Idle,
            };
            (tone, path.line())
        })
}

/// What one engine's apply did.
#[must_use]
pub fn deployment_result_card(result: &DeploymentResult, took: Duration) -> Card {
    let mut card = Card::new(format!("Deployed to {}", result.environment)).badge(duration(took));
    card.field(
        "stacks",
        if result.applied_stacks.is_empty() {
            Value::dim("none")
        } else {
            Value::new(result.applied_stacks.join(", "))
        },
    );
    if !result.removed_stacks.is_empty() {
        card.field("removed", result.removed_stacks.join(", "));
    }
    card.field("routes", result.route_count.to_string());
    card
}

/// What each node did, in apply order, whether or not the apply went through.
#[must_use]
pub fn apply_report_card<T: Serialize>(
    report: &ClusterApplyReport<T>,
    title: &str,
    describe: impl Fn(Option<&T>) -> String,
    took: Duration,
) -> Card {
    let mut card = Card::new(format!("{title} on {}", report.environment)).badge(duration(took));
    let mut rows = Vec::new();
    let mut notes = Vec::new();
    for node in &report.nodes {
        let (state, detail) = match node.state {
            NodeApplyState::Applied => (
                Value::tone("applied", Tone::Ok),
                Value::new(describe(node.result.as_ref())),
            ),
            NodeApplyState::Failed => (
                Value::tone("failed", Tone::Bad),
                Value::new(format!(
                    "{}{}",
                    node.error.as_deref().unwrap_or("no reason given"),
                    if node.rolled_back {
                        "; rolled back"
                    } else if node.unchanged {
                        "; nothing changed here"
                    } else {
                        ""
                    }
                )),
            ),
            NodeApplyState::RolledBack => (
                Value::tone("rolled back", Tone::Warn),
                Value::dim("runs what it ran before"),
            ),
            NodeApplyState::Unfinished => (
                Value::tone("unfinished", Tone::Warn),
                Value::new(
                    node.error
                        .clone()
                        .unwrap_or_else(|| "left as it is".to_owned()),
                ),
            ),
            NodeApplyState::NotStarted => (
                Value::dim("not started"),
                Value::dim(node.error.clone().unwrap_or_else(|| "unchanged".to_owned())),
            ),
        };
        rows.push(vec![Value::new(node.node.clone()), state, detail]);
        if !node.log.is_empty() {
            notes.push((
                Tone::Bad,
                format!(
                    "{}, last log lines of the failed container: {}",
                    node.node,
                    node.log.join(" ⏎ ")
                ),
            ));
        }
    }
    for node in &report.skipped {
        rows.push(vec![
            Value::new(node.clone()),
            Value::dim("left out"),
            Value::dim("unchanged"),
        ]);
    }
    card.table(&["node", "state", ""], rows);
    card.notes(notes);
    card
}

fn stacks_value(plan: &DeploymentPlan) -> Value {
    if plan.stacks.is_empty() {
        return Value::dim("nothing");
    }
    Value::new(
        plan.stacks
            .iter()
            .map(|stack| stack.stack.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn image_count(plan: &DeploymentPlan) -> usize {
    plan.stacks
        .iter()
        .flat_map(|stack| &stack.artifact.manifest.services)
        .filter(|service| service.image.is_some())
        .count()
}

/// Stacks a node stops running and security changes, as change lines.
fn node_changes(node: &str, plan: &DeploymentPlan) -> Vec<(Mark, String, String)> {
    let mut changed = plan
        .removed_stacks
        .iter()
        .map(|stack| {
            (
                Mark::Remove,
                node.to_owned(),
                format!("stops stack {}", stack.stack),
            )
        })
        .collect::<Vec<_>>();
    changed.extend(
        plan.runtime_changes
            .iter()
            .map(|change| (Mark::Change, node.to_owned(), change.line())),
    );
    changed
}

fn route_changes(diff: &DeploymentRouteDiff) -> Vec<(Mark, String, String)> {
    let scheme = |tls: bool| if tls { "https" } else { "http" };
    let mut changed = Vec::new();
    for route in &diff.added {
        changed.push((
            Mark::Add,
            format!(
                "{}://{}{}",
                scheme(route.behavior.tls),
                route.host,
                route.path_prefix
            ),
            super::route_destination(&route.behavior),
        ));
    }
    for route in &diff.changed {
        changed.push((
            Mark::Change,
            format!("{}{}", route.host, route.path_prefix),
            format!(
                "{} → {}",
                super::route_destination(&route.before),
                super::route_destination(&route.after)
            ),
        ));
    }
    for route in &diff.removed {
        changed.push((
            Mark::Remove,
            format!(
                "{}://{}{}",
                scheme(route.behavior.tls),
                route.host,
                route.path_prefix
            ),
            super::route_destination(&route.behavior),
        ));
    }
    changed
}
