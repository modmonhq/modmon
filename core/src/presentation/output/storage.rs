//! The shared volumes of a cluster target: their status, the lines of a
//! deployment plan, and what deleting one removes.

use super::*;
use crate::domain::shared_storage::{
    ClusterStoragePlan, STORAGE_ROOT, StorageReport, VolumeDeletePlan, VolumeDeleted, VolumeReport,
};

/// A size in the unit that keeps it short: `340 B`, `12.4 MiB`.
fn disk_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024 * 1024 && unit + 2 < UNITS.len() {
        value /= 1024;
        unit += 1;
    }
    format!(
        "{}.{} {}",
        value / 1024,
        (value % 1024) * 10 / 1024,
        UNITS[unit + 1]
    )
}

/// `12.4 MiB in 340 files`, or `at least …` when counting stopped early.
fn contents(bytes: Option<u64>, files: Option<u64>, counted: bool) -> String {
    match (bytes, files) {
        (Some(bytes), Some(files)) => format!(
            "{}{} in {files} file{}",
            if counted { "" } else { "at least " },
            disk_size(bytes),
            if files == 1 { "" } else { "s" }
        ),
        _ => "size not counted".to_owned(),
    }
}

fn volume_line(volume: &VolumeReport) -> String {
    let name = format!("{}/{}", volume.stack, volume.volume);
    if !volume.stored {
        return format!(
            "  {:<24} not created yet; the next deployment that uses it creates it",
            terminal_safe(&name)
        );
    }
    let mounted = if volume.mounted_on.is_empty() {
        "mounted nowhere".to_owned()
    } else {
        format!("mounted on {}", volume.mounted_on.join(", "))
    };
    let mut line = format!(
        "  {:<24} {}; {}",
        terminal_safe(&name),
        contents(volume.bytes, volume.files, volume.counted),
        terminal_safe(&mounted)
    );
    if !volume.declared {
        line.push_str(&format!(
            "\n  {:<24} not in the configuration any more; its files are kept. `mm target storage delete {}{}` removes them",
            "",
            terminal_safe(&volume.volume),
            terminal_safe(&format!(" --stack {}", volume.stack)),
        ));
    }
    line
}

/// `mm target storage status`: the storage node, every volume with its
/// size and the nodes that mount it, each node's reach, then what is wrong
/// and what to do.
pub(crate) fn write_storage_report(
    report: &StorageReport,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    let disk = match (report.free_bytes, report.total_bytes) {
        (Some(free), Some(total)) => {
            format!(", {} free of {}", disk_size(free), disk_size(total))
        }
        _ => String::new(),
    };
    writeln!(
        writer,
        "Target {}: {} keeps shared volumes in {STORAGE_ROOT}{disk}",
        terminal_safe(&report.target),
        terminal_safe(&report.storage_node),
    )
    .map_err(output_error)?;
    if report.volumes.is_empty() {
        writeln!(
            writer,
            "\n  No shared volume yet. List a stack's Compose volumes under `stacks.<stack>.shared-volumes` to share them between the nodes."
        )
        .map_err(output_error)?;
    } else {
        writeln!(writer).map_err(output_error)?;
    }
    for volume in &report.volumes {
        writeln!(writer, "{}", volume_line(volume)).map_err(output_error)?;
    }
    writeln!(writer).map_err(output_error)?;
    for node in &report.nodes {
        let says = match &node.status {
            None => format!(
                "no answer: {}",
                node.problem.as_deref().unwrap_or("unknown")
            ),
            Some(status) if status.serves && status.reachable => format!(
                "keeps the volumes and serves them on {}",
                status.address.map_or_else(
                    || "its mesh address".to_owned(),
                    |address| address.to_string()
                )
            ),
            Some(status) if status.reachable => match status.reach_ms.unwrap_or_default() {
                0 => format!("reaches {} in under 1 ms", report.storage_node),
                took => format!("reaches {} in {took} ms", report.storage_node),
            },
            Some(status) if status.serves => format!(
                "keeps the volumes, and its storage server does not answer: {}",
                status.problem.as_deref().unwrap_or("no reason given")
            ),
            Some(status) => format!(
                "cannot reach {}: {}",
                report.storage_node,
                status.problem.as_deref().unwrap_or("no reason given")
            ),
        };
        writeln!(
            writer,
            "  {:<24} {}",
            terminal_safe(&node.name),
            terminal_safe(&says)
        )
        .map_err(output_error)?;
    }
    if report.problems.is_empty() {
        return writeln!(writer, "\nEvery node reaches the storage node.").map_err(output_error);
    }
    writeln!(writer, "\nProblems:").map_err(output_error)?;
    write_diagnostics(&mut writer, &report.problems)
}

/// The shared volumes of a deployment plan: where each one's files are
/// kept and which nodes mount it.
pub(crate) fn storage_plan_lines(plan: &ClusterStoragePlan) -> Vec<String> {
    let mut lines = vec![format!(
        "  shared volumes: {} keeps the files and serves them to the other nodes over the mesh. For files that replicas share; a database belongs on an ordinary volume",
        plan.node
    )];
    lines.extend(plan.lines().into_iter().map(|line| format!("    {line}")));
    lines
}

/// What `mm target storage delete` would remove, and how to go ahead.
pub(crate) fn write_volume_delete_plan(
    plan: &VolumeDeletePlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Delete shared volume {}/{}\n  on:      {}\n  removes: {} with everything in it ({})\n  This cannot be undone: mm keeps no copy and makes no backup.",
        terminal_safe(&plan.stack),
        terminal_safe(&plan.volume),
        terminal_safe(&plan.node),
        terminal_safe(&plan.path),
        contents(Some(plan.bytes), Some(plan.files), plan.counted),
    )
    .map_err(output_error)?;
    if plan.declared {
        writeln!(
            writer,
            "  stacks.{}.shared-volumes still lists '{}': the next deployment that uses it creates it again, empty.",
            terminal_safe(&plan.stack),
            terminal_safe(&plan.volume),
        )
        .map_err(output_error)?;
    }
    writeln!(
        writer,
        "\nTo delete it: mm target storage delete {} --stack {} --plan {}",
        terminal_safe(&plan.volume),
        terminal_safe(&plan.stack),
        plan.plan_hash,
    )
    .map_err(output_error)
}

pub(crate) fn write_volume_deleted(
    deleted: &VolumeDeleted,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(deleted);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Deleted shared volume {}/{}: {} on {} is gone.",
        terminal_safe(&deleted.stack),
        terminal_safe(&deleted.volume),
        terminal_safe(&deleted.path),
        terminal_safe(&deleted.node),
    )
    .map_err(output_error)
}
