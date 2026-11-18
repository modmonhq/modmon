//! `mm target nodes`: every node of the selected environment's target, and
//! the addresses of the networks that span them.

use super::OutputFormat;
use crate::composition;
use crate::domain::error::MmError;
use crate::presentation::inline;
use crate::presentation::output;

/// Lists the nodes and exits 3 when any of them is not ready, so a script
/// can gate on it; the list itself is printed either way.
pub(super) async fn run(invocation: &super::dispatch::Invocation) -> Result<u8, MmError> {
    let progress = inline::Progress::new(invocation.format);
    let (target, nodes, addresses) = inline::step(
        &progress,
        ("nodes", "Asking every node"),
        composition::target_nodes_and_addresses(
            &invocation.selection,
            invocation.home(),
            composition::RemoteIdentity::User,
        ),
        |(_, nodes, _)| {
            Some(format!(
                "{} of {} answered",
                nodes.iter().filter(|node| node.reachable).count(),
                nodes.len()
            ))
        },
    )
    .await?;
    progress.finish();
    output::write_node_reports(&target, &nodes, &addresses, invocation.format)?;
    let ready = nodes.iter().all(|node| node.reachable);
    if !ready && invocation.format == OutputFormat::Human {
        eprintln!(
            "Not every node is ready. To act on the others, pass `--nodes` to `mm deploy plan`, `mm team sync`, or `mm target setup plan`."
        );
    }
    Ok(if ready { 0 } else { 3 })
}
