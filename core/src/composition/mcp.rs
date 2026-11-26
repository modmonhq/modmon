use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{RuntimeComposition, gate_services, job_scheduler_service, target_services};
use crate::adapters::config::ConfigLoader;
use crate::adapters::mcp::{AgentTrustStore, McpPolicyStore};
use crate::adapters::state::{MmHome, SqliteStateStore};
use crate::adapters::target::FilesystemTargetStore;
use crate::app::WorkspaceSelection;
use crate::app::ports::{McpAuditJournal, McpUseCases, TargetStore};
use crate::domain::agents::{AgentAccess, AgentAction, agent_access_in};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::mcp::{
    MCP_MAX_EXEC_ARGUMENTS, MCP_MAX_EXEC_BYTES, MCP_MAX_EXEC_SECONDS, MCP_MAX_LOG_BYTES,
    MCP_MAX_LOG_RECORDS, MCP_RESOURCE_SCHEMA_VERSION, MCP_SOURCE, MCP_TOOL_SCHEMA_VERSION,
    McpAccessOverview, McpAuthorizationError, McpAuthorizationRequest, McpDenial, McpDenialReason,
    McpEnvironmentAccess, McpExecRequest, McpExecResult, McpJobAction, McpLogRequest, McpReadKind,
    McpRequestContext, McpSelectedScope, McpServerConfig,
};
use crate::domain::observability::{
    ApprovalContext, ExecEvent, ExecOutputSource, ExecRequest, ExecStdinMode, ExecTtyMode,
    LogLimits, LogMode, LogOutputPolicy, LogRequest, ServiceSelector,
};
use crate::domain::onboarding::{ConfigMutationRequest, ConfigResourceKind};
use crate::domain::runtime::RuntimeKind;

pub(crate) struct McpComposition {
    pub(crate) use_cases: Arc<dyn McpUseCases>,
    pub(crate) audit: Arc<dyn McpAuditJournal>,
}

/// The server for the project at `requested_root`, or at the directory the
/// coding tool launched it in. `config_path` names private limits that must
/// exist; without it `MM_HOME/mcp.json` applies when present.
pub(crate) fn server(
    config_path: Option<&Path>,
    explicit_home: Option<&Path>,
    requested_root: Option<&Path>,
) -> Result<crate::presentation::mcp::McpServer, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let config = match config_path {
        Some(path) => McpPolicyStore::load(path, true)?,
        None => McpPolicyStore::load(&McpPolicyStore::default_path(&home.root), false)?,
    };
    let start = match requested_root {
        Some(root) => root.to_path_buf(),
        None => std::env::current_dir().map_err(|source| MmError::Io {
            context: "resolving the current directory".to_owned(),
            source,
        })?,
    };
    let start = dunce::canonicalize(&start).map_err(|source| MmError::Io {
        context: format!("resolving MCP project directory {}", start.display()),
        source,
    })?;
    let composed = services(config.clone(), &start, Some(&home.root))?;
    Ok(crate::presentation::mcp::McpServer::new(
        config,
        composed.use_cases,
        composed.audit,
    ))
}

pub(crate) fn services(
    config: McpServerConfig,
    start: &Path,
    explicit_home: Option<&Path>,
) -> Result<McpComposition, MmError> {
    let home = MmHome::resolve(explicit_home)?;
    let journal: Arc<dyn McpAuditJournal> = Arc::new(SqliteStateStore::open(&home.state_database)?);
    let audit: Arc<dyn McpAuditJournal> = Arc::new(super::mcp_commands::RecordingAudit::new(
        journal,
        home.root.clone(),
    ));
    Ok(McpComposition {
        use_cases: Arc::new(McpApplicationAdapter::new(
            config,
            home.root,
            start.to_path_buf(),
        )),
        audit,
    })
}

pub struct McpApplicationAdapter {
    config: McpServerConfig,
    mm_home: PathBuf,
    /// Where the project is looked up from: the launch directory.
    start: PathBuf,
}

/// What the project grants an agent in one environment, and whether the
/// user trusts the block that says so.
struct Grant {
    /// The grant in force: the declared one when trusted, else `view`.
    access: AgentAccess,
    /// What the block would grant if the user trusted it.
    declared: AgentAccess,
    trusted: bool,
}

impl McpApplicationAdapter {
    #[must_use]
    pub fn new(config: McpServerConfig, mm_home: PathBuf, start: PathBuf) -> Self {
        Self {
            config,
            mm_home,
            start,
        }
    }

    fn selection(&self, context: &McpRequestContext) -> WorkspaceSelection {
        let mut selection = WorkspaceSelection::new(
            &context.project_root,
            Some(context.scope.environment.clone()),
        );
        selection.runtime = Some(context.scope.runtime);
        selection.target = Some(context.scope.target.clone());
        selection
    }

    /// The remote hosts of the context's environment, reached with the
    /// agent key and nothing else; `None` for a local target.
    fn remote(
        &self,
        selection: &WorkspaceSelection,
        context: &McpRequestContext,
    ) -> Result<Option<crate::app::RemoteEnvironment>, MmError> {
        super::remote_environment_as(
            selection,
            Some(&self.mm_home),
            context.scope.runtime,
            super::RemoteIdentity::Agent,
        )
    }

    /// Status, logs, and commands for the context's environment: the same
    /// gateway the command line and the terminal UI use.
    fn gateway(
        &self,
        selection: &WorkspaceSelection,
        context: &McpRequestContext,
        runtime: &RuntimeComposition,
    ) -> Result<crate::app::ServiceGateway, MmError> {
        super::service_gateway_as(
            selection,
            Some(&self.mm_home),
            context.scope.runtime,
            runtime,
            super::RemoteIdentity::Agent,
        )
    }

    fn approval(&self) -> ApprovalContext {
        ApprovalContext {
            principal: self.config.principal.clone(),
            source: MCP_SOURCE.to_owned(),
            approved_high_risk: false,
        }
    }

    fn document(&self, context: &McpRequestContext) -> Result<ConfigDocument, MmError> {
        let document = ConfigLoader.load_from(&context.project_root)?;
        if document.root != context.project_root {
            return Err(MmError::Security {
                message: "MCP project ownership changed during request resolution".to_owned(),
            });
        }
        Ok(document)
    }

    fn runtime(&self) -> RuntimeComposition {
        RuntimeComposition::new()
    }

    fn job_plan_hash(
        &self,
        context: &McpRequestContext,
        action: &McpJobAction,
    ) -> Result<String, MmError> {
        let document = self.document(context)?;
        let canonical = serde_json::to_vec(&(
            "mm-mcp-job-action-v1",
            document.config.project.id,
            document.source_hash,
            &context.scope,
            action,
        ))
        .map_err(|error| MmError::State {
            message: format!("canonicalizing MCP job action: {error}"),
        })?;
        Ok(blake3::hash(&canonical).to_hex().to_string())
    }

    fn read_reconciled_state(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        document: &ConfigDocument,
        runtime: &RuntimeComposition,
    ) -> Result<Option<serde_json::Value>, MmError> {
        let value = match kind {
            McpReadKind::Gates => {
                let policies =
                    gate_services(&context.project_root, Some(&self.mm_home), runtime.clock())?
                        .manager
                        .show(document.config.project.id, &context.scope.environment, None)?;
                let summaries = policies.into_iter().map(|policy| serde_json::json!({
                    "id": policy.id, "name": policy.name, "environment": policy.environment,
                    "mode": policy.mode, "drivers": policy.drivers,
                    "session_ttl_seconds": policy.session_ttl_seconds, "revision": policy.revision,
                })).collect::<Vec<_>>();
                serde_json::json!({ "configured": &document.config.gates, "policies": summaries })
            }
            _ => return Ok(None),
        };
        Ok(Some(value))
    }
}

#[async_trait::async_trait]
impl McpUseCases for McpApplicationAdapter {
    async fn commands(
        &self,
        context: &McpRequestContext,
        request: &crate::domain::mcp::McpReadRequest,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::Read, None)?;
        super::mcp_commands::read(
            &context.project_root,
            &self.mm_home,
            (
                request.command.as_deref(),
                request.filter.as_ref(),
                request.cursor.as_deref(),
                request.limit,
            ),
        )
    }

    async fn authorize(
        &self,
        request: McpAuthorizationRequest,
    ) -> Result<McpRequestContext, McpAuthorizationError> {
        self.authorize_request(request)
    }

    async fn access(&self) -> Result<McpAccessOverview, MmError> {
        self.access_overview()
    }

    async fn read(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::Read, None)?;
        let document = self.document(context)?;
        let runtime = self.runtime();
        let selection = self.selection(context);
        if matches!(kind, McpReadKind::Jobs | McpReadKind::Schedules) {
            return self
                .read_jobs(context, kind, limit, &document, &selection)
                .await;
        }
        if kind == McpReadKind::DeployLogs {
            return self
                .read_deploy_logs(context, (cursor.as_deref(), limit), &selection)
                .await;
        }
        if let Some(value) = self.read_reconciled_state(context, kind, &document, &runtime)? {
            return Ok(value);
        }
        match kind {
            McpReadKind::Project => json_value(serde_json::json!({
                "schema_version": MCP_RESOURCE_SCHEMA_VERSION,
                "project": document.config.project,
                "config_path": document.path,
                "config_split_files": document
                    .split_sources()
                    .map(|(key, path)| (key.to_owned(), path.to_path_buf()))
                    .collect::<std::collections::BTreeMap<_, _>>(),
                "config_hash": document.source_hash,
                "selected_environment": context.scope.environment,
                "selected_target": context.scope.target,
                "selected_runtime": context.scope.runtime,
                "remote": context.remote,
            })),
            McpReadKind::Environments => json_value(serde_json::json!({
                "schema_version": MCP_RESOURCE_SCHEMA_VERSION,
                "selected": context.scope.environment,
                "default": document.config.default_environment,
                "environments": document.config.environments,
            })),
            McpReadKind::Targets => json_value(
                target_services(Some(&self.mm_home), runtime.process(), runtime.clock())?
                    .manager
                    .list()?,
            ),
            McpReadKind::Jobs
            | McpReadKind::Schedules
            | McpReadKind::Gates
            | McpReadKind::DeployLogs => Err(MmError::State {
                message: "unreachable MCP reconciled category".to_owned(),
            }),
            McpReadKind::Commands => {
                super::mcp_commands::page(context, &self.mm_home, (cursor, limit))
            }
            McpReadKind::Nodes | McpReadKind::Mesh | McpReadKind::Deployments => {
                self.read_cluster(context, kind, &selection).await
            }
            McpReadKind::Secrets | McpReadKind::Env => {
                self.read_variables(context, kind, None).await
            }
            McpReadKind::Optional => self.read_optional(context),
            McpReadKind::SecretStore => self.read_secret_store(context, &selection).await,
            // The same read as `mm target storage status`, as the agent.
            McpReadKind::Storage => json_value(
                super::target_storage(
                    &selection,
                    Some(&self.mm_home),
                    super::RemoteIdentity::Agent,
                    true,
                )
                .await?,
            ),
            McpReadKind::Plans => self.read_plans(context, &runtime, &selection).await,
            McpReadKind::Health => self.read_health(context, &runtime, &selection).await,
            McpReadKind::Services
            | McpReadKind::Routes
            | McpReadKind::Names
            | McpReadKind::Operations => {
                self.read_runtime_snapshot(
                    context,
                    kind,
                    (cursor.as_deref(), limit),
                    &runtime,
                    &selection,
                )
                .await
            }
        }
    }

    async fn logs(
        &self,
        context: &McpRequestContext,
        request: McpLogRequest,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<Vec<crate::domain::observability::LogRecord>, MmError> {
        context.require(AgentAction::Logs, None)?;
        if request.tail == 0 || request.tail > MCP_MAX_LOG_RECORDS {
            return Err(MmError::Security {
                message: format!("MCP log tail must be between 1 and {MCP_MAX_LOG_RECORDS}"),
            }
            .in_own_words());
        }
        let selection = self.selection(context);
        let runtime = self.runtime();
        let log_request = LogRequest {
            selector: Some(ServiceSelector {
                stack: request.stack,
                module: None,
                service: request.service,
                container: None,
            }),
            nodes: Vec::new(),
            follow: false,
            tail: request.tail,
            since_unix_ms: request.since_unix_ms,
            until_unix_ms: request.until_unix_ms,
            timestamps: true,
            output: LogOutputPolicy::Both,
            mode: LogMode::Redacted,
            limits: LogLimits {
                maximum_line_bytes: 16 * 1024,
                maximum_bytes_per_second: Some(MCP_MAX_LOG_BYTES as u64),
                maximum_lines_per_second: Some(MCP_MAX_LOG_RECORDS),
                buffered_records: MCP_MAX_LOG_RECORDS as usize,
            },
            approval: self.approval(),
        };
        let mut stream = self
            .gateway(&selection, context, &runtime)?
            .logs(log_request, cancellation.clone())
            .await?;
        let mut records = Vec::new();
        let mut bytes = 0_usize;
        while let Some(record) = stream.recv().await {
            let record = record?;
            bytes = bytes.saturating_add(record.message.len());
            if bytes > MCP_MAX_LOG_BYTES || records.len() >= MCP_MAX_LOG_RECORDS as usize {
                stream.cancel();
                break;
            }
            records.push(record);
        }
        Ok(records)
    }

    async fn exec(
        &self,
        context: &McpRequestContext,
        request: McpExecRequest,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<McpExecResult, MmError> {
        self.run_exec(context, request, cancellation).await
    }

    async fn plan_config(
        &self,
        context: &McpRequestContext,
        mutation: ConfigMutationRequest,
    ) -> Result<crate::domain::onboarding::ConfigMutationPlan, MmError> {
        context.require(AgentAction::Plan, None)?;
        refuse_access_edit(&mutation)?;
        super::onboarding_service().plan_mutation(&context.project_root, mutation)
    }

    async fn apply_config(
        &self,
        context: &McpRequestContext,
        mutation: ConfigMutationRequest,
        plan_hash: &str,
    ) -> Result<crate::domain::onboarding::ConfigMutationResult, MmError> {
        context.require(AgentAction::ConfigApply, None)?;
        refuse_access_edit(&mutation)?;
        super::onboarding_service().apply_mutation(&context.project_root, mutation, plan_hash)
    }

    async fn plan_build(
        &self,
        context: &McpRequestContext,
        publish: bool,
    ) -> Result<crate::domain::build::ImageBuildPlan, MmError> {
        context.require(AgentAction::Plan, None)?;
        let selection = self.selection(context);
        self.runtime()
            .build_manager_for(
                &selection,
                Some(&self.mm_home),
                (context.scope.runtime, publish),
            )?
            .plan(&selection, publish)
            .await
    }

    async fn apply_build(
        &self,
        context: &McpRequestContext,
        publish: bool,
        plan_hash: &str,
    ) -> Result<crate::domain::build::ImageBuildResult, MmError> {
        context.require(AgentAction::BuildApply, None)?;
        let runtime = self.runtime();
        let selection = self.selection(context);
        let planner = runtime.build_manager_for(
            &selection,
            Some(&self.mm_home),
            (context.scope.runtime, publish),
        )?;
        runtime
            .build_service(planner, &selection, Some(&self.mm_home))?
            .apply_reviewed(&selection, publish, plan_hash)
            .await
    }

    async fn stage_value(
        &self,
        context: &McpRequestContext,
        request: crate::domain::mcp::McpStageValueRequest,
    ) -> Result<serde_json::Value, MmError> {
        self.stage_as_agent(context, request).await
    }

    async fn development_unit(
        &self,
        context: &McpRequestContext,
        request: crate::domain::mcp::McpDevelopmentUnitRequest,
    ) -> Result<serde_json::Value, MmError> {
        self.development_unit_as_agent(context, request)
    }

    async fn plan_deployment(
        &self,
        context: &McpRequestContext,
        (kind, release): (
            crate::domain::mcp::McpDeployPlanKind,
            &crate::domain::release_facts::ReleaseRequest,
        ),
    ) -> Result<serde_json::Value, MmError> {
        use crate::domain::mcp::McpDeployPlanKind as K;
        context.require(AgentAction::Plan, None)?;
        if kind != K::Checkout && !release.is_empty() {
            return Err(MmError::ReleaseInput {
                message: if kind == K::Rollback {
                    "a rollback returns to the release the hosts ran before, with the name and inputs it ran with; leave out release"
                } else {
                    "a redeploy keeps the release the hosts run, with its name and inputs; leave out release, or plan this checkout with it"
                }
                .to_owned(),
            }
            .in_own_words());
        }
        let runtime = self.runtime();
        let selection = self.selection(context);
        if let Some(environment) = self.remote(&selection, context)? {
            self.plan_remote(&environment, (kind, release), &selection)
                .await
        } else if kind != K::Checkout {
            Err(MmError::Environment {
                message: if kind == K::Rollback {
                    format!(
                        "a rollback returns a remote environment to what its hosts ran before, and environment '{}' runs on this machine. To return to an earlier version here, check out its commit and plan a deployment",
                        context.scope.environment
                    )
                } else {
                    format!(
                        "a redeploy deploys again the commit a remote environment's hosts run, and environment '{}' runs on this machine. Here, plan a deployment of this checkout",
                        context.scope.environment
                    )
                },
            }
            .in_own_words())
        } else {
            let selection = super::released(&selection, Some(release.clone()));
            json_value(
                runtime
                    .deployment_service(&selection, Some(&self.mm_home))?
                    .plan(&selection)
                    .await?,
            )
        }
    }

    async fn apply_deployment(
        &self,
        context: &McpRequestContext,
        plan_hash: &str,
        release: &crate::domain::release_facts::ReleaseRequest,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::DeployApply, None)?;
        let runtime = self.runtime();
        let selection = self.selection(context);
        let Some(environment) = self.remote(&selection, context)? else {
            let selection = super::released(&selection, Some(release.clone()));
            return json_value(
                runtime
                    .deployment_service(&selection, Some(&self.mm_home))?
                    .apply_reviewed(&selection, plan_hash)
                    .await?,
            );
        };
        if !release.is_empty() {
            return Err(MmError::ReleaseInput {
                message: "a remote deployment plan carries its release; apply it with the plan hash alone".to_owned(),
            }
            .in_own_words());
        }
        let mut report = environment.apply_deployment(plan_hash, &|_| {}).await?;
        if let Some(failure) = report.failure("mm deploy plan") {
            return Err(MmError::RuntimeResponse { message: failure });
        }
        if environment.is_cluster() {
            return json_value(report);
        }
        json_value(report.nodes.pop().and_then(|node| node.result))
    }

    async fn read_values(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        key: Option<&str>,
    ) -> Result<serde_json::Value, MmError> {
        self.read_variables(context, kind, key).await
    }

    async fn plan_service_restart(
        &self,
        context: &McpRequestContext,
        service: &str,
    ) -> Result<crate::domain::mcp::McpServiceRestartPlan, MmError> {
        self.plan_restart(context, service).await
    }

    async fn apply_service_restart(
        &self,
        context: &McpRequestContext,
        service: &str,
        plan_hash: &str,
    ) -> Result<crate::domain::observability::RestartOutcome, MmError> {
        self.apply_restart(context, service, plan_hash).await
    }

    async fn plan_setup(
        &self,
        context: &McpRequestContext,
        step: &crate::domain::mcp::McpSetupStep,
    ) -> Result<serde_json::Value, MmError> {
        self.plan_setup_step(context, step).await
    }

    async fn apply_setup(
        &self,
        context: &McpRequestContext,
        step: &crate::domain::mcp::McpSetupStep,
        plan_hash: &str,
    ) -> Result<serde_json::Value, MmError> {
        self.apply_setup_step(context, step, plan_hash).await
    }

    async fn plan_job(
        &self,
        context: &McpRequestContext,
        action: McpJobAction,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::Plan, None)?;
        self.refuse_remote_schedules(context, &action)?;
        let runtime = self.runtime();
        let scheduler = job_scheduler_service(
            &context.project_root,
            Some(&self.mm_home),
            context.scope.runtime,
            &runtime,
        )?;
        if action == McpJobAction::Reconcile {
            return json_value(
                scheduler.plan_reconciliation(&self.selection(context), &self.approval())?,
            );
        }
        let plan_hash = self.job_plan_hash(context, &action)?;
        json_value(serde_json::json!({
            "schema": "mm-mcp-job-action-v1",
            "scope": context.scope,
            "action": action,
            "plan_hash": plan_hash,
        }))
    }

    async fn apply_job(
        &self,
        context: &McpRequestContext,
        action: McpJobAction,
        plan_hash: &str,
    ) -> Result<serde_json::Value, MmError> {
        context.require(action.agent_action(), None)?;
        self.refuse_remote_schedules(context, &action)?;
        let runtime = self.runtime();
        let scheduler = job_scheduler_service(
            &context.project_root,
            Some(&self.mm_home),
            context.scope.runtime,
            &runtime,
        )?;
        let selection = self.selection(context);
        let approval = self.approval();
        if action == McpJobAction::Reconcile {
            return json_value(scheduler.apply_reconciliation(&selection, &approval, plan_hash)?);
        }
        let current = self.job_plan_hash(context, &action)?;
        if current != plan_hash {
            return Err(MmError::Security {
                message: "MCP job action plan is stale; review the current action plan".to_owned(),
            }
            .in_own_words());
        }
        // Running, retrying, and cancelling a declared job take the same
        // path as `mm job`: on a remote environment, its host.
        let jobs = || {
            super::job_gateway_as(
                &selection,
                Some(&self.mm_home),
                context.scope.runtime,
                &runtime,
                super::RemoteIdentity::Agent,
            )
        };
        match action {
            McpJobAction::Reconcile => Err(MmError::State {
                message: "unreachable reconciler action".to_owned(),
            }),
            McpJobAction::Enqueue { name } => json_value(
                jobs()?
                    .run(
                        crate::domain::scheduler::JobRunRequest::manual(&name, false),
                        &approval,
                    )
                    .await?,
            ),
            McpJobAction::RunSchedule { name } => {
                json_value(scheduler.run_schedule_now(&selection, &name, &approval)?)
            }
            McpJobAction::Cancel { run_id } => json_value(jobs()?.cancel(run_id, &approval).await?),
            McpJobAction::Retry { run_id } => {
                json_value(jobs()?.retry(run_id, false, &approval).await?)
            }
            McpJobAction::SetScheduleEnabled { name, enabled } => {
                json_value(scheduler.set_schedule_enabled(&selection, &name, enabled, &approval)?)
            }
        }
    }
}

/// Who may reach an environment is never an agent's edit to make, at any
/// level: the users file goes through its own review.
fn refuse_access_edit(mutation: &ConfigMutationRequest) -> Result<(), MmError> {
    if mutation.resource == ConfigResourceKind::User {
        return Err(MmError::Security {
            message: "the users file is not edited through MCP".to_owned(),
        }
        .in_own_words());
    }
    Ok(())
}

fn json_value(value: impl serde::Serialize) -> Result<serde_json::Value, MmError> {
    serde_json::to_value(value).map_err(state_json)
}

fn state_json(error: serde_json::Error) -> MmError {
    MmError::State {
        message: format!("serializing typed MCP application result: {error}"),
    }
}

fn decode_cursor(cursor: Option<&str>) -> Result<usize, MmError> {
    cursor.map_or(Ok(0), |value| {
        value.parse::<usize>().map_err(|_| MmError::Environment {
            message: "MCP cursor is invalid".to_owned(),
        })
    })
}

fn page_value(
    values: Vec<serde_json::Value>,
    offset: usize,
    limit: usize,
) -> Result<serde_json::Value, MmError> {
    if limit == 0 || offset > values.len() {
        return Err(MmError::Environment {
            message: "MCP page bounds are invalid".to_owned(),
        });
    }
    let total = values.len();
    let end = offset.saturating_add(limit).min(total);
    json_value(crate::domain::mcp::McpPage {
        schema_version: MCP_RESOURCE_SCHEMA_VERSION.to_owned(),
        items: values[offset..end].to_vec(),
        next_cursor: (end < total).then(|| end.to_string()),
        total_items: total,
    })
}

#[path = "mcp/access.rs"]
mod access;
#[path = "mcp/development.rs"]
mod development;
#[path = "mcp/exec.rs"]
mod exec;
#[path = "mcp/jobs.rs"]
mod jobs;
#[path = "mcp/plans.rs"]
mod plans;
#[path = "mcp/reads.rs"]
mod reads;
#[path = "mcp/restart.rs"]
mod restart;
#[path = "mcp/setup.rs"]
mod setup;
#[path = "mcp/values.rs"]
mod values;
#[path = "mcp/variables.rs"]
mod variables;

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
