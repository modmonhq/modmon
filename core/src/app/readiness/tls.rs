use super::*;

pub struct LocalTlsCheck {
    context: super::context::PrerequisiteContext,
    ca: crate::app::LocalCaManager,
}

impl LocalTlsCheck {
    pub fn new(
        documents: Arc<dyn ProjectDocumentStore>,
        state: Arc<dyn StateInspector>,
        targets: Arc<dyn TargetStore>,
        ca: crate::app::LocalCaManager,
    ) -> Self {
        Self {
            context: super::context::PrerequisiteContext::new(documents, state, targets),
            ca,
        }
    }
}

#[async_trait::async_trait]
impl ReadinessProbe for LocalTlsCheck {
    fn name(&self) -> &'static str {
        "local TLS"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["selection"]
    }

    async fn inspect(&self, selection: &WorkspaceSelection) -> Result<Vec<Diagnostic>, MmError> {
        let (document, context) = self.context.resolve(selection)?;
        if context.target.definition.transport != TargetTransport::Local {
            return Ok(Vec::new());
        }
        let development = document.config.project.development_environment.as_deref()
            == Some(&context.environment);
        let automatic_https = development
            && document.config.ingress.automatic_local != Some(false)
            && document.config.environments[&context.environment].automatic_local_ingress
                != Some(false)
            && document.config.modules.values().any(|module| {
                module.services.values().any(|service| {
                    service.is_active(&context.environment)
                        && service.automatic_local_ingress != Some(false)
                        && service.local_https != Some(false)
                        && !service
                            .role
                            .is_some_and(|role| role.excludes_automatic_ingress())
                })
            });
        if !automatic_https
            && !super::super::stacks::needs_development_services(
                &document,
                &context.environment,
                &context.target,
            )
        {
            return Ok(Vec::new());
        }
        let status = self.ca.status().await?;
        if status.trust_disabled {
            return self.ca.diagnostics().await;
        }
        if !status.installed
            || !status
                .trust
                .as_ref()
                .is_some_and(|trust| trust.installed && !trust.mismatch)
        {
            return Err(MmError::Setup {
                issue: SetupIssue::LocalTlsRequired,
            });
        }
        self.ca.diagnostics().await
    }
}
