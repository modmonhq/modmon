use super::*;

#[derive(Debug, Clone, Subcommand)]
pub enum DoctorCommand {
    /// Review recovery of pre-baseline state; apply requires the reviewed hash and --yes.
    RecoverState {
        /// Exact hash from the recovery review.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Confirm backup and replacement of the reviewed legacy state.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
}

pub(super) fn run(
    command: DoctorCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let DoctorCommand::RecoverState { plan, yes } = command;
    let recovery = composition::state_recovery(invocation.home())?;
    if let Some(hash) = plan.filter(|_| yes) {
        let backup = recovery.recover(&hash)?;
        output::write_json(&serde_json::json!({ "backup": backup, "recovered": true }))?;
    } else {
        let plan = recovery.plan()?;
        write_plan(&plan, invocation.format, &invocation.recovery_context)?;
    }
    Ok(0)
}

pub(super) fn recover_interactively(
    invocation: &super::dispatch::Invocation,
) -> Result<bool, MmError> {
    let recovery = composition::state_recovery(invocation.home())?;
    let plan = recovery.plan()?;
    write_plan(&plan, OutputFormat::Human, &invocation.recovery_context)?;
    if confirm("Back up and reset this pre-baseline state database?")? {
        let backup = recovery.recover(&plan.plan_hash)?;
        output::write_json(&serde_json::json!({ "backup": backup, "recovered": true }))?;
        return Ok(true);
    }
    Ok(false)
}

fn write_plan(
    plan: &crate::domain::readiness::StateRecoveryPlan,
    format: OutputFormat,
    context: &output::RecoveryContext,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(plan);
    }
    let mut command = context.command(&crate::domain::readiness::SetupIssue::LegacyState {
        path: plan.database.clone(),
    });
    command.extend([
        "--plan".to_owned(),
        plan.plan_hash.clone(),
        "--yes".to_owned(),
    ]);
    writeln!(crate::presentation::record::stdout(),
        "State recovery: {}\n\nStop other mm processes before applying. A private SQLite backup is saved beside the database.\nThis resets private preferences, build records, operation history, and resource ownership records.\nExisting containers, secrets files, target catalog, and project files are preserved.\nExisting runtime resources may need ownership reconciliation before further operations.\n\nApply the reviewed plan:\n  {}",
        output::terminal_safe(&plan.database.display().to_string()), output::terminal_safe(&output::display_command(&command)))
        .map_err(|source| MmError::Io { context: "writing recovery plan".to_owned(), source })
}
