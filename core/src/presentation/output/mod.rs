use std::io::{self, Write};

use serde::Serialize;

use crate::domain::build::{ImageBuildPlan, ImageBuildResult};
use crate::domain::deployment::{
    DeploymentPlan, DeploymentResult, DeploymentRouteBehavior, DeploymentRouteSnapshot,
};
use crate::domain::diagnostic::{Diagnostic, Severity, ValidationReport};
use crate::domain::environment::{
    CompiledEnvironmentManifest, EnvironmentDiff, EnvironmentReview, EnvironmentStatusSnapshot,
    EnvironmentValueOverride, MaterializedEnvironment, ResolutionStatus, ValueSource,
};
use crate::domain::environment_delivery::ResolvedEnvironmentDelivery;
use crate::domain::error::MmError;
use crate::domain::runtime::{RuntimeProbe, ServiceState};
use crate::domain::secret::InitializedAgeStore;
use crate::domain::secret::{
    SecretCapabilitySnapshot, SecretGenerationReport, SecretGenerationStatus,
    SecretRotationApplyReport, SecretRotationPlan, SecretRotationPlanStatus,
    SecretRotationResultItem, SecretStatus, SecretStatusSnapshot,
};
use crate::domain::secret_contract::StorageOperation;
use crate::domain::stack::StackCompilationReport;
use crate::domain::target::{
    NamedTarget, TargetBootstrapPlan, TargetBootstrapResult, TargetCatalogSnapshot, TargetIngress,
    TargetTransport,
};
use crate::domain::target_ingress::TargetIngressPlan;
use crate::domain::workspace::{
    DoctorReport, InitializedProject, ValidationSnapshot, WorkspaceSnapshot,
};
use crate::presentation::cli::OutputFormat;

mod address_table;
mod build_receipt;
pub(crate) use build_receipt::*;
mod ca;
pub(crate) use ca::*;
mod cluster;
pub(crate) use cluster::*;
mod cluster_setup;
mod deploy_cards;
pub use cluster::{local_plan_lines, remote_plan_lines};
pub(crate) use cluster_setup::write_store_restore_plan;
pub use cluster_setup::{setup_plan_card, setup_result_card};
pub use deploy_cards::{
    apply_report_card, deployment_result_card, local_plan_card, remote_plan_card,
};
mod access;
pub(crate) use access::*;
mod config;
mod config_files;
pub(crate) use config::*;
mod deployment;
pub(crate) use deployment::*;
mod development;
pub(crate) use development::*;
mod environment;
pub(crate) use environment::*;
mod image_build;
pub(crate) use image_build::*;
mod job;
pub(crate) use job::*;
mod logs;
pub(crate) use logs::LogLines;
mod mesh;
pub(crate) mod names;
mod networks;
pub(crate) use mesh::*;
mod placement;
mod provider;
pub(crate) use provider::*;
mod release;
mod rollout;
pub(crate) use rollout::{
    redeploy_line, rollback_line, write_cancel_report, write_deployment_status,
};
pub(crate) mod restarts;
mod scaling;
mod secret;
pub(crate) use secret::*;
mod secret_store;
pub(crate) use secret_store::write_cluster_store_report;
mod values;
pub use values::{remote_values_card, stage_summary};
pub(crate) use values::{write_remote_values, write_stage_report, write_store_migrated};
mod status;
pub use status::service_table;
pub(crate) use status::*;
mod storage;
pub(crate) use storage::{write_storage_report, write_volume_delete_plan, write_volume_deleted};
mod services;
pub(crate) use services::*;
mod target;
pub(crate) use target::*;
mod update;
pub(crate) use update::update_summary_lines;
mod workspace;
pub(crate) use workspace::*;

pub(crate) fn write_json(value: &impl Serialize) -> Result<(), MmError> {
    if let Ok(recorded) = serde_json::to_value(value) {
        crate::presentation::record::result(&recorded);
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| MmError::Io {
        context: "writing JSON output".to_owned(),
        source: io::Error::other(error),
    })?;
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "{}", crate::domain::error::shown(&text)).map_err(output_error)
}

/// An error in one line, as the command line prints it first: its code,
/// then what happened and what to run next. The terminal UI shows errors
/// in these words too.
#[must_use]
pub fn error_line(error: &MmError) -> String {
    format!(
        "error[{}]: {}",
        error.code(),
        terminal_safe(&error.to_string())
    )
}

pub fn write_error(error: &MmError, format: OutputFormat) -> Result<(), io::Error> {
    write_error_with_context(error, format, &RecoveryContext::default())
}

pub fn write_error_with_context(
    error: &MmError,
    format: OutputFormat,
    context: &RecoveryContext,
) -> Result<(), io::Error> {
    let stderr = io::stderr();
    let mut writer = stderr.lock();
    if format == OutputFormat::Json {
        serde_json::to_writer(
            &mut writer,
            &serde_json::json!({
                "error": { "code": error.code(), "message": crate::domain::error::shown(&error.to_string()),
                    "recovery": match error {
                        MmError::Setup { issue } => Some(serde_json::json!({ "issue": issue, "help": issue.help(), "command": context.command(issue) })),
                        _ => None,
                    }
                }
            }),
        )
        .map_err(io::Error::other)?;
        writeln!(writer)
    } else {
        writeln!(writer, "{}", error_line(error))?;
        if let MmError::Setup { issue } = error {
            writeln!(writer, "\n  {}", terminal_safe(issue.help()))?;
            writeln!(
                writer,
                "\n  Next: {}",
                terminal_safe(&context.display_command(issue))
            )?;
        }
        Ok(())
    }
}

pub(crate) fn write_runtime(
    writer: &mut impl Write,
    runtime: &RuntimeProbe,
) -> Result<(), MmError> {
    writeln!(
        writer,
        "  runtime:     {:?} ({})",
        runtime.kind,
        if !runtime.inspected_here() {
            "on the target's hosts; this machine's engine is not used"
        } else if runtime.available {
            "available"
        } else {
            "unavailable"
        }
    )
    .map_err(output_error)?;
    if let Some(version) = &runtime.engine_version {
        writeln!(writer, "  engine:      {}", terminal_safe(version)).map_err(output_error)?;
    }
    if let Some(provider) = &runtime.compose_provider {
        writeln!(
            writer,
            "  compose:     {}{}",
            terminal_safe(&provider.name),
            provider
                .version
                .as_deref()
                .map_or_else(String::new, |version| format!(
                    " ({})",
                    terminal_safe(version)
                ))
        )
        .map_err(output_error)?;
    }
    if let Some(rootless) = runtime.rootless {
        writeln!(writer, "  rootless:    {rootless}").map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_validation_report(
    writer: &mut impl Write,
    report: &ValidationReport,
) -> Result<(), MmError> {
    if report.diagnostics.is_empty() {
        writeln!(writer, "  config:      valid").map_err(output_error)
    } else {
        write_diagnostics(writer, &report.diagnostics)
    }
}

pub(crate) fn write_diagnostics(
    writer: &mut impl Write,
    diagnostics: &[Diagnostic],
) -> Result<(), MmError> {
    for diagnostic in diagnostics {
        writeln!(
            writer,
            "  {}[{}]: {}",
            severity(diagnostic.severity),
            terminal_safe(&diagnostic.code),
            terminal_safe(&diagnostic.message)
        )
        .map_err(output_error)?;
        if let Some(path) = &diagnostic.path {
            let location = match (diagnostic.line, diagnostic.column) {
                (Some(line), Some(column)) => format!(":{line}:{column}"),
                _ => String::new(),
            };
            writeln!(
                writer,
                "    at: {}{location}",
                terminal_safe(&path.display().to_string())
            )
            .map_err(output_error)?;
        }
        if let Some(help) = &diagnostic.help {
            writeln!(writer, "    help: {}", terminal_safe(help)).map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) const fn value_source(source: ValueSource) -> &'static str {
    match source {
        ValueSource::ServiceEnv => "service env",
        ValueSource::SqliteOverride => "local override",
        ValueSource::ConfigValue => "config",
        ValueSource::Release => "release",
        ValueSource::Secret => "secret",
        ValueSource::TemplateDefault => "template default",
        ValueSource::TemplateLiteral => "template literal",
        ValueSource::Missing => "missing",
    }
}

pub(crate) const fn resolution_status(status: ResolutionStatus) -> &'static str {
    match status {
        ResolutionStatus::Resolved => "resolved",
        ResolutionStatus::Defaulted => "defaulted",
        ResolutionStatus::Unset => "unset",
        ResolutionStatus::NotChecked => "not checked",
        ResolutionStatus::ProviderUnavailable => "unavailable",
        ResolutionStatus::AccessDenied => "denied",
        ResolutionStatus::Invalid => "invalid",
    }
}

pub(crate) const fn secret_status(status: SecretStatus) -> &'static str {
    match status {
        SecretStatus::Set => "set",
        SecretStatus::Unset => "unset",
        SecretStatus::NotChecked => "not checked",
        SecretStatus::ProviderUnavailable => "unavailable",
        SecretStatus::AccessDenied => "denied",
        SecretStatus::Invalid => "invalid",
    }
}

pub(crate) const fn rotation_plan_status(status: SecretRotationPlanStatus) -> &'static str {
    match status {
        SecretRotationPlanStatus::NotScheduled => "not scheduled",
        SecretRotationPlanStatus::NotDue => "not due",
        SecretRotationPlanStatus::Ready => "ready",
        SecretRotationPlanStatus::InputRequired => "input required",
        SecretRotationPlanStatus::UnsupportedStrategy => "requires deployment orchestration",
        SecretRotationPlanStatus::Unset => "unset",
        SecretRotationPlanStatus::Invalid => "invalid",
        SecretRotationPlanStatus::ProviderUnavailable => "provider unavailable",
        SecretRotationPlanStatus::CandidateStaged => "candidate staged",
    }
}

pub(crate) const fn severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

pub(crate) const fn service_state(state: ServiceState) -> &'static str {
    match state {
        ServiceState::Missing => "missing",
        ServiceState::Running => "running",
        ServiceState::Degraded => "degraded",
        ServiceState::Stopped => "stopped",
        ServiceState::Unknown => "unknown",
        ServiceState::JobOnly => "job only",
    }
}

pub(crate) fn output_error(source: io::Error) -> MmError {
    MmError::Io {
        context: "writing command output".to_owned(),
        source,
    }
}

/// One line describing a finished restart, for every interface that reports one.
pub(crate) fn restart_summary(outcome: &crate::domain::observability::RestartOutcome) -> String {
    let service = &outcome.identity.service;
    let containers = match outcome.containers.len() {
        1 => "1 container".to_owned(),
        count => format!("{count} containers"),
    };
    match outcome.nodes.as_slice() {
        [] => format!("Restarted {service} ({containers}); it is healthy."),
        [node] => format!("Restarted {service} on {node} ({containers}); it is healthy."),
        nodes => format!(
            "Restarted {service} on {}, one node at a time, each healthy before the next ({containers}).",
            nodes.join(", then ")
        ),
    }
}

pub(crate) fn terminal_safe(value: &str) -> String {
    crate::domain::error::shown(value)
        .chars()
        .flat_map(|character| match character {
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            character if character.is_control() => format!("\\u{{{:x}}}", u32::from(character))
                .chars()
                .collect(),
            character => vec![character],
        })
        .collect()
}

pub(crate) fn write_terminal_safe_multiline(
    writer: &mut impl Write,
    value: &str,
) -> Result<(), MmError> {
    for line in value.lines() {
        writeln!(writer, "{}", terminal_safe(line)).map_err(output_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{terminal_safe, write_terminal_safe_multiline};

    #[test]
    fn escapes_terminal_control_sequences() {
        assert_eq!(terminal_safe("ok\u{1b}[31m\n"), "ok\\u{1b}[31m\\n");
    }

    #[test]
    fn multiline_reviews_preserve_lines_and_escape_controls()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut output = Vec::new();
        write_terminal_safe_multiline(&mut output, "--- a/file\n+safe\u{1b}[31m\n")?;
        assert_eq!(output, b"--- a/file\n+safe\\u{1b}[31m\n");
        Ok(())
    }
}
mod recovery;
pub use recovery::{RecoveryContext, display_command};
