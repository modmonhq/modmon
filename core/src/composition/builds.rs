use super::*;
use crate::domain::build::LocalImageEngine;
use crate::domain::readiness::SetupIssue;
use crate::domain::target::TargetTransport;

impl RuntimeComposition {
    /// The build planner for `selection`: a build without `--publish` of an
    /// environment on this machine loads into the engine of its runtime.
    pub(crate) fn build_manager_for(
        &self,
        selection: &WorkspaceSelection,
        explicit_home: Option<&Path>,
        (runtime, publish): (RuntimeKind, bool),
    ) -> Result<BuildManager, MmError> {
        let engine = if publish {
            None
        } else {
            local_build_engine(selection, explicit_home, runtime)?
        };
        let source: Arc<dyn BuildSourceInspector> =
            Arc::new(GitBuildSourceInspector::new(self.process()));
        Ok(
            BuildManager::new(documents_on(selection.document_scope()), source)
                .with_local_engine(engine),
        )
    }
}

/// The engine a build of the selected environment loads into: its
/// runtime's, when the environment's target is this machine. Planning opens
/// no state that does not exist yet.
pub(crate) fn local_build_engine(
    selection: &WorkspaceSelection,
    explicit_home: Option<&Path>,
    runtime: RuntimeKind,
) -> Result<Option<LocalImageEngine>, MmError> {
    let document = ConfigLoader.load_from(&selection.start)?;
    let environment = resolve_environment(&document, selection.environment.as_deref())?;
    let this_machine = || LocalImageEngine::for_runtime(selection.runtime.unwrap_or(runtime));
    let Ok(home) = MmHome::resolve(explicit_home) else {
        let (target, _) = crate::app::resolve_target_name(
            &document,
            &environment,
            selection.target.as_deref(),
            None,
            None,
        )?;
        return Ok((target == "local").then(this_machine));
    };
    let state = if home.state_database.is_file() {
        Some(SqliteStateStore::open(&home.state_database)?)
    } else {
        None
    };
    let (environment_pref, project_pref) = crate::app::load_selection_preferences(
        state.as_ref().map(|state| state as &dyn StateStore),
        &document,
        &environment,
    )?;
    let targets = FilesystemTargetStore::new(home.targets_file.clone());
    match crate::app::resolve_request_context(
        &document,
        &environment,
        (selection.runtime, runtime),
        selection.target.as_deref(),
        (environment_pref.as_ref(), project_pref.as_ref()),
        &targets,
    ) {
        Ok(context) => Ok((context.transport == TargetTransport::Local)
            .then(|| LocalImageEngine::for_runtime(context.runtime))),
        Err(MmError::Setup {
            issue: SetupIssue::MissingTarget { name, .. },
        }) if name == "local" => Ok(Some(this_machine())),
        Err(error) => Err(error),
    }
}

impl RuntimeComposition {
    /// The deployment `mm dev up` applies: it runs the environment's
    /// current local build, which it builds first when that is stale.
    pub(crate) async fn development_deployment(
        &self,
        selection: &WorkspaceSelection,
        explicit_home: Option<&Path>,
        runtime: RuntimeKind,
    ) -> Result<DeploymentService, MmError> {
        let deployment = self.deployment_service(selection, explicit_home)?;
        let planner = self.build_manager_for(selection, explicit_home, (runtime, false))?;
        if planner.local_engine().is_none() || !planner.builds_modules(selection)? {
            return Ok(deployment);
        }
        let builds = self.build_service(planner, selection, explicit_home)?;
        Ok(match builds.local_build(selection).await? {
            Some(build) => deployment.with_local_build(build),
            None => deployment,
        })
    }
}
