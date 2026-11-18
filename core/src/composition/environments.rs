use super::*;

/// Where one configured environment runs, resolved the way any command resolves it.
pub(crate) struct EnvironmentPlacement {
    pub(crate) name: String,
    /// The project's default environment.
    pub(crate) default: bool,
    pub(crate) deploy_from: Vec<String>,
    pub(crate) resolved: Result<ResolvedPlacement, MmError>,
}

pub(crate) struct ResolvedPlacement {
    pub(crate) target: String,
    /// `user@host:port` when the target is reached over SSH.
    pub(crate) remote: Option<String>,
    pub(crate) runtime: RuntimeKind,
}

/// Every environment in the project configuration with its target and runtime.
/// An environment whose target cannot be resolved is listed with the reason.
pub(crate) fn environment_placements(
    start: &Path,
    explicit_home: Option<&Path>,
) -> Result<Vec<EnvironmentPlacement>, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::resolve(explicit_home)?;
    let state = SqliteStateStore::open(&home.state_database)?;
    let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(home.targets_file));
    Ok(document
        .config
        .environments
        .iter()
        .map(|(name, environment)| {
            let resolved = crate::app::load_selection_preferences(Some(&state), &document, name)
                .and_then(|(environment_pref, project_pref)| {
                    crate::app::resolve_operation_context(
                        &document,
                        name,
                        None,
                        None,
                        environment_pref.as_ref(),
                        project_pref.as_ref(),
                        targets.as_ref(),
                    )
                })
                .map(|context| ResolvedPlacement {
                    remote: match &context.target.definition.transport {
                        crate::domain::target::TargetTransport::Ssh {
                            host, port, user, ..
                        } => Some(format!("{user}@{host}:{port}")),
                        crate::domain::target::TargetTransport::Local => None,
                    },
                    target: context.target.name,
                    runtime: context.runtime,
                });
            EnvironmentPlacement {
                name: name.clone(),
                default: document.config.default_environment.as_deref() == Some(name.as_str()),
                deploy_from: environment.deploy_from.clone().unwrap_or_default(),
                resolved,
            }
        })
        .collect())
}
