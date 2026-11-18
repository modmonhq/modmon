use super::*;
use crate::app::ports::LocalDeploymentRuntime;

/// Uses the same mail-binding policy as stack compilation, then verifies the
/// exact target-owned network rather than trusting an installation journal.
pub struct DevelopmentServicesCheck {
    context: super::context::PrerequisiteContext,
    runtimes: Vec<(RuntimeKind, Arc<dyn LocalDeploymentRuntime>)>,
}

impl DevelopmentServicesCheck {
    pub fn new(
        documents: Arc<dyn ProjectDocumentStore>,
        state: Arc<dyn StateInspector>,
        targets: Arc<dyn TargetStore>,
        runtimes: Vec<(RuntimeKind, Arc<dyn LocalDeploymentRuntime>)>,
    ) -> Self {
        Self {
            context: super::context::PrerequisiteContext::new(documents, state, targets),
            runtimes,
        }
    }
}

#[async_trait::async_trait]
impl ReadinessProbe for DevelopmentServicesCheck {
    fn name(&self) -> &'static str {
        "development services"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["selection"]
    }

    async fn inspect(&self, selection: &WorkspaceSelection) -> Result<Vec<Diagnostic>, MmError> {
        let (document, context) = self.context.resolve(selection)?;
        if !super::super::stacks::needs_development_services(
            &document,
            &context.environment,
            &context.target,
        ) {
            return Ok(Vec::new());
        }
        let runtime = self
            .runtimes
            .iter()
            .find(|(kind, _)| *kind == context.runtime)
            .ok_or_else(|| MmError::State {
                message: "no network inspector is registered for the selected runtime".to_owned(),
            })?;
        let network =
            crate::domain::service_stack::development_network_name(context.target.definition.id);
        if !runtime
            .1
            .inspect_network(
                &context.target,
                &network,
                crate::domain::service_stack::DEVELOPMENT_NETWORK_RESOURCE,
            )
            .await?
        {
            return Err(MmError::Setup {
                issue: SetupIssue::MissingDevelopmentServices { network },
            });
        }
        Ok(Vec::new())
    }
}
