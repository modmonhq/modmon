use super::*;

pub(in crate::presentation::cli) async fn run(
    command: Option<DevCommand>,
    invocation: &super::super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        Some(DevCommand::Mounts { command }) => mounts(command, invocation).await,
        Some(DevCommand::Setup {
            command:
                Some(SetupCommand::Project {
                    plan,
                    deploy_plan,
                    yes,
                }),
            ..
        }) => {
            run_project_setup(ProjectSetupRequest {
                invocation,
                reviewed_workflow: plan.as_deref(),
                reviewed_deployment: deploy_plan.as_deref(),
                yes,
            })
            .await
        }
        Some(DevCommand::Setup {
            command: None,
            no_trust,
            system,
            install_services,
            smtp_port,
            yes,
        }) => {
            setup(
                no_trust,
                system,
                install_services,
                smtp_port,
                yes,
                invocation,
            )
            .await
        }
        Some(DevCommand::Up { with, plan, yes }) => {
            let command = plan.map(|plan| DeployCommand::Apply {
                plan,
                receipt: None,
            });
            units::up(command, yes, with, invocation).await
        }
        Some(DevCommand::Enable { units }) => units::set(&units, true, false, invocation).await,
        Some(DevCommand::Disable { units, yes }) => {
            units::set(&units, false, yes, invocation).await
        }
        Some(DevCommand::Down { yes }) => run_dev_down(yes, invocation).await,
        Some(DevCommand::Status) => {
            run_service_status(
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
                invocation.format,
                &invocation.services,
                None,
            )
            .await
        }
        Some(DevCommand::Tls { system, command }) => tls(system, command, invocation).await,
        Some(DevCommand::Services { command }) => services(command, invocation).await,
        Some(DevCommand::Dashboard {
            command: DashboardCommand::Serve { listen },
        }) => dashboard_server(listen, invocation).await,
        Some(DevCommand::Open { .. }) => open(invocation),
        None => Err(MmError::State {
            message: "interactive development command was not routed to the TUI".to_owned(),
        }),
    }
}

async fn mounts(
    command: DevMountCommand,
    invocation: &super::super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let manager =
        composition::development_mount_manager(invocation.home(), Arc::clone(&invocation.runtime))?;
    let plan = match command {
        DevMountCommand::Plan(options) => {
            manager
                .plan(&invocation.selection, &options.request())
                .await?
        }
        DevMountCommand::Configure { options, plan } => {
            manager
                .configure_reviewed(&invocation.selection, &options.request(), &plan)
                .await?
        }
        DevMountCommand::Disable { module, plan } => {
            let request = crate::app::DevelopmentMountPlanRequest {
                module,
                ..Default::default()
            };
            manager
                .disable_reviewed(&invocation.selection, &request, &plan)
                .await?
        }
    };
    output::write_development_mount_plan(&plan, invocation.format)?;
    Ok(0)
}

async fn setup(
    no_trust: bool,
    system: bool,
    install_services: bool,
    smtp_port: Option<u16>,
    yes: bool,
    invocation: &super::super::dispatch::Invocation,
) -> Result<u8, MmError> {
    composition::ensure_local_target(invocation.home())?;
    output::write_tls_privilege_notice(system, no_trust)?;
    let ca =
        composition::local_ca_manager(invocation.home(), system, invocation.services.process())?;
    output::write_local_ca_status(&ca.setup(!no_trust).await?, invocation.format)?;
    if install_services {
        let manager = composition::service_stack_manager(
            invocation.home(),
            invocation.runtime_kind,
            &invocation.services,
        )?;
        return run_services_setup(&manager, smtp_port, yes, invocation.format).await;
    }
    Ok(0)
}

async fn tls(
    system: bool,
    command: TlsCommand,
    invocation: &super::super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let ca =
        composition::local_ca_manager(invocation.home(), system, invocation.services.process())?;
    let status = match command {
        TlsCommand::Status => ca.status().await?,
        TlsCommand::Repair => ca.repair().await?,
        TlsCommand::Rotate => ca.rotate().await?,
        TlsCommand::Uninstall => ca.uninstall().await?,
    };
    output::write_local_ca_status(&status, invocation.format)?;
    Ok(0)
}

async fn services(
    command: ServicesCommand,
    invocation: &super::super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let manager = composition::service_stack_manager(
        invocation.home(),
        invocation.runtime_kind,
        &invocation.services,
    )?;
    run_services_command(command, &manager, invocation.format).await
}

async fn dashboard_server(
    listen: std::net::SocketAddr,
    invocation: &super::super::dispatch::Invocation,
) -> Result<u8, MmError> {
    let (queries, events, token, gates) = composition::dashboard_services(
        &invocation.start,
        invocation.home(),
        invocation.runtime_kind,
        &invocation.services,
    )?;
    dashboard::serve(listen, token, queries, events, gates).await?;
    Ok(0)
}

fn open(invocation: &super::super::dispatch::Invocation) -> Result<u8, MmError> {
    let document = composition::project_documents().load_from(&invocation.start)?;
    output::write_dashboard_links(&document.config.project.name)?;
    Ok(0)
}
