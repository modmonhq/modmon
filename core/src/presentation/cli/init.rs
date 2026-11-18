use super::*;

pub(super) async fn run(
    name: Option<String>,
    reviewed_plan: Option<String>,
    yes: bool,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let onboarding = composition::onboarding_service();
    let preview = onboarding.plan_initialization(&invocation.start, name.as_deref())?;
    let Some(reviewed_plan) = reviewed_plan else {
        output::write_initialization_plan(&preview, invocation.format)?;
        return Ok(0);
    };
    if !yes {
        return Err(MmError::Terminal {
            message: "project initialization requires --yes with the reviewed --plan hash"
                .to_owned(),
        });
    }
    onboarding.verify_initialization(&invocation.start, name.as_deref(), &reviewed_plan)?;
    if invocation.format == OutputFormat::Human {
        output::write_initialization_plan(&preview, invocation.format)?;
    }
    composition::ensure_local_target(invocation.home())?;
    let application = composition::persistent_application(
        invocation.home(),
        Arc::clone(&invocation.runtime),
        Arc::clone(&invocation.clock),
    )?;
    let initialized =
        application.initialize_reviewed(&invocation.start, name.as_deref(), &reviewed_plan)?;
    // The user just reviewed the new configuration, its agents: block included.
    composition::trust_agents(&invocation.start, invocation.home(), None)?;
    output::write_initialized(&initialized, invocation.format)?;
    Ok(0)
}
