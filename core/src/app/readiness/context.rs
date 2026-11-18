use super::*;

pub(super) struct PrerequisiteContext {
    documents: Arc<dyn ProjectDocumentStore>,
    state: Arc<dyn StateInspector>,
    targets: Arc<dyn TargetStore>,
}

impl PrerequisiteContext {
    pub fn new(
        documents: Arc<dyn ProjectDocumentStore>,
        state: Arc<dyn StateInspector>,
        targets: Arc<dyn TargetStore>,
    ) -> Self {
        Self {
            documents,
            state,
            targets,
        }
    }

    pub fn resolve(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<(ConfigDocument, crate::app::ResolvedOperationContext), MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let state = self.state.inspect()?;
        let (environment_pref, project_pref) =
            load_selection_preferences(state.as_deref(), &document, &environment)?;
        let context = crate::app::resolve_operation_context(
            &document,
            &environment,
            selection.runtime,
            selection.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            self.targets.as_ref(),
        )?;
        Ok((document, context))
    }
}
