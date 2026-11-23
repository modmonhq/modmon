//! Staging secrets and variables on a remote environment's hosts:
//! every node takes a change or none keeps it, a staged value is what the
//! next deployment reads while running services keep theirs, it goes live
//! only at the revision a plan saw, and no value shows in what a node
//! reports.

mod temp_dirs;

use std::sync::Arc;

use async_trait::async_trait;
use mm::adapters::secrets::{
    AgeSecretStore, PreparedStage, StagedSlot, StoreChange, StoreStagedChange,
};
use mm::app::ports::{ExecStream, LogStream, RemoteTransport, SecretMetadataInspector};
use mm::app::{KeyEdit, RemoteEnvironment, RemoteNode, ValueEdit};
use mm::composition::AgeValueSealer;
use mm::domain::build::BuildReceipt;
use mm::domain::deployment::DeploymentResult;
use mm::domain::error::MmError;
use mm::domain::observability::{ExecRequest, LogRequest, OperationCancellation};
use mm::domain::remote::{
    ClusterPlanSeal, NodeFacts, RemoteAccessPlan, RemoteDeploymentPlan, RemoteIdentity, RemoteScope,
};
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret::{SecretReference, SecretScope, SecretStatus};
use mm::domain::staged::{
    HostValue, NodeValues, StageAnswer, StageState, StageStep, StagedAction, StagedPayload,
    StagedPayloadValue, ValueChange, ValueKind, ValueStage,
};
use mm::domain::users::AccessChange;
use mm::domain::workspace::{ProjectId, WorkspaceSnapshot};
use zeroize::Zeroizing;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const VALUE: &str = "correct-horse-battery-staple";

fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(11))
}

fn reference() -> TestResult<SecretReference> {
    Ok("age://web/app-key".parse()?)
}

fn slot() -> TestResult<StagedSlot> {
    Ok(StagedSlot::Secret {
        environment: "uat".to_owned(),
        reference: reference()?,
    })
}

fn store(directory: &std::path::Path, name: &str) -> TestResult<AgeSecretStore> {
    let store = AgeSecretStore::new(
        directory.join(format!("{name}.key")),
        directory.join(format!("{name}.age")),
    );
    store.ensure_recipient()?;
    Ok(store)
}

/// A node that keeps its values in a real host store, as `mmd` does.
struct Node {
    store: AgeSecretStore,
    /// The node refuses the commit step.
    refuses_commit: bool,
    unreachable: bool,
}

impl Node {
    fn answer(&self) -> Result<(), MmError> {
        if self.unreachable {
            return Err(MmError::Unreachable {
                subject: "app-2".to_owned(),
                problem: "did not answer".to_owned(),
                help: "check the node".to_owned(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl RemoteTransport for Node {
    async fn secret_recipient(&self, _scope: RemoteScope) -> Result<String, MmError> {
        self.answer()?;
        self.store.ensure_recipient()
    }

    async fn stage_values(
        &self,
        scope: RemoteScope,
        stage: ValueStage,
    ) -> Result<StageAnswer, MmError> {
        self.answer()?;
        let answer = |state| StageAnswer {
            stage: stage.stage,
            state,
            changes: Vec::new(),
        };
        match stage.step {
            StageStep::Prepare { changes, sealed } => {
                if let Some(sealed) = &sealed {
                    self.store
                        .open_staged_values(sealed.as_bytes())?
                        .check_matches(project(), &scope.environment, stage.stage, &changes)
                        .map_err(|message| MmError::Secret { message })?;
                }
                self.store.prepare_stage(
                    stage.stage,
                    PreparedStage {
                        environment: scope.environment,
                        principal: "alice".to_owned(),
                        changes,
                        sealed,
                        expires_at_unix_ms: i64::MAX,
                    },
                    1,
                )?;
                Ok(answer(StageState::Prepared))
            }
            StageStep::Commit if self.refuses_commit => Err(MmError::Security {
                message: "no grant matches this capability and exact scope".to_owned(),
            }),
            StageStep::Commit => {
                let prepared =
                    self.store
                        .take_prepared(stage.stage, 2)?
                        .ok_or_else(|| MmError::State {
                            message: "nothing prepared".to_owned(),
                        })?;
                let mut values = match &prepared.sealed {
                    Some(sealed) => self.store.open_staged_values(sealed.as_bytes())?.values,
                    None => Vec::new(),
                };
                let changes = prepared
                    .changes
                    .iter()
                    .map(|change| {
                        let change = match &change.action {
                            StagedAction::Set { revision } => StoreChange::Set {
                                revision: revision.clone(),
                                value: values.remove(0).value,
                            },
                            StagedAction::Unset { revision } => StoreChange::Unset {
                                revision: revision.clone(),
                            },
                            StagedAction::Discard => StoreChange::Discard,
                        };
                        Ok(StoreStagedChange {
                            slot: slot().map_err(|error| MmError::State {
                                message: error.to_string(),
                            })?,
                            change,
                        })
                    })
                    .collect::<Result<Vec<_>, MmError>>()?;
                self.store.stage_values(stage.stage, changes, "alice", 3)?;
                Ok(answer(StageState::Committed))
            }
            StageStep::Revert => {
                self.store.revert_stage(stage.stage)?;
                Ok(answer(StageState::Reverted))
            }
        }
    }

    async fn host_values(&self, scope: RemoteScope) -> Result<NodeValues, MmError> {
        self.answer()?;
        let slot = slot().map_err(|error| MmError::State {
            message: error.to_string(),
        })?;
        let report = self.store.slot_reports(&[slot])?.remove(0);
        Ok(NodeValues {
            node: scope.node,
            commit: "c".repeat(40),
            values: vec![HostValue {
                key: "web.app-key".to_owned(),
                kind: ValueKind::Secret,
                origin: report.origin,
                provider: None,
                services: vec!["web".to_owned()],
                required: true,
                active: report.active,
                staged: report.staged,
            }],
        })
    }

    async fn status(&self, _scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        unused()
    }

    async fn logs(
        &self,
        _scope: RemoteScope,
        _request: LogRequest,
        _cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        unused()
    }

    async fn exec(
        &self,
        _scope: RemoteScope,
        _request: ExecRequest,
        _cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        unused()
    }

    async fn plan_deployment(
        &self,
        _scope: RemoteScope,
        _commit: String,
        _receipt: Option<BuildReceipt>,
        _release: mm::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError> {
        unused()
    }

    async fn apply_deployment(
        &self,
        _scope: RemoteScope,
        _remote_plan_hash: String,
    ) -> Result<Option<DeploymentResult>, MmError> {
        unused()
    }

    async fn seal_plan(
        &self,
        _scope: RemoteScope,
        _seal: ClusterPlanSeal,
    ) -> Result<ClusterPlanSeal, MmError> {
        unused()
    }

    async fn sealed_plan(
        &self,
        _scope: RemoteScope,
        _cluster_plan_hash: String,
    ) -> Result<ClusterPlanSeal, MmError> {
        unused()
    }

    async fn node(&self, _scope: RemoteScope) -> Result<NodeFacts, MmError> {
        unused()
    }

    async fn plan_team_sync(
        &self,
        _scope: RemoteScope,
        _commit: String,
    ) -> Result<RemoteAccessPlan, MmError> {
        unused()
    }

    async fn apply_team_sync(
        &self,
        _scope: RemoteScope,
        _remote_plan_hash: String,
    ) -> Result<AccessChange, MmError> {
        unused()
    }

    async fn whoami(&self, _scope: RemoteScope) -> Result<RemoteIdentity, MmError> {
        unused()
    }

    async fn forward(
        &self,
        _scope: RemoteScope,
        _service: String,
        _port: u16,
        _local: tokio::net::TcpStream,
    ) -> Result<mm::domain::forward::ForwardTotals, MmError> {
        unused()
    }
}

fn unused<T>() -> Result<T, MmError> {
    Err(MmError::State {
        message: "not used by this test".to_owned(),
    })
}

/// Three nodes, each with its own store in `directory`.
fn cluster(
    directory: &std::path::Path,
    refusing: Option<&str>,
    unreachable: Option<&str>,
) -> TestResult<(RemoteEnvironment, Vec<Arc<Node>>)> {
    let mut nodes = Vec::new();
    let mut remote = Vec::new();
    for (index, name) in ["app-1", "app-2", "app-3"].into_iter().enumerate() {
        let node = Arc::new(Node {
            store: store(directory, name)?,
            refuses_commit: refusing == Some(name),
            unreachable: unreachable == Some(name),
        });
        remote.push(RemoteNode {
            name: Some(name.to_owned()),
            host: format!("203.0.113.{}", index + 1),
            port: 22,
            repo_address: None,
            private_address: None,
            transport: Ok(Arc::clone(&node) as Arc<dyn RemoteTransport>),
            scope: RemoteScope {
                project_id: project(),
                environment: "uat".to_owned(),
                target: "uat".to_owned(),
                target_fingerprint: "a".repeat(64),
                runtime: RuntimeKind::DockerCompose,
                node: Some(name.to_owned()),
            },
            placed: None,
        });
        nodes.push(node);
    }
    Ok((RemoteEnvironment::new("uat".to_owned(), remote)?, nodes))
}

fn set(value: &str) -> Vec<KeyEdit> {
    vec![KeyEdit {
        kind: ValueKind::Secret,
        key: "web.app-key".to_owned(),
        edit: ValueEdit::Set(Zeroizing::new(value.to_owned())),
    }]
}

fn staged_revision(node: &Node) -> TestResult<Option<String>> {
    Ok(node
        .store
        .slot_reports(&[slot()?])?
        .remove(0)
        .staged
        .map(|staged| staged.revision))
}

#[tokio::test]
async fn every_node_stages_the_same_revision_and_status_never_shows_the_value() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let (environment, nodes) = cluster(directory.path(), None, None)?;
    let report = environment
        .stage_values(set(VALUE), &AgeValueSealer)
        .await?;
    assert_eq!(report.nodes, ["app-1", "app-2", "app-3"]);
    let revision = report.changes[0]
        .action
        .revision()
        .ok_or("a set has a revision")?
        .to_owned();
    for node in &nodes {
        assert_eq!(staged_revision(node)?.as_deref(), Some(revision.as_str()));
    }
    let values = environment.host_values().await?;
    assert_eq!(values.nodes.len(), 3);
    assert!(!values.disagreeing(ValueKind::Secret, "web.app-key"));
    let shown = serde_json::to_string(&values)?;
    assert!(!shown.contains(VALUE), "status carries no value");
    assert!(!format!("{report:?}").contains(VALUE));
    assert!(!serde_json::to_string(&report)?.contains(VALUE));
    Ok(())
}

#[tokio::test]
async fn a_node_that_refuses_the_commit_leaves_no_node_with_the_stage() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let (environment, nodes) = cluster(directory.path(), Some("app-3"), None)?;
    let error = environment
        .stage_values(set(VALUE), &AgeValueSealer)
        .await
        .err()
        .ok_or("the stage should fail")?;
    let message = error.to_string();
    assert!(message.contains("app-3"), "{message}");
    assert!(message.contains("No node keeps it"), "{message}");
    assert!(!message.contains(VALUE));
    for node in &nodes {
        assert_eq!(
            staged_revision(node)?,
            None,
            "no node keeps a partial stage"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_unreachable_node_stops_the_stage_before_any_node_prepares() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let (environment, nodes) = cluster(directory.path(), None, Some("app-2"))?;
    let error = environment
        .stage_values(set(VALUE), &AgeValueSealer)
        .await
        .err()
        .ok_or("the stage should fail")?;
    assert!(error.to_string().contains("app-2"), "{error}");
    for node in &nodes {
        assert_eq!(staged_revision(node)?, None);
    }
    Ok(())
}

#[tokio::test]
async fn a_later_stage_replaces_an_earlier_one_and_discard_drops_it() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let (environment, nodes) = cluster(directory.path(), None, None)?;
    environment
        .stage_values(set("first"), &AgeValueSealer)
        .await?;
    let second = environment
        .stage_values(set("second"), &AgeValueSealer)
        .await?;
    let revision = second.changes[0].action.revision().map(str::to_owned);
    assert_eq!(staged_revision(&nodes[0])?, revision);
    environment
        .stage_values(
            vec![KeyEdit {
                kind: ValueKind::Secret,
                key: "web.app-key".to_owned(),
                edit: ValueEdit::Discard,
            }],
            &AgeValueSealer,
        )
        .await?;
    for node in &nodes {
        assert_eq!(staged_revision(node)?, None);
    }
    Ok(())
}

#[tokio::test]
async fn the_next_deployment_reads_the_staged_value_and_it_goes_live_at_the_planned_revision()
-> TestResult {
    let directory = temp_dirs::tempdir()?;
    let host = store(directory.path(), "host")?;
    let scope = SecretScope::new("uat");
    let stage = |value: &str, revision: &str, stage| -> TestResult {
        host.stage_values(
            stage,
            vec![StoreStagedChange {
                slot: slot()?,
                change: StoreChange::Set {
                    revision: revision.to_owned(),
                    value: Zeroizing::new(value.to_owned()),
                },
            }],
            "alice",
            10,
        )?;
        Ok(())
    };
    stage("live-one", "r1", uuid::Uuid::new_v4())?;
    assert_eq!(host.go_live(&[(slot()?, "r1".to_owned())], 11)?, 1);
    stage("staged-two", "r2", uuid::Uuid::new_v4())?;
    let metadata = host.inspect(&scope, &reference()?).await?;
    assert_eq!(
        metadata.revision.as_deref(),
        Some("r2"),
        "a plan reads the staged revision"
    );
    let report = host.slot_reports(&[slot()?])?.remove(0);
    assert_eq!(
        report
            .active
            .as_ref()
            .map(|active| active.revision.as_str()),
        Some("r1")
    );
    assert_eq!(
        report
            .active
            .and_then(|active| active.changed_by)
            .as_deref(),
        Some("alice")
    );
    assert_eq!(
        host.go_live(&[(slot()?, "r1".to_owned())], 12)?,
        0,
        "another revision stays staged"
    );
    assert_eq!(host.go_live(&[(slot()?, "r2".to_owned())], 12)?, 1);
    let report = host.slot_reports(&[slot()?])?.remove(0);
    assert_eq!(report.staged, None);
    assert_eq!(
        report.active.map(|active| active.revision).as_deref(),
        Some("r2")
    );
    Ok(())
}

#[tokio::test]
async fn a_staged_removal_reads_as_unset_and_a_reverted_stage_restores_what_was_staged()
-> TestResult {
    let directory = temp_dirs::tempdir()?;
    let host = store(directory.path(), "host")?;
    let scope = SecretScope::new("uat");
    let first = uuid::Uuid::new_v4();
    host.stage_values(
        first,
        vec![StoreStagedChange {
            slot: slot()?,
            change: StoreChange::Set {
                revision: "r1".to_owned(),
                value: Zeroizing::new(VALUE.to_owned()),
            },
        }],
        "alice",
        1,
    )?;
    let removal = uuid::Uuid::new_v4();
    host.stage_values(
        removal,
        vec![StoreStagedChange {
            slot: slot()?,
            change: StoreChange::Unset {
                revision: "r2".to_owned(),
            },
        }],
        "bob",
        2,
    )?;
    let metadata = host.inspect(&scope, &reference()?).await?;
    assert_eq!(metadata.status, SecretStatus::Unset);
    assert!(host.revert_stage(removal)?);
    assert!(
        !host.revert_stage(first)?,
        "only the last stage can be undone"
    );
    let metadata = host.inspect(&scope, &reference()?).await?;
    assert_eq!(metadata.revision.as_deref(), Some("r1"));
    Ok(())
}

#[test]
fn sealed_values_open_only_for_their_node_and_match_their_changes() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let mine = store(directory.path(), "mine")?;
    let other = store(directory.path(), "other")?;
    let stage = uuid::Uuid::new_v4();
    let payload = StagedPayload {
        project_id: project(),
        environment: "uat".to_owned(),
        stage,
        values: vec![StagedPayloadValue {
            key: "web.app-key".to_owned(),
            kind: ValueKind::Secret,
            revision: "r1".to_owned(),
            value: Zeroizing::new(VALUE.to_owned()),
        }],
    };
    assert!(!format!("{payload:?}").contains(VALUE));
    let sealed = mm::adapters::secrets::seal_staged_values(&mine.ensure_recipient()?, &payload)?;
    assert!(!sealed.contains(VALUE));
    let error = other
        .open_staged_values(sealed.as_bytes())
        .err()
        .ok_or("another node's store opened it")?;
    assert!(!error.to_string().contains(VALUE));
    let opened = mine.open_staged_values(sealed.as_bytes())?;
    let change = |revision: &str| ValueChange {
        key: "web.app-key".to_owned(),
        kind: ValueKind::Secret,
        action: StagedAction::Set {
            revision: revision.to_owned(),
        },
    };
    opened.check_matches(project(), "uat", stage, &[change("r1")])?;
    assert!(
        opened
            .check_matches(project(), "uat", stage, &[change("r9")])
            .is_err()
    );
    assert!(
        opened
            .check_matches(project(), "prod", stage, &[change("r1")])
            .is_err()
    );
    Ok(())
}

#[test]
fn a_stage_without_its_sealed_values_or_with_a_bad_key_is_refused() {
    let stage = |key: &str, sealed: Option<&str>| ValueStage {
        stage: uuid::Uuid::new_v4(),
        step: StageStep::Prepare {
            changes: vec![ValueChange {
                key: key.to_owned(),
                kind: ValueKind::Secret,
                action: StagedAction::Set {
                    revision: "r1".to_owned(),
                },
            }],
            sealed: sealed.map(str::to_owned),
        },
    };
    assert!(stage("web.app-key", Some("x")).validate().is_ok());
    assert!(stage("web.app-key", None).validate().is_err());
    assert!(stage("web/../app", Some("x")).validate().is_err());
    assert!(stage("", Some("x")).validate().is_err());
}

fn host_context(
    scope: RemoteScope,
    project_root: &std::path::Path,
) -> mm::app::ports::RemoteRequestContext {
    use mm::domain::access::{
        AccessAction, AccessResourceKind, Authorization, Capability, PrincipalId,
    };
    let principal_id = PrincipalId(uuid::Uuid::from_u128(31));
    mm::app::ports::RemoteRequestContext {
        request_id: uuid::Uuid::from_u128(32),
        operation_nonce: uuid::Uuid::from_u128(33),
        principal_id,
        principal: "alice".to_owned(),
        key_fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
        session_id: uuid::Uuid::from_u128(34),
        source: "test".to_owned(),
        request_plan_hash: None,
        authorization: Authorization {
            principal_id,
            display_name: "alice".to_owned(),
            capability: Capability::new(AccessAction::Status, AccessResourceKind::Project),
            policy_revision: 1,
            grant_ids: std::collections::BTreeSet::new(),
            authorization_hash: "a".repeat(64),
        },
        deadline_unix_ms: i64::MAX,
        scope,
        project_root: project_root.to_path_buf(),
        release: None,
        protocol: mm::domain::remote::PROTOCOL_VERSION,
    }
}

#[tokio::test]
async fn a_host_holds_no_file_secret_and_refuses_to_stage_one() -> TestResult {
    use mm::app::ports::RemoteUseCases;
    let directory = temp_dirs::tempdir()?;
    let checkout = directory.path().join("checkouts/1");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    std::fs::create_dir_all(checkout.join(".config"))?;
    std::fs::create_dir_all(checkout.join("api"))?;
    for file in [".config/mm.yaml", "compose.yaml", "api/.env.example"] {
        std::fs::copy(fixture.join(file), checkout.join(file))?;
    }
    let config = checkout.join(".config/mm.yaml");
    std::fs::write(
        &config,
        std::fs::read_to_string(&config)?.replace(
            "environments:\n",
            "  registry.auth:\n    storage:\n      provider: file\n      location: ~/.config/tool/auth.json\nenvironments:\n",
        ),
    )?;
    let document = mm::adapters::config::ConfigLoader.load_from(&checkout)?;
    let scope = RemoteScope {
        project_id: document.config.project.id,
        environment: "local".to_owned(),
        target: "uat".to_owned(),
        target_fingerprint: "b".repeat(64),
        runtime: RuntimeKind::DockerCompose,
        node: None,
    };
    let host = mm::composition::DaemonUseCaseAdapter::new(directory.path().join("mm-home"));
    let context = host_context(scope, &checkout);
    host.secret_recipient(&context).await?;
    let held = host.host_values(&context).await?;
    let auth = held
        .values
        .iter()
        .find(|value| value.key == "registry.auth")
        .ok_or("the host does not list the file secret")?;
    assert_eq!(auth.provider.as_deref(), Some("file"));
    assert!(auth.active.is_none() && auth.staged.is_none());

    let refused = host
        .stage_values(
            &context,
            ValueStage {
                stage: uuid::Uuid::new_v4(),
                step: StageStep::Prepare {
                    changes: vec![ValueChange {
                        key: "registry.auth".to_owned(),
                        kind: ValueKind::Secret,
                        action: StagedAction::Set {
                            revision: "r1".to_owned(),
                        },
                    }],
                    sealed: Some("sealed".to_owned()),
                },
            },
        )
        .await
        .err()
        .ok_or("a host staged a file secret")?
        .to_string();
    assert!(refused.contains("hosts never read it"), "{refused}");
    assert!(refused.contains("provider: age"), "{refused}");
    assert!(
        refused.contains("mm -e local secret set NAME --stdin"),
        "{refused}"
    );
    Ok(())
}
