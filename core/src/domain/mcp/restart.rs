use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{McpScope, McpSelectedScope};
use crate::domain::error::MmError;
use crate::domain::runtime::{RuntimeKind, ServiceSnapshot, ServiceState};
use crate::domain::workspace::WorkspaceSnapshot;

/// `mm_service_restart_plan`: review a restart of one service, as
/// `mm service restart SERVICE` makes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpServiceRestartPlanRequest {
    pub scope: McpScope,
    pub service: String,
}

/// `mm_service_restart_apply`: restart the service just reviewed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpServiceRestartApplyRequest {
    pub scope: McpScope,
    /// The service the plan was made for.
    pub service: String,
    pub plan_hash: String,
    pub approval_token: String,
    pub confirmation: String,
}

/// What a restart will touch: the service, where it runs, and the
/// containers (and on a cluster target, the nodes) it restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpServiceRestartPlan {
    pub plan_hash: String,
    pub environment: String,
    pub target: String,
    pub runtime: RuntimeKind,
    /// The environment runs on remote hosts, which restart the service
    /// themselves.
    pub remote: bool,
    pub stack: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub service: String,
    pub state: ServiceState,
    /// The containers the restart stops and starts again.
    pub containers: Vec<String>,
    /// The nodes of a cluster target the service runs on, restarted one
    /// at a time; empty on one host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
}

impl McpServiceRestartPlan {
    /// The plan for `service` in `snapshot`. A service the environment does
    /// not have, or one that runs no container, is refused with what the
    /// agent may name instead.
    pub fn of(
        scope: &McpSelectedScope,
        remote: bool,
        snapshot: &WorkspaceSnapshot,
        service: &str,
    ) -> Result<Self, MmError> {
        let matches = snapshot
            .stacks
            .iter()
            .flat_map(|stack| &stack.services)
            .filter(|candidate| candidate.name == service)
            .collect::<Vec<_>>();
        let found = match matches.as_slice() {
            [found] => *found,
            [] => return Err(unknown_service(scope, snapshot, service)),
            _ => {
                return Err(refusal(format!(
                    "service '{service}' is in more than one stack ({}) of environment '{}', and a restart names one service; the user restarts it with `mm service restart`",
                    matches
                        .iter()
                        .map(|candidate| candidate.stack.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    scope.environment
                )));
            }
        };
        if found.containers.iter().all(|container| container.history) {
            return Err(refusal(format!(
                "service '{service}' runs no container in environment '{}', so there is nothing to restart; mm_deploy_plan reviews a deployment that starts it",
                scope.environment
            )));
        }
        Self::new(scope, remote, &snapshot.config_hash, found)
    }

    fn new(
        scope: &McpSelectedScope,
        remote: bool,
        config_hash: &str,
        service: &ServiceSnapshot,
    ) -> Result<Self, MmError> {
        let mut containers = service
            .containers
            .iter()
            .filter(|container| !container.history)
            .map(|container| (container.name.clone(), container.id.clone()))
            .collect::<Vec<_>>();
        containers.sort();
        let nodes = service
            .nodes
            .iter()
            .filter(|node| node.desired_count > 0 || node.running_count > 0)
            .map(|node| node.node.clone())
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&(
            "mm-mcp-service-restart-v1",
            config_hash,
            scope,
            remote,
            (&service.stack, &service.module, &service.name),
            &containers,
            &nodes,
        ))
        .map_err(|error| MmError::State {
            message: format!("canonicalizing an MCP restart plan: {error}"),
        })?;
        Ok(Self {
            plan_hash: blake3::hash(&bytes).to_hex().to_string(),
            environment: scope.environment.clone(),
            target: scope.target.clone(),
            runtime: scope.runtime,
            remote,
            stack: service.stack.clone(),
            module: service.module.clone(),
            service: service.name.clone(),
            state: service.state,
            containers: containers.into_iter().map(|(name, _)| name).collect(),
            nodes,
        })
    }
}

fn unknown_service(
    scope: &McpSelectedScope,
    snapshot: &WorkspaceSnapshot,
    service: &str,
) -> MmError {
    let mut names = snapshot
        .stacks
        .iter()
        .flat_map(|stack| &stack.services)
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    refusal(if names.is_empty() {
        format!(
            "service '{service}' is not in environment '{}', which has no services yet",
            scope.environment
        )
    } else {
        format!(
            "service '{service}' is not in environment '{}'; name one of: {}",
            scope.environment,
            names.join(", ")
        )
    })
}

fn refusal(message: String) -> MmError {
    MmError::Environment { message }.in_own_words()
}
