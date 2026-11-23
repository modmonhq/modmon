use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolRequestParams, ReadResourceRequestParams};
use rmcp::{ServerHandler, ServiceExt};

use super::McpServer;
use crate::app::ports::{McpAuditJournal, McpUseCases};
use crate::domain::agents::{AgentAccess, AgentAction};
use crate::domain::error::MmError;
use crate::domain::host_policy::{AccessLevel, ServiceSelection};
use crate::domain::mcp::{
    MCP_SOURCE, McpAccessOverview, McpApplyRequest, McpAuditRecord, McpAuthorizationError,
    McpAuthorizationRequest, McpConfigApplyRequest, McpConfigPlanRequest, McpDenial,
    McpDenialReason, McpDeployPlanRequest, McpEnvironmentAccess, McpExecRequest, McpExecResult,
    McpJobAction, McpJobApplyRequest, McpJobPlanRequest, McpLogRequest, McpReadKind,
    McpReadRequest, McpRequestContext, McpScope, McpSelectedScope, McpServerConfig,
};
use crate::domain::observability::{LogRecord, OperationCancellation};
use crate::domain::onboarding::{
    ConfigMutationAction, ConfigMutationPlan, ConfigMutationRequest, ConfigMutationResult,
    ConfigResourceKind,
};
use crate::domain::runtime::RuntimeKind;

const SENTINEL: &str = "TOP_SECRET_MCP_SENTINEL";

#[derive(Default)]
struct RecordingAudit {
    records: Mutex<Vec<McpAuditRecord>>,
    fail: AtomicBool,
}

impl RecordingAudit {
    fn outcomes(&self) -> Vec<(String, String)> {
        self.records.lock().map_or_else(
            |_| Vec::new(),
            |records| {
                records
                    .iter()
                    .map(|record| (record.operation.clone(), record.outcome_code.clone()))
                    .collect()
            },
        )
    }
}

impl McpAuditJournal for RecordingAudit {
    fn append_mcp_audit(&self, record: &McpAuditRecord) -> Result<(), MmError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(MmError::State {
                message: "injected audit failure".to_owned(),
            });
        }
        self.records
            .lock()
            .map_err(|_| MmError::State {
                message: "test audit lock poisoned".to_owned(),
            })?
            .push(record.clone());
        Ok(())
    }
}

/// An application with three environments: `local` (deploy and exec),
/// `staging` (operate, exec in `api` only), and `production` (view).
struct FakeUseCases {
    generation: AtomicUsize,
    plans: AtomicUsize,
    applies: AtomicUsize,
    execs: AtomicUsize,
    jobs: AtomicUsize,
    deploys: AtomicUsize,
    dropped_operation: Arc<AtomicBool>,
    access: BTreeMap<String, AgentAccess>,
    /// The `agents:` block was edited: only `view` is in force.
    untrusted: AtomicBool,
    /// Remote environments, and whether this device has an agent key.
    remote: Vec<String>,
    agent_key: AtomicBool,
}

impl Default for FakeUseCases {
    fn default() -> Self {
        let grant = |level, exec| AgentAccess {
            level,
            exec,
            secrets: false,
        };
        Self {
            generation: AtomicUsize::new(1),
            plans: AtomicUsize::new(0),
            applies: AtomicUsize::new(0),
            execs: AtomicUsize::new(0),
            jobs: AtomicUsize::new(0),
            deploys: AtomicUsize::new(0),
            dropped_operation: Arc::new(AtomicBool::new(false)),
            access: BTreeMap::from([
                (
                    "local".to_owned(),
                    grant(AccessLevel::Deploy, ServiceSelection::All(true)),
                ),
                (
                    "staging".to_owned(),
                    grant(
                        AccessLevel::Operate,
                        ServiceSelection::Only(["api".to_owned()].into()),
                    ),
                ),
                (
                    "production".to_owned(),
                    grant(AccessLevel::View, ServiceSelection::All(false)),
                ),
            ]),
            untrusted: AtomicBool::new(false),
            remote: vec!["staging".to_owned(), "production".to_owned()],
            agent_key: AtomicBool::new(true),
        }
    }
}

impl FakeUseCases {
    fn plan(&self, mutation: ConfigMutationRequest) -> ConfigMutationPlan {
        let generation = self.generation.load(Ordering::SeqCst).max(1);
        ConfigMutationPlan {
            request: mutation,
            affected_files: vec![PathBuf::from("/project/.config/mm.yaml")],
            canonical_diff: format!("generation {generation}"),
            generated_resources: Vec::new(),
            generated_routes: Vec::new(),
            privilege_prompts: Vec::new(),
            source_hash: "b".repeat(64),
            result_hash: "c".repeat(64),
            plan_hash: format!("{generation:064x}"),
        }
    }
}

struct DropMarker(Arc<AtomicBool>);

impl Drop for DropMarker {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl McpUseCases for FakeUseCases {
    async fn authorize(
        &self,
        request: McpAuthorizationRequest,
    ) -> Result<McpRequestContext, McpAuthorizationError> {
        let environment = request.scope.environment.clone();
        let declared = self.access.get(&environment).cloned().ok_or_else(|| {
            McpAuthorizationError::Failed(MmError::Environment {
                message: "unknown environment".to_owned(),
            })
        })?;
        let untrusted = self.untrusted.load(Ordering::SeqCst);
        let access = if untrusted {
            AgentAccess::VIEW
        } else {
            declared.clone()
        };
        let request_id = uuid::Uuid::new_v4();
        let service = request.service.as_deref();
        let reason = if !access.permits(request.action, service) {
            Some(if untrusted && declared.permits(request.action, service) {
                McpDenialReason::PolicyNotTrusted
            } else {
                McpDenialReason::NotGranted
            })
        } else if self.remote.contains(&environment) && !self.agent_key.load(Ordering::SeqCst) {
            Some(McpDenialReason::AgentKeyMissing)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(McpAuthorizationError::Denied(Box::new(McpDenial {
                request_id,
                reason,
                project_root: PathBuf::from("/project"),
                environment,
                action: request.action,
                service: request.service,
                access,
            })));
        }
        Ok(McpRequestContext {
            request_id,
            principal: "agent".to_owned(),
            source: MCP_SOURCE.to_owned(),
            project_root: PathBuf::from("/project"),
            scope: McpSelectedScope {
                environment: environment.clone(),
                target: request.scope.target.unwrap_or_else(|| "local".to_owned()),
                runtime: request.scope.runtime.unwrap_or(RuntimeKind::DockerCompose),
            },
            remote: self.remote.contains(&environment),
            access,
        })
    }

    async fn access(&self) -> Result<McpAccessOverview, MmError> {
        Ok(McpAccessOverview {
            schema_version: "test".to_owned(),
            project_root: PathBuf::from("/project"),
            policy_trusted: !self.untrusted.load(Ordering::SeqCst),
            default_environment: Some("local".to_owned()),
            environments: self
                .access
                .iter()
                .map(|(environment, access)| McpEnvironmentAccess {
                    environment: environment.clone(),
                    access: access.summary(),
                    level: access.level,
                    exec: access.exec.clone(),
                })
                .collect(),
            outside_the_grant: "ask the user".to_owned(),
        })
    }

    async fn read(
        &self,
        _context: &McpRequestContext,
        kind: McpReadKind,
        _cursor: Option<String>,
        _limit: usize,
    ) -> Result<serde_json::Value, MmError> {
        if kind == McpReadKind::Health {
            let _marker = DropMarker(Arc::clone(&self.dropped_operation));
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        Ok(serde_json::json!({
            "public": "visible",
            "access_token": SENTINEL,
        }))
    }

    async fn logs(
        &self,
        _context: &McpRequestContext,
        _request: McpLogRequest,
        _cancellation: OperationCancellation,
    ) -> Result<Vec<LogRecord>, MmError> {
        Ok(Vec::new())
    }

    async fn exec(
        &self,
        context: &McpRequestContext,
        request: McpExecRequest,
        _cancellation: OperationCancellation,
    ) -> Result<McpExecResult, MmError> {
        context.require(AgentAction::Exec, Some(&request.service))?;
        self.execs.fetch_add(1, Ordering::SeqCst);
        Ok(McpExecResult {
            schema_version: "test".to_owned(),
            service: request.service,
            exit_code: 0,
            stdout: request.argv.join(" "),
            stderr: String::new(),
            truncated: false,
        })
    }

    async fn plan_config(
        &self,
        _context: &McpRequestContext,
        mutation: ConfigMutationRequest,
    ) -> Result<ConfigMutationPlan, MmError> {
        self.plans.fetch_add(1, Ordering::SeqCst);
        Ok(self.plan(mutation))
    }

    async fn apply_config(
        &self,
        context: &McpRequestContext,
        mutation: ConfigMutationRequest,
        plan_hash: &str,
    ) -> Result<ConfigMutationResult, MmError> {
        context.require(AgentAction::ConfigApply, None)?;
        let current = self.plan(mutation);
        if current.plan_hash != plan_hash {
            return Err(MmError::State {
                message: "reviewed plan changed".to_owned(),
            });
        }
        self.applies.fetch_add(1, Ordering::SeqCst);
        Ok(ConfigMutationResult {
            config_path: PathBuf::from("/project/.config/mm.yaml"),
            source_hash: current.source_hash,
            plan_hash: current.plan_hash,
        })
    }

    async fn plan_build(
        &self,
        _context: &McpRequestContext,
        _publish: bool,
    ) -> Result<crate::domain::build::ImageBuildPlan, MmError> {
        Err(unused())
    }

    async fn apply_build(
        &self,
        _context: &McpRequestContext,
        _publish: bool,
        _plan_hash: &str,
    ) -> Result<crate::domain::build::ImageBuildResult, MmError> {
        Err(unused())
    }

    async fn plan_deployment(
        &self,
        _context: &McpRequestContext,
        _request: (
            crate::domain::mcp::McpDeployPlanKind,
            &crate::domain::release_facts::ReleaseRequest,
        ),
    ) -> Result<serde_json::Value, MmError> {
        Ok(serde_json::json!({ "plan_hash": "d".repeat(64), "changes": ["api"] }))
    }

    async fn apply_deployment(
        &self,
        context: &McpRequestContext,
        _plan_hash: &str,
        _release: &crate::domain::release_facts::ReleaseRequest,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::DeployApply, None)?;
        self.deploys.fetch_add(1, Ordering::SeqCst);
        Err(unused())
    }

    async fn plan_job(
        &self,
        _context: &McpRequestContext,
        action: McpJobAction,
    ) -> Result<serde_json::Value, MmError> {
        Ok(serde_json::json!({ "plan_hash": "e".repeat(64), "action": action }))
    }

    async fn apply_job(
        &self,
        context: &McpRequestContext,
        action: McpJobAction,
        _plan_hash: &str,
    ) -> Result<serde_json::Value, MmError> {
        context.require(action.agent_action(), None)?;
        self.jobs.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::json!({ "queued": true }))
    }
}

fn unused() -> MmError {
    MmError::State {
        message: "unused test operation".to_owned(),
    }
}

fn scope() -> McpScope {
    scope_in("local")
}

fn scope_in(environment: &str) -> McpScope {
    McpScope {
        environment: environment.to_owned(),
        target: None,
        runtime: None,
    }
}

fn mutation() -> ConfigMutationRequest {
    ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Module,
        name: "api".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({"path": "api", "services": {}})),
    }
}

fn fixture(
    timeout_ms: u64,
    maximum_response_bytes: usize,
) -> (McpServer, Arc<FakeUseCases>, Arc<RecordingAudit>) {
    let use_cases = Arc::new(FakeUseCases::default());
    let audit = Arc::new(RecordingAudit::default());
    let config = McpServerConfig {
        maximum_page_size: 10,
        maximum_response_bytes,
        request_timeout_ms: timeout_ms,
        ..McpServerConfig::default()
    };
    (
        McpServer::new(config, use_cases.clone(), audit.clone()),
        use_cases,
        audit,
    )
}

fn structured(result: rmcp::model::CallToolResult) -> serde_json::Value {
    result.structured_content.unwrap_or(serde_json::Value::Null)
}

fn exec_request(environment: &str, service: &str, argv: &[&str]) -> McpExecRequest {
    McpExecRequest {
        scope: scope_in(environment),
        service: service.to_owned(),
        stack: None,
        argv: argv.iter().map(|value| (*value).to_owned()).collect(),
        workdir: None,
        user: None,
        timeout_seconds: 30,
    }
}

fn user_commands(result: &serde_json::Value) -> Vec<String> {
    result["error"]["handoff"]["user_commands"]
        .as_array()
        .map(|commands| {
            commands
                .iter()
                .filter_map(|command| command.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn exec_runs_where_granted_and_is_handed_to_the_user_elsewhere() {
    let (server, use_cases, audit) = fixture(1_000, 64 * 1024);
    let local = structured(
        server
            .exec(Parameters(exec_request(
                "local",
                "postgres",
                &["psql", "-c", "select 1"],
            )))
            .await,
    );
    assert_eq!(local["exit_code"], 0);
    assert_eq!(local["stdout"], "psql -c select 1");

    // staging grants exec in `api` only.
    let api = structured(
        server
            .exec(Parameters(exec_request("staging", "api", &["ls"])))
            .await,
    );
    assert_eq!(api["exit_code"], 0);
    let other = structured(
        server
            .exec(Parameters(exec_request("staging", "postgres", &["ls"])))
            .await,
    );
    assert_eq!(other["error"]["code"], "MM_AGENT_HANDOFF");

    let production = structured(
        server
            .exec(Parameters(exec_request(
                "production",
                "api",
                &["sh", "-c", "echo it's live"],
            )))
            .await,
    );
    assert_eq!(production["error"]["code"], "MM_AGENT_HANDOFF");
    assert_eq!(production["error"]["mutation_state"], "not_started");
    assert_eq!(production["error"]["handoff"]["reason"], "not_granted");
    assert_eq!(production["error"]["handoff"]["granted"], "view");
    assert_eq!(production["error"]["handoff"]["requires"], "exec");
    assert_eq!(
        user_commands(&production),
        [r"mm --environment production service exec api -- sh -c 'echo it'\''s live'"]
    );
    assert!(
        production["error"]["next_action"]
            .as_str()
            .is_some_and(|text| text.contains("their own terminal"))
    );
    // The user's command carries every option the agent asked for.
    let detailed = structured(
        server
            .exec(Parameters(McpExecRequest {
                stack: Some("app".to_owned()),
                user: Some("www-data".to_owned()),
                workdir: Some(PathBuf::from("/srv/app")),
                timeout_seconds: 120,
                ..exec_request("production", "api", &["ls"])
            }))
            .await,
    );
    assert_eq!(
        user_commands(&detailed),
        [
            "mm --environment production service exec --stack app --user www-data --workdir /srv/app --timeout 120 api -- ls"
        ]
    );
    assert_eq!(use_cases.execs.load(Ordering::SeqCst), 2);
    assert_eq!(
        audit.outcomes(),
        [
            ("exec".to_owned(), "OK".to_owned()),
            ("exec".to_owned(), "OK".to_owned()),
            ("exec".to_owned(), "MM_AGENT_HANDOFF".to_owned()),
            ("exec".to_owned(), "MM_AGENT_HANDOFF".to_owned()),
            ("exec".to_owned(), "MM_AGENT_HANDOFF".to_owned()),
        ]
    );
}

#[tokio::test]
async fn a_view_environment_plans_for_the_user_and_never_applies() {
    let (server, use_cases, _audit) = fixture(1_000, 64 * 1024);
    let plan = structured(
        server
            .deploy_plan(Parameters(McpDeployPlanRequest {
                scope: scope_in("production"),
                publish: false,
                rollback: false,
                redeploy: false,
                release: Default::default(),
            }))
            .await,
    );
    assert_eq!(plan["applied_by"], "user");
    assert!(plan.get("approval_token").is_none());
    assert_eq!(plan["plan"]["changes"][0], "api");
    assert_eq!(
        plan["user_commands"],
        serde_json::json!([
            "mm --environment production deploy plan",
            "mm --environment production deploy apply --plan HASH --yes",
        ])
    );

    // A guessed token and confirmation do not help: the grant is checked first.
    let apply = structured(
        server
            .deploy_apply(Parameters(McpApplyRequest {
                scope: scope_in("production"),
                publish: false,
                plan_hash: "d".repeat(64),
                approval_token: "0".repeat(64),
                confirmation: format!("apply:{}", "d".repeat(64)),
                release: Default::default(),
            }))
            .await,
    );
    assert_eq!(apply["error"]["code"], "MM_AGENT_HANDOFF");
    assert_eq!(apply["error"]["handoff"]["requires"], "level deploy");
    assert_eq!(user_commands(&apply).len(), 2);
    assert_eq!(use_cases.deploys.load(Ordering::SeqCst), 0);

    // Where deploy is granted, the plan carries a token for the agent.
    let local = structured(
        server
            .deploy_plan(Parameters(McpDeployPlanRequest {
                scope: scope(),
                publish: false,
                rollback: false,
                redeploy: false,
                release: Default::default(),
            }))
            .await,
    );
    assert_eq!(local["applied_by"], "agent");
    assert_eq!(local["approval_token"].as_str().map(str::len), Some(64));
}

#[tokio::test]
async fn job_actions_need_operate_and_schedule_changes_need_deploy() {
    let (server, use_cases, _audit) = fixture(1_000, 64 * 1024);
    let run = |environment: &str, action: McpJobAction| {
        let server = server.clone();
        let scope = scope_in(environment);
        async move {
            let plan = structured(
                server
                    .job_plan(Parameters(McpJobPlanRequest {
                        scope: scope.clone(),
                        action: action.clone(),
                    }))
                    .await,
            );
            let plan_hash = plan["plan_hash"].as_str().unwrap_or_default().to_owned();
            let token = plan["approval_token"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let applied = structured(
                server
                    .job_apply(Parameters(McpJobApplyRequest {
                        scope,
                        action,
                        confirmation: format!("apply:{plan_hash}"),
                        plan_hash,
                        approval_token: token,
                    }))
                    .await,
            );
            (plan, applied)
        }
    };
    let enqueue = McpJobAction::Enqueue {
        name: "migrate".to_owned(),
    };
    let (_, staging) = run("staging", enqueue.clone()).await;
    assert_eq!(staging["queued"], true);

    let (plan, production) = run("production", enqueue).await;
    assert_eq!(plan["applied_by"], "user");
    assert_eq!(production["error"]["code"], "MM_AGENT_HANDOFF");
    assert_eq!(
        user_commands(&production),
        ["mm --environment production job run migrate"]
    );

    let (plan, disabled) = run(
        "staging",
        McpJobAction::SetScheduleEnabled {
            name: "nightly".to_owned(),
            enabled: false,
        },
    )
    .await;
    assert_eq!(plan["applied_by"], "user");
    assert_eq!(disabled["error"]["handoff"]["requires"], "level deploy");
    assert_eq!(
        user_commands(&disabled),
        ["mm --environment staging job schedule disable nightly"]
    );
    assert_eq!(use_cases.jobs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_edited_agents_block_and_a_missing_agent_key_name_the_fix() {
    let (server, use_cases, _audit) = fixture(1_000, 64 * 1024);
    use_cases.untrusted.store(true, Ordering::SeqCst);
    let untrusted = structured(
        server
            .exec(Parameters(exec_request("local", "api", &["ls"])))
            .await,
    );
    assert_eq!(
        untrusted["error"]["handoff"]["reason"],
        "policy_not_trusted"
    );
    assert_eq!(user_commands(&untrusted), ["mm agent trust"]);
    // What the block never granted is still an ordinary refusal.
    let never = structured(
        server
            .exec(Parameters(exec_request("production", "api", &["ls"])))
            .await,
    );
    assert_eq!(never["error"]["handoff"]["reason"], "not_granted");
    use_cases.untrusted.store(false, Ordering::SeqCst);

    use_cases.agent_key.store(false, Ordering::SeqCst);
    let read = structured(
        server
            .read(Parameters(McpReadRequest {
                scope: scope_in("production"),
                kind: McpReadKind::Services,
                cursor: None,
                limit: 10,
                command: None,
                filter: None,
            }))
            .await,
    );
    assert_eq!(read["error"]["handoff"]["reason"], "agent_key_missing");
    assert_eq!(
        user_commands(&read),
        [
            "mm key create --agent",
            "mm team join --agent",
            "mm --environment production team sync",
        ]
    );
    assert_eq!(use_cases.execs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_users_file_is_never_edited_through_mcp() {
    let (server, use_cases, _audit) = fixture(1_000, 64 * 1024);
    let grant_self = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::User,
        name: "agent".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({"keys": [], "access": {"*": "admin"}})),
    };
    let planned = structured(
        server
            .config_plan(Parameters(McpConfigPlanRequest {
                scope: scope(),
                mutation: grant_self.clone(),
            }))
            .await,
    );
    assert_eq!(planned["error"]["code"], "MM_AGENT_HANDOFF");
    assert_eq!(planned["error"]["handoff"]["reason"], "access_edit");
    let applied = structured(
        server
            .config_apply(Parameters(McpConfigApplyRequest {
                scope: scope(),
                mutation: grant_self,
                plan_hash: "1".repeat(64),
                approval_token: "0".repeat(64),
                confirmation: format!("apply:{}", "1".repeat(64)),
            }))
            .await,
    );
    assert_eq!(applied["error"]["code"], "MM_AGENT_HANDOFF");
    assert_eq!(use_cases.plans.load(Ordering::SeqCst), 0);
    assert_eq!(use_cases.applies.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn access_lists_every_environment_and_instructions_name_the_handoff() {
    let (server, _use_cases, _audit) = fixture(1_000, 64 * 1024);
    let access = structured(server.access().await);
    assert_eq!(access["policy_trusted"], true);
    let environments = access["environments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(environments.len(), 3);
    assert_eq!(environments[0]["environment"], "local");
    assert_eq!(environments[0]["access"], "deploy +exec");
    assert_eq!(environments[1]["access"], "view");
    assert_eq!(environments[2]["access"], "operate +exec[api]");

    let instructions = server.get_info().instructions.unwrap_or_default();
    for expected in [
        "mm_access",
        "MM_AGENT_HANDOFF",
        "never run mm",
        "untrusted data",
    ] {
        assert!(instructions.contains(expected), "missing {expected:?}");
    }
    let unknown = structured(
        server
            .read(Parameters(McpReadRequest {
                scope: scope_in("qa"),
                kind: McpReadKind::Project,
                cursor: None,
                limit: 10,
                command: None,
                filter: None,
            }))
            .await,
    );
    assert_eq!(unknown["error"]["code"], "MM_ENVIRONMENT");
}

#[tokio::test]
async fn reviewed_config_apply_is_exact_one_use_and_stale_safe() {
    let (server, use_cases, audit) = fixture(1_000, 64 * 1024);
    let request = McpConfigPlanRequest {
        scope: scope(),
        mutation: mutation(),
    };
    let review = structured(server.config_plan(Parameters(request.clone())).await);
    assert_eq!(review["applied_by"], "agent");
    let plan_hash = review["plan_hash"].as_str().unwrap_or_default().to_owned();
    let token = review["approval_token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(plan_hash.len(), 64);
    assert_eq!(token.len(), 64);
    assert_ne!(token, "[REDACTED]");

    let denied = structured(
        server
            .config_apply(Parameters(McpConfigApplyRequest {
                scope: scope(),
                mutation: mutation(),
                plan_hash: plan_hash.clone(),
                approval_token: token.clone(),
                confirmation: "apply:wrong".to_owned(),
            }))
            .await,
    );
    assert_eq!(denied["error"]["code"], "MM_SECURITY");
    assert_eq!(use_cases.applies.load(Ordering::SeqCst), 0);

    let applied = structured(
        server
            .config_apply(Parameters(McpConfigApplyRequest {
                scope: scope(),
                mutation: mutation(),
                plan_hash: plan_hash.clone(),
                approval_token: token.clone(),
                confirmation: format!("apply:{plan_hash}"),
            }))
            .await,
    );
    assert_eq!(applied["plan_hash"], plan_hash);
    assert_eq!(use_cases.applies.load(Ordering::SeqCst), 1);

    let replay = structured(
        server
            .config_apply(Parameters(McpConfigApplyRequest {
                scope: scope(),
                mutation: mutation(),
                plan_hash: plan_hash.clone(),
                approval_token: token,
                confirmation: format!("apply:{plan_hash}"),
            }))
            .await,
    );
    assert_eq!(replay["error"]["code"], "MM_SECURITY");

    let stale_review = structured(server.config_plan(Parameters(request)).await);
    let stale_hash = stale_review["plan_hash"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let stale_token = stale_review["approval_token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    use_cases.generation.store(2, Ordering::SeqCst);
    let stale = structured(
        server
            .config_apply(Parameters(McpConfigApplyRequest {
                scope: scope(),
                mutation: mutation(),
                plan_hash: stale_hash.clone(),
                approval_token: stale_token,
                confirmation: format!("apply:{stale_hash}"),
            }))
            .await,
    );
    assert_eq!(stale["error"]["code"], "MM_STATE");
    assert_eq!(use_cases.applies.load(Ordering::SeqCst), 1);
    assert!(audit.records.lock().is_ok_and(|records| records.len() == 8));
}

#[tokio::test]
async fn unavailable_audit_journal_prevents_mutation() {
    let (server, use_cases, audit) = fixture(1_000, 64 * 1024);
    let review = structured(
        server
            .config_plan(Parameters(McpConfigPlanRequest {
                scope: scope(),
                mutation: mutation(),
            }))
            .await,
    );
    let plan_hash = review["plan_hash"].as_str().unwrap_or_default().to_owned();
    let token = review["approval_token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    audit.fail.store(true, Ordering::SeqCst);
    let result = structured(
        server
            .config_apply(Parameters(McpConfigApplyRequest {
                scope: scope(),
                mutation: mutation(),
                plan_hash: plan_hash.clone(),
                approval_token: token,
                confirmation: format!("apply:{plan_hash}"),
            }))
            .await,
    );
    assert_eq!(result["error"]["code"], "MM_MCP_AUDIT");
    assert_eq!(use_cases.applies.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn sensitive_arguments_are_rejected_before_the_use_case_and_outputs_are_scrubbed() {
    let (server, use_cases, _audit) = fixture(1_000, 64 * 1024);
    let mut prohibited = mutation();
    prohibited.definition = Some(serde_json::json!({"password": SENTINEL}));
    let denied = structured(
        server
            .config_plan(Parameters(McpConfigPlanRequest {
                scope: scope(),
                mutation: prohibited,
            }))
            .await,
    );
    assert_eq!(denied["error"]["code"], "MM_SECURITY");
    assert!(!serde_json::to_string(&denied).is_ok_and(|text| text.contains(SENTINEL)));
    assert_eq!(use_cases.plans.load(Ordering::SeqCst), 0);

    let read = structured(
        server
            .read(Parameters(McpReadRequest {
                scope: scope(),
                kind: McpReadKind::Project,
                cursor: None,
                limit: 10,
                command: None,
                filter: None,
            }))
            .await,
    );
    assert_eq!(read["public"], "visible");
    assert_eq!(read["access_token"], "[REDACTED]");
    assert!(!serde_json::to_string(&read).is_ok_and(|text| text.contains(SENTINEL)));
}

#[tokio::test]
async fn bounds_and_timeout_drop_in_flight_operations() {
    let (bounded_server, _use_cases, _audit) = fixture(1_000, 32);
    let oversized = structured(
        bounded_server
            .read(Parameters(McpReadRequest {
                scope: scope(),
                kind: McpReadKind::Project,
                cursor: None,
                limit: 10,
                command: None,
                filter: None,
            }))
            .await,
    );
    assert_eq!(oversized["error"]["code"], "MM_MCP_RESULT_TOO_LARGE");

    let (timeout_server, use_cases, _audit) = fixture(10, 64 * 1024);
    let timeout = structured(
        timeout_server
            .read(Parameters(McpReadRequest {
                scope: scope(),
                kind: McpReadKind::Health,
                cursor: None,
                limit: 10,
                command: None,
                filter: None,
            }))
            .await,
    );
    // A call that runs out of time is a timeout, and says where to look.
    assert_eq!(timeout["error"]["code"], "MM_TIMEOUT");
    assert_eq!(timeout["error"]["retryable"], true);
    assert!(
        timeout["error"]["next_action"]
            .as_str()
            .is_some_and(|next| next.contains("'operations'") && next.contains("mm deploy cancel"))
    );
    assert!(use_cases.dropped_operation.load(Ordering::SeqCst));
}

#[test]
fn apply_request_schema_does_not_accept_secret_or_shell_fields() {
    let schema = serde_json::to_string(&schemars::schema_for!(McpApplyRequest)).unwrap_or_default();
    for forbidden in [
        "password",
        "private_key",
        "session_token",
        "command",
        "shell",
    ] {
        assert!(!schema.contains(forbidden), "schema exposed {forbidden}");
    }
}

#[test]
fn tool_schema_snapshot_is_versioned_and_has_no_shell_surface() {
    let (server, _use_cases, _audit) = fixture(1_000, 64 * 1024);
    let snapshot = server.schema_snapshot();
    let tools = snapshot["tools"].as_array().cloned().unwrap_or_default();
    let names = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "mm_access",
            "mm_build_apply",
            "mm_build_plan",
            "mm_config_apply",
            "mm_config_plan",
            "mm_deploy_apply",
            "mm_deploy_plan",
            "mm_dev_unit",
            "mm_exec",
            "mm_job_apply",
            "mm_job_plan",
            "mm_logs",
            "mm_read",
            "mm_setup_apply",
            "mm_setup_plan",
            "mm_stage_value",
        ]
    );
    assert_eq!(snapshot["schema_version"], "mm-mcp-tools-v1");
    assert_eq!(snapshot["project_schema_version"], "mm-project-v1");
    let canonical = serde_json::to_vec(&snapshot).unwrap_or_default();
    let digest = blake3::hash(&canonical).to_hex().to_string();
    assert_eq!(
        digest, "89b285c0b3c19c5c9f55933c3072a694a575dd948cf9888f626ca658c37746f4",
        "the tool contract changed; bump MCP_TOOL_SCHEMA_VERSION if it is incompatible"
    );
    let text = String::from_utf8_lossy(&canonical).to_ascii_lowercase();
    assert!(!text.contains(SENTINEL));
    // The one execution tool takes an exact argv; nothing takes a shell string.
    assert!(!names.iter().any(|name| name.contains("shell")));
    let exec = tools
        .iter()
        .find(|tool| tool["name"] == "mm_exec")
        .map(|tool| tool["inputSchema"].to_string())
        .unwrap_or_default();
    assert!(exec.contains("argv") && !exec.contains("\"tty\"") && !exec.contains("stdin"));
}

#[tokio::test]
async fn sdk_transport_handshake_resources_validation_and_disconnect_are_live() {
    let (server, _use_cases, _audit) = fixture(1_000, 64 * 1024);
    let (client_transport, server_transport) = tokio::io::duplex(256 * 1024);
    let server_task = tokio::spawn(async move {
        let Ok(service) = server.serve(server_transport).await else {
            return false;
        };
        service.waiting().await.is_ok()
    });
    let client_result = ().serve(client_transport).await;
    assert!(client_result.is_ok());
    let Ok(client) = client_result else {
        return;
    };
    let resource = client
        .read_resource(ReadResourceRequestParams::new("mm://context"))
        .await;
    assert!(resource.is_ok());
    let resource_text = resource
        .as_ref()
        .ok()
        .and_then(|value| serde_json::to_string(value).ok())
        .unwrap_or_default();
    assert!(resource_text.contains("mm-mcp-tools-v1"));
    assert!(resource_text.contains("stdio"));

    let arguments = serde_json::json!({"scope": scope(), "kind": "project", "unknown": true})
        .as_object()
        .cloned()
        .unwrap_or_default();
    let malformed = client
        .call_tool(CallToolRequestParams::new("mm_read").with_arguments(arguments))
        .await;
    assert!(
        malformed
            .as_ref()
            .is_ok_and(|result| result.is_error == Some(true))
    );
    assert!(client.cancel().await.is_ok());
    let disconnected = tokio::time::timeout(Duration::from_secs(1), server_task).await;
    assert!(disconnected.is_ok_and(|result| result.is_ok_and(|closed| closed)));
}
