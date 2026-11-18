//! Setting up some nodes of a cluster while others cannot be reached
//! (`--nodes`). A node that is left out is never contacted; steps that need
//! it limit themselves and the plan says how.

use crate::domain::cluster_setup::{ClusterSetupStep, ClusterSkippedNode};
use crate::domain::error::MmError;
use crate::domain::remote::unreachable_help;
use crate::domain::repo_target::{MeshConfig, RepoTargetConfig, RepoTargetNode};

/// Refuses a `--nodes` list that names a node the target does not have.
pub(super) fn check_names(target: &RepoTargetConfig, only: &[String]) -> Result<(), MmError> {
    let known = target
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    match only.iter().find(|name| !known.contains(&name.as_str())) {
        Some(unknown) => Err(MmError::Target {
            message: format!(
                "--nodes names '{unknown}', which the target does not list; its nodes are {}",
                known.join(", ")
            ),
        }),
        None => Ok(()),
    }
}

/// The steps a setup runs, with a notice for each one a left-out node limits.
pub(super) fn steps(
    target: &RepoTargetConfig,
    storage: &str,
    skipped: &[ClusterSkippedNode],
    notices: &mut Vec<String>,
) -> Vec<ClusterSetupStep> {
    let on_interface = on_existing_interface(target, notices);
    if skipped.is_empty() {
        return ClusterSetupStep::ALL
            .into_iter()
            .filter(|step| !(on_interface && *step == ClusterSetupStep::ServeStorage))
            .collect();
    }
    let names = skipped
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    notices.push(format!(
        "left out: {names}. Setup does not contact {}; {} the mm version and configuration {} now",
        if skipped.len() == 1 { "it" } else { "them" },
        if skipped.len() == 1 {
            "it keeps"
        } else {
            "they keep"
        },
        if skipped.len() == 1 {
            "it has"
        } else {
            "they have"
        },
    ));
    if matches!(target.cluster.mesh, MeshConfig::Wireguard(_)) {
        notices.push(format!(
            "mesh: the nodes set up now keep the WireGuard peers they already have for {names}; {names} will not learn about changes to the other nodes until a setup includes {}",
            if skipped.len() == 1 { "it" } else { "them" },
        ));
    }
    let storage_skipped = skipped.iter().any(|node| node.name == storage);
    if storage_skipped {
        notices.push(format!(
            "storage: {storage} serves shared volumes and is left out, so the serve-storage step does not run and its export stays as it is"
        ));
    }
    ClusterSetupStep::ALL
        .into_iter()
        .filter(|step| {
            !((storage_skipped || on_interface) && *step == ClusterSetupStep::ServeStorage)
        })
        .collect()
}

/// Whether the nodes share an interface of their own instead of mm's
/// mesh. No node serves shared storage then, and the plan says so.
fn on_existing_interface(target: &RepoTargetConfig, notices: &mut Vec<String>) -> bool {
    let MeshConfig::Interface(interface) = &target.cluster.mesh else {
        return false;
    };
    notices.push(format!(
        "storage: no node serves shared volumes. They travel over the WireGuard mesh mm creates, and this target uses the existing interface {interface}; a stack that lists `shared-volumes` is refused here"
    ));
    true
}

fn subject(node: &RepoTargetNode) -> String {
    format!("node '{}' ({}:{})", node.name, node.host, node.port)
}

fn without(target: &RepoTargetConfig, node: &RepoTargetNode) -> String {
    let others = target
        .nodes
        .iter()
        .map(|other| other.name.as_str())
        .filter(|name| *name != node.name)
        .collect::<Vec<_>>();
    unreachable_help(&[&node.name], &others)
}

/// A node whose SSH port gave no host key: say which node, and how to set
/// up the others without it.
pub(super) fn unreachable(
    target: &RepoTargetConfig,
    node: &RepoTargetNode,
    error: &MmError,
) -> MmError {
    // A host that is down gives the scan nothing to report.
    let reason = match error {
        MmError::ProcessFailed { stderr, .. } if stderr.trim().is_empty() => {
            "nothing answered the key scan".to_owned()
        }
        MmError::ProcessFailed { stderr, .. } => stderr.trim().to_owned(),
        other => other.to_string(),
    };
    MmError::Unreachable {
        subject: subject(node),
        problem: format!("did not present an SSH host key ({reason})"),
        help: format!(
            "Check that the host is up and that this machine can reach its SSH port. {}",
            without(target, node)
        ),
    }
}

/// A node that answers with another host key than the repo records.
pub(super) fn foreign_host_key(target: &RepoTargetConfig, node: &RepoTargetNode) -> MmError {
    MmError::Security {
        message: format!(
            "{} presents a different SSH host key than the repo records. If it was rebuilt, plan again with --trust-new-host-keys. If it was not, something else answers on that address. {}",
            subject(node),
            without(target, node)
        ),
    }
}
