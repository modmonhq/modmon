//! The cards of `mm target setup` for a cluster target: the plan, and what an apply did.

use std::time::Duration;

use crate::domain::cluster_setup::{ClusterSetupPlan, ClusterSetupResult, HostKeyOrigin};
use crate::domain::mesh::{MeshNodeOrigin, MeshNodePlan, PeerAction};
use crate::domain::repo_target::MeshConfig;
use crate::presentation::inline::{Card, Mark, Value, count, duration, short, short_fingerprint};
use crate::presentation::tui::Tone;

/// The plan: what setup does to which nodes, led by what changes.
#[must_use]
pub fn setup_plan_card(plan: &ClusterSetupPlan) -> Card {
    let mut card = Card::new(format!("Cluster setup for {}", plan.environment))
        .badge(format!("plan {}", short(&plan.plan_hash)));
    let mut target = vec![
        Value::new(plan.target.clone()),
        Value::new(count(plan.nodes.len(), "node", "nodes")),
        Value::new(plan.runtime.name()),
    ];
    if !plan.skipped.is_empty() {
        target.push(Value::tone(
            format!("{} left out", plan.skipped.len()),
            Tone::Warn,
        ));
    }
    card.fields("target", target);
    card.fields("mesh", mesh_values(&plan.cluster.mesh));
    card.field("storage", storage_value(plan));
    card.field(
        "users",
        format!("from commit {}", short(&plan.access_commit)),
    );
    let mut secrets = vec![Value::new(format!("{} pushed", plan.secrets.len()))];
    if !plan.unset_secrets.is_empty() {
        secrets.push(Value::tone(
            format!("{} unset here", plan.unset_secrets.len()),
            Tone::Warn,
        ));
    }
    card.fields("secrets", secrets);
    if let Some(store) = &plan.secret_store {
        card.fields("secret store", store_values(store));
    }
    card.field(
        "delivery",
        if plan.pushes() {
            Value::new("mm sends each commit's files")
        } else {
            Value::new(format!(
                "nodes fetch with the deploy key in {}",
                plan.deploy_key.secret
            ))
        },
    );
    card.table(
        &["node", "address", "mesh", "host key", ""],
        plan.nodes
            .iter()
            .map(|node| {
                vec![
                    Value::new(node.name.clone()),
                    Value::new(format!("{}:{}", node.host, node.port)),
                    Value::new(node.private_address.to_string()),
                    Value::dim(short_fingerprint(&node.host_key_fingerprint)),
                    match node.host_key_origin {
                        HostKeyOrigin::Recorded => Value::dim("recorded"),
                        HostKeyOrigin::FirstUse => Value::tone("first use", Tone::Warn),
                        HostKeyOrigin::Replaced => Value::tone("replaced", Tone::Bad),
                    },
                ]
            })
            .collect(),
    );
    let (changed, unchanged) = plan
        .mesh
        .as_ref()
        .map(|mesh| mesh_changes(&mesh.nodes))
        .unwrap_or_default();
    card.changes(
        changed,
        (unchanged > 0).then(|| format!("{} unchanged", count(unchanged, "node", "nodes"))),
    );
    let mut notes = plan
        .notices
        .iter()
        .map(|notice| (Tone::Warn, notice.clone()))
        .collect::<Vec<_>>();
    if !plan.unset_secrets.is_empty() {
        notes.push((
            Tone::Info,
            format!(
                "Not pushed, no value here: {}",
                plan.unset_secrets.join(", ")
            ),
        ));
    }
    notes.extend(
        plan.secret_store
            .iter()
            .flat_map(crate::domain::secret_store::ClusterStorePlan::changes)
            .map(|line| (Tone::Info, format!("secret store: {line}"))),
    );
    card.notes(notes);
    card.command("Apply this plan", format!("{} --yes", plan.apply_command()));
    card
}

fn store_values(store: &crate::domain::secret_store::ClusterStorePlan) -> Vec<Value> {
    let mut values = vec![
        Value::new(format!("{} (OpenBao {})", store.provider, store.version)),
        Value::new(format!(
            "members {}",
            store
                .members
                .iter()
                .map(|member| member.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        if store.initialised {
            Value::dim("initialised")
        } else {
            Value::tone("initialised by this apply", Tone::Warn)
        },
    ];
    if !store.rejoins.is_empty() {
        values.push(Value::tone(
            format!("rejoins {}", store.rejoins.join(", ")),
            Tone::Warn,
        ));
    }
    values
}

/// What an apply did, node by node, and how to record it.
#[must_use]
pub fn setup_result_card(result: &ClusterSetupResult, took: Duration) -> Card {
    let mut card = Card::new(format!(
        "Set up {} for {}",
        result.target, result.environment
    ))
    .badge(duration(took));
    card.fields(
        "secrets",
        vec![Value::new(format!("{} changed", result.secrets_changed))],
    );
    card.field(
        "users",
        format!("from commit {}", short(&result.access_commit)),
    );
    card.table(
        &["node", "mesh", "mm", "daemon"],
        result
            .nodes
            .iter()
            .map(|node| {
                vec![
                    Value::new(node.name.clone()),
                    Value::new(node.private_address.to_string()),
                    Value::new(node.version.clone()),
                    Value::dim(short_fingerprint(&node.daemon_fingerprint)),
                ]
            })
            .collect(),
    );
    card.command(
        "Record the nodes' keys and addresses in the repo",
        result.record_command.clone(),
    );
    card
}

fn mesh_values(mesh: &MeshConfig) -> Vec<Value> {
    use crate::domain::repo_target::MESH_INTERFACE;
    match mesh {
        MeshConfig::Wireguard(wireguard) => vec![
            Value::new(format!("WireGuard {MESH_INTERFACE}")),
            Value::new(wireguard.subnet.to_string()),
            Value::new(format!("UDP {}", wireguard.port)),
            Value::new(format!("MTU {}", wireguard.mtu)),
        ],
        MeshConfig::Interface(interface) => {
            vec![Value::new(format!("existing interface {interface}"))]
        }
    }
}

fn storage_value(plan: &ClusterSetupPlan) -> Value {
    if !plan
        .steps
        .contains(&crate::domain::cluster_setup::ClusterSetupStep::ServeStorage)
    {
        return Value::dim("not served by this setup");
    }
    match plan.nodes.iter().find(|node| node.name == plan.storage) {
        Some(node) => Value::new(format!(
            "{} serves shared volumes on {}",
            plan.storage, node.private_address
        )),
        None => Value::new(format!("{} serves shared volumes", plan.storage)),
    }
}

/// One line per node whose mesh changes, and how many do not change.
fn mesh_changes(nodes: &[MeshNodePlan]) -> (Vec<(Mark, String, String)>, usize) {
    let mut changed = Vec::new();
    let mut unchanged = 0;
    for node in nodes {
        if !node.changes() {
            unchanged += 1;
            continue;
        }
        let mut parts = Vec::new();
        match node.origin {
            MeshNodeOrigin::New => parts.push(if node.public_key.is_none() {
                "joins the mesh and makes its key".to_owned()
            } else {
                "joins the mesh".to_owned()
            }),
            MeshNodeOrigin::Legacy => parts.push("replaces wg0, keeps its key".to_owned()),
            MeshNodeOrigin::Current => {}
        }
        parts.extend(node.interface_changes.iter().cloned());
        for (action, verb) in [
            (PeerAction::Add, "adds"),
            (PeerAction::Change, "changes"),
            (PeerAction::Remove, "removes"),
        ] {
            let peers = node
                .peers
                .iter()
                .filter(|peer| peer.action == action)
                .map(|peer| peer.name.as_str())
                .collect::<Vec<_>>();
            if !peers.is_empty() {
                parts.push(format!(
                    "{verb} peer{} {}",
                    if peers.len() == 1 { "" } else { "s" },
                    peers.join(", ")
                ));
            }
        }
        let mark = if node.origin == MeshNodeOrigin::New {
            Mark::Add
        } else {
            Mark::Change
        };
        changed.push((mark, node.node.clone(), parts.join("; ")));
    }
    (changed, unchanged)
}

/// The plan of `mm secret provider restore` for a cluster's store: what it
/// puts back where, and the command that does.
pub(crate) fn write_store_restore_plan(
    plan: &crate::domain::secret_store::StoreRestorePlan,
    file: &std::path::Path,
    format: crate::presentation::cli::OutputFormat,
) -> Result<(), crate::domain::error::MmError> {
    use std::io::Write as _;
    if format == crate::presentation::cli::OutputFormat::Json {
        return super::write_json(plan);
    }
    let members = plan
        .members
        .iter()
        .map(|member| format!("{} ({})", member.name, member.address))
        .collect::<Vec<_>>()
        .join(", ");
    let lines = [
        format!(
            "Restore {} ({}) from the snapshot taken on {} at {}",
            plan.provider,
            plan.environment,
            plan.taken_on,
            crate::domain::rollout::utc_time(plan.taken_at_unix_ms)
        ),
        format!("  replaces every value, version and login the store holds now, on {members}"),
        "  every node then gets the keys the snapshot opens with, and the recovery copy here becomes the snapshot's".to_owned(),
        format!(
            "Apply: mm -e {} secret provider restore {} --file {} --plan {} --yes",
            plan.environment,
            plan.provider,
            file.display(),
            plan.plan_hash
        ),
    ];
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for line in lines {
        writeln!(writer, "{}", super::terminal_safe(&line)).map_err(super::output_error)?;
    }
    Ok(())
}
