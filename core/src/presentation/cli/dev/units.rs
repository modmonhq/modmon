use super::*;
use crate::presentation::cli::deploy::{DeployCommandRequest, run_deploy_command};
use crate::presentation::cli::dispatch::Invocation;

/// `mm dev up`, with the `--with` units on for this run only.
pub(super) async fn up(
    command: Option<DeployCommand>,
    yes: bool,
    with: Vec<String>,
    invocation: &Invocation,
) -> Result<u8, MmError> {
    if with.is_empty() {
        return deploy::run(command, yes, (Vec::new(), None), invocation).await;
    }
    if composition::remote_environment(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
    )?
    .is_some()
    {
        return Err(MmError::Environment {
            message: "--with turns optional units on for the development environment on this machine; this environment runs on remote hosts".to_owned(),
        });
    }
    composition::development_units().check_with(&invocation.selection.start, &with)?;
    let mut selection = invocation.selection.clone();
    selection.development_with = with;
    run_deploy_command(DeployCommandRequest {
        command,
        yes,
        nodes: Vec::new(),
        release: None,
        selection: &selection,
        explicit_home: invocation.home(),
        format: invocation.format,
        runtime: &invocation.services,
        runtime_kind: invocation.runtime_kind,
    })
    .await
}

/// `mm dev enable` and `mm dev disable`. A disabled unit that still runs is
/// stopped by the same deployment `mm dev up` applies.
pub(super) async fn set(
    units: &[String],
    enable: bool,
    yes: bool,
    invocation: &Invocation,
) -> Result<u8, MmError> {
    let change =
        composition::development_units().set(&invocation.selection.start, units, enable)?;
    output::write_development_change(&change, invocation.format)?;
    if enable || change.stopped.is_empty() || invocation.format == OutputFormat::Json {
        return Ok(0);
    }
    let running = still_running(&change.stopped, invocation).await;
    if running.is_empty() {
        return Ok(0);
    }
    output::write_development_stopping(&running)?;
    deploy::run(None, yes, (Vec::new(), None), invocation).await
}

/// The services among `stopped` that still have containers.
async fn still_running(stopped: &[String], invocation: &Invocation) -> Vec<String> {
    let Ok(gateway) = composition::service_gateway_as(
        &invocation.selection,
        invocation.home(),
        invocation.runtime_kind,
        &invocation.services,
        composition::RemoteIdentity::User,
    ) else {
        return Vec::new();
    };
    let Ok(snapshot) = gateway
        .status(&crate::presentation::cli::service::approval("cli", false))
        .await
    else {
        return Vec::new();
    };
    snapshot
        .stacks
        .iter()
        .flat_map(|stack| &stack.services)
        .filter(|service| !service.containers.is_empty())
        .filter_map(|service| {
            let name = format!("{}/{}", service.module.as_deref()?, service.name);
            stopped.contains(&name).then_some(name)
        })
        .collect()
}
