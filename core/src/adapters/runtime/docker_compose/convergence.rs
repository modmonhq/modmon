fn is_missing_resource(output: &CommandOutput) -> bool {
    let stderr = output.stderr.to_ascii_lowercase();
    output.code == Some(1)
        && (stderr.contains("no such network")
            || stderr.contains("no such volume")
            || stderr.contains("not found"))
}

fn validate_volume_file(file_name: &str) -> Result<(), MmError> {
    if file_name.is_empty()
        || file_name.len() > 255
        || file_name == "."
        || file_name == ".."
        || file_name.contains(['/', '\\'])
        || file_name.chars().any(char::is_control)
    {
        return Err(MmError::Security {
            message: "volume file name must be one safe relative path component".to_owned(),
        });
    }
    Ok(())
}

fn require_local_named_target(target: &NamedTarget) -> Result<(), MmError> {
    match target.definition.transport {
        TargetTransport::Local => Ok(()),
        TargetTransport::Ssh { .. } => Err(MmError::Target {
            message: "Docker Compose adapter supports only local targets".to_owned(),
        }),
    }
}

fn require_local_target(target: &RuntimeTarget) -> Result<(), MmError> {
    match target
        .target
        .as_ref()
        .map(|target| &target.definition.transport)
    {
        None | Some(TargetTransport::Local) => Ok(()),
        Some(TargetTransport::Ssh { .. }) => Err(MmError::Target {
            message: "Docker Compose adapter supports only local targets".to_owned(),
        }),
    }
}
