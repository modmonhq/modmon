use std::sync::Arc;

use tempfile::TempDir;

use super::*;
use crate::app::OnboardingService;
use crate::domain::agents::AgentAccessConfig;
use crate::domain::host_policy::AccessLevel;
use crate::domain::mcp::McpScope;
use crate::domain::onboarding::{ConfigMutationAction, ConfigMutationRequest, ConfigResourceKind};
use crate::domain::target::{TargetBootstrap, TargetDefinition, TargetId, TargetTransport};

/// A fresh project (`local` with deploy and exec, everything else view),
/// with a `production` environment on a remote target.
struct Fixture {
    project: TempDir,
    home: TempDir,
    adapter: McpApplicationAdapter,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_limits(McpServerConfig {
            principal: "parity-agent".to_owned(),
            maximum_page_size: 10,
            maximum_response_bytes: 64 * 1024,
            request_timeout_ms: 1_000,
            ..McpServerConfig::default()
        })
    }

    fn with_limits(config: McpServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let project = crate::test_support::tempdir()?;
        std::fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
        ConfigLoader.initialize(project.path(), Some("mcp-parity"))?;
        let home = crate::test_support::tempdir()?;
        // Claim the home before anything else is written into it.
        MmHome::resolve(Some(home.path()))?;
        let fixture = Self {
            adapter: McpApplicationAdapter::new(
                config,
                home.path().to_path_buf(),
                std::fs::canonicalize(project.path())?,
            ),
            project,
            home,
        };
        fixture.edit(|config| {
            let mut production = config["environments"]["local"].clone();
            production["target"] = "prod".into();
            config["environments"]["production"] = production;
        })?;
        let targets = FilesystemTargetStore::new(fixture.home.path().join("targets.yaml"));
        targets.insert_target(
            "local",
            TargetDefinition::local(TargetId(uuid::Uuid::from_u128(71))),
        )?;
        targets.insert_target(
            "prod",
            TargetDefinition {
                id: TargetId(uuid::Uuid::from_u128(72)),
                transport: TargetTransport::Ssh {
                    host: "example.test".to_owned(),
                    port: 22,
                    user: "relay".to_owned(),
                    host_key_fingerprint: "SHA256:kmYcvdi2GkPeWxB6XLjrZB8JHsy2Hm8luHMFp9GMvqk"
                        .to_owned(),
                    host_key: None,
                    identity_file: None,
                    remote_root: None,
                    daemon_fingerprint: None,
                },
                bootstrap: TargetBootstrap::Ansible,
                ingress: None,
                default_runtime: Some(RuntimeKind::DockerCompose),
                available_runtimes: vec![RuntimeKind::DockerCompose],
                runtime: None,
                nodes: Vec::new(),
            },
        )?;
        fixture.trust()?;
        Ok(fixture)
    }

    fn config_path(&self) -> PathBuf {
        self.project.path().join(".config/mm.yaml")
    }

    /// Rewrites the project configuration, as an agent with file access can.
    fn edit(
        &self,
        change: impl FnOnce(&mut serde_yaml_ng::Value),
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut config: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&std::fs::read_to_string(self.config_path())?)?;
        change(&mut config);
        std::fs::write(self.config_path(), serde_yaml_ng::to_string(&config)?)?;
        Ok(())
    }

    /// What `mm agent trust` does.
    fn trust(&self) -> Result<(), Box<dyn std::error::Error>> {
        let document = ConfigLoader.load_from(self.project.path())?;
        AgentTrustStore::new(self.home.path()).trust(
            &document.root,
            document.config.project.id,
            &document.config.agents,
            1,
        )?;
        Ok(())
    }

    async fn authorize(
        &self,
        environment: &str,
        action: AgentAction,
        service: Option<&str>,
    ) -> Result<McpRequestContext, McpAuthorizationError> {
        self.adapter
            .authorize(McpAuthorizationRequest {
                scope: McpScope {
                    environment: environment.to_owned(),
                    target: None,
                    runtime: None,
                },
                action,
                service: service.map(str::to_owned),
            })
            .await
    }

    async fn context(&self, action: AgentAction) -> Result<McpRequestContext, String> {
        self.authorize("local", action, Some("api"))
            .await
            .map_err(|error| format!("{error:?}"))
    }
}

fn denial(result: Result<McpRequestContext, McpAuthorizationError>) -> Option<McpDenialReason> {
    match result {
        Err(McpAuthorizationError::Denied(denial)) => Some(denial.reason),
        _ => None,
    }
}

fn module_mutation() -> ConfigMutationRequest {
    ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::Module,
        name: "api".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({"path": "api", "services": {}})),
    }
}

#[tokio::test]
async fn the_agents_block_decides_per_environment() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    // `mm init` grants deploy and exec in the development environment…
    let local = fixture
        .authorize("local", AgentAction::Exec, Some("api"))
        .await
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(local.access.summary(), "deploy +exec");
    assert_eq!(local.scope.target, "local");
    assert!(!local.remote);
    assert_eq!(local.principal, "parity-agent");
    for action in [AgentAction::DeployApply, AgentAction::ConfigApply] {
        assert!(fixture.authorize("local", action, None).await.is_ok());
    }
    // …and only view everywhere else.
    for (action, service) in [
        (AgentAction::Exec, Some("api")),
        (AgentAction::DeployApply, None),
        (AgentAction::JobRun, None),
    ] {
        assert_eq!(
            denial(fixture.authorize("production", action, service).await),
            Some(McpDenialReason::NotGranted),
            "{action:?}"
        );
    }
    // An environment the project does not have: mm's own refusal.
    assert!(matches!(
        fixture.authorize("qa", AgentAction::Read, None).await,
        Err(McpAuthorizationError::Failed(error))
            if error.code() == "MM_ENVIRONMENT" && error.refusal().is_some()
    ));

    let overview = fixture.adapter.access().await?;
    assert!(overview.policy_trusted);
    let summary = overview
        .environments
        .iter()
        .map(|entry| (entry.environment.as_str(), entry.access.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(summary, [("local", "deploy +exec"), ("production", "view")]);
    Ok(())
}

#[tokio::test]
async fn editing_the_block_grants_nothing_until_the_user_trusts_it()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    // An agent widens its own access in the checkout.
    fixture.edit(|config| {
        config["agents"]["production"] =
            serde_yaml_ng::from_str("{level: deploy, exec: true}").unwrap_or_default();
    })?;
    let overview = fixture.adapter.access().await?;
    assert!(!overview.policy_trusted);
    assert!(
        overview
            .environments
            .iter()
            .all(|entry| entry.access == "view")
    );
    assert!(overview.outside_the_grant.contains("mm agent trust"));
    // What the edit asked for is refused as untrusted, and so is everything
    // the trusted block used to grant.
    for environment in ["production", "local"] {
        assert_eq!(
            denial(
                fixture
                    .authorize(environment, AgentAction::Exec, Some("api"))
                    .await
            ),
            Some(McpDenialReason::PolicyNotTrusted),
            "{environment}"
        );
    }
    // Reads still work, with view.
    let read = fixture
        .authorize("local", AgentAction::Read, None)
        .await
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(read.access, AgentAccess::VIEW);

    // A local override cannot carry the block either.
    let overrides = fixture.project.path().join(".config/mm.overrides.yaml");
    std::fs::write(&overrides, "agents:\n  production: deploy\n")?;
    assert!(fixture.adapter.access().await.is_err());
    std::fs::remove_file(&overrides)?;

    // Once the user trusts the new block, it applies.
    fixture.trust()?;
    std::fs::create_dir_all(fixture.home.path().join("identities/ssh"))?;
    std::fs::write(
        fixture.home.path().join("identities/ssh/agent_ed25519"),
        "key",
    )?;
    assert!(
        fixture
            .authorize("production", AgentAction::DeployApply, None)
            .await
            .is_ok()
    );
    Ok(())
}

#[tokio::test]
async fn a_remote_environment_needs_the_agent_key() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let first = fixture
        .authorize("production", AgentAction::Read, None)
        .await;
    assert!(
        matches!(&first, Err(McpAuthorizationError::Denied(denial)) if denial.reason == McpDenialReason::AgentKeyMissing),
        "{first:?}"
    );
    // The user's own key and MM_SSH_KEY are never a substitute.
    std::fs::create_dir_all(fixture.home.path().join("identities/ssh"))?;
    std::fs::write(fixture.home.path().join("identities/ssh/id_ed25519"), "key")?;
    assert_eq!(
        denial(
            fixture
                .authorize("production", AgentAction::Read, None)
                .await
        ),
        Some(McpDenialReason::AgentKeyMissing)
    );
    std::fs::write(
        fixture.home.path().join("identities/ssh/agent_ed25519"),
        "key",
    )?;
    let context = fixture
        .authorize("production", AgentAction::Read, None)
        .await
        .map_err(|error| format!("{error:?}"))?;
    assert!(context.remote);
    // The fixture's remote target cannot run the project's runtime, so its
    // selection does not resolve. The environment still keeps its own
    // target; it never falls back to the project default.
    assert_eq!(context.scope.target, "prod");

    // A target that exists only in the repo is remote too.
    fixture.edit(|config| {
        config["environments"]["production"]["target"] = "declared".into();
        config["targets"]["declared"] = serde_yaml_ng::from_str(
            "{host: example.test, host-key: \"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w\", daemon-fingerprint: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}",
        )
        .unwrap_or_default();
    })?;
    std::fs::remove_file(fixture.home.path().join("identities/ssh/agent_ed25519"))?;
    let declared = fixture
        .authorize("production", AgentAction::Read, None)
        .await;
    assert!(
        matches!(&declared, Err(McpAuthorizationError::Denied(denial)) if denial.reason == McpDenialReason::AgentKeyMissing),
        "{declared:?}"
    );
    Ok(())
}

#[tokio::test]
async fn private_limits_narrow_the_project_grant() -> Result<(), Box<dyn std::error::Error>> {
    let mut limits = McpServerConfig::default();
    limits.ceiling.insert(
        "*".to_owned(),
        AgentAccessConfig::Level(AccessLevel::Operate),
    );
    let fixture = Fixture::with_limits(limits)?;
    let context = fixture.context(AgentAction::JobRun).await?;
    assert_eq!(context.access.summary(), "operate");
    assert_eq!(
        denial(
            fixture
                .authorize("local", AgentAction::Exec, Some("api"))
                .await
        ),
        Some(McpDenialReason::NotGranted)
    );
    Ok(())
}

#[tokio::test]
async fn mcp_config_uses_the_same_reviewed_use_case() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let context = fixture.context(AgentAction::ConfigApply).await?;
    let direct = OnboardingService::new(Arc::new(ConfigLoader))
        .plan_mutation(&context.project_root, module_mutation())?;
    let mcp = fixture
        .adapter
        .plan_config(&context, module_mutation())
        .await?;
    assert_eq!(mcp, direct);
    fixture
        .adapter
        .apply_config(&context, module_mutation(), &mcp.plan_hash)
        .await?;

    // The users file is refused at every level.
    let user = ConfigMutationRequest {
        action: ConfigMutationAction::Add,
        resource: ConfigResourceKind::User,
        name: "agent".to_owned(),
        parent: None,
        definition: Some(serde_json::json!({"keys": [], "access": {"*": "admin"}})),
    };
    assert!(
        fixture
            .adapter
            .plan_config(&context, user.clone())
            .await
            .is_err()
    );
    assert!(
        fixture
            .adapter
            .apply_config(&context, user, &mcp.plan_hash)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn bounded_reads_and_logs_reach_the_application() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let context = fixture.context(AgentAction::Read).await?;
    for kind in [
        McpReadKind::Project,
        McpReadKind::Environments,
        McpReadKind::Targets,
        McpReadKind::Jobs,
        McpReadKind::Schedules,
        McpReadKind::Gates,
    ] {
        let value = fixture.adapter.read(&context, kind, None, 10).await?;
        assert!(value.is_object(), "{kind:?}");
    }
    for kind in [
        McpReadKind::Plans,
        McpReadKind::Services,
        McpReadKind::Routes,
        McpReadKind::Health,
        McpReadKind::Names,
        McpReadKind::Operations,
    ] {
        let _runtime_dependent = fixture.adapter.read(&context, kind, None, 10).await;
    }
    let log = |tail| McpLogRequest {
        scope: McpScope {
            environment: "local".to_owned(),
            target: None,
            runtime: None,
        },
        stack: Some("app".to_owned()),
        service: "api".to_owned(),
        tail,
        since_unix_ms: None,
        until_unix_ms: None,
    };
    assert!(
        fixture
            .adapter
            .logs(&context, log(0), Default::default())
            .await
            .is_err()
    );
    let _runtime_dependent = fixture
        .adapter
        .logs(&context, log(1), Default::default())
        .await;
    let _build_plan = fixture.adapter.plan_build(&context, false).await;
    let _deployment_plan = fixture
        .adapter
        .plan_deployment(
            &context,
            (
                crate::domain::mcp::McpDeployPlanKind::Checkout,
                &Default::default(),
            ),
        )
        .await;
    Ok(())
}

#[tokio::test]
async fn job_actions_plan_and_refuse_stale_applies() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let context = fixture.context(AgentAction::ScheduleManage).await?;
    for action in [
        McpJobAction::Enqueue {
            name: "job".to_owned(),
        },
        McpJobAction::RunSchedule {
            name: "schedule".to_owned(),
        },
        McpJobAction::Cancel {
            run_id: uuid::Uuid::from_u128(1),
        },
        McpJobAction::Retry {
            run_id: uuid::Uuid::from_u128(2),
        },
        McpJobAction::SetScheduleEnabled {
            name: "schedule".to_owned(),
            enabled: true,
        },
    ] {
        assert!(
            fixture
                .adapter
                .plan_job(&context, action.clone())
                .await?
                .is_object()
        );
        assert!(
            fixture
                .adapter
                .apply_job(&context, action, "stale-plan")
                .await
                .is_err()
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_use_case_checks_the_grant_again() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    // A context that was authorized for view, however it got here.
    let mut view = fixture.context(AgentAction::Read).await?;
    view.access = AgentAccess::VIEW;
    let adapter = &fixture.adapter;
    assert!(
        adapter
            .apply_config(&view, module_mutation(), "plan")
            .await
            .is_err()
    );
    assert!(adapter.apply_build(&view, false, "plan").await.is_err());
    assert!(
        adapter
            .apply_deployment(&view, "plan", &Default::default())
            .await
            .is_err()
    );
    assert!(
        adapter
            .apply_job(&view, McpJobAction::Reconcile, "plan")
            .await
            .is_err()
    );
    assert!(
        adapter
            .apply_job(
                &view,
                McpJobAction::Enqueue {
                    name: "job".to_owned()
                },
                "plan"
            )
            .await
            .is_err()
    );
    let exec = McpExecRequest {
        scope: McpScope {
            environment: "local".to_owned(),
            target: None,
            runtime: None,
        },
        service: "api".to_owned(),
        stack: None,
        argv: vec!["ls".to_owned()],
        workdir: None,
        user: None,
        timeout_seconds: 5,
    };
    let refused = adapter.exec(&view, exec.clone(), Default::default()).await;
    assert!(matches!(refused, Err(MmError::Security { .. })));

    // With the grant, exec validates its bounds before it reaches a runtime.
    let granted = fixture.context(AgentAction::Exec).await?;
    for invalid in [
        McpExecRequest {
            argv: Vec::new(),
            ..exec.clone()
        },
        McpExecRequest {
            argv: vec!["x".to_owned(); MCP_MAX_EXEC_ARGUMENTS + 1],
            ..exec.clone()
        },
        McpExecRequest {
            timeout_seconds: MCP_MAX_EXEC_SECONDS + 1,
            ..exec.clone()
        },
    ] {
        assert!(matches!(
            adapter.exec(&granted, invalid, Default::default()).await,
            Err(error) if error.code() == "MM_ENVIRONMENT" && error.refusal().is_some()
        ));
    }
    Ok(())
}

#[tokio::test]
async fn a_forged_source_fails_every_check() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let adapter = &fixture.adapter;
    let mut forged = fixture.context(AgentAction::Exec).await?;
    forged.source = "forged-source".to_owned();
    assert!(
        adapter
            .read(&forged, McpReadKind::Project, None, 1)
            .await
            .is_err()
    );
    assert!(
        adapter
            .plan_deployment(
                &forged,
                (
                    crate::domain::mcp::McpDeployPlanKind::Checkout,
                    &Default::default()
                ),
            )
            .await
            .is_err()
    );
    assert!(adapter.plan_build(&forged, false).await.is_err());
    assert!(
        adapter
            .plan_job(&forged, McpJobAction::Reconcile)
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn pagination_and_cursor_helpers_reject_invalid_bounds() -> Result<(), MmError> {
    assert_eq!(decode_cursor(None)?, 0);
    assert_eq!(decode_cursor(Some("2"))?, 2);
    assert!(decode_cursor(Some("bad")).is_err());
    let page = page_value(vec![serde_json::json!(1), serde_json::json!(2)], 0, 1)?;
    assert_eq!(page["next_cursor"], "1");
    assert!(page_value(Vec::new(), 0, 0).is_err());
    assert!(page_value(Vec::new(), 1, 1).is_err());
    Ok(())
}

#[test]
fn the_server_starts_from_a_project_directory_with_optional_limits()
-> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::new()?;
    let home = fixture.home.path();
    let root = fixture.project.path();
    let composed = services(McpServerConfig::default(), root, Some(home))?;
    assert!(Arc::strong_count(&composed.use_cases) >= 1);
    assert!(Arc::strong_count(&composed.audit) >= 1);

    // No private limits file is needed.
    let _server = server(None, Some(home), Some(root))?;
    // A named file must exist and be private.
    let policy = home.join("policy.json");
    assert!(server(Some(&policy), Some(home), Some(root)).is_err());
    std::fs::write(&policy, serde_json::to_vec(&McpServerConfig::default())?)?;
    #[cfg(unix)]
    std::fs::set_permissions(&policy, std::fs::Permissions::from_mode(0o600))?;
    let _server = server(Some(&policy), Some(home), Some(root))?;
    assert!(server(None, Some(home), Some(&root.join("missing"))).is_err());
    Ok(())
}
