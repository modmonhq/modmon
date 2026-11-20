    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use async_trait::async_trait;
    use tokio::net::UnixStream;

    use super::{
        DaemonConfig, RemoteDaemon, add_stream_bytes, bind_principal, bind_socket, is_cancel,
        method_capability, timed, validate_relay_attribution,
    };
    use crate::adapters::remote::codec::{FrameReader, read_frame, write_frame};
    use crate::app::ports::{ExecStream, LogStream, RemoteRequestContext, RemoteUseCases};
    use crate::domain::build::BuildReceipt;
    use crate::domain::deployment::{
        DEPLOYMENT_PLAN_SCHEMA, DeploymentPlan, DeploymentResult, DeploymentRouteDiff,
    };
    use crate::domain::error::MmError;
    use crate::domain::observability::{
        ApprovalContext, ExecEvent, ExecOutputSource, ExecRequest, ExecStdinMode, ExecTtyMode,
        LogLimits, LogMode, LogOutputPolicy, LogRecord, LogRequest, LogSource,
        OperationCancellation, ServiceSelector,
    };
    use crate::domain::remote::{
        ClientFrame, ClientHello, MINIMUM_PROTOCOL_VERSION, MutationState, PROTOCOL_VERSION,
        RemoteMethod, RemoteResult,
        RemoteScope, RemoteStreamItem, ServerFrame,
    };
    use crate::domain::runtime::{RuntimeKind, ServiceIdentity};
    use crate::domain::target::TargetId;
    use crate::domain::traefik_dynamic::{
        PreparedTraefikDynamicConfig, TraefikDynamicConfigManifest,
    };
    use crate::domain::workspace::{ProjectId, WorkspaceSnapshot};

    pub(super) struct FakeUseCases {
        pub(super) cancelled: Arc<AtomicBool>,
    }

    #[async_trait]
    impl RemoteUseCases for FakeUseCases {
        async fn mesh_status(
            &self,
            _context: &RemoteRequestContext,
        ) -> Result<crate::domain::mesh::MeshNodeStatus, MmError> {
            Ok(crate::domain::mesh::MeshNodeStatus {
                interface: Some("mm0".to_owned()),
                up: true,
                port: Some(51820),
                ..crate::domain::mesh::MeshNodeStatus::default()
            })
        }

        async fn status(
            &self,
            context: &RemoteRequestContext,
        ) -> Result<WorkspaceSnapshot, MmError> {
            if context.request_id == uuid::Uuid::from_u128(700) {
                return Err(unused());
            }
            Ok(workspace_snapshot(context))
        }

        async fn logs(
            &self,
            context: &RemoteRequestContext,
            _request: LogRequest,
            cancellation: OperationCancellation,
        ) -> Result<LogStream, MmError> {
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            let project_id = context.scope.project_id;
            let cancelled = Arc::clone(&self.cancelled);
            let task_cancellation = cancellation.clone();
            tokio::spawn(async move {
                let _ = sender
                    .send(Ok(LogRecord {
                        identity: ServiceIdentity {
                            project_id,
                            environment: "production".to_owned(),
                            stack: "app".to_owned(),
                            module: Some("api".to_owned()),
                            service: "web".to_owned(),
                        },
                        sequence: 1,
                        source: LogSource::Stdout,
                        node: None,
                        container: None,
                        timestamp: None,
                        message: "bounded".to_owned(),
                        lossy_utf8: false,
                        truncated: false,
                        reconnect: 0,
                    }))
                    .await;
                while !task_cancellation.is_cancelled() {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                cancelled.store(true, Ordering::SeqCst);
            });
            Ok(LogStream::new(receiver, cancellation))
        }

        async fn exec(
            &self,
            context: &RemoteRequestContext,
            _request: ExecRequest,
            cancellation: OperationCancellation,
        ) -> Result<ExecStream, MmError> {
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            sender
                .send(Ok(ExecEvent::Output {
                    source: ExecOutputSource::Stdout,
                    value: format!("{}-exec", context.principal),
                    lossy_utf8: false,
                }))
                .await
                .map_err(|_| unused())?;
            drop(sender);
            Ok(ExecStream::new(receiver, cancellation))
        }

        async fn plan_deployment(
            &self,
            context: &RemoteRequestContext,
            receipt: Option<&BuildReceipt>,
        ) -> Result<DeploymentPlan, MmError> {
            let mut plan = deployment_plan(context)?;
            plan.build_receipt = receipt.map(BuildReceipt::origin);
            // On app-2 the engine finds a service that moved to app-1.
            if context.scope.node.as_deref() == Some("app-2") {
                plan.node = Some(crate::domain::deployment::NodeProjectPlan {
                    node: "app-2".to_owned(),
                    services: Vec::new(),
                    starting: Vec::new(),
                    leaving: vec![crate::domain::deployment::LeavingService {
                        stack: "app".to_owned(),
                        service: "worker".to_owned(),
                        compose_project_name: "example-production-app".to_owned(),
                        moves_to: vec!["app-1".to_owned()],
                    }],
                    dropped_dependencies: Vec::new(),
                    networks: Vec::new(),
                    restart_defaults: Vec::new(),
                    shared_volumes: Vec::new(),
                    ingress: None,
                    scaled: Vec::new(),
                    health: Vec::new(),
                });
            }
            plan.plan_hash = plan
                .canonical_hash()
                .map_err(|message| MmError::Security { message })?;
            Ok(plan)
        }

        async fn finish_moves(
            &self,
            context: &RemoteRequestContext,
            _deployment_plan_hash: &str,
        ) -> Result<Vec<crate::domain::placement::PlacedService>, MmError> {
            assert!(
                context.project_root.is_absolute(),
                "the step runs in the checkout of the applied commit"
            );
            Ok(vec![crate::domain::placement::PlacedService {
                stack: "app".to_owned(),
                service: "worker".to_owned(),
            }])
        }

        async fn begin_rollout(
            &self,
            context: &RemoteRequestContext,
            _deployment_plan_hash: &str,
            _receipt: Option<&BuildReceipt>,
        ) -> Result<(), MmError> {
            assert!(
                context.project_root.is_absolute(),
                "a rollout runs in the checkout of the planned commit"
            );
            Ok(())
        }

        /// The stack named `sick` never becomes healthy.
        async fn roll_stack(
            &self,
            _context: &RemoteRequestContext,
            _deployment_plan_hash: &str,
            stack: &str,
            _step: crate::app::StackStep,
        ) -> Result<crate::app::StackOutcome, MmError> {
            if stack == "sick" {
                return Ok(crate::app::StackOutcome::Failed(
                    crate::domain::rollout::StepFailure {
                        stack: stack.to_owned(),
                        service: Some("web".to_owned()),
                        reason: "its health check fails".to_owned(),
                        log: vec!["cannot reach the database".to_owned()],
                        code: None,
                    },
                ));
            }
            Ok(crate::app::StackOutcome::Done {
                started: Vec::new(),
                addresses: vec![std::net::Ipv4Addr::new(10, 64, 0, 2)],
            })
        }

        async fn finish_rollout(
            &self,
            context: &RemoteRequestContext,
            deployment_plan_hash: &str,
        ) -> Result<DeploymentResult, MmError> {
            self.apply_deployment(context, deployment_plan_hash, None).await
        }

        async fn rollback_rollout(
            &self,
            _context: &RemoteRequestContext,
            _deployment_plan_hash: &str,
            _changes: Option<&crate::domain::rollout::RolloutChanges>,
        ) -> Result<bool, MmError> {
            Ok(true)
        }

        async fn apply_deployment(
            &self,
            context: &RemoteRequestContext,
            deployment_plan_hash: &str,
            receipt: Option<&BuildReceipt>,
        ) -> Result<DeploymentResult, MmError> {
            // Reports the receipt it was handed in place of a stack name.
            let stack = receipt.map_or_else(
                || "app".to_owned(),
                |receipt| format!("receipt:{}", receipt.receipt_hash),
            );
            Ok(DeploymentResult {
                plan_hash: deployment_plan_hash.to_owned(),
                target_name: context.scope.target.clone(),
                target_id: TargetId(uuid::Uuid::from_u128(94)),
                environment: context.scope.environment.clone(),
                applied_stacks: vec![stack],
                removed_stacks: Vec::new(),
                route_generation: 0,
                route_count: 0,
                moved_away: Vec::new(),
                jobs: Vec::new(),
            })
        }
    }

    async fn call(
        daemon: &Arc<RemoteDaemon>,
        id: u128,
        method: RemoteMethod,
    ) -> Result<ServerFrame, Box<dyn std::error::Error>> {
        call_in(daemon, id, remote_scope(std::path::PathBuf::new()), method).await
    }

    /// One request in `scope`, and the first frame the daemon answers with.
    pub(super) async fn call_in(
        daemon: &Arc<RemoteDaemon>,
        id: u128,
        scope: RemoteScope,
        method: RemoteMethod,
    ) -> Result<ServerFrame, Box<dyn std::error::Error>> {
        let (task, mut client) = connected_client(daemon).await?;
        write_frame(
            &mut client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id: uuid::Uuid::from_u128(id),
                operation_nonce: uuid::Uuid::from_u128(id + 1),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope,
                method: Box::new(method),
            },
        )
        .await?;
        let frame = read_frame::<_, ServerFrame>(&mut client)
            .await?
            .ok_or("daemon closed without a response")?;
        task.await??;
        Ok(frame)
    }

    #[tokio::test]
    async fn build_receipts_must_match_the_commit_and_travel_with_the_plan_to_apply()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let config = config(temporary.path(), &root, uid, gid)?;
        let audit_path = config.state_root.join("audit.jsonl");
        let config_audit = audit_path.clone();
        let daemon = Arc::new(RemoteDaemon::new(config, handler)?);
        let commit = origin(temporary.path())?.1;
        let project = ProjectId(uuid::Uuid::from_u128(4));
        let plan = |receipt: BuildReceipt| RemoteMethod::PlanDeployment {
            commit: commit.clone(),
            receipt: Some(Box::new(receipt)),
            release: Default::default(),
        };

        // Another commit's or project's build is refused before planning.
        let other = ProjectId(uuid::Uuid::from_u128(5));
        for (id, wrong) in [
            (400, crate::test_support::build_receipt(project, "production", &"f".repeat(40))?),
            (402, crate::test_support::build_receipt(other, "production", &commit)?),
        ] {
            let frame = call(&daemon, id, plan(wrong)).await?;
            assert!(
                matches!(&frame, ServerFrame::Error { error, .. }
                    if error.code == "MM_SECURITY" && error.mutation_state == MutationState::NotStarted),
                "{frame:?}"
            );
        }

        let receipt = crate::test_support::build_receipt(project, "production", &commit)?;
        let ServerFrame::Result { result, .. } = call(&daemon, 410, plan(receipt.clone())).await?
        else {
            return Err("plan with a receipt failed".into());
        };
        let RemoteResult::DeploymentPlan(reviewed) = *result else {
            return Err("plan returned another result".into());
        };
        assert!(reviewed.validate().is_ok());
        assert_eq!(reviewed.receipt.as_ref(), Some(&receipt));

        // The stored plan is only for a deployment apply, and a refused
        // attempt does not use it up.
        let frame = call(&daemon, 415, RemoteMethod::ApplyTeamSync {
            remote_plan_hash: reviewed.binding.plan_hash.clone(),
        })
        .await?;
        assert!(
            matches!(&frame, ServerFrame::Error { error, .. } if error.message.contains("different operation")),
            "{frame:?}"
        );

        let ServerFrame::Result { result, .. } = call(&daemon, 420, RemoteMethod::ApplyDeployment {
            remote_plan_hash: reviewed.binding.plan_hash.clone(),
        })
        .await?
        else {
            return Err("apply failed".into());
        };
        let RemoteResult::DeploymentApplied(applied) = *result else {
            return Err("apply returned another result".into());
        };
        assert_eq!(applied.applied_stacks, [format!("receipt:{}", receipt.receipt_hash)]);

        let audit = std::fs::read_to_string(audit_path)?;
        let records = audit
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<Result<Vec<_>, _>>()?;
        assert!(records.iter().any(|record| record["result"] == "planned"
            && record["build_receipt"] == receipt.receipt_hash.as_str()
            && record["build_receipt_environment"] == "production"
            && record["images"][0] == format!("registry.example/api@sha256:{}", "d".repeat(64))));

        // A build for another environment reaches the use case, which decides
        // whether this environment builds the same units; the plan and audit
        // name the environment it was built for.
        let ci = crate::test_support::build_receipt(project, "ci", &commit)?;
        let ServerFrame::Result { result, .. } = call(&daemon, 430, plan(ci.clone())).await? else {
            return Err("plan with a promoted receipt failed".into());
        };
        let RemoteResult::DeploymentPlan(promoted) = *result else {
            return Err("plan returned another result".into());
        };
        assert!(promoted.validate().is_ok());
        let promoted_plan = promoted.plan.as_ref().ok_or("a single host always has a plan")?;
        assert_eq!(promoted_plan.build_receipt.as_ref().map(|origin| origin.environment.as_str()), Some("ci"));
        let audit = std::fs::read_to_string(config_audit)?;
        assert!(audit.lines().any(|line| line.contains("\"build_receipt_environment\":\"ci\"")));

        // A plan that does not deploy the receipt it carries is invalid.
        let mut tampered = (*promoted).clone();
        if let Some(plan) = &mut tampered.plan {
            plan.build_receipt = None;
        }
        assert!(tampered.validate().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn incompatible_version_returns_structured_upgrade_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let daemon = Arc::new(RemoteDaemon::new(
            config(temporary.path(), &root, uid, gid)?,
            handler,
        )?);
        for (version, expected) in [
            (PROTOCOL_VERSION + 1, "Upgrade the host: run `mm target setup plan --upgrade`"),
            (MINIMUM_PROTOCOL_VERSION - 1, "Update mm on this machine"),
        ] {
            let (server, mut client) = unix_pair()?;
            let task = tokio::spawn({
                let daemon = Arc::clone(&daemon);
                async move { daemon.serve_stream(server).await }
            });
            send_session(&mut client).await?;
            write_frame(
                &mut client,
                &ClientFrame::Hello {
                    version,
                    minimum: version,
                    hello: ClientHello {
                        client_nonce: uuid::Uuid::from_u128(20),
                        expected_daemon_fingerprint: daemon.identity().fingerprint.clone(),
                        client_version: String::new(),
                    },
                },
            )
            .await?;
            let frame = read_frame::<_, ServerFrame>(&mut client).await?;
            assert!(
                matches!(&frame, Some(ServerFrame::Error { error, .. })
                    if error.code == "MM_REMOTE_UPGRADE_REQUIRED"
                        && error.message.contains(expected)
                        && error.message.contains(&format!("protocol {version}"))),
                "{frame:?}"
            );
            assert!(task.await?.is_err());
        }
        Ok(())
    }

    #[tokio::test]
    async fn unauthorized_method_returns_a_correlated_structured_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let daemon = Arc::new(RemoteDaemon::new(
            config(temporary.path(), &root, uid, gid)?,
            handler,
        )?);
        let fingerprint = daemon.identity().fingerprint.clone();
        let (server, mut client) = unix_pair()?;
        let task = tokio::spawn({
            let daemon = Arc::clone(&daemon);
            async move { daemon.serve_stream(server).await }
        });
        send_session(&mut client).await?;
        let hello_nonce = uuid::Uuid::from_u128(30);
        write_frame(
            &mut client,
            &ClientFrame::Hello {
                version: PROTOCOL_VERSION,
                minimum: PROTOCOL_VERSION,
                hello: ClientHello {
                    client_nonce: hello_nonce,
                    expected_daemon_fingerprint: fingerprint,
                    client_version: String::new(),
                },
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut client).await?,
            Some(ServerFrame::Hello { hello, .. }) if hello.client_nonce == hello_nonce
        ));
        let request_id = uuid::Uuid::from_u128(31);
        write_frame(
            &mut client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id,
                operation_nonce: uuid::Uuid::from_u128(32),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope: RemoteScope {
                    project_id: ProjectId(uuid::Uuid::from_u128(33)),
                    environment: "production".to_owned(),
                    target: "production".to_owned(),
                    target_fingerprint: "a".repeat(64),
                    runtime: RuntimeKind::DockerCompose,
                    node: None,
                },
                method: Box::new(RemoteMethod::Status),
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut client).await?,
            Some(ServerFrame::Error {
                request_id: Some(received),
                error,
                ..
            }) if received == request_id
                && error.code == "MM_SECURITY"
                && error.mutation_state == MutationState::NotStarted
        ));
        task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn reviewed_apply_rechecks_plan_and_apply_authorizations_independently()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let root = std::fs::canonicalize(root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let daemon = Arc::new(RemoteDaemon::new(
            config(temporary.path(), &root, uid, gid)?,
            handler,
        )?);

        let (plan_task, mut plan_client) = connected_client(&daemon).await?;
        let plan_request_id = uuid::Uuid::from_u128(40);
        write_frame(
            &mut plan_client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id: plan_request_id,
                operation_nonce: uuid::Uuid::from_u128(41),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope: remote_scope(root.clone()),
                method: Box::new(RemoteMethod::PlanDeployment {
                    commit: origin(temporary.path())?.1,
                    receipt: None,
                    release: Default::default(),
                }),
            },
        )
        .await?;
        let (remote_plan_hash, deployment_plan_hash) =
            match read_frame::<_, ServerFrame>(&mut plan_client).await? {
                Some(ServerFrame::Result {
                    request_id, result, ..
                }) if request_id == plan_request_id => match *result {
                    RemoteResult::DeploymentPlan(plan) => (
                        plan.binding.plan_hash,
                        plan.plan.map(|plan| plan.plan_hash).unwrap_or_default(),
                    ),
                    other => return Err(format!("unexpected plan result: {other:?}").into()),
                },
                other => return Err(format!("unexpected plan frame: {other:?}").into()),
            };
        plan_task.await??;

        let (apply_task, mut apply_client) = connected_client(&daemon).await?;
        let apply_request_id = uuid::Uuid::from_u128(42);
        write_frame(
            &mut apply_client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id: apply_request_id,
                operation_nonce: uuid::Uuid::from_u128(43),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope: remote_scope(root),
                method: Box::new(RemoteMethod::ApplyDeployment {
                    remote_plan_hash: remote_plan_hash.clone(),
                }),
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut apply_client).await?,
            Some(ServerFrame::Result {
                request_id,
                result,
                ..
            }) if request_id == apply_request_id
                && matches!(*result, RemoteResult::DeploymentApplied(ref applied) if applied.plan_hash == deployment_plan_hash)
        ));
        apply_task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn successful_status_exec_and_failed_dispatch_have_correlated_terminal_frames()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let root = std::fs::canonicalize(root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let daemon = Arc::new(RemoteDaemon::new(
            config(temporary.path(), &root, uid, gid)?,
            Arc::new(FakeUseCases {
                cancelled: Arc::new(AtomicBool::new(false)),
            }),
        )?);

        let (status_task, mut status_client) = connected_client(&daemon).await?;
        let status_id = uuid::Uuid::from_u128(701);
        write_frame(
            &mut status_client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id: status_id,
                operation_nonce: uuid::Uuid::from_u128(711),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope: remote_scope(root.clone()),
                method: Box::new(RemoteMethod::Status),
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut status_client).await?,
            Some(ServerFrame::Result { request_id, result, .. })
                if request_id == status_id && matches!(*result, RemoteResult::Status(_))
        ));
        status_task.await??;

        let (exec_task, mut exec_client) = connected_client(&daemon).await?;
        let exec_id = uuid::Uuid::from_u128(702);
        write_frame(
            &mut exec_client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id: exec_id,
                operation_nonce: uuid::Uuid::from_u128(712),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope: remote_scope(root.clone()),
                method: Box::new(RemoteMethod::Exec(exec_request())),
            },
        )
        .await?;
        let started = read_frame::<_, ServerFrame>(&mut exec_client).await?;
        if !matches!(
            started,
            Some(ServerFrame::Result { request_id, ref result, .. })
                if request_id == exec_id && matches!(**result, RemoteResult::StreamStarted)
        ) {
            return Err(format!("unexpected exec start frame: {started:?}").into());
        }
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut exec_client).await?,
            Some(ServerFrame::Stream { request_id, item, .. })
                if request_id == exec_id && matches!(*item, RemoteStreamItem::Exec(_))
        ));
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut exec_client).await?,
            Some(ServerFrame::End { request_id, frames: 1, .. }) if request_id == exec_id
        ));
        exec_task.await??;

        let (failure_task, mut failure_client) = connected_client(&daemon).await?;
        let failure_id = uuid::Uuid::from_u128(700);
        write_frame(
            &mut failure_client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id: failure_id,
                operation_nonce: uuid::Uuid::from_u128(713),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope: remote_scope(root),
                method: Box::new(RemoteMethod::Status),
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut failure_client).await?,
            Some(ServerFrame::Error { request_id: Some(request_id), error, .. })
                if request_id == failure_id && error.mutation_state == MutationState::NotStarted
        ));
        failure_task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn unix_peer_mapping_fails_closed_and_stream_cancel_is_bounded()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let foreign = config(temporary.path(), &root, uid.saturating_add(1), gid)?;
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let daemon = RemoteDaemon::new(foreign, Arc::clone(&handler))?;
        let (server, mut client) = unix_pair()?;
        send_session(&mut client).await?;
        let error = daemon
            .serve_stream(server)
            .await
            .err()
            .ok_or("peer accepted")?;
        assert_eq!(error.code(), "MM_SECURITY");

        let cancelled = Arc::new(AtomicBool::new(false));
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::clone(&cancelled),
        });
        let daemon = Arc::new(RemoteDaemon::new(
            config(temporary.path(), &root, uid, gid)?,
            handler,
        )?);
        let fingerprint = daemon.identity().fingerprint.clone();
        let (server, mut client) = unix_pair()?;
        let task = tokio::spawn({
            let daemon = Arc::clone(&daemon);
            async move { daemon.serve_stream(server).await }
        });
        send_session(&mut client).await?;
        let hello_nonce = uuid::Uuid::from_u128(1);
        write_frame(
            &mut client,
            &ClientFrame::Hello {
                version: PROTOCOL_VERSION,
                minimum: PROTOCOL_VERSION,
                hello: ClientHello {
                    client_nonce: hello_nonce,
                    expected_daemon_fingerprint: fingerprint,
                    client_version: String::new(),
                },
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut client).await?,
            Some(ServerFrame::Hello { hello, .. }) if hello.client_nonce == hello_nonce
        ));
        let request_id = uuid::Uuid::from_u128(2);
        write_frame(
            &mut client,
            &ClientFrame::Request {
                version: PROTOCOL_VERSION,
                request_id,
                operation_nonce: uuid::Uuid::from_u128(3),
                deadline_unix_ms: chrono::Utc::now().timestamp_millis() + 10_000,
                scope: RemoteScope {
                    project_id: ProjectId(uuid::Uuid::from_u128(4)),
                    environment: "production".to_owned(),
                    target: "production".to_owned(),
                    target_fingerprint: "a".repeat(64),
                    runtime: RuntimeKind::DockerCompose,
                    node: None,
                },
                method: Box::new(RemoteMethod::Logs(log_request())),
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut client).await?,
            Some(ServerFrame::Result { request_id: received, .. }) if received == request_id
        ));
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut client).await?,
            Some(ServerFrame::Stream {
                request_id: received,
                item,
                ..
            }) if received == request_id && matches!(*item, RemoteStreamItem::Log(_))
        ));
        write_frame(
            &mut client,
            &ClientFrame::Cancel {
                version: PROTOCOL_VERSION,
                request_id,
            },
        )
        .await?;
        assert!(matches!(
            read_frame::<_, ServerFrame>(&mut client).await?,
            Some(ServerFrame::End { request_id: received, frames: 1, .. }) if received == request_id
        ));
        task.await??;
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !cancelled.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await?;
        let audit =
            std::fs::read_to_string(temporary.path().join(format!("state-{uid}/audit.jsonl")))?;
        assert!(audit.contains(&ops_fingerprint()));
        assert!(audit.contains("principal_id"));
        assert!(audit.contains("capability"));
        assert!(audit.contains("daemon_version"));
        assert!(audit.contains("127.0.0.1 10000 127.0.0.1 22"));
        assert!(!audit.contains("forged"));
        assert!(!audit.contains("bounded"));
        Ok(())
    }

    #[tokio::test]
    async fn session_handshake_and_request_validation_reject_every_protocol_transition()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let root = std::fs::canonicalize(root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let daemon = RemoteDaemon::new(
            config(temporary.path(), &root, uid, gid)?,
            Arc::new(FakeUseCases { cancelled: Arc::new(AtomicBool::new(false)) }),
        )?;
        let fingerprint = daemon.identity().fingerprint.clone();

        assert!(serve_frames(&daemon, vec![ClientFrame::Cancel {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::new_v4(),
        }]).await.is_err());
        assert!(serve_frames(&daemon, vec![ClientFrame::RelaySession {
            version: PROTOCOL_VERSION + 1,
            key_fingerprint: "SHA256:test".to_owned(),
            source: "127.0.0.1 1 127.0.0.1 2".to_owned(),
            session_id: uuid::Uuid::new_v4(),
        }]).await.is_err());
        assert!(serve_frames(&daemon, vec![ClientFrame::RelaySession {
            version: PROTOCOL_VERSION,
            key_fingerprint: "invalid".to_owned(),
            source: "invalid/source".to_owned(),
            session_id: uuid::Uuid::new_v4(),
        }]).await.is_err());

        let session = || ClientFrame::RelaySession {
            version: PROTOCOL_VERSION,
            key_fingerprint: "SHA256:test".to_owned(),
            source: "127.0.0.1 1 127.0.0.1 2".to_owned(),
            session_id: uuid::Uuid::new_v4(),
        };
        let hello = |expected: String| ClientFrame::Hello {
            version: PROTOCOL_VERSION,
            minimum: PROTOCOL_VERSION,
            hello: ClientHello {
                client_nonce: uuid::Uuid::new_v4(),
                expected_daemon_fingerprint: expected,
                client_version: String::new(),
            },
        };
        assert!(serve_frames(&daemon, vec![session(), ClientFrame::Cancel {
            version: PROTOCOL_VERSION,
            request_id: uuid::Uuid::new_v4(),
        }]).await.is_err());
        assert!(serve_frames(&daemon, vec![session(), hello("0".repeat(64))]).await.is_err());
        assert!(serve_frames(&daemon, vec![
            session(),
            hello(fingerprint.clone()),
            ClientFrame::Cancel {
                version: PROTOCOL_VERSION,
                request_id: uuid::Uuid::new_v4(),
            },
        ]).await.is_err());

        let request = |version: u16, deadline_unix_ms: i64, scope: RemoteScope| {
            ClientFrame::Request {
                version,
                request_id: uuid::Uuid::new_v4(),
                operation_nonce: uuid::Uuid::new_v4(),
                deadline_unix_ms,
                scope,
                method: Box::new(RemoteMethod::Status),
            }
        };
        let deadline = chrono::Utc::now().timestamp_millis() + 10_000;
        assert!(serve_frames(&daemon, vec![
            session(),
            hello(fingerprint.clone()),
            request(PROTOCOL_VERSION + 1, deadline, remote_scope(root.clone())),
        ]).await.is_err());
        assert!(serve_frames(&daemon, vec![
            session(),
            hello(fingerprint.clone()),
            request(PROTOCOL_VERSION, 1, remote_scope(root.clone())),
        ]).await.is_ok());
        let mut invalid_scope = remote_scope(root.clone());
        invalid_scope.target_fingerprint = "invalid".to_owned();
        assert!(serve_frames(&daemon, vec![
            session(),
            hello(fingerprint.clone()),
            request(PROTOCOL_VERSION, deadline, invalid_scope),
        ]).await.is_ok());
        assert!(serve_frames(&daemon, vec![
            session(),
            hello(fingerprint),
            request(PROTOCOL_VERSION, deadline, remote_scope(root)),
        ]).await.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn server_support_helpers_cover_socket_identity_capability_and_stream_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt as _;

        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let mut daemon_config = config(temporary.path(), &root, uid, gid)?;

        let listener = bind_socket(&daemon_config, &crate::adapters::kept_sockets::KeptSockets::none())?;
        assert_eq!(
            std::fs::metadata(&daemon_config.socket_path)?
                .permissions()
                .mode()
                & 0o777,
            0o660
        );
        drop(listener);
        let listener = bind_socket(&daemon_config, &crate::adapters::kept_sockets::KeptSockets::none())?;
        drop(listener);
        std::fs::remove_file(&daemon_config.socket_path)?;
        std::fs::write(&daemon_config.socket_path, b"not-a-socket")?;
        assert!(bind_socket(&daemon_config, &crate::adapters::kept_sockets::KeptSockets::none()).is_err());
        std::fs::remove_file(&daemon_config.socket_path)?;

        std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o777))?;
        assert!(bind_socket(&daemon_config, &crate::adapters::kept_sockets::KeptSockets::none()).is_err());
        std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o700))?;
        daemon_config.socket_path = temporary.path().join("missing/daemon.sock");
        assert!(bind_socket(&daemon_config, &crate::adapters::kept_sockets::KeptSockets::none()).is_err());
        daemon_config.socket_path = std::path::PathBuf::from("/");
        assert!(bind_socket(&daemon_config, &crate::adapters::kept_sockets::KeptSockets::none()).is_err());

        validate_relay_attribution(
            "SHA256:AbCdEf0123+/",
            "127.0.0.1 10000 127.0.0.1 22",
        )?;
        for (fingerprint, source) in [
            ("invalid", "source"),
            ("SHA256:test", ""),
            ("SHA256:test", "bad/source"),
        ] {
            assert!(validate_relay_attribution(fingerprint, source).is_err());
        }

        let mut logs = RemoteMethod::Logs(log_request());
        assert_eq!(method_capability(&logs).1, Some("web"));
        bind_principal(&mut logs, "authenticated");
        let RemoteMethod::Logs(logs) = logs else {
            return Err("log method changed variant".into());
        };
        assert_eq!(logs.approval.principal, "authenticated");
        assert_eq!(logs.approval.source, "authenticated-ssh-relay");

        let mut exec = RemoteMethod::Exec(ExecRequest {
            selector: ServiceSelector {
                stack: Some("app".to_owned()),
                module: None,
                service: "worker".to_owned(),
                container: None,
            },
            argv: vec!["true".to_owned()],
            user: None,
            workdir: None,
            environment: Vec::new(),
            tty: ExecTtyMode::Disabled,
            stdin: ExecStdinMode::Closed,
            timeout: std::time::Duration::from_secs(1),
            approval: ApprovalContext {
                principal: "forged".to_owned(),
                source: "forged".to_owned(),
                approved_high_risk: false,
            },
            shell_wrapper: false,
        });
        assert_eq!(method_capability(&exec).1, Some("worker"));
        bind_principal(&mut exec, "authenticated");
        let RemoteMethod::Exec(exec) = exec else {
            return Err("exec method changed variant".into());
        };
        assert_eq!(exec.approval.principal, "authenticated");
        for method in [
            RemoteMethod::Status,
            RemoteMethod::PlanDeployment {
                commit: "a".repeat(40),
                receipt: None,
                release: Default::default(),
            },
            RemoteMethod::WhoAmI,
            RemoteMethod::ApplyDeployment {
                remote_plan_hash: "a".repeat(64),
            },
        ] {
            assert!(method_capability(&method).1.is_none());
        }

        assert_eq!(add_stream_bytes(10, 5, 1)?, 15);
        assert!(add_stream_bytes(u64::MAX, 1, 1).is_err());
        assert!(add_stream_bytes(0, 0, crate::domain::remote::MAX_STREAM_FRAMES + 1).is_err());
        let request_id = uuid::Uuid::from_u128(601);
        let session = (PROTOCOL_VERSION, request_id);
        assert!(is_cancel(None, session)?);
        assert!(is_cancel(
            Some(ClientFrame::Cancel {
                version: PROTOCOL_VERSION,
                request_id,
            }),
            session,
        )?);
        assert!(
            is_cancel(
                Some(ClientFrame::Cancel {
                    version: PROTOCOL_VERSION,
                    request_id: uuid::Uuid::from_u128(602),
                }),
                session,
            )
            .is_err()
        );

        assert_eq!(
            timed(std::time::Duration::from_secs(1), async {
                Ok::<_, MmError>(7_u8)
            })
            .await?,
            7
        );
        assert!(
            timed(std::time::Duration::from_millis(1), async {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                Ok::<_, MmError>(())
            })
            .await
            .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn daemon_listener_and_idle_stage_timeouts_cover_live_session_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = crate::test_support::tempdir()?;
        let root = temporary.path().join("project");
        std::fs::create_dir(&root)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let handler: Arc<dyn RemoteUseCases> = Arc::new(FakeUseCases {
            cancelled: Arc::new(AtomicBool::new(false)),
        });

        let mut timeout_config = config(temporary.path(), &root, uid, gid)?;
        timeout_config.idle_timeout_ms = 1;
        let timeout_daemon = RemoteDaemon::new(timeout_config, Arc::clone(&handler))?;
        for stage in 0..3 {
            let (server, _peer) = tokio::io::duplex(64);
            let (mut reader, _unused_writer) = tokio::io::split(server);
            let reader = &mut FrameReader::new(&mut reader);
            let mut writer = tokio::io::sink();
            let result = match stage {
                0 => timeout_daemon
                    .authenticate_relay(reader, &mut writer)
                    .await
                    .map(|_| ()),
                1 => timeout_daemon
                    .handshake(reader, &mut writer)
                    .await
                    .map(|_| ()),
                _ => timeout_daemon
                    .receive_request(
                        reader,
                        &mut writer,
                        &super::Agreed {
                            protocol: PROTOCOL_VERSION,
                            offered: PROTOCOL_VERSION,
                            client_release: String::new(),
                        },
                    )
                    .await
                    .map(|_| ()),
            };
            assert!(result.is_err());
        }

        let daemon_config = config(temporary.path(), &root, uid, gid)?;
        let socket_path = daemon_config.socket_path.clone();
        let daemon = RemoteDaemon::new(daemon_config, handler)?;
        let task = tokio::spawn(daemon.run());
        for _ in 0..100 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        let client = UnixStream::connect(&socket_path).await?;
        drop(client);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        task.abort();
        assert!(task.await.is_err());
        Ok(())
    }

    /// Break-glass key the test sessions authenticate with: admin, exec,
    /// and forward on every enrolled project.
    const OPS_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 ops@desk";

    fn ops_fingerprint() -> String {
        crate::domain::host_policy::parse_public_key(OPS_KEY)
            .map(|key| key.fingerprint)
            .unwrap_or_default()
    }

    fn git(directory: &std::path::Path, arguments: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["-c", "user.name=test", "-c", "user.email=test@example.test"])
            .args(arguments)
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    /// A bare origin whose `main` holds a valid configuration for project 4.
    pub(super) fn origin(path: &std::path::Path) -> Result<(std::path::PathBuf, String), Box<dyn std::error::Error>> {
        let bare = path.join("origin.git");
        let commit_file = path.join("origin.commit");
        if bare.is_dir() {
            return Ok((bare, std::fs::read_to_string(commit_file)?));
        }
        let work = path.join("origin-work");
        std::fs::create_dir_all(work.join(".config"))?;
        git(&work, &["init", "--quiet", "--initial-branch=main"])?;
        std::fs::write(
            work.join(".config/mm.yaml"),
            format!(
                "schema-version: 1\nproject:\n  id: {}\n  name: example\ntarget: local\nstacks:\n  app:\n    files: [compose.yaml]\nenvironments:\n  production: {{}}\nmodules:\n  api:\n    path: .\n    services:\n      web: {{stack: app}}\ntargets:\n  cluster:\n    nodes:\n      - {{name: app-1, host: 203.0.113.11}}\n      - {{name: app-2, host: 203.0.113.12}}\n",
                uuid::Uuid::from_u128(4)
            ),
        )?;
        std::fs::write(work.join("compose.yaml"), "services:\n  web:\n    image: busybox\n")?;
        git(&work, &["add", "."])?;
        git(&work, &["commit", "--quiet", "-m", "initial"])?;
        let commit = git(&work, &["rev-parse", "HEAD"])?;
        git(path, &["clone", "--quiet", "--bare", "origin-work", "origin.git"])?;
        std::fs::write(commit_file, &commit)?;
        Ok((bare, commit))
    }

    pub(super) fn config(
        path: &std::path::Path,
        root: &std::path::Path,
        uid: u32,
        gid: u32,
    ) -> Result<DaemonConfig, Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;
        let (origin, _) = origin(path)?;
        let etc = path.join(format!("etc-{uid}"));
        std::fs::create_dir_all(&etc)?;
        std::fs::set_permissions(&etc, std::fs::Permissions::from_mode(0o700))?;
        let host_policy_path = etc.join("host.yaml");
        std::fs::write(
            &host_policy_path,
            format!(
                "schema-version: 1\nbreak-glass:\n  - name: ops\n    key: {OPS_KEY}\nprojects:\n  {}:\n    name: example\n    origin: {}\n    environments: [production]\n",
                uuid::Uuid::from_u128(4),
                origin.display()
            ),
        )?;
        std::fs::set_permissions(&host_policy_path, std::fs::Permissions::from_mode(0o600))?;
        let state_root = path.join(format!("state-{uid}"));
        let project = ProjectId(uuid::Uuid::from_u128(4));
        let deployed = state_root
            .join("projects")
            .join(project.0.to_string())
            .join("deployed");
        std::fs::create_dir_all(&deployed)?;
        std::fs::write(
            deployed.join("production.json"),
            serde_json::to_vec(&crate::adapters::remote::DeployedState {
                project_id: project,
                environment: "production".to_owned(),
                commit: "a".repeat(40),
                root: std::fs::canonicalize(root)?,
                deployed_at_unix_ms: 1,
                deployed_by: "test".to_owned(),
                runs_services: true,
            })?,
        )?;
        Ok(DaemonConfig {
            schema_version: 1,
            socket_path: path.join("daemon.sock"),
            state_root,
            mm_home: path.join(format!("home-{uid}")),
            socket_uid: uid,
            socket_gid: gid,
            idle_timeout_ms: 1_000,
            host_policy_path,
            relay_uid: uid,
            authorized_keys_path: path.join(format!("ssh-{uid}/authorized_keys")),
            relay_command: "/usr/bin/mm relay serve --stdio".to_owned(),
            projects_root: path.join(format!("projects-{uid}")),
            node_env_path: path.join(format!("node-{uid}.env")),
        })
    }

    async fn send_session(stream: &mut UnixStream) -> Result<(), Box<dyn std::error::Error>> {
        write_frame(
            stream,
            &ClientFrame::RelaySession {
                version: PROTOCOL_VERSION,
                key_fingerprint: ops_fingerprint(),
                source: "127.0.0.1 10000 127.0.0.1 22".to_owned(),
                session_id: uuid::Uuid::from_u128(89),
            },
        )
        .await?;
        Ok(())
    }

    pub(super) async fn connected_client(
        daemon: &Arc<RemoteDaemon>,
    ) -> Result<
        (tokio::task::JoinHandle<Result<(), MmError>>, UnixStream),
        Box<dyn std::error::Error>,
    > {
        let fingerprint = daemon.identity().fingerprint.clone();
        let (server, mut client) = unix_pair()?;
        let task = tokio::spawn({
            let daemon = Arc::clone(daemon);
            async move { daemon.serve_stream(server).await }
        });
        send_session(&mut client).await?;
        let client_nonce = uuid::Uuid::from_u128(95);
        write_frame(
            &mut client,
            &ClientFrame::Hello {
                version: PROTOCOL_VERSION,
                minimum: PROTOCOL_VERSION,
                hello: ClientHello {
                    client_nonce,
                    expected_daemon_fingerprint: fingerprint,
                    client_version: String::new(),
                },
            },
        )
        .await?;
        if !matches!(
            read_frame::<_, ServerFrame>(&mut client).await?,
            Some(ServerFrame::Hello { hello, .. }) if hello.client_nonce == client_nonce
        ) {
            return Err("daemon did not complete the test handshake".into());
        }
        Ok((task, client))
    }

    async fn serve_frames(
        daemon: &RemoteDaemon,
        frames: Vec<ClientFrame>,
    ) -> Result<(), MmError> {
        let (server, mut client) = unix_pair().map_err(|source| MmError::Io {
            context: "creating remote session test pair".to_owned(),
            source,
        })?;
        for frame in frames {
            write_frame(&mut client, &frame).await?;
        }
        daemon.serve_stream(server).await
    }

    pub(super) fn remote_scope(_root: std::path::PathBuf) -> RemoteScope {
        RemoteScope {
            project_id: ProjectId(uuid::Uuid::from_u128(4)),
            environment: "production".to_owned(),
            target: "production".to_owned(),
            target_fingerprint: "a".repeat(64),
            runtime: RuntimeKind::DockerCompose,
            node: None,
        }
    }

    fn deployment_plan(context: &RemoteRequestContext) -> Result<DeploymentPlan, MmError> {
        let target_id = TargetId(uuid::Uuid::from_u128(94));
        let dynamic = PreparedTraefikDynamicConfig {
            root: context.project_root.join("dynamic"),
            path: context.project_root.join("dynamic/dynamic.yaml"),
            manifest: TraefikDynamicConfigManifest {
                target_id,
                claim_count: 0,
                router_count: 0,
                service_count: 0,
                digest: "b".repeat(64),
            },
        };
        let mut plan = DeploymentPlan {
            excluded_services: Vec::new(),
            schema: DEPLOYMENT_PLAN_SCHEMA.to_owned(),
            project_id: context.scope.project_id,
            project_name: "example".to_owned(),
            target_name: context.scope.target.clone(),
            target_id,
            target_fingerprint: context.scope.target_fingerprint.clone(),
            environment: context.scope.environment.clone(),
            runtime: Some(context.scope.runtime),
            runtime_source: Some("test".to_owned()),
            workload_intent_hash: Some("d".repeat(64)),
            networks: Vec::new(),
            expected_route_generation: 0,
            expected_route_config_digest: None,
            expected_previous_plan_hash: None,
            stacks: Vec::new(),
            removed_stacks: Vec::new(),
            preserved_volumes: Vec::new(),
            removed_preserved_volumes: Vec::new(),
            previous_owner_claims: Vec::new(),
            adopted_route_claims: Vec::new(),
            desired_route_claims: Vec::new(),
            complete_route_claims: Vec::new(),
            desired_middlewares: std::collections::BTreeMap::new(),
            environment_review: Default::default(),
            runtime_secret_plan: None,
            scheduler_reconciliation: None,
            environment_bundle_root: None,
            previous_dynamic_config: dynamic.clone(),
            dynamic_config: dynamic,
            local_certificates: Vec::new(),
            static_ingress: None,
            removed_ingress: None,
            route_diff: DeploymentRouteDiff::default(),
            ingress: Vec::new(),
            route_state_changed: false,
            build_receipt: None,
            local_build: None,
        registries: Default::default(),
            node: None,
            rollout: Default::default(),
            config_files: Vec::new(),
            runtime_changes: Vec::new(),
            unknown_config_keys: Vec::new(),
            secret_file_owners: Vec::new(),
            release: None,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Security { message })?;
        Ok(plan)
    }

    fn unix_pair() -> Result<(UnixStream, UnixStream), std::io::Error> {
        let (left, right) = std::os::unix::net::UnixStream::pair()?;
        left.set_nonblocking(true)?;
        right.set_nonblocking(true)?;
        Ok((UnixStream::from_std(left)?, UnixStream::from_std(right)?))
    }

    fn log_request() -> LogRequest {
        LogRequest {
            selector: Some(ServiceSelector {
                stack: Some("app".to_owned()),
                module: Some("api".to_owned()),
                service: "web".to_owned(),
                container: None,
            }),
            nodes: Vec::new(),
            tail: 1,
            since_unix_ms: None,
            until_unix_ms: None,
            follow: true,
            timestamps: false,
            output: LogOutputPolicy::Both,
            mode: LogMode::Redacted,
            limits: LogLimits::default(),
            approval: ApprovalContext {
                principal: "forged".to_owned(),
                source: "forged".to_owned(),
                approved_high_risk: false,
            },
        }
    }

    fn exec_request() -> ExecRequest {
        ExecRequest {
            selector: ServiceSelector {
                stack: Some("app".to_owned()),
                module: Some("api".to_owned()),
                service: "web".to_owned(),
                container: None,
            },
            argv: vec!["true".to_owned()],
            user: None,
            workdir: None,
            environment: Vec::new(),
            tty: ExecTtyMode::Disabled,
            stdin: ExecStdinMode::Closed,
            timeout: std::time::Duration::from_secs(1),
            approval: ApprovalContext {
                principal: "forged".to_owned(),
                source: "forged".to_owned(),
                approved_high_risk: false,
            },
            shell_wrapper: false,
        }
    }

    fn workspace_snapshot(context: &RemoteRequestContext) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            workspace: crate::domain::workspace::WorkspaceRecord {
                id: crate::domain::workspace::WorkspaceId(uuid::Uuid::from_u128(720)),
                project_id: context.scope.project_id,
                root: context.project_root.clone(),
                config_path: context.project_root.join(".config/mm.yaml"),
            },
            project_name: "remote-test".to_owned(),
            target: context.scope.target.clone(),
            environment: context.scope.environment.clone(),
            config_hash: "a".repeat(64),
            target_source: Some("test".to_owned()),
            runtime_kind: Some(context.scope.runtime),
            runtime_source: Some("test".to_owned()),
            runtime: crate::domain::runtime::RuntimeProbe {
                kind: context.scope.runtime,
                available: true,
                engine_version: Some("test".to_owned()),
                compose_provider: None,
                rootless: Some(true),
                socket_path: None,
                capabilities: Vec::new(),
                diagnostics: Vec::new(),
            },
            validation: crate::domain::diagnostic::ValidationReport {
                diagnostics: Vec::new(),
            },
            stacks: Vec::new(),
            ingress: Vec::new(),
            recent_operations: Vec::new(),
            nodes: Vec::new(),
            names: None,
            deployment: None,
            drift: Vec::new(),
            network_drift: Vec::new(),
            optional: Vec::new(),
            tunnel: None,
        }
    }

    fn unused() -> MmError {
        MmError::State {
            message: "unused fake use case".to_owned(),
        }
    }
