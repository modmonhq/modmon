use super::*;

/// The one path to status, logs, commands, and restarts for the selected
/// environment: the remote daemon for an SSH target, the local runtime otherwise.
pub(crate) fn service_gateway(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
) -> Result<crate::app::ServiceGateway, MmError> {
    service_gateway_as(
        selection,
        explicit_home,
        runtime,
        services,
        RemoteIdentity::User,
    )
}

/// The same gateway, reaching a remote environment as `identity`.
pub(crate) fn service_gateway_as(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
    identity: RemoteIdentity,
) -> Result<crate::app::ServiceGateway, MmError> {
    if let Some(environment) = remote_environment_as(selection, explicit_home, runtime, identity)? {
        return Ok(crate::app::ServiceGateway::Remote {
            environment: Box::new(environment),
        });
    }
    Ok(crate::app::ServiceGateway::Local {
        operations: Box::new(runtime_operations_service(
            &selection.start,
            explicit_home,
            runtime,
            services,
        )?),
        selection: Box::new(selection.clone()),
    })
}

/// The one path to running and reading the declared jobs of the selected
/// environment: the hosts of a remote target, this machine otherwise.
pub(crate) fn job_gateway_as(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
    services: &RuntimeComposition,
    identity: RemoteIdentity,
) -> Result<crate::app::JobGateway, MmError> {
    if let Some(environment) = remote_environment_as(selection, explicit_home, runtime, identity)? {
        return Ok(crate::app::JobGateway::Remote {
            environment: Box::new(environment),
            document: Box::new(ConfigLoader.load_from(&selection.start)?),
        });
    }
    Ok(crate::app::JobGateway::Local {
        scheduler: Box::new(job_scheduler_service_on(
            selection.document_scope(),
            &selection.start,
            explicit_home,
            runtime,
            services,
        )?),
        selection: Box::new(selection.clone()),
    })
}
