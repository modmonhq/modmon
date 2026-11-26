//! `mm_stage_value` and the plans `mm_deploy_plan` makes for a remote
//! environment: the same service and the same plans as `mm secret set`,
//! `mm deploy plan`, `rollback` and `redeploy`, as the agent.

use super::*;

impl McpApplicationAdapter {
    pub(super) async fn stage_as_agent(
        &self,
        context: &McpRequestContext,
        request: crate::domain::mcp::McpStageValueRequest,
    ) -> Result<serde_json::Value, MmError> {
        use crate::domain::mcp::McpStageAction as A;
        context.require(AgentAction::SetValues, None)?;
        let selection = self.selection(context);
        let Some(environment) = self.remote(&selection, context)? else {
            return Err(MmError::Environment {
                message: format!(
                    "environment '{}' runs on this machine; staging sets values on a remote environment's hosts. Here the user sets a secret with `mm secret set`",
                    context.scope.environment
                ),
            }
            .in_own_words());
        };
        let edit = match (request.action, request.value) {
            (A::Set, Some(value)) => crate::app::ValueEdit::Set(zeroize::Zeroizing::new(value)),
            (A::Set, None) => {
                return Err(MmError::Environment {
                    message: "set needs a value".to_owned(),
                });
            }
            (_, Some(_)) => {
                return Err(MmError::Environment {
                    message: "only set takes a value".to_owned(),
                });
            }
            (A::Unset, None) => crate::app::ValueEdit::Unset,
            (A::Discard, None) => crate::app::ValueEdit::Discard,
        };
        json_value(
            environment
                .stage_values(
                    vec![crate::app::KeyEdit {
                        kind: request.kind,
                        key: request.key,
                        edit,
                    }],
                    &crate::composition::AgeValueSealer,
                )
                .await?,
        )
    }

    pub(super) async fn plan_remote(
        &self,
        environment: &crate::app::RemoteEnvironment,
        (kind, release): (
            crate::domain::mcp::McpDeployPlanKind,
            &crate::domain::release_facts::ReleaseRequest,
        ),
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        use crate::domain::mcp::McpDeployPlanKind as K;
        let (plan, target) = match kind {
            K::Rollback => {
                let (target, plan) = environment.plan_rollback(None).await?;
                (plan, Some(("rollback", target)))
            }
            K::Redeploy => {
                let (target, plan) = environment
                    .plan_redeploy_reporting(None, &crate::app::ports::NoProgress)
                    .await?;
                (plan, Some(("redeploy", target)))
            }
            K::Checkout => {
                let commit = crate::composition::resolve_commit(&selection.start, None)?;
                let planning = environment.plan_deployment_reporting(
                    (&commit, None, release),
                    None,
                    &crate::app::ports::NoProgress,
                );
                (planning.await?, None)
            }
        };
        let mut value = if environment.is_cluster() {
            json_value(plan)?
        } else {
            json_value(plan.nodes.into_iter().next().map(|node| node.plan))?
        };
        if let (Some((name, target)), Some(object)) = (target, value.as_object_mut()) {
            let mut described = serde_json::json!({
                "commit": target.commit,
                "nodes": target.nodes,
            });
            if name == "rollback" {
                described["undoes"] = serde_json::json!(target.undoes);
            }
            object.insert(name.to_owned(), described);
        }
        Ok(value)
    }

    /// `mm_read` kind `secrets`: what the hosts hold, or this machine's store.
    /// The same report as `mm secret provider status` of a cluster
    /// environment's managed store, as the agent.
    pub(super) async fn read_secret_store(
        &self,
        context: &McpRequestContext,
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        let Some(environment) = self.remote(selection, context)? else {
            return Err(MmError::Environment {
                message: format!(
                    "environment '{}' runs on this machine; the managed store of a cluster environment runs on its nodes. Here the user checks a local provider with `mm secret provider status NAME`",
                    context.scope.environment
                ),
            }
            .in_own_words());
        };
        let document = self.document(context)?;
        json_value(
            crate::composition::cluster_store_report(
                &document,
                &context.scope.environment,
                &environment,
            )
            .await?,
        )
    }
}
