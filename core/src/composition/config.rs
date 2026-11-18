use super::*;

pub(crate) fn resolve_runtime(
    start: &Path,
    explicit_home: Option<&Path>,
    explicit_runtime: Option<RuntimeKind>,
    target: Option<&str>,
    environment: Option<&str>,
    allow_missing_project: bool,
    require_resolved_selection: bool,
) -> Result<RuntimeKind, MmError> {
    if let Some(runtime) = explicit_runtime {
        return Ok(runtime);
    }
    let document = match ConfigLoader.load_from(start) {
        Ok(document) => document,
        Err(MmError::ConfigNotFound { .. }) if allow_missing_project => {
            return Ok(RuntimeKind::PodmanCompose);
        }
        Err(error) => return Err(error),
    };
    if !require_resolved_selection {
        return Ok(document
            .config
            .runtime
            .driver
            .unwrap_or(RuntimeKind::PodmanCompose));
    }
    let environment = resolve_environment(&document, environment)?;
    let home = MmHome::resolve(explicit_home)?;
    let state = SqliteStateStore::open(&home.state_database)?;
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(home.targets_file));
    let (environment_pref, project_pref) =
        crate::app::load_selection_preferences(Some(&state), &document, &environment)?;
    crate::app::resolve_operation_context(
        &document,
        &environment,
        None,
        target,
        environment_pref.as_ref(),
        project_pref.as_ref(),
        targets.as_ref(),
    )
    .map(|context| context.runtime)
}

pub(crate) fn schema() -> schemars::Schema {
    ConfigLoader.schema()
}

/// The schema for one top-level key's split file, if `key` can be split.
pub(crate) fn key_schema(key: &str) -> Option<schemars::Schema> {
    ConfigLoader.key_schema(key)
}

pub(crate) fn secret_capabilities()
-> Result<crate::domain::secret::SecretCapabilitySnapshot, MmError> {
    Ok(crate::domain::secret::SecretCapabilitySnapshot {
        storage: vec![age_storage_descriptor()?, environment_storage_descriptor()?],
        delivery: vec![
            crate::adapters::environment::direct_environment_descriptor()?,
            crate::adapters::environment::operation_environment_file_descriptor()?,
            crate::adapters::secrets::podman_native_secret_descriptor()?,
            crate::adapters::secrets::compose_file_secret_descriptor()?,
        ],
    })
}

pub(crate) fn discover_project(start: &Path) -> Result<PathBuf, MmError> {
    ConfigLoader.discover(start)
}
