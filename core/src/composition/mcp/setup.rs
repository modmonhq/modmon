//! `mm_setup_plan` and `mm_setup_apply`: the steps that set up this
//! machine, through the services `mm dev setup --services`, `mm secret
//! provider plan|apply`, `mm secret init`, and `mm secret generate` use.

use super::*;
use crate::domain::mcp::{McpSecretSetupPlan, McpSetupStep};

impl McpApplicationAdapter {
    pub(super) async fn plan_setup_step(
        &self,
        context: &McpRequestContext,
        step: &McpSetupStep,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::Plan, None)?;
        self.refuse_remote_setup(context)?;
        match step {
            McpSetupStep::DevServices { smtp_port } => {
                let services = self.service_stacks(context)?;
                json_value(services.plan(local_services_request(&services, *smtp_port)?)?)
            }
            McpSetupStep::SecretProvider { name } => {
                let (backends, request) = self.provider_install(context, name)?;
                json_value(backends.plan_managed(request)?)
            }
            McpSetupStep::SecretInit | McpSetupStep::SecretGenerate { .. } => {
                json_value(self.secret_setup_plan(context, step).await?)
            }
        }
    }

    pub(super) async fn apply_setup_step(
        &self,
        context: &McpRequestContext,
        step: &McpSetupStep,
        plan_hash: &str,
    ) -> Result<serde_json::Value, MmError> {
        context.require(step.action(), None)?;
        self.refuse_remote_setup(context)?;
        match step {
            McpSetupStep::DevServices { smtp_port } => {
                crate::composition::ensure_local_target(Some(&self.mm_home))?;
                let services = self.service_stacks(context)?;
                let request = local_services_request(&services, *smtp_port)?;
                json_value(services.execute_reviewed(request, plan_hash).await?)
            }
            McpSetupStep::SecretProvider { name } => {
                let (backends, request) = self.provider_install(context, name)?;
                json_value(backends.apply_managed(request, plan_hash).await?)
            }
            McpSetupStep::SecretInit => {
                self.require_secret_plan(context, step, plan_hash).await?;
                json_value(self.secret_services(context)?.initialize_age()?)
            }
            McpSetupStep::SecretGenerate { name } => {
                self.require_secret_plan(context, step, plan_hash).await?;
                let report = self
                    .secret_services(context)?
                    .secrets
                    .generate(
                        &context.project_root,
                        Some(&context.scope.environment),
                        &name.clone().map_or(
                            crate::domain::secret::SecretGenerationSelection::Missing,
                            crate::domain::secret::SecretGenerationSelection::Named,
                        ),
                    )
                    .await?;
                json_value(serde_json::json!({
                    "project_name": report.project_name,
                    "environment": report.environment,
                    "generated": report.secrets,
                }))
            }
        }
    }

    fn refuse_remote_setup(&self, context: &McpRequestContext) -> Result<(), MmError> {
        if !context.remote {
            return Ok(());
        }
        Err(MmError::Environment {
            message: format!(
                "environment '{}' runs on remote hosts, and these steps set up this machine. Stage its secrets with mm_stage_value; its hosts are set up with `mm target setup plan`",
                context.scope.environment
            ),
        }
        .in_own_words())
    }

    fn service_stacks(
        &self,
        context: &McpRequestContext,
    ) -> Result<crate::app::ServiceStackManager, MmError> {
        crate::composition::service_stack_manager(
            Some(&self.mm_home),
            context.scope.runtime,
            &self.runtime(),
        )
    }

    fn provider_install(
        &self,
        context: &McpRequestContext,
        name: &str,
    ) -> Result<
        (
            crate::app::SecretBackendManager,
            crate::domain::secret_backend::ManagedSecretBackendPlanRequest,
        ),
        MmError,
    > {
        let document = self.document(context)?;
        let name = crate::domain::secret_backend::ProviderInstanceName::new(name.to_owned())
            .map_err(|message| MmError::Security { message })?;
        let request = crate::composition::managed_provider_request(
            &document,
            &context.scope.environment,
            name,
            crate::domain::secret_backend::ManagedSecretBackendAction::Install,
            false,
        )?;
        let backends = crate::composition::interactive_secret_backend_manager(
            Some(&self.mm_home),
            &self.runtime(),
            context.scope.runtime,
            document.config.providers,
        )?;
        Ok((backends, request))
    }

    fn secret_services(
        &self,
        context: &McpRequestContext,
    ) -> Result<crate::composition::EnvironmentServices, MmError> {
        crate::composition::environment_services(
            &context.project_root,
            Some(&self.mm_home),
            self.runtime().clock(),
        )
    }

    async fn secret_setup_plan(
        &self,
        context: &McpRequestContext,
        step: &McpSetupStep,
    ) -> Result<McpSecretSetupPlan, MmError> {
        let services = self.secret_services(context)?;
        let current = match step {
            McpSetupStep::SecretGenerate { name } => {
                services
                    .manager
                    .secret_status(&self.selection(context), name.as_deref())
                    .await?
                    .secrets
            }
            _ => Vec::new(),
        };
        Ok(McpSecretSetupPlan::new(
            context.scope.environment.clone(),
            step.clone(),
            services.age_initialized(),
            current,
        ))
    }

    async fn require_secret_plan(
        &self,
        context: &McpRequestContext,
        step: &McpSetupStep,
        reviewed: &str,
    ) -> Result<(), MmError> {
        let current = self.secret_setup_plan(context, step).await?.plan_hash;
        if current == reviewed {
            return Ok(());
        }
        Err(MmError::State {
            message: format!(
                "setup plan changed (expected {reviewed}, current {current}); review the new plan"
            ),
        })
    }
}

/// The request `mm dev setup --services` plans: a repair of an interrupted
/// install first, then the install.
fn local_services_request(
    services: &crate::app::ServiceStackManager,
    smtp_port: Option<u16>,
) -> Result<crate::app::ServicePlanRequest, MmError> {
    let action = services.setup_action()?;
    Ok(crate::app::ServicePlanRequest {
        action,
        host_smtp_port: (action == crate::domain::service_stack::ServiceStackAction::Apply)
            .then_some(smtp_port)
            .flatten(),
        purge_data: false,
        include_gate: false,
    })
}
